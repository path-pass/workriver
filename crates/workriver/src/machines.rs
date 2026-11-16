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
//! The list of machines is workriver's own, in `<home>/settings.json`.
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use workriver_core::RuntimeRef;
use workriver_core::runtime::{Place, Runtime, RuntimeAgent, RuntimeError};
use workriver_scan as scan;

use crate::herdr;

/// A machine as the user added it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Setting {
    /// What ssh takes as the destination: a Host of the user's config, `user@host`.
    pub ssh: String,
    /// Herdr's socket on that machine; none: asked of the machine when connecting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub herdr_socket: Option<String>,
    /// The port on that machine that leads back to this daemon.
    pub port: u16,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct Settings {
    /// By the machine's name.
    #[serde(default)]
    machines: BTreeMap<String, Setting>,
}

/// A machine as the page and `workriver machine list` show it.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MachineState {
    pub label: String,
    pub ssh: String,
    pub enabled: bool,
    pub port: u16,
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
/// that is not known yet, give up on a dead line.
const QUIET: [&str; 6] = ["BatchMode=yes", "ForwardAgent=no", "StrictHostKeyChecking=yes", "ConnectTimeout=10", "ServerAliveInterval=30", "ServerAliveCountMax=3"];

/// What to do about an ssh failure the user can fix, after ssh's own words.
fn hint(error: &str, ssh: &str) -> String {
    let e = error.to_lowercase();
    if e.contains("host key verification failed") || e.contains("no matching host key") || e.contains("remote host identification has changed") {
        format!("{error} (workriver takes no host key by itself: connect once by hand to check and accept it, `ssh {ssh}`)")
    } else if e.contains("forwarding") || e.contains("cannot listen") || e.contains("address already in use") {
        // ponytail: forwards from the user's config cannot be cleared without clearing ours
        // (ClearAllForwardings clears both); a Host alias of workriver's own is the way out.
        format!("{error} (if the port there is taken: `--remote-port`; if your ssh config sets LocalForward/RemoteForward for this host, they are set up on workriver's connection too and can fail it: give workriver a Host alias without them)")
    } else {
        error.to_string()
    }
}

impl Machines {
    pub fn new(local: Arc<dyn Runtime>, home: PathBuf, terminal_app: String) -> Arc<Machines> {
        Arc::new(Machines { local, home, terminal_app, entries: Mutex::default(), ports: Mutex::default() })
    }

    fn settings_path(&self) -> PathBuf {
        self.home.join("settings.json")
    }

    fn settings(&self) -> Settings {
        std::fs::read(self.settings_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn save(&self, s: &Settings) -> Result<(), String> {
        let text = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
        std::fs::write(self.settings_path(), text).map_err(|e| format!("cannot write {}: {e}", self.settings_path().display()))
    }

    /// Every machine with what is known of it now.
    pub fn list(&self) -> Vec<MachineState> {
        let running = self.entries.lock().unwrap().clone();
        self.settings()
            .machines
            .into_iter()
            .map(|(label, s)| match running.iter().find(|e| e.label == label) {
                Some(e) => state_of(e),
                None => MachineState { label, ssh: s.ssh, enabled: s.enabled, port: s.port, reachable: false, error: None, connecting: false, tunnel: false, tunnel_error: None, agents: 0 },
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
        let mut all = self.settings();
        if all.machines.contains_key(label) {
            return Err(format!("there is a machine {label} already; remove it first"));
        }
        all.machines.insert(label.into(), setting);
        self.save(&all)
    }

    /// Records whether a machine is followed; the caller starts or stops it.
    pub fn enable(&self, label: &str, enabled: bool) -> Result<Setting, String> {
        let mut all = self.settings();
        let s = all.machines.get_mut(label).ok_or_else(|| format!("no machine {label}; add it with `workriver machine add {label} --ssh <target>`"))?;
        s.enabled = enabled;
        let s = s.clone();
        self.save(&all)?;
        Ok(s)
    }

    pub fn remove(&self, label: &str) -> Result<(), String> {
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

    /// Starts following a machine: keeps its connection up (its Herdr socket here, its
    /// `port` leading to this daemon's `local_port`) and reads its agents, each on a thread.
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
                // Until the connection is up there is nothing to read and nothing to say
                // about it: look again soon, so its agents show as soon as it is.
                let up = e.tunnel.lock().unwrap().up;
                delay = if poll_once(&e, !up) {
                    OK
                } else if !up {
                    Duration::from_secs(1)
                } else {
                    (delay * 2).min(MAX)
                };
                nap(delay, &e.stop);
            }
        });
        std::thread::spawn(move || {
            let e = entry;
            // Where Herdr's socket is over there: as given, else asked of the machine.
            let mut delay = Pace::default().min;
            let remote = loop {
                if e.stop.load(Ordering::Relaxed) {
                    return;
                }
                match e.setting.herdr_socket.clone().map(Ok).unwrap_or_else(|| ask_socket(&e.setting.ssh)) {
                    Ok(p) => break p,
                    Err(why) => {
                        tracing::warn!("machine {}: {why}", e.label);
                        *e.tunnel.lock().unwrap() = Tunnel { up: false, error: Some(why) };
                        delay = (delay * 2).min(Pace::default().max);
                        nap(delay, &e.stop);
                    }
                }
            };
            let words = tunnel_words(&e.setting.ssh, &socket.to_string_lossy(), &remote, e.setting.port, local_port);
            supervise(&words, &e.tunnel, &e.stop, Pace::default());
            let _ = std::fs::remove_file(&socket);
        });
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
        self.entry(machine).filter(|e| e.seen.lock().unwrap().ok).map(|e| e.runner.clone())
    }

    /// The port on a machine that leads to this daemon.
    pub fn port(&self, machine: &str) -> Option<u16> {
        self.entry(machine).map(|e| e.setting.port)
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
            return f(e.runtime.as_ref());
        }
        match f(self.local.as_ref()) {
            Err(RuntimeError::NoAgent(_) | RuntimeError::None) => match self.owner(agent, true) {
                Some(e) => f(e.runtime.as_ref()),
                None => Err(RuntimeError::NoAgent(agent.into())),
            },
            r => r,
        }
    }

    /// For tests: machines whose runtimes are given, read once, with no threads or ssh.
    #[cfg(test)]
    pub fn with(local: Arc<dyn Runtime>, remotes: Vec<(&str, Arc<dyn Runtime>)>) -> Arc<Machines> {
        let m = Machines::new(local, PathBuf::new(), String::new());
        for (label, runtime) in remotes {
            let setting = Setting { ssh: label.into(), herdr_socket: None, port: 47820, enabled: true };
            let e = Arc::new(Entry { label: label.into(), setting, runtime, runner: scan::Runner::Local, seen: Mutex::default(), tunnel: Mutex::default(), stop: AtomicBool::new(false) });
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
        entries[k] = Arc::new(Entry { label: old.label.clone(), setting: old.setting.clone(), runtime: old.runtime.clone(), runner, seen: Mutex::new(std::mem::take(&mut *old.seen.lock().unwrap())), tunnel: Mutex::default(), stop: AtomicBool::new(false) });
    }

    #[cfg(test)]
    pub fn poll_all(&self) {
        for e in self.entries.lock().unwrap().iter() {
            poll_once(e, false);
        }
    }
}

fn ask_socket(ssh: &str) -> Result<String, String> {
    let mut cmd = Command::new("ssh");
    for o in QUIET {
        cmd.args(["-o", o]);
    }
    // This one sets up no forward, so the config's are cleared.
    cmd.args(["-o", "ClearAllForwardings=yes", "--", ssh, ASK_SOCKET]);
    let out = herdr::output_within(cmd, Duration::from_secs(20)).map_err(|e| format!("cannot run ssh: {e}"))?;
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
        port: e.setting.port,
        reachable: seen.ok,
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
    let read = e.runtime.agents().and_then(|a| Ok((a, e.runtime.groups()?)));
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

/// Sleeps `d`, waking early when `stop` is set.
fn nap(d: Duration, stop: &AtomicBool) {
    let until = Instant::now() + d;
    while Instant::now() < until && !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(50).min(d));
    }
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
            all.extend(e.seen.lock().unwrap().agents.iter().cloned());
        }
        Ok(all)
    }

    fn locate(&self, machine: Option<&str>, place: &str) -> Result<Place, RuntimeError> {
        match machine {
            None => self.local.locate(None, place),
            Some(m) => {
                let e = self.entry(m).ok_or_else(|| RuntimeError::Failed(format!("machine {m} is not followed; enable it with `workriver machine enable`")))?;
                let at = e.runtime.locate(None, place)?;
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
                let a = their_agent(e.runtime.start_temporary(id, harness, name, cwd)?, &e.label);
                // Known at once, so a message to it by name finds its machine before the next read.
                e.seen.lock().unwrap().agents.push(a.clone());
                Ok(a)
            }
        }
    }

    fn close(&self, agent: &str) -> Result<(), RuntimeError> {
        self.route(agent, |r| r.close(agent))
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
/// and forwards `port` on the machine to this daemon's `local_port`. It never asks for
/// anything and ends if a forward cannot be set up or the connection dies.
pub fn tunnel_words(target: &str, local_socket: &str, remote_socket: &str, port: u16, local_port: u16) -> Vec<String> {
    let mut w: Vec<String> = vec!["ssh".into(), "-N".into()];
    for o in QUIET.iter().chain(&["ExitOnForwardFailure=yes", "StreamLocalBindUnlink=yes"]) {
        w.extend(["-o".to_string(), o.to_string()]);
    }
    w.extend(["-L".to_string(), format!("{local_socket}:{remote_socket}")]);
    w.extend(["-R".to_string(), format!("127.0.0.1:{port}:127.0.0.1:{local_port}")]);
    w.extend(["--".to_string(), target.to_string()]);
    w
}

/// The supervisor's waits; shortened in tests.
#[derive(Clone, Copy)]
pub struct Pace {
    /// Running this long without ending counts as up.
    pub settle: Duration,
    /// The wait before a restart: `min`, doubling to `max`; back to `min` after a run of `healthy`.
    pub min: Duration,
    pub max: Duration,
    pub healthy: Duration,
}

impl Default for Pace {
    fn default() -> Self {
        Pace { settle: Duration::from_secs(3), min: Duration::from_secs(2), max: Duration::from_secs(120), healthy: Duration::from_secs(60) }
    }
}

/// Keeps the command running until `stop`: restarts it when it ends, waiting longer after
/// each quick failure, and records in `tunnel` whether it is up and why it last ended.
pub fn supervise(words: &[String], tunnel: &Mutex<Tunnel>, stop: &AtomicBool, pace: Pace) {
    // The command runs under a shell that ends it when our end of its stdin closes, so it
    // does not outlive the daemon however the daemon ends.
    const WRAP: &str = r#"exec 3<&0; "$@" </dev/null & pid=$!; (cat <&3 >/dev/null 2>&1; kill $pid 2>/dev/null) 2>/dev/null & wait $pid"#;
    let mut delay = pace.min;
    while !stop.load(Ordering::Relaxed) {
        let started = Instant::now();
        let spawned = Command::new("sh").arg("-c").arg(WRAP).arg("sh").args(words).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn();
        let why = match spawned {
            Err(e) => format!("cannot run {}: {e}", words[0]),
            Ok(mut child) => {
                let status = loop {
                    match child.try_wait() {
                        Ok(Some(s)) => break Some(s),
                        Err(_) => break None,
                        Ok(None) => {}
                    }
                    if stop.load(Ordering::Relaxed) {
                        let _ = child.kill();
                        let _ = child.wait();
                        *tunnel.lock().unwrap() = Tunnel::default();
                        return;
                    }
                    if started.elapsed() >= pace.settle {
                        let mut t = tunnel.lock().unwrap();
                        if !t.up {
                            *t = Tunnel { up: true, error: None };
                        }
                    }
                    std::thread::sleep(Duration::from_millis(20));
                };
                // Closing stdin ends the watcher that still holds stderr open.
                drop(child.stdin.take());
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() {
                    let _ = std::io::Read::read_to_string(&mut e, &mut err);
                }
                let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
                if last.is_empty() { format!("ended ({})", status.map(|s| s.to_string()).unwrap_or_default()) } else { last }
            }
        };
        {
            let mut t = tunnel.lock().unwrap();
            if t.up || t.error.as_deref() != Some(why.as_str()) {
                tracing::warn!("tunnel `{}`: {why}", words.join(" "));
            }
            *t = Tunnel { up: false, error: Some(why) };
        }
        delay = if started.elapsed() >= pace.healthy { pace.min } else { (delay * 2).min(pace.max) };
        nap(delay, stop);
    }
}

/// Where a file in the daemon's store is for an agent on a machine: through its tunnel.
pub fn attachment_url(port: u16, id: &str) -> String {
    format!("http://127.0.0.1:{port}/api/attachments/{id}")
}

/// A message for an agent on another machine: every path into the attachment store
/// (`dir` + `<id>.<ext>`) becomes a link through its tunnel. The link carries the reader's
/// place as the shell fills it in, since the service answers no reader that is nobody.
pub fn links(text: &str, dir: &str, port: u16) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(k) = rest.find(dir) {
        out += &rest[..k];
        let after = &rest[k + dir.len()..];
        // The file's name: its id, a dot, its extension.
        let id = after.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(after.len());
        let ext = after[id..].strip_prefix('.').map(|e| 1 + e.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(e.len())).unwrap_or(0);
        out += &format!("\"{}?place=$HERDR_PANE_ID\" (fetch it with curl -fsSO)", attachment_url(port, &after[..id + ext]));
        rest = &after[id + ext..];
    }
    out + rest
}
