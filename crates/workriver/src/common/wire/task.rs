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
    /// None from a service of an earlier build, for work recorded on the intent itself.
    pub unit: Option<Part>,
    /// Where the work is, as it is on the machine it is on.
    pub place: Option<Place>,
    /// The agent it is assigned to, by name.
    pub agent: Option<String>,
    pub blind: bool,
    /// What it asks, in full; and who wrote it ("the user", or an agent's name).
    pub text: String,
    pub written_by: String,
    /// Requirements added to it since (`workriver task note`), oldest first: part of what it asks.
    #[serde(default)]
    pub notes: Vec<NoteShow>,
    /// Files its text (or what was added to it) names that the attachment rule took: each
    /// "name (removed DATE)" or "name (moved: `fetch command`)".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub no_longer_stored: Vec<String>,
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
    /// A decide gate's options, as stored (`core::Choice`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<core::Choice>,
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
    /// Which harness and model its agent is, and where the harness keeps the session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<Trace>,
    /// Placed from its unit's template: which step, and what kind of step it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<StepShow>,
    /// A gate of a unit with placed steps: what it lists of them (`State::gate_steps`), with
    /// whoever did each by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_steps: Option<core::GateSteps>,
}

/// The step of a template a task was placed as.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StepShow {
    pub id: String,
    pub template: String,
    /// The user's template, not the built-in: the description is the user's words.
    #[serde(default)]
    pub users: bool,
    pub description: String,
    /// Not filled yet: its manager fills it (`workriver task fill`).
    #[serde(default)]
    pub to_fill: bool,
}

/// Which harness and model did a task, and where the harness keeps the session, on the
/// machine the agent ran on (`server::views::trace`). Nothing reads that file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Trace {
    /// The agent's session: the harness's own id.
    pub session: String,
    /// The harness, as the runtime named it (claude, codex), once the agent was seen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
    /// The model, as the agent said when it started the task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The machine the agent ran on; none: the one workriver runs on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    /// The session file on that machine; with `*` in it where it is found by its id and could
    /// not be looked for from here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// `file` is worked out, not seen: on another machine, or not there under the name the
    /// harness would give it (the directory the agent was started in may not be the one the
    /// session was begun in).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unconfirmed: bool,
    /// Why there is no file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unknown: Option<NoFile>,
}

/// Why a task's trace names no session file.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NoFile {
    /// workriver does not know where this harness keeps its sessions (or which harness it is).
    Harness,
    /// The harness gave no session id of its own.
    Id,
    /// The working directory the harness keeps it by was not recorded (seen before it was).
    Cwd,
    /// Looked for by its id on this machine, and not there.
    Missing,
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
    /// A gate's: done, problem, or answered.
    pub result: Option<String>,
    /// An answered gate's: the option picked.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected: Vec<String>,
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
