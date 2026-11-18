// The workspace in the exact shape the web UI reads (frontend/web/src/types.ts): the
// recorded state, the units' commits from git, and the agents the runtime knows.
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use serde::Serialize;
use workriver_core::runtime::{AgentState, RuntimeAgent};
use workriver_core::{self as core, FindingState, TaskKind, TaskState, name, parse_task_ref};
use workriver_scan as scan;

use crate::quote;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    repos: Vec<Repo>,
    projects: Vec<ProjectOut>,
    backlog: Vec<BacklogOut>,
    built_at: String,
    intents: Vec<Intent>,
    sessions: Vec<Session>,
    /// Every workspace the runtime has, per machine, with what it is linked to.
    workspaces: Vec<Group>,
    /// The other machines followed, with whether each can be reached.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    machines: Vec<crate::machines::MachineState>,
    /// A part only: what an agent on a blind task may see.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    reduced: bool,
    /// Chores that belong to no intent: a project's, or (no project) nobody's.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    chores: Vec<Chore>,
    /// Machines past an alert threshold now: for the user only (Needs you).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<crate::metrics::Alert>,
    /// The user's messages to managers, with their replies, oldest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    messages: Vec<MessageOut>,
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
struct MessageOut {
    id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    intent: String,
    workspace: String,
    /// The manager's name.
    manager: String,
    text: String,
    at: String,
    /// sending, blocked (the manager waits for a person in its terminal; it goes in once
    /// free), delivered or not-delivered.
    step: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason_code: Option<core::ReasonCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocked: Option<Blocked>,
    replies: Vec<ReplyOut>,
    /// Started by the manager (`workriver say`): `text` is the manager's.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    by_manager: bool,
    /// The manager's message, not read by the user yet.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    unread: bool,
}

#[derive(Serialize)]
struct ReplyOut {
    by: String,
    text: String,
    at: String,
    /// Not read by the user yet.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    unread: bool,
}

fn messages(state: &core::State, live: &[RuntimeAgent], names: &Names) -> Vec<MessageOut> {
    state
        .messages
        .iter()
        .map(|m| {
            // The manager's words the user has not read: written after the user last read
            // the workspace's conversation (times are the service's, so they compare as text).
            let read = state.read.get(&m.workspace);
            let unread = |at: &String| read.is_none_or(|r| at > r);
            let stuck = live.iter().find(|a| a.session == m.manager && a.state == AgentState::Blocked).filter(|_| m.delivery.is_none());
            MessageOut {
                id: m.id.clone(),
                intent: m.intent.clone(),
                workspace: m.workspace.clone(),
                manager: names.of(&m.manager),
                text: m.text.clone(),
                at: m.at.clone(),
                step: match m.delivery.as_ref().map(|d| d.step) {
                    // Nothing to deliver: it is the manager's.
                    _ if m.by_manager => "delivered",
                    None if stuck.is_some() => "blocked",
                    None => "sending",
                    Some(core::DeliveryStep::NotDelivered) => "not-delivered",
                    Some(_) => "delivered",
                },
                reason_code: m.delivery.as_ref().and_then(|d| d.reason_code),
                reason: m.delivery.as_ref().and_then(|d| d.reason.clone()),
                blocked: stuck.map(|a| Blocked { agent: a.name.clone(), session: a.session.clone() }),
                replies: m.replies.iter().map(|r| ReplyOut { by: names.of(&r.by), text: r.text.clone(), at: r.at.clone(), unread: unread(&r.at) }).collect(),
                by_manager: m.by_manager,
                unread: m.by_manager && unread(&m.at),
            }
        })
        .collect()
}

/// Takes out of a snapshot built from `State::blind_view` what git adds to it and an agent
/// on a blind task is not to be told: other commits' subjects and authors (the range to
/// review is the hashes; the code is read from git), tasks made up from unregistered
/// commits, uncommitted work, and what other sessions are about.
pub fn hide(ws: &mut Workspace) {
    ws.reduced = true;
    for u in ws.intents.iter_mut().flat_map(|i| i.units.iter_mut()) {
        hide_unit(u);
    }
    for s in &mut ws.sessions {
        s.topic = None;
        // When another agent went idle would say when it delivered.
        s.idle_since = None;
    }
}

/// `hide` for one unit; its code (`code`) is hidden the same way.
fn hide_unit(u: &mut Unit) {
    u.tasks.retain(|t| t.state != "unregistered");
    u.commits.retain(|c| !c.live);
    u.code.clear();
    for c in &mut u.commits {
        c.subject.clear();
        c.author.clear();
        c.files.clear();
    }
}

/// A unit's file texts, which the page's snapshot leaves out (`Cache::bare`): read when its
/// code is shown, and again only when `code` (the unit's `code` in the snapshot) changes.
#[derive(Serialize)]
pub struct Code {
    #[serde(skip_serializing_if = "String::is_empty")]
    code: String,
    commits: Vec<CommitCode>,
}

#[derive(Serialize)]
struct CommitCode {
    id: String,
    files: Vec<scan::FileChange>,
}

/// The texts of unit `id` as `state` lets its reader see them (none of a blind view's),
/// but for the commits in `have` (short hashes), which the reader has.
pub fn code(state: &core::State, cache: &mut Cache, live: Option<&[RuntimeAgent]>, id: &str, have: &[&str]) -> Option<Code> {
    let u = state.units.iter().find(|u| u.id == id && u.has_code())?;
    cache.refs.clear();
    cache.worktrees.clear();
    (cache.recomputed, cache.reused) = (0, 0);
    let mut shown = unit(state, live.unwrap_or_default(), u, cache, &Names(HashMap::new()), None);
    if state.reduced {
        hide_unit(&mut shown);
    }
    let commits = shown.commits.into_iter().filter(|c| c.live || !have.contains(&c.id.as_str())).map(|c| CommitCode { id: c.id, files: c.files }).collect();
    Some(Code { code: shown.code, commits })
}

fn chore(c: &core::Chore, names: &Names, project: Option<String>) -> Chore {
    Chore {
        id: c.id.clone(),
        project,
        title: c.title.clone(),
        by: names.of(&c.by),
        steps: c.steps.clone(),
        state: if c.done { "done" } else { "open" },
        since: c.at.clone(),
        unit: c.unit.clone(),
    }
}

/// A runtime workspace: of this machine, or of another with the local one it is linked to.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Group {
    id: String,
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    machine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    linked_to: Option<String>,
    /// The project (its folder) it works on, its title, and how that is known; none: unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_from: Option<Known>,
}

/// The group a linked group of another machine stands for: this machine's, by the link.
fn canon(state: &core::State, groups: &[core::RuntimeRef], g: &core::RuntimeRef) -> core::RuntimeRef {
    match state.link_of(g) {
        None => g.clone(),
        Some(l) => groups.iter().find(|x| x.machine.is_none() && x.label == l.local).cloned().unwrap_or_else(|| core::RuntimeRef { id: l.local.clone(), label: l.local.clone(), machine: None }),
    }
}

#[derive(Serialize)]
struct Repo {
    path: String,
    /// The machine it is on; none: this one.
    #[serde(skip_serializing_if = "Option::is_none")]
    machine: Option<String>,
    name: String,
    modules: scan::Module,
}

#[derive(Serialize)]
struct Session {
    agent: String,
    /// The session's own id; names may repeat across workspaces, this does not.
    session: String,
    /// What the session is about (its terminal title), while it lives.
    #[serde(skip_serializing_if = "Option::is_none")]
    topic: Option<String>,
    /// Waiting for a person in its terminal since then (a question, an approval).
    #[serde(rename = "blockedSince", skip_serializing_if = "Option::is_none")]
    blocked_since: Option<String>,
    /// Idle (or done) in its terminal since then, while it lives.
    #[serde(rename = "idleSince", skip_serializing_if = "Option::is_none")]
    idle_since: Option<String>,
    /// The harness that runs it (claude, codex), when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    harness: Option<String>,
    /// The runtime group (Herdr workspace) it runs in.
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<core::RuntimeRef>,
    ended: bool,
    /// The runtime's state of the agent, while it lives.
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<String>,
    /// The current intent of the runtime group it works in.
    #[serde(skip_serializing_if = "Option::is_none")]
    intent: Option<String>,
    /// It manages its runtime group.
    manager: bool,
    /// The machine it runs on; none: this one.
    #[serde(skip_serializing_if = "Option::is_none")]
    machine: Option<String>,
    /// Its machine cannot be reached: what is shown is as last seen.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    unreachable: bool,
}

/// A project: a registered folder of repositories, or a repository outside all of them.
#[derive(Serialize)]
struct ProjectOut {
    /// Its folder, which is what intents name.
    id: String,
    /// A registered project's short id (p1).
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    title: String,
    registered: bool,
    repos: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Intent {
    id: String,
    title: String,
    /// One sentence on what it is for; may be empty.
    goal: String,
    state: core::IntentState,
    /// Its project's folder; empty when nothing says.
    project: String,
    /// Who decides its open findings: the user, or (handed over by the user) its manager.
    findings: core::FindingsPolicy,
    #[serde(skip_serializing_if = "String::is_empty")]
    note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    external: Option<core::External>,
    /// The runtime group it runs (or last ran) in.
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace: Option<core::RuntimeRef>,
    /// Planned: its place in its project's roadmap, from 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    order: Option<usize>,
    /// Dropped: why.
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    units: Vec<Unit>,
    chores: Vec<Chore>,
    /// The manager's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    manager: Option<String>,
    /// The manager's runtime state, or "gone" when it is not running.
    #[serde(skip_serializing_if = "Option::is_none")]
    manager_state: Option<String>,
    /// Still active, and every unit of it merged or done: it can be closed.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    complete: bool,
}

/// Where the message about a user's input is, as the page shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sent {
    step: core::DeliveryStep,
    /// The receiver's name.
    to: Option<String>,
    to_manager: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason_code: Option<core::ReasonCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    message: String,
}

#[derive(Serialize)]
struct Chore {
    id: String,
    /// A chore of no intent: its project's folder; empty: it belongs to no project.
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<String>,
    title: String,
    by: String,
    steps: Vec<core::Step>,
    state: &'static str,
    since: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit: Option<String>,
}

#[derive(Serialize)]
struct Unit {
    id: String,
    /// The intent's own work (research, design) shown as a unit without a repository.
    #[serde(rename = "intentLevel", skip_serializing_if = "std::ops::Not::not")]
    intent_level: bool,
    /// Research and design, with no branch, or a spike, never merged; absent: code.
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<core::UnitKind>,
    title: String,
    purpose: String,
    repo: String,
    /// The machine its repository is on; none: this one.
    #[serde(skip_serializing_if = "Option::is_none")]
    machine: Option<String>,
    branch: Option<String>,
    /// The branch it leaves from and is merged into.
    #[serde(skip_serializing_if = "Option::is_none")]
    base: Option<String>,
    /// When it was recorded as merged.
    #[serde(skip_serializing_if = "Option::is_none")]
    merged: Option<String>,
    /// The branches its work was on before (a rebase onto a new branch, a rename).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    previously: Vec<String>,
    status: &'static str,
    /// Where it stands: working, waiting, ready, merged or done.
    progress: core::Progress,
    /// Closed without a merge: why.
    #[serde(rename = "doneReason", skip_serializing_if = "Option::is_none")]
    done_reason: Option<String>,
    tasks: Vec<Task>,
    commits: Vec<Commit>,
    /// Which texts its commits and uncommitted work have (a hash of them): the page reads
    /// them (`code`) again only when this changes.
    #[serde(skip_serializing_if = "String::is_empty")]
    code: String,
    /// Ready, and it looks squash-merged (`scan::squashed`): where, and by which commit.
    #[serde(skip_serializing_if = "Option::is_none")]
    squash: Option<Squash>,
    /// Ready for longer than the user's `readyHours` without a merge: since when.
    #[serde(skip_serializing_if = "Option::is_none")]
    overdue: Option<String>,
    /// Conversations about the unit as a whole ("ask why" with no task picked), oldest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    threads: Vec<ThreadOut>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Task {
    id: String,
    kind: &'static str,
    state: String,
    task_id: Option<String>,
    title: String,
    agent: Option<String>,
    /// The agent's session, to tell agents of the same name apart.
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_session: Option<String>,
    writing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    raised_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gate: Option<core::GateKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    since: Option<String>,
    /// When its agent started it, and when it was delivered (a gate: submitted).
    #[serde(skip_serializing_if = "Option::is_none")]
    started: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    delivered: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reviewer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<&'static str>,
    inputs: Vec<core::Input>,
    input: TaskIn,
    output: TaskOut,
    plans: Vec<String>,
    /// Gates: where the message with the user's result is.
    #[serde(skip_serializing_if = "Option::is_none")]
    delivery: Option<Sent>,
    /// A design task (shown like implement work; its output is its note).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    design: bool,
    /// Done without seeing other work: its findings are its own view.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    blind: bool,
    /// A blocker (a gate): the task it was put before, by its id as in `inputs`.
    #[serde(skip_serializing_if = "Option::is_none")]
    blocks: Option<String>,
    /// It has no commits on the unit's branch now, but had these on a branch the unit left.
    #[serde(rename = "lostCommits", skip_serializing_if = "Vec::is_empty")]
    lost_commits: Vec<String>,
    /// A research task (shown like implement work; its output is its note and attachments).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    research: bool,
    /// Conversations about it ("ask why"), oldest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    threads: Vec<ThreadOut>,
    /// Waiting gates: the latest conclusion reached about it or the work it follows, to start
    /// the user's note from.
    #[serde(skip_serializing_if = "Option::is_none")]
    draft: Option<String>,
    /// When its output was last corrected.
    #[serde(skip_serializing_if = "Option::is_none")]
    amended: Option<String>,
    /// Dropped as no longer needed: when, by whom (by name), why.
    #[serde(skip_serializing_if = "Option::is_none")]
    dropped: Option<core::Closed>,
    /// Running, and its agent stopped without delivering (idle too long, or gone).
    #[serde(skip_serializing_if = "Option::is_none")]
    stalled: Option<crate::stall::Stall>,
}

#[derive(Serialize)]
struct TaskIn {
    text: String,
    causes: Vec<Cause>,
    #[serde(skip_serializing_if = "Option::is_none")]
    steps: Option<Vec<core::Step>>,
}

#[derive(Serialize)]
struct Cause {
    review: String,
    findings: Vec<String>,
}

#[derive(Serialize, Default)]
struct TaskOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    commits: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    findings: Option<Vec<Finding>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence: Option<Vec<EvidenceOut>>,
    /// Gates: pictures with the note; other tasks: files with the output.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    attachments: Vec<AttachmentOut>,
}

#[derive(Serialize)]
struct EvidenceOut {
    command: String,
    result: core::CheckResult,
    summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    attachment: Option<AttachmentOut>,
}

/// A stored file as the page shows it: an image previews, anything else is a link.
#[derive(Serialize, Clone)]
struct AttachmentOut {
    id: String,
    name: String,
    url: String,
    #[serde(rename = "type")]
    mime: String,
}

fn attachments_of(state: &core::State, ids: &[String]) -> Vec<AttachmentOut> {
    ids.iter()
        .filter_map(|id| state.attachments.iter().find(|a| &a.id == id))
        .map(|a| AttachmentOut { id: a.id.clone(), name: a.name.clone(), url: format!("/api/attachments/{}", a.id), mime: a.mime.clone() })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Finding {
    id: String,
    severity: core::Severity,
    problem: String,
    impact: String,
    evidence: Vec<String>,
    state: FindingState,
    leads_to: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    decided_by: Option<core::Who>,
    /// When it was last decided.
    #[serde(skip_serializing_if = "Option::is_none")]
    decided: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recommendation: Option<core::Suggestion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    delivery: Option<Sent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    attachments: Vec<AttachmentOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    external: Option<core::External>,
    /// While open: "manager" when it is the intent's manager's to decide (and there is one
    /// running); absent: it waits for the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    decider: Option<&'static str>,
    /// While open: it would be the manager's, but none is registered or running, so it
    /// falls to the user.
    #[serde(rename = "noManager", skip_serializing_if = "std::ops::Not::not")]
    no_manager: bool,
    /// What was added to it since it was reported, oldest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    notes: Vec<NoteOut>,
    /// Since when it is known: when its task was delivered, or it was added to the backlog.
    #[serde(skip_serializing_if = "Option::is_none")]
    since: Option<String>,
    /// Dropped: why.
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

/// An addition to a finding.
#[derive(Serialize)]
struct NoteOut {
    by: String,
    at: String,
    text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    attachments: Vec<AttachmentOut>,
}

fn notes_of(state: &core::State, names: &Names, f: &core::Finding) -> Vec<NoteOut> {
    f.notes.iter().map(|n| NoteOut { by: names.of(&n.by), at: n.at.clone(), text: n.text.clone(), attachments: attachments_of(state, &n.attachments) }).collect()
}

/// A deferred finding from outside workriver's tasks, in its project's backlog.
#[derive(Serialize)]
struct BacklogOut {
    project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    intent: Option<String>,
    source: String,
    by: String,
    finding: Finding,
}

#[derive(Serialize)]
struct Commit {
    id: String,
    subject: String,
    date: String,
    author: String,
    live: bool,
    files: Vec<scan::FileChange>,
    /// It belongs to its task because someone linked it (`workriver commit link`), not by
    /// its own `Task:` trailer.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    linked: bool,
    /// The same change as this commit of a branch the unit left (a rebase): its old hash.
    #[serde(rename = "rebasedFrom", skip_serializing_if = "Option::is_none")]
    rebased_from: Option<String>,
    /// A merge commit (the base merged into the branch): its files are not work of its own.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    merge: bool,
}

#[derive(Serialize)]
struct ThreadOut {
    id: String,
    ended: bool,
    /// Its temporary agent runs, so a follow-up keeps its context.
    live: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    conclusion: Option<String>,
    turns: Vec<Turn>,
}

#[derive(Serialize)]
struct Turn {
    id: String,
    /// The agent that asked; absent when the user did.
    #[serde(skip_serializing_if = "Option::is_none")]
    by: Option<String>,
    question: String,
    context: Vec<core::AskContext>,
    answer: Option<String>,
    /// Who answered (the agent's name, or "user") and when.
    #[serde(rename = "answerBy", skip_serializing_if = "Option::is_none")]
    answer_by: Option<String>,
    #[serde(rename = "answeredAt", skip_serializing_if = "Option::is_none")]
    answered_at: Option<String>,
    /// While it comes: starting (the agent), reading (it works on it), blocked (its agent
    /// waits for a person in its terminal: the question waits too).
    #[serde(skip_serializing_if = "Option::is_none")]
    step: Option<&'static str>,
    /// While blocked: the agent that waits, to bring its terminal to the front.
    #[serde(skip_serializing_if = "Option::is_none")]
    blocked: Option<Blocked>,
    #[serde(skip_serializing_if = "Option::is_none")]
    consulted: Option<Consulted>,
    at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    failed: Option<Failed>,
    /// A request for the thread's conclusion.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    conclude: bool,
}

#[derive(Serialize)]
struct Blocked {
    agent: String,
    session: String,
}

#[derive(Serialize)]
struct Consulted {
    agent: String,
    outcome: &'static str,
}

#[derive(Serialize)]
struct Failed {
    code: core::ReasonCode,
    reason: String,
}

/// A task's conversations, from the records, with the runtime's word on their agents.
fn threads(state: &core::State, live: &[RuntimeAgent], names: &Names, unit: &str, n: u32) -> Vec<ThreadOut> {
    state
        .threads
        .iter()
        .filter(|t| t.unit == unit && t.task == n)
        .map(|t| {
            let agent = t.name.as_ref().and_then(|name| live.iter().find(|a| &a.name == name));
            ThreadOut {
                id: t.id.clone(),
                ended: t.ended,
                live: agent.is_some(),
                conclusion: t.conclusion.clone(),
                turns: state
                    .questions
                    .iter()
                    .filter(|q| q.thread == t.id)
                    .map(|q| {
                        // The agent answering it, when the runtime says it waits for a person.
                        let stuck = q.agent.as_ref().and_then(|s| live.iter().find(|a| &a.session == s)).filter(|a| q.open() && a.state == AgentState::Blocked);
                        Turn {
                        id: q.id.clone(),
                        by: (q.by != "user").then(|| names.of(&q.by)),
                        question: q.text.clone(),
                        context: q.context.clone(),
                        answer: q.answer.clone(),
                        // The agent started for it is known by the name it was given.
                        answer_by: q.answered_by.as_ref().map(|a| q.name.clone().filter(|_| q.agent.as_ref() == Some(a)).unwrap_or_else(|| names.of(a))),
                        answered_at: q.answered_at.clone(),
                        step: q.open().then_some(if stuck.is_some() { "blocked" } else if q.agent.is_some() && agent.is_some() { "reading" } else { "starting" }),
                        blocked: stuck.map(|a| Blocked { agent: a.name.clone(), session: a.session.clone() }),
                        consulted: q.asked.as_ref().map(|a| Consulted { agent: names.of(a), outcome: "asked" }),
                        at: q.at.clone(),
                        failed: q.failed.as_ref().map(|(code, reason)| Failed { code: *code, reason: reason.clone() }),
                        conclude: q.conclude,
                        }
                    })
                    .collect(),
            }
        })
        .collect()
}

/// Commits never change and a tree only with its HEAD, so both are kept between snapshots.
#[derive(Default)]
pub struct Cache {
    /// Commits with their files' texts, by hash: they never change.
    // ponytail: unbounded; a commit's texts are kept for the life of the service.
    commits: HashMap<String, scan::Commit>,
    trees: HashMap<String, (String, scan::Module)>,
    /// The machines followed, for repositories that are on one of them.
    pub machines: Option<std::sync::Arc<crate::machines::Machines>>,
    /// What is kept across restarts (in `file`): where branches fork, which commits each
    /// unit had for which tips, what commits say, patch ids.
    stored: Stored,
    file: Option<std::path::PathBuf>,
    dirty: bool,
    /// This build: no file texts, uncommitted work or trees (board, `task show`).
    pub light: bool,
    /// This build: files without their texts, only how many lines changed (the page's
    /// snapshot; `code` gives a unit's texts).
    pub bare: bool,
    /// Whether this cache reads repositories on other machines (the background one does);
    /// one that does not takes what the background read last, from `shared`.
    pub remote_reads: bool,
    pub shared: std::sync::Arc<std::sync::Mutex<Shared>>,
    /// Each repository's refs and work trees, read once per build.
    refs: HashMap<String, HashMap<String, String>>,
    worktrees: HashMap<String, HashMap<String, std::path::PathBuf>>,
    /// Units found merged in git and not yet recorded so: (unit, its commits).
    pub merged_now: Vec<(String, Vec<String>)>,
    /// How long a unit may be ready before it is overdue (the page shows it); none: never.
    pub ready_after: Option<std::time::Duration>,
    /// This build: units whose commits were worked out again, and those reused.
    pub recomputed: usize,
    pub reused: usize,
}

/// The rules by which a unit's commits are worked out (`scan::branch`). Bump it whenever the
/// scan rules change what a unit's commits are: a cache kept under other rules is dropped on
/// load (one rescan), else its old lists and forks would be served on, their keys unchanged.
/// 1: the base's upstream is left out too.
const RULES: u32 = 1;

/// What a cache keeps across restarts.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Stored {
    /// `RULES` when it was written; 0 before there was one.
    #[serde(default)]
    rules: u32,
    #[serde(default)]
    seen: HashMap<String, Seen>,
    #[serde(default)]
    memo: HashMap<String, Memo>,
    #[serde(default)]
    meta: HashMap<String, scan::Named>,
    #[serde(default)]
    patch: HashMap<String, String>,
    /// Whether each ready unit looked squash-merged, under the key of its commits (`Memo`).
    #[serde(default)]
    squash: HashMap<String, (String, Option<Squash>)>,
}

/// A unit that looks squash-merged: its branch's tip when it was seen so, where its change
/// is (its base, or the base's upstream) and the commit there that brought it.
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
pub struct Squash {
    pub tip: String,
    pub into: String,
    pub commit: String,
}

/// A unit's commits for one state of its branch and base (`key`).
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct Memo {
    key: String,
    hashes: Vec<String>,
    merged: bool,
}

/// A unit on another machine as last read: its commits, its uncommitted work, the patch ids
/// of its commits and of those of branches it left.
pub type RemoteUnit = (Vec<scan::Commit>, Vec<scan::FileChange>, HashMap<String, String>);

/// What the background read of other machines found last, for requests to show.
#[derive(Default)]
pub struct Shared {
    pub units: HashMap<String, RemoteUnit>,
    pub trees: HashMap<String, scan::Module>,
    /// Ready units, on any machine, that looked squash-merged when last read.
    pub squash: HashMap<String, Squash>,
}

/// A unit's branch as last read.
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
struct Seen {
    /// The repository, branch and base it was read at; another place starts over.
    place: String,
    fork: Option<String>,
    commits: Vec<scan::Named>,
}

/// How often a repository on another machine is read again.
// ponytail: polled, since files there cannot be watched; a watcher on the machine could tell.
pub const REMOTE: std::time::Duration = std::time::Duration::from_secs(15);

impl Cache {
    pub fn following(machines: Option<std::sync::Arc<crate::machines::Machines>>) -> Cache {
        Cache { machines, ..Default::default() }
    }

    /// What the background read of other machines found goes through `shared`.
    pub fn sharing(mut self, shared: std::sync::Arc<std::sync::Mutex<Shared>>) -> Cache {
        self.shared = shared;
        self
    }

    /// Keeps what it learns in `file`, and starts from what it holds.
    pub fn remembering(mut self, file: std::path::PathBuf) -> Cache {
        let stored: Option<Stored> = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok());
        self.stored = stored.filter(|s| s.rules == RULES).unwrap_or_default();
        self.stored.rules = RULES;
        self.file = Some(file);
        self
    }

    /// Writes what was learnt since the last time.
    pub fn save(&mut self) {
        let Some(file) = self.file.as_ref().filter(|_| self.dirty) else { return };
        let written = file.parent().map(std::fs::create_dir_all).unwrap_or(Ok(())).and_then(|_| serde_json::to_vec(&self.stored).map_err(std::io::Error::other)).and_then(|b| {
            let tmp = file.with_extension("tmp");
            std::fs::write(&tmp, b).and_then(|_| std::fs::rename(&tmp, file))
        });
        match written {
            Ok(()) => self.dirty = false,
            Err(e) => tracing::warn!("cannot write {}: {e}", file.display()),
        }
    }

    /// Where git runs for a repository: None while its machine cannot be reached.
    fn runner(&self, machine: Option<&str>) -> Option<scan::Runner> {
        match machine {
            None => Some(scan::Runner::Local),
            Some(m) => self.machines.as_ref().and_then(|x| x.runner(m)),
        }
    }

    /// A repository's refs, read once per build.
    fn refs(&mut self, at: &scan::Runner, u: &core::Unit) -> Option<HashMap<String, String>> {
        let key = format!("{}\0{}", u.machine.as_deref().unwrap_or_default(), u.repo);
        if !self.refs.contains_key(&key) {
            let r = scan::refs(at, Path::new(&u.repo)).ok()?;
            self.refs.insert(key.clone(), r);
        }
        self.refs.get(&key).cloned()
    }
}

/// The id of the live commit, as in the UI's data.
const LIVE: &str = "uncommitted";

/// Names to show for agent sessions: as last seen in the records, then as live now.
struct Names(HashMap<String, String>);

impl Names {
    fn of(&self, session: &str) -> String {
        self.0.get(session).cloned().unwrap_or_else(|| session.to_string())
    }

    fn sent(&self, d: &Option<core::Delivery>, manager: Option<&str>) -> Option<Sent> {
        d.as_ref().map(|d| Sent {
            step: d.step,
            to: d.to.as_deref().map(|s| self.of(s)),
            to_manager: d.to.is_some() && d.to.as_deref() == manager,
            reason_code: d.reason_code,
            reason: d.reason.clone(),
            message: d.message.clone(),
        })
    }
}

/// `blocked`: when each blocked live agent (by session) was first seen blocked; `idle`: idle
/// (or done) the same way; `stalls`: the
/// running tasks whose agent stopped, by (unit or intent, task).
pub fn build(state: &core::State, cache: &mut Cache, live: Option<&[RuntimeAgent]>, blocked: &HashMap<String, String>, idle: &HashMap<String, String>, stalls: &HashMap<(String, u32), crate::stall::Stall>, groups: &[core::RuntimeRef]) -> Workspace {
    let live = live.unwrap_or_default();
    cache.refs.clear();
    cache.worktrees.clear();
    (cache.recomputed, cache.reused) = (0, 0);
    let machines = cache.machines.as_ref().map(|m| m.states()).unwrap_or_default();
    let names = Names(state.agents.iter().map(|a| (a.session.clone(), a.name.clone())).chain(live.iter().map(|a| (a.session.clone(), a.name.clone()))).collect());
    let repos: BTreeSet<(Option<&str>, &str)> = state.units.iter().filter(|u| u.has_code()).map(|u| (u.machine.as_deref(), u.repo.as_str())).collect();
    let repos = repos.into_iter().map(|(m, r)| repo(m, r, cache)).collect();
    let repos: Vec<Repo> = repos;
    let mut intents = state
        .intents
        .iter()
        .map(|i| Intent {
            id: i.id.clone(),
            title: i.title.clone(),
            goal: i.goal.clone(),
            state: i.state,
            findings: i.findings,
            project: project_folder(state, i),
            note: i.note.clone(),
            external: i.external.clone(),
            workspace: i.workspace.clone(),
            order: (i.state == core::IntentState::Planned).then(|| {
                let mine = project_folder(state, i);
                state.roadmap.iter().filter_map(|id| state.intents.iter().find(|x| &x.id == id)).filter(|x| project_folder(state, x) == mine).position(|x| x.id == i.id)
            }).flatten(),
            reason: i.reason.clone(),
            units: intent_unit(state, live, i, &names)
                .into_iter()
                .chain(state.units.iter().filter(|u| u.intent == i.id).map(|u| if u.kind.is_code() { unit(state, live, u, cache, &names, state.manager_of(&i.id)) } else { branchless(state, live, u, &names, false) }))
                .collect(),
            chores: state
                .chores
                .iter()
                .filter(|c| c.intent == i.id)
                .map(|c| chore(c, &names, None))
                .collect(),
            manager: state.manager_of(&i.id).map(|m| names.of(m)),
            manager_state: state.manager_of(&i.id).map(|m| live.iter().find(|a| a.session == m).map(|a| name(&a.state)).unwrap_or_else(|| "gone".into())),
            complete: i.state == core::IntentState::Active && state.finished(&i.id),
        })
        .collect::<Vec<Intent>>();
    for u in intents.iter_mut().flat_map(|i| i.units.iter_mut()) {
        for t in &mut u.tasks {
            t.stalled = t.id.parse().ok().and_then(|n| stalls.get(&(u.id.clone(), n))).cloned();
        }
    }
    // Live agents, then those the records name that are gone.
    let mut sessions: Vec<Session> = live
        .iter()
        .map(|a| {
            let intent = a.group.as_ref().and_then(|g| state.intent_for(g));
            let manages = a.group.as_ref().and_then(|g| state.workspace_of(g)).and_then(|w| w.manager.as_deref()) == Some(a.session.as_str());
            let away = a.machine.as_ref().is_some_and(|m| machines.iter().any(|x| &x.label == m && !x.reachable));
            // A linked workspace of another machine is shown as the workspace of this machine.
            let group = a.group.as_ref().map(|g| canon(state, groups, g));
            Session {
                machine: a.machine.clone(),
                unreachable: away,
                agent: a.name.clone(),
                session: a.session.clone(),
                topic: a.topic.clone(),
                // An agent that cannot be seen now is not known to wait.
                blocked_since: (a.state == AgentState::Blocked && !away).then(|| blocked.get(&a.session).cloned().unwrap_or_else(core::now)),
                idle_since: idle.get(&a.session).filter(|_| !away && matches!(a.state, AgentState::Idle | AgentState::Done)).cloned(),
                harness: (!a.harness.is_empty()).then(|| a.harness.clone()),
                group,
                ended: false,
                state: Some(if away { "unknown".into() } else { name(&a.state) }),
                intent: intent.map(|i| i.id.clone()),
                manager: manages,
            }
        })
        .collect();
    let recorded = state.agents.iter().map(|a| a.session.clone()).chain(state.units.iter().flat_map(|u| u.tasks.iter().filter_map(|t| t.agent.clone())));
    for session in recorded {
        let agent = names.of(&session);
        if !live.iter().any(|a| a.session == session) && !sessions.iter().any(|s| s.agent == agent) {
            let harness = state.agents.iter().find(|a| a.session == session).map(|a| a.harness.clone()).filter(|h| !h.is_empty());
            sessions.push(Session { agent, session, topic: None, blocked_since: None, idle_since: None, harness, group: None, ended: true, state: None, intent: None, manager: false, machine: None, unreachable: false });
        }
    }
    let backlog = state
        .backlog
        .iter()
        .map(|b| {
            let f = &b.finding;
            BacklogOut {
                project: crate::serve::project_of(state, &b.project).map(|p| p.path.clone()).unwrap_or_else(|| b.project.clone()),
                intent: b.intent.clone(),
                source: b.source.clone(),
                by: names.of(&b.by),
                finding: Finding {
                    id: f.found.id.clone(),
                    severity: f.found.severity,
                    problem: f.found.problem.clone(),
                    impact: f.found.impact.clone(),
                    evidence: f.found.r#where.clone(),
                    state: f.state,
                    leads_to: vec![],
                    decided_by: None,
                    decided: None,
                    recommendation: None,
                    note: f.note.clone(),
                    delivery: names.sent(&f.delivery, b.intent.as_deref().and_then(|i| state.manager_of(i))),
                    attachments: attachments_of(state, &f.found.attachments),
                    external: f.found.external.clone(),
                    since: Some(b.at.clone()),
                    reason: f.reason.clone(),
                    decider: None,
                    no_manager: false,
                    notes: vec![],
                },
            }
        })
        .collect();
    let workspaces = groups
        .iter()
        .map(|g| {
            let (project, from) = group_project(state, live, g).unzip();
            Group { id: g.id.clone(), label: g.label.clone(), machine: g.machine.clone(), linked_to: state.link_of(g).map(|l| l.local.clone()), project_title: project.as_deref().map(|p| project_title(state, p)), project, project_from: from }
        })
        .collect();
    let chores = state
        .chores
        .iter()
        .filter(|c| c.intent.is_empty())
        .map(|c| chore(c, &names, Some(crate::serve::project_of(state, &c.project).map(|p| p.path.clone()).unwrap_or_else(|| c.project.clone()))))
        .collect();
    let messages = messages(state, live, &names);
    Workspace { repos, projects: projects(state), backlog, built_at: core::now(), intents, sessions, workspaces, machines, reduced: false, chores, alerts: vec![], messages, runs: vec![] }
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

/// The project (its folder) a runtime group works on, and how that is known: what the user
/// set; else where its manager runs; else its current intent's project. None: unknown.
pub fn group_project(state: &core::State, live: &[RuntimeAgent], g: &core::RuntimeRef) -> Option<(String, Known)> {
    let w = state.workspace_of(g);
    if let Some(p) = w.and_then(|w| w.project.clone()) {
        return Some((p, Known::User));
    }
    let manager = w.and_then(|w| w.manager.as_deref()).and_then(|m| live.iter().find(|a| a.session == m));
    if let Some(p) = manager.and_then(|a| project_at(state, a.machine.as_deref(), &a.cwd)) {
        return Some((p, Known::Manager));
    }
    state.intent_for(g).map(|i| project_folder(state, i)).filter(|p| !p.is_empty()).map(|p| (p, Known::Intents))
}

/// The project a folder is in. Here: the deepest registered project holding it, else the
/// repository it is in (a project of its own; a linked worktree counts as its main
/// repository). On another machine its paths are its own: only the repositories of units
/// recorded on that machine, as their intents' projects.
fn project_at(state: &core::State, machine: Option<&str>, cwd: &str) -> Option<String> {
    if cwd.is_empty() {
        return None;
    }
    let cwd = Path::new(cwd);
    if let Some(m) = machine {
        let u = state.units.iter().filter(|u| u.machine.as_deref() == Some(m) && u.has_code() && cwd.starts_with(&u.repo)).max_by_key(|u| u.repo.len())?;
        let i = state.intents.iter().find(|i| i.id == u.intent)?;
        return Some(project_folder(state, i)).filter(|p| !p.is_empty());
    }
    if let Some(p) = crate::serve::project_of(state, &cwd.to_string_lossy()) {
        return Some(p.path.clone());
    }
    let top = cwd.ancestors().find(|d| d.join(".git").exists())?;
    // A linked worktree's `.git` is a file: `gitdir: <main>/.git/worktrees/<name>`.
    let main = std::fs::read_to_string(top.join(".git")).ok().and_then(|g| g.trim().strip_prefix("gitdir:").and_then(|d| d.trim().split_once("/.git/worktrees/")).map(|(m, _)| m.to_string()));
    let top = main.unwrap_or_else(|| top.to_string_lossy().into_owned());
    Some(crate::serve::project_of(state, &top).map(|p| p.path.clone()).unwrap_or(top))
}

/// A project's title: a registered one's, else its folder's name.
pub(crate) fn project_title(state: &core::State, folder: &str) -> String {
    match state.projects.iter().find(|p| p.path == folder) {
        Some(p) => p.title.clone(),
        None => Path::new(folder).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| folder.to_string()),
    }
}

/// When `intent` belongs to a known project and the group is known to work on another: the
/// two, in a sentence. None when they agree or either is unknown.
pub fn other_project(state: &core::State, live: &[RuntimeAgent], g: &core::RuntimeRef, intent: &str) -> Option<String> {
    let i = state.intents.iter().find(|i| i.id == intent)?;
    let mine = project_folder(state, i);
    let (theirs, known) = group_project(state, live, g)?;
    if mine.is_empty() || mine == theirs {
        return None;
    }
    let how = match known {
        Known::Manager => "its manager runs in that project's folder",
        Known::Intents => "its current intent belongs to it",
        Known::User => "the user set it",
    };
    Some(format!(
        "workspace {} works on project {} ({theirs}; {how}), but {intent} belongs to project {} ({mine})",
        quote::name(&g.label),
        quote::name(&project_title(state, &theirs)),
        quote::name(&project_title(state, &mine))
    ))
}

/// An intent's project folder: as recorded, else where its first unit's repository is.
pub(crate) fn project_folder(state: &core::State, i: &core::Intent) -> String {
    let path = if i.project.is_empty() { state.units.iter().find(|u| u.intent == i.id).map(|u| u.repo.clone()).unwrap_or_default() } else { i.project.clone() };
    if path.is_empty() {
        return path;
    }
    crate::serve::project_of(state, &path).map(|p| p.path.clone()).unwrap_or(path)
}

/// Registered projects with the repositories in them, and one implicit project per
/// repository that intents use outside all of them.
fn projects(state: &core::State) -> Vec<ProjectOut> {
    let repos_in = |folder: &str| {
        let f = Path::new(folder);
        if f.join(".git").exists() {
            return vec![folder.to_string()];
        }
        let mut list: Vec<String> = std::fs::read_dir(f).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.join(".git").exists()).map(|p| p.to_string_lossy().into_owned()).collect();
        list.sort();
        list
    };
    let mut out: Vec<ProjectOut> = state.projects.iter().map(|p| ProjectOut { id: p.path.clone(), key: Some(p.id.clone()), title: p.title.clone(), registered: true, repos: repos_in(&p.path) }).collect();
    let folders = state.intents.iter().map(|i| project_folder(state, i)).chain(state.chores.iter().filter(|c| c.intent.is_empty()).map(|c| crate::serve::project_of(state, &c.project).map(|p| p.path.clone()).unwrap_or_else(|| c.project.clone()))).chain(state.backlog.iter().map(|b| crate::serve::project_of(state, &b.project).map(|p| p.path.clone()).unwrap_or_else(|| b.project.clone())));
    for folder in folders {
        if !folder.is_empty() && !out.iter().any(|p| p.id == folder) {
            let title = Path::new(&folder).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| folder.clone());
            out.push(ProjectOut { id: folder.clone(), key: None, title, registered: false, repos: vec![folder] });
        }
    }
    out
}

/// An intent's own tasks (research, design and their reviews and gates, recorded before
/// research units) as a unit without a repository, listed first under the intent; none when
/// it has none.
fn intent_unit(state: &core::State, live: &[RuntimeAgent], i: &core::Intent, names: &Names) -> Option<Unit> {
    if i.tasks.is_empty() {
        return None;
    }
    let u = state.unit_or_intent(&i.id)?;
    Some(branchless(state, live, &u, names, true))
}

/// A unit with no branch: a research unit, or (`intent_level`) an intent's own work. No
/// commits, no code, nothing to merge.
fn branchless(state: &core::State, live: &[RuntimeAgent], u: &core::Unit, names: &Names, intent_level: bool) -> Unit {
    let manager = state.manager_of(&u.intent);
    Unit {
        id: u.id.clone(),
        intent_level,
        kind: (!u.kind.is_code()).then_some(u.kind),
        title: u.title.clone(),
        purpose: u.purpose.clone(),
        repo: String::new(),
        machine: None,
        branch: None,
        base: None,
        merged: None,
        previously: vec![],
        status: if u.tasks.iter().any(|t| t.state != TaskState::Todo) { "active" } else { "planned" },
        progress: state.progress(u),
        done_reason: u.done.as_ref().map(|d| d.reason.clone()),
        tasks: u.tasks.iter().map(|t| task(state, live, u, t, names, manager, vec![])).collect(),
        commits: vec![],
        code: String::new(),
        squash: None,
        overdue: None,
        threads: if intent_level { vec![] } else { threads(state, live, names, &u.id, 0) },
    }
}

fn repo(machine: Option<&str>, path: &str, cache: &mut Cache) -> Repo {
    let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let p = Path::new(path);
    let key = match machine {
        Some(m) => format!("{m}\0{path}"),
        None => path.to_string(),
    };
    let empty = || scan::Module { name: name.clone(), children: Some(vec![]), ..Default::default() };
    let modules = match cache.runner(machine) {
        // Not wanted (a light build), or on another machine and read in the background.
        _ if cache.light => empty(),
        _ if machine.is_some() && !cache.remote_reads => cache.shared.lock().unwrap().trees.get(&key).cloned().unwrap_or_else(empty),
        None => cache.trees.get(&key).map(|(_, m)| m.clone()).unwrap_or_else(empty),
        Some(at) => {
            let head = scan::head(&at, p).unwrap_or_default();
            match cache.trees.get(&key) {
                Some((h, m)) if *h == head => m.clone(),
                _ => match scan::tree(&at, p) {
                    Ok(m) => {
                        cache.trees.insert(key.clone(), (head, m.clone()));
                        if machine.is_some() {
                            cache.shared.lock().unwrap().trees.insert(key, m.clone());
                        }
                        m
                    }
                    Err(e) => {
                        tracing::warn!("{e}");
                        empty()
                    }
                },
            }
        }
    };
    Repo { path: path.to_string(), machine: machine.map(str::to_string), name, modules }
}

/// A unit's commits and its uncommitted files, read from git where its repository is.
/// A unit's commits now (oldest first), by the rules of `scan::branch`, and whether git
/// shows it merged. What the branch held is kept, for when it is merged or gone.
pub fn unit_commits(at: &scan::Runner, u: &core::Unit, cache: &mut Cache) -> scan::Result<(Vec<scan::Named>, bool)> {
    let place = format!("{}|{}|{}", u.repo, u.branch, u.base);
    let seen = cache.stored.seen.get(&u.id).filter(|s| s.place == place).cloned();
    let fork = seen.as_ref().and_then(|s| s.fork.clone());
    let ours = |c: &scan::Named| c.task.as_deref().and_then(parse_task_ref).is_some_and(|(x, _)| x == u.id) || u.links.iter().any(|l| l.hash == c.hash);
    let (commits, fork, merged) = match scan::branch(at, Path::new(&u.repo), &u.base, &u.branch, fork.as_deref(), &ours)? {
        scan::Found::Ahead { commits, fork } => (commits, Some(fork), false),
        scan::Found::Merged(commits) => (commits, fork, true),
        scan::Found::Fresh => (vec![], fork, false),
        scan::Found::Gone => return Ok((seen.map(|s| s.commits).unwrap_or_default(), false)),
    };
    let now = Seen { place, fork, commits: commits.clone() };
    if seen.as_ref() != Some(&now) {
        cache.stored.seen.insert(u.id.clone(), now);
        cache.dirty = true;
    }
    Ok((commits, merged))
}

/// Which commits are the unit's: as worked out last time while its branch, its base and
/// what is recorded about it stay as they were (one `git for-each-ref` per repository tells);
/// else worked out again (`unit_commits`).
fn unit_hashes(at: &scan::Runner, u: &core::Unit, cache: &mut Cache) -> scan::Result<(Vec<String>, bool)> {
    if u.merged.is_some() && !u.merged_commits.is_empty() {
        return Ok((u.merged_commits.clone(), false));
    }
    let fork = cache.stored.seen.get(&u.id).and_then(|s| s.fork.clone()).unwrap_or_default();
    let key = cache.refs(at, u).and_then(|refs| {
        let base = scan::tip(&refs, &u.base)?.to_string();
        let tip = scan::tip(&refs, &u.branch).unwrap_or("-").to_string();
        // The base's remote copies too: a fetch can merge the branch upstream (`scan::since`).
        let mut up: Vec<&str> = refs.iter().filter(|(r, _)| r.strip_prefix("refs/remotes/").and_then(|r| r.split_once('/')).is_some_and(|(_, b)| b == u.base)).map(|(_, h)| h.as_str()).collect();
        up.sort();
        Some(format!("{}|{}={tip}|{}={base}|{fork}|{}|{}|{}|{}", u.repo, u.branch, u.base, u.earlier.len(), u.links.len(), u.merged_commits.len(), up.join(",")))
    });
    if let Some(m) = cache.stored.memo.get(&u.id).filter(|m| key.as_ref() == Some(&m.key)) {
        cache.reused += 1;
        return Ok((m.hashes.clone(), m.merged));
    }
    cache.recomputed += 1;
    let (list, merged) = unit_commits(at, u, cache)?;
    // Under the fork as now known: what the next read finds unchanged.
    let fork = cache.stored.seen.get(&u.id).and_then(|s| s.fork.clone()).unwrap_or_default();
    let key = key.map(|k| {
        let mut parts: Vec<&str> = k.split('|').collect();
        parts[3] = &fork;
        parts.join("|")
    });
    let hashes: Vec<String> = list.iter().map(|c| c.hash.clone()).collect();
    for c in list {
        cache.stored.meta.insert(c.hash.clone(), c);
    }
    if let Some(key) = key {
        cache.stored.memo.insert(u.id.clone(), Memo { key, hashes: hashes.clone(), merged });
    }
    cache.dirty = true;
    Ok((hashes, merged))
}

/// The background read of repositories on other machines: each unit's commits and work,
/// and each repository's tree, into `shared`. Whether anything changed, and whether a
/// machine could not be read (then it is tried again soon).
pub fn refresh_remote(state: &core::State, cache: &mut Cache) -> (bool, bool) {
    cache.refs.clear();
    cache.worktrees.clear();
    (cache.recomputed, cache.reused) = (0, 0);
    let (mut changed, mut skipped) = (false, false);
    for u in state.units.iter().filter(|u| u.machine.is_some() && u.has_code()) {
        let Some(at) = cache.runner(u.machine.as_deref()) else {
            skipped = true;
            // Not reachable (yet): what was read before the last restart, without texts.
            let known = cache.stored.memo.get(&u.id).map(|m| m.hashes.clone()).unwrap_or_else(|| u.merged_commits.clone());
            let mut shared = cache.shared.lock().unwrap();
            if !shared.units.contains_key(&u.id) && !known.is_empty() {
                let commits = known.iter().filter_map(|h| cache.stored.meta.get(h).map(|n| scan::Commit { id: h.chars().take(9).collect(), hash: h.clone(), subject: n.subject.clone(), date: n.date.clone(), author: n.author.clone(), task: n.task.clone(), merge: false, files: vec![] })).collect();
                let patch = cache.stored.patch.iter().filter(|(h, _)| known.contains(h)).map(|(h, p)| (h.clone(), p.clone())).collect();
                shared.units.insert(u.id.clone(), (commits, vec![], patch));
                changed = true;
            }
            continue;
        };
        let (c, f) = read_unit(&at, u, cache, state.progress(u) == core::Progress::Ready);
        let p: HashMap<String, String> = c.iter().map(|x| &x.hash).chain(u.earlier.iter().flat_map(|e| &e.commits).map(|x| &x.hash)).filter_map(|h| cache.stored.patch.get(h).map(|p| (h.clone(), p.clone()))).collect();
        let mut shared = cache.shared.lock().unwrap();
        let same = shared.units.get(&u.id).is_some_and(|(oc, of, op)| oc.iter().map(|x| &x.hash).eq(c.iter().map(|x| &x.hash)) && *of == f && *op == p);
        if !same {
            shared.units.insert(u.id.clone(), (c, f, p));
            changed = true;
        }
    }
    let repos: BTreeSet<(&str, &str)> = state.units.iter().filter(|u| u.has_code()).filter_map(|u| Some((u.machine.as_deref()?, u.repo.as_str()))).collect();
    for (m, r) in repos {
        let key = format!("{m}\0{r}");
        let before = cache.trees.get(&key).map(|(h, _)| h.clone());
        repo(Some(m), r, cache);
        changed |= cache.trees.get(&key).map(|(h, _)| h.clone()) != before;
    }
    (changed, skipped)
}

/// A unit's commits and its uncommitted work, read from git where its repository is: the
/// commits' texts only for those not read before, all in one go; none in a light build.
/// `ready`: the unit is ready to merge, so whether it looks squash-merged is looked at too.
fn read_unit(at: &scan::Runner, u: &core::Unit, cache: &mut Cache, ready: bool) -> (Vec<scan::Commit>, Vec<scan::FileChange>) {
    let started = std::time::Instant::now();
    let calls = scan::CALLS.load(std::sync::atomic::Ordering::Relaxed);
    let repo = Path::new(&u.repo);
    let hashes = match unit_hashes(at, u, cache) {
        Ok((hashes, merged)) => {
            // A spike is never merged: neither recorded so nor looked at for a squash.
            if merged && u.kind.merges() && u.merged.is_none() && !cache.merged_now.iter().any(|(x, _)| x == &u.id) {
                cache.merged_now.push((u.id.clone(), hashes.clone()));
            }
            if ready && u.kind.merges() && !merged && u.merged.is_none() {
                squash_check(at, u, cache);
            }
            hashes
        }
        Err(e) => {
            tracing::warn!("unit {}: {e}", u.id);
            vec![]
        }
    };
    // Light: what each commit says is enough. Else their texts, those not read yet at once.
    let named = |h: &String, cache: &Cache| cache.stored.meta.get(h).map(|n| scan::Commit { id: h.chars().take(9).collect(), hash: h.clone(), subject: n.subject.clone(), date: n.date.clone(), author: n.author.clone(), task: n.task.clone(), merge: false, files: vec![] });
    let missing: Vec<String> = hashes.iter().filter(|h| !cache.commits.contains_key(*h) && (!cache.light || !cache.stored.meta.contains_key(*h))).cloned().collect();
    if !missing.is_empty() {
        match scan::commits(at, repo, &missing) {
            Ok(read) => {
                for c in read {
                    cache.stored.meta.entry(c.hash.clone()).or_insert_with(|| scan::Named { hash: c.hash.clone(), subject: c.subject.clone(), author: c.author.clone(), date: c.date.clone(), task: c.task.clone() });
                    cache.commits.insert(c.hash.clone(), c);
                }
                cache.dirty = true;
            }
            Err(e) => tracing::warn!("unit {}: {e}", u.id),
        }
    }
    // Light and bare: which files, not their texts (they are what makes a snapshot many
    // megabytes); not even copied.
    let bare = cache.light || cache.bare;
    let commits: Vec<scan::Commit> = hashes.iter().filter_map(|h| cache.commits.get(h).map(|c| if bare { bare_commit(c) } else { c.clone() }).or_else(|| named(h, cache))).collect();
    // Links whose commit is not here any more, and commits of a branch the unit left (a
    // rebase gave them other hashes): told by patch ids, worked out once each (none: "").
    let stale = u.links.iter().any(|l| !commits.iter().any(|c| c.hash == l.hash));
    if stale || !u.earlier.is_empty() {
        let old = u.earlier.iter().flat_map(|e| &e.commits).filter(|c| c.patch_id.is_empty()).map(|c| &c.hash);
        let want: Vec<String> = commits.iter().map(|c| &c.hash).chain(old).filter(|h| !cache.stored.patch.contains_key(*h)).cloned().collect::<BTreeSet<_>>().into_iter().collect();
        if !want.is_empty() {
            let found = scan::patch_ids(at, repo, &want).unwrap_or_default();
            for h in want {
                let id = found.get(&h).cloned().unwrap_or_default();
                cache.stored.patch.insert(h, id);
            }
            cache.dirty = true;
        }
    }
    let files = if u.merged.is_some() || cache.light {
        vec![]
    } else {
        let key = format!("{}\0{}", u.machine.as_deref().unwrap_or_default(), u.repo);
        if !cache.worktrees.contains_key(&key) {
            let w = scan::worktrees(at, repo).unwrap_or_default();
            cache.worktrees.insert(key.clone(), w);
        }
        match cache.worktrees.get(&key).and_then(|w| w.get(&u.branch)).cloned() {
            None => vec![],
            Some(dir) => scan::live_in(at, &dir).unwrap_or_else(|e| {
                tracing::warn!("unit {}: {e}", u.id);
                vec![]
            }),
        }
    };
    let ms = started.elapsed().as_millis() as u64;
    tracing::info!(target: "perf", kind = "unit", unit = %u.id, machine = u.machine.as_deref().unwrap_or("local"), ms, git = scan::CALLS.load(std::sync::atomic::Ordering::Relaxed) - calls, commits = commits.len(), light = cache.light, slow = ms >= SLOW_MS);
    (commits, files)
}

/// Whether a ready unit git does not show merged looks squash-merged (`scan::squashed`):
/// worked out once per state of its branch and base (the key of its commits), and kept in
/// `Shared::squash`, where the page and the service's reminder read it.
fn squash_check(at: &scan::Runner, u: &core::Unit, cache: &mut Cache) {
    let Some(key) = cache.stored.memo.get(&u.id).map(|m| m.key.clone()) else { return };
    let found = match cache.stored.squash.get(&u.id) {
        Some((k, f)) if *k == key => f.clone(),
        _ => {
            // Its branch gone, or where its commits start not known: nothing to compare.
            let tip = cache.refs(at, u).and_then(|r| scan::tip(&r, &u.branch).map(str::to_string));
            let fork = cache.stored.seen.get(&u.id).and_then(|s| s.fork.clone());
            let (Some(tip), Some(fork)) = (tip, fork) else { return };
            let found = match scan::squashed(at, Path::new(&u.repo), &u.base, &u.branch, &fork) {
                Ok(f) => f.map(|(into, commit)| Squash { tip, into, commit }),
                Err(e) => {
                    tracing::warn!("unit {}: {e}", u.id);
                    return;
                }
            };
            cache.stored.squash.insert(u.id.clone(), (key, found.clone()));
            cache.dirty = true;
            found
        }
    };
    let mut shared = cache.shared.lock().unwrap();
    match found {
        Some(s) => shared.squash.insert(u.id.clone(), s),
        None => shared.squash.remove(&u.id),
    };
}

fn bare_commit(c: &scan::Commit) -> scan::Commit {
    scan::Commit { id: c.id.clone(), hash: c.hash.clone(), subject: c.subject.clone(), date: c.date.clone(), author: c.author.clone(), task: c.task.clone(), merge: c.merge, files: c.files.iter().map(scan::FileChange::bare).collect() }
}

/// Which texts a unit's commits and uncommitted files have: its commits' hashes (a commit
/// never changes) and its uncommitted texts.
fn code_of(commits: &[scan::Commit], files: &[scan::FileChange]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for c in commits {
        c.hash.hash(&mut h);
    }
    for f in files {
        (&f.path, &f.status, &f.before, &f.after).hash(&mut h);
    }
    format!("{:016x}", h.finish())
}

/// Anything slower than this is marked so in the timing log.
pub const SLOW_MS: u64 = 500;

fn unit(state: &core::State, live: &[RuntimeAgent], u: &core::Unit, cache: &mut Cache, names: &Names, manager: Option<&str>) -> Unit {
    let merged = u.merged.is_some();
    // On another machine: as the background read it last (it never waits on ssh here); what
    // was read stays while the machine cannot be reached.
    let (commits, files, patch) = if u.machine.is_some() && !cache.remote_reads {
        let shared = cache.shared.lock().unwrap();
        let (c, f, p) = shared.units.get(&u.id).cloned().unwrap_or_default();
        (c, if cache.light { vec![] } else { f }, p)
    } else {
        match cache.runner(u.machine.as_deref()) {
            None => Default::default(),
            Some(at) => {
                let (c, f) = read_unit(&at, u, cache, state.progress(u) == core::Progress::Ready);
                let p: HashMap<String, String> = c.iter().map(|x| &x.hash).chain(u.earlier.iter().flat_map(|e| &e.commits).map(|x| &x.hash)).filter_map(|h| cache.stored.patch.get(h).map(|p| (h.clone(), p.clone()))).collect();
                if u.machine.is_some() {
                    cache.shared.lock().unwrap().units.insert(u.id.clone(), (c.clone(), f.clone(), p.clone()));
                }
                (c, f, p)
            }
        }
    };
    let code = code_of(&commits, &files);
    let (commits, files) = if cache.light || cache.bare { (commits.iter().map(bare_commit).collect(), files.iter().map(scan::FileChange::bare).collect()) } else { (commits, files) };

    // A commit linked to a task by hand: by its hash; or, for a link whose commit is gone (a
    // rebase), by the same patch id, else by the same subject and author date.
    let here = |hash: &str| commits.iter().any(|c| c.hash == hash);
    let link = |c: &scan::Commit| {
        let stale = || u.links.iter().filter(|l| !here(&l.hash));
        u.links
            .iter()
            .find(|l| l.hash == c.hash)
            .or_else(|| patch.get(&c.hash).filter(|p| !p.is_empty()).and_then(|p| stale().find(|l| &l.patch_id == p)))
            .or_else(|| stale().find(|l| !l.subject.is_empty() && l.subject == c.subject && l.date == c.date))
            .filter(|l| u.tasks.iter().any(|t| t.n == l.task))
    };
    // Which task each commit carries out: a link wins over what its message says; a commit
    // naming no task of this unit is a gap.
    let owner = |c: &scan::Commit| {
        link(c).map(|l| l.task).or_else(|| c.task.as_deref().and_then(parse_task_ref).filter(|(unit, n)| *unit == u.id && u.tasks.iter().any(|t| t.n == *n)).map(|(_, n)| n))
    };
    let mut tasks: Vec<Task> = u.tasks.iter().map(|t| task(state, live, u, t, names, manager, commits.iter().filter(|c| owner(c) == Some(t.n)).map(|c| c.id.clone()).collect())).collect();
    // Commits of branches the unit left, each once: found again here (the same hash, the same
    // patch id, or the same subject and author date) or not on the current branch.
    let (again, gone) = earlier_commits(u, &commits, &patch);
    for t in &mut tasks {
        let n: Option<u32> = t.id.parse().ok();
        t.lost_commits = gone.iter().filter(|c| c.task.is_some() && c.task == n).map(|c| c.hash.chars().take(9).collect()).collect();
    }
    for c in commits.iter().filter(|c| owner(c).is_none()) {
        tasks.push(unregistered(c));
    }

    let linked: Vec<bool> = commits.iter().map(|c| link(c).is_some()).collect();
    let mut shown: Vec<Commit> = commits
        .into_iter()
        .zip(linked)
        .map(|(c, linked)| {
            let rebased_from = again.get(&c.hash).cloned();
            Commit { id: c.id, subject: c.subject, date: c.date, author: c.author, live: false, merge: c.merge, files: c.files, linked, rebased_from }
        })
        .collect();
    // Uncommitted work belongs to the running implement task started last.
    // ponytail: a guess when several run in one unit; ask the runtime whose cwd it is (stage 2).
    let writer = u.tasks.iter().filter(|t| t.kind == TaskKind::Implement && t.state == TaskState::Running).max_by_key(|t| &t.started).map(|t| t.n.to_string());
    {
        if !files.is_empty() {
            let at = tasks.iter_mut().find(|t| Some(&t.id) == writer.as_ref());
            let author = at.as_ref().and_then(|t| t.agent.clone()).unwrap_or_default();
            if let Some(t) = at {
                t.writing = true;
                t.output.commits.get_or_insert_with(Vec::new).push(LIVE.into());
            }
            shown.push(Commit { id: LIVE.into(), subject: String::new(), date: core::now(), author, live: true, files, linked: false, rebased_from: None, merge: false });
        }
    }

    let started = u.tasks.iter().any(|t| t.state != TaskState::Todo && t.kind != TaskKind::Gate) || !shown.is_empty();
    // Found merged in git: so already, before the record of it lands.
    let progress_now = if !merged && cache.merged_now.iter().any(|(x, _)| x == &u.id) { core::Progress::Merged } else { state.progress(u) };
    Unit {
        id: u.id.clone(),
        intent_level: false,
        kind: (!u.kind.merges()).then_some(u.kind),
        title: u.title.clone(),
        purpose: u.purpose.clone(),
        repo: u.repo.clone(),
        machine: u.machine.clone(),
        branch: (!merged).then(|| u.branch.clone()),
        base: (!u.base.is_empty()).then(|| u.base.clone()),
        merged: u.merged.clone(),
        previously: {
            let mut b: Vec<String> = vec![];
            for e in u.earlier.iter().filter(|e| e.branch != u.branch) {
                if !b.contains(&e.branch) {
                    b.push(e.branch.clone());
                }
            }
            b
        },
        status: if merged { "merged" } else if started { "active" } else { "planned" },
        progress: progress_now,
        done_reason: u.done.as_ref().map(|d| d.reason.clone()),
        tasks,
        commits: shown,
        code,
        squash: (progress_now == core::Progress::Ready).then(|| cache.shared.lock().unwrap().squash.get(&u.id).cloned()).flatten(),
        // Waiting for a merge too long; a spike waits for none.
        overdue: cache.ready_after.filter(|_| progress_now == core::Progress::Ready && u.kind.merges()).and_then(|after| {
            let since = state.ready_since(u)?;
            let ago = humantime::parse_rfc3339(&since).ok()?.elapsed().ok()?;
            (ago >= after).then_some(since)
        }),
        threads: threads(state, live, names, &u.id, 0),
    }
}

/// The commits of branches `u` left, each once, against `now`: those found again here, as
/// new hash → old short hash (a rebase), and those that are not here.
fn earlier_commits(u: &core::Unit, now: &[scan::Commit], patch: &HashMap<String, String>) -> (HashMap<String, String>, Vec<core::SeenCommit>) {
    let mut again = HashMap::new();
    let mut gone: Vec<core::SeenCommit> = vec![];
    let mut seen = BTreeSet::new();
    for old in u.earlier.iter().flat_map(|e| &e.commits) {
        if !seen.insert(old.hash.clone()) {
            continue;
        }
        if now.iter().any(|c| c.hash == old.hash) {
            continue;
        }
        let old_patch = if old.patch_id.is_empty() { patch.get(&old.hash).cloned().unwrap_or_default() } else { old.patch_id.clone() };
        let same = |c: &&scan::Commit| {
            (!old_patch.is_empty() && patch.get(&c.hash) == Some(&old_patch))
                || (!old.subject.is_empty() && c.subject == old.subject && (old.date.is_empty() || c.date == old.date))
        };
        match now.iter().find(same) {
            Some(c) => {
                again.entry(c.hash.clone()).or_insert_with(|| old.hash.chars().take(9).collect());
            }
            None => gone.push(old.clone()),
        }
    }
    (again, gone)
}

#[allow(clippy::too_many_arguments)]
fn task(state: &core::State, live: &[RuntimeAgent], u: &core::Unit, t: &core::Task, names: &Names, manager: Option<&str>, commits: Vec<String>) -> Task {
    let design = t.kind == TaskKind::Design;
    let research = t.kind == TaskKind::Research;
    let finder = !matches!(t.kind, TaskKind::Implement | TaskKind::Design | TaskKind::Research);
    let reviewer = match t.kind {
        TaskKind::Tests => Some("test".to_string()),
        TaskKind::Review => t.reviewer.map(|r| name(&r)),
        _ => None,
    };
    let by_task = |n: u32| u.tasks.iter().find(|x| x.n == n);
    // Who raised implement work: the finder of what it answers, the user, or the plan.
    let raised_by = (!finder).then(|| match t.causes.first().and_then(|c| by_task(c.task)) {
        Some(f) if f.kind == TaskKind::Gate => "user".to_string(),
        Some(f) if f.kind == TaskKind::Tests => "test".to_string(),
        Some(f) => f.reviewer.map(|r| name(&r)).unwrap_or_else(|| "code-review".into()),
        None if t.by == "user" => "user".into(),
        None => "plan".into(),
    });
    let policy = state.intents.iter().find(|i| i.id == u.intent).map(|i| i.findings).unwrap_or_default();
    // The manager decides only while there is one running; else its findings fall to the user.
    let manager_runs = manager.is_some_and(|m| live.iter().any(|a| a.session == m));
    let open = t.findings.iter().any(|f| f.state == FindingState::Open);
    let since = match t.state {
        TaskState::Waiting => Some(t.waiting_since.clone().unwrap_or_else(|| t.at.clone())),
        TaskState::Done if open => t.done.clone(),
        _ => None,
    };
    let findings = t
        .findings
        .iter()
        .map(|f| Finding {
            id: f.found.id.clone(),
            severity: f.found.severity,
            problem: f.found.problem.clone(),
            impact: f.found.impact.clone(),
            evidence: f.found.r#where.clone(),
            state: f.state,
            leads_to: u
                .tasks
                .iter()
                .filter(|x| x.causes.iter().any(|c| c.unit.is_none() && c.task == t.n && c.findings.contains(&f.found.id)))
                .map(|x| x.n.to_string())
                .collect(),
            decided_by: f.decided_by,
            decided: f.decisions.last().map(|d| d.at.clone()),
            recommendation: if f.state == FindingState::Open { f.found.suggestion.clone() } else { None },
            note: f.note.clone(),
            delivery: names.sent(&f.delivery, manager),
            attachments: attachments_of(state, &f.found.attachments),
            external: f.found.external.clone(),
            since: t.done.clone(),
            reason: f.reason.clone(),
            decider: (f.state == FindingState::Open && policy.managers() && manager_runs).then_some("manager"),
            no_manager: f.state == FindingState::Open && policy.managers() && !manager_runs,
            notes: notes_of(state, names, f),
        })
        .collect();
    let kind = match t.kind {
        TaskKind::Implement | TaskKind::Design | TaskKind::Research => "implement",
        TaskKind::Review | TaskKind::Tests => "review",
        TaskKind::Gate => "gate",
    };
    Task {
        id: t.n.to_string(),
        kind,
        state: name(&t.state),
        task_id: (kind != "review").then(|| t.n.to_string()),
        title: t.title.clone(),
        agent: t.agent.as_deref().map(|a| names.of(a)),
        agent_session: t.agent.clone(),
        writing: false,
        raised_by,
        gate: t.gate,
        since,
        started: t.started.clone(),
        delivered: t.done.clone(),
        reviewer,
        result: (kind == "review" && t.state != TaskState::Dropped).then_some(match t.state {
            TaskState::Done if !t.findings.is_empty() => "found",
            TaskState::Done => "passed",
            _ => "running",
        }),
        inputs: t.inputs.clone(),
        input: TaskIn {
            text: if t.text.is_empty() { t.title.clone() } else { t.text.clone() },
            causes: t.causes.iter().map(|c| Cause { review: c.task.to_string(), findings: c.findings.clone() }).collect(),
            steps: (t.kind == TaskKind::Gate).then(|| t.steps.clone()),
        },
        output: TaskOut {
            commits: Some(commits),
            findings: finder.then_some(findings),
            note: (!t.note.is_empty()).then(|| t.note.clone()),
            evidence: (!t.evidence.is_empty()).then(|| {
                t.evidence
                    .iter()
                    .map(|e| EvidenceOut {
                        command: e.command.clone(),
                        result: e.result,
                        summary: e.summary.clone(),
                        attachment: e.attachment.as_ref().and_then(|id| attachments_of(state, std::slice::from_ref(id)).pop()),
                    })
                    .collect()
            }),
            attachments: attachments_of(state, &t.attachments),
        },
        plans: if t.state == TaskState::Done { vec![] } else { t.plans.clone() },
        // Only a gate's own message is shown on the page; the others went to the manager.
        design,
        research,
        blind: t.blind,
        blocks: t.blocks.map(|b| b.to_string()),
        lost_commits: vec![],
        threads: threads(state, live, names, &u.id, t.n),
        // The latest conclusion about the gate or the work it follows starts the user's note.
        draft: (t.state == TaskState::Waiting).then(|| {
            let follows: Option<u32> = t.inputs.iter().find(|i| i.primary).and_then(|i| i.from.parse().ok());
            state.threads.iter().rev().filter(|x| x.unit == u.id && (x.task == t.n || Some(x.task) == follows)).find_map(|x| x.conclusion.clone())
        }).flatten(),
        delivery: if t.kind == TaskKind::Gate && t.state == TaskState::Done { names.sent(&t.delivery, manager) } else { None },
        amended: t.amended.clone(),
        dropped: t.dropped.clone().map(|d| core::Closed { by: names.of(&d.by), ..d }),
        stalled: None,
    }
}

/// A commit that names no task of its unit: a node of its own, a gap in the record.
fn unregistered(c: &scan::Commit) -> Task {
    Task {
        id: format!("commit:{}", c.id),
        kind: "implement",
        state: "unregistered".into(),
        task_id: None,
        title: c.subject.clone(),
        agent: None,
        agent_session: None,
        writing: false,
        raised_by: None,
        gate: None,
        since: None,
        started: None,
        delivered: None,
        reviewer: None,
        result: None,
        inputs: vec![core::Input { from: "start".into(), primary: true }],
        input: TaskIn { text: String::new(), causes: vec![], steps: None },
        output: TaskOut { commits: Some(vec![c.id.clone()]), ..Default::default() },
        plans: vec![],
        delivery: None,
        design: false,
        research: false,
        blind: false,
        blocks: None,
        lost_commits: vec![],
        threads: vec![],
        draft: None,
        amended: None,
        dropped: None,
        stalled: None,
    }
}

/// A cache kept under other rules (or before rules were versioned) is not reused.
#[cfg(test)]
#[test]
fn a_cache_of_other_rules_is_dropped() {
    use serde_json::json;
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("local.json");
    let memo = json!({ "u1": { "key": "k", "hashes": ["abc"], "merged": false } });
    for old in [json!({ "memo": memo }), json!({ "rules": RULES - 1, "memo": memo })] {
        std::fs::write(&file, old.to_string()).unwrap();
        assert!(Cache::default().remembering(file.clone()).stored.memo.is_empty(), "{old}");
    }
    std::fs::write(&file, json!({ "rules": RULES, "memo": memo }).to_string()).unwrap();
    let mut c = Cache::default().remembering(file.clone());
    assert_eq!(c.stored.memo.len(), 1, "kept under these rules");
    c.dirty = true;
    c.save();
    assert_eq!(Cache::default().remembering(file).stored.memo.len(), 1, "and saved under them");
}
