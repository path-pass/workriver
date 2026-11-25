// The recorded event's content: the one vocabulary of what can happen, as stored (the
// `kind` and `body` columns). Its tags, fields and serde attributes are the stored format.
use serde::{Deserialize, Serialize};

use super::*;
use crate::choices::Choice;

/// A field that may be absent (None, by `default`), written as null (Some(None)) or given:
/// serde reads a plain double option's null as absent.
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
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
        /// The project's folder: one a registered project holds, or empty (no project).
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
        /// What kind of work it is, how big, and free tags: none when not given, and then not
        /// written (an intent without them is stored as before).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<IntentKind>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size: Option<IntentSize>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
    },
    /// A project: a folder holding one or more repositories.
    #[serde(rename = "project.added")]
    ProjectAdded {
        #[serde(default)]
        id: String,
        path: String,
        title: String,
    },
    /// A project's title changed; `project` is its id or its folder.
    #[serde(rename = "project.renamed")]
    ProjectRenamed { project: String, title: String },
    /// Planned → active (started in a workspace) → done; or dropped; or back to planned.
    #[serde(rename = "intent.state_changed")]
    IntentStateChanged {
        intent: String,
        state: IntentState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        /// Where it is started: the workspace's id. The user names one of the intent's
        /// project; a manager's start is filled in here, from the seat it holds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace: Option<String>,
    },
    /// The user moves an active intent to another workspace of its project.
    #[serde(rename = "intent.workspace_set")]
    IntentWorkspaceSet { intent: String, workspace: String },
    /// A planned intent's place in its project's roadmap.
    #[serde(rename = "intent.moved")]
    IntentMoved {
        intent: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<String>,
    },
    /// Planned intents folded into another planned one of their project (a roadmap split too
    /// finely): each is dropped, and its title, goal, note and external id go into the
    /// target's note, in the order given. The target keeps its place in the roadmap; with
    /// `title`, it takes that title (the merged whole may need a new name).
    #[serde(rename = "intent.merged")]
    IntentMerged {
        intent: String,
        from: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
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
    /// The user makes a workspace in a registered project (`project`: its id or folder).
    #[serde(rename = "workspace.created")]
    WorkspaceCreated {
        #[serde(default)]
        id: String,
        name: String,
        project: String,
    },
    #[serde(rename = "workspace.renamed")]
    WorkspaceRenamed { workspace: String, name: String },
    /// Closed: it has no active intent; its seat is emptied. Its intents and conversation stay.
    #[serde(rename = "workspace.closed")]
    WorkspaceClosed { workspace: String },
    #[serde(rename = "workspace.opened")]
    WorkspaceOpened { workspace: String },
    /// The user seats a workspace's manager (a session; none: empties the seat). `machine` and
    /// `folder` are where the runtime showed that session, filled in by the service.
    /// `replaces`: the seat the user was shown; refused when it is no longer that one.
    #[serde(rename = "workspace.manager_set")]
    WorkspaceManagerSet {
        workspace: String,
        #[serde(default)]
        manager: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        folder: String,
        #[serde(default)]
        replaces: Option<SeatRef>,
    },
    /// A seated session whose terminal was gone shows again where it was (the same machine and
    /// folder) and has its seat back by itself: a line of the workspace's conversation, which
    /// only workriver records. The seat and its appointment are as they were.
    #[serde(rename = "workspace.manager_back")]
    ManagerBack {
        #[serde(default)]
        id: String,
        workspace: String,
        session: String,
    },
    /// The user starts a manager for a workspace (the page's Start manager): the harness and
    /// model they chose, and the place the service has just opened for it with its shell.
    /// `folder` is filled in by the rules (the project's folder); the agent goes by a name of
    /// the start's own (`ManagerStart::name_of`).
    /// What it brings up is seated by `ManagerStartSeated`, which names it by its sequence.
    /// Recorded only by the start action, never through `/api/append`.
    #[serde(rename = "workspace.manager_started")]
    WorkspaceManagerStarted {
        workspace: String,
        harness: String,
        /// Empty: the harness's own default.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        model: String,
        #[serde(default)]
        folder: String,
        place: String,
        shell: Shell,
        #[serde(default)]
        replaces: Option<SeatRef>,
    },
    /// The end of the user's start: the session it brought up is seated, as the service read it
    /// at the start's place and shell. workriver's own (`Body::service_only`); the rules accept
    /// it only for the workspace's pending start (`start`: its sequence).
    #[serde(rename = "workspace.manager_start_seated")]
    ManagerStartSeated {
        workspace: String,
        start: u64,
        session: String,
        harness: String,
        folder: String,
        place: String,
        shell: Shell,
    },
    /// The user's start ended without a seating: why, and the runtime's words when it said
    /// any. workriver's own, for the workspace's pending start only.
    #[serde(rename = "workspace.manager_start_ended")]
    ManagerStartEnded {
        workspace: String,
        start: u64,
        reason: StartEnd,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        words: String,
    },
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
    /// An agent as the runtime shows it; only workriver records it (`Body::service_only`).
    #[serde(rename = "agent.seen")]
    AgentSeen {
        session: String,
        name: String,
        harness: String,
        /// The working directory it was first seen in, as the runtime gave it; where some
        /// harnesses keep the session. Empty before it was recorded.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        cwd: String,
        /// The machine it runs on; none: this one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
    },
    /// A new title, goal, note, kind, size or tags, or who decides its findings. Title, goal
    /// and findings are the user's; the note, kind, size and tags are also its manager's while
    /// it is active, and the note takes additions from a manager of its project while it is
    /// planned (`prepare_intents`).
    #[serde(rename = "intent.updated")]
    IntentUpdated {
        intent: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        goal: Option<String>,
        /// Who decides its findings from now on; the user's alone to set.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        findings: Option<FindingsPolicy>,
        /// A paragraph added at the end of its note (a blank line before it).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        add_note: Option<String>,
        /// The whole note, in place of what it held (empty: none).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        /// The version the change was made from: a replace is refused when what it replaces
        /// changed after it. None: not checked (records from before versions, an addition).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<u32>,
        /// Absent: as it is; null: cleared; else the new kind (`present`: null and absent are
        /// two things here).
        #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
        kind: Option<Option<IntentKind>>,
        #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
        size: Option<Option<IntentSize>>,
        /// The whole new set; `[]` clears it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tags: Option<Vec<String>>,
    },
    /// A unit's repository is somewhere else than recorded: another machine, path or branch.
    #[serde(rename = "unit.relocated")]
    UnitRelocated {
        unit: String,
        /// The machine it is on now; none: the one workriver runs on.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
        repo: String,
        branch: String,
        base: String,
        /// The commits on the place it leaves, read once now: what was there is remembered,
        /// since the old branch is not read again.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        previous: Vec<SeenCommit>,
    },
    /// A commit belongs to a task because someone says so: for one made without the
    /// `Task:` trailer, without rewriting history.
    #[serde(rename = "commit.linked")]
    CommitLinked {
        unit: String,
        /// The commit's full hash, as resolved in the unit's repository.
        hash: String,
        /// Its `git patch-id --stable`: what a rebased copy of it is known by.
        #[serde(default)]
        patch_id: String,
        #[serde(default)]
        subject: String,
        /// The author's date, which a rebase keeps too.
        #[serde(default)]
        date: String,
        task: u32,
    },
    #[serde(rename = "commit.unlinked")]
    CommitUnlinked {
        /// Empty when recorded: the unit that has the link.
        #[serde(default)]
        unit: String,
        /// The hash, or its beginning.
        hash: String,
    },
    #[serde(rename = "unit.created")]
    UnitCreated {
        #[serde(default)]
        id: String,
        intent: String,
        title: String,
        #[serde(default)]
        purpose: String,
        /// Absolute path of the repository (or a worktree of it). A research unit: its
        /// intent's project folder, filled in here (may be empty).
        #[serde(default)]
        repo: String,
        /// Empty for a research unit, as is `base`.
        #[serde(default)]
        branch: String,
        #[serde(default)]
        base: String,
        /// The machine the repository is on; none: this one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
        #[serde(default, skip_serializing_if = "UnitKind::is_default")]
        kind: UnitKind,
        /// The template it was made with (required of a new unit; none before templates).
        #[serde(default, skip_serializing_if = "String::is_empty")]
        template: String,
        /// The user's template's revision (the sequence of its `template.set`); none: built
        /// in. Filled in here, as `placed` is: what the request says is thrown away.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        revision: Option<u64>,
        /// The template's steps as placed: one task each, numbered from 1.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        placed: Vec<TemplateStep>,
    },
    /// The user saves a template in the library: the whole of it each time, and the text it
    /// was read from (kept for the user to edit, comments included; never in the state).
    /// Recorded only by the action that reads the text, so the two cannot differ.
    #[serde(rename = "template.set")]
    TemplateSet {
        #[serde(flatten)]
        template: Template,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        source: String,
    },
    /// The user removes a template of theirs; one of a built-in's name brings it back.
    #[serde(rename = "template.removed")]
    TemplateRemoved { name: String },
    /// Finished without a merge.
    #[serde(rename = "unit.done")]
    UnitDone {
        unit: String,
        #[serde(default)]
        reason: String,
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
    /// A task placed from a template given what it asks (`workriver task fill`), once, by the
    /// intent's manager or the user, who becomes its author. Its kind, reviewer, gate and step
    /// are the template's: a fill cannot say them. `inputs` are added to those it was placed
    /// with.
    #[serde(rename = "task.filled")]
    TaskFilled {
        unit: String,
        task: u32,
        #[serde(default)]
        title: String,
        #[serde(default)]
        text: String,
        #[serde(default)]
        agent: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        inputs: Vec<Input>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        plans: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        steps: Vec<Step>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        blind: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        choice: Option<Choice>,
    },
    #[serde(rename = "task.started")]
    TaskStarted {
        unit: String,
        task: u32,
        #[serde(default)]
        agent: Option<String>,
        /// The model the agent says it works with (`task start --model`); the runtime does
        /// not tell it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
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
    /// A task no longer needed (a gate asked again, a plan changed): kept, never deleted;
    /// what follows it no longer waits on it.
    #[serde(rename = "task.dropped")]
    TaskDropped { unit: String, task: u32, reason: String },
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
        /// More findings, after those it has (which stay, decided or not).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        add: Vec<NewFinding>,
    },
    /// A requirement added to a task not delivered yet, by the intent's manager or the user:
    /// part of what the task asks from then on. What it asked first is never rewritten.
    #[serde(rename = "task.noted")]
    TaskNoted {
        unit: String,
        task: u32,
        #[serde(default)]
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<String>,
    },
    /// Something added to a finding after it was reported: a requirement for its fix, a
    /// detail. The finding itself is never rewritten.
    #[serde(rename = "finding.noted")]
    FindingNoted {
        unit: String,
        task: u32,
        finding: String,
        #[serde(default)]
        text: String,
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
        /// The user changing a decision already made: what it was before.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        was: Option<FindingState>,
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
        /// A choice gate answered: the option's value (one; a list so more may come).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        selected: Vec<String>,
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
    /// The attachment rule took these files out of the store (one event per run and way):
    /// deleted, or moved to `to`, the target as the settings gave it (kind and all; core
    /// keeps it and reads nothing of it). `rule` is what made them go. workriver's own
    /// (`Body::service_only`): only the service's run records it, after a verified copy and
    /// before the local delete.
    #[serde(rename = "attachment.removed")]
    AttachmentRemoved {
        ids: Vec<String>,
        how: Removed,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<Target>,
        rule: Conditions,
    },
    /// A removed file's bytes are in the store again: the service wrote them (an upload of
    /// the same content). workriver's own (`Body::service_only`); the only event that clears
    /// `gone`.
    #[serde(rename = "attachment.restored")]
    AttachmentRestored { id: String },
    /// The user ran a command in a shell of their own, from the page, after confirming its
    /// exact text: what, where, and who wrote it. Recorded before it runs; never its output.
    #[serde(rename = "command.ran")]
    CommandRan {
        id: String,
        command: String,
        dir: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
        /// The unit whose work tree it ran in, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
        /// Who wrote it (an agent's session, or "user").
        #[serde(default)]
        by: String,
        /// Whose step it was: a task (`u1#3`) or a chore (`c2`); none: typed by the user.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<String>,
        /// Which of that task's or chore's steps (from 1).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<u32>,
    },
    /// Such a command ended, with its exit status, as workriver saw it end.
    #[serde(rename = "command.ended")]
    CommandEnded { id: String, exit: i32 },
    /// What the agent workriver started to review such a command said it does, recorded by
    /// workriver once that agent (and only it) gave it and the command ran.
    #[serde(rename = "command.reviewed")]
    CommandReviewed { id: String, verdict: Verdict, text: String },
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
    /// The user asks why about a task, or (task 0, the user only) about a unit itself; a
    /// temporary agent answers.
    #[serde(rename = "question.asked")]
    QuestionAsked {
        #[serde(default)]
        id: String,
        /// The thread it continues; empty starts a new one (whose id is this question's).
        #[serde(default)]
        thread: String,
        unit: String,
        /// 0: about the unit, not one of its tasks.
        #[serde(default)]
        task: u32,
        text: String,
        #[serde(default)]
        context: Vec<AskContext>,
        /// A request to write the thread's conclusion rather than a question.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        conclude: bool,
        /// The user discusses a delivered design or research with its own agent: one thread
        /// per task, answered by that agent alone, until the task's gate is submitted.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        discuss: bool,
    },
    /// The user has read the discussion of a task (`unit#task`) up to `at` (the time of the
    /// newest answer the page showed); the page records it.
    #[serde(rename = "questions.read")]
    QuestionsRead { unit: String, task: u32, at: String },
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
        /// The intent it belongs to; empty: a project's (or nobody's) chore.
        #[serde(default)]
        intent: String,
        /// Without an intent: the project's folder; empty: it belongs to no project.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        project: String,
        #[serde(default)]
        unit: Option<String>,
        title: String,
        #[serde(default)]
        steps: Vec<Step>,
    },
    #[serde(rename = "chore.done")]
    ChoreDone { chore: String },
    /// The user writes to a workspace's manager from the page; workriver types it into the
    /// manager's terminal. Given an intent (its workspace's manager) or a workspace's id;
    /// the rest is filled in here.
    #[serde(rename = "message.sent")]
    MessageSent {
        #[serde(default)]
        id: String,
        /// The intent it is about, if any.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        intent: String,
        /// The workspace whose manager it goes to, by id.
        #[serde(default)]
        workspace: String,
        /// That manager's session.
        #[serde(default)]
        manager: String,
        text: String,
        /// The unit the user had picked, if any.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        unit: String,
        /// One of its tasks; 0: the unit, or none.
        #[serde(default, skip_serializing_if = "is_zero")]
        task: u32,
        /// What else the user picked or attached: findings, files, lines, attachments, other
        /// tasks of the unit.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        context: Vec<AskContext>,
        /// To every manager: the managers it goes to, one copy each (then no intent,
        /// workspace or manager above). Filled in by the service: the seats of the open
        /// workspaces whose session runs now.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        to: Vec<Recipient>,
        /// The user's answer to a manager's choice: which message (or reply of it) asked, and
        /// the value picked; the text is the user's note, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        answer: Option<Answer>,
    },
    /// Where the user's message to a manager is; workriver's alone to record.
    #[serde(rename = "message.delivery")]
    MessageDelivery {
        /// The message's id (its text as typed is the delivery's `message`).
        id: String,
        /// A message to every manager: whose copy (that manager's session); else empty.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        manager: String,
        #[serde(flatten)]
        delivery: Delivery,
    },
    /// The manager the message went to answers it (`workriver reply`); it may answer more than once.
    #[serde(rename = "message.replied")]
    MessageReplied {
        message: String,
        text: String,
        /// Options the user picks from (see `choices`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        choice: Option<Choice>,
    },
    /// A workspace's manager starts a message to the user (`workriver say`): shown in that
    /// workspace's conversation on the page, as the manager's. Nothing is typed anywhere.
    #[serde(rename = "message.said")]
    MessageSaid {
        #[serde(default)]
        id: String,
        /// The workspace's current intent, if any; filled in here.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        intent: String,
        /// The workspace the caller holds the seat of, by id; filled in here.
        #[serde(default)]
        workspace: String,
        text: String,
        /// Options the user picks from (see `choices`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        choice: Option<Choice>,
    },
    /// A user's rating (or its removal) of a manager's words, with the session as read by the service.
    #[serde(rename = "message.rated")]
    MessageRated {
        message: String,
        #[serde(default, skip_serializing_if = "crate::records::is_zero")]
        reply: u32,
        vote: Option<Vote>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        #[serde(default)]
        session: String,
        #[serde(default)]
        usage: SessionUsage,
    },
    /// The user has read a workspace's conversation (by its id; empty: the one with every
    /// manager) up to `at` (the time of the newest message of the manager's the page showed);
    /// the page records it.
    #[serde(rename = "messages.read")]
    MessagesRead { workspace: String, at: String },
    /// An agent tells the manager of a task's intent something (`workriver manager tell`):
    /// typed into the manager's terminal, quoted. The way a worker reaches its manager.
    #[serde(rename = "manager.told")]
    ManagerTold { unit: String, task: u32, text: String },
    /// The user answered a dialog in an agent's terminal from the page: the keys sent, what
    /// came of them, and of the screen only its question line and the option's number and
    /// words, cut where a command, a value or a path begins (`screens::kept`).
    #[serde(rename = "agent.keys")]
    AgentKeys {
        /// The agent's session.
        agent: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
        /// The task it held, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task: Option<u32>,
        keys: Vec<crate::screens::Key>,
        outcome: crate::screens::Outcome,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        question: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        option: Option<crate::screens::KeptOption>,
    },
}

impl Body {
    /// The intent, unit and task (`u1#3`) an event concerns, for the indexed columns.
    pub fn refs(&self) -> (Option<String>, Option<String>, Option<String>) {
        use Body::*;
        let task = |u: &str, n: u32| Some(format!("{u}#{n}"));
        match self {
            IntentCreated { id, .. } => (Some(id.clone()), None, None),
            AgentSeen { .. } | QuestionStarted { .. } | QuestionAnswered { .. } | QuestionFailed { .. } | ThreadEnded { .. } => (None, None, None),
            QuestionsRead { unit, task: n, .. } => (None, Some(unit.clone()), task(unit, *n)),
            MessageDelivery { .. } | MessageReplied { .. } | MessageRated { .. } | MessagesRead { .. } => (None, None, None),
            ManagerTold { unit, task: n, .. } => (None, Some(unit.clone()), task(unit, *n)),
            MessageSent { intent, .. } | MessageSaid { intent, .. } => ((!intent.is_empty()).then(|| intent.clone()), None, None),
            QuestionAsked { unit, task: 0, .. } => (None, Some(unit.clone()), None),
            QuestionAsked { unit, task: n, .. } => (None, Some(unit.clone()), task(unit, *n)),
            IntentWorkspaceSet { intent, .. } | IntentUpdated { intent, .. } | IntentStateChanged { intent, .. } | IntentMoved { intent, .. } | IntentMerged { intent, .. } | FindingsBatched { intent, .. } => (Some(intent.clone()), None, None),
            ProjectAdded { .. } | ProjectRenamed { .. } | TemplateSet { .. } | TemplateRemoved { .. } => (None, None, None),
            FindingDropped { unit, task: n, .. } => (None, (!unit.is_empty()).then(|| unit.clone()), (!unit.is_empty()).then(|| task(unit, *n)).flatten()),
            FindingAdded { intent, .. } => (intent.clone(), None, None),
            WorkspaceCreated { .. } | WorkspaceRenamed { .. } | WorkspaceClosed { .. } | WorkspaceOpened { .. } | WorkspaceManagerSet { .. } | ManagerBack { .. } | WorkspaceManagerStarted { .. } | ManagerStartSeated { .. } | ManagerStartEnded { .. } => (None, None, None),
            DeliveryUpdated { chore: Some(_), .. } => (None, None, None),
            UnitCreated { id, intent, .. } => (Some(intent.clone()), Some(id.clone()), None),
            UnitMerged { unit, .. } | UnitDone { unit, .. } | UnitRelocated { unit, .. } | CommitUnlinked { unit, .. } => (None, Some(unit.clone()), None),
            CommitLinked { unit, task: n, .. } => (None, Some(unit.clone()), task(unit, *n)),
            TaskCreated(t) => ((!t.intent.is_empty()).then(|| t.intent.clone()), Some(t.unit.clone()), task(&t.unit, t.n)),
            TaskStarted { unit, task: n, .. }
            | TaskFilled { unit, task: n, .. }
            | TaskDelivered { unit, task: n, .. }
            | TaskDropped { unit, task: n, .. }
            | TaskAmended { unit, task: n, .. }
            | TaskNoted { unit, task: n, .. }
            | FindingNoted { unit, task: n, .. }
            | FindingDecided { unit, task: n, .. }
            | GateSubmitted { unit, task: n, .. }
            | DeliveryUpdated { unit, task: n, .. } => (None, Some(unit.clone()), task(unit, *n)),
            ChoreCreated { intent, unit, .. } => ((!intent.is_empty()).then(|| intent.clone()), unit.clone(), None),
            ChoreDone { .. } => (None, None, None),
            AttachmentAdded { unit, task: n, .. } => (None, unit.clone(), unit.as_ref().zip(*n).and_then(|(u, n)| task(u, n))),
            AttachmentRemoved { .. } | AttachmentRestored { .. } => (None, None, None),
            CommandRan { unit, .. } => (None, unit.clone(), None),
            CommandEnded { .. } | CommandReviewed { .. } => (None, None, None),
            AgentKeys { unit, task: n, .. } => (None, unit.clone(), unit.as_ref().zip(*n).and_then(|(u, n)| task(u, n))),
        }
    }
}
