// Questions the user asks about a task or a unit, and the threads they form.
use serde::{Deserialize, Serialize};

use crate::*;

/// What a question is about besides its task: a finding, a file, lines of a file, or one of
/// the task's attachments.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct AskContext {
    /// "task", "finding", "file", "lines" or "attachment".
    pub kind: String,
    pub label: String,
    /// attachment: its id in the store (one shown with the task, see `State::attachments_of`,
    /// or one the user stored for the question).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<u32>,
}

/// A question about a task (task 0: its unit) and how it fared.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Question {
    pub id: String,
    pub thread: String,
    pub conclude: bool,
    pub unit: String,
    pub task: u32,
    pub text: String,
    pub context: Vec<AskContext>,
    pub at: String,
    /// Who asked: "user", or an agent's session.
    #[serde(default = "user")]
    pub by: String,
    /// The agent answering it: its session and name, once it has the question.
    pub agent: Option<String>,
    pub name: Option<String>,
    /// That agent was started for it (and is closed later), not the task's own.
    #[serde(default = "yes")]
    pub temporary: bool,
    pub answer: Option<String>,
    #[serde(default)]
    pub answered_at: Option<String>,
    pub asked: Option<String>,
    pub answered_by: Option<String>,
    pub failed: Option<(ReasonCode, String)>,
}

impl Question {
    pub fn open(&self) -> bool {
        self.answer.is_none() && self.failed.is_none()
    }
}

/// A conversation about a task (or, task 0, its unit): its questions share one temporary
/// agent while it lives.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Thread {
    pub id: String,
    pub unit: String,
    pub task: u32,
    /// The temporary agent last started for it: session and name.
    pub agent: Option<String>,
    pub name: Option<String>,
    pub ended: bool,
    pub conclusion: Option<String>,
}

impl State {
    pub fn question(&self, id: &str) -> Result<&Question, Rejected> {
        match self.questions.iter().find(|q| q.id == id) {
            Some(q) => Ok(q),
            None => no(format!("no question {id}")),
        }
    }
}
