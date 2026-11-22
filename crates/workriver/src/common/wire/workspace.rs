// The workspace in the exact shape the web UI reads (frontend/web/src/*/types.ts): what the
// service builds for `/api/workspace` and `/api/unit/{id}/code`. Shapes only; the service's
// views fill them.
use serde::Serialize;
use workriver_core::{self as core, FindingState};
use workriver_scan as scan;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub(crate) repos: Vec<Repo>,
    pub(crate) projects: Vec<ProjectOut>,
    pub(crate) backlog: Vec<BacklogOut>,
    pub(crate) built_at: String,
    /// The agent runtime's name, as its adapter gives it: the page's texts that must name the
    /// runtime show it; the page itself names none.
    pub(crate) runtime: &'static str,
    pub(crate) intents: Vec<Intent>,
    pub(crate) sessions: Vec<Session>,
    /// Every workspace, open and closed, in the order they were made.
    pub(crate) workspaces: Vec<WorkspaceOut>,
    /// The other machines followed, with whether each can be reached.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) machines: Vec<super::MachineState>,
    /// A part only: what an agent on a blind task may see.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) reduced: bool,
    /// Chores that belong to no intent: a project's, or (no project) nobody's.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) chores: Vec<Chore>,
    /// Machines past an alert threshold now: for the user only (Needs you).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<super::Alert>,
    /// The user's messages to managers, with their replies, oldest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) messages: Vec<MessageOut>,
    /// The commands the user ran from the page, newest first: for the user only.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<RunOut>,
}

/// A command the user ran from the page, as the Run section lists it; never its output.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOut {
    pub id: String,
    pub command: String,
    pub dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Whose step it was (`u1#3`, `c2`) and which step; none: typed by the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
    pub at: String,
    /// "running" (its terminal is read, no end yet), "done" (`exit` set), or "unknown" (not
    /// followed: the service restarted, or its terminal closed before the end was seen).
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit: Option<i32>,
    /// Its terminal can be brought to the front: this service knows where it is.
    pub focusable: bool,
    /// What its review found, if it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<workriver_core::Verdict>,
}

/// The user's message to a workspace's manager, as the page shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MessageOut {
    pub(crate) id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) intent: String,
    /// The workspace whose conversation it is in, by its id; empty: every manager's.
    pub(crate) workspace: String,
    /// The manager's name.
    pub(crate) manager: String,
    /// A line of workriver's own, not a message: what it says (`manager` is who it is about).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) notice: Option<core::Notice>,
    pub(crate) text: String,
    /// What the user picked with it: a unit, one of its tasks, and the rest, as chips.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) unit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) task: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) context: Vec<core::AskContext>,
    /// The stored files its context points at, to open in the page.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) attachments: Vec<AttachmentOut>,
    pub(crate) at: String,
    /// sending, blocked (the manager waits for a person in its terminal; it goes in once
    /// free), held (the manager is on a blind task; it goes once that is delivered, `reason`
    /// says so), delivered or not-delivered.
    pub(crate) step: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason_code: Option<core::ReasonCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) blocked: Option<Blocked>,
    pub(crate) replies: Vec<ReplyOut>,
    /// To every manager (`workspace` and `manager` then empty): each copy, with where it is
    /// and its manager's replies (`replies` above then empty).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) to: Vec<CopyOut>,
    /// Started by the manager (`workriver say`): `text` is the manager's.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) by_manager: bool,
    /// The manager's message, not read by the user yet.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) unread: bool,
    /// The manager's message with options (`say`), the user's answer to it, and whether the
    /// user can still answer it (not answered, and asked under the appointment that holds the
    /// workspace's seat now).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) choice: Option<core::Choice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) answered: Option<core::Answered>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) open: bool,
    /// The user's message that answers a manager's options: which, and the value picked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) answer: Option<core::Answer>,
}

/// One manager's copy of the user's message to every manager, as the page shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CopyOut {
    /// The workspace it manages, by its id, and the manager's machine (none: this one).
    pub(crate) workspace: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) machine: Option<String>,
    /// The manager's name.
    pub(crate) manager: String,
    /// As a message's `step`, `reasonCode`, `reason` and `blocked`.
    pub(crate) step: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason_code: Option<core::ReasonCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) blocked: Option<Blocked>,
    pub(crate) replies: Vec<ReplyOut>,
}

#[derive(Serialize)]
pub(crate) struct ReplyOut {
    pub(crate) by: String,
    pub(crate) text: String,
    pub(crate) at: String,
    /// Not read by the user yet.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) unread: bool,
    /// As on `MessageOut`: the reply's options, their answer, and whether it is still open.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) choice: Option<core::Choice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) answered: Option<core::Answered>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) open: bool,
}

/// A unit's file texts, which the page's snapshot leaves out (`Cache::bare`): read when its
/// code is shown, and again only when `code` (the unit's `code` in the snapshot) changes.
#[derive(Serialize)]
pub struct Code {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) code: String,
    pub(crate) commits: Vec<CommitCode>,
    /// The unit's net change with its texts (`Unit::net`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) net: Option<Net>,
}

/// A unit's net change against its base, as `git diff base...head` names it: from where the
/// branch meets its base now (merged: where it met it before), to its last commit.
#[derive(Serialize)]
pub(crate) struct Net {
    /// "from..tip" (full hashes): it changes when the base moves, the commits the same.
    pub(crate) id: String,
    pub(crate) files: Vec<scan::FileChange>,
}

#[derive(Serialize)]
pub(crate) struct CommitCode {
    pub(crate) id: String,
    pub(crate) files: Vec<scan::FileChange>,
}

/// A workspace, as the page shows it: workriver's own record, with its manager's seat.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceOut {
    /// `ws1`: what everything names it by. The name is shown only, and may repeat.
    pub(crate) id: String,
    pub(crate) name: String,
    /// Its project's folder (a project's `id` on the page).
    pub(crate) project: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) closed: bool,
    /// The intent new work goes to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) current: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) manager: Option<SeatOut>,
    /// Its manager does not run where it was seated: the session to propose for the seat
    /// (the seated one in another terminal, or the agent now in the old terminal), if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) proposal: Option<String>,
}

/// A manager's seat, as the page shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SeatOut {
    pub(crate) session: String,
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) machine: Option<String>,
    /// The folder it was seated in.
    pub(crate) folder: String,
    /// The appointment: what a change of the seat names as the seat it replaces.
    pub(crate) since: u64,
    /// The runtime's state of it, or "gone" when it is not running.
    pub(crate) state: String,
}

#[derive(Serialize)]
pub(crate) struct Repo {
    pub(crate) path: String,
    /// The machine it is on; none: this one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) machine: Option<String>,
    pub(crate) name: String,
    pub(crate) modules: scan::Module,
}

#[derive(Serialize)]
pub(crate) struct Session {
    pub(crate) agent: String,
    /// The session's own id; names may repeat across workspaces, this does not.
    pub(crate) session: String,
    /// What the session is about (its terminal title), while it lives.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) topic: Option<String>,
    /// Waiting for a person in its terminal since then (a question, an approval).
    #[serde(rename = "blockedSince", skip_serializing_if = "Option::is_none")]
    pub(crate) blocked_since: Option<String>,
    /// Idle (or done) in its terminal since then, while it lives.
    #[serde(rename = "idleSince", skip_serializing_if = "Option::is_none")]
    pub(crate) idle_since: Option<String>,
    /// The harness that runs it (claude, codex), when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) harness: Option<String>,
    /// Where its terminal is in the runtime: shown as a hint, never a workspace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) group: Option<core::RuntimeRef>,
    /// Its workspace by the records (its id): the one whose seat it holds, else the one
    /// workspace of the tasks it has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) workspace: Option<String>,
    /// The folder it was started in, while it lives.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) folder: String,
    pub(crate) ended: bool,
    /// The runtime's state of the agent, while it lives.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) state: Option<String>,
    /// The current intent of its workspace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) intent: Option<String>,
    /// It holds its workspace's seat.
    pub(crate) manager: bool,
    /// The machine it runs on; none: this one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) machine: Option<String>,
    /// Its machine cannot be reached: what is shown is as last seen.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) unreachable: bool,
}

/// A project: a registered folder of repositories.
#[derive(Serialize)]
pub(crate) struct ProjectOut {
    /// Its folder, which is what intents name.
    pub(crate) id: String,
    /// A registered project's short id (p1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) key: Option<String>,
    pub(crate) title: String,
    pub(crate) repos: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Intent {
    pub(crate) id: String,
    pub(crate) title: String,
    /// One sentence on what it is for; may be empty.
    pub(crate) goal: String,
    pub(crate) state: core::IntentState,
    /// Its project's folder; empty when nothing says.
    pub(crate) project: String,
    /// Who decides its open findings: the user, or (handed over by the user) its manager.
    pub(crate) findings: core::FindingsPolicy,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) external: Option<core::External>,
    /// The workspace it runs (or last ran) in, by its id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) workspace: Option<String>,
    /// Planned: its place in its project's roadmap, from 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) order: Option<usize>,
    /// Dropped: why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
    /// What kind of work it is, how big it is expected to be, its tags: left out when not set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<core::IntentKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) size: Option<core::IntentSize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) tags: Vec<String>,
    pub(crate) units: Vec<Unit>,
    pub(crate) chores: Vec<Chore>,
    /// The manager's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) manager: Option<String>,
    /// The manager's runtime state, or "gone" when it is not running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) manager_state: Option<String>,
    /// Still active, and every unit of it merged or done: it can be closed.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) complete: bool,
    /// Its last version (1 when never changed): what an edit names as its base.
    pub(crate) version: u32,
    /// Who made that version and when; its history is read on its own route.
    pub(crate) edited: Edited,
}

/// Who made an intent's last version, and when.
#[derive(Serialize)]
pub(crate) struct Edited {
    pub(crate) at: String,
    /// The agent's name; none: the user.
    pub(crate) by: Option<String>,
}

/// Where the message about a user's input is, as the page shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Sent {
    pub(crate) step: core::DeliveryStep,
    /// The receiver's name.
    pub(crate) to: Option<String>,
    pub(crate) to_manager: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason_code: Option<core::ReasonCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
    pub(crate) message: String,
}

#[derive(Serialize)]
pub(crate) struct Chore {
    pub(crate) id: String,
    /// A chore of no intent: its project's folder; empty: it belongs to no project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project: Option<String>,
    pub(crate) title: String,
    pub(crate) by: String,
    pub(crate) steps: Vec<core::Step>,
    pub(crate) state: &'static str,
    pub(crate) since: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) unit: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct Unit {
    pub(crate) id: String,
    /// The intent's own work (research, design) shown as a unit without a repository.
    #[serde(rename = "intentLevel", skip_serializing_if = "std::ops::Not::not")]
    pub(crate) intent_level: bool,
    /// Research and design, with no branch, or a spike, never merged; absent: code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<core::UnitKind>,
    pub(crate) title: String,
    pub(crate) purpose: String,
    pub(crate) repo: String,
    /// The machine its repository is on; none: this one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) machine: Option<String>,
    pub(crate) branch: Option<String>,
    /// The branch it leaves from and is merged into.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) base: Option<String>,
    /// When it was recorded as merged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) merged: Option<String>,
    /// The branches its work was on before (a rebase onto a new branch, a rename).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) previously: Vec<String>,
    pub(crate) status: &'static str,
    /// Where it stands: working, waiting, ready, merged or done.
    pub(crate) progress: core::Progress,
    /// Closed without a merge: why.
    #[serde(rename = "doneReason", skip_serializing_if = "Option::is_none")]
    pub(crate) done_reason: Option<String>,
    pub(crate) tasks: Vec<Task>,
    pub(crate) commits: Vec<Commit>,
    /// Its net change (`Net`): the code tree with nothing picked. None where git could not
    /// tell, or before it was read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) net: Option<Net>,
    /// Which texts its commits and uncommitted work have (a hash of them): the page reads
    /// them (`code`) again only when this changes.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) code: String,
    /// Ready, and it looks squash-merged (`scan::squashed`): where, and by which commit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) squash: Option<Squash>,
    /// Ready for longer than the user's `readyHours` without a merge: since when.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) overdue: Option<String>,
    /// Conversations about the unit as a whole ("ask why" with no task picked), oldest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) threads: Vec<ThreadOut>,
    /// Open, the files it changes that another open unit of its project, in the same
    /// repository, changes too: by path.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) overlaps: Vec<Overlap>,
}

/// A file two or more open units change (`Unit::overlaps`): its path and the other units.
#[derive(Serialize)]
pub(crate) struct Overlap {
    pub(crate) path: String,
    pub(crate) units: Vec<String>,
}

/// Which harness and model did a task, as the page shows it.
#[derive(Serialize)]
pub(crate) struct Ran {
    /// The harness, as the runtime named it, once the agent was seen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) harness: Option<String>,
    /// The model, as the agent said when it started the task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) model: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Task {
    pub(crate) id: String,
    pub(crate) kind: &'static str,
    pub(crate) state: String,
    pub(crate) task_id: Option<String>,
    pub(crate) title: String,
    pub(crate) agent: Option<String>,
    /// The agent's session, to tell agents of the same name apart.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) agent_session: Option<String>,
    pub(crate) writing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) raised_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) gate: Option<core::GateKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) since: Option<String>,
    /// When its agent started it, and when it was delivered (a gate: submitted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) started: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) delivered: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reviewer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result: Option<&'static str>,
    pub(crate) inputs: Vec<core::Input>,
    pub(crate) input: TaskIn,
    pub(crate) output: TaskOut,
    pub(crate) plans: Vec<String>,
    /// Gates: where the message with the user's result is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) delivery: Option<Sent>,
    /// A design task (shown like implement work; its output is its note).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) design: bool,
    /// Done without seeing other work: its findings are its own view.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) blind: bool,
    /// A blocker (a gate): the task it was put before, by its id as in `inputs`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) blocks: Option<String>,
    /// It has no commits on the unit's branch now, but had these on a branch the unit left.
    #[serde(rename = "lostCommits", skip_serializing_if = "Vec::is_empty")]
    pub(crate) lost_commits: Vec<String>,
    /// A research task (shown like implement work; its output is its note and attachments).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) research: bool,
    /// Conversations about it ("ask why"), oldest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) threads: Vec<ThreadOut>,
    /// Waiting gates: the latest conclusion reached about it or the work it follows, to start
    /// the user's note from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) draft: Option<String>,
    /// When its output was last corrected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) amended: Option<String>,
    /// Dropped as no longer needed: when, by whom (by name), why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dropped: Option<core::Closed>,
    /// Running, and its agent stopped without delivering (idle too long, or gone).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stalled: Option<super::Stall>,
    /// Its agent's harness and model: all the page shows of its trace (the session and its
    /// file are kept for `task show --json` and `task session`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) trace: Option<Ran>,
    /// A delivered design or research with its agent: whether the user may discuss it now
    /// ("open"), its agent is gone ("gone"), or its gate was submitted ("over").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) discuss: Option<&'static str>,
}

#[derive(Serialize)]
pub(crate) struct TaskIn {
    pub(crate) text: String,
    pub(crate) causes: Vec<Cause>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) steps: Option<Vec<core::Step>>,
    /// Requirements added to what it asks since it was recorded, oldest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) notes: Vec<NoteOut>,
    /// A decide gate's options, as stored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) choice: Option<core::Choice>,
}

#[derive(Serialize)]
pub(crate) struct Cause {
    pub(crate) review: String,
    pub(crate) findings: Vec<String>,
}

#[derive(Serialize, Default)]
pub(crate) struct TaskOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) commits: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) findings: Option<Vec<Finding>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) evidence: Option<Vec<EvidenceOut>>,
    /// Gates: pictures with the note; other tasks: files with the output.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) attachments: Vec<AttachmentOut>,
    /// A design or research: the gate that asks the user about it (`State::result_gate`), by
    /// its id as in `inputs`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) gate: Option<String>,
    /// An answered gate: the option picked.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) selected: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct EvidenceOut {
    pub(crate) command: String,
    pub(crate) result: core::CheckResult,
    pub(crate) summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) attachment: Option<AttachmentOut>,
}

/// A stored file as the page shows it: an image previews, anything else is a link.
#[derive(Serialize, Clone)]
pub(crate) struct AttachmentOut {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) url: String,
    #[serde(rename = "type")]
    pub(crate) mime: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Finding {
    pub(crate) id: String,
    pub(crate) severity: core::Severity,
    pub(crate) problem: String,
    pub(crate) impact: String,
    pub(crate) evidence: Vec<String>,
    pub(crate) state: FindingState,
    pub(crate) leads_to: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) decided_by: Option<core::Who>,
    /// When it was last decided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) decided: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) recommendation: Option<core::Suggestion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) delivery: Option<Sent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) attachments: Vec<AttachmentOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) external: Option<core::External>,
    /// While open: "manager" when it is the intent's manager's to decide (and there is one
    /// running); absent: it waits for the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) decider: Option<&'static str>,
    /// While open: it would be the manager's, but none is registered or running, so it
    /// falls to the user.
    #[serde(rename = "noManager", skip_serializing_if = "std::ops::Not::not")]
    pub(crate) no_manager: bool,
    /// What was added to it since it was reported, oldest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) notes: Vec<NoteOut>,
    /// Since when it is known: when its task was delivered, or it was added to the backlog.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) since: Option<String>,
    /// Dropped: why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
}

/// An addition to a finding, or to what a task asks.
#[derive(Serialize)]
pub(crate) struct NoteOut {
    /// The name its author is shown by.
    pub(crate) by: String,
    /// The user wrote it: told by who recorded it, never by the name (an agent may be called "user").
    pub(crate) user: bool,
    pub(crate) at: String,
    pub(crate) text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) attachments: Vec<AttachmentOut>,
}

/// A deferred finding from outside workriver's tasks, in its project's backlog.
#[derive(Serialize)]
pub(crate) struct BacklogOut {
    pub(crate) project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) intent: Option<String>,
    pub(crate) source: String,
    pub(crate) by: String,
    pub(crate) finding: Finding,
}

#[derive(Serialize)]
pub(crate) struct Commit {
    pub(crate) id: String,
    pub(crate) subject: String,
    pub(crate) date: String,
    pub(crate) author: String,
    pub(crate) live: bool,
    pub(crate) files: Vec<scan::FileChange>,
    /// It belongs to its task because someone linked it (`workriver commit link`), not by
    /// its own `Task:` trailer.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) linked: bool,
    /// The same change as this commit of a branch the unit left (a rebase): its old hash.
    #[serde(rename = "rebasedFrom", skip_serializing_if = "Option::is_none")]
    pub(crate) rebased_from: Option<String>,
    /// A merge that brought the base into the branch (a parent other than its first is in the
    /// history of where the branch meets its base): its files, against its first parent as
    /// every commit's, are the base's work, none of it a task's. A merge of any other branch
    /// is not one: what it brought is the branch's.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) merge: bool,
}

#[derive(Serialize)]
pub(crate) struct ThreadOut {
    pub(crate) id: String,
    pub(crate) ended: bool,
    /// Its temporary agent runs, so a follow-up keeps its context.
    pub(crate) live: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) conclusion: Option<String>,
    pub(crate) turns: Vec<Turn>,
    /// The user's discussion of the task with its own agent.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) discuss: bool,
}

#[derive(Serialize)]
pub(crate) struct Turn {
    pub(crate) id: String,
    /// The agent that asked; absent when the user did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) by: Option<String>,
    pub(crate) question: String,
    pub(crate) context: Vec<core::AskContext>,
    /// The stored files its context points at (the user's own, or the task's), to open in the page.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) attachments: Vec<AttachmentOut>,
    pub(crate) answer: Option<String>,
    /// Who answered (the agent's name, or "user") and when.
    #[serde(rename = "answerBy", skip_serializing_if = "Option::is_none")]
    pub(crate) answer_by: Option<String>,
    #[serde(rename = "answeredAt", skip_serializing_if = "Option::is_none")]
    pub(crate) answered_at: Option<String>,
    /// While it comes: starting (the agent), reading (it works on it), blocked (its agent
    /// waits for a person in its terminal: the question waits too).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) step: Option<&'static str>,
    /// While blocked: the agent that waits, to bring its terminal to the front.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) blocked: Option<Blocked>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) consulted: Option<Consulted>,
    pub(crate) at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) failed: Option<Failed>,
    /// A request for the thread's conclusion.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) conclude: bool,
    /// While held (step "held"): why, in the service's words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
    /// A discussion's answer the user has not read yet.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) unread: bool,
}

#[derive(Serialize)]
pub(crate) struct Blocked {
    pub(crate) agent: String,
    pub(crate) session: String,
}

#[derive(Serialize)]
pub(crate) struct Consulted {
    pub(crate) agent: String,
    pub(crate) outcome: &'static str,
}

#[derive(Serialize)]
pub(crate) struct Failed {
    pub(crate) code: core::ReasonCode,
    pub(crate) reason: String,
}

/// A unit that looks squash-merged: its branch's tip when it was seen so, where its change
/// is (its base, or the base's upstream) and the commit there that brought it.
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
pub struct Squash {
    pub tip: String,
    pub into: String,
    pub commit: String,
}
