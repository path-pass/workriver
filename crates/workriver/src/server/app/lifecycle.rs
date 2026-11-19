// While the service runs: the branches it watches, and its poll of the runtime, in the order
// things have always happened in a tick.
use super::*;

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
            if let Err(e) = self.store.lock().unwrap().append(&Principal::Service, body) {
                tracing::warn!("cannot record a pick-up: {e}");
            }
        }
        // Held questions whose agent is free now are given to it; one whose agent is gone fails.
        if let Some(agents) = &now {
            let state = |h: &Held| agents.iter().find(|a| a.session == h.to || a.name == h.to).map(|a| a.state);
            let ready: Vec<Held> = {
                let mut held = self.held.lock().unwrap();
                let at = std::time::Instant::now();
                let free = |h: &Held| h.not_ready.is_some_and(|end| end > at) || !matches!(state(h), Some(AgentState::Blocked | AgentState::Working | AgentState::Unknown));
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
                    },
                    Some(_) => self.hand(h),
                }
            }
        }
        // Questions whose temporary agent has not answered in time.
        let now = std::time::Instant::now();
        let late: Vec<String> = self.deadlines.lock().unwrap().iter().filter(|(_, at)| *at <= now).map(|(id, _)| id.clone()).collect();
        for id in &late {
            let held = self.held.lock().unwrap().iter().find(|h| &h.question == id).map(|h| (h.name.clone(), h.not_ready.is_some()));
            let secs = self.waits.answer.as_secs();
            match held {
                // Its tab stays: the user may be about to answer what it waits on.
                Some((name, true)) => self.drop_question(id, ReasonCode::NotReady, &format!("{name} never became ready for a prompt in {secs} seconds; the question never reached it, and its tab was left open for you")),
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
}
