//! Other machines: the agents of each machine's own runtime next to this machine's, behind
//! the one `Runtime`. Every machine has an independent runtime that workriver talks to
//! itself; nothing is assumed unique across machines, so what is read from one carries its
//! name.
//!
//! Per enabled machine one connection (ssh.rs) brings that machine's runtime socket here.
//! Its agents are read on a thread of their own, so a machine that cannot be reached never
//! holds up this one.
//!
//! While a machine's connection is down nothing waits on it: its agents are as last seen
//! (and not counted as working or waiting), its repositories as last read. So are this
//! machine's agents while its runtime does not answer and other machines are followed.
//!
//! The list of machines is workriver's own, in `<home>/settings.json`.
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use workriver_core::RuntimeRef;
use workriver_core::runtime::{Place, Root, Runtime, RuntimeAgent, RuntimeError};
use workriver_scan as scan;

use super::ssh::{Pace, Tunnel, hint, nap, publish, supervise, this_host, tunnel_words};
use super::{Setting, Settings, adapter, remote_ref};
use crate::common::config::defaults::SLOW_MS;
use crate::common::config::SETTINGS;
use crate::common::wire::MachineState;

#[derive(Default)]
struct Seen {
    agents: Vec<RuntimeAgent>,
    ok: bool,
    error: Option<String>,
}

struct Entry {
    label: String,
    setting: Setting,
    runtime: Arc<dyn Runtime>,
    runner: scan::Runner,
    seen: Mutex<Seen>,
    tunnel: Mutex<Tunnel>,
    stop: AtomicBool,
}

pub struct Machines {
    local: Arc<dyn Runtime>,
    /// This machine's agents as last read, and whether the last read was.
    here: Mutex<Seen>,
    home: PathBuf,
    terminal_app: String,
    /// The ssh program (a stand-in in tests) and the supervisor's waits.
    ssh: String,
    pace: Pace,
    entries: Mutex<Vec<Arc<Entry>>>,
    /// Each machine's listener port on this machine.
    ports: Mutex<BTreeMap<String, u16>>,
}

/// How long between reads of a machine's agents: `OK` while it answers, doubling up to
/// `MAX` while it does not.
const OK: Duration = Duration::from_secs(5);
const MAX: Duration = Duration::from_secs(120);

impl Machines {
    pub fn new(local: Arc<dyn Runtime>, home: PathBuf, terminal_app: String) -> Arc<Machines> {
        Arc::new(Machines { local, here: Mutex::default(), home, terminal_app, ssh: "ssh".into(), pace: Pace::default(), entries: Mutex::default(), ports: Mutex::default() })
    }

    /// For tests: ssh is `program` (a stand-in), with these waits.
    #[cfg(test)]
    pub fn with_ssh(local: Arc<dyn Runtime>, home: PathBuf, program: &str, pace: Pace) -> Arc<Machines> {
        Arc::new(Machines { local, here: Mutex::default(), home, terminal_app: String::new(), ssh: program.into(), pace, entries: Mutex::default(), ports: Mutex::default() })
    }

    fn settings(&self) -> Settings {
        Settings::read(&self.home)
    }

    fn save(&self, s: &Settings) -> Result<(), String> {
        s.write(&self.home)
    }

    /// Every machine with what is known of it now.
    pub fn list(&self) -> Vec<MachineState> {
        let running = self.entries.lock().unwrap().clone();
        self.settings()
            .machines
            .into_iter()
            .map(|(label, s)| match running.iter().find(|e| e.label == label) {
                Some(e) => state_of(e),
                None => MachineState { label, ssh: s.ssh, enabled: s.enabled, port: None, down_since: None, retry_in: None, reachable: false, error: None, connecting: false, tunnel: false, tunnel_error: None, agents: 0 },
            })
            .collect()
    }

    /// Whether this machine's runtime answered the last read of the agents: if not, they are
    /// as last seen, and what is not listed is not known to be gone.
    pub fn here_read(&self) -> bool {
        self.here.lock().unwrap().ok
    }

    /// The machines followed now.
    pub fn states(&self) -> Vec<MachineState> {
        self.entries.lock().unwrap().iter().map(|e| state_of(e)).collect()
    }

    /// The machines to follow from the start.
    pub fn wanted(&self) -> Vec<(String, Setting)> {
        self.settings().machines.into_iter().filter(|(_, s)| s.enabled).collect()
    }

    /// Adds a machine, not yet followed. Its name is used inside ids (`w1@name`) and on the
    /// command line (`name:workspace`), so it is kept plain.
    pub fn add(&self, label: &str, setting: Setting) -> Result<(), String> {
        if label == "local" {
            return Err("\"local\" names the machine workriver runs on; give the other machine another name".into());
        }
        let plain = !label.is_empty() && label.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        if !plain {
            return Err(format!("a machine's name is letters, digits, '-', '_' or '.'; not {label:?}"));
        }
        if setting.ssh.trim().is_empty() || setting.ssh.starts_with('-') {
            return Err("a machine needs its ssh target: --ssh <host or user@host>".into());
        }
        let _held = SETTINGS.lock().unwrap();
        let mut all = self.settings();
        if all.machines.contains_key(label) {
            return Err(format!("there is a machine {label} already; remove it first"));
        }
        all.machines.insert(label.into(), setting);
        self.save(&all)
    }

    /// Records whether a machine is followed; the caller starts or stops it.
    pub fn enable(&self, label: &str, enabled: bool) -> Result<Setting, String> {
        let _held = SETTINGS.lock().unwrap();
        let mut all = self.settings();
        let s = all.machines.get_mut(label).ok_or_else(|| format!("no machine {label}; add it with `workriver machine add {label} --ssh <target>`"))?;
        s.enabled = enabled;
        let s = s.clone();
        self.save(&all)?;
        Ok(s)
    }

    pub fn remove(&self, label: &str) -> Result<(), String> {
        let _held = SETTINGS.lock().unwrap();
        let mut all = self.settings();
        all.machines.remove(label).ok_or_else(|| format!("no machine {label}"))?;
        self.stop(label);
        self.save(&all)
    }

    pub fn listening(&self, label: &str, port: u16) {
        self.ports.lock().unwrap().insert(label.into(), port);
    }

    #[cfg(test)]
    pub fn local_port(&self, label: &str) -> Option<u16> {
        self.ports.lock().unwrap().get(label).copied()
    }

    /// Where a machine's runtime socket appears on this machine.
    fn socket(&self, label: &str) -> PathBuf {
        adapter::remote::socket_here(&self.home, label)
    }

    /// Starts following a machine: keeps its connection up (its runtime socket here, a port
    /// there leading to this daemon's `local_port`) and reads its agents, each on a thread.
    pub fn start(self: &Arc<Self>, label: &str, setting: Setting, local_port: u16) {
        self.stop(label);
        let socket = self.socket(label);
        let entry = Arc::new(Entry {
            label: label.into(),
            runtime: adapter::remote::connect(socket.clone(), label, self.terminal_app.clone()),
            runner: scan::Runner::ssh(&setting.ssh, &self.home),
            setting,
            seen: Mutex::default(),
            tunnel: Mutex::default(),
            stop: AtomicBool::new(false),
        });
        self.entries.lock().unwrap().push(entry.clone());
        let e = entry.clone();
        std::thread::spawn(move || {
            let mut delay = OK;
            while !e.stop.load(Ordering::Relaxed) {
                // While the connection is down there is nothing to read and nothing to wait
                // on: what was seen stays; look again soon, so its agents show once it is up.
                let up = e.tunnel.lock().unwrap().up;
                delay = if !up {
                    e.seen.lock().unwrap().ok = false;
                    Duration::from_millis(500)
                } else if poll_once(&e, false) {
                    OK
                } else {
                    (delay * 2).min(MAX)
                };
                nap(delay, &e.stop);
            }
        });
        let (ssh, pace, host) = (self.ssh.clone(), self.pace, this_host());
        std::thread::spawn(move || {
            let e = entry;
            // Where the runtime's socket is over there: as given, else asked of the machine.
            let mut delay = pace.min;
            let remote = loop {
                if e.stop.load(Ordering::Relaxed) {
                    return;
                }
                match adapter::remote::socket_there(&ssh, &e.setting) {
                    Ok(p) => break p,
                    Err(why) => {
                        tracing::warn!("machine {}: {why}", e.label);
                        *e.tunnel.lock().unwrap() = Tunnel { up: false, error: Some(why), since: Some(workriver_core::now()), retry: Some(Instant::now() + delay), port: None };
                        nap(delay, &e.stop);
                        delay = (delay * 2).min(pace.max);
                    }
                }
            };
            let mut words = tunnel_words(&e.setting.ssh, &socket.to_string_lossy(), &remote, local_port);
            words[0] = ssh.clone();
            supervise(&words, &e.tunnel, &e.stop, pace, &|port| publish(&ssh, &e.setting.ssh, port, &host));
            let _ = std::fs::remove_file(&socket);
        });
    }

    /// Stops following every machine: their connections end (the service stops).
    pub fn stop_all(&self) {
        for e in self.entries.lock().unwrap().iter() {
            e.stop.store(true, Ordering::Relaxed);
        }
    }

    /// Stops following a machine; its agents leave the list.
    pub fn stop(&self, label: &str) {
        self.entries.lock().unwrap().retain(|e| {
            let hit = e.label == label;
            if hit {
                e.stop.store(true, Ordering::Relaxed);
            }
            !hit
        });
    }

    /// Where git runs for a machine's repository: None while the machine cannot be reached
    /// (or is not followed), so nothing waits on it.
    pub fn runner(&self, machine: &str) -> Option<scan::Runner> {
        self.entry(machine).filter(|e| e.tunnel.lock().unwrap().up && e.seen.lock().unwrap().ok).map(|e| e.runner.clone())
    }

    /// The machine's connection is down: since when.
    pub fn down_since(&self, machine: &str) -> Option<String> {
        let e = self.entry(machine)?;
        let t = e.tunnel.lock().unwrap();
        (!t.up).then(|| t.since.clone().unwrap_or_default())
    }

    /// For tests: the connection ends, as when the computer sleeps.
    #[cfg(test)]
    pub fn drop_tunnel(&self, machine: &str) {
        if let Some(e) = self.entry(machine) {
            *e.tunnel.lock().unwrap() = Tunnel { up: false, error: Some("Timeout, server not responding.".into()), since: Some(workriver_core::now()), ..Tunnel::default() };
        }
    }

    fn entry(&self, label: &str) -> Option<Arc<Entry>> {
        self.entries.lock().unwrap().iter().find(|e| e.label == label).cloned()
    }

    /// The machine an agent (by session, else by name) was last seen on.
    fn owner(&self, agent: &str, by_name: bool) -> Option<Arc<Entry>> {
        self.entries.lock().unwrap().iter().find(|e| e.seen.lock().unwrap().agents.iter().any(|a| if by_name { a.name == agent } else { a.session == agent })).cloned()
    }

    /// Runs `f` where the agent is: on the machine that has its session; else here; else on
    /// a machine that has an agent of that name.
    fn route<T>(&self, agent: &str, f: impl Fn(&dyn Runtime) -> Result<T, RuntimeError>) -> Result<T, RuntimeError> {
        if let Some(e) = self.owner(agent, false) {
            return on(&e, &f);
        }
        match f(self.local.as_ref()) {
            Err(RuntimeError::NoAgent(_) | RuntimeError::None) => match self.owner(agent, true) {
                Some(e) => on(&e, &f),
                None => Err(RuntimeError::NoAgent(agent.into())),
            },
            r => r,
        }
    }

    /// Runs `f` on `machine`'s runtime (none: this one's).
    fn at<T>(&self, machine: Option<&str>, f: impl Fn(&dyn Runtime) -> Result<T, RuntimeError>) -> Result<T, RuntimeError> {
        match machine {
            None => f(self.local.as_ref()),
            Some(m) => on(&*self.entry(m).ok_or_else(|| RuntimeError::Failed(format!("machine {m} is not followed")))?, &f),
        }
    }

    /// For tests: machines whose runtimes are given, read once, with no threads or ssh.
    #[cfg(test)]
    pub fn with(local: Arc<dyn Runtime>, remotes: Vec<(&str, Arc<dyn Runtime>)>) -> Arc<Machines> {
        let m = Machines::new(local, PathBuf::new(), String::new());
        for (label, runtime) in remotes {
            let setting = Setting { ssh: label.into(), options: Default::default(), port: 0, enabled: true };
            let tunnel = Mutex::new(Tunnel { up: true, port: Some(47999), ..Tunnel::default() });
            let e = Arc::new(Entry { label: label.into(), setting, runtime, runner: scan::Runner::Local, seen: Mutex::default(), tunnel, stop: AtomicBool::new(false) });
            poll_once(&e, false);
            m.entries.lock().unwrap().push(e);
        }
        m
    }

    /// For tests: git for a machine's repositories runs through `runner` (an ssh stand-in).
    #[cfg(test)]
    pub fn set_runner(&self, label: &str, runner: scan::Runner) {
        let mut entries = self.entries.lock().unwrap();
        let k = entries.iter().position(|e| e.label == label).unwrap();
        let old = entries[k].clone();
        let tunnel = Mutex::new(old.tunnel.lock().unwrap().clone());
        entries[k] = Arc::new(Entry { label: old.label.clone(), setting: old.setting.clone(), runtime: old.runtime.clone(), runner, seen: Mutex::new(std::mem::take(&mut *old.seen.lock().unwrap())), tunnel, stop: AtomicBool::new(false) });
    }

    /// Whether any machine's connection is up: whether `refresh` would read anything.
    pub fn any_up(&self) -> bool {
        self.entries.lock().unwrap().iter().any(|e| e.tunnel.lock().unwrap().up)
    }

    /// Reads again, now, the agents of every machine whose connection is up, each on a
    /// thread of its own so that a slow one holds up no other: for whoever looks for an
    /// agent the last read did not have, which may have started since. `found` is called
    /// here after each machine was read, until it says the search is over. It returns once
    /// every read ended, which a slow machine makes long: never call it while a request waits.
    pub fn refresh(&self, mut found: impl FnMut() -> bool) {
        let entries: Vec<Arc<Entry>> = self.entries.lock().unwrap().iter().filter(|e| e.tunnel.lock().unwrap().up).cloned().collect();
        let (read, reads) = std::sync::mpsc::channel();
        std::thread::scope(|s| {
            for e in &entries {
                let read = read.clone();
                s.spawn(move || {
                    poll_once(e, false);
                    let _ = read.send(());
                });
            }
            drop(read);
            for () in reads {
                if found() {
                    break;
                }
            }
        });
    }

    #[cfg(test)]
    pub fn poll_all(&self) {
        for e in self.entries.lock().unwrap().iter() {
            poll_once(e, false);
        }
    }
}

/// `f` on a machine's runtime; at once an error while its connection is down, so nothing
/// waits on a socket that leads nowhere (`Unreachable`: nothing was given there).
fn on<T>(e: &Entry, f: &impl Fn(&dyn Runtime) -> Result<T, RuntimeError>) -> Result<T, RuntimeError> {
    let t = e.tunnel.lock().unwrap().clone();
    if !t.up {
        return Err(RuntimeError::Unreachable(format!("machine {} is disconnected{}; workriver connects again by itself", e.label, t.since.map(|s| format!(" since {s}")).unwrap_or_default())));
    }
    f(e.runtime.as_ref())
}

/// Still on the first attempt: ssh is neither up nor has it said why not; or it is up and
/// the machine's agents have not been read yet, with nothing wrong so far.
pub fn connecting(tunnel: &Tunnel, read: bool, read_failed: bool) -> bool {
    if tunnel.up { !read && !read_failed } else { tunnel.error.is_none() }
}

fn state_of(e: &Entry) -> MachineState {
    let e_ssh = e.setting.ssh.clone();
    let seen = e.seen.lock().unwrap();
    let tunnel = e.tunnel.lock().unwrap();
    MachineState {
        label: e.label.clone(),
        ssh: e.setting.ssh.clone(),
        enabled: true,
        port: tunnel.port.filter(|_| tunnel.up),
        down_since: tunnel.since.clone().filter(|_| !tunnel.up),
        retry_in: tunnel.retry.filter(|_| !tunnel.up).map(|r| r.saturating_duration_since(Instant::now()).as_secs()),
        reachable: seen.ok && tunnel.up,
        error: seen.error.clone(),
        connecting: connecting(&tunnel, seen.ok, seen.error.is_some()),
        tunnel: tunnel.up,
        tunnel_error: tunnel.error.as_deref().map(|e| hint(e, &e_ssh)),
        agents: seen.agents.len(),
    }
}

/// A group read from a machine's runtime, named as that machine's (a runtime that does
/// not say so itself: a stand-in).
fn theirs(mut g: RuntimeRef, machine: &str) -> RuntimeRef {
    if g.machine.is_none() {
        g.id = remote_ref(&g.id, machine);
        g.machine = Some(machine.into());
    }
    g
}

fn their_agent(a: RuntimeAgent, machine: &str) -> RuntimeAgent {
    RuntimeAgent { machine: Some(machine.into()), group: a.group.map(|g| theirs(g, machine)), ..a }
}

/// One read of a machine's agents; what was last seen stays while it cannot be reached.
/// `quiet`: a failure is not news (the connection is not up yet).
fn poll_once(e: &Entry, quiet: bool) -> bool {
    let started = Instant::now();
    let read = e.runtime.agents();
    let ms = started.elapsed().as_millis() as u64;
    match &read {
        Ok(a) => tracing::info!(target: "perf", kind = "poll", machine = %e.label, ms, agents = a.len(), ok = true, slow = ms >= SLOW_MS),
        Err(err) => tracing::info!(target: "perf", kind = "poll", machine = %e.label, ms, ok = false, error = crate::common::diagnostics::kind_of(&err.to_string()), slow = ms >= SLOW_MS),
    }
    let mut seen = e.seen.lock().unwrap();
    match read {
        Ok(agents) => {
            seen.agents = agents.into_iter().map(|a| their_agent(a, &e.label)).collect();
            seen.ok = true;
            seen.error = None;
        }
        Err(_) if quiet => seen.ok = false,
        Err(err) => {
            let why = match err {
                RuntimeError::None => adapter::words::REMOTE_UNREACHABLE.to_string(),
                e => e.to_string(),
            };
            if seen.ok || seen.error.is_none() {
                tracing::warn!("machine {}: {why}", e.label);
            }
            seen.ok = false;
            seen.error = Some(why);
        }
    }
    seen.ok
}

impl Runtime for Machines {
    /// This machine's agents read fresh, and every followed machine's as last seen. While
    /// this machine's runtime does not answer, its agents too are as last seen, in no state
    /// of theirs: a part not read is never taken for a part with nobody in it.
    fn agents(&self) -> Result<Vec<RuntimeAgent>, RuntimeError> {
        let entries = self.entries.lock().unwrap().clone();
        let read = self.local.agents();
        let mut all = {
            let mut here = self.here.lock().unwrap();
            here.ok = read.is_ok();
            match read {
                Ok(a) => {
                    here.agents = a.clone();
                    a
                }
                Err(e) if entries.is_empty() => return Err(e),
                Err(_) => here.agents.iter().map(|a| RuntimeAgent { state: workriver_core::runtime::AgentState::Unknown, ..a.clone() }).collect(),
            }
        };
        for e in entries {
            // A machine that is disconnected: its agents as last seen, in no state of theirs.
            let up = e.tunnel.lock().unwrap().up;
            let seen = e.seen.lock().unwrap().agents.clone();
            all.extend(seen.into_iter().map(|a| if up { a } else { RuntimeAgent { state: workriver_core::runtime::AgentState::Unknown, ..a } }));
        }
        Ok(all)
    }

    fn locate(&self, machine: Option<&str>, place: &str) -> Result<Place, RuntimeError> {
        match machine {
            None => self.local.locate(None, place),
            Some(m) => {
                let e = self.entry(m).ok_or_else(|| RuntimeError::Failed(format!("machine {m} is not followed; enable it with `workriver machine enable`")))?;
                let at = on(&e, &|r: &dyn Runtime| r.locate(None, place))?;
                Ok(Place { agent: at.agent.map(|a| their_agent(a, m)), group: at.group.map(|g| theirs(g, m)) })
            }
        }
    }

    fn focus(&self, session: &str) -> Result<(), RuntimeError> {
        self.route(session, |r| r.focus(session))
    }

    fn send(&self, agent: &str, text: &str) -> Result<(), RuntimeError> {
        self.route(agent, |r| r.send(agent, text))
    }

    /// Asked of the machine named, whether or not the agent was seen there yet.
    fn send_to(&self, machine: Option<&str>, session: &str, text: &str) -> Result<(), RuntimeError> {
        self.at(machine, |r| r.send_to(None, session, text))
    }

    /// Asked of the machine named, at that machine's own place.
    fn send_at(&self, machine: Option<&str>, place: &str, session: &str, text: &str) -> Result<(), RuntimeError> {
        self.at(machine, |r| r.send_at(None, place, session, text))
    }

    /// A group of a machine is `<id>@<machine>`: the agent starts there.
    fn start_temporary(&self, group: &str, harness: &str, name: &str, cwd: &str) -> Result<RuntimeAgent, RuntimeError> {
        let remote = group.split_once('@').and_then(|(id, m)| Some((id, self.entry(m)?)));
        match remote {
            None => self.local.start_temporary(group, harness, name, cwd),
            Some((id, e)) => {
                let a = their_agent(on(&e, &|r: &dyn Runtime| r.start_temporary(id, harness, name, cwd))?, &e.label);
                // Known at once, so a message to it by name finds its machine before the next read.
                e.seen.lock().unwrap().agents.push(a.clone());
                Ok(a)
            }
        }
    }

    /// On the machine named, in its own runtime's group of that label.
    fn start_temporary_at(&self, machine: Option<&str>, group: &str, harness: &str, name: &str, cwd: &str) -> Result<RuntimeAgent, RuntimeError> {
        let Some(m) = machine else { return self.local.start_temporary_at(None, group, harness, name, cwd) };
        let e = self.entry(m).ok_or_else(|| RuntimeError::Failed(format!("machine {m} is not followed")))?;
        let a = their_agent(on(&e, &|r: &dyn Runtime| r.start_temporary_at(None, group, harness, name, cwd))?, &e.label);
        // Known at once, so a message to it by name finds its machine before the next read.
        e.seen.lock().unwrap().agents.push(a.clone());
        Ok(a)
    }

    fn close(&self, agent: &str) -> Result<(), RuntimeError> {
        self.route(agent, |r| r.close(agent))
    }

    /// Asked of that machine's own runtime: the ids are its operating system's.
    fn processes(&self, machine: Option<&str>) -> Result<Vec<(RuntimeAgent, Vec<u32>)>, RuntimeError> {
        match machine {
            None => self.local.processes(None),
            Some(m) => {
                let e = self.entry(m).ok_or_else(|| RuntimeError::Failed(format!("machine {m} is not followed")))?;
                Ok(on(&e, &|r: &dyn Runtime| r.processes(None))?.into_iter().map(|(a, pids)| (their_agent(a, m), pids)).collect())
            }
        }
    }

    /// Asked of that machine's own runtime, like its processes.
    fn roots(&self, machine: Option<&str>, place: Option<&str>) -> Result<Vec<Root>, RuntimeError> {
        match machine {
            None => self.local.roots(None, place),
            Some(m) => {
                let e = self.entry(m).ok_or_else(|| RuntimeError::Failed(format!("machine {m} is not followed; enable it with `workriver machine enable`")))?;
                let roots = on(&e, &|r: &dyn Runtime| r.roots(None, place))?;
                Ok(roots.into_iter().map(|r| Root { at: Place { agent: r.at.agent.map(|a| their_agent(a, m)), group: r.at.group.map(|g| theirs(g, m)) }, ..r }).collect())
            }
        }
    }

    /// The service reads this machine's on its tick; another machine's are not read so.
    fn agent_roots(&self, machine: Option<&str>) -> Result<Vec<Root>, RuntimeError> {
        match machine {
            None => self.local.agent_roots(None),
            Some(_) => Ok(self.roots(machine, None)?.into_iter().filter(|r| r.at.agent.is_some()).collect()),
        }
    }

    /// On the machine named, through its own runtime: the shell is the user's over there.
    fn run_shell(&self, machine: Option<&str>, group: &str, dir: &str, line: &str) -> Result<String, RuntimeError> {
        self.at(machine, |r| r.run_shell(None, group, dir, line))
    }

    fn shell_processes(&self, machine: Option<&str>, group: &str, place: &str) -> Result<(u32, Vec<u32>), RuntimeError> {
        self.at(machine, |r| r.shell_processes(None, group, place))
    }

    fn close_shell(&self, machine: Option<&str>, group: &str, place: &str) -> Result<(), RuntimeError> {
        self.at(machine, |r| r.close_shell(None, group, place))
    }

    fn read_place(&self, machine: Option<&str>, place: &str, lines: u32) -> Result<String, RuntimeError> {
        self.at(machine, |r| r.read_place(None, place, lines))
    }

    fn focus_place(&self, machine: Option<&str>, place: &str) -> Result<(), RuntimeError> {
        self.at(machine, |r| r.focus_place(None, place))
    }
}
