// The workspace in the exact shape the web UI reads (frontend/web/src/types.ts): the
// recorded state, the units' commits from git, and the agents the runtime knows.
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use serde::Serialize;
use workriver_core::runtime::{Runtime, RuntimeError};
use workriver_core::{self as core, FindingState, TaskKind, TaskState, name, parse_task_ref};
use workriver_scan as scan;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    repos: Vec<Repo>,
    built_at: String,
    intents: Vec<Intent>,
    sessions: Vec<Session>,
}

#[derive(Serialize)]
struct Repo {
    path: String,
    name: String,
    modules: scan::Module,
}

#[derive(Serialize)]
struct Session {
    agent: String,
    ended: bool,
}

#[derive(Serialize)]
struct Intent {
    id: String,
    title: String,
    units: Vec<Unit>,
    chores: Vec<Chore>,
}

#[derive(Serialize)]
struct Chore {
    id: String,
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
    title: String,
    purpose: String,
    repo: String,
    branch: Option<String>,
    status: &'static str,
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
    evidence: Option<Vec<core::Evidence>>,
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
}

#[derive(Serialize)]
struct Commit {
    id: String,
    subject: String,
    date: String,
    author: String,
    live: bool,
    files: Vec<scan::FileChange>,
}

/// Commits never change and a tree only with its HEAD, so both are kept between snapshots.
#[derive(Default)]
pub struct Cache {
    commits: HashMap<String, scan::Commit>,
    trees: HashMap<String, (String, scan::Module)>,
    /// Each unit's commits as last seen, for recording a merge after the branch is gone.
    pub last: HashMap<String, Vec<String>>,
}

/// The id of the live commit, as in the UI's data.
const LIVE: &str = "uncommitted";

pub fn build(state: &core::State, cache: &mut Cache, runtime: &dyn Runtime) -> Workspace {
    let repos: BTreeSet<&str> = state.units.iter().map(|u| u.repo.as_str()).collect();
    let repos = repos.into_iter().map(|r| repo(r, cache)).collect();
    let intents = state
        .intents
        .iter()
        .map(|i| Intent {
            id: i.id.clone(),
            title: i.title.clone(),
            units: state.units.iter().filter(|u| u.intent == i.id).map(|u| unit(u, cache)).collect(),
            chores: state
                .chores
                .iter()
                .filter(|c| c.intent == i.id)
                .map(|c| Chore {
                    id: c.id.clone(),
                    title: c.title.clone(),
                    by: c.by.clone(),
                    steps: c.steps.clone(),
                    state: if c.done { "done" } else { "open" },
                    since: c.at.clone(),
                    unit: c.unit.clone(),
                })
                .collect(),
        })
        .collect();
    let sessions = match runtime.agents() {
        Ok(list) => list.into_iter().map(|a| Session { agent: a.name, ended: false }).collect(),
        Err(RuntimeError::None) => vec![],
        Err(e) => {
            tracing::warn!("runtime: {e}");
            vec![]
        }
    };
    Workspace { repos, built_at: core::now(), intents, sessions }
}

fn repo(path: &str, cache: &mut Cache) -> Repo {
    let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let p = Path::new(path);
    let head = scan::head(p).unwrap_or_default();
    let modules = match cache.trees.get(path) {
        Some((h, m)) if *h == head => m.clone(),
        _ => match scan::tree(p) {
            Ok(m) => {
                cache.trees.insert(path.to_string(), (head, m.clone()));
                m
            }
            Err(e) => {
                tracing::warn!("{e}");
                scan::Module { name: name.clone(), children: Some(vec![]), ..Default::default() }
            }
        },
    };
    Repo { path: path.to_string(), name, modules }
}

fn unit(u: &core::Unit, cache: &mut Cache) -> Unit {
    let repo = Path::new(&u.repo);
    let merged = u.merged.is_some();
    let hashes = if merged { Ok(u.merged_commits.clone()) } else { scan::commit_list(repo, &u.base, &u.branch) };
    let mut commits = vec![];
    match hashes {
        Ok(hashes) => {
            for h in &hashes {
                let c = match cache.commits.get(h) {
                    Some(c) => c.clone(),
                    None => match scan::commit(repo, h) {
                        Ok(c) => cache.commits.entry(h.clone()).or_insert(c).clone(),
                        Err(e) => {
                            tracing::warn!("{e}");
                            continue;
                        }
                    },
                };
                commits.push(c);
            }
            if !merged {
                cache.last.insert(u.id.clone(), hashes);
            }
        }
        Err(e) => tracing::warn!("unit {}: {e}", u.id),
    }

    // Which task each commit carries out; a commit naming no task of this unit is a gap.
    let owner = |c: &scan::Commit| {
        c.task.as_deref().and_then(parse_task_ref).filter(|(unit, n)| *unit == u.id && u.tasks.iter().any(|t| t.n == *n)).map(|(_, n)| n)
    };
    let mut tasks: Vec<Task> = u.tasks.iter().map(|t| task(u, t, commits.iter().filter(|c| owner(c) == Some(t.n)).map(|c| c.id.clone()).collect())).collect();
    for c in commits.iter().filter(|c| owner(c).is_none()) {
        tasks.push(unregistered(c));
    }

    let mut shown: Vec<Commit> = commits
        .into_iter()
        .map(|c| Commit { id: c.id, subject: c.subject, date: c.date, author: c.author, live: false, files: c.files })
        .collect();
    // Uncommitted work belongs to the running implement task started last.
    // ponytail: a guess when several run in one unit; ask the runtime whose cwd it is (stage 2).
    let writer = u.tasks.iter().filter(|t| t.kind == TaskKind::Implement && t.state == TaskState::Running).max_by_key(|t| &t.started).map(|t| t.n.to_string());
    if !merged {
        let files = scan::live(repo, &u.branch).unwrap_or_else(|e| {
            tracing::warn!("unit {}: {e}", u.id);
            vec![]
        });
        if !files.is_empty() {
            let at = tasks.iter_mut().find(|t| Some(&t.id) == writer.as_ref());
            let author = at.as_ref().and_then(|t| t.agent.clone()).unwrap_or_default();
            if let Some(t) = at {
                t.writing = true;
                t.output.commits.get_or_insert_with(Vec::new).push(LIVE.into());
            }
            shown.push(Commit { id: LIVE.into(), subject: String::new(), date: core::now(), author, live: true, files });
        }
    }

    let started = u.tasks.iter().any(|t| t.state != TaskState::Todo && t.kind != TaskKind::Gate) || !shown.is_empty();
    Unit {
        id: u.id.clone(),
        title: u.title.clone(),
        purpose: u.purpose.clone(),
        repo: u.repo.clone(),
        branch: (!merged).then(|| u.branch.clone()),
        status: if merged { "merged" } else if started { "active" } else { "planned" },
        tasks,
        commits: shown,
    }
}

fn task(u: &core::Unit, t: &core::Task, commits: Vec<String>) -> Task {
    let finder = t.kind != TaskKind::Implement;
    let reviewer = match t.kind {
        TaskKind::Tests => Some("test".to_string()),
        TaskKind::Review => t.reviewer.map(|r| name(&r)),
        _ => None,
    };
    let by_task = |n: u32| u.tasks.iter().find(|x| x.n == n);
    // Who raised implement work: the finder of what it answers, the user, or the plan.
    let raised_by = (t.kind == TaskKind::Implement).then(|| match t.causes.first().and_then(|c| by_task(c.task)) {
        Some(f) if f.kind == TaskKind::Gate => "user".to_string(),
        Some(f) if f.kind == TaskKind::Tests => "test".to_string(),
        Some(f) => f.reviewer.map(|r| name(&r)).unwrap_or_else(|| "code-review".into()),
        None if t.by == "user" => "user".into(),
        None => "plan".into(),
    });
    let open = t.findings.iter().any(|f| f.state == FindingState::Open);
    let since = match t.state {
        TaskState::Waiting => Some(t.at.clone()),
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
                .filter(|x| x.causes.iter().any(|c| c.task == t.n && c.findings.contains(&f.found.id)))
                .map(|x| x.n.to_string())
                .collect(),
            decided_by: f.decided_by,
            recommendation: if f.state == FindingState::Open { f.found.suggestion.clone() } else { None },
            note: f.note.clone(),
        })
        .collect();
    let kind = match t.kind {
        TaskKind::Implement => "implement",
        TaskKind::Review | TaskKind::Tests => "review",
        TaskKind::Gate => "gate",
    };
    Task {
        id: t.n.to_string(),
        kind,
        state: name(&t.state),
        task_id: (kind != "review").then(|| t.n.to_string()),
        title: t.title.clone(),
        agent: t.agent.clone(),
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
            evidence: (!t.evidence.is_empty()).then(|| t.evidence.clone()),
        },
        plans: if t.state == TaskState::Done { vec![] } else { t.plans.clone() },
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
    }
}
