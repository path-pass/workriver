// What the runtime misses, the screen still shows: an agent the runtime calls idle or done
// whose screen shows a dialog that waits is "waiting on its screen", counted and held like a
// blocked one. Read when its state becomes idle or done (and again at the first poll half a
// tick or more after that read began, however many polls came between), every poll while it
// is so marked, and before every message workriver types into any agent, at the place the
// message then goes to. A seated manager's screen is read where its seat is held to, and a
// lost seat's is not read at all. An agent is its session on its machine: the same session resumed on
// two machines is two agents. Another machine's screens are read beside the poll, never in
// it: a slow machine holds nothing of this one back.
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use workriver_core::runtime::{AgentState, RuntimeAgent, RuntimeError};
use workriver_core::screens;

use super::*;

/// In memory only, like `blocked` and `idle_since`: by agent (`agent_key`, its session on
/// its machine).
#[derive(Default)]
pub(in crate::server) struct Marks {
    /// The state the runtime gave at the last poll.
    last: HashMap<String, AgentState>,
    /// The read still to make of an agent that just became idle or done, from then on.
    again: HashMap<String, Instant>,
    /// Waiting on its screen, since then.
    pub(in crate::server) on: HashMap<String, String>,
    /// Another machine's agents whose screen is being read now, beside the poll.
    reading: HashSet<String>,
}

impl Marks {
    /// What a read of the agent's screen found, unless it is no longer idle or done (a poll
    /// since saw it change): marked when it shows a question, unmarked when it does not; as
    /// it was when it could not be read. Whether the mark changed.
    fn found(&mut self, key: &str, shows: Result<bool, RuntimeError>) -> bool {
        if !matches!(self.last.get(key), Some(AgentState::Idle | AgentState::Done)) {
            return false;
        }
        match shows {
            Ok(true) if self.on.contains_key(key) => false,
            Ok(true) => {
                self.on.insert(key.to_string(), workriver_core::now());
                true
            }
            Ok(false) => self.on.remove(key).is_some(),
            Err(RuntimeError::Unsupported) => false,
            Err(e) => {
                tracing::debug!("cannot read the screen of {}: {e}", key.replace('\n', " "));
                false
            }
        }
    }
}

/// Whether the agent's screen shows a question, read at its place: a seated manager's as
/// the poll found it (`at`, `App::seat_place`), whose lost seat shows none; any other's, the
/// one place that shows it.
fn read_shows(runtime: &dyn workriver_core::runtime::Runtime, session: &str, machine: Option<&str>, at: Option<Result<String, RuntimeError>>) -> Result<bool, RuntimeError> {
    let place = match at {
        None => runtime.resolve(machine, session)?,
        Some(Err(RuntimeError::NoAgent(_))) => return Ok(false),
        Some(at) => at?,
    };
    runtime.screen(machine, &place).map(|s| shows_a_question(&s.text))
}

/// Whether a screen shows a dialog that waits: options, and a key named under them.
fn shows_a_question(text: &str) -> bool {
    screens::parse(text).is_some_and(|d| d.waiting())
}

impl App {
    /// Types a message into the agent with this session on `machine` (none: this one): its
    /// place found once (`place_of`: a seated manager's is the place its seat is held to,
    /// read now, and nowhere else), its screen read there, and the message typed at that
    /// same place only while it shows the session (`Runtime::send_at`); a screen that shows a
    /// question takes no message (`Blocked`; the agent is marked waiting on its screen). Until
    /// the message is asked for nothing is typed: a read that fails then (the runtime does not
    /// answer, or its answer failed; or it cannot resolve a session at all) is that error,
    /// and the message waits. Files as the agent's machine can get them (`readable_on`). The
    /// place is found before the agent's writer is taken; under the writer, the read and the
    /// message, so a press never comes between them. A message's send holds
    /// `App::appointing` (or `App::discussing`) around this (`App::hand`): the order is always
    /// that, then the writer, never the other way. `key`: the message's own (`Held.key`), the
    /// same at every try, so a runtime that keeps keys takes it once.
    pub(in crate::server) fn type_at(&self, session: &str, machine: Option<&str>, key: &str, text: &str) -> Result<(), RuntimeError> {
        let place = self.place_of(session, machine).map_err(untyped)?;
        let writer = self.terminals.writer(session, machine);
        let _one = writer.lock().unwrap();
        match self.runtime.screen(machine, &place) {
            Ok(s) if shows_a_question(&s.text) => {
                self.terminals.marks.lock().unwrap().on.entry(agent_key(session, machine)).or_insert_with(workriver_core::now);
                return Err(RuntimeError::Blocked(format!("{session} shows a question on its screen and takes no message until it is answered")));
            }
            Ok(_) | Err(RuntimeError::Unsupported) => {}
            Err(e) => return Err(untyped(e)),
        }
        self.runtime.send_at(machine, &place, session, key, &self.readable_on(machine, text))
    }

    /// At each poll, before anything is worked out from the agents: reads the screens of the
    /// agents that just became idle or done (or are first seen so) and of those marked
    /// waiting on their screen, and shows a marked one as blocked. One listed in any other
    /// state is not marked; nor one whose dialog is gone. The second read of one that just
    /// became idle or done is made at the first poll half a tick or more after the first: a
    /// poll woken early (a change told, a read's answer) does not use it up. This machine's
    /// screens are read here; another machine's on a thread of their own, one read per agent
    /// at a time, whose answer is kept when it comes and wakes the poll only when it changed
    /// the mark.
    pub(in crate::server) fn look_at_screens(&self, now: &mut [RuntimeAgent]) {
        let mut here = vec![];
        // A seated manager is read where its seat is held to, as this tick's read found;
        // worked out before the marks are taken.
        let mut seated: HashMap<String, Result<String, RuntimeError>> = now.iter().filter_map(|a| Some((agent_key(&a.session, a.machine.as_deref()), self.seat_place(&a.session, a.machine.as_deref())?))).collect();
        {
            let mut m = self.terminals.marks.lock().unwrap();
            let listed: HashSet<String> = now.iter().map(|a| agent_key(&a.session, a.machine.as_deref())).collect();
            m.last.retain(|k, _| listed.contains(k));
            m.again.retain(|k, _| listed.contains(k));
            m.on.retain(|k, _| listed.contains(k));
            for a in now.iter() {
                let key = agent_key(&a.session, a.machine.as_deref());
                let was = m.last.insert(key.clone(), a.state);
                if !matches!(a.state, AgentState::Idle | AgentState::Done) {
                    m.again.remove(&key);
                    m.on.remove(&key);
                    continue;
                }
                // Once now, once again at a poll half a tick or more later (however many polls
                // came between): a dialog may be drawn after the state changed.
                let first = was != Some(a.state);
                if first {
                    m.again.insert(key.clone(), Instant::now() + self.waits.tick / 2);
                }
                let again = !first && m.again.get(&key).is_some_and(|at| *at <= Instant::now());
                if !(first || again || m.on.contains_key(&key)) {
                    continue;
                }
                let at = seated.remove(&key);
                match a.machine.clone() {
                    None => here.push((key.clone(), a.session.clone(), at)),
                    // Still being read: this look waits for that read.
                    Some(_) if m.reading.contains(&key) => continue,
                    Some(machine) => {
                        m.reading.insert(key.clone());
                        let (runtime, marks, wake, session) = (self.runtime.clone(), self.terminals.marks.clone(), self.wake.clone(), a.session.clone());
                        let k = key.clone();
                        std::thread::spawn(move || {
                            let shows = read_shows(runtime.as_ref(), &session, Some(&machine), at);
                            let mut m = marks.lock().unwrap();
                            m.reading.remove(&k);
                            let changed = m.found(&k, shows);
                            drop(m);
                            if changed {
                                wake.notify_one();
                            }
                        });
                    }
                }
                if again {
                    m.again.remove(&key);
                }
            }
        }
        for (key, session, at) in here {
            let shows = read_shows(self.runtime.as_ref(), &session, None, at);
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
