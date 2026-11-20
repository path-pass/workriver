// The service's life, in today's order: what start-up recovers from a previous run, the
// background read of other machines' repositories, the branches it watches, its poll of the
// runtime (a tick's steps as they have always run), and the stop.
use super::*;
use crate::server::commands::{review, shell};

/// How often the runtime's agents are read.
pub(in crate::server) const POLL: Duration = Duration::from_secs(2);

/// The git changes that can change what is shown: work-tree files, refs and HEAD.
// ponytail: node_modules and target are skipped by name, not by .gitignore; check ignores if a repo tracks such a folder.
pub(in crate::server) fn worth_a_look(path: &Path) -> bool {
    let parts: Vec<_> = path.components().map(|c| c.as_os_str().to_string_lossy()).collect();
    if parts.iter().any(|p| p == "node_modules" || p == "target") || parts.last().is_some_and(|p| p.ends_with(".lock")) {
        return false;
    }
    match parts.iter().position(|p| p == ".git") {
        None => true,
        Some(k) => parts[k..].iter().any(|p| p == "refs" || p == "HEAD" || p == "packed-refs"),
    }
}

impl App {
    /// Watches a unit's repository: its work tree, the work tree of its branch, and the
    /// git folder where refs move.
    pub(in crate::server) fn watch(&self, repo: &Path, branch: &str) {
        let here = scan::Runner::Local;
        let mut paths = vec![repo.to_path_buf()];
        paths.extend(scan::common_dir(&here, repo).ok());
        paths.extend(scan::worktree_of(&here, repo, branch).ok().flatten());
        let mut watcher = self.watcher.lock().unwrap();
        let Some(w) = watcher.as_mut() else { return };
        let mut seen = self.watched.lock().unwrap();
        for p in paths {
            if seen.insert(p.clone())
                && let Err(e) = w.watcher().watch(&p, RecursiveMode::Recursive)
            {
                tracing::warn!("cannot watch {}: {e}", p.display());
            }
        }
    }

    /// Reads the runtime's agents; says "changed" when they differ, and marks delivered
    /// messages picked up when their receiver starts working.
    pub(in crate::server) fn poll(&self) {
        self.see_roots();
        let mut now = self.runtime.agents().ok();
        // A held question's new agent still not ready after its window waits for the user
        // (e.g. on a first-run prompt the runtime does not report): shown blocked.
        {
            let at = std::time::Instant::now();
            let held = self.held.lock().unwrap();
            for a in now.iter_mut().flatten().filter(|a| a.state == AgentState::Unknown) {
                if held.iter().any(|h| (a.session == h.to || a.name == h.to) && h.not_ready.is_some_and(|end| end <= at)) {
                    a.state = AgentState::Blocked;
                }
            }
        }
        let groups = self.runtime.groups().unwrap_or_default();
        let regrouped = std::mem::replace(&mut *self.groups.lock().unwrap(), groups.clone()) != groups;
        {
            let mut blocked = self.blocked.lock().unwrap();
            let still: Vec<&RuntimeAgent> = now.iter().flatten().filter(|a| a.state == AgentState::Blocked).collect();
            blocked.retain(|s, _| still.iter().any(|a| &a.session == s));
            for a in still {
                blocked.entry(a.session.clone()).or_insert_with(core::now);
            }
            let mut idle = self.idle_since.lock().unwrap();
            let still: Vec<&RuntimeAgent> = now.iter().flatten().filter(|a| matches!(a.state, AgentState::Idle | AgentState::Done)).collect();
            idle.retain(|s, _| still.iter().any(|a| &a.session == s));
            for a in still {
                idle.entry(a.session.clone()).or_insert_with(|| (core::now(), std::time::Instant::now()));
            }
        }
        let stalled = now.as_deref().is_some_and(|live| self.find_stalls(live));
        if let Some(live) = now.as_deref() {
            self.merge_reminders(live);
            self.idle_reminders(live);
        }
        let differs = {
            let mut live = self.live.lock().unwrap();
            let differs = *live != now;
            *live = now.clone();
            differs
        };
        let tunnels: Vec<(String, bool)> = self.machines.iter().flat_map(|m| m.states()).map(|m| (m.label, m.tunnel)).collect();
        let differs = differs || std::mem::replace(&mut *self.tunnels.lock().unwrap(), tunnels.clone()) != tunnels;
        let mut picked = vec![];
        self.pending.lock().unwrap().retain_mut(|p| {
            let to = p.delivery.to.as_deref();
            match now.as_ref().and_then(|l| l.iter().find(|a| Some(a.session.as_str()) == to)) {
                None => p.sent.elapsed() < WATCH,
                Some(a) if a.state == AgentState::Working && p.armed => {
                    picked.push(Body::DeliveryUpdated {
                        unit: p.unit.clone(),
                        task: p.task,
                        finding: p.finding.clone(),
                        chore: p.chore.clone(),
                        delivery: Delivery { step: DeliveryStep::Picked, ..p.delivery.clone() },
                    });
                    false
                }
                Some(a) => {
                    p.armed |= a.state != AgentState::Working;
                    p.sent.elapsed() < WATCH
                }
            }
        });
        let recorded = !picked.is_empty();
        for body in picked {
            if let Err(e) = self.store.lock().unwrap().record(&Principal::Service, body) {
                tracing::warn!("cannot record a pick-up: {e}");
            }
        }
        // Held questions whose agent is free now are given to it; one whose agent is gone fails.
        if let Some(agents) = &now {
            let state = |h: &Held| agents.iter().find(|a| a.session == h.to || a.name == h.to).map(|a| a.state);
            let ready: Vec<Held> = {
                let mut held = self.held.lock().unwrap();
                let at = std::time::Instant::now();
                // A notice was never kept from an agent at work: only from a blind one, which
                // `hand` checks again.
                let free = |h: &Held| {
                    h.not_ready.is_some_and(|end| end > at)
                        || match h.kind {
                            For::Notice => !matches!(state(h), Some(AgentState::Blocked | AgentState::Unknown)),
                            // Only to an agent listed and free: whether it is gone is
                            // `discussions_gone`'s to say, from a complete read.
                            For::Discussion => matches!(state(h), Some(AgentState::Idle | AgentState::Done)),
                            For::Duty => matches!(state(h), Some(s) if !matches!(s, AgentState::Blocked | AgentState::Unknown)),
                            _ => !matches!(state(h), Some(AgentState::Blocked | AgentState::Working | AgentState::Unknown)),
                        }
                };
                let (ready, wait): (Vec<Held>, Vec<Held>) = held.drain(..).partition(free);
                *held = wait;
                ready
            };
            for h in ready {
                match state(&h) {
                    None => match h.kind {
                        For::Message => self.message_not_delivered(&h.question, Some(&h.to), ReasonCode::ManagerGone, &format!("{} closed before it could be given the message", h.name)),
                        For::Question => self.fail_question(&h.question, ReasonCode::RuntimeError, &format!("{} closed before it could be given the question", h.name)),
                        For::Review => self.review_failed(&h.question, &format!("{} closed before it could be given the command", h.name)),
                        For::Notice => {
                            let gone = RuntimeError::NoAgent(h.name.clone());
                            self.told(h, Err(gone))
                        }
                        // Not listed is not gone: only `discussions_gone`, from a complete read, says
                        // that; a duty waits for its manager. Both are held again.
                        For::Discussion | For::Duty => self.held.lock().unwrap().push(Held { not_ready: None, ..h }),
                    },
                    Some(_) => self.hand(h),
                }
            }
            let gone = crate::server::conversations::gone_from(&mut self.seen_on.lock().unwrap(), agents, &self.readable(agents));
            *self.gone.lock().unwrap() = gone.clone();
            self.discussions_gone(&gone);
            self.discussions_over();
        }
        // Questions whose temporary agent has not answered in time.
        let now = std::time::Instant::now();
        let late: Vec<String> = self.deadlines.lock().unwrap().iter().filter(|(_, at)| *at <= now).map(|(id, _)| id.clone()).collect();
        for id in &late {
            let held = self.held.lock().unwrap().iter().find(|h| &h.question == id).map(|h| (h.name.clone(), h.not_ready.is_some()));
            let secs = self.waits.answer.as_secs();
            match held {
                // Its terminal stays: the user may be about to answer what it waits on.
                Some((name, true)) => self.drop_question(id, ReasonCode::NotReady, &format!("{name} never became ready for a prompt in {secs} seconds; the question never reached it, and its terminal was left open for you")),
                Some((name, false)) => self.fail_question(id, ReasonCode::Blocked, &format!("{name} waited for an answer in its terminal for {secs} seconds; the question never reached it")),
                None => self.fail_question(id, ReasonCode::Timeout, &format!("no answer within {secs} seconds")),
            }
        }
        let recorded = recorded || !late.is_empty();
        // Reviews of commands not given in time: their agents go.
        let late: Vec<String> = self.runs.lock().unwrap().iter().filter(|(_, r)| r.review.pending() && r.review.until.is_some_and(|u| u <= now)).map(|(id, _)| id.clone()).collect();
        for id in late {
            self.review_failed(&id, &format!("no review within {} seconds", self.waits.answer.as_secs()));
        }
        // Threads idle for too long: their temporary agent goes; the thread stays open.
        let gone: Vec<String> = {
            let mut idle = self.idle.lock().unwrap();
            let gone = idle.iter().filter(|(_, _, at)| *at <= now).map(|(_, name, _)| name.clone()).collect();
            idle.retain(|(_, _, at)| *at > now);
            gone
        };
        for name in gone {
            if let Err(e) = self.runtime.close(&name) {
                tracing::warn!("cannot close {name}: {e}");
            }
        }
        if differs || recorded || regrouped || stalled {
            let _ = self.changed.send(());
        }
    }

    /// Repositories on other machines, read in the background (start-up; `kicked`: whenever
    /// a record touches one of their units).
    pub(in crate::server) fn read_remote(self: &Arc<Self>, shared: Arc<Mutex<views::Shared>>, kicked: std::sync::mpsc::Receiver<()>) {
        let app = self;
        // Repositories on other machines cannot be watched: they are read in the background,
        // every `REMOTE` and whenever a record touches one of their units, never while a request
        // waits; the page hears when something there changed.
        let remote = app.clone();
        let mut bg = Cache::following(app.machines.clone()).sharing(shared).remembering(app.home.join(crate::common::home::SCAN_CACHE).join(crate::common::home::SCAN_CACHE_REMOTE));
        bg.remote_reads = true;
        std::thread::spawn(move || {
            let mut wait = views::REMOTE;
            loop {
                match kicked.recv_timeout(wait) {
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    _ => while kicked.try_recv().is_ok() {},
                }
                if *remote.stopping.borrow() {
                    break;
                }
                let Ok(Some(state)) = got(remote.store.lock().unwrap().recorded()) else { continue };
                if !state.units.iter().any(|u| u.machine.is_some()) {
                    continue;
                }
                let started = std::time::Instant::now();
                let (changed, skipped) = views::refresh_remote(&state, &mut bg);
                // A machine not reachable yet (just started, reconnecting): looked at again soon.
                wait = if skipped { Duration::from_secs(1) } else { views::REMOTE };
                bg.save();
                let found = std::mem::take(&mut bg.merged_now);
                remote.record_merges(found);
                let ms = started.elapsed().as_millis() as u64;
                tracing::info!(target: "perf", kind = "remote_read", ms, recomputed = bg.recomputed, reused = bg.reused, changed, slow = ms >= crate::common::config::defaults::SLOW_MS);
                if changed {
                    let _ = remote.changed.send(());
                }
            }
        });
        let _ = app.remote_kick.send(());
    }

    /// What a previous run left: deliveries to watch again, questions and messages that lost
    /// their wait, temporary agents nobody will hear (start-up, in this order).
    pub(in crate::server) fn recover(self: &Arc<Self>) {
        let app = self;
        // Messages delivered before a restart are watched again; whether their receiver was seen
        // idle since is lost, so it must be seen not working first.
        let now = std::time::Instant::now();
        let pending: Vec<Pending> = got(app.store.lock().unwrap().delivered()).ok().flatten().unwrap_or_default().into_iter().map(|d| Pending { unit: d.unit, task: d.task, finding: d.finding, chore: d.chore, delivery: d.delivery, armed: false, sent: now }).collect();
        app.pending.lock().unwrap().extend(pending);
        // Questions left open by a previous run have lost their wait; close them. Not those of
        // a discussion: their answerer is the task's own agent, which goes on; one never given
        // is given now.
        let open = got(app.store.lock().unwrap().open_questions()).ok().flatten().unwrap_or_default();
        for id in open {
            app.fail_question(&id, ReasonCode::Restarted, "the service restarted while the question was open");
        }
        app.discussions_again();
        // Messages to a manager held by a previous run (its manager was waiting in its terminal) are lost.
        let held = got(app.store.lock().unwrap().held_messages()).ok().flatten().unwrap_or_default();
        for id in held {
            app.message_not_delivered(&id, None, ReasonCode::Restarted, "the service restarted while the message waited for its manager");
        }
        // So are deliveries a previous run held for a receiver on a blind task: recorded as not
        // delivered, so the page says so and the user can send it again.
        let held = got(app.store.lock().unwrap().held_deliveries()).ok().flatten().unwrap_or_default();
        for d in held {
            let delivery = Delivery { step: DeliveryStep::NotDelivered, reason_code: Some(ReasonCode::Restarted), reason: Some("the service restarted while it waited for its receiver's blind task".into()), ..d.delivery };
            let body = Body::DeliveryUpdated { unit: d.unit, task: d.task, finding: d.finding, chore: d.chore, delivery };
            if let Err(e) = app.store.lock().unwrap().record(&Principal::Service, body) {
                tracing::warn!("cannot record a delivery: {e}");
            }
        }
        // ponytail: a thread's temporary agent from a previous run is closed rather than adopted;
        // the next question starts a new one with the thread so far.
        let left = got(app.store.lock().unwrap().thread_agents()).ok().flatten().unwrap_or_default();
        for name in left {
            let _ = app.runtime.close(&name);
        }
        // So is an agent a previous run started to review a command: what it would say is lost.
        let left: Vec<String> = app.runtime.agents().unwrap_or_default().into_iter().filter(|a| a.name.starts_with(&review::reviewer("")) && a.group.as_ref().is_some_and(|g| g.label == shell::GROUP)).map(|a| a.name).collect();
        for name in left {
            let _ = app.runtime.close(&name);
        }
    }

    /// Waits for the stop signal, then tells every part to stop.
    pub(in crate::server) async fn stop(self: Arc<Self>) {
        let app = self;
        stop_signal().await;
        tracing::info!("stopping: open requests get {} s", GRACE.as_secs());
        let _ = app.stopping.send(true);
        app.metrics.stop();
        if let Some(m) = &app.machines {
            m.stop_all();
        }
    }
}

/// How long open requests may finish once the service is told to stop.
pub(in crate::server) const GRACE: Duration = Duration::from_secs(2);

/// Returns at the first Ctrl-C or SIGTERM; a second one ends the process at once.
async fn stop_signal() {
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term => {}
    }
    tokio::spawn(async {
        if tokio::signal::ctrl_c().await.is_ok() {
            std::process::exit(130);
        }
    });
}
