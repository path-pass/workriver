// Herdr's snapshot as workriver's agents, groups and roots: names, states, session ids.
use serde::Deserialize;
use serde_json::json;
use workriver_core::RuntimeRef;
use workriver_core::runtime::{AgentState, Place, Root, RuntimeAgent, RuntimeError};

use super::Herdr;
use crate::runtime::remote_ref;

#[derive(Deserialize)]
pub(super) struct Snap {
    #[serde(default)]
    pub(super) workspaces: Vec<Workspace>,
    #[serde(default)]
    pub(super) panes: Vec<Pane>,
    #[serde(default)]
    pub(super) agents: Vec<AgentInfo>,
}

#[derive(Deserialize)]
pub(super) struct Workspace {
    pub(super) workspace_id: String,
    #[serde(default)]
    pub(super) label: String,
}

#[derive(Deserialize)]
pub(super) struct Pane {
    pub(super) pane_id: String,
    pub(super) workspace_id: String,
    #[serde(default)]
    pub(super) label: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct AgentInfo {
    pub(super) pane_id: String,
    pub(super) workspace_id: String,
    #[serde(default)]
    pub(super) agent_status: String,
    #[serde(default)]
    pub(super) agent: Option<String>,
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) agent_session: Option<Session>,
    #[serde(default)]
    pub(super) cwd: Option<String>,
    #[serde(default)]
    pub(super) foreground_cwd: Option<String>,
    /// Still launching / ready for input: in Herdr's schema, not described; absent says nothing.
    #[serde(default)]
    pub(super) launch_pending: Option<bool>,
    #[serde(default)]
    pub(super) interactive_ready: Option<bool>,
    /// The terminal title without decoration: what the session is about.
    #[serde(default)]
    pub(super) terminal_title_stripped: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct Session {
    pub(super) value: String,
}

impl Herdr {
    pub(super) fn snapshot(&self) -> Result<Snap, RuntimeError> {
        let r = self.call("session.snapshot", json!({}))?;
        serde_json::from_value(r["snapshot"].clone()).map_err(|e| RuntimeError::Failed(format!("herdr session.snapshot: {e}")))
    }

    /// The live agents with the pane each sits in; also remembers session -> pane.
    pub(super) fn read(&self) -> Result<(Snap, Vec<(String, RuntimeAgent)>), RuntimeError> {
        let snap = self.snapshot()?;
        let list = agents_of(&snap, self.label());
        *self.panes.lock().unwrap() = list.iter().map(|(p, a)| (a.session.clone(), p.clone())).collect();
        Ok((snap, list))
    }

    pub(super) fn label(&self) -> Option<&str> {
        self.machine.as_deref()
    }

    /// The roots of `place`, or of every pane (`agents`: of every pane that holds an agent).
    /// A pane whose root cannot be asked is left out, except an agent's pane when `agents`:
    /// it still shows its session, with no known shell (rule 5 then pins nobody there).
    pub(super) fn roots_of(&self, place: Option<&str>, agents: bool) -> Result<Vec<Root>, RuntimeError> {
        let (snap, list) = self.read()?;
        let mut panes: Vec<(&str, &str)> = snap.panes.iter().map(|p| (p.pane_id.as_str(), p.workspace_id.as_str())).collect();
        for a in &snap.agents {
            if !panes.iter().any(|(p, _)| *p == a.pane_id) {
                panes.push((&a.pane_id, &a.workspace_id));
            }
        }
        let mut out = vec![];
        for (pane, workspace) in panes.into_iter().filter(|(p, _)| place.is_none_or(|x| x == *p)) {
            let agent = list.iter().find(|(p, _)| p == pane).map(|(_, a)| a.clone());
            if agents && agent.is_none() {
                continue;
            }
            let r = match self.call("pane.process_info", json!({ "pane_id": pane })) {
                Ok(r) => r,
                Err(_) if agents => json!({}),
                Err(_) => continue,
            };
            let info = &r["process_info"];
            let front = info["foreground_processes"].as_array().into_iter().flatten().filter_map(|p| p["pid"].as_u64()).map(|p| p as u32).collect();
            out.push(Root { place: pane.to_string(), shell: info["shell_pid"].as_u64().map(|p| p as u32), front, at: Place { agent, group: group(&snap, workspace, self.label()) } });
        }
        Ok(out)
    }

    /// The pane of an agent given by session or by name.
    pub(super) fn pane_of(&self, agent: &str) -> Result<String, RuntimeError> {
        if let Some(p) = self.panes.lock().unwrap().get(agent) {
            return Ok(p.clone());
        }
        let (_, list) = self.read()?;
        list.into_iter().find(|(_, a)| a.session == agent || a.name == agent).map(|(p, _)| p).ok_or_else(|| RuntimeError::NoAgent(agent.into()))
    }
}

/// A workspace as a runtime group. Ids are per Herdr server, so another machine's carry its name.
pub(super) fn group(snap: &Snap, workspace: &str, machine: Option<&str>) -> Option<RuntimeRef> {
    let id = |w: &str| machine.map(|m| remote_ref(w, m)).unwrap_or_else(|| w.to_string());
    snap.workspaces.iter().find(|w| w.workspace_id == workspace).map(|w| RuntimeRef { id: id(&w.workspace_id), label: w.label.clone(), machine: machine.map(str::to_string) })
}

/// Panes with an agent in them, as workriver's agents. The name is the agent's name in Herdr
/// (`herdr agent start <name>` / `herdr agent rename`), else the pane's label, else
/// `<harness> <n>`, numbered within its workspace in pane order.
pub(super) fn agents_of(snap: &Snap, machine: Option<&str>) -> Vec<(String, RuntimeAgent)> {
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
                // An id of that form is ours alone to give: a pane that reports one as its
                // session is named by the pane it is in, never as the pane it names.
                session: a.agent_session.as_ref().map(|s| s.value.clone()).filter(|v| !v.starts_with("herdr:")).unwrap_or_else(|| match machine {
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
