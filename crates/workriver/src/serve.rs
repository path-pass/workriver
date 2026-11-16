// The daemon: one process on 127.0.0.1 that records events, reads git, and serves the
// web UI with its data and a stream of "something changed".
use std::collections::HashSet;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use axum::extract::{DefaultBodyLimit, Path as UrlPath, Query, Request, State};
use axum::http::{Method, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast;
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};
use workriver_core::runtime::{AgentState, Runtime, RuntimeAgent, RuntimeError};
use workriver_core::{self as core, Body, Delivery, DeliveryStep, ReasonCode, Store};
use workriver_scan as scan;

use crate::{ask, attachments, deliver};
use crate::snapshot::{self, Cache};

pub struct App {
    store: Mutex<Store>,
    cache: Mutex<Cache>,
    runtime: Arc<dyn Runtime>,
    /// What was wrong with the runtime at start, if anything.
    warning: Option<String>,
    /// The live agents as last polled; None while the runtime cannot be reached.
    live: Mutex<Option<Vec<RuntimeAgent>>>,
    /// Delivered messages waiting to be picked up.
    pending: Mutex<Vec<Pending>>,
    /// Questions waiting for their temporary agent's answer, with when they give up.
    deadlines: Mutex<Vec<(String, std::time::Instant)>>,
    /// Threads whose temporary agent waits for a follow-up: name, and when it is closed.
    idle: Mutex<Vec<(String, String, std::time::Instant)>>,
    waits: Waits,
    /// When each live agent was first seen blocked, by session.
    // ponytail: in memory; a restart makes a blocked agent's wait start again.
    blocked: Mutex<std::collections::HashMap<String, String>>,
    changed: broadcast::Sender<()>,
    port: u16,
    /// The data directory: the database's folder, which holds the attachment store too.
    home: PathBuf,
    watcher: Mutex<Option<Debouncer<RecommendedWatcher>>>,
    watched: Mutex<HashSet<PathBuf>>,
}

/// A delivered message: picked up once its receiver is seen working after it arrived.
/// Rebuilt from the records at start.
struct Pending {
    unit: String,
    task: u32,
    finding: Option<String>,
    chore: Option<String>,
    delivery: Delivery,
    /// The receiver was seen not working since the message went out.
    armed: bool,
    sent: std::time::Instant,
}

/// How long a delivered message is watched for its pick-up; the receiver may vanish from
/// the runtime for a while (a restart, a moved pane) and come back.
const WATCH: Duration = Duration::from_secs(30 * 60);

type Shared = Arc<App>;

/// How long after the decision a temporary agent is closed, so its own command can return.
const CLOSE_DELAY: Duration = Duration::from_secs(3);

/// How often the runtime's agents are read.
const POLL: Duration = Duration::from_secs(2);

/// Serves on an already bound listener until Ctrl-C.
/// How long a temporary agent may take to answer, and to wait idle for a follow-up.
#[derive(Clone, Copy)]
pub struct Waits {
    pub answer: Duration,
    pub idle: Duration,
}

impl Default for Waits {
    fn default() -> Self {
        Waits { answer: ask::TIMEOUT, idle: ask::IDLE }
    }
}

pub async fn serve(listener: tokio::net::TcpListener, store: Store, runtime: Arc<dyn Runtime>, warning: Option<String>, waits: Waits) -> anyhow::Result<()> {
    let port = listener.local_addr()?.port();
    let store_home = store.path().parent().map(Path::to_path_buf).unwrap_or_default();
    let (changed, _) = broadcast::channel(16);
    let app = Arc::new(App {
        store: Mutex::new(store),
        cache: Mutex::default(),
        runtime,
        warning,
        live: Mutex::new(None),
        pending: Mutex::default(),
        deadlines: Mutex::default(),
        idle: Mutex::default(),
        waits,
        blocked: Mutex::default(),
        changed: changed.clone(),
        port,
        home: store_home,
        watcher: Mutex::new(None),
        watched: Mutex::default(),
    });
    let poller = app.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(POLL);
        loop {
            tick.tick().await;
            let app = poller.clone();
            let _ = tokio::task::spawn_blocking(move || app.poll()).await;
        }
    });
    let tx = changed.clone();
    let debouncer = new_debouncer(Duration::from_millis(300), move |res: DebounceEventResult| {
        if res.is_ok_and(|events| events.iter().any(|e| worth_a_look(&e.path))) {
            let _ = tx.send(());
        }
    });
    match debouncer {
        Ok(d) => *app.watcher.lock().unwrap() = Some(d),
        Err(e) => tracing::warn!("cannot watch repositories, git changes show on the next record: {e}"),
    }
    let units: Vec<(String, String)> = app.store.lock().unwrap().state().units.iter().map(|u| (u.repo.clone(), u.branch.clone())).collect();
    for (repo, branch) in units {
        app.watch(Path::new(&repo), &branch);
    }
    // Messages delivered before a restart are watched again; whether their receiver was seen
    // idle since is lost, so it must be seen not working first.
    let pending: Vec<Pending> = {
        let store = app.store.lock().unwrap();
        let delivered = |d: &Option<Delivery>| d.clone().filter(|d| d.step == DeliveryStep::Delivered);
        let now = std::time::Instant::now();
        let mut out = vec![];
        for u in &store.state().units {
            for t in &u.tasks {
                if let Some(d) = delivered(&t.delivery) {
                    out.push(Pending { unit: u.id.clone(), task: t.n, finding: None, chore: None, delivery: d, armed: false, sent: now });
                }
                for f in &t.findings {
                    if let Some(d) = delivered(&f.delivery) {
                        out.push(Pending { unit: u.id.clone(), task: t.n, finding: Some(f.found.id.clone()), chore: None, delivery: d, armed: false, sent: now });
                    }
                }
            }
        }
        for c in &store.state().chores {
            if let Some(d) = delivered(&c.delivery) {
                out.push(Pending { unit: String::new(), task: 0, finding: None, chore: Some(c.id.clone()), delivery: d, armed: false, sent: now });
            }
        }
        out
    };
    app.pending.lock().unwrap().extend(pending);
    // Questions left open by a previous run have lost their wait; close them.
    let open: Vec<String> = app.store.lock().unwrap().state().questions.iter().filter(|q| q.open()).map(|q| q.id.clone()).collect();
    for id in open {
        app.fail_question(&id, ReasonCode::Restarted, "the service restarted while the question was open");
    }
    // ponytail: a thread's temporary agent from a previous run is closed rather than adopted;
    // the next question starts a new one with the thread so far.
    let left: Vec<String> = app.store.lock().unwrap().state().threads.iter().filter_map(|t| t.name.clone()).collect();
    for name in left {
        let _ = app.runtime.close(&name);
    }
    tracing::info!("workriver on http://127.0.0.1:{port}");
    axum::serve(listener, router(app)).with_graceful_shutdown(async { tokio::signal::ctrl_c().await.ok(); }).await?;
    Ok(())
}

/// The git changes that can change what is shown: work-tree files, refs and HEAD.
// ponytail: node_modules and target are skipped by name, not by .gitignore; check ignores if a repo tracks such a folder.
fn worth_a_look(path: &Path) -> bool {
    let parts: Vec<_> = path.components().map(|c| c.as_os_str().to_string_lossy()).collect();
    if parts.iter().any(|p| p == "node_modules" || p == "target") || parts.last().is_some_and(|p| p.ends_with(".lock")) {
        return false;
    }
    match parts.iter().position(|p| p == ".git") {
        None => true,
        Some(k) => parts[k..].iter().any(|p| p == "refs" || p == "HEAD" || p == "packed-refs"),
    }
}

impl App {
    /// Watches a unit's repository: its work tree, the work tree of its branch, and the
    /// git folder where refs move.
    fn watch(&self, repo: &Path, branch: &str) {
        let mut paths = vec![repo.to_path_buf()];
        paths.extend(scan::common_dir(repo).ok());
        paths.extend(scan::worktree_of(repo, branch).ok().flatten());
        let mut watcher = self.watcher.lock().unwrap();
        let Some(w) = watcher.as_mut() else { return };
        let mut seen = self.watched.lock().unwrap();
        for p in paths {
            if seen.insert(p.clone())
                && let Err(e) = w.watcher().watch(&p, RecursiveMode::Recursive)
            {
                tracing::warn!("cannot watch {}: {e}", p.display());
            }
        }
    }

    /// Reads the runtime's agents; says "changed" when they differ, and marks delivered
    /// messages picked up when their receiver starts working.
    fn poll(&self) {
        let now = self.runtime.agents().ok();
        {
            let mut blocked = self.blocked.lock().unwrap();
            let still: Vec<&RuntimeAgent> = now.iter().flatten().filter(|a| a.state == AgentState::Blocked).collect();
            blocked.retain(|s, _| still.iter().any(|a| &a.session == s));
            for a in still {
                blocked.entry(a.session.clone()).or_insert_with(core::now);
            }
        }
        let differs = {
            let mut live = self.live.lock().unwrap();
            let differs = *live != now;
            *live = now.clone();
            differs
        };
        let mut picked = vec![];
        self.pending.lock().unwrap().retain_mut(|p| {
            let to = p.delivery.to.as_deref();
            match now.as_ref().and_then(|l| l.iter().find(|a| Some(a.session.as_str()) == to)) {
                None => p.sent.elapsed() < WATCH,
                Some(a) if a.state == AgentState::Working && p.armed => {
                    picked.push(Body::DeliveryUpdated {
                        unit: p.unit.clone(),
                        task: p.task,
                        finding: p.finding.clone(),
                        chore: p.chore.clone(),
                        delivery: Delivery { step: DeliveryStep::Picked, ..p.delivery.clone() },
                    });
                    false
                }
                Some(a) => {
                    p.armed |= a.state != AgentState::Working;
                    p.sent.elapsed() < WATCH
                }
            }
        });
        let recorded = !picked.is_empty();
        for body in picked {
            if let Err(e) = self.store.lock().unwrap().append("workriver", body) {
                tracing::warn!("cannot record a pick-up: {e}");
            }
        }
        // Questions whose temporary agent has not answered in time.
        let now = std::time::Instant::now();
        let late: Vec<String> = self.deadlines.lock().unwrap().iter().filter(|(_, at)| *at <= now).map(|(id, _)| id.clone()).collect();
        for id in &late {
            self.fail_question(id, ReasonCode::Timeout, &format!("no answer within {} seconds", self.waits.answer.as_secs()));
        }
        let recorded = recorded || !late.is_empty();
        // Threads idle for too long: their temporary agent goes; the thread stays open.
        let gone: Vec<String> = {
            let mut idle = self.idle.lock().unwrap();
            let gone = idle.iter().filter(|(_, _, at)| *at <= now).map(|(_, name, _)| name.clone()).collect();
            idle.retain(|(_, _, at)| *at > now);
            gone
        };
        for name in gone {
            if let Err(e) = self.runtime.close(&name) {
                tracing::warn!("cannot close {name}: {e}");
            }
        }
        if differs || recorded {
            let _ = self.changed.send(());
        }
    }

    fn live(&self) -> Option<Vec<RuntimeAgent>> {
        self.live.lock().unwrap().clone()
    }

    fn record(&self, actor: &str, body: Body) -> Result<core::Event, ApiError> {
        let body = self.complete(actor, body)?;
        let e = self.store.lock().unwrap().append(actor, body)?;
        self.deliver(&e);
        match &e.body {
            Body::QuestionAnswered { question, .. } => self.answered(question),
            Body::ThreadEnded { thread } => self.end_thread(thread),
            _ => {}
        }
        // A unit's branch may have got a work tree of its own since; watching again is cheap.
        if let (_, Some(unit), _) = e.body.refs() {
            let found = self.store.lock().unwrap().state().unit(&unit).map(|u| (u.repo.clone(), u.branch.clone()));
            if let Ok((repo, branch)) = found {
                self.watch(Path::new(&repo), &branch);
            }
        }
        let _ = self.changed.send(());
        Ok(e)
    }

    /// What only git can check or fill in before an event is recorded.
    fn complete(&self, actor: &str, body: Body) -> Result<Body, ApiError> {
        Ok(match body {
            Body::ProjectAdded { id, path, title } => {
                let folder = std::fs::canonicalize(&path).map_err(|e| bad(format!("{path}: {e}")))?;
                let title = if title.trim().is_empty() { folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() } else { title };
                Body::ProjectAdded { id, path: folder.to_string_lossy().into_owned(), title }
            }
            Body::FindingAdded { project, intent, finding, source, note } => Body::FindingAdded { project: self.project_folder(&project)?, intent, finding, source, note },
            Body::IntentCreated { id, title, goal, runtime_ref, project, state, note, external } => {
                let project = if project.is_empty() { project } else { self.project_folder(&project)? };
                Body::IntentCreated { id, title, goal, runtime_ref, project, state, note, external }
            }
            Body::UnitCreated { id, intent, title, purpose, repo, branch, base } => {
                let top = scan::toplevel(Path::new(&repo)).map_err(|e| bad(format!("{repo} is not a git repository: {e}")))?;
                if !scan::has_rev(&top, &base) {
                    return Err(bad(format!("base {base:?} is not a commit or branch in {}", top.display())));
                }
                Body::UnitCreated { id, intent, title, purpose, repo: top.to_string_lossy().into_owned(), branch, base }
            }
            Body::UnitMerged { unit, commits } if commits.is_empty() => {
                // The branch's commits now, or as last seen if they are already merged away.
                let state = self.store.lock().unwrap();
                let u = state.state().unit(&unit)?;
                let now = scan::commit_list(Path::new(&u.repo), &u.base, &u.branch).unwrap_or_default();
                let commits = if now.is_empty() { self.cache.lock().unwrap().last.get(&unit).cloned().unwrap_or_default() } else { now };
                Body::UnitMerged { unit, commits }
            }
            // An agent given by its name is recorded by its session.
            Body::TaskCreated(mut t) => {
                t.agent = t.agent.map(|a| self.session_of(&a, Some(actor)));
                Body::TaskCreated(t)
            }
            Body::TaskStarted { unit, task, agent } => Body::TaskStarted { unit, task, agent: agent.map(|a| self.session_of(&a, Some(actor))) },
            other => other,
        })
    }

    /// A live agent's session by its name; anything else as it is.
    /// A live agent's session, given as a session or by its name; anything else as it is.
    /// Names may repeat across runtime groups: then the one in `near`'s group wins.
    fn session_of(&self, agent: &str, near: Option<&str>) -> String {
        let live = self.live().unwrap_or_default();
        if live.iter().any(|a| a.session == agent) {
            return agent.to_string();
        }
        let group_of = |s: &str| live.iter().find(|a| a.session == s).and_then(|a| a.group.clone());
        let here = near.and_then(group_of);
        let named: Vec<_> = live.iter().filter(|a| a.name == agent).collect();
        named.iter().find(|a| here.is_some() && a.group == here).or(named.first()).map(|a| a.session.clone()).unwrap_or_else(|| agent.to_string())
    }

    /// Starts the temporary agent for a question and gives it the prompt; any failure is
    /// recorded on the question. Blocking: it waits for the agent to be ready.
    fn start_question(&self, id: &str) {
        let fail = |code, why: String| self.fail_question(id, code, &why);
        // Read fresh: a temporary agent started since the last poll counts.
        let Ok(live) = self.runtime.agents() else { return fail(ReasonCode::RuntimeDown, "the agent runtime (Herdr) is not connected".into()) };
        // An agent's question goes straight to the task's own agent while it is free (it knows
        // its work best); a working or blocked one is not interrupted: a temporary agent answers.
        let direct = {
            let store = self.store.lock().unwrap();
            let s = store.state();
            s.question(id).ok().filter(|q| q.by != "user").and_then(|q| s.task(&q.unit, q.task).ok()).and_then(|t| t.agent.clone())
        };
        if let Some(own) = direct.and_then(|own| live.iter().find(|a| a.session == own && matches!(a.state, AgentState::Idle | AgentState::Done)).cloned()) {
            if let Err(e) = self.record("workriver", Body::QuestionStarted { question: id.into(), agent: own.session.clone(), name: own.name.clone(), temporary: false }) {
                tracing::warn!("question {id}: {}", e.1);
            }
            self.deadlines.lock().unwrap().push((id.to_string(), std::time::Instant::now() + self.waits.answer));
            let text = {
                let store = self.store.lock().unwrap();
                let s = store.state();
                s.question(id).ok().and_then(|q| ask::direct(s, q))
            };
            if let Err(e) = self.runtime.send(&own.session, &text.unwrap_or_default()) {
                fail(ReasonCode::RuntimeError, format!("cannot give {} the question: {e}", own.name));
            }
            return;
        }
        // A follow-up goes to the thread's agent while it lives, which keeps its context.
        let current = {
            let store = self.store.lock().unwrap();
            let s = store.state();
            s.question(id).ok().and_then(|q| s.threads.iter().find(|t| t.id == q.thread)).and_then(|t| Some((t.id.clone(), t.agent.clone()?, t.name.clone()?)))
        };
        if let Some((thread, agent, name)) = current.filter(|(_, _, n)| live.iter().any(|a| &a.name == n)) {
            self.idle.lock().unwrap().retain(|(t, _, _)| *t != thread);
            if let Err(e) = self.record("workriver", Body::QuestionStarted { question: id.into(), agent, name: name.clone(), temporary: true }) {
                tracing::warn!("question {id}: {}", e.1);
            }
            self.deadlines.lock().unwrap().push((id.to_string(), std::time::Instant::now() + self.waits.answer));
            let text = self.store.lock().unwrap().state().question(id).ok().map(ask::follow_up).unwrap_or_default();
            if let Err(e) = self.runtime.send(&name, &text) {
                fail(ReasonCode::RuntimeError, format!("cannot give {name} the question: {e}"));
            }
            return;
        }
        let (q, unit, task_agent, manager, group) = {
            let store = self.store.lock().unwrap();
            let s = store.state();
            let Ok(q) = s.question(id).cloned() else { return };
            let Some(u) = s.unit_or_intent(&q.unit) else { return };
            let task_agent = s.task(&q.unit, q.task).ok().and_then(|t| t.agent.clone());
            let intent = s.intents.iter().find(|i| i.id == u.intent);
            let manager = intent.and_then(|i| s.manager_of(&i.id)).map(str::to_string);
            let group = intent.and_then(|i| i.workspace.clone());
            (q, u, task_agent, manager, group)
        };
        let live_one = |s: &Option<String>| s.as_ref().and_then(|s| live.iter().find(|a| &a.session == s)).cloned();
        let (worker, boss) = (live_one(&task_agent), live_one(&manager));
        // Where: the intent's workspace, else where its agents run.
        let Some(group) = group.or_else(|| worker.as_ref().and_then(|a| a.group.clone())).or_else(|| boss.as_ref().and_then(|a| a.group.clone())) else {
            return fail(ReasonCode::NoWorkspace, format!("intent {} follows no Herdr workspace and none of its agents runs", unit.intent));
        };
        // Which harness: the task's agent's, else the manager's, else claude.
        let known = |s: &Option<String>| s.as_ref().and_then(|s| self.store.lock().unwrap().state().agents.iter().find(|a| &a.session == s).map(|a| a.harness.clone()));
        let harness = worker.map(|a| a.harness).or_else(|| known(&task_agent)).or_else(|| boss.map(|a| a.harness)).or_else(|| known(&manager)).unwrap_or_else(|| "claude".into());
        // Intent-level work has no branch: its project folder is where to read.
        let dir = if unit.branch.is_empty() { unit.repo.clone() } else { scan::worktree_of(Path::new(&unit.repo), &unit.branch).ok().flatten().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| unit.repo.clone()) };
        let commits = if unit.branch.is_empty() { vec![] } else { task_commits(&unit, q.task) };
        let name = ask::agent_name(id);
        let agent = match self.runtime.start_temporary(&group.id, &harness, &name, &dir) {
            Ok(a) => a,
            Err(e) => return fail(if matches!(e, RuntimeError::None) { ReasonCode::RuntimeDown } else { ReasonCode::RuntimeError }, format!("cannot start {harness} as {name}: {e}")),
        };
        if let Err(e) = self.record("workriver", Body::QuestionStarted { question: id.into(), agent: agent.session.clone(), name: agent.name.clone(), temporary: true }) {
            tracing::warn!("question {id}: {}", e.1);
        }
        self.deadlines.lock().unwrap().push((id.to_string(), std::time::Instant::now() + self.waits.answer));
        let prompt = ask::prompt(self.store.lock().unwrap().state(), &q, &dir, &commits, &self.home).unwrap_or_default();
        if let Err(e) = self.runtime.send(&agent.name, &prompt) {
            fail(ReasonCode::RuntimeError, format!("cannot give {name} the question: {e}"));
        }
    }

    /// Records a question as failed (if still open) and closes its temporary agent, which
    /// did not answer.
    fn fail_question(&self, id: &str, code: ReasonCode, why: &str) {
        let body = Body::QuestionFailed { question: id.into(), reason_code: code, reason: why.into() };
        if let Err(e) = self.record("workriver", body) {
            tracing::warn!("question {id}: {}", e.1);
        }
        self.deadlines.lock().unwrap().retain(|(q, _)| q != id);
        // Only an agent started for it is closed; a task's own agent is not.
        let name = self.store.lock().unwrap().state().question(id).ok().filter(|q| q.temporary).and_then(|q| q.name.clone());
        if let Some(name) = name {
            self.close_later(name);
        }
    }

    /// An answer came: the thread's agent waits for a follow-up until it has been idle too long.
    fn answered(&self, id: &str) {
        self.deadlines.lock().unwrap().retain(|(q, _)| q != id);
        // An agent that asked hears the answer in its terminal.
        let back = {
            let store = self.store.lock().unwrap();
            let s = store.state();
            s.question(id).ok().filter(|q| q.by != "user").map(|q| (q.by.clone(), ask::answer_back(s, q)))
        };
        if let Some((asker, text)) = back
            && let Err(e) = self.runtime.send(&asker, &text)
        {
            tracing::warn!("cannot give {asker} the answer to {id}: {e}");
        }
        let found = {
            let store = self.store.lock().unwrap();
            let s = store.state();
            s.question(id).ok().and_then(|q| s.threads.iter().find(|t| t.id == q.thread)).map(|t| (t.id.clone(), t.name.clone()))
        };
        if let Some((thread, Some(name))) = found {
            let mut idle = self.idle.lock().unwrap();
            idle.retain(|(t, _, _)| *t != thread);
            idle.push((thread, name, std::time::Instant::now() + self.waits.idle));
        }
    }

    /// The user ended a thread: its agent goes now.
    fn end_thread(&self, thread: &str) {
        self.idle.lock().unwrap().retain(|(t, _, _)| t != thread);
        let name = self.store.lock().unwrap().state().threads.iter().find(|t| t.id == thread).and_then(|t| t.name.clone());
        if let Some(name) = name {
            self.close_later(name);
        }
    }

    /// Closes a temporary agent a moment later, so a `workriver answer` it runs can finish.
    fn close_later(&self, name: String) {
        let runtime = self.runtime.clone();
        std::thread::spawn(move || {
            std::thread::sleep(CLOSE_DELAY);
            if let Err(e) = runtime.close(&name) {
                tracing::warn!("cannot close {name}: {e}");
            }
        });
    }

    /// Sends the message a user's input calls for, and records how far it got.
    fn deliver(&self, e: &core::Event) {
        let live = self.live();
        let plan = deliver::plan(self.store.lock().unwrap().state(), live.as_deref(), &self.home, e);
        let Some(plan) = plan else { return };
        let delivery = match &plan.to {
            Err((code, reason)) => Delivery { step: DeliveryStep::NotDelivered, to: None, reason_code: Some(*code), reason: Some(reason.clone()), message: plan.message.clone() },
            Ok(to) => match self.runtime.send(to, &plan.message) {
                Ok(()) => Delivery { step: DeliveryStep::Delivered, to: Some(to.to_string()), reason_code: None, reason: None, message: plan.message.clone() },
                Err(err) => {
                    let code = if matches!(err, RuntimeError::None) { ReasonCode::RuntimeDown } else { ReasonCode::RuntimeError };
                    Delivery { step: DeliveryStep::NotDelivered, to: Some(to.to_string()), reason_code: Some(code), reason: Some(err.to_string()), message: plan.message.clone() }
                }
            },
        };
        if delivery.step == DeliveryStep::Delivered {
            let working = live.iter().flatten().any(|a| Some(&a.session) == delivery.to.as_ref() && a.state == AgentState::Working);
            self.pending.lock().unwrap().push(Pending {
                unit: plan.unit.clone(),
                task: plan.task,
                finding: plan.finding.clone(),
                chore: plan.chore.clone(),
                delivery: delivery.clone(),
                armed: !working,
                sent: std::time::Instant::now(),
            });
        }
        let body = Body::DeliveryUpdated { unit: plan.unit, task: plan.task, finding: plan.finding, chore: plan.chore, delivery };
        if let Err(e) = self.store.lock().unwrap().append("workriver", body) {
            tracing::warn!("cannot record a delivery: {e}");
        }
    }

    /// Who and where the CLI is: the agent at its runtime place (recorded by name the first
    /// time), and with `intent`, the intent following its group (created on first use).
    /// Who and where the CLI is: the agent at its runtime place (recorded by name the first
    /// time), its runtime group, and that group's current intent, if any.
    fn here(&self, place: &str) -> Result<serde_json::Value, ApiError> {
        let at = self.runtime.locate(place).map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
        if let Some(a) = &at.agent {
            let known = self.store.lock().unwrap().state().name_of(&a.session) == Some(a.name.as_str());
            if !known {
                self.record("workriver", Body::AgentSeen { session: a.session.clone(), name: a.name.clone(), harness: a.harness.clone() })?;
            }
        }
        let intent = at.group.as_ref().and_then(|g| self.store.lock().unwrap().state().intent_for(g).map(|i| i.id.clone()));
        Ok(json!({ "actor": at.agent.as_ref().map(|a| &a.session), "name": at.agent.as_ref().map(|a| &a.name), "group": at.group, "intent": intent }))
    }

    /// A project as given (`p1`, or a folder) as its folder: a registered project's, or the
    /// repository's top folder for a repository outside every registered project.
    fn project_folder(&self, given: &str) -> Result<String, ApiError> {
        let state = self.store.lock().unwrap().state().clone();
        if let Some(p) = state.projects.iter().find(|p| p.id == given) {
            return Ok(p.path.clone());
        }
        let path = std::fs::canonicalize(given).map_err(|e| bad(format!("project {given:?}: no such id or folder ({e})")))?;
        let path = path.to_string_lossy().into_owned();
        if let Some(p) = project_of(&state, &path) {
            return Ok(p.path.clone());
        }
        scan::toplevel(Path::new(&path)).map(|t| t.to_string_lossy().into_owned()).map_err(|_| bad(format!("{path} is neither a registered project nor a git repository; register it with `workriver project add`")))
    }
}

/// The registered project whose folder holds `path`, the deepest one.
pub fn project_of<'a>(state: &'a core::State, path: &str) -> Option<&'a core::Project> {
    state.projects.iter().filter(|p| Path::new(path).starts_with(&p.path)).max_by_key(|p| p.path.len())
}

pub fn router(app: Shared) -> Router {
    Router::new()
        .route("/api/workspace", get(workspace))
        .route("/api/state", get(state))
        .route("/api/status", get(status))
        .route("/api/events", get(events))
        .route("/api/append", post(append))
        .route("/api/decide", post(decide))
        .route("/api/gate", post(gate))
        .route("/api/chore-done", post(chore_done))
        .route("/api/focus", post(focus))
        .route("/api/here", post(here))
        .route("/api/ask", post(ask_why))
        .route("/api/thread/end", post(end_thread))
        .route("/api/attachments", post(attach).layer(DefaultBodyLimit::max(attachments::MAX)))
        .route("/api/attachments/inbox", post(inbox))
        .route("/api/attachments/{id}", get(attachment))
        .fallback(ui)
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .with_state(app)
}

/// Only this machine's browser and CLI, by name: the Host must be 127.0.0.1 or localhost
/// with our port (against DNS rebinding), and a POST from a page must come from our origin.
async fn guard(State(app): State<Shared>, req: Request, next: Next) -> Response {
    let ours = [format!("127.0.0.1:{}", app.port), format!("localhost:{}", app.port)];
    let allowed = {
        let header = |h| req.headers().get(h).map(|v| v.to_str().unwrap_or(""));
        let host_ok = header(header::HOST).is_some_and(|h| ours.iter().any(|o| o == h));
        let origin_ok = req.method() == Method::GET || header(header::ORIGIN).is_none_or(|o| ours.iter().any(|h| o == format!("http://{h}")));
        host_ok && origin_ok
    };
    if allowed {
        next.run(req).await
    } else {
        (StatusCode::FORBIDDEN, "workriver answers only on 127.0.0.1 or localhost, from its own page").into_response()
    }
}

/// An error answered as plain text: a broken rule is the caller's (422), anything else ours (500).
pub struct ApiError(StatusCode, String);

fn bad(msg: String) -> ApiError {
    ApiError(StatusCode::UNPROCESSABLE_ENTITY, msg)
}

impl From<core::Error> for ApiError {
    fn from(e: core::Error) -> Self {
        match e {
            core::Error::Rejected(r) => bad(r.0),
            e => ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        }
    }
}

impl From<core::Rejected> for ApiError {
    fn from(r: core::Rejected) -> Self {
        bad(r.0)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}

/// Runs git and the store off the async threads.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, ApiError> + Send + 'static) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(f).await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
}

async fn workspace(State(app): State<Shared>) -> Result<Json<snapshot::Workspace>, ApiError> {
    blocking(move || {
        let state = app.store.lock().unwrap().state().clone();
        let mut cache = app.cache.lock().unwrap();
        let blocked = app.blocked.lock().unwrap().clone();
        Ok(Json(snapshot::build(&state, &mut cache, app.live().as_deref(), &blocked)))
    })
    .await
}

async fn state(State(app): State<Shared>) -> Json<core::State> {
    Json(app.store.lock().unwrap().state().clone())
}

async fn status(State(app): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    let store = app.store.lock().unwrap();
    let s = store.state();
    let (files, bytes) = attachments::usage(&app.home);
    Ok(Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "port": app.port,
        "db": store.path(),
        "events": store.count()?,
        "intents": s.intents.len(),
        "units": s.units.len(),
        "runtime": { "connected": app.live().is_some(), "warning": app.warning },
        "attachments": { "dir": attachments::dir(&app.home), "files": files, "bytes": bytes },
    })))
}

async fn events(State(app): State<Shared>) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    // A lagging client missed some, which is still one change.
    let stream = BroadcastStream::new(app.changed.subscribe()).map(|_| Ok(SseEvent::default().event("changed").data("changed")));
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[derive(Deserialize)]
struct Append {
    actor: String,
    kind: String,
    #[serde(default)]
    body: serde_json::Value,
}

/// Any event, as the CLI records it.
async fn append(State(app): State<Shared>, Json(a): Json<Append>) -> Result<Json<core::Event>, ApiError> {
    let body: Body = serde_json::from_value(json!({ "kind": a.kind, "body": a.body })).map_err(|e| bad(format!("{} event: {e}", a.kind)))?;
    blocking(move || app.record(&a.actor, body).map(Json)).await
}

// ---- the user's inputs from the page; `App::record` delivers them to the agents ----

#[derive(Deserialize)]
struct Decide {
    unit: String,
    task: u32,
    finding: String,
    decision: core::Decision,
    #[serde(default)]
    note: String,
}

async fn decide(State(app): State<Shared>, Json(d): Json<Decide>) -> Result<Json<core::Event>, ApiError> {
    let body = Body::FindingDecided {
        unit: d.unit,
        task: d.task,
        finding: d.finding,
        decision: d.decision,
        note: d.note,
        by: core::Who::User,
    };
    blocking(move || app.record("user", body).map(Json)).await
}

#[derive(Deserialize)]
struct Gate {
    unit: String,
    task: u32,
    result: core::GateResult,
    #[serde(default)]
    note: String,
    #[serde(default)]
    findings: Vec<core::NewFinding>,
    #[serde(default)]
    attachments: Vec<String>,
}

async fn gate(State(app): State<Shared>, Json(g): Json<Gate>) -> Result<Json<core::Event>, ApiError> {
    let body = Body::GateSubmitted { unit: g.unit, task: g.task, result: g.result, note: g.note, findings: g.findings, attachments: g.attachments };
    blocking(move || app.record("user", body).map(Json)).await
}

#[derive(Deserialize)]
struct ChoreDone {
    chore: String,
}

async fn chore_done(State(app): State<Shared>, Json(c): Json<ChoreDone>) -> Result<Json<core::Event>, ApiError> {
    blocking(move || app.record("user", Body::ChoreDone { chore: c.chore }).map(Json)).await
}

#[derive(Deserialize)]
struct Focus {
    agent: String,
}

async fn focus(State(app): State<Shared>, Json(f): Json<Focus>) -> Result<StatusCode, ApiError> {
    blocking(move || {
        // The page knows agents by name.
        app.runtime.focus(&app.session_of(&f.agent, None)).map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

#[derive(Deserialize)]
struct Here {
    place: String,
}

/// The CLI asks who and where it is: see `App::here`.
#[derive(Deserialize)]
struct Ask {
    /// Who asks: an agent's session (from the CLI), or the user (the page).
    #[serde(default)]
    by: Option<String>,
    unit: String,
    task: u32,
    text: String,
    /// The thread it continues; none starts a new one.
    #[serde(default)]
    thread: Option<String>,
    #[serde(default)]
    context: Vec<core::AskContext>,
    /// Ask the thread's agent for the conclusion instead.
    #[serde(default)]
    conclude: bool,
}

#[derive(Deserialize)]
struct Upload {
    name: String,
    #[serde(default)]
    by: Option<String>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    task: Option<u32>,
    #[serde(default)]
    finding: Option<String>,
}

/// A file into the store, its body raw (`?name=…`, the type from Content-Type), optionally
/// linked to a task or a finding. Answers its id and path.
async fn attach(State(app): State<Shared>, Query(u): Query<Upload>, headers: axum::http::HeaderMap, body: axum::body::Bytes) -> Result<Json<serde_json::Value>, ApiError> {
    let given = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(str::to_string);
    blocking(move || {
        // Only the last part of the name: it is shown, never used as a path.
        let name = Path::new(&u.name).file_name().map(|n| n.to_string_lossy().into_owned()).filter(|n| !n.is_empty()).ok_or_else(|| bad("an attachment needs a file name".into()))?;
        let mime = attachments::mime(&name, given.as_deref());
        let (id, path) = attachments::put(&app.home, &body, &name).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("cannot store {name}: {e}")))?;
        let by = u.by.unwrap_or_else(|| "user".into());
        let size = body.len() as u64;
        app.record(&by, Body::AttachmentAdded { id: id.clone(), name: name.clone(), mime: mime.clone(), size, unit: u.unit, task: u.task, finding: u.finding })?;
        Ok(Json(json!({ "id": id, "path": path, "name": name, "type": mime, "size": size })))
    })
    .await
}

/// A stored file. Only images show in the page; anything else downloads, and nothing is sniffed.
async fn attachment(State(app): State<Shared>, UrlPath(id): UrlPath<String>) -> Result<Response, ApiError> {
    let found = app.store.lock().unwrap().state().attachments.iter().find(|a| a.id == id).cloned();
    let a = found.ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("no attachment {id}")))?;
    let file = attachments::path(&app.home, &a.id, &a.name);
    let bytes = blocking(move || std::fs::read(&file).map_err(|e| ApiError(StatusCode::NOT_FOUND, format!("attachment {id}: {e}")))).await?;
    let image = a.mime.starts_with("image/") && a.mime != "image/svg+xml";
    let disposition = if image { "inline".to_string() } else { format!("attachment; filename=\"{}\"", a.name.replace(['"', '\\', '\r', '\n'], "_")) };
    let mime = if image { a.mime.clone() } else { "application/octet-stream".to_string() };
    Ok(([(header::CONTENT_TYPE, mime), (header::CONTENT_DISPOSITION, disposition), (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string())], bytes).into_response())
}

#[derive(Deserialize)]
struct Inbox {
    unit: String,
    task: u32,
}

/// A task's staging folder for screenshots and the like, made if needed.
async fn inbox(State(app): State<Shared>, Json(i): Json<Inbox>) -> Result<Json<serde_json::Value>, ApiError> {
    app.store.lock().unwrap().state().task(&i.unit, i.task)?;
    let d = attachments::inbox(&app.home, &i.unit, i.task);
    attachments::mkdir(&d).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("cannot make {}: {e}", d.display())))?;
    Ok(Json(json!({ "dir": d })))
}

#[derive(Deserialize)]
struct EndThread {
    thread: String,
}

async fn end_thread(State(app): State<Shared>, Json(t): Json<EndThread>) -> Result<Json<core::Event>, ApiError> {
    blocking(move || app.record("user", Body::ThreadEnded { thread: t.thread }).map(Json)).await
}

/// The user asks why: recorded at once; the temporary agent starts in the background.
async fn ask_why(State(app): State<Shared>, Json(a): Json<Ask>) -> Result<Json<core::Event>, ApiError> {
    let body = Body::QuestionAsked { id: String::new(), thread: a.thread.unwrap_or_default(), unit: a.unit, task: a.task, text: a.text, context: a.context, conclude: a.conclude };
    let starter = app.clone();
    let by = a.by.unwrap_or_else(|| "user".into());
    let e = blocking(move || app.record(&by, body)).await?;
    if let Body::QuestionAsked { id, .. } = &e.body {
        let id = id.clone();
        tokio::task::spawn_blocking(move || starter.start_question(&id));
    }
    Ok(Json(e))
}

/// A task's commits on its unit's branch, as (short id, subject).
fn task_commits(u: &core::Unit, task: u32) -> Vec<(String, String)> {
    let repo = Path::new(&u.repo);
    let hashes = if u.merged.is_some() { u.merged_commits.clone() } else { scan::commit_list(repo, &u.base, &u.branch).unwrap_or_default() };
    let want = format!("{}#{task}", u.id);
    hashes.iter().filter_map(|h| scan::commit(repo, h).ok()).filter(|c| c.task.as_deref() == Some(want.as_str())).map(|c| (c.id, c.subject)).collect()
}

async fn here(State(app): State<Shared>, Json(h): Json<Here>) -> Result<Json<serde_json::Value>, ApiError> {
    blocking(move || app.here(&h.place).map(Json)).await
}

// ---- the web UI, built into the binary (read from disk in debug builds) ----

#[derive(rust_embed::Embed)]
#[folder = "../../frontend/web/dist/"]
#[allow_missing = true]
struct Ui;

async fn ui(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    // Any other path is a page of the app: index.html.
    let file = Ui::get(path).filter(|_| !path.is_empty()).or_else(|| Ui::get("index.html"));
    match file {
        Some(f) => ([(header::CONTENT_TYPE, f.metadata.mimetype().to_string())], f.data).into_response(),
        None => (StatusCode::NOT_FOUND, "The web UI is not built: run `npm run build` in frontend/web, then build workriver again.").into_response(),
    }
}

/// Binds 127.0.0.1:`port`, or says plainly why not.
pub async fn bind(port: u16) -> anyhow::Result<tokio::net::TcpListener> {
    tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .with_context(|| format!("cannot listen on 127.0.0.1:{port}; is workriver (or something else) already running there? Pick another with --port"))
}
