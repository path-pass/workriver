// A hosted agent's life as the service sees it: the user's start (reserved, spawned, waited
// for, recorded, published), the user's close (asked of the host, recorded from its journal's
// one `closed` line), and reconciliation. The service never signals a host or its agent.
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use workriver_core::{self as core, Body, Principal};

use crate::runtime::{HostError, HostHello, Published};
use crate::server::app::*;
use crate::server::http::*;

/// How long a start may take from its reservation before it is asked to close.
pub(in crate::server) const START_WAIT: Duration = Duration::from_secs(60);
/// How long after a close is asked for its outcome is waited for before it is unconfirmed:
/// the host's own deadline and some.
pub(in crate::server) const CLOSE_WAIT: Duration = Duration::from_secs(40);

/// What the service keeps of its hosts while it runs. The records and journals hold what lasts.
#[derive(Default)]
pub(in crate::server) struct Hosting {
    busy: HashSet<String>,
    /// The first reconciliation's outstanding instances. None until the records are read.
    initial: Option<HashSet<String>>,
    /// The connection whose approval receipts were reconciled before publishing each agent.
    attached: HashMap<String, u64>,
    /// A direct approval receipt whose write failed, even if its connection stayed open.
    answers_pending: HashSet<String>,
    /// Failed starts still waiting for their host to go, and when a close was last accepted.
    ending: HashMap<String, (String, Option<Instant>)>,
    pub(super) incompatible: HashSet<String>,
    pub(super) stopped: HashSet<String>,
    pub(super) closing: HashMap<String, Instant>,
}

/// One instance's turn at its lifecycle: released when dropped.
struct Turn(Shared, String);

impl Drop for Turn {
    fn drop(&mut self) {
        self.0.hosting.lock().unwrap().busy.remove(&self.1);
    }
}

impl App {
    fn turn(self: &Shared, id: &str) -> Option<Turn> {
        self.hosting.lock().unwrap().busy.insert(id.into()).then(|| Turn(self.clone(), id.into()))
    }

    /// Whether the first pass has examined every instance. Leftover actions wait for it.
    pub(super) fn hosted_reconciled(&self) -> bool {
        self.hosting.lock().unwrap().initial.as_ref().is_some_and(HashSet::is_empty)
    }

    /// Records and claims a start before the reconciler can take it. The receipt is already
    /// committed by the time the first read of the reserved instance is made in the background.
    pub(super) fn reserve_hosted(self: &Shared, who: &Principal, body: Body) -> Result<String, ApiError> {
        let mut hosting = self.hosting.lock().unwrap();
        let e = self.record(who, body)?;
        let Body::AgentStartReserved { id, .. } = e.body else { unreachable!() };
        hosting.busy.insert(id.clone());
        drop(hosting);
        let turn = Turn(self.clone(), id.clone());
        let deadline = Instant::now() + self.waits.hosted_start;
        if let Err(e) = std::thread::Builder::new().spawn(move || turn.0.start_hosted(&turn.1, deadline)) {
            self.end_start(&id, &format!("the background start could not run: {e}"));
        }
        Ok(id)
    }

    /// The reserved start's background work. Only a running hello with a session can be
    /// recorded, and its root comes from the service's own spawned child.
    fn start_hosted(&self, id: &str, deadline: Instant) {
        let read = got(self.store.lock().unwrap().hosted_agent(id));
        let Ok(Some(Some(a))) = read else {
            return self.end_start(id, "the reservation could not be read");
        };
        let (pid, start) = match self.hosts.spawn(id, &a.harness, &a.cwd, &a.model, &a.auth) {
            Ok(root) => root,
            Err(e) => return self.end_start(id, &format!("the host did not start: {e:#}")),
        };
        let mut why = "did not start in time".to_string();
        let mut generation = 1;
        while let Some(left) = deadline.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()) {
            self.hosts.reap();
            if !self.hosts.alive(id) {
                why = "the host stopped before the agent started; see its host.log".into();
                break;
            }
            match self.hosts.hello_for(id, left.min(Duration::from_secs(5))) {
                Ok((epoch, h)) => {
                    generation = h.generation;
                    if matches!(h.state.as_str(), "closing" | "closed") {
                        why = "the host closed before the agent started".into();
                        break;
                    }
                    if h.state == "running" && let Some(session) = h.session.as_deref() {
                        self.hosts.pending(id, session);
                        let body = started(id, session, &h, core::Shell { pid, start });
                        match self.record(&Principal::Service, body) {
                            Ok(_) => return self.publish(id, epoch),
                            Err(e) => {
                                why = format!("its start could not be recorded: {}", e.1);
                                break;
                            }
                        }
                    }
                }
                Err(HostError::Refused(w)) if w == "incompatible" => {
                    why = "the host speaks another protocol".into();
                    break;
                }
                Err(_) => {}
            }
            std::thread::sleep(deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(200)));
        }
        self.end_failed_start(id, &why, Some((true, generation)).filter(|_| self.hosts.alive(id)));
    }

    /// Asks a live host of a failed start to close. Its name stays reserved until it is gone;
    /// the next round checks again without keeping this round waiting for the host to exit.
    /// The boolean identifies a host already tracked from our spawned child.
    fn end_failed_start(&self, id: &str, why: &str, live: Option<(bool, u64)>) {
        let (why, asked) = self.hosting.lock().unwrap().ending.entry(id.into())
            .or_insert_with(|| (why.into(), None)).clone();
        let Some((tracked, generation)) = live else { return self.end_start(id, &why) };
        if asked.is_some_and(|at| at.elapsed() < self.waits.hosted_close) {
            return;
        }
        let close = if tracked { self.hosts.close(id, generation) } else { self.hosts.orphan_close(id, generation) };
        if close.is_ok() && let Some(entry) = self.hosting.lock().unwrap().ending.get_mut(id) {
            entry.1 = Some(Instant::now());
        }
    }

    fn end_start(&self, id: &str, why: &str) {
        self.hosts.forget(id);
        match self.record(&Principal::Service, Body::AgentStartEnded { agent: id.into(), why: why.into() }) {
            Ok(_) => { self.hosting.lock().unwrap().ending.remove(id); }
            Err(e) => tracing::warn!("cannot record that the start of {id} ended ({why}): {}", e.1),
        }
    }

    /// The instance is an agent's place only after reading its committed start. A failed read
    /// leaves it unpublished for the next reconciliation, with its original trusted root.
    fn publish(&self, id: &str, epoch: u64) {
        let read = got(self.store.lock().unwrap().hosted_agent(id));
        if let Ok(Some(Some(a))) = read && let Some(s) = &a.started {
            self.hosts.track(id, s.host.pid, s.host.start);
            self.hosts.publish(id, Published { session: s.session.clone(), name: a.name, harness: a.harness, cwd: a.cwd, generation: s.generation });
            self.hosting.lock().unwrap().attached.insert(id.into(), epoch);
            let _ = self.changed.send(());
        }
    }

    /// Open instances and directories left without a start, each under its own lifecycle
    /// turn. A slow host cannot delay another host's reconciliation or the next round.
    pub(in crate::server) fn reconcile_hosted(self: &Shared) {
        self.hosts.reap();
        let read = got(self.store.lock().unwrap().recorded());
        let Ok(Some(all)) = read else { return };
        let mut ids: HashSet<String> = all.hosted.iter().filter(|a| a.open()).map(|a| a.id.clone()).collect();
        ids.extend(self.hosts.on_disk().into_iter().filter(|id| all.hosted_agent(id).is_err() || all.hosted_agent(id).is_ok_and(|a| a.ended.is_some())));
        {
            let mut h = self.hosting.lock().unwrap();
            let initial = h.initial.get_or_insert_with(|| ids.clone());
            ids.extend(initial.iter().cloned());
        }
        for id in ids {
            let Some(turn) = self.turn(&id) else { continue };
            if let Err(e) = std::thread::Builder::new().spawn(move || turn.0.reconcile_one(&turn.1)) {
                tracing::warn!("cannot reconcile {id} this round: {e}");
            }
        }
    }

    fn reconcile_one(&self, id: &str) {
        let read = got(self.store.lock().unwrap().hosted_agent(id));
        let Ok(Some(agent)) = read else { return };
        match agent {
            Some(a) if a.open() => match &a.started {
                Some(s) => self.reconcile_started(&a, s),
                None => self.reconcile_reserved(&a),
            },
            None | Some(core::HostedAgent { ended: Some(_), .. }) => {
                // A lost spawn may write its instance record after we ended the reservation.
                // It is only ever contacted for cleanup, never used as the identity of a place.
                if let Some((host, generation)) = self.hosts.instance_host(id) {
                    tracing::debug!("an unpublished host runs for {id} (process {}): it is asked to close", host.pid);
                    let _ = self.hosts.orphan_close(id, generation);
                }
            }
            _ => {}
        }
        if let Some(initial) = self.hosting.lock().unwrap().initial.as_mut() {
            initial.remove(id);
        }
    }

    /// A lost reservation may wait for a young host still starting. A ready host is also
    /// closed, since a restarted service cannot attest to a root it did not spawn.
    fn reconcile_reserved(&self, a: &core::HostedAgent) {
        const LOST: &str = "the service stopped before the start finished";
        let tracked = self.hosts.alive(&a.id);
        let live = if tracked { Some((true, 1)) } else { self.hosts.instance_host(&a.id).map(|(_, generation)| (false, generation)) };
        let ending = self.hosting.lock().unwrap().ending.contains_key(&a.id);
        if ending || live.is_none() {
            return self.end_failed_start(&a.id, LOST, live);
        }
        let age = humantime::parse_rfc3339_weak(&a.at).ok().and_then(|t| t.elapsed().ok()).unwrap_or_default();
        let left = self.waits.hosted_start.saturating_sub(age);
        if left.is_zero() {
            return self.end_failed_start(&a.id, "did not start in time", live);
        }
        let wait = left.min(Duration::from_secs(5));
        let hello = if tracked {
            self.hosts.hello_for(&a.id, wait).map(|(_, hello)| hello)
        } else {
            self.hosts.orphan_hello(&a.id, wait)
        };
        if hello.is_ok_and(|h| matches!(h.state.as_str(), "running" | "closing" | "closed")) {
            self.end_failed_start(&a.id, LOST, live);
        }
    }

    /// A started agent's receipts are recovered before it advances to attached, stopped, or
    /// closed. The attached epoch identifies the exact connection, even if another request
    /// has already opened a replacement before this round.
    fn reconcile_started(&self, a: &core::HostedAgent, s: &core::hosted::Started) {
        self.hosts.track(&a.id, s.host.pid, s.host.start);
        let alive = self.hosts.alive(&a.id);
        let (closing, stopped, attached, incompatible, pending) = {
            let h = self.hosting.lock().unwrap();
            (h.closing.contains_key(&a.id), h.stopped.contains(&a.id), h.attached.get(&a.id).copied(),
                h.incompatible.contains(&a.id), h.answers_pending.contains(&a.id))
        };
        if stopped || alive && incompatible {
            return;
        }
        if alive && !closing && attached.is_some() && self.hosts.connection_epoch(&a.id) == attached && !pending {
            return;
        }
        // A successful handshake comes before reading the journal: the old connection may
        // have lost the acknowledgement of an answer it finished just before the handshake.
        let hello = (alive && !closing).then(|| self.hosts.hello(&a.id));
        // Clear only the pending write that prompted this scan. A direct answer failing during
        // the scan leaves a new pending flag for the next round.
        self.hosting.lock().unwrap().answers_pending.remove(&a.id);
        let receipts = match self.hosts.journal_receipts(&a.id) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("cannot read the journal of {}: {e:#}", a.id);
                self.hosting.lock().unwrap().answers_pending.insert(a.id.clone());
                return;
            }
        };
        if !self.record_answers(&a.id, receipts.decided) {
            self.hosting.lock().unwrap().answers_pending.insert(a.id.clone());
            return;
        }
        if let Some(closed) = receipts.closed {
            let body = Body::AgentClosed {
                agent: a.id.clone(), outcome: closed.outcome, pids: closed.pids,
                processes: closed.processes, marked_error: closed.marked_error,
            };
            match self.record(&Principal::Service, body) {
                Ok(_) => {
                    self.hosts.forget(&a.id);
                    let mut h = self.hosting.lock().unwrap();
                    h.closing.remove(&a.id);
                    h.attached.remove(&a.id);
                    h.answers_pending.remove(&a.id);
                }
                Err(e) => tracing::warn!("cannot record the close of {}: {}", a.id, e.1),
            }
        } else if !alive {
            let mut h = self.hosting.lock().unwrap();
            h.closing.remove(&a.id);
            h.stopped.insert(a.id.clone());
            let _ = self.changed.send(());
        } else if let Some(hello) = hello {
            match hello {
                Ok((epoch, _)) => self.publish(&a.id, epoch),
                Err(HostError::Refused(w)) if w == "incompatible" => {
                    tracing::warn!("the host of {} speaks another protocol than this service's ({}): close it by ending process {} and start the agent again", a.id, crate::runtime::HOST_PROTOCOL, s.host.pid);
                    self.hosting.lock().unwrap().incompatible.insert(a.id.clone());
                }
                Err(e) => tracing::debug!("cannot attach {} yet: {e}", a.id),
            }
        }
    }

    /// The journal decisions whose answer is not recorded yet. Set difference recovers an
    /// early lost receipt even when a later answer was recorded.
    fn record_answers(&self, id: &str, decided: Vec<(u64, core::hosted::Answer)>) -> bool {
        let read = got(self.store.lock().unwrap().hosted_agent(id));
        let Ok(Some(Some(mut a))) = read else { return false };
        for (request, decision) in decided {
            if a.answered.contains(&request) {
                continue;
            }
            if let Err(e) = self.record_answer(id, request, decision) {
                tracing::warn!("cannot record the answer to request {request} of {id}: {}", e.1);
                return false;
            }
            a.answered.push(request);
        }
        true
    }

    pub(super) fn record_answer(&self, id: &str, request: u64, decision: core::hosted::Answer) -> Result<(), ApiError> {
        let result = self.record(&Principal::Service, Body::AgentAnswered {
            agent: id.into(), request, decision, by: core::hosted::AnswerBy::User,
        });
        let Err(error) = result else { return Ok(()) };
        self.hosting.lock().unwrap().answers_pending.insert(id.into());
        // The reconciliation and a direct response can both try to record the same decision.
        if error.0 == axum::http::StatusCode::UNPROCESSABLE_ENTITY
            && self.store.lock().unwrap().hosted_agent(id)?.is_some_and(|a| a.answered.contains(&request))
        {
            return Ok(());
        }
        Err(error)
    }

    /// The user's close returns once its host accepts. Its outcome is recorded from the
    /// journal when the host has finished.
    pub(super) fn close_hosted(self: &Shared, a: &core::HostedAgent) -> Result<Value, ApiError> {
        let s = a.started.as_ref().ok_or_else(|| bad(format!("{} has not started: a start that does not finish ends by itself", a.id)))?;
        let _turn = self.turn(&a.id).ok_or_else(|| bad(format!("{} is starting or closing now; try again in a moment", a.id)))?;
        if !self.hosts.alive(&a.id) {
            return Err(bad(format!("{}'s host is not running: there is nothing to close", a.id)));
        }
        match self.hosts.close(&a.id, s.generation) {
            Ok(()) => {
                self.hosting.lock().unwrap().closing.insert(a.id.clone(), Instant::now());
                Ok(json!({"agent": a.id, "state": "closing"}))
            }
            Err(e) => Err(ApiError(axum::http::StatusCode::SERVICE_UNAVAILABLE, format!("{} was not closed: {e}; if its host (process {}) does not answer, end that process yourself", a.id, s.host.pid))),
        }
    }
}

fn started(id: &str, session: &str, h: &HostHello, host: core::Shell) -> Body {
    Body::AgentStarted {
        agent: id.into(),
        session: session.into(),
        model: h.model.clone(),
        host,
        generation: h.generation,
        version: h.version.clone(),
        account: h.account.clone(),
        plan: h.plan.clone(),
        approvals: h.approvals.clone(),
    }
}
