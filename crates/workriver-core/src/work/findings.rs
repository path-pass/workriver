// Findings: what reviews and gates found, the suggestions on them, their decisions and
// notes, and the deferred ones in a project's backlog.
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    P1,
    P2,
    P3,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Accept,
    /// It must be fixed, but the user's way: the note says how.
    Revise,
    Reject,
    Defer,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Who {
    User,
    Agent,
}

/// A value's name as it is serialized, e.g. "running".
pub(crate) fn an_agent() -> Who {
    Who::Agent
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum FindingState {
    Open,
    Accepted,
    /// To be fixed the way the user's note says.
    Revised,
    Rejected,
    Deferred,
    /// A deferred one picked into a batch to be done now.
    Scheduled,
    /// A scheduled one whose work was delivered.
    Done,
    /// A deferred or scheduled one given up.
    Dropped,
}

impl FindingState {
    /// Accepted or revised: work is handed out for it, and it can be a cause.
    pub fn leads_to_work(self) -> bool {
        matches!(self, FindingState::Accepted | FindingState::Revised | FindingState::Scheduled)
    }
}

/// What the finder suggests the user decide, with one sentence of why.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Suggestion {
    pub decision: Decision,
    pub reason: String,
}

/// A problem as reported. `id` is assigned when it is recorded ("P2-1").
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct NewFinding {
    #[serde(default)]
    pub id: String,
    pub severity: Severity,
    pub problem: String,
    #[serde(default)]
    pub impact: String,
    /// File paths that show the problem.
    #[serde(default)]
    pub r#where: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<Suggestion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<External>,
    /// Stored attachments that show it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<String>,
}

/// One finding of one task; or (`task` 0, `unit` empty) a backlog finding by its id (`f3`).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct FindingRef {
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub task: u32,
    pub finding: String,
}

/// A deferred finding that came from outside workriver's tasks (an earlier review, a
/// notes file): it waits in its project's backlog.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct BacklogFinding {
    pub finding: Finding,
    /// The project's folder.
    pub project: String,
    /// The intent it is meant for, if any (set when it is batched).
    pub intent: Option<String>,
    /// Where it came from, in words: which review, branch, date.
    pub source: String,
    pub by: String,
    pub at: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Finding {
    #[serde(flatten)]
    pub found: NewFinding,
    pub state: FindingState,
    pub decided_by: Option<Who>,
    pub note: Option<String>,
    /// Where the message with the user's decision is.
    #[serde(default)]
    pub delivery: Option<Delivery>,
    /// Dropped: why.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// What was added to it since it was reported, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<FindingNote>,
    /// Every decision on it, oldest first (the last is its state): a decision changed by the
    /// user stays readable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<Decided>,
}

/// One decision on a finding.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Decided {
    pub state: FindingState,
    pub by: Who,
    /// The agent that decided it (the manager); none: the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    pub at: String,
}

/// An addition to a finding: by whom, when, what.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct FindingNote {
    pub by: String,
    pub at: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<String>,
}

pub(crate) fn open(found: NewFinding) -> Finding {
    Finding { found, state: FindingState::Open, decided_by: None, note: None, delivery: None, reason: None, notes: vec![], decisions: vec![] }
}

/// Ids for findings added later: per severity, after the highest there is.
pub(crate) fn numbered_after(mut add: Vec<NewFinding>, have: &[Finding]) -> Vec<NewFinding> {
    let mut seen = [0u32; 3];
    for f in have {
        let n = f.found.id.rsplit_once('-').and_then(|(_, n)| n.parse().ok()).unwrap_or(0);
        let k = f.found.severity as usize;
        seen[k] = seen[k].max(n);
    }
    for f in &mut add {
        let k = f.severity as usize;
        seen[k] += 1;
        f.id = format!("{:?}-{}", f.severity, seen[k]);
    }
    add
}

/// Ids per severity, as a reviewer numbers them: P1-1, P2-1, P2-2…
pub(crate) fn numbered(mut findings: Vec<NewFinding>) -> Vec<NewFinding> {
    let mut seen = [0; 3];
    for f in &mut findings {
        let k = f.severity as usize;
        seen[k] += 1;
        f.id = format!("{:?}-{}", f.severity, seen[k]);
    }
    findings
}

impl State {
    /// A finding by reference: a task's, or a backlog one.
    pub fn finding(&self, r: &FindingRef) -> Result<&Finding, Rejected> {
        if r.task == 0 {
            return match self.backlog.iter().find(|b| b.finding.found.id == r.finding) {
                Some(b) => Ok(&b.finding),
                None => no(format!("no backlog finding {}", r.finding)),
            };
        }
        match self.task(&r.unit, r.task)?.findings.iter().find(|f| f.found.id == r.finding) {
            Some(f) => Ok(f),
            None => no(format!("{}#{} has no finding {}", r.unit, r.task, r.finding)),
        }
    }

    pub(crate) fn finding_mut(&mut self, r: &FindingRef) -> Option<&mut Finding> {
        if r.task == 0 {
            return self.backlog.iter_mut().find(|b| b.finding.found.id == r.finding).map(|b| &mut b.finding);
        }
        self.task_mut(&r.unit, r.task)?.findings.iter_mut().find(|f| f.found.id == r.finding)
    }
}
