// What the runtime misses, the screen still shows: an agent the runtime calls idle or done
// whose screen shows a dialog that waits is "waiting on its screen", counted and held like a
// blocked one. Read when its state becomes idle or done (and a tick later), every tick while
// it is so marked, and before every message workriver types into any agent, at the place the
// message then goes to.
use std::collections::{HashMap, HashSet};

use workriver_core::runtime::{AgentState, RuntimeAgent, RuntimeError};
use workriver_core::screens;

use super::*;

/// In memory only, like `blocked` and `idle_since`: by session.
#[derive(Default)]
pub(in crate::server) struct Marks {
    /// The state the runtime gave at the last poll.
    last: HashMap<String, AgentState>,
    /// Reads still to make of an agent that just became idle or done.
    look: HashMap<String, u8>,
    /// Waiting on its screen, since then.
    pub(in crate::server) on: HashMap<String, String>,
}

/// Whether a screen shows a dialog that waits: options, and a key named under them.
fn shows_a_question(text: &str) -> bool {
    screens::parse(text).is_some_and(|d| d.waiting())
}

impl App {
    /// Types a message into the agent with this session on `machine` (none: this one): its
    /// place resolved once, its screen read there, and the message typed at that same place;
    /// a screen that shows a question takes no message (`Blocked`; the agent is marked waiting
    /// on its screen). Under the agent's writer, so a press never comes between. Until the
    /// message is asked for nothing is typed: a read that fails then is `Unreachable`. A
    /// runtime that cannot resolve a session types it as it always did.
    pub(in crate::server) fn type_in(&self, session: &str, machine: Option<&str>, text: &str) -> Result<(), RuntimeError> {
        let writer = self.terminals.writer(session, machine);
        let _one = writer.lock().unwrap();
        let untyped = |e: RuntimeError| match e {
            RuntimeError::Failed(why) => RuntimeError::Unreachable(why),
            e => e,
        };
        let place = match self.runtime.resolve(machine, session) {
            Err(RuntimeError::None) => return self.runtime.send_to(machine, session, text),
            r => r.map_err(untyped)?,
        };
        match self.runtime.screen(machine, &place) {
            Ok(s) if shows_a_question(&s.text) => {
                self.terminals.marks.lock().unwrap().on.entry(session.to_string()).or_insert_with(workriver_core::now);
                return Err(RuntimeError::Blocked(format!("{session} shows a question on its screen and takes no message until it is answered")));
            }
            Ok(_) | Err(RuntimeError::None) => {}
            Err(e) => return Err(untyped(e)),
        }
        self.runtime.send_at(machine, &place, text)
    }

    /// At each poll, before anything is worked out from the agents: reads the screens of the
    /// agents that just became idle or done (or are first seen so) and of those marked
    /// waiting on their screen, and shows a marked one as blocked. One listed in any other
    /// state is not marked; nor one whose dialog is gone.
    pub(in crate::server) fn look_at_screens(&self, now: &mut [RuntimeAgent]) {
        let mut read = vec![];
        {
            let mut m = self.terminals.marks.lock().unwrap();
            let listed: HashSet<String> = now.iter().map(|a| a.session.clone()).collect();
            m.last.retain(|s, _| listed.contains(s));
            m.look.retain(|s, _| listed.contains(s));
            m.on.retain(|s, _| listed.contains(s));
            for a in now.iter() {
                let was = m.last.insert(a.session.clone(), a.state);
                if !matches!(a.state, AgentState::Idle | AgentState::Done) {
                    m.look.remove(&a.session);
                    m.on.remove(&a.session);
                    continue;
                }
                // Once now, once a tick later: a dialog may be drawn after the state changed.
                if was != Some(a.state) {
                    m.look.insert(a.session.clone(), 2);
                }
                let left = m.look.get(&a.session).copied().unwrap_or(0);
                if left > 0 {
                    m.look.insert(a.session.clone(), left - 1);
                }
                if left > 0 || m.on.contains_key(&a.session) {
                    read.push((a.session.clone(), a.machine.clone()));
                }
            }
        }
        for (session, machine) in read {
            let machine = machine.as_deref();
            let shows = self.runtime.resolve(machine, &session).and_then(|p| self.runtime.screen(machine, &p)).map(|s| shows_a_question(&s.text));
            let mut m = self.terminals.marks.lock().unwrap();
            match shows {
                Ok(true) => {
                    m.on.entry(session).or_insert_with(workriver_core::now);
                }
                Ok(false) => {
                    m.on.remove(&session);
                }
                // Not read: as it was.
                Err(RuntimeError::None) => {}
                Err(e) => tracing::debug!("cannot read the screen of {session}: {e}"),
            }
        }
        let m = self.terminals.marks.lock().unwrap();
        for a in now.iter_mut().filter(|a| m.on.contains_key(&a.session)) {
            a.state = AgentState::Blocked;
        }
    }

    /// The agents waiting on their screen now, by session.
    pub(in crate::server) fn on_screen(&self) -> HashSet<String> {
        self.terminals.marks.lock().unwrap().on.keys().cloned().collect()
    }
}
