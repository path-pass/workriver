// Terminals and hosts behind one runtime. A method that takes a place goes to the side that
// owns it (`acp:` places are the hosts'). The reads identity, seats and targets depend on are
// the union of both sides and fail as a whole when either fails: a partial answer could hide
// a second place of a session, or send a caller in a hosted agent down the user's branch. A
// side that is not there (`None`: no terminal runtime runs) has no places. Only `agents`
// degrades: a terminal side that fails shows its agents as last seen, state unknown.
use std::sync::{Arc, Mutex};

use workriver_core::runtime::{AgentState, Change, Frames, HarnessState, Place, Root, Runtime, RuntimeAgent, RuntimeError};
use workriver_core::screens::{Key, Screen};

use super::acp::Hosts;

pub struct Composite {
    terminal: Arc<dyn Runtime>,
    hosts: Arc<Hosts>,
    /// The terminal side's agents as last read, for a read of it that fails.
    last: Mutex<Vec<RuntimeAgent>>,
}

impl Composite {
    pub fn new(terminal: Arc<dyn Runtime>, hosts: Arc<Hosts>) -> Composite {
        Composite { terminal, hosts, last: Mutex::default() }
    }

    fn hosted(place: &str) -> bool {
        place.starts_with("acp:")
    }

    /// Both sides' answers as one; the terminal side's `None` is no answer of its own. With
    /// no host known, the terminal side's answer is the whole answer, as it was before hosts.
    fn both<T>(&self, terminal: Result<Vec<T>, RuntimeError>, hosted: Result<Vec<T>, RuntimeError>) -> Result<Vec<T>, RuntimeError> {
        let mut all = match terminal {
            Ok(t) => t,
            Err(e) if self.hosts.none() => return Err(e),
            Err(RuntimeError::None) => vec![],
            Err(e) => return Err(e),
        };
        all.extend(hosted?);
        Ok(all)
    }
}

impl Runtime for Composite {
    fn agents(&self) -> Result<Vec<RuntimeAgent>, RuntimeError> {
        let hosted = self.hosts.agents()?;
        let terminal = match self.terminal.agents() {
            Ok(t) => {
                *self.last.lock().unwrap() = t.clone();
                t
            }
            Err(e) if self.hosts.none() => return Err(e),
            Err(RuntimeError::None) => vec![],
            Err(_) => self.last.lock().unwrap().iter().cloned().map(|a| RuntimeAgent { state: AgentState::Unknown, ..a }).collect(),
        };
        Ok(terminal.into_iter().chain(hosted).collect())
    }

    fn locate(&self, machine: Option<&str>, place: &str) -> Result<Place, RuntimeError> {
        if Self::hosted(place) { self.hosts.locate(machine, place) } else { self.terminal.locate(machine, place) }
    }

    fn focus(&self, session: &str) -> Result<(), RuntimeError> {
        match self.hosts.resolve(None, session) {
            Ok(_) => Err(RuntimeError::Unsupported),
            Err(_) => self.terminal.focus(session),
        }
    }

    fn send_at(&self, machine: Option<&str>, place: &str, session: &str, key: &str, text: &str) -> Result<(), RuntimeError> {
        if Self::hosted(place) { self.hosts.send_at(machine, place, session, key, text) } else { self.terminal.send_at(machine, place, session, key, text) }
    }

    fn start_temporary(&self, group: &str, harness: &str, name: &str, cwd: &str) -> Result<RuntimeAgent, RuntimeError> {
        self.terminal.start_temporary(group, harness, name, cwd)
    }

    fn close(&self, agent: &str) -> Result<(), RuntimeError> {
        self.terminal.close(agent)
    }

    fn processes(&self, machine: Option<&str>) -> Result<Vec<(RuntimeAgent, Vec<u32>)>, RuntimeError> {
        self.both(self.terminal.processes(machine), self.hosts.processes(machine))
    }

    fn roots(&self, machine: Option<&str>, place: Option<&str>) -> Result<Vec<Root>, RuntimeError> {
        match place {
            Some(p) if Self::hosted(p) => self.hosts.roots(machine, place),
            Some(_) => self.terminal.roots(machine, place),
            None => self.both(self.terminal.roots(machine, None), self.hosts.roots(machine, None)),
        }
    }

    fn agent_roots(&self, machine: Option<&str>) -> Result<Vec<Root>, RuntimeError> {
        self.both(self.terminal.agent_roots(machine), self.hosts.agent_roots(machine))
    }

    fn run_shell(&self, machine: Option<&str>, group: &str, dir: &str, line: &str) -> Result<String, RuntimeError> {
        self.terminal.run_shell(machine, group, dir, line)
    }

    fn shell_processes(&self, machine: Option<&str>, group: &str, place: &str) -> Result<(u32, Vec<u32>), RuntimeError> {
        self.terminal.shell_processes(machine, group, place)
    }

    fn close_shell(&self, machine: Option<&str>, group: &str, place: &str) -> Result<(), RuntimeError> {
        self.terminal.close_shell(machine, group, place)
    }

    fn start_temporary_at(&self, machine: Option<&str>, group: &str, harness: &str, name: &str, cwd: &str) -> Result<RuntimeAgent, RuntimeError> {
        self.terminal.start_temporary_at(machine, group, harness, name, cwd)
    }

    fn open_place(&self, machine: Option<&str>, group: &str, dir: &str) -> Result<String, RuntimeError> {
        self.terminal.open_place(machine, group, dir)
    }

    fn start_in(&self, machine: Option<&str>, place: &str, harness: &str, name: &str, args: &[String]) -> Result<(), RuntimeError> {
        if Self::hosted(place) { Err(RuntimeError::Unsupported) } else { self.terminal.start_in(machine, place, harness, name, args) }
    }

    fn has_session(&self, agent: &RuntimeAgent) -> bool {
        self.terminal.has_session(agent)
    }

    fn harnesses(&self, machine: Option<&str>) -> Result<Vec<HarnessState>, RuntimeError> {
        self.terminal.harnesses(machine)
    }

    fn read_place(&self, machine: Option<&str>, place: &str, lines: u32) -> Result<String, RuntimeError> {
        if Self::hosted(place) { Err(RuntimeError::Unsupported) } else { self.terminal.read_place(machine, place, lines) }
    }

    fn focus_place(&self, machine: Option<&str>, place: &str) -> Result<(), RuntimeError> {
        if Self::hosted(place) { Err(RuntimeError::Unsupported) } else { self.terminal.focus_place(machine, place) }
    }

    fn watch(&self, machine: Option<&str>, to: &mut dyn FnMut(Change) -> bool) -> Result<(), RuntimeError> {
        self.terminal.watch(machine, to)
    }

    /// A hosted session is at its host's place, never at a terminal that reports it.
    fn resolve(&self, machine: Option<&str>, session: &str) -> Result<String, RuntimeError> {
        match self.hosts.resolve(machine, session) {
            Ok(place) => Ok(place),
            Err(_) => self.terminal.resolve(machine, session),
        }
    }

    fn screen(&self, machine: Option<&str>, place: &str) -> Result<Screen, RuntimeError> {
        if Self::hosted(place) { Err(RuntimeError::Unsupported) } else { self.terminal.screen(machine, place) }
    }

    fn press(&self, machine: Option<&str>, place: &str, keys: &[Key]) -> Result<(), RuntimeError> {
        if Self::hosted(place) { Err(RuntimeError::Unsupported) } else { self.terminal.press(machine, place, keys) }
    }

    fn observe(&self, machine: Option<&str>, place: &str) -> Result<Frames, RuntimeError> {
        if Self::hosted(place) { Err(RuntimeError::Unsupported) } else { self.terminal.observe(machine, place) }
    }

    fn why_blocked(&self, machine: Option<&str>, place: &str) -> Result<Option<String>, RuntimeError> {
        if Self::hosted(place) { Err(RuntimeError::Unsupported) } else { self.terminal.why_blocked(machine, place) }
    }
}
