// The events workriver records, the state they add up to, and the rules a new event must
// pass. Ids and numbers are assigned here, so replaying the stored events gives the same
// state every time.
use serde::{Deserialize, Serialize};

/// A write that breaks a rule. The message is shown to whoever tried it, as is.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Rejected(pub String);

fn no<T>(msg: impl Into<String>) -> Result<T, Rejected> {
    Err(Rejected(msg.into()))
}

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

/// Where an intent is: in the backlog, being worked on, finished, or given up.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum IntentState {
    Planned,
    Active,
    Done,
    Dropped,
}

fn active() -> IntentState {
    IntentState::Active
}

/// The same item in an outside tracker (Jira, Linear, GitHub Issues); only kept and shown.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct External {
    pub system: String,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
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

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum GateKind {
    Deploy,
    Test,
    /// The user discusses a design (asking why) and confirms it; the note is the decision.
    Decide,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Reviewer {
    CodeReview,
    SecurityReview,
}

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

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum CheckResult {
    Passed,
    Failed,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum GateResult {
    Done,
    Problem,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Todo,
    Running,
    /// Gates only: the user has not done it yet.
    Waiting,
    Done,
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

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

fn user() -> String {
    "user".into()
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
}

/// Who a request was found to come from. The daemon works this out (a session of the web
/// UI or the user's token; the agent at the caller's place in the runtime); it is never
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
            GateSubmitted { .. } | ChoreDone { .. } | ThreadEnded { .. } | WorkspaceLinked(_) | WorkspaceUnlinked { .. } | ProjectAdded { .. } | FindingsBatched { .. } | FindingDropped { .. }
        )
    }
}

/// The runtime's group an intent follows (under Herdr, a workspace): its id and label.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct RuntimeRef {
    /// Unique across machines: another machine's carries its name.
    pub id: String,
    pub label: String,
    /// The machine it is on; none: this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
}

/// A workspace of another machine made one with a workspace of this machine: one manager,
/// the same intents. Workspaces are named by their labels, which outlive their ids.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Link {
    pub machine: String,
    pub workspace: String,
    /// The label of the workspace on this machine.
    pub local: String,
}

/// An agent session as last seen: the name shown for it, and its harness.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Agent {
    pub session: String,
    pub name: String,
    pub harness: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum DeliveryStep {
    NotDelivered,
    Delivered,
    /// The receiver started working after it arrived.
    Picked,
}

/// Why a message was not delivered.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum ReasonCode {
    /// The intent has no manager registered.
    NoManager,
    /// The manager is not running.
    ManagerGone,
    /// The agent runtime cannot be reached.
    RuntimeDown,
    /// The runtime refused or failed; `reason` holds its words.
    RuntimeError,
    /// The intent follows no runtime group, and no agent of it runs: nowhere to start one.
    NoWorkspace,
    /// No answer came in time.
    Timeout,
    /// The service restarted while waiting.
    Restarted,
}

/// What a question is about besides its task: a finding, a file, or lines of a file.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct AskContext {
    /// "task", "finding", "file" or "lines".
    pub kind: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<u32>,
}

/// Where the message about a user's input (a decision, a gate result) is.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Delivery {
    pub step: DeliveryStep,
    /// The receiving session; None when there was none.
    pub to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<ReasonCode>,
    /// Why it was not delivered, in words (the runtime's own, for a runtime error).
    pub reason: Option<String>,
    pub message: String,
}

/// One recorded event's content. `kind` and `body` are the columns of the same name.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "kind", content = "body")]
pub enum Body {
    #[serde(rename = "intent.created")]
    IntentCreated {
        #[serde(default)]
        id: String,
        title: String,
        /// One sentence on what the intent is for.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        goal: String,
        /// Before 0.2, an intent followed one runtime group; kept for replay.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        runtime_ref: Option<RuntimeRef>,
        /// The project's folder; empty before projects existed.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        project: String,
        /// Records before 0.2 have none: those intents were being worked on.
        #[serde(default = "active")]
        state: IntentState,
        /// More about it; may hold links.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        note: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        external: Option<External>,
    },
    /// A project: a folder holding one or more repositories.
    #[serde(rename = "project.added")]
    ProjectAdded {
        #[serde(default)]
        id: String,
        path: String,
        title: String,
    },
    /// Planned → active (started in a runtime group) → done; or dropped; or back to planned.
    #[serde(rename = "intent.state_changed")]
    IntentStateChanged {
        intent: String,
        state: IntentState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        /// Where it is started: the runtime group whose manager runs it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace: Option<RuntimeRef>,
    },
    /// A planned intent's place in its project's roadmap.
    #[serde(rename = "intent.moved")]
    IntentMoved {
        intent: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<String>,
    },
    /// The user picked deferred findings to do together: they go to the intent's manager.
    #[serde(rename = "findings.batched")]
    FindingsBatched { intent: String, findings: Vec<FindingRef> },
    #[serde(rename = "finding.dropped")]
    FindingDropped {
        /// Empty, with `task` 0, for a backlog finding.
        #[serde(default)]
        unit: String,
        #[serde(default)]
        task: u32,
        finding: String,
        reason: String,
    },
    /// A workspace of another machine becomes one with a workspace of this machine.
    #[serde(rename = "workspace.linked")]
    WorkspaceLinked(Link),
    #[serde(rename = "workspace.unlinked")]
    WorkspaceUnlinked { machine: String, workspace: String },
    /// A deferred finding from outside workriver's tasks, into a project's backlog.
    #[serde(rename = "finding.added")]
    FindingAdded {
        /// The project's folder.
        project: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        intent: Option<String>,
        /// Its fields; the id (`f3`) is assigned here.
        finding: NewFinding,
        #[serde(default)]
        source: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        note: String,
    },
    /// The intent now follows another runtime group (its id changed, say).
    #[serde(rename = "intent.linked")]
    IntentLinked { intent: String, runtime_ref: RuntimeRef },
    #[serde(rename = "agent.seen")]
    AgentSeen { session: String, name: String, harness: String },
    #[serde(rename = "manager.registered")]
    ManagerRegistered {
        /// Before 0.2: the intent it managed (whose runtime group it ran in).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        intent: Option<String>,
        /// The agent's session; empty when recorded: whoever records it.
        #[serde(default)]
        agent: String,
        /// The runtime group it manages.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace: Option<RuntimeRef>,
    },
    /// A new title or goal; only the intent's manager or the user may change them.
    #[serde(rename = "intent.updated")]
    IntentUpdated {
        intent: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        goal: Option<String>,
    },
    #[serde(rename = "unit.created")]
    UnitCreated {
        #[serde(default)]
        id: String,
        intent: String,
        title: String,
        #[serde(default)]
        purpose: String,
        /// Absolute path of the repository (or a worktree of it).
        repo: String,
        branch: String,
        base: String,
        /// The machine the repository is on; none: this one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
    },
    #[serde(rename = "unit.merged")]
    UnitMerged {
        unit: String,
        /// The unit's commits when it was merged, oldest first; the branch may be gone later.
        #[serde(default)]
        commits: Vec<String>,
    },
    #[serde(rename = "task.created")]
    TaskCreated(NewTask),
    #[serde(rename = "task.started")]
    TaskStarted {
        unit: String,
        task: u32,
        #[serde(default)]
        agent: Option<String>,
    },
    #[serde(rename = "task.delivered")]
    TaskDelivered {
        unit: String,
        task: u32,
        #[serde(default)]
        note: String,
        #[serde(default)]
        evidence: Vec<Evidence>,
        #[serde(default)]
        findings: Vec<NewFinding>,
        /// Stored attachments of the output (screenshots, reports).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<String>,
    },
    /// A correction to a delivered task: what is given replaces what was delivered.
    #[serde(rename = "task.amended")]
    TaskAmended {
        unit: String,
        task: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        evidence: Option<Vec<Evidence>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        findings: Option<Vec<NewFinding>>,
        /// More stored attachments of the output (added to those it has).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<String>,
    },
    #[serde(rename = "finding.decided")]
    FindingDecided {
        unit: String,
        task: u32,
        finding: String,
        decision: Decision,
        #[serde(default)]
        note: String,
        /// Whose decision; set from who records it.
        #[serde(default = "an_agent")]
        by: Who,
    },
    #[serde(rename = "gate.submitted")]
    GateSubmitted {
        unit: String,
        task: u32,
        result: GateResult,
        #[serde(default)]
        note: String,
        #[serde(default)]
        findings: Vec<NewFinding>,
        /// Stored attachments with the note.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<String>,
    },
    /// A file was put in the store (once per content; added again only to link it elsewhere),
    /// and linked to a task or one of its findings when given.
    #[serde(rename = "attachment.added")]
    AttachmentAdded {
        id: String,
        name: String,
        #[serde(rename = "type")]
        mime: String,
        size: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        finding: Option<String>,
    },
    /// Where a message is: about a task (a decision when `finding` is set, a gate result, a
    /// delivered task, a gate asked for), or about a chore (`chore` set; then no unit/task).
    #[serde(rename = "delivery.updated")]
    DeliveryUpdated {
        #[serde(default)]
        unit: String,
        #[serde(default)]
        task: u32,
        #[serde(default)]
        finding: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chore: Option<String>,
        #[serde(flatten)]
        delivery: Delivery,
    },
    /// The user asks why about a task; a temporary agent answers.
    #[serde(rename = "question.asked")]
    QuestionAsked {
        #[serde(default)]
        id: String,
        /// The thread it continues; empty starts a new one (whose id is this question's).
        #[serde(default)]
        thread: String,
        unit: String,
        task: u32,
        text: String,
        #[serde(default)]
        context: Vec<AskContext>,
        /// A request to write the thread's conclusion rather than a question.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        conclude: bool,
    },
    /// The temporary agent that answers it is running.
    #[serde(rename = "question.started")]
    QuestionStarted {
        question: String,
        agent: String,
        name: String,
        /// False when the question went straight to the task's own agent, which is never closed.
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        temporary: bool,
    },
    #[serde(rename = "question.answered")]
    QuestionAnswered {
        question: String,
        text: String,
        /// The agent that did the work, if the answerer asked it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        asked: Option<String>,
        /// The answer is the thread's conclusion.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        conclusion: bool,
    },
    /// The user ends a thread; its temporary agent is closed.
    #[serde(rename = "thread.ended")]
    ThreadEnded { thread: String },
    #[serde(rename = "question.failed")]
    QuestionFailed { question: String, reason_code: ReasonCode, reason: String },
    #[serde(rename = "chore.created")]
    ChoreCreated {
        #[serde(default)]
        id: String,
        intent: String,
        #[serde(default)]
        unit: Option<String>,
        title: String,
        #[serde(default)]
        steps: Vec<Step>,
    },
    #[serde(rename = "chore.done")]
    ChoreDone { chore: String },
}

impl Body {
    /// The intent, unit and task (`u1#3`) an event concerns, for the indexed columns.
    pub fn refs(&self) -> (Option<String>, Option<String>, Option<String>) {
        use Body::*;
        let task = |u: &str, n: u32| Some(format!("{u}#{n}"));
        match self {
            IntentCreated { id, .. } => (Some(id.clone()), None, None),
            IntentLinked { intent, .. } => (Some(intent.clone()), None, None),
            AgentSeen { .. } | QuestionStarted { .. } | QuestionAnswered { .. } | QuestionFailed { .. } | ThreadEnded { .. } => (None, None, None),
            QuestionAsked { unit, task: n, .. } => (None, Some(unit.clone()), task(unit, *n)),
            ManagerRegistered { intent, .. } => (intent.clone(), None, None),
            IntentUpdated { intent, .. } | IntentStateChanged { intent, .. } | IntentMoved { intent, .. } | FindingsBatched { intent, .. } => (Some(intent.clone()), None, None),
            ProjectAdded { .. } => (None, None, None),
            FindingDropped { unit, task: n, .. } => (None, (!unit.is_empty()).then(|| unit.clone()), (!unit.is_empty()).then(|| task(unit, *n)).flatten()),
            FindingAdded { intent, .. } => (intent.clone(), None, None),
            WorkspaceLinked(_) | WorkspaceUnlinked { .. } => (None, None, None),
            DeliveryUpdated { chore: Some(_), .. } => (None, None, None),
            UnitCreated { id, intent, .. } => (Some(intent.clone()), Some(id.clone()), None),
            UnitMerged { unit, .. } => (None, Some(unit.clone()), None),
            TaskCreated(t) => ((!t.intent.is_empty()).then(|| t.intent.clone()), Some(t.unit.clone()), task(&t.unit, t.n)),
            TaskStarted { unit, task: n, .. }
            | TaskDelivered { unit, task: n, .. }
            | TaskAmended { unit, task: n, .. }
            | FindingDecided { unit, task: n, .. }
            | GateSubmitted { unit, task: n, .. }
            | DeliveryUpdated { unit, task: n, .. } => (None, Some(unit.clone()), task(unit, *n)),
            ChoreCreated { intent, unit, .. } => (Some(intent.clone()), unit.clone(), None),
            ChoreDone { .. } => (None, None, None),
            AttachmentAdded { unit, task: n, .. } => (None, unit.clone(), unit.as_ref().zip(*n).and_then(|(u, n)| task(u, n))),
        }
    }
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
    /// Not the records but a part of them: what an agent on a blind task may see.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reduced: bool,
}

/// A stored file. Never deleted, like the records.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub mime: String,
    pub size: u64,
    pub by: String,
    pub at: String,
}

/// A conversation about a task: its questions share one temporary agent while it lives.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Thread {
    pub id: String,
    pub unit: String,
    pub task: u32,
    /// The temporary agent last started for it: session and name.
    pub agent: Option<String>,
    pub name: Option<String>,
    pub ended: bool,
    pub conclusion: Option<String>,
}

/// A question about a task and how it fared.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Question {
    pub id: String,
    pub thread: String,
    pub conclude: bool,
    pub unit: String,
    pub task: u32,
    pub text: String,
    pub context: Vec<AskContext>,
    pub at: String,
    /// Who asked: "user", or an agent's session.
    #[serde(default = "user")]
    pub by: String,
    /// The agent answering it: its session and name, once it has the question.
    pub agent: Option<String>,
    pub name: Option<String>,
    /// That agent was started for it (and is closed later), not the task's own.
    #[serde(default = "yes")]
    pub temporary: bool,
    pub answer: Option<String>,
    pub asked: Option<String>,
    pub answered_by: Option<String>,
    pub failed: Option<(ReasonCode, String)>,
}

impl Question {
    pub fn open(&self) -> bool {
        self.answer.is_none() && self.failed.is_none()
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Intent {
    pub id: String,
    pub title: String,
    /// Before 0.2: the manager registered for it. Now the manager is the workspace's.
    pub manager: Option<String>,
    #[serde(default)]
    pub goal: String,
    /// Before 0.2: the runtime group it followed.
    #[serde(default)]
    pub runtime_ref: Option<RuntimeRef>,
    pub at: String,
    /// The project folder ("" when not known: then its units' repositories say).
    #[serde(default)]
    pub project: String,
    #[serde(default = "active")]
    pub state: IntentState,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub external: Option<External>,
    /// Where it runs while active (and last ran).
    #[serde(default)]
    pub workspace: Option<RuntimeRef>,
    /// Why it was dropped.
    #[serde(default)]
    pub reason: Option<String>,
    /// Intent-level work (research, design), numbered `i3#1`…
    #[serde(default)]
    pub tasks: Vec<Task>,
}

/// A registered project folder.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Project {
    pub id: String,
    pub path: String,
    pub title: String,
}

/// A runtime group (a Herdr workspace): its manager and the intent new work goes to.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Workspace {
    pub group: RuntimeRef,
    pub manager: Option<String>,
    pub current: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Unit {
    pub id: String,
    pub intent: String,
    pub title: String,
    pub purpose: String,
    pub repo: String,
    pub branch: String,
    pub base: String,
    /// The machine its repository is on; none: this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    pub at: String,
    /// When it was recorded as merged.
    pub merged: Option<String>,
    pub merged_commits: Vec<String>,
    pub tasks: Vec<Task>,
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
    /// Gates: where the message with the user's result is; other tasks: the note to the
    /// manager that they were delivered (or, a gate, asked for).
    #[serde(default)]
    pub delivery: Option<Delivery>,
    /// When it was last corrected after delivery.
    #[serde(default)]
    pub amended: Option<String>,
    /// Stored attachments of its output (a gate's note, a task's screenshots).
    #[serde(default)]
    pub attachments: Vec<String>,
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
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Chore {
    pub id: String,
    pub intent: String,
    pub unit: Option<String>,
    pub title: String,
    pub by: String,
    pub steps: Vec<Step>,
    pub done: bool,
    pub at: String,
    /// The note to the manager that the user was asked.
    #[serde(default)]
    pub delivery: Option<Delivery>,
}

/// `u1#3` -> ("u1", 3).
pub fn parse_task_ref(s: &str) -> Option<(&str, u32)> {
    let (unit, n) = s.split_once('#')?;
    Some((unit, n.parse().ok()?)).filter(|(u, _)| !u.is_empty())
}

/// A value's name as it is serialized, e.g. "running".
fn an_agent() -> Who {
    Who::Agent
}

/// An event's kind, as recorded ("gate.submitted").
fn name_of_kind(b: &Body) -> String {
    serde_json::to_value(b).ok().and_then(|v| v["kind"].as_str().map(str::to_string)).unwrap_or_default()
}

pub fn name<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

fn blank(s: &str) -> bool {
    s.trim().is_empty()
}

impl State {
    pub fn unit(&self, id: &str) -> Result<&Unit, Rejected> {
        match self.units.iter().find(|u| u.id == id) {
            Some(u) => Ok(u),
            None => no(format!("no unit {id}")),
        }
    }

    /// The tasks of a unit, or an intent's intent-level tasks.
    pub fn tasks_of(&self, owner: &str) -> Option<&Vec<Task>> {
        self.units.iter().find(|u| u.id == owner).map(|u| &u.tasks).or_else(|| self.intents.iter().find(|i| i.id == owner).map(|i| &i.tasks))
    }

    /// A task by its owner (a unit, or an intent for intent-level work) and number.
    pub fn task(&self, unit: &str, n: u32) -> Result<&Task, Rejected> {
        let Some(tasks) = self.tasks_of(unit) else { return no(format!("no unit or intent {unit}")) };
        match tasks.iter().find(|t| t.n == n) {
            Some(t) => Ok(t),
            None => no(format!("no task {unit}#{n}")),
        }
    }

    /// A unit, or an intent seen as a unit for its intent-level tasks (no branch; its
    /// project folder as the repository).
    pub fn unit_or_intent(&self, id: &str) -> Option<Unit> {
        if let Ok(u) = self.unit(id) {
            return Some(u.clone());
        }
        let i = self.intents.iter().find(|i| i.id == id)?;
        Some(Unit {
            id: i.id.clone(),
            intent: i.id.clone(),
            title: i.title.clone(),
            purpose: i.goal.clone(),
            repo: i.project.clone(),
            branch: String::new(),
            base: String::new(),
            machine: None,
            at: i.at.clone(),
            merged: None,
            merged_commits: vec![],
            tasks: i.tasks.clone(),
        })
    }

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

    /// The intent a unit (or intent) belongs to.
    pub fn intent_of(&self, owner: &str) -> Option<&str> {
        self.units.iter().find(|u| u.id == owner).map(|u| u.intent.as_str()).or_else(|| self.intents.iter().find(|i| i.id == owner).map(|i| i.id.as_str()))
    }

    /// A runtime group's record: by its id, or else by its label, since a runtime may give
    /// its groups new ids when it restarts.
    pub fn workspace_of(&self, r: &RuntimeRef) -> Option<&Workspace> {
        self.workspace_at(r).map(|k| &self.workspaces[k])
    }

    /// The blind tasks an agent is on: assigned to it or started by it, not delivered yet.
    pub fn blind_tasks(&self, session: &str) -> Vec<(String, u32)> {
        let name = self.name_of(session);
        let mine = |t: &&Task| t.blind && t.state != TaskState::Done && t.agent.as_deref().is_some_and(|a| a == session || Some(a) == name);
        let units = self.units.iter().flat_map(|u| u.tasks.iter().filter(mine).map(|t| (u.id.clone(), t.n)));
        units.chain(self.intents.iter().flat_map(|i| i.tasks.iter().filter(mine).map(|t| (i.id.clone(), t.n)))).collect()
    }

    /// What an agent on a blind task may see until it delivers: its task (as handed out),
    /// the task's unit and intent, what it attached itself. None for anyone else.
    ///
    /// Everything else is left out: other tasks with their output, notes, evidence, findings
    /// and decisions, questions, deliveries, the backlog, other units and intents.
    pub fn blind_view(&self, session: &str) -> Option<State> {
        let mine = self.blind_tasks(session);
        if mine.is_empty() {
            return None;
        }
        let own = |owner: &str, tasks: &[Task]| -> Vec<Task> {
            tasks
                .iter()
                .filter(|t| mine.iter().any(|(o, n)| o == owner && *n == t.n))
                // What the task follows and answers would name other work.
                .map(|t| Task { causes: vec![], delivery: None, ..t.clone() })
                .collect()
        };
        let units: Vec<Unit> = self.units.iter().filter(|u| mine.iter().any(|(o, _)| o == &u.id)).map(|u| Unit { tasks: own(&u.id, &u.tasks), ..u.clone() }).collect();
        let intents = self
            .intents
            .iter()
            .filter(|i| mine.iter().any(|(o, _)| o == &i.id) || units.iter().any(|u| u.intent == i.id))
            .map(|i| Intent { note: String::new(), reason: None, tasks: own(&i.id, &i.tasks), ..i.clone() })
            .collect();
        Some(State {
            intents,
            units,
            agents: self.agents.clone(),
            attachments: self.attachments.iter().filter(|a| a.by == session).cloned().collect(),
            projects: self.projects.clone(),
            workspaces: self.workspaces.clone(),
            links: self.links.clone(),
            reduced: true,
            ..State::default()
        })
    }

    /// The link of a group of another machine, if it has one.
    pub fn link_of(&self, r: &RuntimeRef) -> Option<&Link> {
        let m = r.machine.as_ref()?;
        self.links.iter().find(|l| &l.machine == m && l.workspace == r.label)
    }

    /// Where a group's record is. A linked group of another machine is the workspace of this
    /// machine it is linked to; nothing else is ever matched across machines.
    fn workspace_at(&self, r: &RuntimeRef) -> Option<usize> {
        if let Some(l) = self.link_of(r) {
            return self.workspaces.iter().position(|w| w.group.machine.is_none() && w.group.label == l.local);
        }
        let here = |w: &&Workspace| w.group.machine == r.machine;
        let at = |f: &dyn Fn(&Workspace) -> bool| self.workspaces.iter().position(|w| here(&w) && f(w));
        at(&|w| w.group.id == r.id).or_else(|| at(&|w| w.group.label == r.label))
    }

    /// The intent new work in a runtime group goes to: the group's current intent.
    pub fn intent_for(&self, r: &RuntimeRef) -> Option<&Intent> {
        let id = self.workspace_of(r)?.current.as_deref()?;
        self.intents.iter().find(|i| i.id == id)
    }

    /// The manager of an intent: the manager of the runtime group it runs in (before 0.2,
    /// the one registered for the intent itself).
    pub fn manager_of(&self, intent: &str) -> Option<&str> {
        let i = self.intents.iter().find(|i| i.id == intent)?;
        i.workspace.as_ref().and_then(|g| self.workspace_of(g)).and_then(|w| w.manager.as_deref()).or(i.manager.as_deref())
    }

    /// The name last seen for an agent session.
    pub fn name_of(&self, session: &str) -> Option<&str> {
        self.agents.iter().find(|a| a.session == session).map(|a| a.name.as_str())
    }

    pub fn question(&self, id: &str) -> Result<&Question, Rejected> {
        match self.questions.iter().find(|q| q.id == id) {
            Some(q) => Ok(q),
            None => no(format!("no question {id}")),
        }
    }

    /// Every id names a stored attachment.
    fn stored<'a>(&self, ids: impl Iterator<Item = &'a String>) -> Result<(), Rejected> {
        for id in ids {
            if !self.attachments.iter().any(|a| &a.id == id) {
                return no(format!("no stored attachment {id}; store the file first (workriver attach)"));
            }
        }
        Ok(())
    }

    pub fn intent(&self, id: &str) -> Result<&Intent, Rejected> {
        match self.intents.iter().find(|i| i.id == id) {
            Some(i) => Ok(i),
            None => no(format!("no intent {id}")),
        }
    }

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
        Ok(match body {
            IntentCreated { title, goal, runtime_ref, project, state, note, external, .. } => {
                if blank(&title) {
                    return no("an intent needs a title");
                }
                if runtime_ref.as_ref().is_some_and(|r| blank(&r.id)) {
                    return no("a runtime group needs an id");
                }
                if !matches!(state, IntentState::Planned | IntentState::Active) {
                    return no("a new intent is planned (in the backlog) or active");
                }
                IntentCreated { id: format!("i{}", self.intents.len() + 1), title, goal, runtime_ref, project, state, note, external }
            }
            ProjectAdded { path, title, .. } => {
                if blank(&path) || blank(&title) {
                    return no("a project needs its folder and a title");
                }
                if self.projects.iter().any(|p| p.path == path) {
                    return no(format!("{path} is already a project"));
                }
                ProjectAdded { id: format!("p{}", self.projects.len() + 1), path, title }
            }
            IntentStateChanged { intent, state, reason, workspace } => {
                let i = self.intent(&intent)?;
                use IntentState::*;
                match (i.state, state) {
                    (Planned, Active) => {
                        let Some(g) = &workspace else { return no(format!("start {intent} from its manager's pane: workriver intent start {intent}")) };
                        let manager = self.workspace_of(g).and_then(|w| w.manager.as_deref());
                        if !user && manager != Some(actor) {
                            return no(format!("only the manager of workspace {} starts intents there; register with `workriver manager here` first", g.label));
                        }
                    }
                    (Active, Done) | (Active, Planned) => {}
                    (Planned, Dropped) | (Active, Dropped) => {
                        if !user && self.manager_of(&intent) != Some(actor) {
                            return no(format!("only the user or the manager of intent {intent} drops it"));
                        }
                        if reason.as_deref().is_none_or(blank) {
                            return no("dropping an intent needs a reason: --reason \"...\"");
                        }
                    }
                    (from, to) => return no(format!("intent {intent} is {}; it cannot become {}", name(&from), name(&to))),
                }
                IntentStateChanged { intent, state, reason, workspace }
            }
            IntentMoved { intent, before, after } => {
                let i = self.intent(&intent)?;
                if i.state != IntentState::Planned {
                    return no(format!("intent {intent} is not in the roadmap (it is {})", name(&i.state)));
                }
                let Some(other) = before.as_ref().or(after.as_ref()).filter(|_| before.is_none() || after.is_none()) else {
                    return no("move it --before or --after one other planned intent");
                };
                let o = self.intent(other)?;
                if o.state != IntentState::Planned || other == &intent {
                    return no(format!("{other} is not another planned intent"));
                }
                IntentMoved { intent, before, after }
            }
            FindingsBatched { intent, findings } => {
                let i = self.intent(&intent)?;
                if i.state != IntentState::Active {
                    return no(format!("intent {intent} is {}; start it first (workriver intent start {intent} in its manager's pane)", name(&i.state)));
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
            WorkspaceLinked(l) => {
                if blank(&l.machine) || blank(&l.workspace) || blank(&l.local) {
                    return no("a link names a machine, its workspace and a workspace of this machine");
                }
                if let Some(old) = self.links.iter().find(|x| x.machine == l.machine && x.workspace == l.workspace) {
                    return no(format!("{}:{} is already linked to {}; unlink it first", l.machine, l.workspace, old.local));
                }
                WorkspaceLinked(l)
            }
            WorkspaceUnlinked { machine, workspace } => {
                if !self.links.iter().any(|x| x.machine == machine && x.workspace == workspace) {
                    return no(format!("{machine}:{workspace} is not linked"));
                }
                WorkspaceUnlinked { machine, workspace }
            }
            FindingAdded { project, intent, finding, source, note } => {
                if blank(&project) {
                    return no("a backlog finding needs its project (--project)");
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
            IntentLinked { intent, runtime_ref } => {
                self.intent(&intent)?;
                if blank(&runtime_ref.id) {
                    return no("a runtime group needs an id");
                }
                IntentLinked { intent, runtime_ref }
            }
            AgentSeen { session, name, harness } => {
                if blank(&session) || blank(&name) {
                    return no("an agent needs a session and a name");
                }
                AgentSeen { session, name, harness }
            }
            IntentUpdated { intent, title, goal } => {
                self.intent(&intent)?;
                if !user && self.manager_of(&intent) != Some(actor) {
                    return no(format!("only the user or the manager of intent {intent} may change it"));
                }
                if title.as_deref().is_some_and(blank) {
                    return no("an intent needs a title");
                }
                if title.is_none() && goal.is_none() {
                    return no("nothing to change: give a title or a goal");
                }
                IntentUpdated { intent, title, goal }
            }
            ManagerRegistered { intent, agent, workspace } => {
                if let Some(i) = &intent {
                    self.intent(i)?;
                }
                if workspace.as_ref().is_some_and(|g| blank(&g.id)) || (workspace.is_none() && intent.is_none()) {
                    return no("a manager manages a runtime workspace: run `workriver manager here` in its pane");
                }
                // An agent registers itself; nobody is made a manager by someone else.
                let agent = match who {
                    Principal::Agent { session, .. } if blank(&agent) || &agent == session => session.clone(),
                    Principal::Agent { .. } => return no("an agent registers itself as manager, not another agent"),
                    _ => return no("a manager is an agent: run `workriver manager here` in the manager agent's pane"),
                };
                ManagerRegistered { intent, agent, workspace }
            }
            UnitCreated { intent, title, purpose, repo, branch, base, machine, .. } => {
                let i = self.intent(&intent)?;
                if matches!(i.state, IntentState::Done | IntentState::Dropped) {
                    return no(format!("intent {intent} is {}; it takes no new units", name(&i.state)));
                }
                for (field, v) in [("title", &title), ("repo", &repo), ("branch", &branch), ("base", &base)] {
                    if blank(v) {
                        return no(format!("a unit needs a {field}"));
                    }
                }
                let id = format!("u{}", self.units.len() + 1);
                UnitCreated { id, intent, title, purpose, repo, branch, base, machine }
            }
            UnitMerged { unit, commits } => {
                if self.unit(&unit)?.merged.is_some() {
                    return no(format!("unit {unit} is already merged"));
                }
                UnitMerged { unit, commits }
            }
            TaskCreated(t) => TaskCreated(self.prepare_task(t)?),
            TaskStarted { unit, task, agent } => {
                let t = self.task(&unit, task)?;
                if t.kind == TaskKind::Gate {
                    return no(format!("{unit}#{task} is a gate: only the user does it, with gate submit"));
                }
                if t.state != TaskState::Todo {
                    return no(format!("{unit}#{task} is {}, not todo", name(&t.state)));
                }
                TaskStarted { unit, task, agent }
            }
            TaskDelivered { unit, task, note, evidence, findings, attachments } => {
                let t = self.task(&unit, task)?;
                if t.kind == TaskKind::Gate {
                    return no(format!("{unit}#{task} is a gate: the user submits it with gate submit"));
                }
                if t.state == TaskState::Done {
                    return no(format!("{unit}#{task} is already delivered; to correct it, run `workriver task deliver {unit}#{task} --amend` with what replaces it"));
                }
                check_output(t, &evidence, &findings)?;
                self.stored(evidence.iter().filter_map(|e| e.attachment.as_ref()).chain(findings.iter().flat_map(|f| &f.attachments)).chain(&attachments))?;
                if t.kind == TaskKind::Design && blank(&note) {
                    return no("a design task delivers its design as the note: --note \"...\"");
                }
                if t.kind == TaskKind::Research && blank(&note) && attachments.is_empty() {
                    return no("a research task delivers what it found: a --note, and its report attached (workriver attach --dir)");
                }
                TaskDelivered { unit, task, note, evidence, findings: numbered(findings), attachments }
            }
            TaskAmended { unit, task, note, evidence, findings, attachments } => {
                let t = self.task(&unit, task)?;
                if t.kind == TaskKind::Gate || t.state != TaskState::Done {
                    return no(format!("{unit}#{task} is not a delivered task; only a delivered task can be amended"));
                }
                if note.is_none() && evidence.is_none() && findings.is_none() && attachments.is_empty() {
                    return no("nothing to amend: give a note, evidence or findings");
                }
                if findings.is_some() && t.findings.iter().any(|f| f.state != FindingState::Open) {
                    return no(format!(
                        "some findings of {unit}#{task} are already decided, so they cannot be replaced; record the new findings in a new review or tests task"
                    ));
                }
                check_output(t, evidence.as_deref().unwrap_or_default(), findings.as_deref().unwrap_or_default())?;
                self.stored(
                    evidence.iter().flatten().filter_map(|e| e.attachment.as_ref()).chain(findings.iter().flatten().flat_map(|f| &f.attachments)).chain(&attachments),
                )?;
                TaskAmended { unit, task, note, evidence, findings: findings.map(numbered), attachments }
            }
            FindingDecided { unit, task, finding, decision, note, by } => {
                let t = self.task(&unit, task)?;
                match t.findings.iter().find(|f| f.found.id == finding) {
                    None => return no(format!("{unit}#{task} has no finding {finding}")),
                    Some(f) if f.state != FindingState::Open => {
                        return no(format!("finding {finding} of {unit}#{task} is already {}", name(&f.state)));
                    }
                    Some(_) => {}
                }
                if decision == Decision::Revise && blank(&note) {
                    return no("revise needs a note saying how it is to be fixed: --note \"...\"");
                }
                // Whose decision it is follows from who records it, whatever the event says.
                let _ = by;
                FindingDecided { unit, task, finding, decision, note, by: if user { Who::User } else { Who::Agent } }
            }
            GateSubmitted { unit, task, result, note, findings, attachments } => {
                self.stored(attachments.iter().chain(findings.iter().flat_map(|f| &f.attachments)))?;
                let t = self.task(&unit, task)?;
                if t.kind != TaskKind::Gate || t.state != TaskState::Waiting {
                    return no(format!("{unit}#{task} is not a waiting gate"));
                }
                match result {
                    GateResult::Problem if findings.is_empty() => return no("a gate with a problem needs at least one finding"),
                    GateResult::Done if !findings.is_empty() => return no("a gate that is done carries no findings; submit it as problem"),
                    _ => {}
                }
                // The user at a gate may not know where the problem sits; a problem is enough.
                if findings.iter().any(|f| blank(&f.problem)) {
                    return no("a finding needs a problem");
                }
                GateSubmitted { unit, task, result, note, findings: numbered(findings), attachments }
            }
            AttachmentAdded { id, name, mime, size, unit, task, finding } => {
                if blank(&id) || blank(&name) {
                    return no("an attachment needs its id and name");
                }
                match (&unit, task, &finding) {
                    (None, None, None) => {}
                    (Some(u), Some(n), f) => {
                        let t = self.task(u, n)?;
                        if let Some(f) = f
                            && !t.findings.iter().any(|x| &x.found.id == f)
                        {
                            return no(format!("{u}#{n} has no finding {f}"));
                        }
                    }
                    _ => return no("an attachment is linked to a task (unit and number), or to nothing"),
                }
                AttachmentAdded { id, name, mime, size, unit, task, finding }
            }
            DeliveryUpdated { unit, task, finding, chore, delivery } => {
                if let Some(c) = &chore {
                    if !self.chores.iter().any(|x| &x.id == c) {
                        return no(format!("no chore {c}"));
                    }
                } else if task == 0 {
                    // A backlog finding's (a batch's message is recorded on its first finding).
                    self.finding(&FindingRef { unit: String::new(), task: 0, finding: finding.clone().unwrap_or_default() })?;
                } else {
                    let t = self.task(&unit, task)?;
                    if let Some(id) = &finding
                        && !t.findings.iter().any(|f| &f.found.id == id)
                    {
                        return no(format!("{unit}#{task} has no finding {id}"));
                    }
                }
                DeliveryUpdated { unit, task, finding, chore, delivery }
            }
            ChoreCreated { intent, unit, title, steps, .. } => {
                self.intent(&intent)?;
                if let Some(u) = &unit
                    && self.unit(u)?.intent != intent
                {
                    return no(format!("unit {u} is not part of intent {intent}"));
                }
                if blank(&title) {
                    return no("a chore needs a title");
                }
                ChoreCreated { id: format!("c{}", self.chores.len() + 1), intent, unit, title, steps }
            }
            QuestionAsked { thread, unit, task, text, context, conclude, .. } => {
                let about = self.task(&unit, task)?;
                // Blind work neither asks nor is asked by other agents: both would tell it things.
                if !self.blind_tasks(actor).is_empty() {
                    return no("you are on a blind task: other work is hidden until you deliver, and so are questions. If you cannot go on without one, tell your manager instead");
                }
                if about.blind && about.state != TaskState::Done && !user {
                    return no(format!("{unit}#{task} is a blind task in progress: its agent is not to be told about other work. Ask once it is delivered"));
                }
                if blank(&text) {
                    return no("a question needs its text");
                }
                let id = format!("q{}", self.questions.len() + 1);
                let thread = if thread.is_empty() {
                    if conclude {
                        return no("a conclusion is written for a thread; name it");
                    }
                    id.clone()
                } else {
                    match self.threads.iter().find(|t| t.id == thread) {
                        None => return no(format!("no thread {thread}")),
                        Some(t) if t.unit != unit || t.task != task => return no(format!("thread {thread} is about another task")),
                        Some(t) if t.ended => return no(format!("thread {thread} has ended; start a new one")),
                        Some(_) => thread,
                    }
                };
                QuestionAsked { id, thread, unit, task, text, context, conclude }
            }
            QuestionStarted { question, agent, name, temporary } => {
                let q = self.question(&question)?;
                if !q.open() || q.agent.is_some() {
                    return no(format!("question {question} is not waiting for an agent"));
                }
                QuestionStarted { question, agent, name, temporary }
            }
            QuestionAnswered { question, text, asked, conclusion } => {
                let q = self.question(&question)?;
                if !q.open() {
                    return no(format!("question {question} is already {}", if q.answer.is_some() { "answered" } else { "closed" }));
                }
                // Only the thread's temporary agent (known by session or by name), or the user.
                let t = self.threads.iter().find(|t| t.id == q.thread);
                let name = t.and_then(|t| t.name.as_deref()).or(q.name.as_deref());
                // The task's own agent may always answer about its work.
                let own = self.task(&q.unit, q.task).ok().and_then(|x| x.agent.as_deref()) == Some(actor);
                let mine = own
                    || [q.agent.as_deref(), t.and_then(|t| t.agent.as_deref())].contains(&Some(actor))
                    || [name, q.name.as_deref()].iter().any(|n| n.is_some() && self.name_of(actor) == *n);
                if !user && !mine {
                    return no(format!("question {question} is answered by the agent started for it ({}), or by the user", name.unwrap_or("not started yet")));
                }
                if blank(&text) {
                    return no("an answer needs its text");
                }
                if conclusion != q.conclude {
                    return no(if q.conclude { format!("{question} asks for the conclusion: answer with --conclusion") } else { format!("{question} is a question, not a request for a conclusion: answer without --conclusion") });
                }
                QuestionAnswered { question, text, asked, conclusion }
            }
            ThreadEnded { thread } => match self.threads.iter().find(|t| t.id == thread) {
                None => return no(format!("no thread {thread}")),
                Some(t) if t.ended => return no(format!("thread {thread} has already ended")),
                Some(_) => ThreadEnded { thread },
            },
            QuestionFailed { question, reason_code, reason } => {
                if !self.question(&question)?.open() {
                    return no(format!("question {question} is already closed"));
                }
                QuestionFailed { question, reason_code, reason }
            }
            ChoreDone { chore } => match self.chores.iter().find(|c| c.id == chore) {
                None => return no(format!("no chore {chore}")),
                Some(c) if c.done => return no(format!("chore {chore} is already done")),
                Some(_) => ChoreDone { chore },
            },
        })
    }

    fn prepare_task(&self, mut t: NewTask) -> Result<NewTask, Rejected> {
        // A task is in a unit, or (research, design and their reviews and gates) in an intent.
        let (owner, intent, count) = if let Ok(u) = self.unit(&t.unit) {
            if u.merged.is_some() {
                return no(format!("unit {} is merged; record new work in a new unit", u.id));
            }
            (u.id.clone(), u.intent.clone(), u.tasks.len())
        } else {
            let i = self.intent(&t.unit).map_err(|_| Rejected(format!("no unit or intent {}", t.unit)))?;
            if matches!(t.kind, TaskKind::Implement | TaskKind::Tests) {
                return no(format!("implement and tests tasks belong to a unit; {} takes research, design, review and gate tasks", i.id));
            }
            (i.id.clone(), i.id.clone(), i.tasks.len())
        };
        if let Some(i) = self.intents.iter().find(|i| i.id == intent)
            && matches!(i.state, IntentState::Done | IntentState::Dropped)
        {
            return no(format!("intent {intent} is {}; it takes no new tasks", name(&i.state)));
        }
        match t.kind {
            TaskKind::Implement | TaskKind::Gate | TaskKind::Design | TaskKind::Research if blank(&t.title) => {
                return no("an implement, design or research task, or a gate, needs a title");
            }
            TaskKind::Gate if t.gate.is_none() => return no("a gate needs its kind: deploy or test"),
            TaskKind::Implement | TaskKind::Gate if t.blind => return no("--blind is for looking at work without being told about it: a review, tests, research or design task"),
            TaskKind::Review if t.reviewer.is_none() => t.reviewer = Some(Reviewer::CodeReview),
            _ => {}
        }
        if t.kind != TaskKind::Review {
            t.reviewer = None;
        }
        if t.kind != TaskKind::Gate {
            t.gate = None;
        }
        for c in &mut t.causes {
            if c.unit.as_deref() == Some(owner.as_str()) {
                c.unit = None;
            }
            // Task 0: findings of the project's backlog (`f3`).
            if c.task == 0 {
                for id in &c.findings {
                    let f = self.finding(&FindingRef { unit: String::new(), task: 0, finding: id.clone() })?;
                    if !f.state.leads_to_work() {
                        return no(format!("backlog finding {id} is {}; batch it first (Do together in the Backlog)", name(&f.state)));
                    }
                }
                continue;
            }
            let at = c.unit.clone().unwrap_or_else(|| owner.clone());
            let by = self.task(&at, c.task)?;
            if matches!(by.kind, TaskKind::Implement | TaskKind::Design | TaskKind::Research) {
                return no(format!("{at}#{} is an implement, design or research task; causes are findings of reviews, tests or gates", c.task));
            }
            if c.findings.is_empty() {
                return no(format!("the cause {at}#{} names no finding", c.task));
            }
            for id in &c.findings {
                match by.findings.iter().find(|f| &f.found.id == id) {
                    None => return no(format!("{at}#{} has no finding {id}", c.task)),
                    Some(f) if !f.state.leads_to_work() => {
                        return no(format!(
                            "finding {id} of {at}#{} is neither accepted, revised nor scheduled; decide it first (workriver finding decide {at}#{} {id} accept)",
                            c.task, c.task
                        ));
                    }
                    Some(_) => {}
                }
            }
        }
        // Inputs default to the causes in the same unit (the first one primary), or to the start.
        if t.inputs.is_empty() {
            t.inputs = t.causes.iter().filter(|c| c.unit.is_none() && c.task != 0).enumerate().map(|(k, c)| Input { from: c.task.to_string(), primary: k == 0 }).collect();
        }
        if t.inputs.is_empty() {
            t.inputs.push(Input { from: "start".into(), primary: true });
        }
        for i in &mut t.inputs {
            let from = i.from.trim().trim_start_matches('#').to_string();
            if from != "start" {
                let n: u32 = from.parse().map_err(|_| Rejected(format!("input {:?} is neither \"start\" nor a task number", i.from)))?;
                self.task(&owner, n)?;
            }
            i.from = from;
        }
        if !t.inputs.iter().any(|i| i.primary) {
            t.inputs[0].primary = true;
        }
        t.n = count as u32 + 1;
        t.intent = intent;
        Ok(t)
    }

    /// Adds a recorded event. It passed `prepare` when it was recorded, so it is trusted.
    pub fn apply(&mut self, e: &Event) {
        use Body::*;
        let at = &e.at;
        let agent_of = |actor: &str| (actor != "user").then(|| actor.to_string());
        match &e.body {
            IntentCreated { id, title, goal, runtime_ref, project, state, note, external } => {
                self.intents.push(Intent {
                    id: id.clone(),
                    title: title.clone(),
                    manager: None,
                    goal: goal.clone(),
                    runtime_ref: runtime_ref.clone(),
                    at: at.clone(),
                    project: project.clone(),
                    state: *state,
                    note: note.clone(),
                    external: external.clone(),
                    workspace: runtime_ref.clone(),
                    reason: None,
                    tasks: vec![],
                });
                if *state == IntentState::Planned {
                    self.roadmap.push(id.clone());
                }
                // Before 0.2 an intent was its runtime group's: it is that group's current intent.
                if let Some(g) = runtime_ref
                    && *state == IntentState::Active
                {
                    self.workspace_mut(g).current = Some(id.clone());
                }
            }
            ProjectAdded { id, path, title } => self.projects.push(Project { id: id.clone(), path: path.clone(), title: title.clone() }),
            IntentStateChanged { intent, state, reason, workspace } => {
                let Some(k) = self.intents.iter().position(|i| &i.id == intent) else { return };
                let before = self.intents[k].workspace.clone();
                {
                    let i = &mut self.intents[k];
                    i.state = *state;
                    if reason.is_some() {
                        i.reason = reason.clone();
                    }
                    if let Some(g) = workspace {
                        i.workspace = Some(g.clone());
                    }
                }
                self.roadmap.retain(|x| x != intent);
                match state {
                    IntentState::Active => {
                        if let Some(g) = workspace {
                            self.workspace_mut(g).current = Some(intent.clone());
                        }
                    }
                    _ => {
                        // No longer worked on: no longer where new work goes.
                        if let Some(g) = before {
                            let w = self.workspace_mut(&g);
                            if w.current.as_deref() == Some(intent.as_str()) {
                                w.current = None;
                            }
                        }
                        if *state == IntentState::Planned {
                            self.roadmap.push(intent.clone());
                        }
                    }
                }
            }
            IntentMoved { intent, before, after } => {
                self.roadmap.retain(|x| x != intent);
                let at = before.as_ref().or(after.as_ref()).and_then(|o| self.roadmap.iter().position(|x| x == o));
                let k = match (at, before.is_some()) {
                    (Some(k), true) => k,
                    (Some(k), false) => k + 1,
                    (None, _) => self.roadmap.len(),
                };
                self.roadmap.insert(k, intent.clone());
            }
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
            WorkspaceLinked(l) => self.links.push(l.clone()),
            WorkspaceUnlinked { machine, workspace } => self.links.retain(|x| !(&x.machine == machine && &x.workspace == workspace)),
            FindingAdded { project, intent, finding, source, note } => {
                let found = finding.clone();
                self.backlog.push(BacklogFinding {
                    finding: Finding { found, state: FindingState::Deferred, decided_by: Some(Who::User), note: (!note.is_empty()).then(|| note.clone()), delivery: None, reason: None },
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
            IntentUpdated { intent, title, goal } => {
                if let Some(i) = self.intents.iter_mut().find(|i| &i.id == intent) {
                    if let Some(t) = title {
                        i.title = t.clone();
                    }
                    if let Some(g) = goal {
                        i.goal = g.clone();
                    }
                }
            }
            IntentLinked { intent, runtime_ref } => {
                let mut active = false;
                if let Some(i) = self.intents.iter_mut().find(|i| &i.id == intent) {
                    i.runtime_ref = Some(runtime_ref.clone());
                    i.workspace = Some(runtime_ref.clone());
                    active = i.state == IntentState::Active;
                }
                if active {
                    self.workspace_mut(runtime_ref).current = Some(intent.clone());
                }
            }
            AgentSeen { session, name, harness } => {
                let seen = Agent { session: session.clone(), name: name.clone(), harness: harness.clone() };
                match self.agents.iter_mut().find(|a| &a.session == session) {
                    Some(a) => *a = seen,
                    None => self.agents.push(seen),
                }
            }
            ManagerRegistered { intent, agent, workspace } => {
                // Before 0.2: the intent's manager, which is the manager of its runtime group.
                let mut group = workspace.clone();
                if let Some(i) = intent.as_ref().and_then(|id| self.intents.iter_mut().find(|i| &i.id == id)) {
                    i.manager = Some(agent.clone());
                    group = group.or(i.workspace.clone());
                }
                if let Some(g) = group {
                    self.workspace_mut(&g).manager = Some(agent.clone());
                }
            }
            UnitCreated { id, intent, title, purpose, repo, branch, base, machine } => self.units.push(Unit {
                machine: machine.clone(),
                id: id.clone(),
                intent: intent.clone(),
                title: title.clone(),
                purpose: purpose.clone(),
                repo: repo.clone(),
                branch: branch.clone(),
                base: base.clone(),
                at: at.clone(),
                merged: None,
                merged_commits: vec![],
                tasks: vec![],
            }),
            UnitMerged { unit, commits } => {
                if let Some(u) = self.units.iter_mut().find(|u| &u.id == unit) {
                    u.merged = Some(at.clone());
                    u.merged_commits = commits.clone();
                }
            }
            TaskCreated(t) => {
                // Before 0.2 a task named no intent: its unit's.
                let intent = if t.intent.is_empty() { self.intent_of(&t.unit).unwrap_or_default().to_string() } else { t.intent.clone() };
                let tasks = match self.units.iter_mut().find(|u| u.id == t.unit) {
                    Some(u) => Some(&mut u.tasks),
                    None => self.intents.iter_mut().find(|i| i.id == t.unit).map(|i| &mut i.tasks),
                };
                if let Some(tasks) = tasks {
                    tasks.push(Task {
                        n: t.n,
                        intent,
                        kind: t.kind,
                        title: t.title.clone(),
                        text: t.text.clone(),
                        inputs: t.inputs.clone(),
                        causes: t.causes.clone(),
                        agent: t.agent.clone(),
                        reviewer: t.reviewer,
                        gate: t.gate,
                        steps: t.steps.clone(),
                        plans: t.plans.clone(),
                        blind: t.blind,
                        state: if t.kind == TaskKind::Gate { TaskState::Waiting } else { TaskState::Todo },
                        by: e.actor.clone(),
                        at: at.clone(),
                        started: None,
                        done: None,
                        note: String::new(),
                        evidence: vec![],
                        findings: vec![],
                        result: None,
                        delivery: None,
                        amended: None,
                        attachments: vec![],
                    })
                }
            }
            TaskStarted { unit, task, agent } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.state = TaskState::Running;
                    t.started = Some(at.clone());
                    t.agent = agent.clone().or_else(|| agent_of(&e.actor)).or(t.agent.take());
                }
            }
            TaskDelivered { unit, task, note, evidence, findings, attachments } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.attachments.extend(attachments.iter().cloned());
                    t.state = TaskState::Done;
                    t.done = Some(at.clone());
                    t.note = note.clone();
                    t.evidence = evidence.clone();
                    t.findings = findings.iter().cloned().map(open).collect();
                    if t.agent.is_none() {
                        t.agent = agent_of(&e.actor);
                    }
                }
                // Scheduled findings this work answers are done with.
                let causes: Vec<FindingRef> = self
                    .task(unit, *task)
                    .map(|t| {
                        t.causes
                            .iter()
                            .flat_map(|c| {
                                let at = if c.task == 0 { String::new() } else { c.unit.clone().unwrap_or_else(|| unit.clone()) };
                                c.findings.iter().map(move |f| FindingRef { unit: at.clone(), task: c.task, finding: f.clone() })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                for r in causes {
                    if let Some(f) = self.finding_mut(&r).filter(|f| f.state == FindingState::Scheduled) {
                        f.state = FindingState::Done;
                    }
                }
            }
            FindingDecided { unit, task, finding, decision, note, by, .. } => {
                if let Some(f) = self.task_mut(unit, *task).and_then(|t| t.findings.iter_mut().find(|f| &f.found.id == finding)) {
                    f.state = match decision {
                        Decision::Accept => FindingState::Accepted,
                        Decision::Revise => FindingState::Revised,
                        Decision::Reject => FindingState::Rejected,
                        Decision::Defer => FindingState::Deferred,
                    };
                    f.decided_by = Some(*by);
                    f.note = (!note.is_empty()).then(|| note.clone());
                }
            }
            GateSubmitted { unit, task, result, note, findings, attachments } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.state = TaskState::Done;
                    t.done = Some(at.clone());
                    t.result = Some(*result);
                    t.note = note.clone();
                    t.findings = findings.iter().cloned().map(open).collect();
                    t.attachments.extend(attachments.iter().cloned());
                }
            }
            AttachmentAdded { id, name, mime, size, unit, task, finding } => {
                if !self.attachments.iter().any(|a| &a.id == id) {
                    self.attachments.push(Attachment { id: id.clone(), name: name.clone(), mime: mime.clone(), size: *size, by: e.actor.clone(), at: at.clone() });
                }
                if let (Some(u), Some(n)) = (unit, task)
                    && let Some(t) = self.task_mut(u, *n)
                {
                    let list = match finding {
                        Some(f) => t.findings.iter_mut().find(|x| &x.found.id == f).map(|x| &mut x.found.attachments),
                        None => Some(&mut t.attachments),
                    };
                    if let Some(list) = list.filter(|l| !l.contains(id)) {
                        list.push(id.clone());
                    }
                }
            }
            DeliveryUpdated { chore: Some(c), delivery, .. } => {
                if let Some(x) = self.chores.iter_mut().find(|x| &x.id == c) {
                    x.delivery = Some(delivery.clone());
                }
            }
            TaskAmended { unit, task, note, evidence, findings, attachments } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.attachments.extend(attachments.iter().cloned());
                    if let Some(n) = note {
                        t.note = n.clone();
                    }
                    if let Some(e) = evidence {
                        t.evidence = e.clone();
                    }
                    if let Some(f) = findings {
                        t.findings = f.iter().cloned().map(open).collect();
                    }
                    t.amended = Some(at.clone());
                }
            }
            DeliveryUpdated { task: 0, finding: Some(f), delivery, .. } => {
                if let Some(x) = self.finding_mut(&FindingRef { unit: String::new(), task: 0, finding: f.clone() }) {
                    x.delivery = Some(delivery.clone());
                }
            }
            DeliveryUpdated { unit, task, finding, delivery, .. } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    match finding {
                        None => t.delivery = Some(delivery.clone()),
                        Some(id) => {
                            if let Some(f) = t.findings.iter_mut().find(|f| &f.found.id == id) {
                                f.delivery = Some(delivery.clone());
                            }
                        }
                    }
                }
            }
            ChoreCreated { id, intent, unit, title, steps } => self.chores.push(Chore {
                id: id.clone(),
                intent: intent.clone(),
                unit: unit.clone(),
                title: title.clone(),
                by: e.actor.clone(),
                steps: steps.clone(),
                done: false,
                at: at.clone(),
                delivery: None,
            }),
            QuestionAsked { id, thread, unit, task, text, context, conclude } => {
                if !self.threads.iter().any(|t| &t.id == thread) {
                    self.threads.push(Thread { id: thread.clone(), unit: unit.clone(), task: *task, agent: None, name: None, ended: false, conclusion: None });
                }
                self.questions.push(Question {
                id: id.clone(),
                thread: thread.clone(),
                conclude: *conclude,
                unit: unit.clone(),
                task: *task,
                text: text.clone(),
                context: context.clone(),
                at: at.clone(),
                agent: None,
                name: None,
                answer: None,
                asked: None,
                answered_by: None,
                failed: None,
                by: e.actor.clone(),
                temporary: true,
                })
            }
            QuestionStarted { question, agent, name, temporary } => {
                if let Some(q) = self.questions.iter_mut().find(|q| &q.id == question) {
                    q.agent = Some(agent.clone());
                    q.name = Some(name.clone());
                    q.temporary = *temporary;
                    // Only a temporary agent belongs to the thread (and is closed with it).
                    if *temporary && let Some(t) = self.threads.iter_mut().find(|t| t.id == q.thread) {
                        t.agent = Some(agent.clone());
                        t.name = Some(name.clone());
                    }
                }
            }
            QuestionAnswered { question, text, asked, conclusion } => {
                if let Some(q) = self.questions.iter_mut().find(|q| &q.id == question) {
                    q.answer = Some(text.clone());
                    q.asked = asked.clone();
                    q.answered_by = Some(e.actor.clone());
                    if *conclusion && let Some(t) = self.threads.iter_mut().find(|t| t.id == q.thread) {
                        t.conclusion = Some(text.clone());
                    }
                }
            }
            ThreadEnded { thread } => {
                if let Some(t) = self.threads.iter_mut().find(|t| &t.id == thread) {
                    t.ended = true;
                }
            }
            QuestionFailed { question, reason_code, reason } => {
                if let Some(q) = self.questions.iter_mut().find(|q| &q.id == question) {
                    q.failed = Some((*reason_code, reason.clone()));
                }
            }
            ChoreDone { chore } => {
                if let Some(c) = self.chores.iter_mut().find(|c| &c.id == chore) {
                    c.done = true;
                }
            }
        }
    }

    fn task_mut(&mut self, unit: &str, n: u32) -> Option<&mut Task> {
        let tasks = match self.units.iter_mut().find(|u| u.id == unit) {
            Some(u) => &mut u.tasks,
            None => &mut self.intents.iter_mut().find(|i| i.id == unit)?.tasks,
        };
        tasks.iter_mut().find(|t| t.n == n)
    }

    /// A runtime group's record, made when first needed; it follows the group's latest id and label.
    fn workspace_mut(&mut self, g: &RuntimeRef) -> &mut Workspace {
        // A linked group's record is the local workspace's, which keeps its own id and label.
        let local = self.link_of(g).map(|l| RuntimeRef { id: l.local.clone(), label: l.local.clone(), machine: None });
        match self.workspace_at(g) {
            Some(k) => {
                if local.is_none() {
                    self.workspaces[k].group = g.clone();
                }
                &mut self.workspaces[k]
            }
            None => {
                self.workspaces.push(Workspace { group: local.unwrap_or_else(|| g.clone()), manager: None, current: None });
                self.workspaces.last_mut().unwrap()
            }
        }
    }

    fn finding_mut(&mut self, r: &FindingRef) -> Option<&mut Finding> {
        if r.task == 0 {
            return self.backlog.iter_mut().find(|b| b.finding.found.id == r.finding).map(|b| &mut b.finding);
        }
        self.task_mut(&r.unit, r.task)?.findings.iter_mut().find(|f| f.found.id == r.finding)
    }
}

/// What a delivery or its correction may carry.
fn check_output(t: &Task, evidence: &[Evidence], findings: &[NewFinding]) -> Result<(), Rejected> {
    if !findings.is_empty() && matches!(t.kind, TaskKind::Implement | TaskKind::Design | TaskKind::Research) {
        return no("findings belong to a review or tests task; record a review task and deliver them there");
    }
    if evidence.iter().any(|e| blank(&e.command)) {
        return no("evidence needs the command that was run");
    }
    for f in findings {
        if blank(&f.problem) || blank(&f.impact) || f.r#where.iter().all(|w| blank(w)) {
            return no("a finding needs a problem, an impact and at least one where (a file path)");
        }
    }
    Ok(())
}

fn open(found: NewFinding) -> Finding {
    Finding { found, state: FindingState::Open, decided_by: None, note: None, delivery: None, reason: None }
}

/// Ids per severity, as a reviewer numbers them: P1-1, P2-1, P2-2…
fn numbered(mut findings: Vec<NewFinding>) -> Vec<NewFinding> {
    let mut seen = [0; 3];
    for f in &mut findings {
        let k = f.severity as usize;
        seen[k] += 1;
        f.id = format!("{:?}-{}", f.severity, seen[k]);
    }
    findings
}
