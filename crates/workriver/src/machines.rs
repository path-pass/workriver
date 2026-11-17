//! Other machines: the agents of each machine's own Herdr next to this machine's, behind
//! the one `Runtime`. Every machine is an independent Herdr server that workriver talks to
//! itself; nothing is assumed unique across machines, so what is read from one carries its
//! name.
//!
//! Per enabled machine one `ssh -N` carries both directions: `-L` brings that machine's
//! Herdr socket here (the same protocol, so the same client), `-R` gives the `workriver` CLI
//! over there a port that leads back to this daemon. It never asks for anything (BatchMode)
//! and is restarted with growing waits. Its agents are read on a thread of their own, so a
//! machine that cannot be reached never holds up this one.
//!
//! The port over there is a new one for every connection, chosen by ssh (`-R 0`), and
//! written over there to `~/.config/workriver/port` once the connection is up; the CLI there
//! reads it. A fixed port would be held for many minutes by the session of a connection
//! that died without closing (a computer that slept, a network that changed): the server
//! does not notice that its client is gone, and every reconnect would fail on it.
//!
//! While a machine's connection is down nothing waits on it: its agents are as last seen
//! (and not counted as working or waiting), its repositories as last read.
//!
//! The list of machines is workriver's own, in `<home>/settings.json`.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use workriver_core::RuntimeRef;
use workriver_core::runtime::{Place, Runtime, RuntimeAgent, RuntimeError};
use workriver_scan as scan;

use crate::herdr;
use crate::metrics::Thresholds;

/// A machine as the user added it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Setting {
    /// What ssh takes as the destination: a Host of the user's config, `user@host`.
    pub ssh: String,
    /// Herdr's socket on that machine; none: asked of the machine when connecting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub herdr_socket: Option<String>,
    /// Before 0.3 the port over there was fixed; it is chosen per connection now. Kept so
    /// that older settings still read.
    #[serde(default, skip_serializing)]
    pub port: u16,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize)]
struct Settings {
    /// By the machine's name.
    #[serde(default)]
    machines: BTreeMap<String, Setting>,
    /// When a machine's CPU or memory alerts: for this machine and every followed one.
    #[serde(default)]
    alerts: Thresholds,
    /// Minutes an agent may sit idle on a running task before it counts as stopped.
    #[serde(default = "stall_default", rename = "stallMinutes")]
    stall_minutes: u32,
    /// Hours a unit may stay ready to merge before its manager is asked to check it.
    #[serde(default = "ready_default", rename = "readyHours")]
    ready_hours: u32,
    /// Minutes a worker may sit idle with no task before its manager is told.
    #[serde(default = "idle_default", rename = "idleMinutes")]
    idle_minutes: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { machines: BTreeMap::new(), alerts: Thresholds::default(), stall_minutes: STALL_MINUTES, ready_hours: READY_HOURS, idle_minutes: IDLE_MINUTES }
    }
}

/// A ready unit is merged within a working day when it goes through push, pull request and
/// merge (each a gate the user answers), and a merge git sees is recorded within seconds; a
/// day without one is a squash merge git cannot see, or a pull request stuck or abandoned.
/// Any less would ask again about a pull request left overnight for review.
pub const READY_HOURS: u32 = 24;

fn ready_default() -> u32 {
    READY_HOURS
}

/// After how many hours a unit ready to merge and not merged has its manager asked to check
/// it (at least one). Set by hand in settings.json; read when the service starts.
pub fn ready_hours(home: &Path) -> u32 {
    read_settings(home).ready_hours.max(1)
}

/// In the records of real use a worker got its next task a median 4 minutes after it
/// delivered, 15 times in 21 within 30 minutes; the other 6 waited 37 minutes to 15 hours.
/// Thirty stays quiet through the usual quick hand-on and tells of a worker left holding
/// memory for hours; telling is all it does, the manager decides.
pub const IDLE_MINUTES: u32 = 30;

fn idle_default() -> u32 {
    IDLE_MINUTES
}

/// After how many minutes idle with no task a worker's manager is told to close it (at least
/// one). Set by hand in settings.json; read when the service starts.
pub fn idle_minutes(home: &Path) -> u32 {
    read_settings(home).idle_minutes.max(1)
}

/// See `crate::stall`: why ten.
pub const STALL_MINUTES: u32 = 10;
/// The most the stall threshold may be set to: four hours.
pub const STALL_MAX: u32 = 240;

fn stall_default() -> u32 {
    STALL_MINUTES
}

/// Held while `settings.json` is read and written back: machines and thresholds share it.
static SETTINGS: Mutex<()> = Mutex::new(());

fn read_settings(home: &Path) -> Settings {
    std::fs::read(home.join("settings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn write_settings(home: &Path, s: &Settings) -> Result<(), String> {
    let path = home.join("settings.json");
    let text = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// The alert thresholds as the user set them (the defaults until then).
pub fn thresholds(home: &Path) -> Thresholds {
    read_settings(home).alerts
}

/// Saves the alert thresholds; the machines in the same file stay as they are.
pub fn set_thresholds(home: &Path, alerts: Thresholds) -> Result<(), String> {
    let _held = SETTINGS.lock().unwrap();
    let mut all = read_settings(home);
    all.alerts = alerts;
    write_settings(home, &all)
}

/// After how many idle minutes a running task's agent counts as stopped.
pub fn stall_minutes(home: &Path) -> u32 {
    read_settings(home).stall_minutes
}

/// Saves the stall threshold (1 to `STALL_MAX` minutes); the rest of the file stays.
pub fn set_stall_minutes(home: &Path, minutes: u32) -> Result<(), String> {
    if !(1..=STALL_MAX).contains(&minutes) {
        return Err(format!("the stall threshold is 1 to {STALL_MAX} minutes, not {minutes}"));
    }
    let _held = SETTINGS.lock().unwrap();
    let mut all = read_settings(home);
    all.stall_minutes = minutes;
    write_settings(home, &all)
}

/// A machine as the page and `workriver machine list` show it.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MachineState {
    pub label: String,
    pub ssh: String,
    pub enabled: bool,
    /// The port over there that leads here, while connected (written there to
    /// `~/.config/workriver/port`).
    pub port: Option<u16>,
    /// The connection is down: since when (RFC 3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub down_since: Option<String>,
    /// The next attempt to connect, in seconds from now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_in: Option<u64>,
    /// Its agents can be read now.
    pub reachable: bool,
    /// Why not, in ssh's or Herdr's words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The first attempt to connect has not finished yet: neither up nor failed.
    pub connecting: bool,
    /// The connection (both forwards) is up.
    pub tunnel: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tunnel_error: Option<String>,
    pub agents: usize,
}

#[derive(Default)]
struct Seen {
    agents: Vec<RuntimeAgent>,
    groups: Vec<RuntimeRef>,
    ok: bool,
    error: Option<String>,
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct Tunnel {
    pub up: bool,
    pub error: Option<String>,
    /// The port ssh got over there for this connection.
    pub port: Option<u16>,
    /// When the last connection ended (RFC 3339).
    pub since: Option<String>,
    /// When the next attempt starts.
    pub retry: Option<Instant>,
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

/// Prints Herdr's socket path on the machine, as Herdr itself works it out (default session).
const ASK_SOCKET: &str = r#"printf %s "${HERDR_SOCKET_PATH:-${XDG_CONFIG_HOME:-$HOME/.config}/herdr/herdr.sock}""#;

/// The ssh options every connection of ours uses, whatever the user's ssh config says:
/// never ask, never forward the user's ssh agent to the machine, never take a host key
/// that is not known yet, give up on a dead line: no answer to connect in 5 s, or to three
/// keep-alives 10 s apart (30 s).
const QUIET: [&str; 6] = ["BatchMode=yes", "ForwardAgent=no", "StrictHostKeyChecking=yes", "ConnectTimeout=5", "ServerAliveInterval=10", "ServerAliveCountMax=3"];

/// Where the CLI on a followed machine finds the port that leads here, under its `$HOME`.
pub const PORT_FILE: &str = ".config/workriver/port";
/// And the name of the computer it leads to, for its error messages.
pub const HOST_FILE: &str = ".config/workriver/daemon";

/// What to do about an ssh failure the user can fix, after ssh's own words.
fn hint(error: &str, ssh: &str) -> String {
    let e = error.to_lowercase();
    if e.contains("host key verification failed") || e.contains("no matching host key") || e.contains("remote host identification has changed") {
        format!("{error} (workriver takes no host key by itself: connect once by hand to check and accept it, `ssh {ssh}`)")
    } else if e.contains("forwarding") || e.contains("cannot listen") || e.contains("address already in use") {
        // ponytail: forwards from the user's config cannot be cleared without clearing ours
        // (ClearAllForwardings clears both); a Host alias of workriver's own is the way out.
        format!("{error} (if your ssh config sets LocalForward/RemoteForward for this host, they are set up on workriver's connection too and can fail it: give workriver a Host alias without them)")
    } else {
        error.to_string()
    }
}

impl Machines {
    pub fn new(local: Arc<dyn Runtime>, home: PathBuf, terminal_app: String) -> Arc<Machines> {
        Arc::new(Machines { local, home, terminal_app, ssh: "ssh".into(), pace: Pace::default(), entries: Mutex::default(), ports: Mutex::default() })
    }

    /// For tests: ssh is `program` (a stand-in), with these waits.
    #[cfg(test)]
    pub fn with_ssh(local: Arc<dyn Runtime>, home: PathBuf, program: &str, pace: Pace) -> Arc<Machines> {
        Arc::new(Machines { local, home, terminal_app: String::new(), ssh: program.into(), pace, entries: Mutex::default(), ports: Mutex::default() })
    }

    fn settings(&self) -> Settings {
        read_settings(&self.home)
    }

    fn save(&self, s: &Settings) -> Result<(), String> {
        write_settings(&self.home, s)
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

    /// Where a machine's Herdr socket appears on this machine.
    fn socket(&self, label: &str) -> PathBuf {
        self.home.join(format!("herdr-{label}.sock"))
    }

    /// Starts following a machine: keeps its connection up (its Herdr socket here, a port
    /// there leading to this daemon's `local_port`) and reads its agents, each on a thread.
    pub fn start(self: &Arc<Self>, label: &str, setting: Setting, local_port: u16) {
        self.stop(label);
        let socket = self.socket(label);
        let entry = Arc::new(Entry {
            label: label.into(),
            runtime: Arc::new(herdr::Herdr::on(socket.clone(), label, self.terminal_app.clone())),
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
            // Where Herdr's socket is over there: as given, else asked of the machine.
            let mut delay = pace.min;
            let remote = loop {
                if e.stop.load(Ordering::Relaxed) {
                    return;
                }
                match e.setting.herdr_socket.clone().map(Ok).unwrap_or_else(|| ask_socket(&ssh, &e.setting.ssh)) {
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
            let setting = Setting { ssh: label.into(), herdr_socket: None, port: 0, enabled: true };
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

    #[cfg(test)]
    pub fn poll_all(&self) {
        for e in self.entries.lock().unwrap().iter() {
            poll_once(e, false);
        }
    }
}

/// `f` on a machine's runtime; at once an error while its connection is down, so nothing
/// waits on a socket that leads nowhere.
fn on<T>(e: &Entry, f: &impl Fn(&dyn Runtime) -> Result<T, RuntimeError>) -> Result<T, RuntimeError> {
    let t = e.tunnel.lock().unwrap().clone();
    if !t.up {
        return Err(RuntimeError::Failed(format!("machine {} is disconnected{}; workriver connects again by itself", e.label, t.since.map(|s| format!(" since {s}")).unwrap_or_default())));
    }
    f(e.runtime.as_ref())
}

/// This computer's name, for the CLI over there to say where it could not reach.
fn this_host() -> String {
    let out = Command::new("hostname").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    let plain: String = out.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).collect();
    if plain.is_empty() { "the computer workriver runs on".into() } else { plain }
}

/// Writes the port of a new connection over there (and this computer's name), atomically:
/// from now on the CLI there reaches this daemon through it, and nothing points at an
/// earlier connection's port.
pub fn publish(program: &str, target: &str, port: u16, host: &str) -> Result<(), String> {
    let script = format!(
        "umask 077; d=\"$HOME/.config/workriver\"; mkdir -p \"$d\" && printf '%s\\n' {port} > \"$d/port.$$\" && mv -f \"$d/port.$$\" \"$HOME/{PORT_FILE}\" && printf '%s\\n' '{host}' > \"$HOME/{HOST_FILE}\""
    );
    let mut cmd = Command::new(program);
    for o in QUIET {
        cmd.args(["-o", o]);
    }
    cmd.args(["-o", "ClearAllForwardings=yes", "--", target, &script]);
    let started = Instant::now();
    let out = herdr::output_within(cmd, Duration::from_secs(15)).map_err(|e| format!("cannot run ssh: {e}"))?;
    tracing::debug!(target: "perf", kind = "ssh", purpose = "publish", ms = started.elapsed().as_millis() as u64, code = out.status.code().unwrap_or(-1));
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("cannot write the port over there: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("").trim()))
    }
}

fn ask_socket(program: &str, ssh: &str) -> Result<String, String> {
    let mut cmd = Command::new(program);
    for o in QUIET {
        cmd.args(["-o", o]);
    }
    // This one sets up no forward, so the config's are cleared.
    cmd.args(["-o", "ClearAllForwardings=yes", "--", ssh, ASK_SOCKET]);
    let started = Instant::now();
    let out = herdr::output_within(cmd, Duration::from_secs(20)).map_err(|e| format!("cannot run ssh: {e}"))?;
    tracing::debug!(target: "perf", kind = "ssh", purpose = "herdr_socket", ms = started.elapsed().as_millis() as u64, code = out.status.code().unwrap_or(-1));
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && path.starts_with('/') {
        Ok(path)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("ssh gave no socket path").trim().to_string())
    }
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
        g.id = herdr::remote_ref(&g.id, machine);
        g.machine = Some(machine.into());
    }
    g
}

fn their_agent(a: RuntimeAgent, machine: &str) -> RuntimeAgent {
    RuntimeAgent { machine: Some(machine.into()), group: a.group.map(|g| theirs(g, machine)), ..a }
}

/// One read of a machine's agents and groups; what was last seen stays while it cannot be reached.
/// `quiet`: a failure is not news (the connection is not up yet).
fn poll_once(e: &Entry, quiet: bool) -> bool {
    let started = Instant::now();
    let read = e.runtime.agents().and_then(|a| Ok((a, e.runtime.groups()?)));
    let ms = started.elapsed().as_millis() as u64;
    match &read {
        Ok((a, _)) => tracing::info!(target: "perf", kind = "poll", machine = %e.label, ms, agents = a.len(), ok = true, slow = ms >= crate::snapshot::SLOW_MS),
        Err(err) => tracing::info!(target: "perf", kind = "poll", machine = %e.label, ms, ok = false, error = crate::perf::kind_of(&err.to_string()), slow = ms >= crate::snapshot::SLOW_MS),
    }
    let mut seen = e.seen.lock().unwrap();
    match read {
        Ok((agents, groups)) => {
            seen.agents = agents.into_iter().map(|a| their_agent(a, &e.label)).collect();
            seen.groups = groups.into_iter().map(|g| theirs(g, &e.label)).collect();
            seen.ok = true;
            seen.error = None;
        }
        Err(_) if quiet => seen.ok = false,
        Err(err) => {
            let why = match err {
                RuntimeError::None => "its Herdr cannot be reached (no connection, or Herdr is not running there)".to_string(),
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

/// Sleeps `d`, waking early when `stop` is set or the computer has slept meanwhile.
fn nap(d: Duration, stop: &AtomicBool) {
    let until = Instant::now() + d;
    let mut clock = Clock::now();
    while Instant::now() < until && !stop.load(Ordering::Relaxed) && !clock.woke() {
        std::thread::sleep(Duration::from_millis(50).min(d));
    }
}

/// Tells that the computer slept: the wall clock went on while the monotonic one (which
/// stands still in sleep, on macOS and Linux alike) did not.
struct Clock {
    wall: SystemTime,
    mono: Instant,
}

impl Clock {
    fn now() -> Clock {
        Clock { wall: SystemTime::now(), mono: Instant::now() }
    }

    /// Whether it slept since the last look.
    fn woke(&mut self) -> bool {
        let now = Clock::now();
        let wall = now.wall.duration_since(self.wall).unwrap_or_default();
        let mono = now.mono.duration_since(self.mono);
        *self = now;
        slept(wall, mono)
    }
}

/// More wall-clock time passed than monotonic time, by more than a stall would explain.
pub fn slept(wall: Duration, mono: Duration) -> bool {
    wall > mono + Duration::from_secs(5)
}

impl Runtime for Machines {
    /// This machine's agents read fresh, and every followed machine's as last seen.
    fn agents(&self) -> Result<Vec<RuntimeAgent>, RuntimeError> {
        let entries = self.entries.lock().unwrap().clone();
        let mut all = match self.local.agents() {
            Ok(a) => a,
            Err(e) if entries.is_empty() => return Err(e),
            Err(_) => vec![],
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

    /// On the machine named, through its own runtime: the shell is the user's over there.
    fn run_shell(&self, machine: Option<&str>, group: &str, dir: &str, line: &str) -> Result<String, RuntimeError> {
        self.at(machine, |r| r.run_shell(None, group, dir, line))
    }

    fn read_place(&self, machine: Option<&str>, place: &str, lines: u32) -> Result<String, RuntimeError> {
        self.at(machine, |r| r.read_place(None, place, lines))
    }

    fn focus_place(&self, machine: Option<&str>, place: &str) -> Result<(), RuntimeError> {
        self.at(machine, |r| r.focus_place(None, place))
    }

    fn groups(&self) -> Result<Vec<RuntimeRef>, RuntimeError> {
        let entries = self.entries.lock().unwrap().clone();
        let mut all = match self.local.groups() {
            Ok(g) => g,
            Err(e) if entries.is_empty() => return Err(e),
            Err(_) => vec![],
        };
        for e in entries {
            all.extend(e.seen.lock().unwrap().groups.iter().cloned());
        }
        Ok(all)
    }
}

// ---- the connection: one ssh per machine, both forwards ----

/// The ssh that brings the machine's Herdr socket (`remote_socket`) to `local_socket` here
/// and forwards a port on the machine, which ssh picks and names on stderr ("Allocated
/// port N for remote forward…", hence LogLevel INFO whatever the config says), to this
/// daemon's `local_port`. It never asks for anything and ends if a forward cannot be set
/// up or the connection dies.
pub fn tunnel_words(target: &str, local_socket: &str, remote_socket: &str, local_port: u16) -> Vec<String> {
    let mut w: Vec<String> = vec!["ssh".into(), "-N".into()];
    for o in QUIET.iter().chain(&["ExitOnForwardFailure=yes", "StreamLocalBindUnlink=yes", "LogLevel=INFO"]) {
        w.extend(["-o".to_string(), o.to_string()]);
    }
    w.extend(["-L".to_string(), format!("{local_socket}:{remote_socket}")]);
    w.extend(["-R".to_string(), format!("127.0.0.1:0:127.0.0.1:{local_port}")]);
    w.extend(["--".to_string(), target.to_string()]);
    w
}

/// The port ssh says it got over there, from a line of its stderr.
pub fn allocated(line: &str) -> Option<u16> {
    line.trim().strip_prefix("Allocated port ")?.split_whitespace().next()?.parse().ok()
}

/// The supervisor's waits; shortened in tests.
#[derive(Clone, Copy, Debug)]
pub struct Pace {
    /// The wait before a restart: `min`, doubling to `max`; back to `min` after a run of `healthy`.
    pub min: Duration,
    pub max: Duration,
    pub healthy: Duration,
}

impl Default for Pace {
    fn default() -> Self {
        Pace { min: Duration::from_secs(2), max: Duration::from_secs(60), healthy: Duration::from_secs(60) }
    }
}

/// Keeps the command (the tunnel's ssh) running until `stop`: restarts it when it ends,
/// waiting longer after each quick failure (up to `pace.max`), and records in `tunnel`
/// whether it is up, on which port, and why it last ended. It is up once ssh has named the
/// port it got over there and `publish` has written it there. It is down the moment ssh
/// ends; after the computer slept it is started again at once, since its old connection is
/// most likely dead without knowing it.
pub fn supervise(words: &[String], tunnel: &Mutex<Tunnel>, stop: &AtomicBool, pace: Pace, publish: &dyn Fn(u16) -> Result<(), String>) {
    // For the timing log: which machine (ssh's destination is the last word), since when down.
    let machine = words.last().cloned().unwrap_or_default();
    let mut down_at = Instant::now();
    // The command runs under a shell that ends it when our end of its stdin closes, so it
    // does not outlive the daemon however the daemon ends.
    const WRAP: &str = r#"exec 3<&0; "$@" </dev/null & pid=$!; (cat <&3 >/dev/null 2>&1; kill $pid 2>/dev/null) 2>/dev/null & wait $pid"#;
    let mut delay = pace.min;
    while !stop.load(Ordering::Relaxed) {
        let started = Instant::now();
        let spawned = Command::new("sh").arg("-c").arg(WRAP).arg("sh").args(words).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn();
        let mut again_now = false;
        let why = match spawned {
            Err(e) => format!("cannot run {}: {e}", words[0]),
            Ok(mut child) => {
                // ssh's stderr, line by line, as it comes: the port it got is said there.
                let (tx, rx) = std::sync::mpsc::channel::<String>();
                if let Some(err) = child.stderr.take() {
                    std::thread::spawn(move || {
                        for line in std::io::BufRead::lines(std::io::BufReader::new(err)).map_while(Result::ok) {
                            if tx.send(line).is_err() {
                                break;
                            }
                        }
                    });
                }
                let mut said: Vec<String> = vec![];
                let mut clock = Clock::now();
                let mut failed: Option<String> = None;
                loop {
                    while let Ok(line) = rx.try_recv() {
                        match allocated(&line) {
                            Some(port) if failed.is_none() => match publish(port) {
                                Ok(()) => {
                                    *tunnel.lock().unwrap() = Tunnel { up: true, port: Some(port), ..Tunnel::default() };
                                    tracing::info!(target: "perf", kind = "tunnel", event = "up", machine = %machine, connect_ms = started.elapsed().as_millis() as u64, down_ms = down_at.elapsed().as_millis() as u64);
                                }
                                Err(why) => failed = Some(why),
                            },
                            _ => said.push(line),
                        }
                    }
                    if failed.is_some() || matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
                        break;
                    }
                    if stop.load(Ordering::Relaxed) {
                        let _ = child.kill();
                        let _ = child.wait();
                        *tunnel.lock().unwrap() = Tunnel::default();
                        return;
                    }
                    if clock.woke() {
                        failed = Some("the computer slept; connecting again".into());
                        again_now = true;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                let _ = child.kill();
                let status = child.wait().ok();
                // Closing stdin ends the watcher that still holds stderr open.
                drop(child.stdin.take());
                while let Ok(line) = rx.recv_timeout(Duration::from_millis(200)) {
                    if allocated(&line).is_none() {
                        said.push(line);
                    }
                }
                let last = said.iter().rev().find(|l| !l.trim().is_empty()).map(|l| l.trim().to_string());
                failed.or(last).unwrap_or_else(|| format!("ended ({})", status.map(|s| s.to_string()).unwrap_or_default()))
            }
        };
        delay = if again_now || started.elapsed() >= pace.healthy { pace.min } else { delay };
        let wait = if again_now { Duration::ZERO } else { delay };
        {
            let mut t = tunnel.lock().unwrap();
            if t.up || t.error.as_deref() != Some(why.as_str()) {
                tracing::warn!("tunnel `{}`: {why}", words.join(" "));
            }
            if t.up {
                tracing::info!(target: "perf", kind = "tunnel", event = "down", machine = %machine, up_ms = started.elapsed().as_millis() as u64, error = crate::perf::kind_of(&why));
                down_at = Instant::now();
            }
            *t = Tunnel { up: false, error: Some(why), port: None, since: Some(workriver_core::now()), retry: Some(Instant::now() + wait) };
        }
        nap(wait, stop);
        delay = (delay * 2).min(pace.max);
    }
    *tunnel.lock().unwrap() = Tunnel::default();
}

/// Where a file in the daemon's store is for an agent on a machine: through its tunnel, on
/// the port of the connection there is when it is fetched (a shell fills it in).
pub fn attachment_url(id: &str) -> String {
    format!("http://127.0.0.1:$(cat ~/{PORT_FILE})/api/attachments/{id}")
}

/// A message for an agent on another machine: every path into the attachment store
/// (`dir` + `<id>.<ext>`) becomes a link through its tunnel. The link carries the reader's
/// place as the shell fills it in, since the service answers no reader that is nobody.
pub fn links(text: &str, dir: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(k) = rest.find(dir) {
        out += &rest[..k];
        let after = &rest[k + dir.len()..];
        // The file's name: its id, a dot, its extension.
        let id = after.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(after.len());
        let ext = after[id..].strip_prefix('.').map(|e| 1 + e.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(e.len())).unwrap_or(0);
        out += &format!("\"{}?place=$HERDR_PANE_ID\" (fetch it with curl -fsSO)", attachment_url(&after[..id + ext]));
        rest = &after[id + ext..];
    }
    out + rest
}
