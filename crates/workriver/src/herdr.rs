// The Herdr runtime: Herdr's socket API in workriver's terms. The only place that knows
// Herdr's names (workspace, pane), its socket and its environment variables. Elsewhere an
// agent is its session id and a workspace is a runtime group.
//
// Protocol: newline-delimited JSON over a Unix socket, one request per connection
// (`{"id","method","params"}` -> `{"id","result"}` or `{"id","error":{"code","message"}}`).
//
// Another machine has its own Herdr server; its socket, forwarded here over SSH (see
// machines.rs), speaks the same protocol, so the same client talks to it. Ids are per
// server: everything read from another machine carries the machine's name.
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use workriver_core::RuntimeRef;
use workriver_core::runtime::{AgentState, Place, Runtime, RuntimeAgent, RuntimeError};

/// The protocol this was written against (Herdr 0.9).
pub const PROTOCOL: u64 = 22;

/// Where the CLI runs, when it runs in a Herdr pane: the pane id, as an opaque place.
pub fn place_from_env() -> Option<String> {
    std::env::var("HERDR_PANE_ID").ok().filter(|s| !s.is_empty())
}

/// Why the CLI at `place` cannot tell which agent it is.
pub fn no_agent_at(place: &str) -> String {
    format!("Herdr lists no agent in pane {place} (see `herdr agent list`); an agent runs this from its own pane; the user, from a terminal that is not an agent's, on the machine workriver runs on (where the user token is).")
}

/// The same when the service cannot ask Herdr.
pub fn cannot_ask(place: &str, why: &str) -> String {
    format!("cannot tell which agent runs in Herdr pane {place}: {why}. Is Herdr running and `workriver serve` started inside it? (As the user: use a terminal outside Herdr.)")
}

/// How an agent may ask another agent one question through Herdr's own CLI.
pub fn ask_hint(agent: &str) -> String {
    format!(
        "If {agent} (the agent that did the work) is idle - check with `herdr agent get {agent}` - you may ask it one short question with `herdr agent prompt {agent} \"...\" --wait --timeout 120000` and read the reply with `herdr agent read {agent} --source recent-unwrapped --lines 80`; never while it is working or blocked. If you asked it, add `--asked {agent}` to your answer."
    )
}

/// How a manager looks at a stopped worker's terminal and gets it going, through Herdr's own CLI.
pub fn stall_hint(agent: &str) -> String {
    format!("Look at its terminal (`herdr agent read {agent} --source recent-unwrapped --lines 80`) and either get it going (`herdr agent prompt {agent} \"...\"`),")
}

/// `HERDR_SOCKET_PATH`, or Herdr's default socket.
pub fn socket_path() -> PathBuf {
    match std::env::var_os("HERDR_SOCKET_PATH") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => std::env::home_dir().unwrap_or_default().join(".config/herdr/herdr.sock"),
    }
}

pub struct Herdr {
    socket: PathBuf,
    /// The terminal app brought to the front after a focus (macOS); empty: none.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    terminal_app: String,
    ids: AtomicU64,
    /// Session -> pane, as last read.
    panes: Mutex<HashMap<String, String>>,
    /// Set for another machine's Herdr: its name, carried by every id read from it.
    machine: Option<String>,
}

/// A workspace id of a machine, told apart from this machine's: `w1@label`.
pub fn remote_ref(id: &str, machine: &str) -> String {
    format!("{id}@{machine}")
}

/// Runs a command to its end, or kills it after `wait`.
pub fn output_within(mut cmd: std::process::Command, wait: Duration) -> std::io::Result<std::process::Output> {
    use std::process::Stdio;
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let until = std::time::Instant::now() + wait;
    // ponytail: output is read after exit, fine for answers that fit a pipe buffer only if
    // drained; so drain on threads.
    let (mut out, mut err) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let o = std::thread::spawn(move || { let mut b = vec![]; let _ = std::io::Read::read_to_end(&mut out, &mut b); b });
    let e = std::thread::spawn(move || { let mut b = vec![]; let _ = std::io::Read::read_to_end(&mut err, &mut b); b });
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if std::time::Instant::now() > until {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, format!("no answer within {}s", wait.as_secs())));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Ok(std::process::Output { status, stdout: o.join().unwrap_or_default(), stderr: e.join().unwrap_or_default() })
}

#[derive(Deserialize)]
struct Snap {
    #[serde(default)]
    workspaces: Vec<Workspace>,
    #[serde(default)]
    panes: Vec<Pane>,
    #[serde(default)]
    agents: Vec<AgentInfo>,
}

#[derive(Deserialize)]
struct Workspace {
    workspace_id: String,
    #[serde(default)]
    label: String,
}

#[derive(Deserialize)]
struct Pane {
    pane_id: String,
    workspace_id: String,
    #[serde(default)]
    label: Option<String>,
}

#[derive(Deserialize)]
struct AgentInfo {
    pane_id: String,
    workspace_id: String,
    #[serde(default)]
    agent_status: String,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    agent_session: Option<Session>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    foreground_cwd: Option<String>,
    /// Still launching / ready for input: in Herdr's schema, not described; absent says nothing.
    #[serde(default)]
    launch_pending: Option<bool>,
    #[serde(default)]
    interactive_ready: Option<bool>,
    /// The terminal title without decoration: what the session is about.
    #[serde(default)]
    terminal_title_stripped: Option<String>,
}

#[derive(Deserialize)]
struct Session {
    value: String,
}

impl Herdr {
    pub fn new(socket: PathBuf, terminal_app: String) -> Herdr {
        Herdr { socket, terminal_app, ids: AtomicU64::new(1), panes: Mutex::default(), machine: None }
    }

    /// The Herdr of another machine, behind a socket forwarded from it.
    pub fn on(socket: PathBuf, machine: &str, terminal_app: String) -> Herdr {
        Herdr { socket, terminal_app, ids: AtomicU64::new(1), panes: Mutex::default(), machine: Some(machine.into()) }
    }

    /// One request on its own connection. A missing socket means Herdr is not running.
    fn call(&self, method: &str, params: Value) -> Result<Value, RuntimeError> {
        self.call_within(method, params, Duration::from_secs(10))
    }

    /// The same, for a request that answers only after `wait` at most (starting an agent).
    fn call_within(&self, method: &str, params: Value, wait: Duration) -> Result<Value, RuntimeError> {
        let fail = |e: std::io::Error| RuntimeError::Failed(format!("herdr {method}: {e}"));
        let stream = UnixStream::connect(&self.socket).map_err(|_| RuntimeError::None)?;
        stream.set_read_timeout(Some(wait)).map_err(fail)?;
        stream.set_write_timeout(Some(Duration::from_secs(5))).map_err(fail)?;
        let id = format!("workriver:{}", self.ids.fetch_add(1, Ordering::Relaxed));
        let mut line = json!({ "id": id, "method": method, "params": params }).to_string();
        line.push('\n');
        (&stream).write_all(line.as_bytes()).map_err(fail)?;
        let mut answer = String::new();
        BufReader::new(&stream).read_line(&mut answer).map_err(fail)?;
        let v: Value = serde_json::from_str(&answer).map_err(|e| RuntimeError::Failed(format!("herdr {method}: unreadable answer: {e}")))?;
        if let Some(e) = v.get("error") {
            let (message, code) = (e["message"].as_str().unwrap_or("?"), e["code"].as_str().unwrap_or("?"));
            let words = format!("herdr {method}: {message} ({code})");
            return Err(match code {
                "agent_blocked" => RuntimeError::Blocked(words),
                "agent_not_ready" => RuntimeError::NotReady(words),
                _ => RuntimeError::Failed(words),
            });
        }
        v.get("result").cloned().ok_or_else(|| RuntimeError::Failed(format!("herdr {method}: answer without a result")))
    }

    /// Herdr focuses a pane inside its window; the window's app is brought forward here (macOS).
    /// A failure only means the user switches to it by hand.
    fn activate(&self) {
        #[cfg(target_os = "macos")]
        if !self.terminal_app.is_empty() && !self.terminal_app.contains('"') {
            let _ = std::process::Command::new("osascript").args(["-e", &format!("tell application \"{}\" to activate", self.terminal_app)]).output();
        }
    }

    /// A line on what is wrong with the connection, or None when Herdr answers with our protocol.
    pub fn check(&self) -> Option<String> {
        match self.call("ping", json!({})) {
            Ok(r) if r["protocol"].as_u64() == Some(PROTOCOL) => None,
            Ok(r) => Some(format!("Herdr {} speaks protocol {}, workriver expects {PROTOCOL}", r["version"].as_str().unwrap_or("?"), r["protocol"])),
            Err(RuntimeError::None) => Some(format!("Herdr is not running (no socket at {})", self.socket.display())),
            Err(e) => Some(e.to_string()),
        }
    }

    fn snapshot(&self) -> Result<Snap, RuntimeError> {
        let r = self.call("session.snapshot", json!({}))?;
        serde_json::from_value(r["snapshot"].clone()).map_err(|e| RuntimeError::Failed(format!("herdr session.snapshot: {e}")))
    }

    /// The live agents with the pane each sits in; also remembers session -> pane.
    fn read(&self) -> Result<(Snap, Vec<(String, RuntimeAgent)>), RuntimeError> {
        let snap = self.snapshot()?;
        let list = agents_of(&snap, self.label());
        *self.panes.lock().unwrap() = list.iter().map(|(p, a)| (a.session.clone(), p.clone())).collect();
        Ok((snap, list))
    }

    fn label(&self) -> Option<&str> {
        self.machine.as_deref()
    }

    /// The pane of an agent given by session or by name.
    fn pane_of(&self, agent: &str) -> Result<String, RuntimeError> {
        if let Some(p) = self.panes.lock().unwrap().get(agent) {
            return Ok(p.clone());
        }
        let (_, list) = self.read()?;
        list.into_iter().find(|(_, a)| a.session == agent || a.name == agent).map(|(p, _)| p).ok_or_else(|| RuntimeError::NoAgent(agent.into()))
    }
}

/// A workspace as a runtime group. Ids are per Herdr server, so another machine's carry its name.
fn group(snap: &Snap, workspace: &str, machine: Option<&str>) -> Option<RuntimeRef> {
    let id = |w: &str| machine.map(|m| remote_ref(w, m)).unwrap_or_else(|| w.to_string());
    snap.workspaces.iter().find(|w| w.workspace_id == workspace).map(|w| RuntimeRef { id: id(&w.workspace_id), label: w.label.clone(), machine: machine.map(str::to_string) })
}

/// Panes with an agent in them, as workriver's agents. The name is the agent's name in Herdr
/// (`herdr agent start <name>` / `herdr agent rename`), else the pane's label, else
/// `<harness> <n>`, numbered within its workspace in pane order.
fn agents_of(snap: &Snap, machine: Option<&str>) -> Vec<(String, RuntimeAgent)> {
    let set = |s: &Option<String>| s.clone().filter(|s| !s.trim().is_empty());
    let pane_no = |p: &str| p.rsplit_once(":p").and_then(|(_, n)| n.parse::<u64>().ok()).unwrap_or(u64::MAX);
    let mut list: Vec<(&AgentInfo, String, Option<String>)> = snap
        .agents
        .iter()
        .filter_map(|a| {
            let harness = set(&a.agent)?;
            let label = snap.panes.iter().find(|p| p.pane_id == a.pane_id).and_then(|p| set(&p.label));
            Some((a, harness, set(&a.name).or(label)))
        })
        .collect();
    list.sort_by_key(|(a, _, _)| (a.workspace_id.clone(), pane_no(&a.pane_id)));
    let mut counts: std::collections::HashMap<(String, String), u32> = Default::default();
    list.into_iter()
        .map(|(a, harness, name)| {
            let name = name.unwrap_or_else(|| {
                let n = counts.entry((a.workspace_id.clone(), harness.clone())).or_default();
                *n += 1;
                format!("{harness} {n}")
            });
            let agent = RuntimeAgent {
                // Without a session id of the harness's own, the pane is the ref; a machine's
                // pane ids repeat this machine's, so they carry its label.
                session: a.agent_session.as_ref().map(|s| s.value.clone()).unwrap_or_else(|| match machine {
                    Some(m) => format!("herdr:{}", remote_ref(&a.pane_id, m)),
                    None => format!("herdr:{}", a.pane_id),
                }),
                machine: machine.map(str::to_string),
                state: match a.agent_status.as_str() {
                    "working" => AgentState::Working,
                    "idle" => AgentState::Idle,
                    "done" => AgentState::Done,
                    "blocked" => AgentState::Blocked,
                    _ => AgentState::Unknown,
                },
                cwd: set(&a.foreground_cwd).or_else(|| set(&a.cwd)).unwrap_or_default(),
                group: group(snap, &a.workspace_id, machine),
                topic: set(&a.terminal_title_stripped),
                harness,
                name,
            };
            (a.pane_id.clone(), agent)
        })
        .collect()
}

impl Herdr {
    /// A new tab labelled `label` in the workspace labelled `group`, or the first tab of that
    /// workspace, made now when there is none; Herdr's answer, which names its root pane.
    fn tab_in(&self, group: &str, dir: &str, label: &str) -> Result<Value, RuntimeError> {
        let snap = self.snapshot()?;
        match snap.workspaces.iter().find(|w| w.label == group) {
            Some(w) => self.call("tab.create", json!({ "workspace_id": w.workspace_id, "cwd": dir, "label": label, "focus": false })),
            None => self.call("workspace.create", json!({ "label": group, "cwd": dir, "focus": false })),
        }
    }

    /// That pane `place` is still a run's shell: there, in the workspace labelled `group`, with
    /// no agent in it. Read fresh each time and never taken from the id alone: an id kept for
    /// days may name another pane by then (whether a restarted Herdr numbers afresh is not known).
    fn run_pane(&self, group: &str, place: &str) -> Result<(), RuntimeError> {
        let snap = self.snapshot()?;
        let pane = snap.panes.iter().find(|p| p.pane_id == place).ok_or_else(|| RuntimeError::NoPlace(format!("herdr: no pane {place}")))?;
        let theirs = snap.workspaces.iter().any(|w| w.workspace_id == pane.workspace_id && w.label == group);
        if !theirs || snap.agents.iter().any(|a| a.pane_id == place) {
            return Err(RuntimeError::NoPlace(format!("herdr: pane {place} is no longer a shell in {group}")));
        }
        Ok(())
    }

    /// Starts `harness` as `name` in the root pane of a tab just made (`tab`, Herdr's answer).
    fn launch(&self, tab: &Value, harness: &str, name: &str) -> Result<RuntimeAgent, RuntimeError> {
        let pane = tab["root_pane"]["pane_id"].as_str().ok_or_else(|| RuntimeError::Failed("herdr tab.create: no pane in the answer".into()))?.to_string();
        let started = self.call_within("agent.start", json!({ "name": name, "kind": harness, "pane_id": pane, "timeout_ms": 60_000 }), Duration::from_secs(75));
        // Herdr may answer before the agent is ready for a prompt; wait until it shows a state
        // and, where Herdr says so (AgentInfo's `launch_pending` / `interactive_ready`, in its
        // schema), its launch is over. Blocked is taken at once: it waits for a person, and the
        // caller holds its message. One still not ready after a minute may sit on a first-run
        // prompt ("trust this folder?") Herdr does not report: it is returned as it is, its tab
        // left open for the user; the caller holds the message until it is free. Only an agent
        // that never showed up in the pane is given up, and its tab closed.
        let found = started.and_then(|_| {
            let mut seen = None;
            for _ in 0..120 {
                let (snap, list) = self.read()?;
                let launched = snap.agents.iter().find(|i| i.pane_id == pane).is_some_and(|i| i.launch_pending != Some(true) && i.interactive_ready != Some(false));
                seen = list.into_iter().find(|(p, _)| *p == pane).map(|(_, a)| a);
                if let Some(a) = seen.take_if(|a| a.state == AgentState::Blocked || (a.state != AgentState::Unknown && launched)) {
                    return Ok(a);
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            seen.ok_or_else(|| RuntimeError::NoAgent(name.into()))
        });
        if found.is_err() {
            let _ = self.call("pane.close", json!({ "pane_id": pane }));
        }
        found
    }
}

impl Runtime for Herdr {
    fn agents(&self) -> Result<Vec<RuntimeAgent>, RuntimeError> {
        Ok(self.read()?.1.into_iter().map(|(_, a)| a).collect())
    }

    fn locate(&self, _: Option<&str>, place: &str) -> Result<Place, RuntimeError> {
        let (snap, list) = self.read()?;
        let agent = list.into_iter().find(|(p, _)| p == place).map(|(_, a)| a);
        let group = snap.panes.iter().find(|p| p.pane_id == place).and_then(|p| group(&snap, &p.workspace_id, self.label()));
        Ok(Place { agent, group })
    }

    fn focus(&self, session: &str) -> Result<(), RuntimeError> {
        let pane = self.pane_of(session)?;
        // On another machine this focuses the pane in that machine's Herdr; nothing in Herdr's
        // API makes this machine's window switch to it, so the user picks it in the sidebar.
        self.call("agent.focus", json!({ "target": pane }))?;
        self.activate();
        Ok(())
    }

    fn send(&self, agent: &str, text: &str) -> Result<(), RuntimeError> {
        let pane = self.pane_of(agent)?;
        self.call("agent.prompt", json!({ "target": pane, "text": text }))?;
        Ok(())
    }

    /// A new tab in the workspace, so nobody's pane is split; the agent starts in its shell.
    fn start_temporary(&self, group: &str, harness: &str, name: &str, cwd: &str) -> Result<RuntimeAgent, RuntimeError> {
        let tab = self.call("tab.create", json!({ "workspace_id": group, "cwd": cwd, "label": name, "focus": false }))?;
        self.launch(&tab, harness, name)
    }

    /// A tab of its own in the workspace labelled `group`, as a run's shell gets.
    fn start_temporary_at(&self, _: Option<&str>, group: &str, harness: &str, name: &str, cwd: &str) -> Result<RuntimeAgent, RuntimeError> {
        let tab = self.tab_in(group, cwd, name)?;
        self.launch(&tab, harness, name)
    }

    fn close(&self, agent: &str) -> Result<(), RuntimeError> {
        let pane = self.pane_of(agent)?;
        self.call("pane.close", json!({ "pane_id": pane }))?;
        Ok(())
    }

    /// One `pane.process_info` per agent pane: its shell, and the foreground processes in
    /// case Herdr knows no shell. A pane that does not answer has no ids.
    fn processes(&self, _: Option<&str>) -> Result<Vec<(RuntimeAgent, Vec<u32>)>, RuntimeError> {
        let (_, list) = self.read()?;
        Ok(list
            .into_iter()
            .map(|(pane, agent)| {
                let info = self.call("pane.process_info", json!({ "pane_id": pane })).map(|r| r["process_info"].clone()).unwrap_or_default();
                let fg = info["foreground_processes"].as_array().into_iter().flatten().filter_map(|p| p["pid"].as_u64());
                let pids = info["shell_pid"].as_u64().into_iter().chain(fg).map(|p| p as u32).collect();
                (agent, pids)
            })
            .collect())
    }

    fn groups(&self) -> Result<Vec<RuntimeRef>, RuntimeError> {
        let snap = self.snapshot()?;
        Ok(snap.workspaces.iter().filter_map(|w| group(&snap, &w.workspace_id, self.label())).collect())
    }

    /// A tab of its own in the workspace labelled `group` (made, with its first tab, when there
    /// is none): a run never types into a shell an earlier run may still hold (a password
    /// prompt, a pager), and each run's output starts clean. Herdr queues the text and Enter
    /// together, so they wait in the terminal until the new shell reads them.
    fn run_shell(&self, _: Option<&str>, group: &str, dir: &str, line: &str) -> Result<String, RuntimeError> {
        let made = self.tab_in(group, dir, "run")?;
        let pane = made["root_pane"]["pane_id"].as_str().ok_or_else(|| RuntimeError::Failed("herdr: no pane in the answer".into()))?.to_string();
        self.call("pane.send_input", json!({ "pane_id": pane, "text": line, "keys": ["enter"] }))?;
        Ok(pane)
    }

    fn shell_processes(&self, _: Option<&str>, group: &str, place: &str) -> Result<(u32, Vec<u32>), RuntimeError> {
        self.run_pane(group, place)?;
        let info = self.call("pane.process_info", json!({ "pane_id": place }))?["process_info"].clone();
        let shell = info["shell_pid"].as_u64().ok_or_else(|| RuntimeError::Failed(format!("herdr pane.process_info: no shell known in {place}")))?;
        let front = info["foreground_processes"].as_array().into_iter().flatten().filter_map(|p| p["pid"].as_u64()).map(|p| p as u32).collect();
        Ok((shell as u32, front))
    }

    /// A run's place is the one pane of its tab: closed as a temporary agent's is.
    fn close_shell(&self, _: Option<&str>, group: &str, place: &str) -> Result<(), RuntimeError> {
        self.run_pane(group, place)?;
        self.call("pane.close", json!({ "pane_id": place }))?;
        Ok(())
    }

    fn read_place(&self, _: Option<&str>, place: &str, lines: u32) -> Result<String, RuntimeError> {
        let r = self.call("pane.read", json!({ "pane_id": place, "source": "recent_unwrapped", "lines": lines }))?;
        Ok(r["read"]["text"].as_str().unwrap_or_default().to_string())
    }

    fn focus_place(&self, _: Option<&str>, place: &str) -> Result<(), RuntimeError> {
        self.call("pane.focus", json!({ "pane_id": place }))?;
        self.activate();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixListener;

    use super::*;

    /// A stand-in for Herdr's socket: answers each connection's one request with the next
    /// reply, checks nothing more comes on it, and hands back the requests.
    fn fake(replies: Vec<Value>) -> (tempfile::TempDir, Herdr, std::thread::JoinHandle<Vec<Value>>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let mut seen = vec![];
            for mut reply in replies {
                let (stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let req: Value = serde_json::from_str(&line).unwrap();
                reply["id"] = req["id"].clone();
                (&stream).write_all(format!("{reply}\n").as_bytes()).unwrap();
                let mut more = String::new();
                assert_eq!(reader.read_line(&mut more).unwrap(), 0, "one request per connection");
                seen.push(req);
            }
            seen
        });
        let herdr = Herdr::new(path, String::new());
        (dir, herdr, server)
    }

    fn snapshot() -> Value {
        json!({ "result": { "type": "session_snapshot", "snapshot": {
            "version": "0.9.0", "protocol": 22,
            "workspaces": [{ "workspace_id": "w5", "label": "chat", "number": 1 }],
            "panes": [
                { "pane_id": "w5:p1", "workspace_id": "w5", "label": "lead" },
                { "pane_id": "w5:p2", "workspace_id": "w5", "label": "side" },
                { "pane_id": "w5:p3", "workspace_id": "w5" }
            ],
            "agents": [
                { "pane_id": "w5:p1", "workspace_id": "w5", "agent": "claude", "agent_status": "working",
                  "agent_session": { "agent": "claude", "kind": "id", "source": "herdr:claude", "value": "s-1" },
                  "cwd": "/r", "foreground_cwd": "/r/sub", "unknown_field": 1 },
                { "pane_id": "w5:p2", "workspace_id": "w5", "agent": "codex", "name": "reviewer", "agent_status": "done" },
                { "pane_id": "w5:p3", "workspace_id": "w5", "agent": null, "agent_status": "unknown" },
                { "pane_id": "w5:p12", "workspace_id": "w5", "agent": "claude", "agent_status": "idle", "terminal_title_stripped": "Fix the login" },
                { "pane_id": "w5:p4", "workspace_id": "w5", "agent": "claude", "agent_status": "idle" }
            ]
        }}})
    }

    #[test]
    fn reads_agents_one_request_per_connection() {
        let (_dir, herdr, server) = fake(vec![snapshot(), snapshot()]);
        let list = herdr.agents().unwrap();
        assert_eq!(list.len(), 4, "a pane without an agent is no agent");
        // Unnamed agents are numbered per harness within their workspace, in pane order.
        let named: Vec<(&str, Option<&str>)> = list.iter().map(|a| (a.name.as_str(), a.topic.as_deref())).collect();
        assert_eq!(named[2..], [("claude 1", None), ("claude 2", Some("Fix the login"))]);
        let a = &list[0];
        assert_eq!((a.session.as_str(), a.name.as_str(), a.harness.as_str(), a.cwd.as_str()), ("s-1", "lead", "claude", "/r/sub"));
        assert_eq!(a.state, AgentState::Working);
        assert_eq!(a.group, Some(RuntimeRef { id: "w5".into(), label: "chat".into(), machine: None }));
        let b = &list[1];
        assert_eq!((b.session.as_str(), b.name.as_str(), b.state), ("herdr:w5:p2", "reviewer", AgentState::Done), "the Herdr name before the pane label");
        // A shell pane: its workspace, no agent.
        let shell = herdr.locate(None, "w5:p3").unwrap();
        assert_eq!((shell.agent, shell.group.map(|g| g.label)), (None, Some("chat".into())));
        let seen = server.join().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0]["method"], "session.snapshot");
        assert_ne!(seen[0]["id"], seen[1]["id"]);
    }

    #[test]
    fn sends_to_the_sessions_pane_and_reports_errors() {
        let error = json!({ "error": { "code": "agent_blocked", "message": "agent is blocked" } });
        let (_dir, herdr, server) = fake(vec![snapshot(), json!({ "result": { "type": "agent_prompted", "agent": {} } }), error]);
        herdr.send("s-1", "hello").unwrap();
        // Blocked is its own error: the message waits rather than fails.
        let err = herdr.send("s-1", "again").unwrap_err();
        assert!(matches!(&err, RuntimeError::Blocked(w) if w.contains("agent_blocked") && w.contains("agent is blocked")), "{err}");
        let seen = server.join().unwrap();
        assert_eq!(seen[1]["method"], "agent.prompt");
        assert_eq!(seen[1]["params"], json!({ "target": "w5:p1", "text": "hello" }));
    }

    /// A run's shell is closed by its pane, the place `run_shell` gave, and only while that
    /// pane is still a plain shell in the runs' workspace: one that is gone, sits in another
    /// workspace or holds an agent by now is refused, and nothing is closed.
    #[test]
    fn closes_only_a_runs_shell() {
        let ok = || json!({ "result": { "type": "ok" } });
        let snap = || {
            let mut s = snapshot();
            let inner = &mut s["result"]["snapshot"];
            inner["workspaces"].as_array_mut().unwrap().push(json!({ "workspace_id": "w7", "label": "workriver-commands", "number": 2 }));
            inner["panes"].as_array_mut().unwrap().extend([json!({ "pane_id": "w7:p3", "workspace_id": "w7", "label": "run" }), json!({ "pane_id": "w7:p4", "workspace_id": "w7", "label": "review-ab" })]);
            inner["agents"].as_array_mut().unwrap().push(json!({ "pane_id": "w7:p4", "workspace_id": "w7", "agent": "claude", "name": "review-ab", "agent_status": "idle" }));
            s
        };
        let (_dir, herdr, server) = fake(vec![snap(), ok()]);
        herdr.close_shell(None, "workriver-commands", "w7:p3").unwrap();
        let seen = server.join().unwrap();
        assert_eq!((seen[0]["method"].as_str(), seen[1]["method"].as_str(), &seen[1]["params"]), (Some("session.snapshot"), Some("pane.close"), &json!({ "pane_id": "w7:p3" })));

        // Another workspace's pane (the user's, an agent's), a pane with an agent, one gone:
        // the snapshot is read and nothing else is asked.
        for place in ["w5:p3", "w5:p1", "w7:p4", "w7:p9"] {
            let (_dir, herdr, server) = fake(vec![snap()]);
            assert!(matches!(herdr.close_shell(None, "workriver-commands", place), Err(RuntimeError::NoPlace(_))), "{place}");
            let (_dir2, herdr2, server2) = fake(vec![snap()]);
            assert!(matches!(herdr2.shell_processes(None, "workriver-commands", place), Err(RuntimeError::NoPlace(_))), "{place}");
            assert_eq!(server.join().unwrap().len() + server2.join().unwrap().len(), 2, "{place}");
        }

        // What runs there: its shell, and what is in front.
        let info = json!({ "result": { "type": "pane_process_info", "process_info": { "pane_id": "w7:p3", "shell_pid": 100, "foreground_processes": [{ "pid": 100 }, { "pid": 101 }] } } });
        let (_dir, herdr, server) = fake(vec![snap(), info]);
        assert_eq!(herdr.shell_processes(None, "workriver-commands", "w7:p3").unwrap(), (100, vec![100, 101]));
        assert_eq!(server.join().unwrap()[1]["params"], json!({ "pane_id": "w7:p3" }));
    }

    /// A new agent on a first-run prompt is blocked and not interactive-ready: taken at once,
    /// its tab kept. One Herdr could not start: its tab is closed.
    #[test]
    fn starts_a_temporary_agent() {
        let tab = json!({ "result": { "type": "tab_created", "root_pane": { "pane_id": "w5:p7" } } });
        let started = json!({ "result": { "type": "agent_started" } });
        let mut snap = snapshot();
        snap["result"]["snapshot"]["agents"].as_array_mut().unwrap().push(json!({ "pane_id": "w5:p7", "workspace_id": "w5", "agent": "claude", "name": "why-q1", "agent_status": "blocked", "interactive_ready": false }));
        let (_dir, herdr, server) = fake(vec![tab.clone(), started, snap]);
        let a = herdr.start_temporary("w5", "claude", "why-q1", "/r").unwrap();
        assert_eq!((a.name.as_str(), a.state), ("why-q1", AgentState::Blocked));
        let seen = server.join().unwrap();
        assert_eq!(seen.iter().map(|r| r["method"].as_str().unwrap()).collect::<Vec<_>>(), ["tab.create", "agent.start", "session.snapshot"], "no pane.close");

        let refused = json!({ "error": { "code": "agent_start_failed", "message": "no such harness" } });
        let (_dir, herdr, server) = fake(vec![tab, refused, json!({ "result": { "type": "ok" } })]);
        assert!(herdr.start_temporary("w5", "nope", "why-q2", "/r").is_err());
        let seen = server.join().unwrap();
        assert_eq!((seen[2]["method"].as_str(), &seen[2]["params"]), (Some("pane.close"), &json!({ "pane_id": "w5:p7" })));
    }

    #[test]
    fn no_socket_means_no_runtime() {
        let herdr = Herdr::new("/nonexistent/herdr.sock".into(), String::new());
        assert!(matches!(herdr.agents(), Err(RuntimeError::None)));
        assert!(herdr.check().unwrap().contains("not running"));
    }

    #[test]
    fn reads_the_processes_of_each_agent_pane() {
        let info = |shell: Value, fg: Value| json!({ "result": { "type": "pane_process_info", "process_info": { "pane_id": "x", "shell_pid": shell, "foreground_processes": fg } } });
        let refused = json!({ "error": { "code": "pane_not_found", "message": "gone" } });
        let replies = vec![snapshot(), info(json!(100), json!([{ "pid": 101, "name": "claude" }])), info(Value::Null, json!([{ "pid": 200, "name": "codex" }])), refused, info(json!(400), json!([]))];
        let (_dir, herdr, server) = fake(replies);
        let got: Vec<(String, Vec<u32>)> = herdr.processes(None).unwrap().into_iter().map(|(a, p)| (a.name, p)).collect();
        assert_eq!(got, [("lead".into(), vec![100, 101]), ("reviewer".into(), vec![200]), ("claude 1".into(), vec![]), ("claude 2".into(), vec![400])]);
        let seen = server.join().unwrap();
        assert_eq!(seen[1]["method"], "pane.process_info");
        assert_eq!(seen[1]["params"], json!({ "pane_id": "w5:p1" }));
    }
}
