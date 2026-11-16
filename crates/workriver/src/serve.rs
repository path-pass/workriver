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
use axum::{Extension, Json, Router};
use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast;
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};
use workriver_core::runtime::{AgentState, Runtime, RuntimeAgent, RuntimeError};
use workriver_core::{self as core, Body, Delivery, DeliveryStep, Principal, ReasonCode, Store};
use workriver_scan as scan;

use crate::machines::Machines;
use crate::auth::{self, Auth};
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
    /// The other machines, when the runtime follows any.
    machines: Option<Arc<Machines>>,
    /// Each followed machine's own listener, to end it when the machine is no longer followed.
    listeners: Mutex<std::collections::HashMap<String, tokio::task::AbortHandle>>,
    /// The runtime's groups as last polled, with or without agents.
    groups: Mutex<Vec<core::RuntimeRef>>,
    /// Who the user is: the token, sessions of the page, one-time codes.
    auth: Auth,
    /// A caller may name the agent it records as (`--as`): for tests and development only.
    allow_as: bool,
}

/// What a request was found to be, before any handler sees it.
#[derive(Clone)]
struct Caller {
    /// The followed machine whose listener it came in on; none: this machine's.
    machine: Option<String>,
    port: u16,
    /// It carries the user token, or a session of the page from our own origin. Never on a
    /// machine's listener: a caller on another machine is never the user.
    user: bool,
}

/// What else the daemon is started with.
pub struct Options {
    pub warning: Option<String>,
    pub waits: Waits,
    pub machines: Option<Arc<Machines>>,
    pub allow_as: bool,
}

/// Which listener a request came in on: this machine's, or a followed machine's own (the
/// far end of its tunnel), which is how the daemon knows where a caller is, whatever it says.
#[derive(Clone)]
struct Via {
    machine: Option<String>,
    /// The port the caller used: ours, or the machine's port that leads here.
    port: u16,
    /// The listener's own port.
    local: u16,
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

/// As the tests start it: callers may say which agent they are.
#[cfg(test)]
pub async fn serve(listener: tokio::net::TcpListener, store: Store, runtime: Arc<dyn Runtime>, warning: Option<String>, waits: Waits) -> anyhow::Result<()> {
    serve_with(listener, store, runtime, Options { warning, waits, machines: None, allow_as: true }).await
}

/// Following other machines too when `machines` is given (`runtime` is then `machines` itself).
pub async fn serve_with(listener: tokio::net::TcpListener, store: Store, runtime: Arc<dyn Runtime>, options: Options) -> anyhow::Result<()> {
    let Options { warning, waits, machines, allow_as } = options;
    let port = listener.local_addr()?.port();
    let store_home = store.path().parent().map(Path::to_path_buf).unwrap_or_default();
    let (changed, _) = broadcast::channel(16);
    let auth = Auth::open(&store_home).with_context(|| format!("cannot make or read the user token in {}", store_home.display()))?;
    if allow_as {
        tracing::warn!("WORKRIVER_ALLOW_AS is set: any local caller may record as any agent (--as). For tests and development only.");
    }
    let app = Arc::new(App {
        auth,
        allow_as,
        store: Mutex::new(store),
        cache: Mutex::new(Cache::following(machines.clone())),
        machines,
        listeners: Mutex::default(),
        groups: Mutex::default(),
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
    let units: Vec<(String, String)> = app.store.lock().unwrap().state().units.iter().filter(|u| u.machine.is_none()).map(|u| (u.repo.clone(), u.branch.clone())).collect();
    for (repo, branch) in units {
        app.watch(Path::new(&repo), &branch);
    }
    for (label, setting) in app.machines.iter().flat_map(|m| m.wanted()) {
        if let Err(e) = follow(&app, &label, setting).await {
            tracing::warn!("machine {label}: {}", e.1);
        }
    }
    // Machines already there (stand-ins in tests) get their listener only.
    #[cfg(test)]
    for m in app.machines.iter().flat_map(|m| m.states()) {
        let _ = listen(&app, &m.label, m.port).await;
    }
    // Repositories on other machines cannot be watched: the page asks again now and then.
    let remote = app.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(snapshot::REMOTE);
        loop {
            tick.tick().await;
            if remote.store.lock().unwrap().state().units.iter().any(|u| u.machine.is_some() && u.merged.is_none()) {
                let _ = remote.changed.send(());
            }
        }
    });
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
    // Nothing that signs anyone in is ever written here: this output is on a screen (a
    // Herdr pane) that agents can read. The user opens the page with `workriver open`.
    tracing::info!("workriver on http://127.0.0.1:{port}  (open it as you with `workriver open`, from a terminal agents do not read)");
    axum::serve(listener, router(app, Via { machine: None, port, local: port })).with_graceful_shutdown(async { tokio::signal::ctrl_c().await.ok(); }).await?;
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
        let here = scan::Runner::Local;
        let mut paths = vec![repo.to_path_buf()];
        paths.extend(scan::common_dir(&here, repo).ok());
        paths.extend(scan::worktree_of(&here, repo, branch).ok().flatten());
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
        let groups = self.runtime.groups().unwrap_or_default();
        let regrouped = std::mem::replace(&mut *self.groups.lock().unwrap(), groups.clone()) != groups;
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
            if let Err(e) = self.store.lock().unwrap().append(&Principal::Service, body) {
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
        if differs || recorded || regrouped {
            let _ = self.changed.send(());
        }
    }

    fn live(&self) -> Option<Vec<RuntimeAgent>> {
        self.live.lock().unwrap().clone()
    }

    /// Where git runs for a repository on `machine`; an error while it cannot be reached.
    fn runner(&self, machine: Option<&str>) -> Result<scan::Runner, ApiError> {
        match machine {
            None => Ok(scan::Runner::Local),
            Some(m) => self.machines.as_ref().and_then(|x| x.runner(m)).ok_or_else(|| ApiError(StatusCode::SERVICE_UNAVAILABLE, format!("machine {m} cannot be reached now; see `workriver machine list`"))),
        }
    }

    /// A repository's top folder on `machine`, checked there to be one and to have `base`.
    fn repository(&self, machine: Option<&str>, repo: &str, base: &str) -> Result<String, ApiError> {
        let at = self.runner(machine)?;
        let on = machine.map(|m| format!(" on {m}")).unwrap_or_default();
        let top = scan::toplevel(&at, Path::new(repo)).map_err(|e| bad(format!("{repo}{on} is not a git repository: {e}")))?;
        if !scan::has_rev(&at, &top, base) {
            return Err(bad(format!("base {base:?} is not a commit or branch in {}{on}", top.display())));
        }
        Ok(top.to_string_lossy().into_owned())
    }

    /// Sends a message to an agent. One on another machine cannot open files of this
    /// machine: paths into the attachment store become links through its tunnel.
    fn send(&self, to: &str, text: &str) -> Result<(), RuntimeError> {
        let machine = self.live().unwrap_or_default().into_iter().find(|a| a.session == to || a.name == to).and_then(|a| a.machine);
        match machine.and_then(|m| self.machines.as_ref()?.port(&m)) {
            Some(port) => self.runtime.send(to, &crate::machines::links(text, &format!("{}/", attachments::dir(&self.home).display()), port)),
            None => self.runtime.send(to, text),
        }
    }

    /// Who a request acts as. An agent is the agent at the caller's place in the runtime (on
    /// the machine the request came from), whatever else it carries; the user is a caller at
    /// no agent's place that carries the token or a session. Nobody is anyone by saying so,
    /// except, where the daemon was started to allow it, an agent named with `--as`.
    fn who(&self, caller: &Caller, place: Option<&str>, named: Option<&str>) -> Result<Principal, ApiError> {
        let machine = caller.machine.as_deref();
        if let Some(name) = named.filter(|n| !n.is_empty()) {
            if !self.allow_as {
                return Err(ApiError(StatusCode::FORBIDDEN, "--as is off: workriver takes the agent from the Herdr pane the command runs in (for tests, start the service with WORKRIVER_ALLOW_AS=1)".into()));
            }
            if matches!(name, "user" | "workriver") {
                return Err(ApiError(StatusCode::FORBIDDEN, format!("nobody records as {name:?} by saying so")));
            }
            return Ok(Principal::Agent { session: self.session_of(name, None), machine: machine.map(str::to_string) });
        }
        if let Some(place) = place.filter(|p| !p.is_empty()) {
            let at = self.runtime.locate(machine, place).map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, crate::herdr::cannot_ask(place, &e.to_string())))?;
            if let Some(a) = at.agent {
                self.seen(&a)?;
                return Ok(Principal::Agent { session: a.session, machine: a.machine });
            }
            if machine.is_some() || !caller.user {
                return Err(ApiError(StatusCode::UNAUTHORIZED, crate::herdr::no_agent_at(place)));
            }
        }
        self.user(caller)
    }

    /// The user, or a refusal that says how to be the user.
    fn user(&self, caller: &Caller) -> Result<Principal, ApiError> {
        match (&caller.machine, caller.user) {
            (None, true) => Ok(Principal::User),
            (Some(m), _) => Err(ApiError(StatusCode::FORBIDDEN, format!("a caller on machine {m} is an agent of that machine, never the user"))),
            (None, false) => Err(ApiError(StatusCode::UNAUTHORIZED, NOT_THE_USER.into())),
        }
    }

    /// Records the name a live agent goes by, the first time and when it changes.
    fn seen(&self, a: &RuntimeAgent) -> Result<(), ApiError> {
        let known = self.store.lock().unwrap().state().name_of(&a.session) == Some(a.name.as_str());
        if !known {
            self.record(&Principal::Service, Body::AgentSeen { session: a.session.clone(), name: a.name.clone(), harness: a.harness.clone() })?;
        }
        Ok(())
    }

    /// Records an event as `who`, found by `App::who`, never taken from the caller's word.
    fn record(&self, who: &Principal, body: Body) -> Result<core::Event, ApiError> {
        let (actor, machine) = (who.actor(), who.machine());
        let body = self.complete(actor, body, machine)?;
        let e = self.store.lock().unwrap().append(who, body)?;
        self.deliver(&e);
        match &e.body {
            Body::QuestionAnswered { question, .. } => self.answered(question),
            Body::ThreadEnded { thread } => self.end_thread(thread),
            _ => {}
        }
        // A unit's branch may have got a work tree of its own since; watching again is cheap.
        if let (_, Some(unit), _) = e.body.refs() {
            let found = self.store.lock().unwrap().state().unit(&unit).ok().filter(|u| u.machine.is_none()).map(|u| (u.repo.clone(), u.branch.clone()));
            if let Some((repo, branch)) = found {
                self.watch(Path::new(&repo), &branch);
            }
        }
        let _ = self.changed.send(());
        Ok(e)
    }

    /// What only git can check or fill in before an event is recorded.
    fn complete(&self, actor: &str, body: Body, machine: Option<&str>) -> Result<Body, ApiError> {
        Ok(match body {
            // ponytail: a project is a folder of this machine; a repository on another
            // machine is a project of its own. Give projects a machine when that is not enough.
            Body::ProjectAdded { .. } if machine.is_some() => return Err(bad("register project folders on the machine workriver runs on; a repository on another machine is a project of its own".into())),
            Body::ProjectAdded { id, path, title } => {
                let folder = std::fs::canonicalize(&path).map_err(|e| bad(format!("{path}: {e}")))?;
                let title = if title.trim().is_empty() { folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() } else { title };
                Body::ProjectAdded { id, path: folder.to_string_lossy().into_owned(), title }
            }
            Body::FindingAdded { project, intent, finding, source, note } => Body::FindingAdded { project: self.project_folder(&project, machine)?, intent, finding, source, note },
            Body::IntentCreated { id, title, goal, runtime_ref, project, state, note, external } => {
                let project = if project.is_empty() { project } else { self.project_folder(&project, machine)? };
                Body::IntentCreated { id, title, goal, runtime_ref, project, state, note, external }
            }
            // The repository is where the work happens: on the machine named, else on the
            // one the unit was recorded from. It is checked there, over ssh for another machine.
            Body::UnitCreated { id, intent, title, purpose, repo, branch, base, machine: named } => {
                let machine = place(named.as_deref(), machine);
                let top = self.repository(machine.as_deref(), &repo, &base)?;
                Body::UnitCreated { id, intent, title, purpose, repo: top, branch, base, machine }
            }
            // What is not given stays as it is; `machine` is always said (the CLI sends it).
            Body::UnitRelocated { unit, machine: named, repo, branch, base } => {
                let u = self.store.lock().unwrap().state().unit(&unit)?.clone();
                let machine = match named.as_deref() {
                    None => u.machine.clone(),
                    named => place(named, None),
                };
                let (repo, branch, base) = (if repo.is_empty() { u.repo } else { repo }, if branch.is_empty() { u.branch } else { branch }, if base.is_empty() { u.base } else { base });
                let top = self.repository(machine.as_deref(), &repo, &base)?;
                Body::UnitRelocated { unit, machine, repo: top, branch, base }
            }
            // A chore of no intent belongs to a project, if one is given or can be told.
            Body::ChoreCreated { id, intent, project, unit, title, steps } if intent.is_empty() && !project.is_empty() => {
                let project = self.project_folder(&project, machine).unwrap_or_default();
                Body::ChoreCreated { id, intent, project, unit, title, steps }
            }
            Body::UnitMerged { unit, commits } if commits.is_empty() => {
                // The branch's commits now, or as last seen if they are already merged away.
                let u = self.store.lock().unwrap().state().unit(&unit)?.clone();
                let now = self.runner(u.machine.as_deref()).ok().and_then(|at| scan::commit_list(&at, Path::new(&u.repo), &u.base, &u.branch).ok()).unwrap_or_default();
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
            if let Err(e) = self.record(&Principal::Service, Body::QuestionStarted { question: id.into(), agent: own.session.clone(), name: own.name.clone(), temporary: false }) {
                tracing::warn!("question {id}: {}", e.1);
            }
            self.deadlines.lock().unwrap().push((id.to_string(), std::time::Instant::now() + self.waits.answer));
            let text = {
                let store = self.store.lock().unwrap();
                let s = store.state();
                s.question(id).ok().and_then(|q| ask::direct(s, q))
            };
            if let Err(e) = self.send(&own.session, &text.unwrap_or_default()) {
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
            if let Err(e) = self.record(&Principal::Service, Body::QuestionStarted { question: id.into(), agent, name: name.clone(), temporary: true }) {
                tracing::warn!("question {id}: {}", e.1);
            }
            self.deadlines.lock().unwrap().push((id.to_string(), std::time::Instant::now() + self.waits.answer));
            let text = {
                let store = self.store.lock().unwrap();
                store.state().question(id).ok().map(|q| ask::follow_up(store.state(), q)).unwrap_or_default()
            };
            if let Err(e) = self.send(&name, &text) {
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
        // Work on another machine is read there: the group of that machine that is the
        // intent's workspace (linked to it), else where the task's agent runs.
        let group = match &unit.machine {
            None => group,
            Some(m) => {
                let state = self.store.lock().unwrap().state().clone();
                let same = |g: &core::RuntimeRef| group.as_ref().is_some_and(|w| state.workspace_of(g).is_some_and(|a| state.workspace_of(w).is_some_and(|b| a.group == b.group)));
                let there = |g: &&core::RuntimeRef| g.machine.as_ref() == Some(m);
                let groups = self.groups.lock().unwrap().clone();
                groups.iter().filter(there).find(|g| same(g)).cloned().or_else(|| worker.as_ref().and_then(|a| a.group.clone()).filter(|g| there(&g)))
            }
        };
        let Some(group) = group.or_else(|| worker.as_ref().and_then(|a| a.group.clone())).or_else(|| boss.as_ref().and_then(|a| a.group.clone())) else {
            return fail(ReasonCode::NoWorkspace, format!("intent {} follows no Herdr workspace and none of its agents runs", unit.intent));
        };
        // Which harness: the task's agent's, else the manager's, else claude.
        let known = |s: &Option<String>| s.as_ref().and_then(|s| self.store.lock().unwrap().state().agents.iter().find(|a| &a.session == s).map(|a| a.harness.clone()));
        let harness = worker.map(|a| a.harness).or_else(|| known(&task_agent)).or_else(|| boss.map(|a| a.harness)).or_else(|| known(&manager)).unwrap_or_else(|| "claude".into());
        // Intent-level work has no branch: its project folder is where to read.
        let at = self.runner(unit.machine.as_deref()).ok().filter(|_| !unit.branch.is_empty());
        let dir = at.as_ref().and_then(|at| scan::worktree_of(at, Path::new(&unit.repo), &unit.branch).ok().flatten()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| unit.repo.clone());
        let commits = at.map(|at| task_commits(&at, &unit, q.task)).unwrap_or_default();
        let name = ask::agent_name(id);
        let agent = match self.runtime.start_temporary(&group.id, &harness, &name, &dir) {
            Ok(a) => a,
            Err(e) => return fail(if matches!(e, RuntimeError::None) { ReasonCode::RuntimeDown } else { ReasonCode::RuntimeError }, format!("cannot start {harness} as {name}: {e}")),
        };
        if let Err(e) = self.record(&Principal::Service, Body::QuestionStarted { question: id.into(), agent: agent.session.clone(), name: agent.name.clone(), temporary: true }) {
            tracing::warn!("question {id}: {}", e.1);
        }
        self.deadlines.lock().unwrap().push((id.to_string(), std::time::Instant::now() + self.waits.answer));
        let prompt = ask::prompt(self.store.lock().unwrap().state(), &q, &dir, &commits, &self.home).unwrap_or_default();
        if let Err(e) = self.send(&agent.name, &prompt) {
            fail(ReasonCode::RuntimeError, format!("cannot give {name} the question: {e}"));
        }
    }

    /// Records a question as failed (if still open) and closes its temporary agent, which
    /// did not answer.
    fn fail_question(&self, id: &str, code: ReasonCode, why: &str) {
        let body = Body::QuestionFailed { question: id.into(), reason_code: code, reason: why.into() };
        if let Err(e) = self.record(&Principal::Service, body) {
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
            && let Err(e) = self.send(&asker, &text)
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
            Ok(to) => match self.send(to, &plan.message) {
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
        if let Err(e) = self.store.lock().unwrap().append(&Principal::Service, body) {
            tracing::warn!("cannot record a delivery: {e}");
        }
    }

    /// Who and where the CLI is: the agent at its runtime place (recorded by name the first
    /// time), and with `intent`, the intent following its group (created on first use).
    /// Who and where the CLI is: the agent at its runtime place (recorded by name the first
    /// time), its runtime group, and that group's current intent, if any.
    fn here(&self, place: &str, machine: Option<&str>) -> Result<serde_json::Value, ApiError> {
        let at = self.runtime.locate(machine, place).map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
        if let Some(a) = &at.agent {
            self.seen(a)?;
        }
        let intent = at.group.as_ref().and_then(|g| self.store.lock().unwrap().state().intent_for(g).map(|i| i.id.clone()));
        Ok(json!({ "actor": at.agent.as_ref().map(|a| &a.session), "name": at.agent.as_ref().map(|a| &a.name), "group": at.group, "intent": intent }))
    }

    /// A project as given (`p1`, or a folder) as its folder: a registered project's, or the
    /// repository's top folder for a repository outside every registered project.
    fn project_folder(&self, given: &str, machine: Option<&str>) -> Result<String, ApiError> {
        let state = self.store.lock().unwrap().state().clone();
        if let Some(p) = state.projects.iter().find(|p| p.id == given) {
            return Ok(p.path.clone());
        }
        // On another machine: the repository's top folder there.
        if machine.is_some() {
            let at = self.runner(machine)?;
            return scan::toplevel(&at, Path::new(given)).map(|t| t.to_string_lossy().into_owned()).map_err(|_| bad(format!("{given} is not a git repository on that machine; name a project of this machine by its id (p1)")));
        }
        let path = std::fs::canonicalize(given).map_err(|e| bad(format!("project {given:?}: no such id or folder ({e})")))?;
        let path = path.to_string_lossy().into_owned();
        if let Some(p) = project_of(&state, &path) {
            return Ok(p.path.clone());
        }
        scan::toplevel(&scan::Runner::Local, Path::new(&path)).map(|t| t.to_string_lossy().into_owned()).map_err(|_| bad(format!("{path} is neither a registered project nor a git repository; register it with `workriver project add`")))
    }
}

/// The registered project whose folder holds `path`, the deepest one.
pub fn project_of<'a>(state: &'a core::State, path: &str) -> Option<&'a core::Project> {
    state.projects.iter().filter(|p| Path::new(path).starts_with(&p.path)).max_by_key(|p| p.path.len())
}

/// The machine a unit is put on: the one named ("local": the one workriver runs on), else
/// the caller's.
fn place(named: Option<&str>, callers: Option<&str>) -> Option<String> {
    match named {
        Some("local") => None,
        Some(m) => Some(m.to_string()),
        None => callers.map(str::to_string),
    }
}

/// What a caller that is nobody is told.
const NOT_THE_USER: &str = "workriver answers the user, and agents from their own Herdr pane; this request does not show it comes from either. As the user: in the browser, open workriver with `workriver open`; with the CLI, use a terminal that is not an agent's pane, on the machine workriver runs on (it reads the user token there).";

/// Every route, on this machine's listener. Nothing answers a caller that is nobody: what
/// acts as the user or changes where workriver reaches asks for the user in its handler
/// (`App::user`); recording and reading ask who the caller is (`App::who`, `App::view`: the
/// user, or the agent at the caller's place, which on a blind task sees only its own). Only
/// `status`, `here` and the session routes answer anyone local.
fn router(app: Shared, via: Via) -> Router {
    Router::new()
        .route("/api/session", get(session).post(session_start))
        .route("/api/session/code", post(session_code))
        .route("/api/machines", get(machines).post(machine_set))
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
        .layer(Extension(via))
        .with_state(app)
}

/// The routes of a followed machine's listener: only what an agent's CLI over there needs.
/// An allow-list: who it is, recording as that agent, asking, attachments, and reading the
/// board. Nothing here is ever done as the user (no session, no token), and what the user
/// alone does (decisions, gates, machines, links, projects, focus) is not routed at all.
fn machine_router(app: Shared, via: Via) -> Router {
    Router::new()
        .route("/api/here", post(here))
        .route("/api/append", post(append))
        .route("/api/ask", post(ask_why))
        .route("/api/attachments", post(attach).layer(DefaultBodyLimit::max(attachments::MAX)))
        .route("/api/attachments/inbox", post(inbox))
        .route("/api/attachments/{id}", get(attachment))
        .route("/api/workspace", get(workspace))
        .route("/api/state", get(state))
        .route("/api/status", get(status))
        .fallback(|| async { (StatusCode::FORBIDDEN, "not available to another machine: its agents record their own work, ask, attach and read the board; everything else is done on the machine workriver runs on") })
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .layer(Extension(via))
        .with_state(app)
}

/// Only this machine's browser and CLI, by name: the Host must be 127.0.0.1 or localhost
/// with our port (against DNS rebinding), and a POST from a page must come from our origin.
/// On a followed machine's listener the port is the one its CLI uses over there.
///
/// Also works out whether the request is the user's: it carries the user token; or a
/// session cookie, which counts only from our own page. Against a forged request from
/// another site we rely on the cookie being SameSite=Strict (the browser does not send it
/// along) and, for anything but a GET, on the Origin header being there and ours.
async fn guard(State(app): State<Shared>, Extension(via): Extension<Via>, mut req: Request, next: Next) -> Response {
    let ours = [via.port, via.local].map(|p| [format!("127.0.0.1:{p}"), format!("localhost:{p}")]).concat();
    let (allowed, user) = {
        let header = |h| req.headers().get(h).map(|v| v.to_str().unwrap_or(""));
        let host_ok = header(header::HOST).is_some_and(|h| ours.iter().any(|o| o == h));
        let our_origin = header(header::ORIGIN).map(|o| ours.iter().any(|h| o == format!("http://{h}")));
        let get = req.method() == Method::GET;
        let origin_ok = get || our_origin.unwrap_or(true);
        let session = app.auth.session(header(header::COOKIE)) && (get || our_origin == Some(true));
        (host_ok && origin_ok, via.machine.is_none() && (app.auth.bearer(header(header::AUTHORIZATION)) || session))
    };
    if !allowed {
        return (StatusCode::FORBIDDEN, "workriver answers only on 127.0.0.1 or localhost, from its own page").into_response();
    }
    req.extensions_mut().insert(Caller { machine: via.machine, port: via.port, user });
    next.run(req).await
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

/// Who reads: the caller's place in the runtime, as for what it records.
#[derive(Deserialize)]
struct Reader {
    #[serde(default)]
    place: Option<String>,
    #[serde(default, rename = "as")]
    named: Option<String>,
}

impl App {
    /// The records as the caller may see them: all of them for the user and for an agent,
    /// except an agent on a blind task, which gets `State::blind_view` until it delivers.
    /// Nobody else reads: a request that is neither the user's nor from an agent's place is
    /// refused.
    fn view(&self, caller: &Caller, r: &Reader) -> Result<core::State, ApiError> {
        let who = self.who(caller, r.place.as_deref(), r.named.as_deref())?;
        let store = self.store.lock().unwrap();
        Ok(match &who {
            Principal::Agent { session, .. } => store.state().blind_view(session).unwrap_or_else(|| store.state().clone()),
            _ => store.state().clone(),
        })
    }
}

async fn workspace(State(app): State<Shared>, Extension(caller): Extension<Caller>, Query(r): Query<Reader>) -> Result<Json<snapshot::Workspace>, ApiError> {
    blocking(move || {
        let state = app.view(&caller, &r)?;
        let mut cache = app.cache.lock().unwrap();
        let blocked = app.blocked.lock().unwrap().clone();
        let groups = app.groups.lock().unwrap().clone();
        let mut ws = snapshot::build(&state, &mut cache, app.live().as_deref(), &blocked, &groups);
        if state.reduced {
            snapshot::hide(&mut ws);
        }
        Ok(Json(ws))
    })
    .await
}

async fn state(State(app): State<Shared>, Extension(caller): Extension<Caller>, Query(r): Query<Reader>) -> Result<Json<core::State>, ApiError> {
    blocking(move || app.view(&caller, &r).map(Json)).await
}

async fn status(State(app): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    let store = app.store.lock().unwrap();
    let s = store.state();
    let (files, bytes) = attachments::usage(&app.home);
    Ok(Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "commit": env!("WORKRIVER_COMMIT"),
        "port": app.port,
        "db": store.path(),
        "events": store.count()?,
        "intents": s.intents.len(),
        "units": s.units.len(),
        "runtime": { "connected": app.live().is_some(), "warning": app.warning },
        "machines": app.machines.as_ref().map(|m| m.states()).unwrap_or_default(),
        "attachments": { "dir": attachments::dir(&app.home), "files": files, "bytes": bytes },
    })))
}

/// "Something changed", for the page: the user's.
async fn events(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>, ApiError> {
    app.user(&caller)?;
    // A lagging client missed some, which is still one change.
    let stream = BroadcastStream::new(app.changed.subscribe()).map(|_| Ok(SseEvent::default().event("changed").data("changed")));
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ---- the user's session in the page ----

/// Whether this request is the user's; the page shows nothing else until it is.
async fn session(Extension(caller): Extension<Caller>) -> Json<serde_json::Value> {
    Json(json!({ "user": caller.user }))
}

#[derive(Deserialize)]
struct Code {
    code: String,
}

/// A one-time code for a session: the cookie is the page's alone (HttpOnly) and is not
/// sent along with requests from other sites (SameSite=Strict).
async fn session_start(State(app): State<Shared>, Json(c): Json<Code>) -> Result<Response, ApiError> {
    let value = app.auth.redeem(&c.code).ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "this link was used already or is older than 5 minutes; run `workriver open` for a new one".into()))?;
    let cookie = format!("{}={value}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}", auth::COOKIE, auth::SESSION_DAYS * 24 * 3600);
    Ok(([(header::SET_COOKIE, cookie)], Json(json!({ "user": true }))).into_response())
}

/// A new one-time code, for `workriver open`: the user's to ask for.
#[derive(Deserialize, Default)]
struct CodeFor {
    /// The link will be printed in a terminal instead of handed to the browser.
    #[serde(default)]
    printed: bool,
}

async fn session_code(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(c): Json<CodeFor>) -> Result<Json<serde_json::Value>, ApiError> {
    app.user(&caller)?;
    let code = app.auth.code(c.printed).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "url": format!("http://127.0.0.1:{}/#k={code}", app.port) })))
}

#[derive(Deserialize)]
struct Append {
    /// The caller's place in the runtime (its Herdr pane), which says which agent it is.
    #[serde(default)]
    place: Option<String>,
    /// `--as`: taken only where the daemon was started to allow it.
    #[serde(default, rename = "as")]
    named: Option<String>,
    kind: String,
    #[serde(default)]
    body: serde_json::Value,
}

/// Any event, as the CLI or the page records it, as whoever the caller is found to be.
async fn append(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(a): Json<Append>) -> Result<Json<core::Event>, ApiError> {
    let body: Body = serde_json::from_value(json!({ "kind": a.kind, "body": a.body })).map_err(|e| bad(format!("{} event: {e}", a.kind)))?;
    blocking(move || {
        let who = app.who(&caller, a.place.as_deref(), a.named.as_deref())?;
        app.record(&who, body).map(Json)
    })
    .await
}

// ---- other machines ----

/// Follows a machine: a listener of its own here (the far end of its tunnel), then its
/// connection and the reading of its agents.
async fn follow(app: &Shared, label: &str, setting: crate::machines::Setting) -> Result<(), ApiError> {
    let Some(machines) = app.machines.clone() else { return Err(bad("this service follows no other machines".into())) };
    let local_port = listen(app, label, setting.port).await?;
    tracing::info!("machine {label}: its port {} leads to 127.0.0.1:{local_port} here", setting.port);
    machines.start(label, setting, local_port);
    Ok(())
}

/// A machine's own listener; returns its port here.
async fn listen(app: &Shared, label: &str, port: u16) -> Result<u16, ApiError> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let local_port = listener.local_addr().map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.port();
    let routes = machine_router(app.clone(), Via { machine: Some(label.to_string()), port, local: local_port });
    let task = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, routes).await {
            tracing::warn!("a machine's listener ended: {e}");
        }
    });
    if let Some(old) = app.listeners.lock().unwrap().insert(label.to_string(), task.abort_handle()) {
        old.abort();
    }
    if let Some(m) = &app.machines {
        m.listening(label, local_port);
    }
    Ok(local_port)
}

async fn machines(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Json<Vec<crate::machines::MachineState>>, ApiError> {
    app.user(&caller)?;
    Ok(Json(app.machines.as_ref().map(|m| m.list()).unwrap_or_default()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MachineSet {
    label: String,
    /// add, enable, disable or remove.
    action: String,
    #[serde(default)]
    ssh: String,
    #[serde(default)]
    herdr_socket: Option<String>,
    #[serde(default)]
    port: Option<u16>,
}

/// Adds, enables, disables or removes a machine; only from this machine.
async fn machine_set(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(m): Json<MachineSet>) -> Result<Json<Vec<crate::machines::MachineState>>, ApiError> {
    app.user(&caller)?;
    let Some(all) = app.machines.clone() else { return Err(bad("this service follows no other machines".into())) };
    let stop = |label: &str| {
        all.stop(label);
        if let Some(l) = app.listeners.lock().unwrap().remove(label) {
            l.abort();
        }
    };
    match m.action.as_str() {
        "add" => all.add(&m.label, crate::machines::Setting { ssh: m.ssh, herdr_socket: m.herdr_socket, port: m.port.unwrap_or(app.port), enabled: false }).map_err(bad)?,
        "enable" => {
            let setting = all.enable(&m.label, true).map_err(bad)?;
            follow(&app, &m.label, setting).await?;
        }
        "disable" => {
            all.enable(&m.label, false).map_err(bad)?;
            stop(&m.label);
        }
        "remove" => {
            stop(&m.label);
            all.remove(&m.label).map_err(bad)?;
        }
        other => return Err(bad(format!("unknown action {other:?}"))),
    }
    let _ = app.changed.send(());
    Ok(Json(all.list()))
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

async fn decide(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(d): Json<Decide>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    let body = Body::FindingDecided {
        unit: d.unit,
        task: d.task,
        finding: d.finding,
        decision: d.decision,
        note: d.note,
        by: core::Who::User,
    };
    blocking(move || app.record(&who, body).map(Json)).await
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

async fn gate(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(g): Json<Gate>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    let body = Body::GateSubmitted { unit: g.unit, task: g.task, result: g.result, note: g.note, findings: g.findings, attachments: g.attachments };
    blocking(move || app.record(&who, body).map(Json)).await
}

#[derive(Deserialize)]
struct ChoreDone {
    chore: String,
}

async fn chore_done(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(c): Json<ChoreDone>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || app.record(&who, Body::ChoreDone { chore: c.chore }).map(Json)).await
}

#[derive(Deserialize)]
struct Focus {
    agent: String,
}

async fn focus(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(f): Json<Focus>) -> Result<StatusCode, ApiError> {
    app.user(&caller)?;
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
    /// The asking agent's place in the runtime; none: the user asks (the page).
    #[serde(default)]
    place: Option<String>,
    #[serde(default, rename = "as")]
    named: Option<String>,
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
    /// The uploading agent's place in the runtime; none: the user uploads (the page).
    #[serde(default)]
    place: Option<String>,
    #[serde(default, rename = "as")]
    named: Option<String>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    task: Option<u32>,
    #[serde(default)]
    finding: Option<String>,
}

/// A file into the store, its body raw (`?name=…`, the type from Content-Type), optionally
/// linked to a task or a finding. Answers its id and path.
async fn attach(State(app): State<Shared>, Extension(via): Extension<Caller>, Query(u): Query<Upload>, headers: axum::http::HeaderMap, body: axum::body::Bytes) -> Result<Json<serde_json::Value>, ApiError> {
    let given = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(str::to_string);
    blocking(move || {
        let who = app.who(&via, u.place.as_deref(), u.named.as_deref())?;
        // Only the last part of the name: it is shown, never used as a path.
        let name = Path::new(&u.name).file_name().map(|n| n.to_string_lossy().into_owned()).filter(|n| !n.is_empty()).ok_or_else(|| bad("an attachment needs a file name".into()))?;
        let mime = attachments::mime(&name, given.as_deref());
        let (id, path) = attachments::put(&app.home, &body, &name).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("cannot store {name}: {e}")))?;
        let size = body.len() as u64;
        app.record(&who, Body::AttachmentAdded { id: id.clone(), name: name.clone(), mime: mime.clone(), size, unit: u.unit, task: u.task, finding: u.finding })?;
        // To a caller on another machine the file is where its tunnel shows it.
        let path = match &via.machine {
            Some(_) => json!(crate::machines::attachment_url(via.port, &path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())),
            None => json!(path),
        };
        Ok(Json(json!({ "id": id, "path": path, "name": name, "type": mime, "size": size })))
    })
    .await
}

/// A stored file. Only images show in the page; anything else downloads, and nothing is sniffed.
async fn attachment(State(app): State<Shared>, Extension(caller): Extension<Caller>, UrlPath(id): UrlPath<String>, Query(r): Query<Reader>) -> Result<Response, ApiError> {
    // An agent on another machine is given the link with the file's extension.
    let id = id.split('.').next().unwrap_or_default().to_string();
    // Of what the caller may see: an agent on a blind task, only what it attached itself.
    let viewer = app.clone();
    let state = blocking(move || viewer.view(&caller, &r)).await?;
    let found = state.attachments.iter().find(|a| a.id == id).cloned();
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
async fn inbox(State(app): State<Shared>, Extension(via): Extension<Caller>, Json(i): Json<Inbox>) -> Result<Json<serde_json::Value>, ApiError> {
    app.store.lock().unwrap().state().task(&i.unit, i.task)?;
    // A caller on another machine keeps the folder on its own disk; the CLI makes it there.
    if via.machine.is_some() {
        return Ok(Json(json!({ "elsewhere": true })));
    }
    let d = attachments::inbox(&app.home, &i.unit, i.task);
    attachments::mkdir(&d).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("cannot make {}: {e}", d.display())))?;
    Ok(Json(json!({ "dir": d })))
}

#[derive(Deserialize)]
struct EndThread {
    thread: String,
}

async fn end_thread(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(t): Json<EndThread>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || app.record(&who, Body::ThreadEnded { thread: t.thread }).map(Json)).await
}

/// The user asks why: recorded at once; the temporary agent starts in the background.
async fn ask_why(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(a): Json<Ask>) -> Result<Json<core::Event>, ApiError> {
    let body = Body::QuestionAsked { id: String::new(), thread: a.thread.unwrap_or_default(), unit: a.unit, task: a.task, text: a.text, context: a.context, conclude: a.conclude };
    let starter = app.clone();
    let e = blocking(move || {
        let who = app.who(&caller, a.place.as_deref(), a.named.as_deref())?;
        app.record(&who, body)
    })
    .await?;
    if let Body::QuestionAsked { id, .. } = &e.body {
        let id = id.clone();
        tokio::task::spawn_blocking(move || starter.start_question(&id));
    }
    Ok(Json(e))
}

/// A task's commits on its unit's branch, as (short id, subject).
fn task_commits(at: &scan::Runner, u: &core::Unit, task: u32) -> Vec<(String, String)> {
    let repo = Path::new(&u.repo);
    let hashes = if u.merged.is_some() { u.merged_commits.clone() } else { scan::commit_list(at, repo, &u.base, &u.branch).unwrap_or_default() };
    let want = format!("{}#{task}", u.id);
    hashes.iter().filter_map(|h| scan::commit(at, repo, h).ok()).filter(|c| c.task.as_deref() == Some(want.as_str())).map(|c| (c.id, c.subject)).collect()
}

async fn here(State(app): State<Shared>, Extension(via): Extension<Caller>, Json(h): Json<Here>) -> Result<Json<serde_json::Value>, ApiError> {
    blocking(move || app.here(&h.place, via.machine.as_deref()).map(Json)).await
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
