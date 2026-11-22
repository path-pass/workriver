// Who may record what, and who may see what: the principal a request was found to come
// from, the refusal a broken rule gives, what is the user's alone, and (blind.rs) what an
// agent on a blind task may see.
use crate::records::Body;

mod blind;

/// A write that breaks a rule. The message is shown to whoever tried it, as is.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Rejected(pub String);

pub(crate) fn no<T>(msg: impl Into<String>) -> Result<T, Rejected> {
    Err(Rejected(msg.into()))
}

/// Who a request was found to come from. The daemon works this out (a session of the web
/// UI or the user's token; the agent in the place the calling process runs at); it is never
/// taken from what the caller says. Events are recorded under `actor()`.
#[derive(Clone, PartialEq, Debug)]
pub enum Principal {
    /// The person: authenticated with the user token or a session of the web UI.
    User,
    /// An agent, by its session, on a machine (none: this one).
    Agent { session: String, machine: Option<String> },
    /// workriver itself: deliveries, what it sees of agents, its temporary agents.
    Service,
}

impl Principal {
    pub fn actor(&self) -> &str {
        match self {
            Principal::User => "user",
            Principal::Agent { session, .. } => session,
            Principal::Service => "workriver",
        }
    }

    pub fn machine(&self) -> Option<&str> {
        match self {
            Principal::Agent { machine, .. } => machine.as_deref(),
            _ => None,
        }
    }
}

/// For tests of the rules: "user", "workriver", or an agent's session.
#[cfg(test)]
impl From<&str> for Principal {
    fn from(s: &str) -> Principal {
        match s {
            "user" => Principal::User,
            "workriver" => Principal::Service,
            s => Principal::Agent { session: s.into(), machine: None },
        }
    }
}

impl crate::State {
    /// One message as `who` may read it whole (`workriver message show`): the user, every
    /// message with every reply; an agent, one of the conversation of the workspace it was
    /// found in (`group`) or one that went to it; and one to every manager that went to it,
    /// with its own replies only (the others answer for other workspaces).
    pub fn message_for(&self, id: &str, who: &Principal, group: Option<&crate::RuntimeRef>) -> Result<crate::Message, Rejected> {
        let m = self.message(id)?;
        let session = match who {
            Principal::User => return Ok(m.clone()),
            Principal::Agent { session, .. } => session,
            Principal::Service => return no("workriver reads no message for itself"),
        };
        if !m.to.is_empty() {
            if !m.to.iter().any(|r| &r.manager == session) {
                return no(format!("message {id} went to every manager; you are not one it went to"));
            }
            return Ok(crate::Message { replies: m.replies.iter().filter(|r| &r.by == session).cloned().collect(), ..m.clone() });
        }
        let ours = !m.workspace.is_empty() && group.and_then(|g| self.workspace_of(g)).is_some_and(|w| w.group.label == m.workspace);
        if ours || &m.manager == session { Ok(m.clone()) } else { no(format!("message {id} is not in your workspace's conversation")) }
    }
}

impl Body {
    /// What only the person may record: their decisions and results, and what changes
    /// where workriver reaches (projects, links between workspaces).
    pub fn user_only(&self) -> bool {
        use Body::*;
        matches!(
            self,
            GateSubmitted { .. } | ChoreDone { .. } | ThreadEnded { .. } | IntentLinked { .. } | WorkspaceLinked(_) | WorkspaceUnlinked { .. } | WorkspaceProjectSet { .. } | ProjectAdded { .. } | ProjectRenamed { .. } | FindingsBatched { .. } | FindingDropped { .. } | MessageSent { .. } | MessagesRead { .. } | QuestionsRead { .. } | CommandRan { .. }
        )
    }

    /// What only workriver itself records: a receipt of what the service did, which nobody
    /// can claim for it. That an agent was given a question to answer is one: whoever is
    /// recorded as answering may answer it. That a question failed is another: it closes the
    /// question, in workriver's words, so no agent may close the user's. What an agent is
    /// seen as (its name, harness, working directory, machine) is the runtime's word, given
    /// by the service: nobody names another agent, or points at another session's file.
    pub fn service_only(&self) -> bool {
        matches!(self, Body::QuestionStarted { .. } | Body::QuestionFailed { .. } | Body::AgentSeen { .. })
    }
}

#[cfg(test)]
mod tests;
