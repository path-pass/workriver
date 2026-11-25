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

impl Decision {
    /// The state a finding is in once decided so.
    fn state(self) -> FindingState {
        match self {
            Decision::Accept => FindingState::Accepted,
            Decision::Revise => FindingState::Revised,
            Decision::Reject => FindingState::Rejected,
            Decision::Defer => FindingState::Deferred,
        }
    }
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

/// An addition to a finding, or to what a task asks: by whom, when, what.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct FindingNote {
    pub by: String,
    pub at: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<String>,
}

pub(super) fn open(found: NewFinding) -> Finding {
    Finding { found, state: FindingState::Open, decided_by: None, note: None, delivery: None, reason: None, notes: vec![], decisions: vec![] }
}

/// Ids for findings added later: per severity, after the highest there is.
pub(super) fn numbered_after(mut add: Vec<NewFinding>, have: &[Finding]) -> Vec<NewFinding> {
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
pub(crate) fn numbered(findings: Vec<NewFinding>) -> Vec<NewFinding> {
    numbered_after(findings, &[])
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

    /// The rules of a finding's events: `prepare`, for these kinds.
    pub(crate) fn prepare_findings(&self, who: &Principal, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        let actor = who.actor();
        let user = *who == Principal::User;
        Ok(match body {
            FindingsBatched { intent, findings } => {
                let i = self.intent(&intent)?;
                if i.state != IntentState::Active {
                    return no(format!("intent {intent} is {}; start it first (workriver intent start {intent} in its manager's terminal)", name(&i.state)));
                }
                if findings.is_empty() {
                    return no("a batch needs at least one finding");
                }
                for r in &findings {
                    let f = self.finding(r)?;
                    // A task's finding belongs to its intent; a backlog one to its intent, if any, or the project.
                    let fits = if r.task == 0 {
                        let b = self.backlog.iter().find(|b| b.finding.found.id == r.finding);
                        b.is_some_and(|b| b.intent.as_deref().is_none_or(|x| x == intent))
                    } else {
                        self.intent_of(&r.unit) == Some(intent.as_str())
                    };
                    if !fits {
                        return no(format!("finding {} is not part of intent {intent}", r.finding));
                    }
                    if f.state != FindingState::Deferred {
                        return no(format!("finding {} is {}, not deferred", r.finding, name(&f.state)));
                    }
                }
                FindingsBatched { intent, findings }
            }
            FindingAdded { project, intent, finding, source, note } => {
                if blank(&project) {
                    return no("a backlog finding needs its project (--project)");
                }
                // A backlog is a registered project's: as for an intent, the folder is one a
                // registered project holds, whoever records it.
                if self.project_of(&project).is_none() {
                    return no(format!("{project} is in no registered project: name a project by its id (`workriver project list`), or ask the user to add it first (`workriver project add <folder>`, from the user's terminal)"));
                }
                if blank(&finding.problem) {
                    return no("a finding needs its problem");
                }
                if let Some(i) = &intent {
                    self.intent(i)?;
                }
                let mut finding = finding;
                finding.id = format!("f{}", self.backlog.len() + 1);
                FindingAdded { project, intent, finding, source, note }
            }
            FindingDropped { unit, task, finding, reason } => {
                let f = self.finding(&FindingRef { unit: unit.clone(), task, finding: finding.clone() })?;
                if !matches!(f.state, FindingState::Deferred | FindingState::Scheduled) {
                    return no(format!("finding {finding} is {}; only deferred or scheduled ones are dropped", name(&f.state)));
                }
                if blank(&reason) {
                    return no("dropping a finding needs a reason: --reason \"...\"");
                }
                FindingDropped { unit, task, finding, reason }
            }
            FindingNoted { unit, task, finding, text, attachments } => {
                let t = self.task(&unit, task)?;
                if !t.findings.iter().any(|f| f.found.id == finding) {
                    return no(format!("{unit}#{task} has no finding {finding}"));
                }
                // By whoever reported it, the intent's manager, or the user.
                let manager = self.intent_of(&unit).and_then(|i| self.manager_of(i)) == Some(actor);
                if !user && !manager && !self.is_agent_of(t, actor) {
                    return no(format!("a finding of {unit}#{task} is added to by the agent that delivered it, the manager of its intent, or the user"));
                }
                if blank(&text) && attachments.is_empty() {
                    return no("a note needs its text: --text \"...\" (or a file: --attach FILE)");
                }
                self.stored(attachments.iter())?;
                FindingNoted { unit, task, finding, text, attachments }
            }
            FindingDecided { unit, task, finding, decision, note, .. } => {
                let t = self.task(&unit, task)?;
                let Some(f) = t.findings.iter().find(|f| f.found.id == finding) else { return no(format!("{unit}#{task} has no finding {finding}")) };
                let to = decision.state();
                // A decision is made once; only the user changes one afterwards (the manager's,
                // or their own), and only while nothing was built on its being put off.
                let was = match f.state {
                    FindingState::Open => None,
                    FindingState::Accepted | FindingState::Revised | FindingState::Rejected | FindingState::Deferred if user && f.state != to => Some(f.state),
                    _ if user && f.state == to && decision != Decision::Revise => return no(format!("finding {finding} of {unit}#{task} is already {}", name(&f.state))),
                    _ if user && f.state == to => Some(f.state),
                    _ if user => return no(format!("finding {finding} of {unit}#{task} is {}; that is no longer changed here", name(&f.state))),
                    _ => return no(format!("finding {finding} of {unit}#{task} is already {}; only the user changes a decision", name(&f.state))),
                };
                if user && decision == Decision::Revise && blank(&note) {
                    return no("revise needs a note saying how it is to be fixed: --note \"...\"");
                }
                // An agent decides only what is the intent's manager's to decide, and only as
                // that manager; whose decision it is follows from who records it.
                if !user {
                    let intent = self.intent_of(&unit).unwrap_or_default();
                    let policy = self.intents.iter().find(|i| i.id == intent).map(|i| i.findings).unwrap_or_default();
                    if !policy.managers() {
                        return no(format!(
                            "finding {finding} of {unit}#{task} is the user's to decide (intent {intent}: findings {}); the user decides it in the web UI",
                            name(&policy)
                        ));
                    }
                    if self.manager_of(intent) != Some(actor) {
                        return no(format!("finding {finding} of {unit}#{task} is for the manager of intent {intent} to decide (or the user), not for other agents"));
                    }
                    if blank(&note) {
                        return no("an agent's finding decision needs a reason (--note \"...\")");
                    }
                }
                FindingDecided { unit, task, finding, decision, note, by: if user { Who::User } else { Who::Agent }, was }
            }
            other => unreachable!("{} is not one of a finding's kinds", name_of_kind(&other)),
        })
    }

    /// What a finding's events change: `apply`, for these kinds.
    pub(crate) fn apply_findings(&mut self, e: &Event) {
        use Body::*;
        let at = &e.at;
        let agent_of = |actor: &str| (actor != "user").then(|| actor.to_string());
        match &e.body {
            FindingsBatched { intent, findings } => {
                for r in findings {
                    if let Some(f) = self.finding_mut(r) {
                        f.state = FindingState::Scheduled;
                    }
                    if r.task == 0
                        && let Some(b) = self.backlog.iter_mut().find(|b| b.finding.found.id == r.finding)
                    {
                        b.intent = Some(intent.clone());
                    }
                }
            }
            FindingAdded { project, intent, finding, source, note } => {
                let found = finding.clone();
                self.backlog.push(BacklogFinding {
                    finding: Finding { found, state: FindingState::Deferred, decided_by: Some(Who::User), note: (!note.is_empty()).then(|| note.clone()), delivery: None, reason: None, notes: vec![], decisions: vec![] },
                    project: project.clone(),
                    intent: intent.clone(),
                    source: source.clone(),
                    by: e.actor.clone(),
                    at: at.clone(),
                });
            }
            FindingDropped { unit, task, finding, reason } => {
                if let Some(f) = self.finding_mut(&FindingRef { unit: unit.clone(), task: *task, finding: finding.clone() }) {
                    f.state = FindingState::Dropped;
                    f.reason = Some(reason.clone());
                }
            }
            FindingDecided { unit, task, finding, decision, note, by, .. } => {
                if let Some(f) = self.task_mut(unit, *task).and_then(|t| t.findings.iter_mut().find(|f| &f.found.id == finding)) {
                    f.state = decision.state();
                    f.decided_by = Some(*by);
                    f.note = (!note.is_empty()).then(|| note.clone());
                    f.decisions.push(Decided { state: f.state, by: *by, agent: agent_of(&e.actor), note: note.clone(), at: at.clone() });
                }
            }
            FindingNoted { unit, task, finding, text, attachments } => {
                if let Some(f) = self.task_mut(unit, *task).and_then(|t| t.findings.iter_mut().find(|f| &f.found.id == finding)) {
                    f.notes.push(FindingNote { by: e.actor.clone(), at: at.clone(), text: text.clone(), attachments: attachments.clone() });
                }
            }
            other => unreachable!("{} is not one of a finding's kinds", name_of_kind(other)),
        }
    }
}
