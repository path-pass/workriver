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
    /// message with every reply; an agent, as `conversations_of` keeps it: one of the
    /// conversation of the workspace whose seat it holds (by the workspace's id), or of one
    /// to every manager its own copy, with its own replies only. Anything else is refused the
    /// same way whether or not it exists.
    pub fn message_for(&self, id: &str, who: &Principal) -> Result<crate::Message, Rejected> {
        let session = match who {
            Principal::User => return self.message(id).cloned(),
            Principal::Agent { session, .. } => session,
            Principal::Service => return no("workriver reads no message for itself"),
        };
        let seat = self.appointment_of(session);
        match self.messages.iter().find(|m| m.id == id).and_then(|m| blind::copy_for(m.clone(), session, seat.as_ref())) {
            Some(m) => Ok(m),
            None => no(format!("no message {id} that you may read: one of the conversation of the workspace you manage, or your copy of one to every manager")),
        }
    }
}

impl Body {
    /// What only the person may record: their decisions and results, and what changes
    /// where workriver reaches or who manages (projects, workspaces, a manager's seat, the
    /// workspace an intent runs in).
    pub fn user_only(&self) -> bool {
        use Body::*;
        matches!(
            self,
            GateSubmitted { .. } | ChoreDone { .. } | ThreadEnded { .. } | IntentWorkspaceSet { .. } | WorkspaceCreated { .. } | WorkspaceRenamed { .. } | WorkspaceClosed { .. } | WorkspaceOpened { .. } | WorkspaceManagerSet { .. } | ProjectAdded { .. } | ProjectRenamed { .. } | FindingsBatched { .. } | FindingDropped { .. } | MessageSent { .. } | MessagesRead { .. } | QuestionsRead { .. } | CommandRan { .. } | AgentKeys { .. }
        )
    }

    /// What only workriver itself records: a receipt of what the service did, which nobody
    /// can claim for it. That an agent was given a question to answer is one: whoever is
    /// recorded as answering may answer it. That a question failed is another: it closes the
    /// question, in workriver's words, so no agent may close the user's. What an agent is
    /// seen as (its name, harness, working directory, machine) is the runtime's word, given
    /// by the service: nobody names another agent, or points at another session's file. That
    /// a seated session has its seat back is what the service saw of its terminal.
    pub fn service_only(&self) -> bool {
        matches!(self, Body::QuestionStarted { .. } | Body::QuestionFailed { .. } | Body::AgentSeen { .. } | Body::ManagerBack { .. })
    }
}

#[cfg(test)]
mod tests;
