// The work: tasks and the graph they form in a unit, what they deliver, the findings of
// reviews and gates and their decisions, chores, and the commands the user ran.
use serde::{Deserialize, Serialize};

use crate::*;

mod chores;
mod commands;
mod findings;
mod gates;
mod graph;
mod tasks;

pub use chores::*;
pub use commands::*;
pub use findings::*;
pub use gates::*;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum TaskKind {
    Implement,
    Review,
    Tests,
    Gate,
    /// Working out how, when the user has choices to make: its output is a design note.
    Design,
    /// Finding out (a survey, a spike, a comparison): its output is a note and attachments.
    Research,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Todo,
    Running,
    /// Gates only: the user has not done it yet.
    Waiting,
    Done,
    /// No longer needed (`task.dropped`); kept, never deleted. See `Task::dropped`.
    Dropped,
}

impl TaskState {
    /// Done or dropped: nothing more comes of it, and nothing waits on it.
    pub fn over(self) -> bool {
        matches!(self, TaskState::Done | TaskState::Dropped)
    }
}

/// One thing to do by hand, with the command for it when there is one.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Step {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

/// The task this one follows: `"start"` or a task number of the same unit.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Input {
    pub from: String,
    #[serde(default = "yes")]
    pub primary: bool,
}

/// Findings of a review, tests task or gate (by task number) that a task answers.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Cause {
    /// The unit (or intent) of that task, when it is not this task's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    pub task: u32,
    pub findings: Vec<String>,
}

/// A check run on a piece of work: the command and what came of it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Evidence {
    pub command: String,
    pub result: CheckResult,
    pub summary: String,
    /// A stored attachment that shows it (a screenshot, a report).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum CheckResult {
    Passed,
    Failed,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct NewTask {
    /// The unit it is in, or an intent for intent-level work (research, design).
    pub unit: String,
    /// The intent it serves; assigned when recorded.
    #[serde(default)]
    pub intent: String,
    /// Assigned when recorded.
    #[serde(default)]
    pub n: u32,
    pub kind: TaskKind,
    #[serde(default)]
    pub title: String,
    /// What the agent was asked to do.
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub inputs: Vec<Input>,
    #[serde(default)]
    pub causes: Vec<Cause>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub reviewer: Option<Reviewer>,
    #[serde(default)]
    pub gate: Option<GateKind>,
    #[serde(default)]
    pub steps: Vec<Step>,
    /// Paths the work expects to change.
    #[serde(default)]
    pub plans: Vec<String>,
    /// A review (tests, research, design) done without seeing other work: until it is
    /// delivered, its agent is shown only this task, its unit and its intent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blind: bool,
    /// A gate put before a running task that is stuck on what only the user can do (a
    /// blocker): the task follows it from then on, so the gate's result goes to its agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocks: Option<u32>,
    /// A decide gate's options, which the user picks from (see `choices`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<Choice>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Task {
    pub n: u32,
    /// The intent it serves.
    #[serde(default)]
    pub intent: String,
    pub kind: TaskKind,
    pub title: String,
    pub text: String,
    pub inputs: Vec<Input>,
    pub causes: Vec<Cause>,
    pub agent: Option<String>,
    pub reviewer: Option<Reviewer>,
    pub gate: Option<GateKind>,
    pub steps: Vec<Step>,
    pub plans: Vec<String>,
    /// Done without seeing other work (see `State::blind_view`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blind: bool,
    /// A blocker: the running task this gate was put before (see `NewTask::blocks`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocks: Option<u32>,
    /// A choice gate: the options the user picks from (see `NewTask::choice`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<Choice>,
    pub state: TaskState,
    /// Who recorded it.
    pub by: String,
    pub at: String,
    pub started: Option<String>,
    /// When it was delivered, or a gate submitted.
    pub done: Option<String>,
    pub note: String,
    pub evidence: Vec<Evidence>,
    pub findings: Vec<Finding>,
    pub result: Option<GateResult>,
    /// A choice gate answered: the value picked.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected: Vec<String>,
    /// Gates: where the message with the user's result is; other tasks: the note to the
    /// manager that they were delivered (or, a gate, asked for).
    #[serde(default)]
    pub delivery: Option<Delivery>,
    /// When it was last corrected after delivery.
    #[serde(default)]
    pub amended: Option<String>,
    /// A gate: since when it waits for the user (every task it follows was done then).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_since: Option<String>,
    /// Stored attachments of its output (a gate's note, a task's screenshots).
    #[serde(default)]
    pub attachments: Vec<String>,
    /// Dropped as no longer needed: when, by whom, why.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dropped: Option<Closed>,
}

/// `u1#3` -> ("u1", 3).
pub fn parse_task_ref(s: &str) -> Option<(&str, u32)> {
    let (unit, n) = s.split_once('#')?;
    Some((unit, n.parse().ok()?)).filter(|(u, _)| !u.is_empty())
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Reviewer {
    CodeReview,
    SecurityReview,
    /// How the code is organized and how the solution is built, against the project's own
    /// rules; workriver holds no style of its own.
    ArchitectureReview,
}

#[cfg(test)]
mod tests;
