// Messages between the user and a workspace's manager, their replies, and the agents seen.
use serde::{Deserialize, Serialize};

use crate::*;

/// An agent session as last seen: the name shown for it, and its harness.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Agent {
    pub session: String,
    pub name: String,
    pub harness: String,
}

/// The user's message to a workspace's manager, and what it answered.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Message {
    pub id: String,
    pub intent: String,
    pub workspace: String,
    /// The manager's session it went to: only that one replies.
    pub manager: String,
    pub text: String,
    pub at: String,
    pub delivery: Option<Delivery>,
    #[serde(default)]
    pub replies: Vec<Reply>,
    /// Started by the manager (`workriver say`): its text is the manager's, and it went nowhere.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_manager: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Reply {
    pub by: String,
    pub text: String,
    pub at: String,
}

impl State {
    /// The name last seen for an agent session.
    pub fn name_of(&self, session: &str) -> Option<&str> {
        self.agents.iter().find(|a| a.session == session).map(|a| a.name.as_str())
    }

    pub fn message(&self, id: &str) -> Result<&Message, Rejected> {
        match self.messages.iter().find(|m| m.id == id) {
            Some(m) => Ok(m),
            None => no(format!("no message {id}")),
        }
    }
}
