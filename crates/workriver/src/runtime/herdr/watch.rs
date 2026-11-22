// Herdr's events, as wake-ups: one long connection subscribed to each agent pane's status
// changes (Herdr takes them only per pane) and to panes detected and closed, which end the
// connection so that the next one subscribes to the panes there are then.
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::Ordering;

use serde_json::{Value, json};
use workriver_core::runtime::{AgentState, Change, RuntimeError};

use super::Herdr;

/// Herdr's word for a state, as `agents_of` reads it.
pub(super) fn state_of(status: &str) -> AgentState {
    match status {
        "working" => AgentState::Working,
        "idle" => AgentState::Idle,
        "done" => AgentState::Done,
        "blocked" => AgentState::Blocked,
        _ => AgentState::Unknown,
    }
}

/// What to subscribe to for these agent panes.
pub(super) fn subscriptions(panes: &[String]) -> Value {
    let mut subs = vec![json!({ "type": "pane.agent_detected" }), json!({ "type": "pane.closed" })];
    subs.extend(panes.iter().map(|p| json!({ "type": "pane.agent_status_changed", "pane_id": p })));
    json!({ "subscriptions": subs })
}

/// One event line as a change to tell, and whether the connection must be opened again
/// (a pane came or went: the panes subscribed to are not the panes there are).
pub(super) fn event(v: &Value) -> (Option<Change>, bool) {
    let place = v["data"]["pane_id"].as_str().unwrap_or_default().to_string();
    match v["event"].as_str() {
        Some("pane.agent_status_changed") => (Some(Change { place, state: state_of(v["data"]["agent_status"].as_str().unwrap_or("")) }), false),
        Some("pane.agent_detected" | "pane.closed") => (Some(Change { place, state: AgentState::Unknown }), true),
        _ => (None, false),
    }
}

impl Herdr {
    pub(super) fn watch_changes(&self, to: &mut dyn FnMut(Change) -> bool) -> Result<(), RuntimeError> {
        loop {
            let (_, list) = self.read()?;
            let panes: Vec<String> = list.into_iter().map(|(p, _)| p).collect();
            let stream = UnixStream::connect(&self.socket).map_err(|_| RuntimeError::None)?;
            let fail = |e: std::io::Error| RuntimeError::Failed(format!("herdr events.subscribe: {e}"));
            let id = format!("workriver:{}", self.ids.fetch_add(1, Ordering::Relaxed));
            let mut line = json!({ "id": id, "method": "events.subscribe", "params": subscriptions(&panes) }).to_string();
            line.push('\n');
            (&stream).write_all(line.as_bytes()).map_err(fail)?;
            let mut lines = BufReader::new(&stream);
            let mut answer = String::new();
            if lines.read_line(&mut answer).map_err(fail)? == 0 {
                return Err(RuntimeError::Failed("herdr events.subscribe: the connection ended".into()));
            }
            let v: Value = serde_json::from_str(&answer).map_err(|e| RuntimeError::Failed(format!("herdr events.subscribe: unreadable answer: {e}")))?;
            if let Some(e) = v.get("error") {
                return Err(RuntimeError::Failed(format!("herdr events.subscribe: {}", e["message"].as_str().unwrap_or("?"))));
            }
            loop {
                let mut l = String::new();
                if lines.read_line(&mut l).map_err(fail)? == 0 {
                    return Err(RuntimeError::Failed("herdr events.subscribe: the connection ended".into()));
                }
                let Ok(v) = serde_json::from_str::<Value>(&l) else { continue };
                let (change, again) = event(&v);
                if let Some(c) = change
                    && !to(c)
                {
                    return Ok(());
                }
                if again {
                    break;
                }
            }
        }
    }
}
