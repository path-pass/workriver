// The events workriver records, the state they add up to, and the rules a new event must
// pass. Ids and numbers are assigned here, so replaying the stored events gives the same
// state every time.
use serde::{Deserialize, Serialize};

use crate::access::{Principal, Rejected, no};
use crate::planning::*;

mod body;
pub use body::*;

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
    /// How the code is organized and how the solution is built, against the project's own
    /// rules; workriver holds no style of its own.
    ArchitectureReview,
}

/// What a review of a command the user is about to run found it to be.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// It does what it says, nothing risky.
    Ok,
    /// Worth a look: changes outside its folder, network, installs, deletes.
    Caution,
    /// Destructive or irreversible.
    Danger,
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
    /// No longer needed (`task.dropped`); kept, never deleted. See `Task::dropped`.
    Dropped,
}

impl TaskState {
    /// Done or dropped: nothing more comes of it, and nothing waits on it.
    pub fn over(self) -> bool {
        matches!(self, TaskState::Done | TaskState::Dropped)
    }
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
    /// A gate put before a running task that is stuck on what only the user can do (a
    /// blocker): the task follows it from then on, so the gate's result goes to its agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocks: Option<u32>,
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
    /// The agent waited for a person in its terminal (a permission or first-run prompt) the
    /// whole time: the message never reached it.
    Blocked,
    /// A new agent never became ready to take its first message.
    NotReady,
}

/// What a question is about besides its task: a finding, a file, lines of a file, or one of
/// the task's attachments.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct AskContext {
    /// "task", "finding", "file", "lines" or "attachment".
    pub kind: String,
    pub label: String,
    /// attachment: its id in the store (one shown with the task, see `State::attachments_of`,
    /// or one the user stored for the question).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
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

/// How many of the latest command runs the state keeps.
pub const RUNS_KEPT: usize = 20;

/// A command the user ran from the page, as recorded, with its exit once it was seen.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct CommandRun {
    pub id: String,
    pub command: String,
    pub dir: String,
    pub machine: Option<String>,
    pub unit: Option<String>,
    /// Who wrote it: an agent's session, or "user".
    pub by: String,
    /// Whose step it was (`u1#3`, `c2`) and which step (from 1); none: typed by the user.
    pub from: Option<String>,
    pub step: Option<u32>,
    pub at: String,
    pub exit: Option<i32>,
    /// What its review found, and said.
    pub review: Option<(Verdict, String)>,
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
    /// The user stored it (first, or the same content again): a question of theirs may point at it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub user: bool,
}

/// The user's message to a workspace's manager, and what it answered.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Message {
    pub id: String,
    pub intent: String,
    pub workspace: String,
    /// The manager's session it went to: only that one replies.
    pub manager: String,
    pub text: String,
    pub at: String,
    pub delivery: Option<Delivery>,
    #[serde(default)]
    pub replies: Vec<Reply>,
    /// Started by the manager (`workriver say`): its text is the manager's, and it went nowhere.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_manager: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Reply {
    pub by: String,
    pub text: String,
    pub at: String,
}

/// A conversation about a task (or, task 0, its unit): its questions share one temporary
/// agent while it lives.
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

/// A question about a task (task 0: its unit) and how it fared.
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
    #[serde(default)]
    pub answered_at: Option<String>,
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

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Chore {
    pub id: String,
    /// Empty: a project's chore (`project`), or nobody's.
    pub intent: String,
    #[serde(default)]
    pub project: String,
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

    /// The tasks task `n` comes after through its inputs, however many between, oldest first.
    pub fn before(&self, owner: &str, n: u32) -> Vec<u32> {
        let mut seen = std::collections::BTreeSet::new();
        let inputs = |t: &Task| t.inputs.iter().filter_map(|i| i.from.parse::<u32>().ok()).collect::<Vec<_>>();
        let mut next = self.task(owner, n).map(inputs).unwrap_or_default();
        while let Some(k) = next.pop() {
            if let (true, Ok(b)) = (seen.insert(k), self.task(owner, k)) {
                next.extend(inputs(b));
            }
        }
        seen.into_iter().collect()
    }

    /// The designs a gate or a review is about: the design tasks it follows directly or
    /// through reviews only (design, its reviews, the gate). Not through implementing or
    /// another gate: a question to merge, asked after the work, is about no design.
    pub fn designs_asked(&self, owner: &str, n: u32) -> Vec<u32> {
        let mut found = std::collections::BTreeSet::new();
        self.before_through(owner, n, &mut found);
        found.into_iter().collect()
    }

    fn before_through(&self, owner: &str, n: u32, found: &mut std::collections::BTreeSet<u32>) {
        let Ok(t) = self.task(owner, n) else { return };
        for k in t.inputs.iter().filter_map(|i| i.from.parse::<u32>().ok()) {
            match self.task(owner, k).map(|b| b.kind) {
                Ok(TaskKind::Design) => {
                    found.insert(k);
                }
                Ok(TaskKind::Review) if k < n => self.before_through(owner, k, found),
                _ => {}
            }
        }
    }

    /// The decide gate that asks the user to confirm design `d` (see `designs_asked`); the
    /// latest when it was asked again. None: nobody was asked.
    pub fn design_gate(&self, owner: &str, d: u32) -> Option<&Task> {
        let gates = self.tasks_of(owner)?.iter().filter(|g| g.gate == Some(GateKind::Decide) && g.dropped.is_none());
        gates.filter(|g| self.designs_asked(owner, g.n).contains(&d)).max_by_key(|g| g.n)
    }

    /// The design tasks that revise design `d`, with their owner: those not dropped that
    /// answer a finding of the design itself or of a gate or review about it (see
    /// `designs_asked`). A design answering a finding about the code built from it does not.
    pub fn revised_by(&self, owner: &str, d: u32) -> Vec<(String, &Task)> {
        let hit = |at: &str, t: &Task| {
            t.kind == TaskKind::Design && t.dropped.is_none() && t.causes.iter().any(|c| c.unit.as_deref().unwrap_or(at) == owner && c.task != 0 && (c.task == d || self.designs_asked(owner, c.task).contains(&d)))
        };
        let units = self.units.iter().flat_map(|u| u.tasks.iter().filter(|t| hit(&u.id, t)).map(|t| (u.id.clone(), t)));
        units.chain(self.intents.iter().flat_map(|i| i.tasks.iter().filter(|t| hit(&i.id, t)).map(|t| (i.id.clone(), t)))).collect()
    }

    /// The design that stands in place of design `d`: its newest delivered revision, and
    /// that one's in turn. None while no revision of it is delivered: it stands itself.
    pub fn design_in_place(&self, owner: &str, d: u32) -> Option<(String, u32)> {
        let (mut at, mut seen, mut found) = ((owner.to_string(), d), vec![(owner.to_string(), d)], None);
        while let Some((o, t)) = self.revised_by(&at.0, at.1).into_iter().filter(|(_, t)| t.done.is_some()).max_by_key(|(_, t)| t.n) {
            at = (o, t.n);
            if seen.contains(&at) {
                break;
            }
            seen.push(at.clone());
            found = Some(at.clone());
        }
        found
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

    /// The stored attachments shown with a task: its output's, its checks', its findings' and
    /// those added to them later, then those of the findings it answers, unless it is blind
    /// and not delivered (its agent is told nothing of other work). Empty for no such task.
    pub fn attachments_of(&self, unit: &str, n: u32) -> Vec<&str> {
        let Ok(t) = self.task(unit, n) else { return vec![] };
        fn of<'a>(fs: &'a [Finding], ids: Option<&[String]>) -> Vec<&'a String> {
            let fs = fs.iter().filter(|f| ids.is_none_or(|ids| ids.contains(&f.found.id)));
            fs.flat_map(|f| f.found.attachments.iter().chain(f.notes.iter().flat_map(|n| &n.attachments))).collect()
        }
        let mut out: Vec<&String> = t.attachments.iter().chain(t.evidence.iter().filter_map(|e| e.attachment.as_ref())).chain(of(&t.findings, None)).collect();
        if !(t.blind && !t.state.over()) {
            for c in &t.causes {
                if let Ok(by) = self.task(c.unit.as_deref().unwrap_or(unit), c.task) {
                    out.extend(of(&by.findings, Some(&c.findings)));
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        out.into_iter().map(String::as_str).filter(|id| seen.insert(*id)).collect()
    }

    /// Whether `actor` is the agent a task is assigned to or was done by (by session, or by
    /// the name it was assigned under).
    fn is_agent_of(&self, t: &Task, actor: &str) -> bool {
        t.agent.as_deref().is_some_and(|a| a == actor || Some(a) == self.name_of(actor))
    }

    /// The tasks handed out for a finding: those naming it as a cause, with their owner.
    pub fn follow_ups(&self, unit: &str, task: u32, finding: &str) -> Vec<(String, &Task)> {
        let hit = |owner: &str, t: &&Task| t.causes.iter().any(|c| c.task == task && c.unit.as_deref().unwrap_or(owner) == unit && c.findings.iter().any(|f| f == finding));
        let units = self.units.iter().flat_map(|u| u.tasks.iter().filter(move |t| hit(&u.id, t)).map(move |t| (u.id.clone(), t)));
        units.chain(self.intents.iter().flat_map(|i| i.tasks.iter().filter(move |t| hit(&i.id, t)).map(move |t| (i.id.clone(), t)))).collect()
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
            ProjectRenamed { project, title } => {
                let Some(p) = self.projects.iter().find(|p| p.id == project || p.path == project) else {
                    return no(format!("no project {project}; a repository outside every project is registered with `workriver project add <folder> --title ...`"));
                };
                if blank(&title) {
                    return no("a project needs a title");
                }
                ProjectRenamed { project: p.id.clone(), title }
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
            IntentMerged { intent, from, title } => {
                let i = self.intent(&intent)?;
                if i.state != IntentState::Planned {
                    return no(format!("intent {intent} is {}; only planned intents are merged", name(&i.state)));
                }
                if title.as_deref().is_some_and(blank) {
                    return no("the new title is empty: give one, or leave --title out to keep the title");
                }
                if from.is_empty() {
                    return no(format!("name the planned intents to merge into {intent}"));
                }
                // The user's own goals are the user's to merge; what agents added, agents may tidy.
                if !user && let Some(mine) = std::iter::once(&intent).chain(&from).find(|x| self.intents.iter().any(|i| &i.id == *x && i.by == "user")) {
                    return no(format!("{mine} was added by the user: only the user merges it (in the roadmap, or `workriver backlog merge` from the user's terminal)"));
                }
                for (k, f) in from.iter().enumerate() {
                    if f == &intent || from[..k].contains(f) {
                        return no(format!("{f} is named twice: give each intent to merge once, and not {intent} itself"));
                    }
                    let x = self.intent(f)?;
                    if x.state != IntentState::Planned {
                        return no(format!("intent {f} is {}; only planned intents are merged", name(&x.state)));
                    }
                    if x.project != i.project {
                        return no(format!("{f} is in another project than {intent}"));
                    }
                    // Work, findings or chores of its own would be lost with it.
                    let held = [
                        ("tasks", !x.tasks.is_empty()),
                        ("units", self.units.iter().any(|u| &u.intent == f)),
                        ("backlog findings", self.backlog.iter().any(|b| b.intent.as_deref() == Some(f))),
                        ("chores", self.chores.iter().any(|c| &c.intent == f)),
                        ("conversations", self.threads.iter().any(|t| &t.unit == f)),
                    ];
                    if let Some((what, _)) = held.iter().find(|(_, has)| *has) {
                        return no(format!("{f} has {what} of its own; only an intent with nothing attached is merged (drop it with a reason instead)"));
                    }
                }
                IntentMerged { intent, from, title: title.map(|t| t.trim().to_string()) }
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
            WorkspaceProjectSet { workspace, project } => {
                if blank(&workspace.id) || blank(&workspace.label) {
                    return no("name the workspace by its id and label, as `workriver workspace list` shows it");
                }
                let project = match project {
                    None => None,
                    // A registered project (by id or folder), or a folder an intent or unit names.
                    Some(p) => match self.projects.iter().find(|x| x.id == p || x.path == p) {
                        Some(x) => Some(x.path.clone()),
                        None if !blank(&p) && (self.intents.iter().any(|i| i.project == p) || self.units.iter().any(|u| u.repo == p)) => Some(p),
                        None => return no(format!("no project {p}; name a project as `workriver project list` shows it")),
                    },
                };
                WorkspaceProjectSet { workspace, project }
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
            IntentUpdated { intent, title, goal, findings, add_note } => {
                self.intent(&intent)?;
                if !user && self.manager_of(&intent) != Some(actor) {
                    return no(format!("only the user or the manager of intent {intent} may change it"));
                }
                // Who decides findings is the user's to say, never the one it is handed to.
                if findings.is_some() && !user {
                    return no("only the user says who decides an intent's findings (workriver intent set <intent> --findings …, or in the web UI)");
                }
                if title.as_deref().is_some_and(blank) {
                    return no("an intent needs a title");
                }
                if add_note.as_deref().is_some_and(blank) {
                    return no("the note to add is empty: --add-note \"...\"");
                }
                if title.is_none() && goal.is_none() && findings.is_none() && add_note.is_none() {
                    return no("nothing to change: give a title, a goal, --add-note or --findings");
                }
                IntentUpdated { intent, title, goal, findings, add_note: add_note.map(|n| n.trim().to_string()) }
            }
            UnitRelocated { unit, machine, repo, branch, base, previous } => {
                let u = self.unit(&unit)?;
                if !u.kind.is_code() {
                    return no(format!("unit {unit} is a research unit: it has no repository or branch to move"));
                }
                if u.merged.is_some() {
                    return no(format!("unit {unit} is merged; its place no longer matters"));
                }
                if !user && self.manager_of(&u.intent) != Some(actor) {
                    let manager = self.manager_of(&u.intent).map(|m| format!(" ({})", self.name_of(m).unwrap_or(m))).unwrap_or_default();
                    return no(format!(
                        "only the manager of intent {}{manager} or the user moves its units; tell the manager the new branch (workriver manager tell <your task> --text \"...\"), who runs: workriver unit move {unit} --branch <new> [--base <new base>]",
                        u.intent
                    ));
                }
                self.may_place(who, &u.intent, machine.as_deref())?;
                for (field, v) in [("repo", &repo), ("branch", &branch), ("base", &base)] {
                    if blank(v) {
                        return no(format!("a unit needs a {field}"));
                    }
                }
                // A branch named before the rule keeps its name while only the rest moves.
                unit_branch(&branch, &base, branch != u.branch, u.kind)?;
                UnitRelocated { unit, machine, repo, branch, base, previous }
            }
            CommitLinked { unit, hash, patch_id, subject, date, task } => {
                let u = self.unit(&unit)?;
                if !u.kind.is_code() {
                    return no(format!("unit {unit} is a research unit: it has no branch, so no commits to link"));
                }
                let t = match u.tasks.iter().find(|t| t.n == task) {
                    Some(t) => t,
                    None => return no(format!("no task {unit}#{task}")),
                };
                if blank(&hash) {
                    return no("which commit? give its hash");
                }
                // By whoever did the task, the intent's manager, or the user.
                if !user && self.manager_of(&u.intent) != Some(actor) && !self.is_agent_of(t, actor) {
                    return no(format!("a commit is linked to {unit}#{task} by the agent of that task, the manager of its intent, or the user"));
                }
                CommitLinked { unit, hash, patch_id, subject, date, task }
            }
            CommitUnlinked { unit, hash } => {
                if blank(&hash) {
                    return no("which commit? give its hash");
                }
                let found: Vec<&Unit> = self.units.iter().filter(|u| (unit.is_empty() || u.id == unit) && u.links.iter().any(|l| l.hash.starts_with(&hash))).collect();
                let [u] = found[..] else {
                    return no(if found.is_empty() { format!("no commit {hash} is linked to a task") } else { format!("commit {hash} is linked in several units; name one: workriver commit unlink {hash} <unit>") });
                };
                let l = u.links.iter().find(|l| l.hash.starts_with(&hash)).expect("found above");
                let own = u.tasks.iter().find(|t| t.n == l.task).is_some_and(|t| self.is_agent_of(t, actor));
                if !user && self.manager_of(&u.intent) != Some(actor) && !own {
                    return no("a link is taken back by the agent of its task, the manager of its intent, or the user");
                }
                CommitUnlinked { unit: u.id.clone(), hash: l.hash.clone() }
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
            UnitCreated { intent, title, purpose, repo, branch, base, machine, kind, .. } => {
                let i = self.intent(&intent)?;
                if matches!(i.state, IntentState::Done | IntentState::Dropped) {
                    return no(format!("intent {intent} is {}; it takes no new units", name(&i.state)));
                }
                let id = format!("u{}", self.units.len() + 1);
                // Research and design: no repository, branch or machine of its own; it reads
                // in its intent's project folder.
                if kind == UnitKind::Research {
                    if blank(&title) {
                        return no("a unit needs a title");
                    }
                    if !blank(&branch) || !blank(&base) || !blank(&repo) || machine.is_some() {
                        return no("a research unit has no repository, branch, base or machine: leave out --repo, --branch, --base and --machine");
                    }
                    let folder = self.projects.iter().find(|p| p.id == i.project).map_or(&i.project, |p| &p.path).clone();
                    return Ok(UnitCreated { id, intent, title, purpose, repo: folder, branch, base, machine, kind });
                }
                for (field, v) in [("title", &title), ("repo", &repo), ("branch", &branch), ("base", &base)] {
                    if blank(v) {
                        return no(format!("a unit needs a {field}"));
                    }
                }
                unit_branch(&branch, &base, true, kind)?;
                self.may_place(who, &intent, machine.as_deref())?;
                UnitCreated { id, intent, title, purpose, repo, branch, base, machine, kind }
            }
            UnitMerged { unit, commits } => {
                let u = self.unit(&unit)?;
                if u.kind == UnitKind::Spike {
                    return no(format!("unit {unit} is a spike: it is never merged; close it with `workriver unit done {unit} --reason \"<what was learned>\"`"));
                }
                if !u.kind.merges() {
                    return no(format!("unit {unit} is a research unit: it is never merged; close it with `workriver unit done {unit} --reason \"...\"`"));
                }
                if u.merged.is_some() {
                    return no(format!("unit {unit} is already merged"));
                }
                UnitMerged { unit, commits }
            }
            UnitDone { unit, reason } => {
                let u = self.unit(&unit)?;
                if u.merged.is_some() || u.done.is_some() {
                    return no(format!("unit {unit} is already {}", if u.merged.is_some() { "merged" } else { "done" }));
                }
                if !user && self.manager_of(&u.intent) != Some(actor) {
                    let manager = self.manager_of(&u.intent).map(|m| format!(" ({})", self.name_of(m).unwrap_or(m))).unwrap_or_default();
                    return no(format!("only the manager of intent {}{manager} or the user closes its units", u.intent));
                }
                UnitDone { unit, reason }
            }
            TaskCreated(t) => {
                // A blocker is put by the agent stuck on that task, its manager or the user.
                if let Some(b) = t.blocks
                    && !user
                    && let Ok(stuck) = self.task(&t.unit, b)
                    && !self.is_agent_of(stuck, actor)
                    && self.intent_of(&t.unit).and_then(|i| self.manager_of(i)) != Some(actor)
                {
                    let r = format!("{}#{b}", t.unit);
                    return no(format!("a blocker before {r} is put by the agent of {r}, the manager of its intent, or the user; to tell the manager: workriver manager tell {r} --text \"...\""));
                }
                TaskCreated(self.prepare_task(t)?)
            }
            TaskStarted { unit, task, agent } => {
                let t = self.task(&unit, task)?;
                not_dropped(&unit, t)?;
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
                not_dropped(&unit, t)?;
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
            TaskDropped { unit, task, reason } => {
                let t = self.task(&unit, task)?;
                not_dropped(&unit, t)?;
                if t.state == TaskState::Done {
                    return no(format!("{unit}#{task} is done; only work not done yet is dropped"));
                }
                if blank(&reason) {
                    return no("dropping a task needs a reason: --reason \"...\" (e.g. superseded by u1#17)");
                }
                // The manager re-plans; a worker takes back only what it asked for itself.
                let manager = self.intent_of(&unit).and_then(|i| self.manager_of(i)) == Some(actor);
                if !user && !manager && t.by != actor {
                    return no(format!("{unit}#{task} is dropped by the manager of its intent or the user (or the agent that recorded it); tell your manager it is no longer needed: workriver manager tell {unit}#{task} --text \"...\""));
                }
                TaskDropped { unit, task, reason }
            }
            TaskAmended { unit, task, note, evidence, findings, attachments, add } => {
                let t = self.task(&unit, task)?;
                if t.kind == TaskKind::Gate || t.state != TaskState::Done {
                    return no(format!("{unit}#{task} is not a delivered task; only a delivered task can be amended"));
                }
                if note.is_none() && evidence.is_none() && findings.is_none() && attachments.is_empty() && add.is_empty() {
                    return no("nothing to amend: give a note, evidence or findings");
                }
                if findings.is_some() && !add.is_empty() {
                    return no("either replace the findings (--findings) or add to them (--add-findings), not both");
                }
                if findings.is_some() && t.findings.iter().any(|f| f.state != FindingState::Open) {
                    return no(format!(
                        "some findings of {unit}#{task} are already decided, so they cannot be replaced; add new ones with `workriver task deliver {unit}#{task} --amend --add-findings FILE`, or add to a finding with `workriver finding note {unit}#{task} <id> --text \"...\"`"
                    ));
                }
                // New findings are the reviewer's to report (or the user's), as the first were.
                if !add.is_empty() && !user && !self.is_agent_of(t, actor) {
                    return no(format!("findings of {unit}#{task} are added by the agent that delivered it"));
                }
                check_output(t, evidence.as_deref().unwrap_or_default(), findings.as_deref().unwrap_or(&add))?;
                self.stored(
                    evidence.iter().flatten().filter_map(|e| e.attachment.as_ref()).chain(findings.iter().flatten().chain(&add).flat_map(|f| &f.attachments)).chain(&attachments),
                )?;
                let add = numbered_after(add, &t.findings);
                TaskAmended { unit, task, note, evidence, findings: findings.map(numbered), attachments, add }
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
                let to = match decision {
                    Decision::Accept => FindingState::Accepted,
                    Decision::Revise => FindingState::Revised,
                    Decision::Reject => FindingState::Rejected,
                    Decision::Defer => FindingState::Deferred,
                };
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
                if decision == Decision::Revise && blank(&note) {
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
                }
                FindingDecided { unit, task, finding, decision, note, by: if user { Who::User } else { Who::Agent }, was }
            }
            GateSubmitted { unit, task, result, note, findings, attachments } => {
                self.stored(attachments.iter().chain(findings.iter().flat_map(|f| &f.attachments)))?;
                let t = self.task(&unit, task)?;
                not_dropped(&unit, t)?;
                if t.kind == TaskKind::Gate && t.state == TaskState::Todo {
                    return no(format!("gate {unit}#{task} is not open yet: it follows {}, not done yet", self.unfinished_inputs(&unit, t).join(", ")));
                }
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
            CommandRan { id, command, dir, machine, unit, by, from, step } => {
                if blank(&id) || blank(&command) || blank(&dir) {
                    return no("a command run needs its id, its text and its directory");
                }
                if let Some(u) = &unit {
                    self.unit(u)?;
                }
                CommandRan { id, command, dir, machine, unit, by, from, step }
            }
            CommandEnded { id, exit } => {
                if *who != Principal::Service {
                    return no("only workriver records how a command ended");
                }
                CommandEnded { id, exit }
            }
            CommandReviewed { id, verdict, text } => {
                if *who != Principal::Service {
                    return no("only workriver records a command's review, from the agent it started for it");
                }
                if blank(&text) {
                    return no("a review says what the command does");
                }
                CommandReviewed { id, verdict, text }
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
            ChoreCreated { intent, project, unit, title, steps, .. } => {
                // Of an intent, or (no intent) of a project, or of nothing at all.
                if !intent.is_empty() {
                    self.intent(&intent)?;
                }
                if let Some(u) = &unit
                    && (intent.is_empty() || self.unit(u)?.intent != intent)
                {
                    return no(format!("unit {u} is not part of intent {intent:?}"));
                }
                if blank(&title) {
                    return no("a chore needs a title");
                }
                let project = if intent.is_empty() { project } else { String::new() };
                ChoreCreated { id: format!("c{}", self.chores.len() + 1), intent, project, unit, title, steps }
            }
            QuestionAsked { thread, unit, task, text, context, conclude, .. } => {
                // Task 0 is the unit itself: the user's to ask about; an agent asks about a task.
                let about = match task {
                    0 if !user => return no("a question is about a task: workriver ask <unit>#<n> \"...\""),
                    0 => {
                        self.unit(&unit)?;
                        None
                    }
                    n => Some(self.task(&unit, n)?),
                };
                // Blind work neither asks nor is asked by other agents: both would tell it things.
                if !self.blind_tasks(actor).is_empty() {
                    return no("you are on a blind task: other work is hidden until you deliver, and so are questions. If you cannot go on without one, tell your manager instead: workriver manager tell <your task> --text \"...\"");
                }
                if about.is_some_and(|t| t.blind && !t.state.over()) && !user {
                    return no(format!("{unit}#{task} is a blind task in progress: its agent is not to be told about other work. Ask once it is delivered"));
                }
                if blank(&text) {
                    return no("a question needs its text");
                }
                // An attachment it points at is one shown with that task (with the unit: with one
                // of its tasks): the temporary agent is given its path, so nothing else gets out.
                // The user's question also points at a file the user stored (attached in the page).
                let shown: Vec<&str> = match about {
                    Some(_) => self.attachments_of(&unit, task),
                    None => self.unit(&unit)?.tasks.iter().flat_map(|t| self.attachments_of(&unit, t.n)).collect(),
                };
                for c in context.iter().filter(|c| c.kind == "attachment") {
                    let a = c.id.as_deref().unwrap_or_default();
                    if !shown.contains(&a) && !(user && self.attachments.iter().any(|x| x.id == a && x.user)) {
                        return no(format!("attachment {a:?} is not one of {unit}#{task}'s; a question points only at what is shown with its task, or at a file the user attached to it"));
                    }
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
            MessageSent { intent, workspace, text, .. } => {
                if blank(&text) {
                    return no("a message needs its text");
                }
                // To the manager of the intent's workspace, or of the workspace named (here).
                let (workspace, manager) = if intent.is_empty() {
                    let w = self.workspaces.iter().find(|w| w.group.machine.is_none() && w.group.label == workspace);
                    (workspace, w.and_then(|w| w.manager.clone()))
                } else {
                    let i = self.intent(&intent)?;
                    let label = i.workspace.as_ref().map(|g| self.workspace_of(g).map_or(&g.label, |w| &w.group.label).clone()).unwrap_or_default();
                    (label, self.manager_of(&intent).map(str::to_string))
                };
                let Some(manager) = manager else {
                    return no(format!("{} has no manager; a manager registers with `workriver manager here`", if intent.is_empty() { format!("workspace {workspace:?}") } else { format!("intent {intent}") }));
                };
                MessageSent { id: format!("m{}", self.messages.len() + 1), intent, workspace, manager, text }
            }
            MessageDelivery { id, delivery } => {
                if *who != Principal::Service {
                    return no("only workriver records where a message is");
                }
                self.message(&id)?;
                MessageDelivery { id, delivery }
            }
            ManagerTold { unit, task, text } => {
                if user {
                    return no("the user writes to a manager in workriver's page (Ask, Manager)");
                }
                self.task(&unit, task)?;
                if self.intent_of(&unit).and_then(|i| self.manager_of(i)) == Some(actor) {
                    return no("you are the manager of this intent");
                }
                if blank(&text) {
                    return no("say what: --text \"...\"");
                }
                ManagerTold { unit, task, text }
            }
            MessageReplied { message, text } => {
                let m = self.message(&message)?;
                // Only the manager it was typed to: nobody else answers for it, the user included.
                if m.manager != actor {
                    return no(format!("message {message} went to {}; only that manager replies to it", self.name_of(&m.manager).unwrap_or(&m.manager)));
                }
                if blank(&text) {
                    return no("a reply needs its text");
                }
                MessageReplied { message, text }
            }
            MessageSaid { text, .. } => {
                if user {
                    return no("the user writes to a manager in workriver's page (Ask, Manager)");
                }
                let Some(w) = self.workspaces.iter().find(|w| w.manager.as_deref() == Some(actor)) else {
                    return no("only a workspace's manager says something to the user; a worker tells its manager: `workriver manager tell <task> --text \"...\"`");
                };
                if blank(&text) {
                    return no("say what: --text \"...\"");
                }
                MessageSaid { id: format!("m{}", self.messages.len() + 1), intent: w.current.clone().unwrap_or_default(), workspace: w.group.label.clone(), text }
            }
            MessagesRead { workspace, at } => {
                if blank(&workspace) {
                    return no("which workspace's conversation was read");
                }
                // Stored as the service writes times, so that they compare as text.
                let Ok(t) = humantime::parse_rfc3339_weak(&at) else { return no(format!("{at:?} is not a time")) };
                MessagesRead { workspace, at: humantime::format_rfc3339_seconds(t).to_string() }
            }
        })
    }

    pub fn message(&self, id: &str) -> Result<&Message, Rejected> {
        match self.messages.iter().find(|m| m.id == id) {
            Some(m) => Ok(m),
            None => no(format!("no message {id}")),
        }
    }

    fn prepare_task(&self, mut t: NewTask) -> Result<NewTask, Rejected> {
        // A task is in a unit, or (research, design and their reviews and gates) in an intent.
        let (owner, intent, count) = if let Ok(u) = self.unit(&t.unit) {
            if u.merged.is_some() {
                return no(format!("unit {} is merged; record new work in a new unit", u.id));
            }
            if !u.kind.is_code() && matches!(t.kind, TaskKind::Implement | TaskKind::Tests) {
                return no(format!("{} is a research unit: it takes research, design, review and gate tasks; implement and tests tasks belong to a unit with a branch", u.id));
            }
            (u.id.clone(), u.intent.clone(), u.tasks.len())
        } else {
            // Before 2026-11-17 research and design went into the intent itself (`i3#1`); those
            // tasks still replay and work, but new ones go into a research unit.
            let i = self.intent(&t.unit).map_err(|_| Rejected(format!("no unit {}", t.unit)))?;
            return no(format!(
                "{} is an intent: tasks go into a unit. For research or design, record a research unit first: workriver unit add {} --kind research --title \"...\", then workriver task add u<N> ...",
                i.id, i.id
            ));
        };
        if let Some(i) = self.intents.iter().find(|i| i.id == intent)
            && matches!(i.state, IntentState::Done | IntentState::Dropped)
        {
            return no(format!("intent {intent} is {}; it takes no new tasks", name(&i.state)));
        }
        // A blocker stands where the task it blocks starts: it follows what that task follows.
        if let Some(b) = t.blocks {
            if t.kind != TaskKind::Gate {
                return no("only a gate blocks a task: --kind gate --gate deploy|test|decide");
            }
            let stuck = self.task(&owner, b)?;
            if stuck.kind == TaskKind::Gate || stuck.state.over() {
                return no(format!("{owner}#{b} is {}: a blocker goes before a task still to do or running", if stuck.kind == TaskKind::Gate { "a gate".to_string() } else { name(&stuck.state) }));
            }
            if t.inputs.is_empty() {
                t.inputs = if stuck.inputs.is_empty() { vec![Input { from: "start".into(), primary: true }] } else { stuck.inputs.clone() };
            }
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
        // The first task starts at the start; a later one must say what it follows, else it
        // would hang off the start beside the first, away from the work it continues.
        if t.inputs.is_empty() && count > 0 {
            return no(format!("{owner} already has tasks: say which one this follows with --after <n> (the latest is #{count}), or --after start to run beside the first"));
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
                    findings: FindingsPolicy::default(),
                    tasks: vec![],
                    by: e.actor.clone(),
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
            ProjectRenamed { project, title } => {
                if let Some(p) = self.projects.iter_mut().find(|p| &p.id == project || &p.path == project) {
                    p.title = title.clone();
                }
            }
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
                        // No longer worked on: no longer where new work goes. New work then
                        // goes to the intent started last among those still active there.
                        if let Some(g) = before {
                            let at = self.workspace_at(&g);
                            let next = self.intents.iter().rev().find(|i| i.state == IntentState::Active && &i.id != intent && i.workspace.as_ref().is_some_and(|x| self.workspace_at(x) == at && at.is_some())).map(|i| i.id.clone());
                            let w = self.workspace_mut(&g);
                            if w.current.as_deref() == Some(intent.as_str()) {
                                w.current = next;
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
            IntentMerged { intent, from, title } => {
                let mut blocks = vec![];
                for f in from {
                    if let Some(x) = self.intents.iter_mut().find(|i| &i.id == f) {
                        blocks.push(merged_note(x));
                        x.state = IntentState::Dropped;
                        x.reason = Some(format!("merged into {intent}"));
                    }
                    self.roadmap.retain(|r| r != f);
                }
                if let Some(i) = self.intents.iter_mut().find(|i| &i.id == intent) {
                    let own = i.note.trim_end().to_string();
                    i.note = std::iter::once(own).chain(blocks).filter(|b| !b.is_empty()).collect::<Vec<_>>().join("\n\n");
                    if let Some(t) = title {
                        i.title = t.clone();
                    }
                }
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
            WorkspaceProjectSet { workspace, project } => self.workspace_mut(workspace).project = project.clone(),
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
            IntentUpdated { intent, title, goal, findings, add_note } => {
                if let Some(i) = self.intents.iter_mut().find(|i| &i.id == intent) {
                    if let Some(n) = add_note {
                        let own = i.note.trim_end();
                        i.note = if own.is_empty() { n.clone() } else { format!("{own}\n\n{n}") };
                    }
                    if let Some(t) = title {
                        i.title = t.clone();
                    }
                    if let Some(g) = goal {
                        i.goal = g.clone();
                    }
                    if let Some(f) = findings {
                        i.findings = *f;
                    }
                }
            }
            CommitLinked { unit, hash, patch_id, subject, date, task } => {
                if let Some(u) = self.units.iter_mut().find(|u| &u.id == unit) {
                    // One link per commit: the later one stands.
                    u.links.retain(|l| &l.hash != hash);
                    u.links.push(CommitLink { hash: hash.clone(), patch_id: patch_id.clone(), subject: subject.clone(), date: date.clone(), task: *task, by: e.actor.clone() });
                }
            }
            CommitUnlinked { unit, hash } => {
                if let Some(u) = self.units.iter_mut().find(|u| &u.id == unit) {
                    u.links.retain(|l| &l.hash != hash);
                }
            }
            UnitRelocated { unit, machine, repo, branch, base, previous } => {
                if let Some(u) = self.units.iter_mut().find(|u| &u.id == unit) {
                    // Where it was is kept, with what was seen there.
                    if (&u.machine, &u.repo, &u.branch, &u.base) != (machine, repo, branch, base) {
                        u.earlier.push(Earlier { machine: u.machine.clone(), repo: u.repo.clone(), branch: u.branch.clone(), base: u.base.clone(), until: at.clone(), commits: previous.clone() });
                    }
                    u.machine = machine.clone();
                    u.repo = repo.clone();
                    u.branch = branch.clone();
                    u.base = base.clone();
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
            UnitCreated { id, intent, title, purpose, repo, branch, base, machine, kind } => self.units.push(Unit {
                kind: *kind,
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
                earlier: vec![],
                links: vec![],
                done: None,
                reopened: None,
            }),
            UnitMerged { unit, commits } => {
                if let Some(u) = self.units.iter_mut().find(|u| &u.id == unit) {
                    u.merged = Some(at.clone());
                    u.merged_commits = commits.clone();
                }
            }
            UnitDone { unit, reason } => {
                if let Some(u) = self.units.iter_mut().find(|u| &u.id == unit) {
                    u.done = Some(Closed { at: at.clone(), by: e.actor.clone(), reason: reason.clone() });
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
                        blocks: t.blocks,
                        // A gate opens (waits for the user) once what it follows is done.
                        state: TaskState::Todo,
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
                        waiting_since: None,
                        attachments: vec![],
                        dropped: None,
                    })
                }
                // More work for a unit closed as done: it is open again, its state from its tasks.
                if let Some(u) = self.units.iter_mut().find(|u| u.id == t.unit)
                    && let Some(was) = u.done.take()
                {
                    u.reopened = Some(was);
                }
                // The task it blocks follows it from now on.
                if let Some(b) = t.blocks
                    && let Some(stuck) = self.task_mut(&t.unit, b)
                {
                    stuck.inputs.push(Input { from: t.n.to_string(), primary: false });
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
                    f.decisions.push(Decided { state: f.state, by: *by, agent: agent_of(&e.actor), note: note.clone(), at: at.clone() });
                }
            }
            TaskDropped { unit, task, reason } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.state = TaskState::Dropped;
                    t.dropped = Some(Closed { at: at.clone(), by: e.actor.clone(), reason: reason.clone() });
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
                let user = e.actor == "user";
                match self.attachments.iter_mut().find(|a| &a.id == id) {
                    Some(a) => a.user |= user,
                    None => self.attachments.push(Attachment { id: id.clone(), name: name.clone(), mime: mime.clone(), size: *size, by: e.actor.clone(), at: at.clone(), user }),
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
            // The latest few only (the user's recent runs); never their output.
            CommandRan { id, command, dir, machine, unit, by, from, step } => {
                self.runs.push(CommandRun { id: id.clone(), command: command.clone(), dir: dir.clone(), machine: machine.clone(), unit: unit.clone(), by: by.clone(), from: from.clone(), step: *step, at: at.clone(), exit: None, review: None });
                let over = self.runs.len().saturating_sub(RUNS_KEPT);
                self.runs.drain(..over);
            }
            CommandEnded { id, exit } => {
                if let Some(r) = self.runs.iter_mut().find(|r| &r.id == id) {
                    r.exit = Some(*exit);
                }
            }
            CommandReviewed { id, verdict, text } => {
                if let Some(r) = self.runs.iter_mut().find(|r| &r.id == id) {
                    r.review = Some((*verdict, text.clone()));
                }
            }
            DeliveryUpdated { chore: Some(c), delivery, .. } => {
                if let Some(x) = self.chores.iter_mut().find(|x| &x.id == c) {
                    x.delivery = Some(delivery.clone());
                }
            }
            FindingNoted { unit, task, finding, text, attachments } => {
                if let Some(f) = self.task_mut(unit, *task).and_then(|t| t.findings.iter_mut().find(|f| &f.found.id == finding)) {
                    f.notes.push(FindingNote { by: e.actor.clone(), at: at.clone(), text: text.clone(), attachments: attachments.clone() });
                }
            }
            TaskAmended { unit, task, note, evidence, findings, attachments, add } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.findings.extend(add.iter().cloned().map(open));
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
            ChoreCreated { id, intent, project, unit, title, steps } => self.chores.push(Chore {
                id: id.clone(),
                intent: intent.clone(),
                project: project.clone(),
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
                answered_at: None,
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
                    q.answered_at = Some(at.clone());
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
            MessageSent { id, intent, workspace, manager, text } => self.messages.push(Message {
                id: id.clone(),
                intent: intent.clone(),
                workspace: workspace.clone(),
                manager: manager.clone(),
                text: text.clone(),
                at: at.clone(),
                delivery: None,
                replies: vec![],
                by_manager: false,
            }),
            MessageSaid { id, intent, workspace, text } => self.messages.push(Message {
                id: id.clone(),
                intent: intent.clone(),
                workspace: workspace.clone(),
                manager: e.actor.clone(),
                text: text.clone(),
                at: at.clone(),
                delivery: None,
                replies: vec![],
                by_manager: true,
            }),
            MessagesRead { workspace, at: upto } => {
                let r = self.read.entry(workspace.clone()).or_default();
                if *upto > *r {
                    *r = upto.clone();
                }
            }
            MessageDelivery { id, delivery } => {
                if let Some(m) = self.messages.iter_mut().find(|m| &m.id == id) {
                    m.delivery = Some(delivery.clone());
                }
            }
            MessageReplied { message, text } => {
                if let Some(m) = self.messages.iter_mut().find(|m| &m.id == message) {
                    m.replies.push(Reply { by: e.actor.clone(), text: text.clone(), at: at.clone() });
                }
            }
            // Only told: the record is the event itself.
            ManagerTold { .. } => {}
        }
        self.open_gates(at);
    }

    /// The tasks a task follows that are not done yet (`start` is always done; a dropped
    /// task holds nothing up).
    pub fn unfinished_inputs(&self, owner: &str, t: &Task) -> Vec<String> {
        t.inputs
            .iter()
            .filter(|i| i.from != "start")
            .filter(|i| i.from.parse().ok().and_then(|n| self.task(owner, n).ok()).is_none_or(|x| !x.state.over()))
            .map(|i| format!("{owner}#{}", i.from))
            .collect()
    }

    /// A gate waits for the user only once every task it follows is done; until then it is
    /// still to do, asks nobody for anything, and counts nowhere. It waits from then on.
    fn open_gates(&mut self, at: &str) {
        let todo = |ts: &[Task]| ts.iter().filter(|t| t.kind == TaskKind::Gate && t.state == TaskState::Todo).map(|t| t.n).collect::<Vec<_>>();
        let owners: Vec<(String, Vec<u32>)> = self.units.iter().map(|u| (u.id.clone(), todo(&u.tasks))).chain(self.intents.iter().map(|i| (i.id.clone(), todo(&i.tasks)))).filter(|(_, g)| !g.is_empty()).collect();
        for (owner, gates) in owners {
            for n in gates {
                let ready = self.task(&owner, n).is_ok_and(|t| self.unfinished_inputs(&owner, t).is_empty());
                if ready && let Some(t) = self.task_mut(&owner, n) {
                    t.state = TaskState::Waiting;
                    t.waiting_since = Some(at.to_string());
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

    fn finding_mut(&mut self, r: &FindingRef) -> Option<&mut Finding> {
        if r.task == 0 {
            return self.backlog.iter_mut().find(|b| b.finding.found.id == r.finding).map(|b| &mut b.finding);
        }
        self.task_mut(&r.unit, r.task)?.findings.iter_mut().find(|f| f.found.id == r.finding)
    }
}

/// A dropped task is no longer worked on: refused, saying why it was dropped.
fn not_dropped(unit: &str, t: &Task) -> Result<(), Rejected> {
    match &t.dropped {
        Some(d) => no(format!("{unit}#{} was dropped as no longer needed ({}); it is not started, delivered or submitted any more", t.n, d.reason)),
        None => Ok(()),
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
    Finding { found, state: FindingState::Open, decided_by: None, note: None, delivery: None, reason: None, notes: vec![], decisions: vec![] }
}

/// Ids for findings added later: per severity, after the highest there is.
fn numbered_after(mut add: Vec<NewFinding>, have: &[Finding]) -> Vec<NewFinding> {
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
fn numbered(mut findings: Vec<NewFinding>) -> Vec<NewFinding> {
    let mut seen = [0; 3];
    for f in &mut findings {
        let k = f.severity as usize;
        seen[k] += 1;
        f.id = format!("{:?}-{}", f.severity, seen[k]);
    }
    findings
}
