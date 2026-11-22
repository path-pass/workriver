// One message of a conversation in full, as the service gives it to `workriver message show`.
use serde::{Deserialize, Serialize};

/// One message whole, with its replies, as its reader may read it. Texts are whole, made safe
/// to quote (one line, no control characters, no quote marks of ours), never cut.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MessageShow {
    pub id: String,
    pub at: String,
    /// "the user", or the manager that said it, by name.
    pub by: String,
    /// The workspace whose conversation it is in, by id (`ws1`); empty: to every manager.
    pub workspace: String,
    /// The intent it is about; empty: none.
    pub intent: String,
    /// What it answers ("m3", or "reply 1 to m3") and the value picked; empty: no answer.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub answers: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub selected: String,
    /// Whether the value picked is workriver's Other: the text is the user's note.
    #[serde(default)]
    pub other: bool,
    pub text: String,
    pub replies: Vec<ReplyShow>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReplyShow {
    /// The manager that replied, by name.
    pub by: String,
    pub at: String,
    pub text: String,
}
