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
}

impl FindingState {
    /// Accepted or revised: work is handed out for it, and it can be a cause.
    pub fn leads_to_work(self) -> bool {
        matches!(self, FindingState::Accepted | FindingState::Revised)
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
    pub task: u32,
    pub findings: Vec<String>,
}

/// A check run on a piece of work: the command and what came of it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Evidence {
    pub command: String,
    pub result: CheckResult,
    pub summary: String,
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
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct NewTask {
    pub unit: String,
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
}

/// The runtime's group an intent follows (under Herdr, a workspace): its id and label.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct RuntimeRef {
    pub id: String,
    pub label: String,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        runtime_ref: Option<RuntimeRef>,
    },
    /// The intent now follows another runtime group (its id changed, say).
    #[serde(rename = "intent.linked")]
    IntentLinked { intent: String, runtime_ref: RuntimeRef },
    #[serde(rename = "agent.seen")]
    AgentSeen { session: String, name: String, harness: String },
    #[serde(rename = "manager.registered")]
    ManagerRegistered { intent: String, agent: String },
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
    },
    #[serde(rename = "finding.decided")]
    FindingDecided {
        unit: String,
        task: u32,
        finding: String,
        decision: Decision,
        #[serde(default)]
        note: String,
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
            ManagerRegistered { intent, .. } | IntentUpdated { intent, .. } => (Some(intent.clone()), None, None),
            DeliveryUpdated { chore: Some(_), .. } => (None, None, None),
            UnitCreated { id, intent, .. } => (Some(intent.clone()), Some(id.clone()), None),
            UnitMerged { unit, .. } => (None, Some(unit.clone()), None),
            TaskCreated(t) => (None, Some(t.unit.clone()), task(&t.unit, t.n)),
            TaskStarted { unit, task: n, .. }
            | TaskDelivered { unit, task: n, .. }
            | TaskAmended { unit, task: n, .. }
            | FindingDecided { unit, task: n, .. }
            | GateSubmitted { unit, task: n, .. }
            | DeliveryUpdated { unit, task: n, .. } => (None, Some(unit.clone()), task(unit, *n)),
            ChoreCreated { intent, unit, .. } => (Some(intent.clone()), unit.clone(), None),
            ChoreDone { .. } => (None, None, None),
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
    /// The manager's session.
    pub manager: Option<String>,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub runtime_ref: Option<RuntimeRef>,
    pub at: String,
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
    pub at: String,
    /// When it was recorded as merged.
    pub merged: Option<String>,
    pub merged_commits: Vec<String>,
    pub tasks: Vec<Task>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Task {
    pub n: u32,
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

    pub fn task(&self, unit: &str, n: u32) -> Result<&Task, Rejected> {
        match self.unit(unit)?.tasks.iter().find(|t| t.n == n) {
            Some(t) => Ok(t),
            None => no(format!("no task {unit}#{n}")),
        }
    }

    /// The intent following a runtime group: by its id, or else by its label, since a
    /// runtime may give its groups new ids when it restarts.
    pub fn intent_for(&self, r: &RuntimeRef) -> Option<&Intent> {
        let by = |f: &dyn Fn(&RuntimeRef) -> bool| self.intents.iter().find(|i| i.runtime_ref.as_ref().is_some_and(f));
        by(&|x| x.id == r.id).or_else(|| by(&|x| x.label == r.label))
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

    fn intent(&self, id: &str) -> Result<&Intent, Rejected> {
        match self.intents.iter().find(|i| i.id == id) {
            Some(i) => Ok(i),
            None => no(format!("no intent {id}")),
        }
    }

    /// Checks a new event against the rules and fills in what is assigned here (ids and
    /// numbers). The result is what gets stored.
    pub fn prepare(&self, actor: &str, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        if blank(actor) {
            return no("the actor is empty");
        }
        Ok(match body {
            IntentCreated { title, goal, runtime_ref, .. } => {
                if blank(&title) {
                    return no("an intent needs a title");
                }
                if runtime_ref.as_ref().is_some_and(|r| blank(&r.id)) {
                    return no("a runtime group needs an id");
                }
                IntentCreated { id: format!("i{}", self.intents.len() + 1), title, goal, runtime_ref }
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
                let i = self.intent(&intent)?;
                if actor != "user" && i.manager.as_deref() != Some(actor) {
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
            ManagerRegistered { intent, agent } => {
                self.intent(&intent)?;
                if blank(&agent) {
                    return no("a manager needs an agent");
                }
                ManagerRegistered { intent, agent }
            }
            UnitCreated { intent, title, purpose, repo, branch, base, .. } => {
                self.intent(&intent)?;
                for (field, v) in [("title", &title), ("repo", &repo), ("branch", &branch), ("base", &base)] {
                    if blank(v) {
                        return no(format!("a unit needs a {field}"));
                    }
                }
                let id = format!("u{}", self.units.len() + 1);
                UnitCreated { id, intent, title, purpose, repo, branch, base }
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
            TaskDelivered { unit, task, note, evidence, findings } => {
                let t = self.task(&unit, task)?;
                if t.kind == TaskKind::Gate {
                    return no(format!("{unit}#{task} is a gate: the user submits it with gate submit"));
                }
                if t.state == TaskState::Done {
                    return no(format!("{unit}#{task} is already delivered; to correct it, run `workriver task deliver {unit}#{task} --amend` with what replaces it"));
                }
                check_output(t, &evidence, &findings)?;
                if t.kind == TaskKind::Design && blank(&note) {
                    return no("a design task delivers its design as the note: --note \"...\"");
                }
                TaskDelivered { unit, task, note, evidence, findings: numbered(findings) }
            }
            TaskAmended { unit, task, note, evidence, findings } => {
                let t = self.task(&unit, task)?;
                if t.kind == TaskKind::Gate || t.state != TaskState::Done {
                    return no(format!("{unit}#{task} is not a delivered task; only a delivered task can be amended"));
                }
                if note.is_none() && evidence.is_none() && findings.is_none() {
                    return no("nothing to amend: give a note, evidence or findings");
                }
                if findings.is_some() && t.findings.iter().any(|f| f.state != FindingState::Open) {
                    return no(format!(
                        "some findings of {unit}#{task} are already decided, so they cannot be replaced; record the new findings in a new review or tests task"
                    ));
                }
                check_output(t, evidence.as_deref().unwrap_or_default(), findings.as_deref().unwrap_or_default())?;
                TaskAmended { unit, task, note, evidence, findings: findings.map(numbered) }
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
                FindingDecided { unit, task, finding, decision, note, by }
            }
            GateSubmitted { unit, task, result, note, findings } => {
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
                GateSubmitted { unit, task, result, note, findings: numbered(findings) }
            }
            DeliveryUpdated { unit, task, finding, chore, delivery } => {
                if let Some(c) = &chore {
                    if !self.chores.iter().any(|x| &x.id == c) {
                        return no(format!("no chore {c}"));
                    }
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
                self.task(&unit, task)?;
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
                if actor != "user" && !mine {
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
        let unit = self.unit(&t.unit)?;
        if unit.merged.is_some() {
            return no(format!("unit {} is merged; record new work in a new unit", unit.id));
        }
        match t.kind {
            TaskKind::Implement | TaskKind::Gate | TaskKind::Design if blank(&t.title) => return no("an implement or design task, or a gate, needs a title"),
            TaskKind::Gate if t.gate.is_none() => return no("a gate needs its kind: deploy or test"),
            TaskKind::Review if t.reviewer.is_none() => t.reviewer = Some(Reviewer::CodeReview),
            _ => {}
        }
        if t.kind != TaskKind::Review {
            t.reviewer = None;
        }
        if t.kind != TaskKind::Gate {
            t.gate = None;
        }
        for c in &t.causes {
            let by = self.task(&unit.id, c.task)?;
            if matches!(by.kind, TaskKind::Implement | TaskKind::Design) {
                return no(format!("{}#{} is an implement or design task; causes are findings of reviews, tests or gates", unit.id, c.task));
            }
            if c.findings.is_empty() {
                return no(format!("the cause {}#{} names no finding", unit.id, c.task));
            }
            for id in &c.findings {
                match by.findings.iter().find(|f| &f.found.id == id) {
                    None => return no(format!("{}#{} has no finding {id}", unit.id, c.task)),
                    Some(f) if !f.state.leads_to_work() => {
                        return no(format!(
                            "finding {id} of {}#{} is neither accepted nor revised; decide it first (workriver finding decide {}#{} {id} accept)",
                            unit.id, c.task, unit.id, c.task
                        ));
                    }
                    Some(_) => {}
                }
            }
        }
        // Inputs default to the causes (the first one primary), or to the start.
        if t.inputs.is_empty() {
            t.inputs = t.causes.iter().enumerate().map(|(k, c)| Input { from: c.task.to_string(), primary: k == 0 }).collect();
        }
        if t.inputs.is_empty() {
            t.inputs.push(Input { from: "start".into(), primary: true });
        }
        for i in &mut t.inputs {
            let from = i.from.trim().trim_start_matches('#').to_string();
            if from != "start" {
                let n: u32 = from.parse().map_err(|_| Rejected(format!("input {:?} is neither \"start\" nor a task number", i.from)))?;
                self.task(&unit.id, n)?;
            }
            i.from = from;
        }
        if !t.inputs.iter().any(|i| i.primary) {
            t.inputs[0].primary = true;
        }
        t.n = unit.tasks.len() as u32 + 1;
        Ok(t)
    }

    /// Adds a recorded event. It passed `prepare` when it was recorded, so it is trusted.
    pub fn apply(&mut self, e: &Event) {
        use Body::*;
        let at = &e.at;
        let agent_of = |actor: &str| (actor != "user").then(|| actor.to_string());
        match &e.body {
            IntentCreated { id, title, goal, runtime_ref } => self.intents.push(Intent {
                id: id.clone(),
                title: title.clone(),
                manager: None,
                goal: goal.clone(),
                runtime_ref: runtime_ref.clone(),
                at: at.clone(),
            }),
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
                if let Some(i) = self.intents.iter_mut().find(|i| &i.id == intent) {
                    i.runtime_ref = Some(runtime_ref.clone());
                }
            }
            AgentSeen { session, name, harness } => {
                let seen = Agent { session: session.clone(), name: name.clone(), harness: harness.clone() };
                match self.agents.iter_mut().find(|a| &a.session == session) {
                    Some(a) => *a = seen,
                    None => self.agents.push(seen),
                }
            }
            ManagerRegistered { intent, agent } => {
                if let Some(i) = self.intents.iter_mut().find(|i| &i.id == intent) {
                    i.manager = Some(agent.clone());
                }
            }
            UnitCreated { id, intent, title, purpose, repo, branch, base } => self.units.push(Unit {
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
                if let Some(u) = self.units.iter_mut().find(|u| u.id == t.unit) {
                    u.tasks.push(Task {
                        n: t.n,
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
            TaskDelivered { unit, task, note, evidence, findings } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.state = TaskState::Done;
                    t.done = Some(at.clone());
                    t.note = note.clone();
                    t.evidence = evidence.clone();
                    t.findings = findings.iter().cloned().map(open).collect();
                    if t.agent.is_none() {
                        t.agent = agent_of(&e.actor);
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
            GateSubmitted { unit, task, result, note, findings, .. } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.state = TaskState::Done;
                    t.done = Some(at.clone());
                    t.result = Some(*result);
                    t.note = note.clone();
                    t.findings = findings.iter().cloned().map(open).collect();
                }
            }
            DeliveryUpdated { chore: Some(c), delivery, .. } => {
                if let Some(x) = self.chores.iter_mut().find(|x| &x.id == c) {
                    x.delivery = Some(delivery.clone());
                }
            }
            TaskAmended { unit, task, note, evidence, findings } => {
                if let Some(t) = self.task_mut(unit, *task) {
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
        self.units.iter_mut().find(|u| u.id == unit)?.tasks.iter_mut().find(|t| t.n == n)
    }
}

/// What a delivery or its correction may carry.
fn check_output(t: &Task, evidence: &[Evidence], findings: &[NewFinding]) -> Result<(), Rejected> {
    if !findings.is_empty() && matches!(t.kind, TaskKind::Implement | TaskKind::Design) {
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
    Finding { found, state: FindingState::Open, decided_by: None, note: None, delivery: None }
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
