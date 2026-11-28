// A hosted agent's life as the service sees it: the user's start (reserved, spawned, waited
// for, recorded, published), the user's close (asked of the host, recorded from its journal's
// one `closed` line), and the reconciliation that brings the records and the hosts back
// together at start-up and at each poll. The service never signals a host or its agent.
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use workriver_core::{self as core, Body, Principal};

use crate::runtime::{HostError, Published};
use crate::server::app::*;
use crate::server::http::*;

/// How long a start may take, from its reservation, before it is closed and ended.
pub(in crate::server) const START_WAIT: Duration = Duration::from_secs(60);
/// How long after a close is asked for its outcome is waited for before it is unconfirmed:
/// the host's own deadline and some.
pub(in crate::server) const CLOSE_WAIT: Duration = Duration::from_secs(40);

/// What the service keeps of its hosts while it runs (in memory; the records and the
/// journals hold what lasts).
#[derive(Default)]
pub(in crate::server) struct Hosting {
    /// Instances whose start, close or reconciliation runs now: one at a time each.
    busy: std::collections::HashSet<String>,
    /// Instances attached in this run (a hello answered with the protocol this service speaks).
    attached: std::collections::HashSet<String>,
    /// Instances whose host speaks another protocol: never attached, the user ends them.
    pub(super) incompatible: std::collections::HashSet<String>,
    /// Instances whose host is gone and whose journal has no `closed` line: stopped, read once.
    pub(super) stopped: std::collections::HashSet<String>,
    /// Closes asked for and not recorded yet, with when they were asked.
    pub(super) closing: std::collections::HashMap<String, Instant>,
}

/// One instance's turn at its lifecycle: released when dropped.
pub(super) struct Turn<'a>(&'a App, String);

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        self.0.hosting.lock().unwrap().busy.remove(&self.1);
    }
}

impl App {
    /// The instance's lifecycle for the caller alone, or None while another holds it.
    pub(super) fn turn(&self, id: &str) -> Option<Turn<'_>> {
        self.hosting.lock().unwrap().busy.insert(id.into()).then(|| Turn(self, id.into()))
    }

    /// The rest of a reserved start, in the background: the host spawned, waited for until
    /// `START_WAIT` after the reservation, its start recorded and published. A start that fails
    /// closes its host and ends the reservation once the host is gone; a host that does not go
    /// keeps the reservation open (its name taken, never published) for the reconciliation.
    pub(super) fn start_hosted(&self, a: core::HostedAgent) {
        let Some(_turn) = self.turn(&a.id) else { return };
        let begun = Instant::now();
        let (pid, start) = match self.hosts.spawn(&a.id, &a.harness, &a.cwd, &a.model, &a.auth) {
            Ok(root) => root,
            Err(e) => return self.end_start(&a.id, &format!("the host did not start: {e:#}")),
        };
        let mut why = "did not start in time".to_string();
        while begun.elapsed() < START_WAIT {
            if !self.hosts.alive(&a.id) {
                why = "the host stopped before the agent started; see its host.log".into();
                break;
            }
            match self.hosts.hello(&a.id) {
                Ok(h) if h["session"].is_string() && h["state"] != "starting" => {
                    self.hosts.pending(&a.id, h["session"].as_str().unwrap_or_default());
                    let body = started(&a.id, &h, pid, start);
                    match self.record(&Principal::Service, body) {
                        Ok(_) => return self.publish(&a.id),
                        Err(e) => {
                            why = format!("its start could not be recorded: {}", e.1);
                            break;
                        }
                    }
                }
                Ok(h) if h["state"] == "closing" || h["state"] == "closed" => {
                    why = "the host closed before the agent started".into();
                    break;
                }
                Err(HostError::Refused(w)) if w == "incompatible" => {
                    why = "the host speaks another protocol".into();
                    break;
                }
                _ => {}
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        self.stop_start(&a.id, &why);
    }

    /// A start that will not finish: its host asked to close, and the start ended once the
    /// host is gone (waited for within the close's own deadline).
    fn stop_start(&self, id: &str, why: &str) {
        let _ = self.hosts.call(id, "close", json!({"generation": 1}), Duration::from_secs(5));
        let until = Instant::now() + self.waits.hosted_close;
        while self.hosts.alive(id) && Instant::now() < until {
            self.hosts.reap();
            std::thread::sleep(Duration::from_millis(200));
        }
        self.hosts.reap();
        if self.hosts.alive(id) {
            tracing::warn!("the host of {id} is still running after its start failed ({why}); it is kept starting until it is gone");
            return;
        }
        self.end_start(id, why);
    }

    fn end_start(&self, id: &str, why: &str) {
        self.hosts.forget(id);
        if let Err(e) = self.record(&Principal::Service, Body::AgentStartEnded { agent: id.into(), why: why.into() }) {
            tracing::warn!("cannot record that the start of {id} ended ({why}): {}", e.1);
        }
    }

    /// The instance's agent is the records' from now on: its root is an agent's place.
    pub(super) fn publish(&self, id: &str) {
        let read = got(self.store.lock().unwrap().recorded());
        let Ok(Some(all)) = read else { return };
        let Ok(a) = all.hosted_agent(id) else { return };
        if let Some(s) = &a.started {
            self.hosts.track(id, s.host.pid, s.host.start);
            self.hosts.publish(id, Published { session: s.session.clone(), name: a.name.clone(), harness: a.harness.clone(), cwd: a.cwd.clone(), generation: s.generation });
            self.hosting.lock().unwrap().attached.insert(id.into());
            let _ = self.changed.send(());
        }
    }

    /// The records and the hosts brought together: at start-up, and at each poll for what is
    /// open. Each instance under its own lifecycle; one held by a start or a close is left to
    /// it. A failed read of the records skips this round.
    pub(in crate::server) fn reconcile_hosted(&self) {
        self.hosts.reap();
        let read = got(self.store.lock().unwrap().recorded());
        let Ok(Some(all)) = read else { return };
        let open: Vec<core::HostedAgent> = all.hosted.iter().filter(|a| a.open()).cloned().collect();
        let unreserved: Vec<String> = self.hosts.on_disk().into_iter().filter(|id| all.hosted_agent(id).is_err()).collect();
        drop(all);
        // A host no reservation is for is never adopted: asked to close, and logged.
        for id in unreserved {
            if let Some((pid, start)) = instance_host(&self.hosts.dir(&id)) {
                tracing::warn!("a host runs for {id}, which no start reserved (process {pid}): it is asked to close");
                self.hosts.track(&id, pid, start);
                let _ = self.hosts.call(&id, "close", json!({"generation": 1}), Duration::from_secs(5));
                self.hosts.forget(&id);
            }
        }
        for a in open {
            let Some(_turn) = self.turn(&a.id) else { continue };
            match &a.started {
                None => self.reconcile_reserved(&a),
                Some(s) => self.reconcile_started(&a, s),
            }
        }
    }

    /// A reservation with no start: a young one is its start's (which ends it either way);
    /// an older one a stopped service, or a start that could not close its host, left. Its
    /// host, if it runs, is closed (never adopted: the service takes a host's root only from
    /// the child it spawned), and the start ends once it is gone.
    fn reconcile_reserved(&self, a: &core::HostedAgent) {
        let age = humantime::parse_rfc3339_weak(&a.at).ok().and_then(|t| t.elapsed().ok());
        if age.is_none_or(|age| age < self.waits.hosted_young) {
            return;
        }
        if self.hosts.root(&a.id).is_none() {
            if let Some((pid, start)) = instance_host(&self.hosts.dir(&a.id)) {
                self.hosts.track(&a.id, pid, start);
            }
        }
        if !self.hosts.alive(&a.id) {
            return self.end_start(&a.id, "the service stopped before the start finished");
        }
        self.stop_start(&a.id, "the service stopped before the start finished");
    }

    /// A started agent not closed: the close its journal holds is recorded (with the answers
    /// whose receipts were lost); a host that runs is attached once; a host gone is stopped.
    fn reconcile_started(&self, a: &core::HostedAgent, s: &core::hosted::Started) {
        self.hosts.track(&a.id, s.host.pid, s.host.start);
        let alive = self.hosts.alive(&a.id);
        let (closing, stopped, attached) = {
            let h = self.hosting.lock().unwrap();
            (h.closing.contains_key(&a.id), h.stopped.contains(&a.id), h.attached.contains(&a.id))
        };
        if alive && !closing {
            if !attached && !self.hosting.lock().unwrap().incompatible.contains(&a.id) {
                match self.hosts.hello(&a.id) {
                    Ok(_) => {
                        self.record_answers(a);
                        self.publish(&a.id);
                    }
                    Err(HostError::Refused(w)) if w == "incompatible" => {
                        tracing::warn!("the host of {} speaks another protocol than this service's ({}): close it by ending process {} and start the agent again", a.id, crate::runtime::HOST_PROTOCOL, s.host.pid);
                        self.hosting.lock().unwrap().incompatible.insert(a.id.clone());
                    }
                    Err(e) => tracing::debug!("cannot attach {} yet: {e}", a.id),
                }
            }
            return;
        }
        if stopped {
            return;
        }
        // Gone, or closing: the journal says how it ended, if it did.
        match self.journal_end(&a.id) {
            Ok(Some(closed)) => {
                self.record_answers(a);
                let outcome = if closed["outcome"] == "ended" { core::hosted::Outcome::Ended } else { core::hosted::Outcome::Stuck };
                let pids = closed["pids"].as_array().into_iter().flatten().filter_map(|p| p.as_u64()).map(|p| p as u32).collect();
                match self.record(&Principal::Service, Body::AgentClosed { agent: a.id.clone(), outcome, pids }) {
                    Ok(_) => {
                        self.hosts.forget(&a.id);
                        let mut h = self.hosting.lock().unwrap();
                        h.closing.remove(&a.id);
                        h.attached.remove(&a.id);
                    }
                    Err(e) => tracing::warn!("cannot record the close of {}: {}", a.id, e.1),
                }
            }
            Ok(None) if !alive => {
                self.record_answers(a);
                self.hosting.lock().unwrap().stopped.insert(a.id.clone());
                let _ = self.changed.send(());
            }
            Ok(None) => {}
            Err(e) => tracing::warn!("cannot read the journal of {}: {e:#}", a.id),
        }
    }

    /// The journal's `closed` line, if it has one: the close's one authority.
    fn journal_end(&self, id: &str) -> anyhow::Result<Option<Value>> {
        let mut found = None;
        crate::runtime::journal_lines(&self.hosts.journal(id), |_, line| {
            if mentions(line, br#""event":"closed""#) {
                let v: Value = serde_json::from_slice(line)?;
                if v["kind"] == "host" && v["event"] == "closed" {
                    found = Some(v);
                }
            }
            Ok(true)
        })?;
        Ok(found)
    }

    /// Every decision of the user's the journal holds (over all generations) whose answer is
    /// not recorded, recorded now: a set difference, so a receipt lost before a later one
    /// that was recorded is found too.
    pub(super) fn record_answers(&self, a: &core::HostedAgent) {
        let mut decided = vec![];
        let read = crate::runtime::journal_lines(&self.hosts.journal(&a.id), |_, line| {
            if mentions(line, br#""event":"decided""#) {
                let v: Value = serde_json::from_slice(line)?;
                if let (Some(request), Some(kind), true) = (v["request"].as_u64(), v["optionKind"].as_str(), v["kind"] == "host" && v["event"] == "decided") {
                    decided.push((request, if kind == "allow_once" { core::hosted::Answer::Allow } else { core::hosted::Answer::Reject }));
                }
            }
            Ok(true)
        });
        if let Err(e) = read {
            return tracing::warn!("cannot read the journal of {}: {e:#}", a.id);
        }
        let recorded = |request: u64| -> bool {
            let read = got(self.store.lock().unwrap().recorded());
            matches!(read, Ok(Some(all)) if all.hosted_agent(&a.id).is_ok_and(|x| x.answered.contains(&request)))
        };
        for (request, decision) in decided {
            if !recorded(request) && let Err(e) = self.record(&Principal::Service, Body::AgentAnswered { agent: a.id.clone(), request, decision }) {
                tracing::warn!("cannot record the answer to request {request} of {}: {}", a.id, e.1);
            }
        }
    }

    /// The user's close: asked of the host, which answers at once; the outcome is recorded
    /// from its journal once it is there (`reconcile_started`).
    pub(super) fn close_hosted(&self, a: &core::HostedAgent) -> Result<Value, ApiError> {
        let s = a.started.as_ref().ok_or_else(|| bad(format!("{} has not started: a start that does not finish ends by itself", a.id)))?;
        let _turn = self.turn(&a.id).ok_or_else(|| bad(format!("{} is starting or closing now; try again in a moment", a.id)))?;
        if !self.hosts.alive(&a.id) {
            return Err(bad(format!("{}'s host is not running: there is nothing to close (it shows as stopped)", a.id)));
        }
        match self.hosts.call(&a.id, "close", json!({"generation": s.generation}), Duration::from_secs(5)) {
            Ok(_) => {
                self.hosting.lock().unwrap().closing.insert(a.id.clone(), Instant::now());
                Ok(json!({"agent": a.id, "state": "closing"}))
            }
            Err(e) => Err(ApiError(axum::http::StatusCode::SERVICE_UNAVAILABLE, format!("{} was not closed: {e}; if its host (process {}) does not answer, end that process yourself", a.id, s.host.pid))),
        }
    }
}

/// Whether a journal line holds `what` at all: a cheap look before it is parsed.
fn mentions(line: &[u8], what: &[u8]) -> bool {
    line.windows(what.len()).any(|w| w == what)
}

/// `agent.started` from the host's `hello`, with its root as the service spawned it.
fn started(id: &str, h: &Value, pid: u32, start: u64) -> Body {
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    Body::AgentStarted {
        agent: id.into(),
        session: text(&h["session"]),
        model: text(&h["model"]),
        host: core::Shell { pid, start },
        generation: h["generation"].as_u64().unwrap_or(1),
        version: text(&h["versions"]["harnessVersion"]),
        account: text(&h["billing"]["account"]),
        plan: text(&h["billing"]["plan"]),
        approvals: text(&h["approvals"]["approvalPolicy"]),
    }
}

/// The host a starting instance's own record names, while that process runs: only ever
/// closed, never published (the service takes a published root only from its own child).
fn instance_host(dir: &std::path::Path) -> Option<(u32, u64)> {
    let v: Value = serde_json::from_slice(&std::fs::read(dir.join("instance.json")).ok()?).ok()?;
    let (pid, start) = (v["host"]["pid"].as_u64()? as u32, v["host"]["start"].as_u64()?);
    crate::common::process::one(pid).filter(|p| p.start == start).map(|_| (pid, start))
}
