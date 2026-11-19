// The workspace in the exact shape the web UI reads (frontend/web/src/types.ts): the
// recorded state, the units' commits from git, and the agents the runtime knows.
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use workriver_core::runtime::{AgentState, RuntimeAgent};
use workriver_core::{self as core, FindingState, TaskKind, TaskState, name, parse_task_ref};
use workriver_scan as scan;

pub use crate::common::wire::workspace::*;
use crate::server::delivery::quote;

mod cache;
mod code;
pub mod task;

pub use cache::*;
pub use code::*;

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

/// The group a linked group of another machine stands for: this machine's, by the link.
fn canon(state: &core::State, groups: &[core::RuntimeRef], g: &core::RuntimeRef) -> core::RuntimeRef {
    match state.link_of(g) {
        None => g.clone(),
        Some(l) => groups.iter().find(|x| x.machine.is_none() && x.label == l.local).cloned().unwrap_or_else(|| core::RuntimeRef { id: l.local.clone(), label: l.local.clone(), machine: None }),
    }
}

fn attachments_of(state: &core::State, ids: &[String]) -> Vec<AttachmentOut> {
    ids.iter()
        .filter_map(|id| state.attachments.iter().find(|a| &a.id == id))
        .map(|a| AttachmentOut { id: a.id.clone(), name: a.name.clone(), url: format!("/api/attachments/{}", a.id), mime: a.mime.clone() })
        .collect()
}

fn notes_of(state: &core::State, names: &Names, f: &core::Finding) -> Vec<NoteOut> {
    f.notes.iter().map(|n| NoteOut { by: names.of(&n.by), at: n.at.clone(), text: n.text.clone(), attachments: attachments_of(state, &n.attachments) }).collect()
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
                        attachments: attachments_of(state, &q.context.iter().filter(|c| c.kind == "attachment").filter_map(|c| c.id.clone()).collect::<Vec<_>>()),
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
pub fn build(state: &core::State, cache: &mut Cache, live: Option<&[RuntimeAgent]>, blocked: &HashMap<String, String>, idle: &HashMap<String, String>, stalls: &HashMap<(String, u32), crate::common::wire::Stall>, groups: &[core::RuntimeRef]) -> Workspace {
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
                project: crate::server::handlers::project_of(state, &b.project).map(|p| p.path.clone()).unwrap_or_else(|| b.project.clone()),
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
        .map(|c| chore(c, &names, Some(crate::server::handlers::project_of(state, &c.project).map(|p| p.path.clone()).unwrap_or_else(|| c.project.clone()))))
        .collect();
    let messages = messages(state, live, &names);
    Workspace { repos, projects: projects(state), backlog, built_at: core::now(), intents, sessions, workspaces, machines, reduced: false, chores, alerts: vec![], messages, runs: vec![] }
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
    if let Some(p) = crate::server::handlers::project_of(state, &cwd.to_string_lossy()) {
        return Some(p.path.clone());
    }
    let top = cwd.ancestors().find(|d| d.join(".git").exists())?;
    // A linked worktree's `.git` is a file: `gitdir: <main>/.git/worktrees/<name>`.
    let main = std::fs::read_to_string(top.join(".git")).ok().and_then(|g| g.trim().strip_prefix("gitdir:").and_then(|d| d.trim().split_once("/.git/worktrees/")).map(|(m, _)| m.to_string()));
    let top = main.unwrap_or_else(|| top.to_string_lossy().into_owned());
    Some(crate::server::handlers::project_of(state, &top).map(|p| p.path.clone()).unwrap_or(top))
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
    crate::server::handlers::project_of(state, &path).map(|p| p.path.clone()).unwrap_or(path)
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
    let folders = state.intents.iter().map(|i| project_folder(state, i)).chain(state.chores.iter().filter(|c| c.intent.is_empty()).map(|c| crate::server::handlers::project_of(state, &c.project).map(|p| p.path.clone()).unwrap_or_else(|| c.project.clone()))).chain(state.backlog.iter().map(|b| crate::server::handlers::project_of(state, &b.project).map(|p| p.path.clone()).unwrap_or_else(|| b.project.clone())));
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

#[cfg(test)]
mod tests;
