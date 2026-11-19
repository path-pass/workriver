// The events workriver records, the state they add up to, and the rules a new event must
// pass. Ids and numbers are assigned here, so replaying the stored events gives the same
// state every time.
use serde::{Deserialize, Serialize};

use crate::access::{Principal, Rejected, no};
use crate::conversations::*;
use crate::planning::*;
use crate::work::*;

mod body;
pub use body::*;

#[cfg(test)]
pub(crate) mod tests;

pub(crate) fn yes() -> bool {
    true
}

pub(crate) fn is_true(b: &bool) -> bool {
    *b
}

pub(crate) fn user() -> String {
    "user".into()
}

/// A recorded event.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Event {
    pub seq: i64,
    pub at: String,
    pub actor: String,
    #[serde(flatten)]
    pub body: Body,
}

// ---- the state the events add up to ----

#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
pub struct State {
    pub intents: Vec<Intent>,
    pub units: Vec<Unit>,
    pub chores: Vec<Chore>,
    #[serde(default)]
    pub agents: Vec<Agent>,
    #[serde(default)]
    pub questions: Vec<Question>,
    #[serde(default)]
    pub threads: Vec<Thread>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
    /// Planned intents in roadmap order (each project's roadmap is its part of this list).
    #[serde(default)]
    pub roadmap: Vec<String>,
    #[serde(default)]
    pub backlog: Vec<BacklogFinding>,
    #[serde(default)]
    pub links: Vec<Link>,
    /// The user's messages to managers, with their replies.
    #[serde(default)]
    pub messages: Vec<Message>,
    /// Up to when the user has read each workspace's conversation with its manager, by label.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub read: std::collections::BTreeMap<String, String>,
    /// Not the records but a part of them: what an agent on a blind task may see.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reduced: bool,
    /// The newest commands the user ran from the page (at most `RUNS_KEPT`), oldest first.
    /// Never serialized: only the user sees them, and the service adds them to the user's
    /// view itself.
    #[serde(skip)]
    pub runs: Vec<CommandRun>,
}

/// An event's kind, as recorded ("gate.submitted").
pub(crate) fn name_of_kind(b: &Body) -> String {
    serde_json::to_value(b).ok().and_then(|v| v["kind"].as_str().map(str::to_string)).unwrap_or_default()
}

pub fn name<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

pub(crate) fn blank(s: &str) -> bool {
    s.trim().is_empty()
}

impl State {

    /// Checks a new event against the rules and fills in what is assigned here (ids and
    /// numbers). The result is what gets stored.
    pub fn prepare(&self, who: &Principal, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        let actor = who.actor();
        let user = *who == Principal::User;
        // An agent is never the user or workriver by calling itself so.
        if blank(actor) || (matches!(who, Principal::Agent { .. }) && matches!(actor, "user" | "workriver")) {
            return no("the actor is empty or not an agent's");
        }
        // The last line of defence: whatever let the request in, these are the user's alone.
        if body.user_only() && !user {
            return no(format!("only the user does this ({}): in the web UI, or with the CLI from a terminal that is not an agent's", name_of_kind(&body)));
        }
        // Each kind's own rules are its domain's.
        match body {
            // Planning: planning/.
            IntentCreated { .. } | IntentStateChanged { .. } | IntentMoved { .. } | IntentMerged { .. } | IntentUpdated { .. } | IntentLinked { .. } => self.prepare_intents(who, body),
            ProjectAdded { .. } | ProjectRenamed { .. } | WorkspaceLinked(..) | WorkspaceUnlinked { .. } | WorkspaceProjectSet { .. } | ManagerRegistered { .. } => self.prepare_projects(who, body),
            UnitCreated { .. } | UnitRelocated { .. } | UnitMerged { .. } | UnitDone { .. } | CommitLinked { .. } | CommitUnlinked { .. } => self.prepare_units(who, body),
            // The work: work/.
            TaskCreated(..) | TaskStarted { .. } | TaskDelivered { .. } | TaskDropped { .. } | TaskAmended { .. } => self.prepare_tasks(who, body),
            FindingsBatched { .. } | FindingAdded { .. } | FindingDropped { .. } | FindingNoted { .. } | FindingDecided { .. } => self.prepare_findings(who, body),
            GateSubmitted { .. } => self.prepare_gates(who, body),
            ChoreCreated { .. } | ChoreDone { .. } => self.prepare_chores(who, body),
            CommandRan { .. } | CommandEnded { .. } | CommandReviewed { .. } => self.prepare_commands(who, body),
            // Conversations: conversations/.
            QuestionAsked { .. } | QuestionStarted { .. } | QuestionAnswered { .. } | ThreadEnded { .. } | QuestionFailed { .. } => self.prepare_questions(who, body),
            MessageSent { .. } | MessageDelivery { .. } | MessageReplied { .. } | MessageSaid { .. } | MessagesRead { .. } | ManagerTold { .. } | AgentSeen { .. } => self.prepare_messages(who, body),
            DeliveryUpdated { .. } => self.prepare_deliveries(who, body),
            AttachmentAdded { .. } => self.prepare_attachments(who, body),
        }
    }

    /// Adds a recorded event. It passed `prepare` when it was recorded, so it is trusted.
    pub fn apply(&mut self, e: &Event) {
        use Body::*;
        let at = &e.at;
        match &e.body {
            // Planning: planning/.
            IntentCreated { .. } | IntentStateChanged { .. } | IntentMoved { .. } | IntentMerged { .. } | IntentUpdated { .. } | IntentLinked { .. } => self.apply_intents(e),
            ProjectAdded { .. } | ProjectRenamed { .. } | WorkspaceLinked(..) | WorkspaceUnlinked { .. } | WorkspaceProjectSet { .. } | ManagerRegistered { .. } => self.apply_projects(e),
            UnitCreated { .. } | UnitRelocated { .. } | UnitMerged { .. } | UnitDone { .. } | CommitLinked { .. } | CommitUnlinked { .. } => self.apply_units(e),
            // The work: work/.
            TaskCreated(..) | TaskStarted { .. } | TaskDelivered { .. } | TaskDropped { .. } | TaskAmended { .. } => self.apply_tasks(e),
            FindingsBatched { .. } | FindingAdded { .. } | FindingDropped { .. } | FindingNoted { .. } | FindingDecided { .. } => self.apply_findings(e),
            GateSubmitted { .. } => self.apply_gates(e),
            ChoreCreated { .. } | ChoreDone { .. } => self.apply_chores(e),
            CommandRan { .. } | CommandEnded { .. } | CommandReviewed { .. } => self.apply_commands(e),
            // Conversations: conversations/.
            QuestionAsked { .. } | QuestionStarted { .. } | QuestionAnswered { .. } | ThreadEnded { .. } | QuestionFailed { .. } => self.apply_questions(e),
            MessageSent { .. } | MessageDelivery { .. } | MessageReplied { .. } | MessageSaid { .. } | MessagesRead { .. } | ManagerTold { .. } | AgentSeen { .. } => self.apply_messages(e),
            DeliveryUpdated { .. } => self.apply_deliveries(e),
            AttachmentAdded { .. } => self.apply_attachments(e),
        }
        self.open_gates(at);
    }
}

