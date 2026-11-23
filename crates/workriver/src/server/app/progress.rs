// What is under way between records: messages held for an agent that cannot take them yet,
// deliveries watched for their pick-up, and the reminders the poll runs.
use super::*;

/// A question's message (or the user's message to a manager) held for its agent, which
/// waits for a person in its terminal; or what workriver tells an agent on a blind task in
/// progress (`App::tell`), held until that task is delivered.
// ponytail: in memory; after a restart the question fails as `restarted`, like any open one.
pub(in crate::server) struct Held {
    /// The question's id; the user's message's; or the run's whose command is to be reviewed.
    pub(in crate::server) question: String,
    pub(in crate::server) kind: For,
    /// The session or live name it goes to, and the name to say.
    pub(in crate::server) to: String,
    pub(in crate::server) name: String,
    pub(in crate::server) text: String,
    /// A new agent not ready for a prompt: retried at each poll until then; after it, it may
    /// sit on a prompt the runtime does not report and waits for the user like a blocked one.
    pub(in crate::server) not_ready: Option<std::time::Instant>,
    /// A notice that is a planned delivery: recorded where it is once given (`App::told`).
    pub(in crate::server) about: Option<delivery::Plan>,
}

impl Held {
    /// What a receiver must act on (`For::Duty`), to `agent`.
    pub(in crate::server) fn duty(agent: String, text: String) -> Held {
        Held { question: String::new(), kind: For::Duty, to: agent.clone(), name: agent, text, not_ready: None, about: None }
    }

    /// Which copy of a user's message it is: to every manager, its manager's; else none.
    pub(in crate::server) fn copy(&self) -> &str {
        if self.kind == For::Broadcast { &self.to } else { "" }
    }
}

/// What a held message is for.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::server) enum For {
    Question,
    /// The user's message to a manager.
    Message,
    /// One manager's copy of the user's message to every manager (`to` is that manager).
    Broadcast,
    /// A command for its review agent (`App::review_run`).
    Review,
    /// What workriver tells an agent (an answer, a delivery's message, an escalation), held
    /// while the agent is on a blind task in progress (`App::tell`).
    Notice,
    /// The user's question in a discussion, to the task's own agent: held while it works too,
    /// and while it is on a blind task in progress or not listed; never timed out.
    Discussion,
    /// What a manager must act on (a discussed result amended while its gate waits, the
    /// conclusion): held like a notice, and also while the manager is not listed or a send
    /// fails, until it is given; never dropped while the service runs.
    Duty,
}

impl For {
    /// A notice and the user's message (or a copy of it) are for a session: an agent that only
    /// goes by it as a name is not it (any agent may take a seated manager's session id as its
    /// name). A duty and a discussion's question are when their `to` is one (`App::by_session`).
    pub(in crate::server) fn by_session(self) -> bool {
        matches!(self, For::Notice | For::Message | For::Broadcast)
    }
}

/// A delivered message: picked up once its receiver is seen working after it arrived.
/// Rebuilt from the records at start.
pub(in crate::server) struct Pending {
    pub(super) unit: String,
    pub(super) task: u32,
    pub(super) finding: Option<String>,
    pub(super) chore: Option<String>,
    pub(super) delivery: Delivery,
    /// The receiver was seen not working since the message went out.
    pub(super) armed: bool,
    pub(super) sent: std::time::Instant,
}

/// How long a delivered message is watched for its pick-up; the receiver may vanish from
/// the runtime for a while (a restart, a moved place) and come back.
pub(super) const WATCH: Duration = Duration::from_secs(30 * 60);

impl App {
    /// Works out which running tasks' agents stopped (see `reminders::step`): the agent itself is
    /// reminded once; if that does not help, its intent's manager is told, once, unless the
    /// manager is the one that stopped (then, as with no manager, only the user sees it, in
    /// Needs you). Whether anything changed.
    pub(super) fn find_stalls(&self, live: &[RuntimeAgent]) -> bool {
        let read = self.readable(live);
        let threshold = self.waits.minute * self.stall_minutes.load(std::sync::atomic::Ordering::Relaxed);
        let idle = self.idle_since.lock().unwrap().clone();
        let mut tell = vec![];
        let Ok(Some(state)) = got(self.store.lock().unwrap().recorded()) else { return false };
        let changed = {
            let state = &state;
            let mut stalls = self.stalls.lock().unwrap();
            let (next, found) = delivery::reminders::step(state, live, &idle, threshold, self.waits.minute, &read, &stalls);
            for f in &found {
                let to = match f.act {
                    delivery::reminders::Act::Remind => Some(f.agent.as_str()),
                    delivery::reminders::Act::Escalate => {
                        let stopped = |m: &str| m == f.agent || state.name_of(m) == Some(f.agent.as_str());
                        state.intent_of(&f.owner).and_then(|i| state.manager_of(i)).filter(|m| !stopped(m) && live.iter().any(|a| a.session == *m && a.state != AgentState::Unknown))
                    }
                };
                if let Some(to) = to
                    && let Some(text) = delivery::reminders::message(state, f)
                {
                    tell.push((to.to_string(), text, matches!(f.act, delivery::reminders::Act::Escalate)));
                }
            }
            let seen = |m: &std::collections::HashMap<(String, u32), delivery::reminders::Watch>| m.iter().filter_map(|(k, w)| Some((k.clone(), w.stall.clone()?))).collect::<Vec<_>>();
            let mut was = seen(&stalls);
            let mut now = seen(&next);
            was.sort_by(|a, b| a.0.cmp(&b.0));
            now.sort_by(|a, b| a.0.cmp(&b.0));
            *stalls = next;
            was != now
        };
        for (to, text, escalate) in tell {
            // The worker hears of its own task; its manager, like every manager, not while blind.
            let sent = if escalate { self.tell(&to, &text, None) } else { Ok(self.send(&to, &text)) };
            if let Ok(Err(e)) = sent {
                tracing::warn!("cannot tell {to} of a stopped worker: {e}");
            }
        }
        changed
    }

    /// Where the runtime was read just now and listed agents (None: this machine): elsewhere
    /// (a failed or empty read, a machine that cannot be reached or this one's runtime that
    /// did not answer, listed as last seen) agents are unknown.
    pub(super) fn readable(&self, live: &[RuntimeAgent]) -> Vec<Option<String>> {
        let down: Vec<String> = self.machines.iter().flat_map(|m| m.states()).filter(|m| !m.reachable).map(|m| m.label).collect();
        let here_unread = self.machines.as_ref().is_some_and(|m| !m.here_read());
        let mut read: Vec<Option<String>> = live.iter().map(|a| a.machine.clone()).filter(|m| !m.as_ref().is_some_and(|m| down.contains(m)) && !(m.is_none() && here_unread)).collect();
        read.dedup();
        read
    }

    /// Workers idle with no task for `idle_minutes` (see `reminders::idle_workers`): their manager
    /// is told, once per idle period, to close them if it has no work for them; workriver never
    /// closes them. Only a manager that is running and not on a blind task; until then nothing
    /// is marked told.
    pub(super) fn idle_reminders(&self, live: &[RuntimeAgent]) {
        let threshold = self.waits.minute * self.idle_minutes;
        let idle = self.idle_since.lock().unwrap().clone();
        let read = self.readable(live);
        let mut told = self.idle_told.lock().unwrap();
        told.retain(|(s, since)| idle.get(s).is_some_and(|(x, _)| x == since));
        let Ok(Some(state)) = got(self.store.lock().unwrap().recorded()) else { return };
        let tell: Vec<_> = {
            let state = &state;
            let reachable = |m: &str| live.iter().any(|a| a.session == m && a.state != AgentState::Unknown) && state.blind_tasks(m).is_empty();
            delivery::reminders::idle_workers(state, live, &idle, threshold, &read)
                .into_iter()
                .filter(|w| !told.contains(&(w.session.clone(), w.since.clone())) && reachable(&w.manager))
                .filter_map(|w| Some((w.manager.clone(), (w.session.clone(), w.since.clone()), delivery::reminders::idle_message(state, &w)?)))
                .collect()
        };
        for (to, key, text) in tell {
            match self.send(&to, &text) {
                Ok(()) => {
                    told.insert(key);
                }
                Err(e) => tracing::warn!("cannot tell {to} of an idle worker: {e}"),
            }
        }
    }

    /// Ready units that may have been merged where git cannot see it. One that looks
    /// squash-merged (`views::Shared::squash`): its manager is told once per branch tip.
    /// One ready for longer than `ready_hours`: its manager is asked, once per ready period,
    /// to check its pull request. Only a manager that is running and not on a blind task;
    /// until then nothing is marked told. With no manager the page shows it in Needs you.
    pub(super) fn merge_reminders(&self, live: &[RuntimeAgent]) {
        let squash = self.shared.lock().unwrap().squash.clone();
        let after = self.waits.hour * self.ready_hours;
        let mut tell = vec![];
        let Ok(Some(state)) = got(self.store.lock().unwrap().recorded()) else { return };
        {
            let state = &state;
            let told = self.merge_told.lock().unwrap();
            // A spike is never merged: nothing to remind of.
            for u in state.units.iter().filter(|u| u.has_code() && u.kind.merges()) {
                let Some(since) = state.ready_since(u) else { continue };
                let Some(to) = state.manager_of(&u.intent).filter(|m| live.iter().any(|a| a.session == *m && a.state != AgentState::Unknown) && state.blind_tasks(m).is_empty()) else { continue };
                let (id, branch) = (&u.id, delivery::quote::name(&u.branch));
                if let Some(s) = squash.get(id) {
                    let key = format!("squash {id} {}", s.tip);
                    let short: String = s.commit.chars().take(9).collect();
                    let text = format!(
                        "[workriver] {id} ({branch}) looks merged into {} by squash: every change of it is there (commit {short} matches). If so, record it: `workriver unit merged {id}`; if not, ignore this. How: `workriver guide how closing-units`.",
                        delivery::quote::name(&s.into)
                    );
                    tell.push((to.to_string(), key, text));
                }
                let ago = humantime::parse_rfc3339(&since).ok().and_then(|t| t.elapsed().ok());
                if let Some(ago) = ago.filter(|a| *a >= after) {
                    let key = format!("ready {id} {since}");
                    let text = format!(
                        "[workriver] {id} ({branch}) has been ready to merge for {} h and git shows no merge. Check its pull request: merged (by squash, say): `workriver unit merged {id}`; still open: nothing to do; abandoned: `workriver unit done {id} --reason \"...\"`. How: `workriver guide how push-pr-merge`.",
                        ago.as_secs() / self.waits.hour.as_secs().max(1)
                    );
                    tell.push((to.to_string(), key, text));
                }
            }
            tell.retain(|(_, key, _)| !told.contains(key));
        }
        for (to, key, text) in tell {
            if let Err(e) = self.send(&to, &text) {
                tracing::warn!("cannot remind {to} of a merge: {e}");
                continue;
            }
            let mut told = self.merge_told.lock().unwrap();
            told.insert(key);
            let path = self.home.join(crate::common::home::MERGE_TOLD);
            if let Err(e) = serde_json::to_vec_pretty(&*told).map_err(std::io::Error::other).and_then(|b| std::fs::write(&path, b)) {
                tracing::warn!("cannot write {}: {e}", path.display());
            }
        }
    }

    /// Units git shows merged (a fast-forward, a merge commit) are recorded merged, with the
    /// commits they had, as `workriver unit merged` would.
    pub(in crate::server) fn record_merges(&self, found: Vec<(String, Vec<String>)>) {
        for (unit, commits) in found {
            if let Err(e) = self.record(&Principal::Service, Body::UnitMerged { unit: unit.clone(), commits }) {
                tracing::warn!("cannot record unit {unit} merged: {}", e.1);
            }
        }
    }

    /// A manager that was at work before this build of the service is told, once, to read
    /// what changed: the skill it follows was read from an older workriver, and nothing else
    /// would tell it. `fresh`: it is seated now, and the seating message has it read this build's guide.
    // Which manager was told of which build is kept in a small file next to the database.
    pub(in crate::server) fn tell_of_update(&self, session: &str, fresh: bool) {
        if !fresh && !got(self.store.lock().unwrap().manages(session)).ok().flatten().unwrap_or(false) {
            return;
        }
        let _one = self.told.lock().unwrap();
        let path = self.home.join(crate::common::home::GUIDE_TOLD);
        let mut told: std::collections::BTreeMap<String, String> = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        if told.get(session).map(String::as_str) == Some(BUILD) {
            return;
        }
        let text = format!("[workriver] workriver was updated (it is now {BUILD}). Some things behave differently: run `workriver guide changes`, read it, then go on with what you were doing.");
        // Not reached now (not running, busy at a prompt): told at its next command instead.
        if fresh || self.send(session, &text).is_ok() {
            told.insert(session.to_string(), BUILD.to_string());
            if let Err(e) = serde_json::to_vec_pretty(&told).map_err(std::io::Error::other).and_then(|b| std::fs::write(&path, b)) {
                tracing::warn!("cannot write {}: {e}", path.display());
            }
        }
    }
}
