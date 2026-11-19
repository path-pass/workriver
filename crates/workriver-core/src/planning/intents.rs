// Intents: the roadmap of a project, an intent's life from planned to done or dropped, and
// who decides its findings.
use serde::{Deserialize, Serialize};

use crate::access::{Rejected, no};
use crate::records::*;

use super::*;

/// Where an intent is: in the backlog, being worked on, finished, or given up.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum IntentState {
    Planned,
    Active,
    Done,
    Dropped,
}

pub(crate) fn active() -> IntentState {
    IntentState::Active
}

/// The same item in an outside tracker (Jira, Linear, GitHub Issues); only kept and shown.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct External {
    pub system: String,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Who decides an intent's open findings; set by the user alone.
///
/// There is no policy by severity: a finding's severity is its reviewer's own rating, so a
/// rule that kept "P1" for the user would look like a control and be none.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FindingsPolicy {
    /// The user decides every finding. (Also what a record of the withdrawn
    /// "manager-except-p1" replays as: the stricter of the two.)
    #[serde(alias = "manager-except-p1")]
    User,
    /// The intent's manager decides them: the default. The user can still decide any
    /// finding, and change what the manager decided.
    #[default]
    Manager,
}

impl FindingsPolicy {
    /// Whether the intent's manager decides its findings.
    pub fn managers(self) -> bool {
        self == FindingsPolicy::Manager
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Intent {
    pub id: String,
    pub title: String,
    /// Before 0.2: the manager registered for it. Now the manager is the workspace's.
    pub manager: Option<String>,
    #[serde(default)]
    pub goal: String,
    /// Before 0.2: the runtime group it followed.
    #[serde(default)]
    pub runtime_ref: Option<RuntimeRef>,
    pub at: String,
    /// The project folder ("" when not known: then its units' repositories say).
    #[serde(default)]
    pub project: String,
    #[serde(default = "active")]
    pub state: IntentState,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub external: Option<External>,
    /// Who added it: "user", or the agent (session) that did. Only the user merges what the
    /// user added; agents may tidy what agents added (an import split too finely).
    #[serde(default)]
    pub by: String,
    /// Where it runs while active (and last ran).
    #[serde(default)]
    pub workspace: Option<RuntimeRef>,
    /// Why it was dropped.
    #[serde(default)]
    pub reason: Option<String>,
    /// Who decides its open findings.
    #[serde(default)]
    pub findings: FindingsPolicy,
    /// Intent-level work (research, design), numbered `i3#1`…
    #[serde(default)]
    pub tasks: Vec<Task>,
}

/// What a merged intent said, as a block of its target's note: its title, goal, note and
/// external id (with its link), one per line, empty ones left out.
pub(crate) fn merged_note(i: &Intent) -> String {
    let external = i.external.as_ref().map(|e| format!("{}:{}{}", e.system, e.key, e.url.as_deref().map(|u| format!(" {u}")).unwrap_or_default()));
    [format!("Merged from {}: {}", i.id, i.title), i.goal.trim().to_string(), i.note.trim().to_string(), external.unwrap_or_default()].into_iter().filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

impl State {
    pub fn intent(&self, id: &str) -> Result<&Intent, Rejected> {
        match self.intents.iter().find(|i| i.id == id) {
            Some(i) => Ok(i),
            None => no(format!("no intent {id}")),
        }
    }

    /// The intent a unit (or intent) belongs to.
    pub fn intent_of(&self, owner: &str) -> Option<&str> {
        self.units.iter().find(|u| u.id == owner).map(|u| u.intent.as_str()).or_else(|| self.intents.iter().find(|i| i.id == owner).map(|i| i.id.as_str()))
    }

    /// The manager of an intent: the manager of the runtime group it runs in (before 0.2,
    /// the one registered for the intent itself).
    pub fn manager_of(&self, intent: &str) -> Option<&str> {
        let i = self.intents.iter().find(|i| i.id == intent)?;
        i.workspace.as_ref().and_then(|g| self.workspace_of(g)).and_then(|w| w.manager.as_deref()).or(i.manager.as_deref())
    }

    /// Whether every unit of `intent` is merged or done (and it has one).
    pub fn finished(&self, intent: &str) -> bool {
        let mut units = self.units.iter().filter(|u| u.intent == intent).peekable();
        units.peek().is_some() && units.all(|u| matches!(self.progress(u), Progress::Merged | Progress::Done))
    }
}
