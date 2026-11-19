// The Herdr runtime: Herdr's socket API in workriver's terms. The only place that knows
// Herdr's names (workspace, pane), its socket and its environment variables. Elsewhere an
// agent is its session id and a workspace is a runtime group.
//
// Protocol: newline-delimited JSON over a Unix socket, one request per connection
// (`{"id","method","params"}` -> `{"id","result"}` or `{"id","error":{"code","message"}}`).
//
// Another machine has its own Herdr server; its socket, forwarded here over SSH (see
// ../ssh.rs), speaks the same protocol, so the same client talks to it. Ids are per
// server: everything read from another machine carries the machine's name.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;

use serde_json::json;
use workriver_core::RuntimeRef;
use workriver_core::runtime::{Place, Root, Runtime, RuntimeAgent, RuntimeError};

mod agents;
mod protocol;
pub mod remote;
mod terminals;
pub mod words;

use agents::group;
pub use protocol::socket_path;

/// Where the CLI runs, when it runs in a Herdr pane: the pane id, as an opaque place.
pub fn place_from_env() -> Option<String> {
    std::env::var("HERDR_PANE_ID").ok().filter(|s| !s.is_empty())
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

impl Herdr {
    pub fn new(socket: PathBuf, terminal_app: String) -> Herdr {
        Herdr { socket, terminal_app, ids: AtomicU64::new(1), panes: Mutex::default(), machine: None }
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

    /// One `pane.process_info` per pane asked for. A pane Herdr does not list, or no longer
    /// answers for, is not there.
    fn roots(&self, _: Option<&str>, place: Option<&str>) -> Result<Vec<Root>, RuntimeError> {
        self.roots_of(place, false)
    }

    fn agent_roots(&self, _: Option<&str>) -> Result<Vec<Root>, RuntimeError> {
        self.roots_of(None, true)
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
mod tests;
