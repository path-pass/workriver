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
        if Self::hosted(&self.resolve(None, session)?) {
            return Err(RuntimeError::Unsupported);
        }
        self.terminal.focus(session)
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

    /// A session must be found once across both runtimes. A failed read cannot rule out a
    /// second place.
    fn resolve(&self, machine: Option<&str>, session: &str) -> Result<String, RuntimeError> {
        let terminal = self.terminal.resolve(machine, session);
        match self.hosts.resolve(machine, session) {
            Ok(place) => match terminal {
                Ok(other) => Err(RuntimeError::Ambiguous(format!("{session} shows in {other} and {place}"))),
                Err(RuntimeError::None | RuntimeError::NoAgent(_)) => Ok(place),
                Err(e) => Err(e),
            },
            Err(RuntimeError::None | RuntimeError::NoAgent(_)) => terminal,
            Err(e) => Err(e),
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use workriver_core::runtime::NoRuntime;

    use super::*;
    use crate::runtime::Published;

    /// A terminal side whose reads answer as set: its agents, or an error for every read.
    struct Terminal(Mutex<Result<Vec<(String, RuntimeAgent, u32)>, String>>);

    fn agent(session: &str, state: AgentState) -> RuntimeAgent {
        RuntimeAgent { session: session.into(), harness: "codex".into(), state, cwd: "/p".into(), group: None, name: session.into(), topic: None, machine: None }
    }

    impl Terminal {
        fn read(&self) -> Result<Vec<(String, RuntimeAgent, u32)>, RuntimeError> {
            self.0.lock().unwrap().clone().map_err(RuntimeError::Unreachable)
        }
    }

    impl Runtime for Terminal {
        fn agents(&self) -> Result<Vec<RuntimeAgent>, RuntimeError> {
            Ok(self.read()?.into_iter().map(|(_, a, _)| a).collect())
        }
        fn locate(&self, _: Option<&str>, _: &str) -> Result<Place, RuntimeError> {
            Ok(Place::default())
        }
        fn focus(&self, _: &str) -> Result<(), RuntimeError> {
            Ok(())
        }
        fn start_temporary(&self, _: &str, _: &str, _: &str, _: &str) -> Result<RuntimeAgent, RuntimeError> {
            Err(RuntimeError::Unsupported)
        }
        fn close(&self, _: &str) -> Result<(), RuntimeError> {
            Ok(())
        }
        fn roots(&self, _: Option<&str>, _: Option<&str>) -> Result<Vec<Root>, RuntimeError> {
            Ok(self.read()?.into_iter().map(|(place, a, shell)| Root { place, shell: Some(shell), front: vec![], at: Place { agent: Some(a), starting: false } }).collect())
        }
        fn processes(&self, _: Option<&str>) -> Result<Vec<(RuntimeAgent, Vec<u32>)>, RuntimeError> {
            Ok(self.read()?.into_iter().map(|(_, a, shell)| (a, vec![shell])).collect())
        }
        fn resolve(&self, _: Option<&str>, session: &str) -> Result<String, RuntimeError> {
            self.read()?.into_iter().find(|(_, a, _)| a.session == session).map(|(p, ..)| p).ok_or_else(|| RuntimeError::NoAgent(session.into()))
        }
    }

    /// Hosts with one instance whose root is this test process: published as session `s-h`.
    fn hosts(dir: &std::path::Path, published: bool) -> Arc<Hosts> {
        let hosts = Hosts::new(dir);
        let me = crate::common::process::one(std::process::id()).unwrap();
        hosts.track("a1", me.pid, me.start);
        if published {
            hosts.publish("a1", Published { session: "s-h".into(), name: "w".into(), harness: "codex".into(), cwd: "/p".into(), generation: 1 });
        }
        hosts
    }

    #[test]
    fn identity_reads_are_the_union_and_fail_as_a_whole() {
        let dir = tempfile::tempdir().unwrap();
        // No terminal runtime and no host: as before hosts, the terminal side's own answer.
        let bare = Composite::new(Arc::new(NoRuntime), Hosts::new(dir.path()));
        assert!(matches!(bare.roots(None, None), Err(RuntimeError::None)));
        // No terminal runtime, a host: its root only.
        let alone = Composite::new(Arc::new(NoRuntime), hosts(dir.path(), true));
        let roots = alone.roots(None, None).unwrap();
        assert_eq!(roots.iter().map(|r| (r.place.as_str(), r.shell)).collect::<Vec<_>>(), [("acp:a1/1", Some(std::process::id()))]);
        assert_eq!(alone.agent_roots(None).unwrap().len(), 1);
        // A terminal side that answers: both; one that fails: every identity read fails.
        let terminal = Arc::new(Terminal(Mutex::new(Ok(vec![("w1:p1".into(), agent("s-t", AgentState::Idle), 1)]))));
        let both = Composite::new(terminal.clone(), hosts(dir.path(), true));
        assert_eq!(both.roots(None, None).unwrap().len(), 2);
        both.agents().unwrap();
        *terminal.0.lock().unwrap() = Err("does not answer".into());
        assert!(matches!(both.roots(None, None), Err(RuntimeError::Unreachable(_))));
        assert!(matches!(both.agent_roots(None), Err(RuntimeError::Unreachable(_))));
        assert!(matches!(both.processes(None), Err(RuntimeError::Unreachable(_))));
        // The place named goes to its own side alone.
        assert_eq!(both.roots(None, Some("acp:a1/1")).unwrap().len(), 1);
        // Only the list degrades: the terminal side's agents as last seen, state unknown.
        let listed = both.agents().unwrap();
        let t = listed.iter().find(|a| a.session == "s-t").unwrap();
        assert_eq!(t.state, AgentState::Unknown);
        assert!(listed.iter().any(|a| a.session == "s-h"));
    }

    #[test]
    fn session_lookups_require_both_runtimes_to_agree_on_one_place() {
        let dir = tempfile::tempdir().unwrap();
        let terminal = Arc::new(Terminal(Mutex::new(Ok(vec![("w1:p1".into(), agent("s-h", AgentState::Idle), 1)]))));
        let both = Composite::new(terminal.clone(), hosts(dir.path(), true));
        assert!(matches!(both.resolve(None, "s-h"), Err(RuntimeError::Ambiguous(_))));
        assert!(matches!(both.focus("s-h"), Err(RuntimeError::Ambiguous(_))));
        *terminal.0.lock().unwrap() = Err("does not answer".into());
        assert!(matches!(both.resolve(None, "s-h"), Err(RuntimeError::Unreachable(_))));
        assert!(matches!(both.focus("s-h"), Err(RuntimeError::Unreachable(_))));
        *terminal.0.lock().unwrap() = Ok(vec![("w1:p1".into(), agent("s-t", AgentState::Idle), 1)]);
        assert_eq!(both.resolve(None, "s-t").unwrap(), "w1:p1");
        both.focus("s-t").unwrap();
        assert_eq!(both.resolve(None, "s-h").unwrap(), "acp:a1/1");
        assert!(matches!(both.focus("s-h"), Err(RuntimeError::Unsupported)));
        assert!(matches!(both.screen(None, "acp:a1/1"), Err(RuntimeError::Unsupported)));
    }

    #[test]
    fn a_host_not_published_is_starting() {
        let dir = tempfile::tempdir().unwrap();
        let starting = Composite::new(Arc::new(NoRuntime), hosts(dir.path(), false));
        let roots = starting.roots(None, None).unwrap();
        assert!(roots[0].at.starting && roots[0].at.agent.is_none(), "{roots:?}");
        assert!(starting.agent_roots(None).unwrap().is_empty());
    }
}
