// The Herdr runtime: Herdr's socket API in workriver's terms. The only place that knows
// Herdr's names (workspace, pane), its socket and its environment variables. Elsewhere an
// agent is its session id and a workspace is a runtime group.
//
// Protocol: newline-delimited JSON over a Unix socket, one request per connection
// (`{"id","method","params"}` -> `{"id","result"}` or `{"id","error":{"code","message"}}`).
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
    format!("Herdr lists no agent in pane {place} (see `herdr agent list`); run this from a pane running a coding agent, or pass --as <name>.")
}

/// The same when the service cannot ask Herdr.
pub fn cannot_ask(place: &str, why: &str) -> String {
    format!("cannot tell which agent runs in Herdr pane {place}: {why}. Is Herdr running and `workriver serve` started inside it? Or pass --as <name>.")
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
    terminal_app: String,
    ids: AtomicU64,
    /// Session -> pane, as last read.
    panes: Mutex<HashMap<String, String>>,
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
        Herdr { socket, terminal_app, ids: AtomicU64::new(1), panes: Mutex::default() }
    }

    /// One request on its own connection. A missing socket means Herdr is not running.
    fn call(&self, method: &str, params: Value) -> Result<Value, RuntimeError> {
        let fail = |e: std::io::Error| RuntimeError::Failed(format!("herdr {method}: {e}"));
        let stream = UnixStream::connect(&self.socket).map_err(|_| RuntimeError::None)?;
        stream.set_read_timeout(Some(Duration::from_secs(10))).map_err(fail)?;
        stream.set_write_timeout(Some(Duration::from_secs(5))).map_err(fail)?;
        let id = format!("workriver:{}", self.ids.fetch_add(1, Ordering::Relaxed));
        let mut line = json!({ "id": id, "method": method, "params": params }).to_string();
        line.push('\n');
        (&stream).write_all(line.as_bytes()).map_err(fail)?;
        let mut answer = String::new();
        BufReader::new(&stream).read_line(&mut answer).map_err(fail)?;
        let v: Value = serde_json::from_str(&answer).map_err(|e| RuntimeError::Failed(format!("herdr {method}: unreadable answer: {e}")))?;
        if let Some(e) = v.get("error") {
            return Err(RuntimeError::Failed(format!("herdr {method}: {} ({})", e["message"].as_str().unwrap_or("?"), e["code"].as_str().unwrap_or("?"))));
        }
        v.get("result").cloned().ok_or_else(|| RuntimeError::Failed(format!("herdr {method}: answer without a result")))
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
        let list = agents_of(&snap);
        *self.panes.lock().unwrap() = list.iter().map(|(p, a)| (a.session.clone(), p.clone())).collect();
        Ok((snap, list))
    }

    fn pane_of(&self, session: &str) -> Result<String, RuntimeError> {
        if let Some(p) = self.panes.lock().unwrap().get(session) {
            return Ok(p.clone());
        }
        let (_, list) = self.read()?;
        list.into_iter().find(|(_, a)| a.session == session).map(|(p, _)| p).ok_or_else(|| RuntimeError::NoAgent(session.into()))
    }
}

fn group(snap: &Snap, workspace: &str) -> Option<RuntimeRef> {
    snap.workspaces.iter().find(|w| w.workspace_id == workspace).map(|w| RuntimeRef { id: w.workspace_id.clone(), label: w.label.clone() })
}

/// Panes with an agent in them, as workriver's agents. The name is the agent's name in Herdr
/// (`herdr agent start <name>` / `herdr agent rename`), else the pane's label, else
/// `<harness> <n>`, numbered within its workspace in pane order.
fn agents_of(snap: &Snap) -> Vec<(String, RuntimeAgent)> {
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
                session: a.agent_session.as_ref().map(|s| s.value.clone()).unwrap_or_else(|| format!("herdr:{}", a.pane_id)),
                state: match a.agent_status.as_str() {
                    "working" => AgentState::Working,
                    "idle" => AgentState::Idle,
                    "done" => AgentState::Done,
                    "blocked" => AgentState::Blocked,
                    _ => AgentState::Unknown,
                },
                cwd: set(&a.foreground_cwd).or_else(|| set(&a.cwd)).unwrap_or_default(),
                group: group(snap, &a.workspace_id),
                topic: set(&a.terminal_title_stripped),
                harness,
                name,
            };
            (a.pane_id.clone(), agent)
        })
        .collect()
}

impl Runtime for Herdr {
    fn agents(&self) -> Result<Vec<RuntimeAgent>, RuntimeError> {
        Ok(self.read()?.1.into_iter().map(|(_, a)| a).collect())
    }

    fn locate(&self, place: &str) -> Result<Place, RuntimeError> {
        let (snap, list) = self.read()?;
        let agent = list.into_iter().find(|(p, _)| p == place).map(|(_, a)| a);
        let group = snap.panes.iter().find(|p| p.pane_id == place).and_then(|p| group(&snap, &p.workspace_id));
        Ok(Place { agent, group })
    }

    fn focus(&self, session: &str) -> Result<(), RuntimeError> {
        let pane = self.pane_of(session)?;
        self.call("agent.focus", json!({ "target": pane }))?;
        // Herdr focuses the pane inside its window; the window's app is brought forward here.
        // A failure only means the user switches to it by hand.
        #[cfg(target_os = "macos")]
        if !self.terminal_app.is_empty() && !self.terminal_app.contains('"') {
            let _ = std::process::Command::new("osascript").args(["-e", &format!("tell application \"{}\" to activate", self.terminal_app)]).output();
        }
        Ok(())
    }

    fn send(&self, session: &str, text: &str) -> Result<(), RuntimeError> {
        let pane = self.pane_of(session)?;
        self.call("agent.prompt", json!({ "target": pane, "text": text }))?;
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
        assert_eq!(a.group, Some(RuntimeRef { id: "w5".into(), label: "chat".into() }));
        let b = &list[1];
        assert_eq!((b.session.as_str(), b.name.as_str(), b.state), ("herdr:w5:p2", "reviewer", AgentState::Done), "the Herdr name before the pane label");
        // A shell pane: its workspace, no agent.
        let shell = herdr.locate("w5:p3").unwrap();
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
        let err = herdr.send("s-1", "again").unwrap_err().to_string();
        assert!(err.contains("agent_blocked") && err.contains("agent is blocked"), "{err}");
        let seen = server.join().unwrap();
        assert_eq!(seen[1]["method"], "agent.prompt");
        assert_eq!(seen[1]["params"], json!({ "target": "w5:p1", "text": "hello" }));
    }

    #[test]
    fn no_socket_means_no_runtime() {
        let herdr = Herdr::new("/nonexistent/herdr.sock".into(), String::new());
        assert!(matches!(herdr.agents(), Err(RuntimeError::None)));
        assert!(herdr.check().unwrap().contains("not running"));
    }
}
