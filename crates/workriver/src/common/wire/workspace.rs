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
    /// Every workspace the runtime has, per machine, with what it is linked to.
    pub(crate) workspaces: Vec<Group>,
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
    pub(crate) workspace: String,
    /// The manager's name.
    pub(crate) manager: String,
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
    /// Started by the manager (`workriver say`): `text` is the manager's.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) by_manager: bool,
    /// The manager's message, not read by the user yet.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) unread: bool,
}

#[derive(Serialize)]
pub(crate) struct ReplyOut {
    pub(crate) by: String,
    pub(crate) text: String,
    pub(crate) at: String,
    /// Not read by the user yet.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) unread: bool,
}

/// A unit's file texts, which the page's snapshot leaves out (`Cache::bare`): read when its
/// code is shown, and again only when `code` (the unit's `code` in the snapshot) changes.
#[derive(Serialize)]
pub struct Code {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) code: String,
    pub(crate) commits: Vec<CommitCode>,
}

#[derive(Serialize)]
pub(crate) struct CommitCode {
    pub(crate) id: String,
    pub(crate) files: Vec<scan::FileChange>,
}

/// A runtime workspace: of this machine, or of another with the local one it is linked to.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Group {
    pub(crate) id: String,
    pub(crate) label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) machine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) linked_to: Option<String>,
    /// The project (its folder) it works on, its title, and how that is known; none: unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project_from: Option<Known>,
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
    /// The runtime group it runs in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) group: Option<core::RuntimeRef>,
    pub(crate) ended: bool,
    /// The runtime's state of the agent, while it lives.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) state: Option<String>,
    /// The current intent of the runtime group it works in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) intent: Option<String>,
    /// It manages its runtime group.
    pub(crate) manager: bool,
    /// The machine it runs on; none: this one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) machine: Option<String>,
    /// Its machine cannot be reached: what is shown is as last seen.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) unreachable: bool,
}

/// A project: a registered folder of repositories, or a repository outside all of them.
#[derive(Serialize)]
pub(crate) struct ProjectOut {
    /// Its folder, which is what intents name.
    pub(crate) id: String,
    /// A registered project's short id (p1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) key: Option<String>,
    pub(crate) title: String,
    pub(crate) registered: bool,
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
    /// The runtime group it runs (or last ran) in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) workspace: Option<core::RuntimeRef>,
    /// Planned: its place in its project's roadmap, from 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) order: Option<usize>,
    /// Dropped: why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
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

/// An addition to a finding.
#[derive(Serialize)]
pub(crate) struct NoteOut {
    pub(crate) by: String,
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
    /// A merge commit (the base merged into the branch): its files are not work of its own.
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

/// How workriver knows which project a workspace works on.
#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Known {
    /// Its manager runs inside the project's folder.
    Manager,
    /// Its current intent belongs to the project.
    Intents,
    /// The user said so (`workspace.project_set`).
    User,
}
