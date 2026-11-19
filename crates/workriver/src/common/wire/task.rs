// `workriver task show`'s shape: one task with everything its reader may see, as the service
// builds it and the CLI prints it (or `--json` gives it); and a stopped worker's mark, which
// the page, the board and `task show` read.
use serde::{Deserialize, Serialize};
use workriver_core as core;

/// One task as `workriver task show --json` prints it. Field names are stable: agents and
/// scripts read them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskShow {
    #[serde(rename = "ref")]
    pub reference: String,
    /// Where the user opens it in the page (the service's address on its machine).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub link: String,
    pub kind: String,
    pub state: String,
    pub title: String,
    pub intent: Part,
    /// None for work of the intent itself (research, design across units).
    pub unit: Option<Part>,
    /// Where the work is, as it is on the machine it is on.
    pub place: Option<Place>,
    /// The agent it is assigned to, by name.
    pub agent: Option<String>,
    pub blind: bool,
    /// What it asks, in full; and who wrote it ("the user", or an agent's name).
    pub text: String,
    pub written_by: String,
    /// The tasks it comes after (`start`: it can begin at once).
    pub after: Vec<String>,
    pub causes: Vec<CauseShow>,
    /// What was designed and found before it: the delivered designs of its intent's research
    /// units, then what the design and research tasks before it in its own unit delivered.
    /// Of a design and the designs that revise it, only the newest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub earlier: Vec<EarlierShow>,
    pub plans: Vec<String>,
    pub steps: Vec<core::Step>,
    /// What it delivered, once it did.
    pub output: Option<OutputShow>,
    pub commits: Vec<CommitShow>,
    /// Its commits that were only on a branch the unit has left.
    pub lost_commits: Vec<String>,
    pub questions: Vec<QuestionShow>,
    /// Dropped as no longer needed: when, by whom (by name), why.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dropped: Option<core::Closed>,
    /// Running, and its agent stopped without delivering: since when, or it is gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stalled: Option<Stall>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Part {
    pub id: String,
    pub title: String,
    /// The intent's goal, or the unit's purpose.
    pub about: String,
    /// The intent's note (more about it; a merge appends the merged intents' text).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// A unit's: working, waiting, ready, merged or done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Place {
    /// None: the machine workriver runs on.
    pub machine: Option<String>,
    /// That machine is disconnected: since when (what is shown is as last read).
    #[serde(rename = "disconnectedSince", default, skip_serializing_if = "Option::is_none")]
    pub disconnected_since: Option<String>,
    pub repo: String,
    pub branch: String,
    pub base: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CauseShow {
    /// The task that found it (`u1#4`), or `backlog`.
    pub from: String,
    pub finding: FindingShow,
}

/// A design or research task a task comes after, with what it delivered.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EarlierShow {
    /// The task (`u1#1`).
    pub from: String,
    /// design, or research.
    pub kind: String,
    pub title: String,
    pub delivered_by: String,
    pub note: String,
    pub files: Vec<String>,
    /// A design's: the decide gate that asks the user to confirm it, if there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<String>,
    /// What the user said at that gate; none while it is not answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed: Option<bool>,
    /// The reader is a review that follows this design directly: it is what it reviews.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub under_review: bool,
    /// The reader is the design task that revises this design.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub revises: bool,
    /// A design task not delivered yet revises it: that task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revised_in: Option<String>,
    /// Shown in place of the design it revises (the reader comes after that one): that design.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_place_of: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FindingShow {
    pub id: String,
    pub severity: String,
    pub problem: String,
    pub impact: String,
    #[serde(rename = "where")]
    pub places: Vec<String>,
    /// What the reporter suggested deciding, and why.
    pub suggestion: Option<(String, String)>,
    pub reported_by: String,
    pub state: String,
    pub decisions: Vec<DecisionShow>,
    pub notes: Vec<NoteShow>,
    pub files: Vec<String>,
    /// Dropped: why.
    pub reason: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DecisionShow {
    pub state: String,
    pub by: String,
    pub note: String,
    pub at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NoteShow {
    pub by: String,
    pub at: String,
    pub text: String,
    pub files: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutputShow {
    pub at: Option<String>,
    pub delivered_by: String,
    pub note: String,
    pub evidence: Vec<EvidenceShow>,
    pub findings: Vec<FindingShow>,
    pub files: Vec<String>,
    /// A gate's: done, or problem.
    pub result: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EvidenceShow {
    pub command: String,
    pub result: String,
    pub summary: String,
    pub file: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CommitShow {
    pub hash: String,
    pub subject: String,
    /// Linked to the task by hand (`workriver commit link`), not by its trailer.
    pub linked: bool,
    /// The same change as this commit of a branch the unit left: its old hash.
    #[serde(rename = "rebasedFrom", default, skip_serializing_if = "Option::is_none")]
    pub rebased_from: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuestionShow {
    pub id: String,
    pub question: String,
    pub asked_by: String,
    pub at: String,
    pub answer: Option<String>,
    pub answered_by: Option<String>,
}

/// A running task whose agent stopped, as the page, the board and `task show` read it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Stall {
    /// Idle (or done) since then; gone: since it was first seen gone (or idle before that).
    pub since: String,
    /// The agent is no longer in the runtime.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gone: bool,
    /// When the agent itself was reminded of its task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reminded: Option<String>,
    /// The reminder did not help (or it is gone): the user (Needs you) and the manager are told.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub escalated: bool,
}
