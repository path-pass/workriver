// What the runtime misses, the screen still shows: an agent the runtime calls idle or done
// whose screen shows a dialog that waits is "waiting on its screen", counted and held like a
// blocked one. Read when its state becomes idle or done (and a tick later), every tick while
// it is so marked, and before every message workriver types into any agent, at the place the
// message then goes to. An agent is its session on its machine: the same session resumed on
// two machines is two agents. Another machine's screens are read beside the poll, never in
// it: a slow machine holds nothing of this one back.
use std::collections::{HashMap, HashSet};

use workriver_core::runtime::{AgentState, RuntimeAgent, RuntimeError};
use workriver_core::screens;

use super::*;

/// In memory only, like `blocked` and `idle_since`: by agent (`agent_key`, its session on
/// its machine).
#[derive(Default)]
pub(in crate::server) struct Marks {
    /// The state the runtime gave at the last poll.
    last: HashMap<String, AgentState>,
    /// Reads still to make of an agent that just became idle or done.
    look: HashMap<String, u8>,
    /// Waiting on its screen, since then.
    pub(in crate::server) on: HashMap<String, String>,
    /// Another machine's agents whose screen is being read now, beside the poll.
    reading: HashSet<String>,
}

impl Marks {
    /// What a read of the agent's screen found, unless it is no longer idle or done (a poll
    /// since saw it change): marked when it shows a question, unmarked when it does not; as
    /// it was when it could not be read.
    fn found(&mut self, key: &str, shows: Result<bool, RuntimeError>) {
        if !matches!(self.last.get(key), Some(AgentState::Idle | AgentState::Done)) {
            return;
        }
        match shows {
            Ok(true) => {
                self.on.entry(key.to_string()).or_insert_with(workriver_core::now);
            }
            Ok(false) => {
                self.on.remove(key);
            }
            Err(RuntimeError::Unsupported) => {}
            Err(e) => tracing::debug!("cannot read the screen of {}: {e}", key.replace('\n', " ")),
        }
    }
}

/// Whether the agent's screen shows a question, read at the one place that shows it.
fn read_shows(runtime: &dyn workriver_core::runtime::Runtime, session: &str, machine: Option<&str>) -> Result<bool, RuntimeError> {
    runtime.resolve(machine, session).and_then(|p| runtime.screen(machine, &p)).map(|s| shows_a_question(&s.text))
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
    /// message is asked for nothing is typed: a read that fails then (the runtime does not
    /// answer, or its answer failed) is that error, and the message waits. Only a runtime that
    /// cannot resolve a session at all (`Unsupported`) types it as it always did.
    pub(in crate::server) fn type_in(&self, session: &str, machine: Option<&str>, text: &str) -> Result<(), RuntimeError> {
        let writer = self.terminals.writer(session, machine);
        let _one = writer.lock().unwrap();
        let untyped = |e: RuntimeError| match e {
            RuntimeError::Failed(why) => RuntimeError::Unreachable(why),
            e => e,
        };
        let place = match self.runtime.resolve(machine, session) {
            Err(RuntimeError::Unsupported) => return self.runtime.send_to(machine, session, text),
            r => r.map_err(untyped)?,
        };
        match self.runtime.screen(machine, &place) {
            Ok(s) if shows_a_question(&s.text) => {
                self.terminals.marks.lock().unwrap().on.entry(agent_key(session, machine)).or_insert_with(workriver_core::now);
                return Err(RuntimeError::Blocked(format!("{session} shows a question on its screen and takes no message until it is answered")));
            }
            Ok(_) | Err(RuntimeError::Unsupported) => {}
            Err(e) => return Err(untyped(e)),
        }
        self.runtime.send_at(machine, &place, text)
    }

    /// At each poll, before anything is worked out from the agents: reads the screens of the
    /// agents that just became idle or done (or are first seen so) and of those marked
    /// waiting on their screen, and shows a marked one as blocked. One listed in any other
    /// state is not marked; nor one whose dialog is gone. This machine's screens are read
    /// here; another machine's on a thread of their own, one read per agent at a time, whose
    /// answer is kept when it comes and wakes the poll to show it.
    pub(in crate::server) fn look_at_screens(&self, now: &mut [RuntimeAgent]) {
        let mut here = vec![];
        {
            let mut m = self.terminals.marks.lock().unwrap();
            let listed: HashSet<String> = now.iter().map(|a| agent_key(&a.session, a.machine.as_deref())).collect();
            m.last.retain(|k, _| listed.contains(k));
            m.look.retain(|k, _| listed.contains(k));
            m.on.retain(|k, _| listed.contains(k));
            for a in now.iter() {
                let key = agent_key(&a.session, a.machine.as_deref());
                let was = m.last.insert(key.clone(), a.state);
                if !matches!(a.state, AgentState::Idle | AgentState::Done) {
                    m.look.remove(&key);
                    m.on.remove(&key);
                    continue;
                }
                // Once now, once a tick later: a dialog may be drawn after the state changed.
                if was != Some(a.state) {
                    m.look.insert(key.clone(), 2);
                }
                let left = m.look.get(&key).copied().unwrap_or(0);
                if !(left > 0 || m.on.contains_key(&key)) {
                    continue;
                }
                match a.machine.clone() {
                    None => here.push((key.clone(), a.session.clone())),
                    // Still being read: this look waits for that read.
                    Some(_) if m.reading.contains(&key) => continue,
                    Some(machine) => {
                        m.reading.insert(key.clone());
                        let (runtime, marks, wake, session) = (self.runtime.clone(), self.terminals.marks.clone(), self.wake.clone(), a.session.clone());
                        let k = key.clone();
                        std::thread::spawn(move || {
                            let shows = read_shows(runtime.as_ref(), &session, Some(&machine));
                            let mut m = marks.lock().unwrap();
                            m.reading.remove(&k);
                            m.found(&k, shows);
                            drop(m);
                            wake.notify_one();
                        });
                    }
                }
                if left > 0 {
                    m.look.insert(key, left - 1);
                }
            }
        }
        for (key, session) in here {
            let shows = read_shows(self.runtime.as_ref(), &session, None);
            self.terminals.marks.lock().unwrap().found(&key, shows);
        }
        let m = self.terminals.marks.lock().unwrap();
        for a in now.iter_mut().filter(|a| m.on.contains_key(&agent_key(&a.session, a.machine.as_deref()))) {
            a.state = AgentState::Blocked;
        }
    }

    /// The agents waiting on their screen now, by `agent_key`.
    pub(in crate::server) fn on_screen(&self) -> HashSet<String> {
        self.terminals.marks.lock().unwrap().on.keys().cloned().collect()
    }
}
