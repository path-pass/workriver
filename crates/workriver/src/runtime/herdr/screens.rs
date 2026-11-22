// A pane's screen, its keys and its terminal stream: the place a session is resolved to,
// what it shows (`pane.read`), keys pressed there (`pane.send_keys`), a message typed there
// (`agent.prompt`), why Herdr calls it blocked (`agent.explain`), and its frames (a
// `herdr terminal session observe` child).
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use serde_json::{Value, json};
use workriver_core::runtime::{Frames, RuntimeError};
use workriver_core::screens::{Frame, Key, Screen};

use super::Herdr;

/// Herdr's name of a key.
pub(super) fn key_name(k: Key) -> String {
    match k {
        Key::Up => "up".into(),
        Key::Down => "down".into(),
        Key::Enter => "enter".into(),
        Key::Escape => "esc".into(),
        Key::Tab => "tab".into(),
        Key::Digit(d) => d.to_string(),
    }
}

/// Where the client socket of the server behind `socket` is: Herdr serves its terminal
/// stream there, next to its API socket (`herdr.sock` → `herdr-client.sock`).
pub fn client_socket(socket: &Path) -> PathBuf {
    let s = socket.to_string_lossy();
    PathBuf::from(format!("{}-client.sock", s.strip_suffix(".sock").unwrap_or(&s)))
}

/// The variables of Herdr's own a child is started without: it is not inside any pane.
pub(super) const CLEARED: [&str; 4] = ["HERDR_PANE_ID", "HERDR_WORKSPACE_ID", "HERDR_TAB_ID", "HERDR_ENV"];

/// The observe child for `pane` of the server behind `socket`: Herdr's client protocol is
/// private, so its own CLI is run (it must match the server's version).
pub(super) fn observe_command(socket: &Path, pane: &str) -> Command {
    let mut cmd = Command::new("herdr");
    cmd.args(["terminal", "session", "observe", pane]);
    for v in CLEARED {
        cmd.env_remove(v);
    }
    cmd.env("HERDR_SOCKET_PATH", socket);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd
}

impl Herdr {
    /// The one pane that shows this session now, from one fresh `agent.list` read (through the
    /// snapshot); never the cached session-to-pane map, never an agent by its name.
    pub(super) fn resolve_pane(&self, session: &str) -> Result<String, RuntimeError> {
        let (_, list) = self.read()?;
        let panes: Vec<String> = list.into_iter().filter(|(_, a)| a.session == session).map(|(p, _)| p).collect();
        match panes.as_slice() {
            [] => Err(RuntimeError::NoAgent(session.into())),
            [one] => Ok(one.clone()),
            _ => Err(RuntimeError::Ambiguous(format!("{session} shows in {} panes ({})", panes.len(), panes.join(", ")))),
        }
    }

    pub(super) fn screen_of(&self, pane: &str) -> Result<Screen, RuntimeError> {
        let r = self.call("pane.read", json!({ "pane_id": pane, "source": "visible", "format": "text" }))?;
        let text = r["read"]["text"].as_str().ok_or_else(|| RuntimeError::Failed("herdr pane.read: no text in the answer".into()))?;
        Ok(Screen::of(text.to_string()))
    }

    pub(super) fn press_at(&self, pane: &str, keys: &[Key]) -> Result<(), RuntimeError> {
        let keys: Vec<String> = keys.iter().map(|k| key_name(*k)).collect();
        self.call("pane.send_keys", json!({ "pane_id": pane, "keys": keys }))?;
        Ok(())
    }

    pub(super) fn prompt_at(&self, pane: &str, text: &str) -> Result<(), RuntimeError> {
        self.call("agent.prompt", json!({ "target": pane, "text": text }))?;
        Ok(())
    }

    /// Herdr's explanation as one line: its rule and evidence when it names them, else its
    /// answer as it is.
    pub(super) fn explain(&self, pane: &str) -> Result<Option<String>, RuntimeError> {
        let r = self.call("agent.explain", json!({ "target": pane }))?;
        Ok(explained(&r["explain"]))
    }

    pub(super) fn frames(&self, pane: &str) -> Result<Frames, RuntimeError> {
        let client = client_socket(&self.socket);
        if !client.exists() {
            return Err(RuntimeError::Failed(format!("no terminal stream: Herdr's client socket {} is not there", client.display())));
        }
        let child = observe_command(&self.socket, pane).spawn().map_err(|e| RuntimeError::Failed(format!("cannot run herdr terminal session observe: {e}")))?;
        Ok(Box::new(read_frames(child)?))
    }
}

/// What `agent.explain` says, kept to a line: `rule: evidence` when it names them.
fn explained(v: &Value) -> Option<String> {
    let text = |v: &Value| v.as_str().map(str::to_string).filter(|s| !s.trim().is_empty());
    let found = |keys: &[&str]| keys.iter().find_map(|k| text(&v[*k]));
    let rule = found(&["rule", "matched_rule", "rule_name"]);
    let evidence = found(&["evidence", "matched_text", "reason"]);
    match (rule, evidence) {
        (Some(r), Some(e)) => Some(format!("{r}: {e}")),
        (Some(r), None) => Some(r),
        (None, Some(e)) => Some(e),
        (None, None) if v.is_null() => None,
        (None, None) => text(v).or_else(|| Some(v.to_string())),
    }
}

/// An observe child's frames, read on a thread of their own so that a quiet terminal still
/// gives a `None` every second; dropping it kills and reaps the child, which ends that thread.
struct Observed {
    child: Child,
    frames: std::sync::mpsc::Receiver<Result<Frame, RuntimeError>>,
    done: bool,
}

/// How long a quiet terminal goes before its reader is asked whether it is still wanted.
const QUIET: std::time::Duration = std::time::Duration::from_secs(1);

/// The child's frames as they come, then why it ended (its last line on stderr), if it said.
fn read_frames(mut child: Child) -> Result<Observed, RuntimeError> {
    let out = child.stdout.take().ok_or_else(|| RuntimeError::Failed("herdr terminal session observe: no output".into()))?;
    let err = child.stderr.take();
    let (tx, frames) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines() {
            let Ok(line) = line else { break };
            let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
            if v["type"] != "terminal.frame" {
                continue;
            }
            let n = |k: &str| v[k].as_u64().unwrap_or(0);
            let frame = Frame { full: v["full"].as_bool().unwrap_or(false), cols: n("width") as u16, rows: n("height") as u16, seq: n("seq"), bytes: v["bytes"].as_str().unwrap_or_default().to_string() };
            if tx.send(Ok(frame)).is_err() {
                return;
            }
        }
        let mut why = String::new();
        if let Some(mut e) = err {
            let _ = std::io::Read::read_to_string(&mut e, &mut why);
        }
        let why = why.lines().last().unwrap_or("").trim().to_string();
        if !why.is_empty() {
            let _ = tx.send(Err(RuntimeError::Failed(format!("herdr terminal session observe: {why}"))));
        }
    });
    Ok(Observed { child, frames, done: false })
}

impl Iterator for Observed {
    type Item = Result<Option<Frame>, RuntimeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        match self.frames.recv_timeout(QUIET) {
            Ok(Ok(f)) => Some(Ok(Some(f))),
            Ok(Err(e)) => {
                self.done = true;
                Some(Err(e))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Some(Ok(None)),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                self.done = true;
                None
            }
        }
    }
}

impl Drop for Observed {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
