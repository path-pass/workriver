// Tabs and shells: where a temporary agent or a run's shell starts, and that a place is still a run's.
use std::time::Duration;

use serde_json::{Value, json};
use workriver_core::runtime::{AgentState, RuntimeAgent, RuntimeError};

use super::Herdr;

impl Herdr {
    /// Herdr focuses a pane inside its window; the window's app is brought forward here (macOS).
    /// A failure only means the user switches to it by hand.
    pub(super) fn activate(&self) {
        #[cfg(target_os = "macos")]
        if !self.terminal_app.is_empty() && !self.terminal_app.contains('"') {
            let _ = std::process::Command::new("osascript").args(["-e", &format!("tell application \"{}\" to activate", self.terminal_app)]).output();
        }
    }

    /// A new tab labelled `label` in the workspace labelled `group`, or the first tab of that
    /// workspace, made now when there is none; Herdr's answer, which names its root pane.
    pub(super) fn tab_in(&self, group: &str, dir: &str, label: &str) -> Result<Value, RuntimeError> {
        let snap = self.snapshot()?;
        match snap.workspaces.iter().find(|w| w.label == group) {
            Some(w) => self.call("tab.create", json!({ "workspace_id": w.workspace_id, "cwd": dir, "label": label, "focus": false })),
            None => self.call("workspace.create", json!({ "label": group, "cwd": dir, "focus": false })),
        }
    }

    /// That pane `place` is still a run's shell: there, in the workspace labelled `group`, with
    /// no agent in it. Read fresh each time and never taken from the id alone: an id kept for
    /// days may name another pane by then (whether a restarted Herdr numbers afresh is not known).
    pub(super) fn run_pane(&self, group: &str, place: &str) -> Result<(), RuntimeError> {
        let snap = self.snapshot()?;
        let pane = snap.panes.iter().find(|p| p.pane_id == place).ok_or_else(|| RuntimeError::NoPlace(format!("herdr: no pane {place}")))?;
        let theirs = snap.workspaces.iter().any(|w| w.workspace_id == pane.workspace_id && w.label == group);
        if !theirs || snap.agents.iter().any(|a| a.pane_id == place) {
            return Err(RuntimeError::NoPlace(format!("herdr: pane {place} is no longer a shell in {group}")));
        }
        Ok(())
    }

    /// Starts `harness` as `name` in the root pane of a tab just made (`tab`, Herdr's answer).
    pub(super) fn launch(&self, tab: &Value, harness: &str, name: &str) -> Result<RuntimeAgent, RuntimeError> {
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
