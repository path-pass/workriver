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
    /// The intent it works in.
    #[serde(skip_serializing_if = "Option::is_none")]
    intent: Option<String>,
    manager: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Intent {
    id: String,
    title: String,
    /// One sentence on what it is for; may be empty.
    goal: String,
    units: Vec<Unit>,
    chores: Vec<Chore>,
    /// The manager's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    manager: Option<String>,
    /// The manager's runtime state, or "gone" when it is not running.
    #[serde(skip_serializing_if = "Option::is_none")]
    manager_state: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    delivery: Option<Sent>,
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
    commits: HashMap<String, scan::Commit>,
    trees: HashMap<String, (String, scan::Module)>,
    /// Each unit's commits as last seen, for recording a merge after the branch is gone.
    pub last: HashMap<String, Vec<String>>,
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
pub fn build(state: &core::State, cache: &mut Cache, live: Option<&[RuntimeAgent]>, blocked: &HashMap<String, String>) -> Workspace {
    let live = live.unwrap_or_default();
    let names = Names(state.agents.iter().map(|a| (a.session.clone(), a.name.clone())).chain(live.iter().map(|a| (a.session.clone(), a.name.clone()))).collect());
    let repos: BTreeSet<&str> = state.units.iter().map(|u| u.repo.as_str()).collect();
    let repos = repos.into_iter().map(|r| repo(r, cache)).collect();
    let intents = state
        .intents
        .iter()
        .map(|i| Intent {
            id: i.id.clone(),
            title: i.title.clone(),
            goal: i.goal.clone(),
            units: state.units.iter().filter(|u| u.intent == i.id).map(|u| unit(state, live, u, cache, &names, i.manager.as_deref())).collect(),
            chores: state
                .chores
                .iter()
                .filter(|c| c.intent == i.id)
                .map(|c| Chore {
                    id: c.id.clone(),
                    title: c.title.clone(),
                    by: names.of(&c.by),
                    steps: c.steps.clone(),
                    state: if c.done { "done" } else { "open" },
                    since: c.at.clone(),
                    unit: c.unit.clone(),
                })
                .collect(),
            manager: i.manager.as_deref().map(|m| names.of(m)),
            manager_state: i.manager.as_ref().map(|m| live.iter().find(|a| &a.session == m).map(|a| name(&a.state)).unwrap_or_else(|| "gone".into())),
        })
        .collect();
    // Live agents, then those the records name that are gone.
    let mut sessions: Vec<Session> = live
        .iter()
        .map(|a| {
            let intent = a.group.as_ref().and_then(|g| state.intent_for(g));
            Session {
                agent: a.name.clone(),
                session: a.session.clone(),
                topic: a.topic.clone(),
                blocked_since: (a.state == AgentState::Blocked).then(|| blocked.get(&a.session).cloned().unwrap_or_else(core::now)),
                group: a.group.clone(),
                ended: false,
                state: Some(name(&a.state)),
                intent: intent.map(|i| i.id.clone()),
                manager: intent.is_some_and(|i| i.manager.as_ref() == Some(&a.session)),
            }
        })
        .collect();
    let recorded = state.agents.iter().map(|a| a.session.clone()).chain(state.units.iter().flat_map(|u| u.tasks.iter().filter_map(|t| t.agent.clone())));
    for session in recorded {
        let agent = names.of(&session);
        if !live.iter().any(|a| a.session == session) && !sessions.iter().any(|s| s.agent == agent) {
            sessions.push(Session { agent, session, topic: None, blocked_since: None, group: None, ended: true, state: None, intent: None, manager: false });
        }
    }
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

fn unit(state: &core::State, live: &[RuntimeAgent], u: &core::Unit, cache: &mut Cache, names: &Names, manager: Option<&str>) -> Unit {
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
    let mut tasks: Vec<Task> = u.tasks.iter().map(|t| task(state, live, u, t, names, manager, commits.iter().filter(|c| owner(c) == Some(t.n)).map(|c| c.id.clone()).collect())).collect();
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

#[allow(clippy::too_many_arguments)]
fn task(state: &core::State, live: &[RuntimeAgent], u: &core::Unit, t: &core::Task, names: &Names, manager: Option<&str>, commits: Vec<String>) -> Task {
    let design = t.kind == TaskKind::Design;
    let finder = !matches!(t.kind, TaskKind::Implement | TaskKind::Design);
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
            delivery: names.sent(&f.delivery, manager),
        })
        .collect();
    let kind = match t.kind {
        TaskKind::Implement | TaskKind::Design => "implement",
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
            evidence: (!t.evidence.is_empty()).then(|| t.evidence.clone()),
        },
        plans: if t.state == TaskState::Done { vec![] } else { t.plans.clone() },
        // Only a gate's own message is shown on the page; the others went to the manager.
        design,
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
        threads: vec![],
        draft: None,
        amended: None,
    }
}
