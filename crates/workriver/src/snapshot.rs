// The workspace in the exact shape the web UI reads (frontend/web/src/types.ts): the
// recorded state, the units' commits from git, and the agents the runtime knows.
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use serde::Serialize;
use workriver_core::runtime::{AgentState, RuntimeAgent};
use workriver_core::{self as core, FindingState, TaskKind, TaskState, name, parse_task_ref};
use workriver_scan as scan;

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
}

/// Takes out of a snapshot built from `State::blind_view` what git adds to it and an agent
/// on a blind task is not to be told: other commits' subjects and authors (the range to
/// review is the hashes; the code is read from git), tasks made up from unregistered
/// commits, uncommitted work, and what other sessions are about.
pub fn hide(ws: &mut Workspace) {
    ws.reduced = true;
    for u in ws.intents.iter_mut().flat_map(|i| i.units.iter_mut()) {
        u.tasks.retain(|t| t.state != "unregistered");
        u.commits.retain(|c| !c.live);
        for c in &mut u.commits {
            c.subject.clear();
            c.author.clear();
            c.files.clear();
        }
    }
    for s in &mut ws.sessions {
        s.topic = None;
    }
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
    title: String,
    purpose: String,
    repo: String,
    /// The machine its repository is on; none: this one.
    #[serde(skip_serializing_if = "Option::is_none")]
    machine: Option<String>,
    branch: Option<String>,
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
    /// While it comes: starting (the agent), reading (it works on it).
    #[serde(skip_serializing_if = "Option::is_none")]
    step: Option<&'static str>,
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
                    .map(|q| Turn {
                        id: q.id.clone(),
                        by: (q.by != "user").then(|| names.of(&q.by)),
                        question: q.text.clone(),
                        context: q.context.clone(),
                        answer: q.answer.clone(),
                        step: q.open().then_some(if q.agent.is_some() && agent.is_some() { "reading" } else { "starting" }),
                        consulted: q.asked.as_ref().map(|a| Consulted { agent: names.of(a), outcome: "asked" }),
                        at: q.at.clone(),
                        failed: q.failed.as_ref().map(|(code, reason)| Failed { code: *code, reason: reason.clone() }),
                        conclude: q.conclude,
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
    /// Whether this cache reads repositories on other machines (the background one does);
    /// one that does not takes what the background read last, from `shared`.
    pub remote_reads: bool,
    pub shared: std::sync::Arc<std::sync::Mutex<Shared>>,
    /// Each repository's refs and work trees, read once per build.
    refs: HashMap<String, HashMap<String, String>>,
    worktrees: HashMap<String, HashMap<String, std::path::PathBuf>>,
    /// Units found merged in git and not yet recorded so: (unit, its commits).
    pub merged_now: Vec<(String, Vec<String>)>,
    /// This build: units whose commits were worked out again, and those reused.
    pub recomputed: usize,
    pub reused: usize,
}

/// What a cache keeps across restarts.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Stored {
    #[serde(default)]
    seen: HashMap<String, Seen>,
    #[serde(default)]
    memo: HashMap<String, Memo>,
    #[serde(default)]
    meta: HashMap<String, scan::Named>,
    #[serde(default)]
    patch: HashMap<String, String>,
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
        self.stored = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
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

/// `blocked`: when each blocked live agent (by session) was first seen blocked.
pub fn build(state: &core::State, cache: &mut Cache, live: Option<&[RuntimeAgent]>, blocked: &HashMap<String, String>, groups: &[core::RuntimeRef]) -> Workspace {
    let live = live.unwrap_or_default();
    cache.refs.clear();
    cache.worktrees.clear();
    (cache.recomputed, cache.reused) = (0, 0);
    let machines = cache.machines.as_ref().map(|m| m.states()).unwrap_or_default();
    let names = Names(state.agents.iter().map(|a| (a.session.clone(), a.name.clone())).chain(live.iter().map(|a| (a.session.clone(), a.name.clone()))).collect());
    let repos: BTreeSet<(Option<&str>, &str)> = state.units.iter().map(|u| (u.machine.as_deref(), u.repo.as_str())).collect();
    let repos = repos.into_iter().map(|(m, r)| repo(m, r, cache)).collect();
    let repos: Vec<Repo> = repos;
    let intents = state
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
                .chain(state.units.iter().filter(|u| u.intent == i.id).map(|u| unit(state, live, u, cache, &names, state.manager_of(&i.id))))
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
        .collect();
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
            sessions.push(Session { agent, session, topic: None, blocked_since: None, group: None, ended: true, state: None, intent: None, manager: false, machine: None, unreachable: false });
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
    let workspaces = groups.iter().map(|g| Group { id: g.id.clone(), label: g.label.clone(), machine: g.machine.clone(), linked_to: state.link_of(g).map(|l| l.local.clone()) }).collect();
    let chores = state
        .chores
        .iter()
        .filter(|c| c.intent.is_empty())
        .map(|c| chore(c, &names, Some(crate::serve::project_of(state, &c.project).map(|p| p.path.clone()).unwrap_or_else(|| c.project.clone()))))
        .collect();
    Workspace { repos, projects: projects(state), backlog, built_at: core::now(), intents, sessions, workspaces, machines, reduced: false, chores }
}

/// An intent's project folder: as recorded, else where its first unit's repository is.
fn project_folder(state: &core::State, i: &core::Intent) -> String {
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

/// An intent's own tasks (research, design and their reviews and gates) as a unit without a
/// repository, listed first under the intent; none when it has none.
fn intent_unit(state: &core::State, live: &[RuntimeAgent], i: &core::Intent, names: &Names) -> Option<Unit> {
    if i.tasks.is_empty() {
        return None;
    }
    let u = core::Unit {
        id: i.id.clone(),
        intent: i.id.clone(),
        title: i.title.clone(),
        purpose: i.goal.clone(),
        repo: String::new(),
        branch: String::new(),
        base: String::new(),
        machine: None,
        at: i.at.clone(),
        merged: None,
        merged_commits: vec![],
        tasks: i.tasks.clone(),
        earlier: vec![],
        links: vec![],
        done: None,
    };
    let manager = state.manager_of(&i.id);
    Some(Unit {
        id: i.id.clone(),
        intent_level: true,
        title: i.title.clone(),
        purpose: i.goal.clone(),
        repo: String::new(),
        machine: None,
        branch: None,
        previously: vec![],
        status: if i.tasks.iter().any(|t| t.state != TaskState::Todo) { "active" } else { "planned" },
        progress: state.progress(&u),
        done_reason: None,
        tasks: i.tasks.iter().map(|t| task(state, live, &u, t, names, manager, vec![])).collect(),
        commits: vec![],
    })
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
        Some(format!("{}|{}={tip}|{}={base}|{fork}|{}|{}|{}", u.repo, u.branch, u.base, u.earlier.len(), u.links.len(), u.merged_commits.len()))
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
    for u in state.units.iter().filter(|u| u.machine.is_some()) {
        let Some(at) = cache.runner(u.machine.as_deref()) else {
            skipped = true;
            // Not reachable (yet): what was read before the last restart, without texts.
            let known = cache.stored.memo.get(&u.id).map(|m| m.hashes.clone()).unwrap_or_else(|| u.merged_commits.clone());
            let mut shared = cache.shared.lock().unwrap();
            if !shared.units.contains_key(&u.id) && !known.is_empty() {
                let commits = known.iter().filter_map(|h| cache.stored.meta.get(h).map(|n| scan::Commit { id: h.chars().take(9).collect(), hash: h.clone(), subject: n.subject.clone(), date: n.date.clone(), author: n.author.clone(), task: n.task.clone(), files: vec![] })).collect();
                let patch = cache.stored.patch.iter().filter(|(h, _)| known.contains(h)).map(|(h, p)| (h.clone(), p.clone())).collect();
                shared.units.insert(u.id.clone(), (commits, vec![], patch));
                changed = true;
            }
            continue;
        };
        let (c, f) = read_unit(&at, u, cache);
        let p: HashMap<String, String> = c.iter().map(|x| &x.hash).chain(u.earlier.iter().flat_map(|e| &e.commits).map(|x| &x.hash)).filter_map(|h| cache.stored.patch.get(h).map(|p| (h.clone(), p.clone()))).collect();
        let mut shared = cache.shared.lock().unwrap();
        let same = shared.units.get(&u.id).is_some_and(|(oc, of, op)| oc.iter().map(|x| &x.hash).eq(c.iter().map(|x| &x.hash)) && *of == f && *op == p);
        if !same {
            shared.units.insert(u.id.clone(), (c, f, p));
            changed = true;
        }
    }
    let repos: BTreeSet<(&str, &str)> = state.units.iter().filter_map(|u| Some((u.machine.as_deref()?, u.repo.as_str()))).collect();
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
fn read_unit(at: &scan::Runner, u: &core::Unit, cache: &mut Cache) -> (Vec<scan::Commit>, Vec<scan::FileChange>) {
    let started = std::time::Instant::now();
    let calls = scan::CALLS.load(std::sync::atomic::Ordering::Relaxed);
    let repo = Path::new(&u.repo);
    let hashes = match unit_hashes(at, u, cache) {
        Ok((hashes, merged)) => {
            if merged && u.merged.is_none() && !cache.merged_now.iter().any(|(x, _)| x == &u.id) {
                cache.merged_now.push((u.id.clone(), hashes.clone()));
            }
            hashes
        }
        Err(e) => {
            tracing::warn!("unit {}: {e}", u.id);
            vec![]
        }
    };
    // Light: what each commit says is enough. Else their texts, those not read yet at once.
    let named = |h: &String, cache: &Cache| cache.stored.meta.get(h).map(|n| scan::Commit { id: h.chars().take(9).collect(), hash: h.clone(), subject: n.subject.clone(), date: n.date.clone(), author: n.author.clone(), task: n.task.clone(), files: vec![] });
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
    let commits: Vec<scan::Commit> = hashes.iter().filter_map(|h| cache.commits.get(h).cloned().or_else(|| named(h, cache))).collect();
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
                let (c, f) = read_unit(&at, u, cache);
                let p: HashMap<String, String> = c.iter().map(|x| &x.hash).chain(u.earlier.iter().flat_map(|e| &e.commits).map(|x| &x.hash)).filter_map(|h| cache.stored.patch.get(h).map(|p| (h.clone(), p.clone()))).collect();
                if u.machine.is_some() {
                    cache.shared.lock().unwrap().units.insert(u.id.clone(), (c.clone(), f.clone(), p.clone()));
                }
                (c, f, p)
            }
        }
    };

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
            Commit { id: c.id, subject: c.subject, date: c.date, author: c.author, live: false, files: c.files, linked, rebased_from }
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
            shown.push(Commit { id: LIVE.into(), subject: String::new(), date: core::now(), author, live: true, files, linked: false, rebased_from: None });
        }
    }

    let started = u.tasks.iter().any(|t| t.state != TaskState::Todo && t.kind != TaskKind::Gate) || !shown.is_empty();
    Unit {
        id: u.id.clone(),
        intent_level: false,
        title: u.title.clone(),
        purpose: u.purpose.clone(),
        repo: u.repo.clone(),
        machine: u.machine.clone(),
        branch: (!merged).then(|| u.branch.clone()),
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
        // Found merged in git: so already, before the record of it lands.
        progress: if !merged && cache.merged_now.iter().any(|(x, _)| x == &u.id) { core::Progress::Merged } else { state.progress(u) },
        done_reason: u.done.as_ref().map(|d| d.reason.clone()),
        tasks,
        commits: shown,
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
        reviewer,
        result: (kind == "review").then_some(match t.state {
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
        lost_commits: vec![],
        threads: threads(state, live, names, &u.id, t.n),
        // The latest conclusion about the gate or the work it follows starts the user's note.
        draft: (t.state == TaskState::Waiting).then(|| {
            let follows: Option<u32> = t.inputs.iter().find(|i| i.primary).and_then(|i| i.from.parse().ok());
            state.threads.iter().rev().filter(|x| x.unit == u.id && (x.task == t.n || Some(x.task) == follows)).find_map(|x| x.conclusion.clone())
        }).flatten(),
        delivery: if t.kind == TaskKind::Gate && t.state == TaskState::Done { names.sent(&t.delivery, manager) } else { None },
        amended: t.amended.clone(),
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
        lost_commits: vec![],
        threads: vec![],
        draft: None,
        amended: None,
    }
}
