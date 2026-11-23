// What the service's tests share: a stand-in runtime and its kernel, daemons on a free port
// with a temporary database, git and HTTP helpers.
use std::io::{Read, Write};
use std::process::Command;

use serde_json::json;

use crate::cli::Daemon;

pub(super) fn git(dir: &std::path::Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "agent-a")
        .env("GIT_AUTHOR_EMAIL", "a@example.invalid")
        .env("GIT_COMMITTER_NAME", "agent-a")
        .env("GIT_COMMITTER_EMAIL", "a@example.invalid")
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// Process ids from here up are places' roots, by the name of their place.
const ROOTS: u32 = 1_000_000;

/// The shell at the root of `place`, in every test's runtime.
pub(super) fn root(place: &str) -> u32 {
    ROOTS + place.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)) % 1_000_000
}

fn shell(pid: u32) -> crate::common::process::Proc {
    crate::common::process::Proc { pid, parent: 1, tty: Some(format!("tty{pid}")), start: 1, name: "zsh".into() }
}

/// Processes as tests make them up: a chain per process id; any other id from `ROOTS` up
/// is a root (a place's shell) that runs until it is killed.
#[derive(Default)]
pub(super) struct FakeProcs {
    chains: std::sync::Mutex<std::collections::HashMap<u32, crate::common::process::Chain>>,
    dead: std::sync::Mutex<Vec<u32>>,
}

impl FakeProcs {
    pub(super) fn set(&self, pid: u32, chain: crate::common::process::Chain) {
        self.chains.lock().unwrap().insert(pid, chain);
    }
    pub(super) fn kill(&self, pid: u32) {
        self.dead.lock().unwrap().push(pid);
    }
}

impl crate::common::process::Procs for FakeProcs {
    fn chain(&self, _: Option<&workriver_scan::Runner>, pid: u32) -> Result<crate::common::process::Chain, String> {
        if self.dead.lock().unwrap().contains(&pid) {
            return Ok(vec![]);
        }
        if let Some(c) = self.chains.lock().unwrap().get(&pid) {
            return Ok(c.clone());
        }
        Ok(if pid >= ROOTS { vec![shell(pid)] } else { vec![] })
    }
}

/// What a test's service believes of processes and the proofs it gives: handed to it in its
/// `Options` (`options`). The test's CLI clients are made-up `workriver` processes at the
/// places they name.
#[derive(Clone, Default)]
pub(super) struct Kernel {
    pub(super) callers: std::sync::Arc<crate::server::access::caller::Callers>,
    pub(super) procs: std::sync::Arc<FakeProcs>,
    /// How many proofs the clients fetched.
    pub(super) fetched: std::sync::Arc<std::sync::atomic::AtomicU32>,
    /// The proof and process id last given to a client at each place.
    kept: std::sync::Arc<std::sync::Mutex<Minted>>,
}

/// A proof and a process id, by the place they were made up at.
type Minted = std::collections::HashMap<Option<String>, (String, u32)>;

impl Kernel {
    /// A service's options with this kernel, nothing else asked for.
    pub(super) fn options(&self) -> crate::server::Options {
        crate::server::Options { warning: None, waits: Default::default(), machines: None, allow_as: false, sample: false, procs: self.procs.clone(), callers: self.callers.clone(), user_home: None, jobs: vec![] }
    }

    /// The CLI's client as the user's: with the user token of the daemon's home (made here if
    /// the daemon has not started yet), at no place in the runtime.
    pub(super) fn client(&self, port: u16, home: &std::path::Path) -> Daemon {
        crate::server::access::auth::Auth::open(home).unwrap();
        self.daemon(port, None, Some(home.to_path_buf()))
    }

    /// A client of the service on `port` (its own listener, or a machine's), at `place`.
    pub(super) fn daemon(&self, port: u16, place: Option<String>, home: Option<std::path::PathBuf>) -> Daemon {
        let k = self.clone();
        Daemon { port, place, home, prove: std::sync::Arc::new(move |d: &Daemon, fresh| k.mint(d.place.as_deref(), fresh)) }
    }

    /// A made-up `workriver` process at `place` (a child of its root's shell; none: at a
    /// terminal in no place): its proof, for this machine's listener, and its id, for a
    /// machine's. The same one for a place until `fresh`.
    fn mint(&self, place: Option<&str>, fresh: bool) -> (Option<String>, u32) {
        use std::sync::atomic::Ordering;
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1000);
        let mut kept = self.kept.lock().unwrap();
        let key = place.map(str::to_string);
        if fresh || !kept.contains_key(&key) {
            let pid = NEXT.fetch_add(1, Ordering::Relaxed);
            let me = |parent: u32, tty: String| crate::common::process::Proc { pid, parent, tty: Some(tty), start: 5, name: "workriver".into() };
            let chain = match place {
                Some(p) => vec![me(root(p), format!("tty{}", root(p))), shell(root(p))],
                None => vec![me(1, "ttys000".into())],
            };
            self.procs.set(pid, chain.clone());
            self.fetched.fetch_add(1, Ordering::Relaxed);
            kept.insert(key.clone(), (self.proof(chain), pid));
        }
        let (proof, pid) = &kept[&key];
        (Some(proof.clone()), *pid)
    }

    /// A proof for a process with this chain, as the caller socket would give it.
    pub(super) fn proof(&self, chain: crate::common::process::Chain) -> String {
        self.callers.admit(chain).unwrap()
    }
}

/// A line typed into a new shell: machine, group, dir, line.
type Typed = (Option<String>, String, String, String);

/// A harness started in a place: place, harness, name, args.
type StartedIn = (String, String, String, Vec<String>);

/// A runtime of our own: agents by place, every send kept, states set by the test.
#[derive(Default)]
pub(in crate::server) struct Fake {
    pub(super) agents: std::sync::Mutex<Vec<(String, workriver_core::runtime::RuntimeAgent)>>,
    pub(super) focused: std::sync::Mutex<Vec<String>>,
    pub(super) sent: std::sync::Mutex<Vec<(String, String)>>,
    /// Sends typed at a place (`send_at`), also kept in `sent`: (place, text).
    pub(super) typed_at: std::sync::Mutex<Vec<(String, String)>>,
    /// Temporary agents started: (group, harness, name, cwd); and those closed.
    pub(super) started: std::sync::Mutex<Vec<(String, String, String, String)>>,
    pub(super) closed: std::sync::Mutex<Vec<String>>,
    /// As Herdr does: a message to an agent it does not have is an error.
    pub(super) strict: std::sync::atomic::AtomicBool,
    /// The state a temporary agent starts in (Idle unless set), e.g. Blocked on a first-run prompt.
    pub(super) start_as: std::sync::Mutex<Option<workriver_core::runtime::AgentState>>,
    /// As Herdr may just after a start: this many prompts are refused as not ready yet.
    pub(super) not_ready: std::sync::atomic::AtomicUsize,
    /// Herdr does not answer: reading the agents and typing into one fail.
    pub(super) down: std::sync::atomic::AtomicBool,
    /// The agents are listed, but typing into one fails as when Herdr's socket drops before
    /// the prompt is written: nothing is typed.
    pub(super) prompt_down: std::sync::atomic::AtomicBool,
    /// Lines typed into new shells: (machine, group, dir, line); what each place shows, by place.
    pub(super) shells: std::sync::Mutex<Vec<Typed>>,
    pub(super) screens: std::sync::Mutex<std::collections::HashMap<String, String>>,
    /// Places closed, in order.
    pub(super) closed_places: std::sync::Mutex<Vec<String>>,
    /// What runs at a run's place: (its shell, what is in front); unset: a shell at its prompt.
    pub(super) procs: std::sync::Mutex<std::collections::HashMap<String, (u32, Vec<u32>)>>,
    /// Places asked what runs there, in order; and those that are no longer a run's shell.
    pub(super) looked: std::sync::Mutex<Vec<String>>,
    pub(super) not_shells: std::sync::Mutex<Vec<String>>,
    /// This many closes are refused first, as when the runtime does not answer; those tried.
    pub(super) close_fails: std::sync::atomic::AtomicUsize,
    pub(super) close_tried: std::sync::Mutex<Vec<String>>,
    /// Plain panes (no agent) that are there; panes that are gone; places with no known shell.
    /// A place a caller names that is in none of the lists is a plain pane that is there.
    pub(super) plain: std::sync::Mutex<Vec<String>>,
    pub(super) gone: std::sync::Mutex<Vec<String>>,
    pub(super) no_shell: std::sync::Mutex<Vec<String>>,
    /// Which places hold an agent cannot be asked (the runtime errs), the rest can.
    pub(super) agent_roots_down: std::sync::atomic::AtomicBool,
    /// Called with the receiver as a message is taken, before `send` returns: what a test
    /// wants to look at at that very moment.
    pub(super) on_send: std::sync::Mutex<Option<Look>>,
    /// Called as the agents are listed, before the list is read: a test pauses the service
    /// there.
    pub(super) on_agents: std::sync::Mutex<Option<std::sync::Arc<dyn Fn() + Send + Sync>>>,
    /// What each agent's place shows now (its visible text), by place; none: an empty screen.
    pub(super) shown: std::sync::Mutex<std::collections::HashMap<String, String>>,
    /// What each press at a place brings, in order: the screen then, and its agent's state then.
    pub(super) then: std::sync::Mutex<std::collections::HashMap<String, std::collections::VecDeque<Next>>>,
    /// Keys pressed, one entry per press, with the place.
    pub(super) pressed: std::sync::Mutex<Vec<(String, Vec<workriver_core::screens::Key>)>>,
    /// Pressing fails as when the runtime's answer is lost: the keys may have gone.
    pub(super) press_fails: std::sync::atomic::AtomicBool,
    /// Screens read, by place, in order.
    pub(super) reads: std::sync::Mutex<Vec<String>>,
    /// Other places that show a session too, (place, session): as a resumed session does.
    pub(super) also: std::sync::Mutex<Vec<(String, String)>>,
    /// Called with the place as a screen is read, before it is given.
    pub(super) on_read: std::sync::Mutex<Option<Look>>,
    /// The changes `watch` tells, as the test sends them; how often `watch` was called.
    pub(super) changes: std::sync::Mutex<Option<std::sync::mpsc::Receiver<workriver_core::runtime::Change>>>,
    pub(super) watched: std::sync::atomic::AtomicUsize,
    /// Why the runtime calls an agent blocked.
    pub(super) why: std::sync::Mutex<Option<String>>,
    /// The frames a view gives, then nothing but quiet; how many views are open now.
    pub(super) frames: std::sync::Mutex<Vec<workriver_core::screens::Frame>>,
    pub(super) observing: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// As Herdr's socket that does not answer, for one call only: resolving a session, reading
    /// a screen, reading what is at a place. Everything else answers.
    pub(super) resolve_down: std::sync::atomic::AtomicBool,
    pub(super) screen_down: std::sync::atomic::AtomicBool,
    pub(super) locate_down: std::sync::atomic::AtomicBool,
    /// Called with the place as the runtime is asked why it calls an agent blocked.
    pub(super) on_why: std::sync::Mutex<Option<Look>>,
    /// Places opened for a manager's start or a try: (group, dir, place), in order.
    pub(super) opened: std::sync::Mutex<Vec<(String, String, String)>>,
    /// Harnesses started in a place: (place, harness, name, args), in order.
    pub(super) started_in: std::sync::Mutex<Vec<StartedIn>>,
    /// What a start in a place brings up (by default an agent with a session of its own).
    pub(super) bring: std::sync::Mutex<Bring>,
    /// The harnesses this runtime has; none set: claude and codex, installed and integrated.
    pub(super) harness_states: std::sync::Mutex<Option<Vec<workriver_core::runtime::HarnessState>>>,
    /// What a place shows once a harness was started in it (none: nothing).
    pub(super) start_screen: std::sync::Mutex<Option<String>>,
    /// This many reads of what is at one named place fail, as when the runtime cannot read a
    /// pane's processes for a moment.
    pub(super) place_fails: std::sync::atomic::AtomicUsize,
}

/// What a start in a place brings up.
#[derive(Default, Clone)]
pub(super) enum Bring {
    /// An agent idle at the place, with its own session `s-<name>`, started in its folder.
    #[default]
    Session,
    /// An agent blocked at the place with no session yet (it goes by `pending:<place>`); the
    /// start is answered not ready. The test gives it a session (`give_session`).
    Pending,
    /// Nothing shows up; the start is answered as if it went well.
    Nothing,
    /// An agent with its own session `s-<name>` that is still launching (not ready); the test
    /// makes it ready.
    Launching,
    /// The runtime refuses the start, in these words.
    Refuse(String),
}

impl Fake {
    /// The agent that has no session yet at `place` gets `session`, and is idle.
    pub(super) fn give_session(&self, place: &str, session: &str) {
        for (_, a) in self.agents.lock().unwrap().iter_mut().filter(|(p, _)| p == place) {
            a.session = session.into();
            a.state = workriver_core::runtime::AgentState::Idle;
        }
    }
}

/// An open view of the fake runtime: counted while it lives.
struct Viewing {
    frames: std::collections::VecDeque<workriver_core::screens::Frame>,
    open: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Iterator for Viewing {
    type Item = Result<Option<workriver_core::screens::Frame>, workriver_core::runtime::RuntimeError>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.frames.pop_front() {
            Some(f) => Some(Ok(Some(f))),
            None => {
                std::thread::sleep(std::time::Duration::from_millis(50));
                Some(Ok(None))
            }
        }
    }
}

impl Drop for Viewing {
    fn drop(&mut self) {
        self.open.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// What a press brings: the screen then, and its agent's state then (none: as it was).
type Next = (String, Option<workriver_core::runtime::AgentState>);

/// What a test does as a message is taken, given its receiver.
type Look = Box<dyn Fn(&str) + Send>;

/// The shell of a place nobody set: a real process that started nothing (a shell is only
/// taken for idle when its machine's process list holds it).
// ponytail: one `sleep` for all tests, left to end by itself after the run; it holds none of
// the test's pipes, so nothing reading the test's output waits for it.
pub(super) fn nobody() -> u32 {
    static IDLE: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *IDLE.get_or_init(|| {
        use std::process::Stdio;
        std::process::Command::new("sleep").arg("600").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap().id()
    })
}

impl Fake {
    /// As Herdr's implementation: only a place that is there, in the runs' group, a plain shell.
    fn run_shell_at(&self, group: &str, place: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
        let opened = self.opened.lock().unwrap().iter().any(|(g, _, p)| g == group && p == place);
        if !opened {
            assert_eq!(group, crate::server::commands::shell::GROUP);
        }
        let agent = self.agents.lock().unwrap().iter().any(|(p, _)| p == place);
        if agent || !self.screens.lock().unwrap().contains_key(place) || self.not_shells.lock().unwrap().iter().any(|p| p == place) {
            return Err(workriver_core::runtime::RuntimeError::NoPlace(format!("{place} is no longer a shell in {group}")));
        }
        Ok(())
    }
}

impl Fake {
    /// What is at a place: its agent, if one.
    fn at(&self, place: &str) -> workriver_core::runtime::Place {
        let agent = self.agents.lock().unwrap().iter().find(|(p, _)| p == place).map(|(_, a)| a.clone());
        workriver_core::runtime::Place { agent }
    }

    /// What Herdr's prompt does once the place is known: typed, or refused as Herdr refuses.
    fn take(&self, session: &str, text: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
        // As Herdr's socket that does not answer: nothing is typed.
        if self.down.load(std::sync::atomic::Ordering::Relaxed) || self.prompt_down.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(workriver_core::runtime::RuntimeError::None);
        }
        if self.strict.load(std::sync::atomic::Ordering::Relaxed) && !self.agents.lock().unwrap().iter().any(|(_, a)| a.session == session || a.name == session) {
            return Err(workriver_core::runtime::RuntimeError::NoAgent(session.into()));
        }
        if self.not_ready.try_update(std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst, |n| n.checked_sub(1)).is_ok() {
            return Err(workriver_core::runtime::RuntimeError::NotReady(format!("herdr agent.prompt: agent {session} is not an active named agent (agent_not_ready)")));
        }
        // As Herdr does: an agent waiting for a person takes no prompt.
        if self.agents.lock().unwrap().iter().any(|(_, a)| (a.session == session || a.name == session) && a.state == workriver_core::runtime::AgentState::Blocked) {
            return Err(workriver_core::runtime::RuntimeError::Blocked(format!("herdr agent.prompt: agent {session} is blocked (agent_blocked)")));
        }
        self.sent.lock().unwrap().push((session.into(), text.into()));
        if let Some(look) = &*self.on_send.lock().unwrap() {
            look(session);
        }
        Ok(())
    }
}

impl workriver_core::runtime::Runtime for Fake {
    fn agents(&self) -> Result<Vec<workriver_core::runtime::RuntimeAgent>, workriver_core::runtime::RuntimeError> {
        let probe = self.on_agents.lock().unwrap().clone();
        if let Some(probe) = probe { probe(); }
        if self.down.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(workriver_core::runtime::RuntimeError::None);
        }
        Ok(self.agents.lock().unwrap().iter().map(|(_, a)| a.clone()).collect())
    }
    fn locate(&self, _: Option<&str>, place: &str) -> Result<workriver_core::runtime::Place, workriver_core::runtime::RuntimeError> {
        if self.locate_down.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(workriver_core::runtime::RuntimeError::None);
        }
        Ok(self.at(place))
    }
    fn agent_roots(&self, machine: Option<&str>) -> Result<Vec<workriver_core::runtime::Root>, workriver_core::runtime::RuntimeError> {
        if self.agent_roots_down.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(workriver_core::runtime::RuntimeError::Failed("herdr does not answer".into()));
        }
        Ok(self.roots(machine, None)?.into_iter().filter(|r| r.at.agent.is_some()).collect())
    }
    /// Every place has a shell at its root, by its name (`root`).
    fn roots(&self, _: Option<&str>, place: Option<&str>) -> Result<Vec<workriver_core::runtime::Root>, workriver_core::runtime::RuntimeError> {
        if self.down.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(workriver_core::runtime::RuntimeError::None);
        }
        if place.is_some() && self.place_fails.try_update(std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst, |n| n.checked_sub(1)).is_ok() {
            return Err(workriver_core::runtime::RuntimeError::Failed("herdr pane.process_info: the connection ended".into()));
        }
        let mut places: Vec<String> = self.agents.lock().unwrap().iter().map(|(p, _)| p.clone()).chain(self.plain.lock().unwrap().clone()).collect();
        if let Some(p) = place.filter(|p| !places.iter().any(|x| x == p) && !self.gone.lock().unwrap().iter().any(|x| x == p)) {
            places.push(p.to_string());
        }
        let no_shell = self.no_shell.lock().unwrap().clone();
        Ok(places
            .into_iter()
            .filter(|p| place.is_none_or(|x| x == p))
            .map(|p| {
                let pid = root(&p);
                workriver_core::runtime::Root { shell: (!no_shell.contains(&p)).then_some(pid), front: vec![pid], at: self.at(&p), place: p }
            })
            .collect())
    }
    fn focus(&self, session: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
        self.focused.lock().unwrap().push(session.into());
        Ok(())
    }
    /// As Herdr's: only into the place named, while it shows the session, read from the same
    /// list `resolve` reads (so it fails as `resolve` does); then as `take`.
    fn send_at(&self, _: Option<&str>, place: &str, session: &str, text: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
        if self.resolve_down.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(workriver_core::runtime::RuntimeError::None);
        }
        let shown = self.agents.lock().unwrap().iter().any(|(p, a)| p == place && a.session == session) || self.also.lock().unwrap().iter().any(|(p, s)| p == place && s == session);
        if !shown && !self.down.load(std::sync::atomic::Ordering::Relaxed) && !self.prompt_down.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(workriver_core::runtime::RuntimeError::NoAgent(session.into()));
        }
        self.take(session, text)?;
        self.typed_at.lock().unwrap().push((place.into(), text.into()));
        Ok(())
    }
    fn start_temporary(&self, group: &str, harness: &str, name: &str, cwd: &str) -> Result<workriver_core::runtime::RuntimeAgent, workriver_core::runtime::RuntimeError> {
        self.started.lock().unwrap().push((group.into(), harness.into(), name.into(), cwd.into()));
        let state = self.start_as.lock().unwrap().unwrap_or(workriver_core::runtime::AgentState::Idle);
        let mut agent = live(&format!("s-{name}"), name, group, state);
        agent.harness = harness.into();
        self.agents.lock().unwrap().push((format!("place-{name}"), agent.clone()));
        Ok(agent)
    }
    /// Kept with the others, the group's label as its id.
    fn start_temporary_at(&self, _: Option<&str>, group: &str, harness: &str, name: &str, cwd: &str) -> Result<workriver_core::runtime::RuntimeAgent, workriver_core::runtime::RuntimeError> {
        self.start_temporary(group, harness, name, cwd)
    }
    fn close(&self, agent: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
        self.closed.lock().unwrap().push(agent.into());
        self.agents.lock().unwrap().retain(|(_, a)| a.name != agent && a.session != agent);
        Ok(())
    }
    /// As Herdr's: the places that show the session now (its agent's, and any `also`); a
    /// runtime that does not answer, as Herdr's socket that is not there.
    fn resolve(&self, _: Option<&str>, session: &str) -> Result<String, workriver_core::runtime::RuntimeError> {
        use workriver_core::runtime::RuntimeError;
        if self.down.load(std::sync::atomic::Ordering::Relaxed) || self.resolve_down.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(RuntimeError::None);
        }
        let mut places: Vec<String> = self.agents.lock().unwrap().iter().filter(|(_, a)| a.session == session).map(|(p, _)| p.clone()).collect();
        places.extend(self.also.lock().unwrap().iter().filter(|(_, s)| s == session).map(|(p, _)| p.clone()));
        match places.as_slice() {
            [] => Err(RuntimeError::NoAgent(session.into())),
            [one] => Ok(one.clone()),
            _ => Err(RuntimeError::Ambiguous(format!("{session} shows in {}", places.join(", ")))),
        }
    }
    fn screen(&self, _: Option<&str>, place: &str) -> Result<workriver_core::screens::Screen, workriver_core::runtime::RuntimeError> {
        self.reads.lock().unwrap().push(place.into());
        if let Some(look) = &*self.on_read.lock().unwrap() {
            look(place);
        }
        if self.screen_down.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(workriver_core::runtime::RuntimeError::None);
        }
        Ok(workriver_core::screens::Screen::of(self.shown.lock().unwrap().get(place).cloned().unwrap_or_default()))
    }
    fn press(&self, _: Option<&str>, place: &str, keys: &[workriver_core::screens::Key]) -> Result<(), workriver_core::runtime::RuntimeError> {
        self.pressed.lock().unwrap().push((place.into(), keys.to_vec()));
        if self.press_fails.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(workriver_core::runtime::RuntimeError::Failed("herdr pane.send_keys: the answer was lost".into()));
        }
        let next = self.then.lock().unwrap().get_mut(place).and_then(|q| q.pop_front());
        if let Some((screen, state)) = next {
            self.shown.lock().unwrap().insert(place.into(), screen);
            if let Some(state) = state {
                self.agents.lock().unwrap().iter_mut().filter(|(p, _)| p == place).for_each(|(_, a)| a.state = state);
            }
        }
        Ok(())
    }
    fn watch(&self, _: Option<&str>, to: &mut dyn FnMut(workriver_core::runtime::Change) -> bool) -> Result<(), workriver_core::runtime::RuntimeError> {
        use std::sync::mpsc::RecvTimeoutError;
        self.watched.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let changes = self.changes.lock().unwrap();
        let Some(rx) = changes.as_ref() else { return Err(workriver_core::runtime::RuntimeError::None) };
        loop {
            match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(c) => {
                    if !to(c) {
                        return Ok(());
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Err(workriver_core::runtime::RuntimeError::Failed("the change stream ended".into())),
            }
        }
    }
    fn observe(&self, _: Option<&str>, _: &str) -> Result<workriver_core::runtime::Frames, workriver_core::runtime::RuntimeError> {
        self.observing.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(Box::new(Viewing { frames: self.frames.lock().unwrap().clone().into(), open: self.observing.clone() }))
    }
    fn why_blocked(&self, _: Option<&str>, place: &str) -> Result<Option<String>, workriver_core::runtime::RuntimeError> {
        if let Some(look) = &*self.on_why.lock().unwrap() {
            look(place);
        }
        Ok(self.why.lock().unwrap().clone())
    }
    fn run_shell(&self, machine: Option<&str>, group: &str, dir: &str, line: &str) -> Result<String, workriver_core::runtime::RuntimeError> {
        let mut shells = self.shells.lock().unwrap();
        shells.push((machine.map(str::to_string), group.into(), dir.into(), line.into()));
        let place = format!("shell-{}", shells.len());
        self.screens.lock().unwrap().insert(place.clone(), format!("$ {line}\n"));
        Ok(place)
    }
    fn read_place(&self, _: Option<&str>, place: &str, _: u32) -> Result<String, workriver_core::runtime::RuntimeError> {
        self.screens.lock().unwrap().get(place).cloned().ok_or_else(|| workriver_core::runtime::RuntimeError::NoAgent(place.into()))
    }
    fn shell_processes(&self, _: Option<&str>, group: &str, place: &str) -> Result<(u32, Vec<u32>), workriver_core::runtime::RuntimeError> {
        self.looked.lock().unwrap().push(place.into());
        self.run_shell_at(group, place)?;
        // An opened place's shell is its root, as `roots` gives it.
        if self.opened.lock().unwrap().iter().any(|(_, _, p)| p == place) {
            return Ok((root(place), vec![root(place)]));
        }
        Ok(self.procs.lock().unwrap().get(place).cloned().unwrap_or((nobody(), vec![nobody()])))
    }
    fn open_place(&self, _: Option<&str>, group: &str, dir: &str) -> Result<String, workriver_core::runtime::RuntimeError> {
        if self.down.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(workriver_core::runtime::RuntimeError::None);
        }
        let mut opened = self.opened.lock().unwrap();
        let place = format!("opened-{}", opened.len() + 1);
        opened.push((group.into(), dir.into(), place.clone()));
        self.plain.lock().unwrap().push(place.clone());
        self.screens.lock().unwrap().insert(place.clone(), String::new());
        Ok(place)
    }
    fn start_in(&self, _: Option<&str>, place: &str, harness: &str, name: &str, args: &[String]) -> Result<(), workriver_core::runtime::RuntimeError> {
        use workriver_core::runtime::{AgentState, RuntimeError};
        self.started_in.lock().unwrap().push((place.into(), harness.into(), name.into(), args.to_vec()));
        let dir = self.opened.lock().unwrap().iter().find(|(_, _, p)| p == place).map(|(_, d, _)| d.clone()).unwrap_or_default();
        // As Herdr: a live agent's name is unique across the runtime.
        if self.agents.lock().unwrap().iter().any(|(_, a)| a.name == name) {
            return Err(RuntimeError::Failed(format!("herdr agent.start: agent name {name} is already used (agent_name_taken)")));
        }
        let bring = self.bring.lock().unwrap().clone();
        let (session, state) = match bring {
            Bring::Refuse(words) => return Err(RuntimeError::Failed(words)),
            Bring::Nothing => return Ok(()),
            Bring::Session => (format!("s-{name}"), AgentState::Idle),
            Bring::Launching => (format!("s-{name}"), AgentState::Unknown),
            Bring::Pending => (format!("pending:{place}"), AgentState::Blocked),
        };
        if let Some(screen) = self.start_screen.lock().unwrap().clone() {
            self.screens.lock().unwrap().insert(place.into(), screen);
        }
        let agent = workriver_core::runtime::RuntimeAgent { harness: harness.into(), cwd: dir, ..live(&session, name, "w1", state) };
        self.plain.lock().unwrap().retain(|p| p != place);
        self.agents.lock().unwrap().push((place.into(), agent));
        match state {
            AgentState::Blocked => Err(RuntimeError::NotReady(format!("herdr agent.start: agent {name} was blocked during startup and is not ready for prompts (agent_not_ready)"))),
            _ => Ok(()),
        }
    }
    fn has_session(&self, agent: &workriver_core::runtime::RuntimeAgent) -> bool {
        !agent.session.starts_with("pending:")
    }
    fn harnesses(&self, _: Option<&str>) -> Result<Vec<workriver_core::runtime::HarnessState>, workriver_core::runtime::RuntimeError> {
        if self.down.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(workriver_core::runtime::RuntimeError::None);
        }
        let all = || ["claude", "codex"].map(|kind| workriver_core::runtime::HarnessState { kind: kind.into(), installed: true, integrated: true }).to_vec();
        Ok(self.harness_states.lock().unwrap().clone().unwrap_or_else(all))
    }
    fn close_shell(&self, _: Option<&str>, group: &str, place: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
        self.close_tried.lock().unwrap().push(place.into());
        self.run_shell_at(group, place)?;
        if self.close_fails.try_update(std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst, |n| n.checked_sub(1)).is_ok() {
            return Err(workriver_core::runtime::RuntimeError::Failed("herdr does not answer".into()));
        }
        self.closed_places.lock().unwrap().push(place.into());
        self.screens.lock().unwrap().remove(place);
        // An opened place is gone once closed.
        if self.opened.lock().unwrap().iter().any(|(_, _, p)| p == place) {
            self.plain.lock().unwrap().retain(|p| p != place);
            self.gone.lock().unwrap().push(place.into());
        }
        Ok(())
    }
}

pub(super) fn live(session: &str, name: &str, group: &str, state: workriver_core::runtime::AgentState) -> workriver_core::runtime::RuntimeAgent {
    workriver_core::runtime::RuntimeAgent {
        session: session.into(),
        harness: "claude".into(),
        state,
        cwd: String::new(),
        group: Some(workriver_core::RuntimeRef { id: group.into(), label: "chat".into(), machine: None }),
        name: name.into(),
        topic: None,
        machine: None,
    }
}

/// The id of the open workspace named `name`, made when there is none: the first one with
/// the test's project (a folder in the daemon's home, registered as the user), the others
/// added to it. `d`: the user's client.
pub(super) fn workspace_named(d: &Daemon, name: &str) -> String {
    let state = d.get("/api/state").unwrap();
    if let Some(w) = state["workspaces"].as_array().into_iter().flatten().find(|w| w["name"] == name && w["closed"] != true) {
        return w["id"].as_str().unwrap().to_string();
    }
    if state["projects"].as_array().is_none_or(|p| p.is_empty()) {
        let folder = d.home.clone().expect("the user's client").join("proj");
        std::fs::create_dir_all(&folder).unwrap();
        d.append("", "project.added", json!({ "path": folder, "title": name })).unwrap();
        return workspace_named(d, name);
    }
    d.call("/api/workspaces", Some(json!({ "action": "add", "project": "p1", "name": name }))).unwrap()["body"]["id"].as_str().unwrap().to_string()
}

/// The seat of workspace `ws` as the records have it now: what a seating names as `replaces`.
pub(super) fn seat_of(d: &Daemon, ws: &str) -> serde_json::Value {
    let state = d.get("/api/state").unwrap();
    let w = state["workspaces"].as_array().unwrap().iter().find(|w| w["id"] == ws).unwrap_or_else(|| panic!("no workspace {ws}")).clone();
    match w.get("manager").filter(|m| !m.is_null()) {
        Some(m) => json!({ "session": m["session"], "since": m["since"] }),
        None => serde_json::Value::Null,
    }
}

/// The user seats `agent` (a session or a name; none: empties the seat) in workspace `ws`,
/// over the seat as it is.
pub(super) fn seat(d: &Daemon, ws: &str, agent: Option<&str>) -> anyhow::Result<serde_json::Value> {
    d.call("/api/workspaces", Some(json!({ "action": "manager", "workspace": ws, "manager": agent, "replaces": seat_of(d, ws) })))
}

/// The agent at `place` becomes the manager of a workspace, by the user's act, and the user
/// starts a new intent there (the next id, i1 for the first). The workspace is the one named
/// as the agent's runtime group is labelled, as the page lists the agent (made when there is
/// none; `live` labels every group "chat"), so that agents a test puts in groups of two
/// labels manage two workspaces: only a way for the tests to say which, nothing the product
/// reads. `d`: the user's client.
pub(super) fn manage(d: &Daemon, place: &str, agent: &str) -> String {
    at_place(d, place).call("/api/here", Some(json!({}))).unwrap();
    let mut label = None;
    wait_until("the agent is listed", || {
        let sessions = d.get("/api/workspace").unwrap()["sessions"].clone();
        label = sessions.as_array().into_iter().flatten().find(|s| s["session"] == agent && s["ended"] != true).map(|s| s["group"]["label"].as_str().unwrap_or("chat").to_string());
        label.is_some()
    });
    let ws = workspace_named(d, label.as_deref().unwrap_or("chat"));
    seat(d, &ws, Some(agent)).unwrap();
    let made = d.append("", "intent.created", json!({ "title": "Chat", "state": "planned" })).unwrap();
    let id = made["body"]["id"].as_str().unwrap().to_string();
    // The user starts it: an intent of no project is not its manager's to start.
    d.append("", "intent.state_changed", json!({ "intent": id, "state": "active", "workspace": ws })).unwrap();
    id
}

/// Starts a daemon on a free port over a fake runtime with short waits; returns its client.
pub(super) fn daemon_with(fake: std::sync::Arc<Fake>, db: &std::path::Path, waits: crate::server::Waits) -> Daemon {
    daemon_and(fake, db, waits).0
}

/// The same, with what its service believes of processes.
pub(super) fn daemon_and(fake: std::sync::Arc<Fake>, db: &std::path::Path, waits: crate::server::Waits) -> (Daemon, Kernel) {
    let store = workriver_data::Store::open(db).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let d = kernel.client(listener.local_addr().unwrap().port(), db.parent().unwrap());
    let options = crate::server::Options { waits, allow_as: true, ..kernel.options() };
    std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, fake, options)));
    (d, kernel)
}

/// What a test makes fail in a `Flaky` store, in the words "the disk is gone".
#[derive(Default)]
pub(super) struct Faults {
    /// Every read fails while this is set.
    pub(super) down: std::sync::atomic::AtomicBool,
    /// Reads start failing (`down` is set) as soon as an event of this kind is committed.
    pub(super) down_after: std::sync::Mutex<Option<String>>,
    /// Only the read of an agent by its session fails.
    pub(super) agents_down: std::sync::atomic::AtomicBool,
    /// Nothing can be recorded while this is set.
    pub(super) writes_down: std::sync::atomic::AtomicBool,
    /// When `down_after` sets `down`, it sets `writes_down` too.
    pub(super) and_writes: std::sync::atomic::AtomicBool,
    /// Once an event of this kind is committed, this one is recorded as the user right
    /// after it: another request that commits before anything is read again.
    pub(super) then_record: std::sync::Mutex<Option<(String, workriver_core::Body)>>,
}

/// The real store, with reads that fail when the test says so: a storage that cannot be
/// read. Writing works unless the test stops it too.
pub(super) struct Flaky {
    inner: workriver_data::Store,
    faults: std::sync::Arc<Faults>,
}

fn gone<T>() -> Result<T, workriver_core::ports::DataError> {
    Err(workriver_core::ports::DataError::Unavailable("the disk is gone".into()))
}

impl workriver_core::ports::WorkStore for Flaky {
    fn recorded(&self) -> Result<workriver_core::ports::Recorded, workriver_core::ports::DataError> {
        if self.faults.down.load(std::sync::atomic::Ordering::SeqCst) {
            return gone();
        }
        self.inner.recorded()
    }
    fn record(&mut self, who: &workriver_core::Principal, request: workriver_core::Body) -> Result<workriver_core::ports::Committed, workriver_core::ports::DataError> {
        if self.faults.writes_down.load(std::sync::atomic::Ordering::SeqCst) {
            return gone();
        }
        let done = self.inner.record(who, request)?;
        let next = self.faults.then_record.lock().unwrap().take_if(|(kind, _)| serde_json::to_value(&done.event).unwrap()["kind"].as_str() == Some(kind.as_str()));
        if let Some((_, body)) = next {
            self.inner.record(&workriver_core::Principal::User, body)?;
        }
        let mut after = self.faults.down_after.lock().unwrap();
        if after.as_deref() == serde_json::to_value(&done.event).unwrap()["kind"].as_str() {
            *after = None;
            self.faults.down.store(true, std::sync::atomic::Ordering::SeqCst);
            if self.faults.and_writes.load(std::sync::atomic::Ordering::SeqCst) {
                self.faults.writes_down.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        Ok(done)
    }
    fn history(&self, after: workriver_core::ports::Revision, limit: usize) -> Result<workriver_core::ports::History, workriver_core::ports::DataError> {
        self.inner.history(after, limit)
    }
    fn status(&self) -> Result<workriver_core::ports::DataStatus, workriver_core::ports::DataError> {
        self.inner.status()
    }
    fn intent_events(&self, id: &str) -> Result<Vec<workriver_core::Event>, workriver_core::ports::DataError> {
        if self.faults.down.load(std::sync::atomic::Ordering::SeqCst) {
            return gone();
        }
        self.inner.intent_events(id)
    }
    fn agent(&self, session: &str) -> Result<Option<workriver_core::Agent>, workriver_core::ports::DataError> {
        if self.faults.agents_down.load(std::sync::atomic::Ordering::SeqCst) {
            return gone();
        }
        Ok(self.recorded()?.agents.iter().find(|a| a.session == session).cloned())
    }
}

/// A daemon as `daemon_and` starts one, over a `Flaky` store: its client as the user, its
/// kernel, and what the test may make fail.
pub(super) fn flaky_service(fake: std::sync::Arc<Fake>, db: &std::path::Path, waits: crate::server::Waits) -> (Daemon, Kernel, std::sync::Arc<Faults>) {
    let faults = std::sync::Arc::new(Faults::default());
    let store = Flaky { inner: workriver_data::Store::open(db).unwrap(), faults: faults.clone() };
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let home = db.parent().unwrap().to_path_buf();
    let d = kernel.client(listener.local_addr().unwrap().port(), &home);
    let options = crate::server::Options { waits, allow_as: true, ..kernel.options() };
    std::thread::spawn(move || rt.block_on(crate::server::serve_over(listener, Box::new(store), home, fake, options)));
    (d, kernel, faults)
}

/// Every event stored in the database at `db`, oldest first, read beside a running service
/// through the data port's history.
pub(super) fn stored(db: &std::path::Path) -> Vec<workriver_core::Event> {
    use workriver_core::ports::{Revision, WorkStore};
    workriver_data::Store::open(db).unwrap().history(Revision(0), usize::MAX).unwrap().events
}

/// The bodies of every stored event of `kind`, oldest first: what is only in the record
/// (commands the user ran).
pub(super) fn bodies(db: &std::path::Path, kind: &str) -> Vec<serde_json::Value> {
    stored(db).iter().map(|e| serde_json::to_value(e).unwrap()).filter(|v| v["kind"] == kind).map(|v| v["body"].clone()).collect()
}

pub(super) fn wait_until(what: &str, mut ok: impl FnMut() -> bool) {
    assert!((0..60).any(|_| ok() || { std::thread::sleep(std::time::Duration::from_millis(200)); false }), "{what}");
}

/// A runtime that cannot be reached.
pub(super) struct Broken;

impl workriver_core::runtime::Runtime for Broken {
    fn agents(&self) -> Result<Vec<workriver_core::runtime::RuntimeAgent>, workriver_core::runtime::RuntimeError> {
        Err(workriver_core::runtime::RuntimeError::Failed("ssh: connect to host gone: Operation timed out".into()))
    }
    fn locate(&self, _: Option<&str>, _: &str) -> Result<workriver_core::runtime::Place, workriver_core::runtime::RuntimeError> {
        Err(workriver_core::runtime::RuntimeError::None)
    }
    fn focus(&self, _: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
        Err(workriver_core::runtime::RuntimeError::None)
    }
    /// As its agents are read: no place for any session.
    fn resolve(&self, _: Option<&str>, session: &str) -> Result<String, workriver_core::runtime::RuntimeError> {
        self.agents()?;
        Err(workriver_core::runtime::RuntimeError::NoAgent(session.into()))
    }
    fn start_temporary(&self, _: &str, _: &str, _: &str, _: &str) -> Result<workriver_core::runtime::RuntimeAgent, workriver_core::runtime::RuntimeError> {
        Err(workriver_core::runtime::RuntimeError::None)
    }
    fn close(&self, _: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
        Err(workriver_core::runtime::RuntimeError::None)
    }
}

/// A runtime with nobody in it that takes `wait` milliseconds to say so, each time it is read.
#[derive(Default)]
pub(super) struct Slow {
    pub(super) wait: std::sync::atomic::AtomicU64,
}

impl workriver_core::runtime::Runtime for Slow {
    fn agents(&self) -> Result<Vec<workriver_core::runtime::RuntimeAgent>, workriver_core::runtime::RuntimeError> {
        std::thread::sleep(std::time::Duration::from_millis(self.wait.load(std::sync::atomic::Ordering::Relaxed)));
        Ok(vec![])
    }
    fn locate(&self, _: Option<&str>, _: &str) -> Result<workriver_core::runtime::Place, workriver_core::runtime::RuntimeError> {
        Err(workriver_core::runtime::RuntimeError::None)
    }
    fn focus(&self, _: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
        Err(workriver_core::runtime::RuntimeError::None)
    }
    /// As its agents are read: no place for any session.
    fn resolve(&self, _: Option<&str>, session: &str) -> Result<String, workriver_core::runtime::RuntimeError> {
        self.agents()?;
        Err(workriver_core::runtime::RuntimeError::NoAgent(session.into()))
    }
    fn start_temporary(&self, _: &str, _: &str, _: &str, _: &str) -> Result<workriver_core::runtime::RuntimeAgent, workriver_core::runtime::RuntimeError> {
        Err(workriver_core::runtime::RuntimeError::None)
    }
    fn close(&self, _: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
        Err(workriver_core::runtime::RuntimeError::None)
    }
}

/// A stand-in for ssh on a machine whose sshd keeps the sessions of dead connections (and
/// the ports they forward) for good. `-N`: a tunnel; it asks for a port and gets the next
/// free one, refusing one still held ("remote port forwarding failed"), says it on stderr
/// as ssh does, and lives until `drop` exists (the line dies: it is removed and the port
/// stays held). Anything else runs its command with `$HOME` the machine's home.
pub(super) fn fake_ssh(dir: &std::path::Path) -> std::path::PathBuf {
    let ssh = dir.join("ssh");
    let script = format!(
        r#"#!/bin/sh
d={d}
tunnel=; want=
for a in "$@"; do
  [ "$a" = -N ] && tunnel=1
  case "$a" in 127.0.0.1:*:127.0.0.1:*) want=$(echo "$a" | cut -d: -f2);; esac
done
while [ "$1" != "--" ]; do shift; done; shift; shift
if [ -z "$tunnel" ]; then HOME=$d/remote exec sh -c "$1"; fi
echo x >> $d/connections
if [ "$want" = 0 ]; then
  p=$((40000 + $(wc -l < $d/connections)))
  while grep -qx "$p" $d/held 2>/dev/null; do p=$((p + 1)); done
else
  p=$want
  if grep -qx "$p" $d/held 2>/dev/null; then echo "Error: remote port forwarding failed for listen port $p" >&2; exit 255; fi
fi
echo $p >> $d/held
echo "Allocated port $p for remote forward to 127.0.0.1:1" >&2
while [ ! -e $d/drop ]; do sleep 0.05; done
rm -f $d/drop
echo "Timeout, server not responding." >&2
exit 255
"#,
        d = dir.display()
    );
    std::fs::create_dir_all(dir.join("remote")).unwrap();
    std::fs::write(&ssh, script).unwrap();
    std::fs::set_permissions(&ssh, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    ssh
}

/// One HTTP request by hand, to set headers a client would not: the status and the whole answer.
pub(super) fn http(port: u16, method: &str, path: &str, headers: &[(&str, &str)], body: Option<&str>) -> (u16, String) {
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n");
    for (k, v) in headers {
        req += &format!("{k}: {v}\r\n");
    }
    if let Some(b) = body {
        req += &format!("Content-Type: application/json\r\nContent-Length: {}\r\n", b.len());
    }
    // In one write, as a client would; the service reads what a handler leaves of a body
    // before it answers (http/drain.rs), so a refusal arrives whichever way it is sent.
    s.write_all(format!("{req}\r\n{}", body.unwrap_or_default()).as_bytes()).unwrap();
    let mut answer = vec![];
    s.read_to_end(&mut answer).unwrap();
    let text = String::from_utf8_lossy(&answer).into_owned();
    (text.split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0), text)
}

/// Starts a daemon over `runtime` as it runs for real (no `--as`), on a free port; its port
/// and what it believes of processes.
pub(super) fn daemon_over(runtime: std::sync::Arc<dyn workriver_core::runtime::Runtime>, machines: Option<std::sync::Arc<crate::runtime::follow::Machines>>, db: &std::path::Path) -> (u16, Kernel) {
    let store = workriver_data::Store::open(db).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let port = listener.local_addr().unwrap().port();
    let options = crate::server::Options { machines, ..kernel.options() };
    std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, runtime, options)));
    (port, kernel)
}

/// The user (`user`) seats the agent at `boss` as the manager of workspace "chat" (made, with
/// the test's project, when there is none); the workspace's id.
pub(super) fn seat_at(user: &Daemon, boss: &Daemon) -> String {
    let here = boss.call("/api/here", Some(json!({}))).unwrap();
    let ws = workspace_named(user, "chat");
    seat(user, &ws, here["actor"].as_str()).unwrap();
    ws
}

/// The user (`user`) seats the agent at `boss` as the manager of workspace "chat" (ws1) and
/// starts a new intent there (an intent of no project is not its manager's to start).
pub(super) fn manage_here(user: &Daemon, boss: &Daemon) {
    let ws = seat_at(user, boss);
    boss.append("", "intent.created", json!({ "title": "Chat", "state": "planned" })).unwrap();
    user.append("", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": ws })).unwrap();
}

/// A plain request to `port` as the CLI of a command run at `place` sends it (`d`: a client of
/// that service), without the token.
pub(super) fn http_at(d: &Daemon, port: u16, place: &str, method: &str, path: &str, extra: &[(&str, &str)], body: Option<&str>) -> (u16, String) {
    let (proof, pid) = at_place(d, place).process(false);
    let (proof, pid) = (proof.unwrap_or_default(), pid.to_string());
    let mut headers = vec![("x-workriver-place", place), ("x-workriver-caller", proof.as_str()), ("x-workriver-pid", pid.as_str())];
    headers.extend_from_slice(extra);
    http(port, method, path, &headers, body)
}

/// A request as the user's CLI sends it (the token, a process at no place): the status and
/// the answer's body.
pub(super) fn as_user(d: &Daemon, method: &str, path: &str, body: Option<&serde_json::Value>) -> (u16, String) {
    let (proof, pid) = d.process(false);
    let (proof, pid, token) = (proof.unwrap_or_default(), pid.to_string(), format!("Bearer {}", d.token().unwrap_or_default()));
    let headers = [("authorization", token.as_str()), ("x-workriver-caller", proof.as_str()), ("x-workriver-pid", pid.as_str())];
    let (code, text) = http(d.port, method, path, &headers, body.map(|b| b.to_string()).as_deref());
    (code, text.split("\r\n\r\n").nth(1).unwrap_or_default().to_string())
}

/// The status and body a request from `place` is answered with (no token).
pub(super) fn from_place(d: &Daemon, place: &str, method: &str, path: &str, body: Option<&serde_json::Value>) -> (u16, String) {
    let (code, text) = http_at(d, d.port, place, method, path, &[], body.map(|b| b.to_string()).as_deref());
    (code, text.split("\r\n\r\n").nth(1).unwrap_or_default().to_string())
}

/// A process as a test makes it up.
pub(super) fn proc(pid: u32, parent: u32, tty: Option<&str>, name: &str) -> crate::common::process::Proc {
    // A pane's root started before what runs in it, and always at the same time.
    crate::common::process::Proc { pid, parent, tty: tty.map(str::to_string), start: if pid >= 1_000_000 { 1 } else { 5 }, name: name.into() }
}

/// The same client, as a command run at `place`.
pub(super) fn at_place(d: &Daemon, place: &str) -> Daemon {
    Daemon { place: Some(place.into()), ..d.clone() }
}

/// A service as it runs for real (no `--as`, a terminal asked for) over `fake`; its port and
/// what it believes of processes.
pub(super) fn real_service(fake: std::sync::Arc<Fake>, home: &std::path::Path) -> (u16, Kernel) {
    let store = workriver_data::Store::open(&home.join("w.db")).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let port = listener.local_addr().unwrap().port();
    let options = kernel.options();
    std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, fake, options)));
    crate::server::access::auth::Auth::open(home).unwrap();
    wait_until("the service", || http(port, "GET", "/api/status", &[], None).0 == 200);
    (port, kernel)
}

/// One record tried over plain HTTP: what the request says (`says`: the pane it names),
/// whether it carries the token, and the proof of its process, if any. The status and the answer.
pub(super) fn try_record(port: u16, home: &std::path::Path, token: bool, says: Option<&str>, proof: Option<&str>) -> (u16, String) {
    let bearer = format!("Bearer {}", crate::common::home::read_token(home).unwrap());
    let mut headers = vec![];
    if token {
        headers.push(("authorization", bearer.as_str()));
    }
    headers.extend(says.map(|p| ("x-workriver-place", p)));
    headers.extend(proof.map(|p| ("x-workriver-caller", p)));
    let (code, text) = http(port, "POST", "/api/append", &headers, Some(&json!({ "kind": "intent.created", "body": { "title": "x", "state": "planned" } }).to_string()));
    (code, text.split("\r\n\r\n").nth(1).unwrap_or_default().to_string())
}
