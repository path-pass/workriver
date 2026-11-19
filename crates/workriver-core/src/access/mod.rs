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

impl Body {
    /// What only the person may record: their decisions and results, and what changes
    /// where workriver reaches (projects, links between workspaces).
    pub fn user_only(&self) -> bool {
        use Body::*;
        matches!(
            self,
            GateSubmitted { .. } | ChoreDone { .. } | ThreadEnded { .. } | WorkspaceLinked(_) | WorkspaceUnlinked { .. } | WorkspaceProjectSet { .. } | ProjectAdded { .. } | ProjectRenamed { .. } | FindingsBatched { .. } | FindingDropped { .. } | MessageSent { .. } | MessagesRead { .. } | CommandRan { .. }
        )
    }
}
