// The daemon: one process on 127.0.0.1 that records events, reads git, and serves the
// web UI with its data and a stream of "something changed".
use std::collections::HashSet;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use axum::extract::{Request, State};
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
use workriver_core::{self as core, Body, Delivery, DeliveryStep, ReasonCode, RuntimeRef, Store};
use workriver_scan as scan;

use crate::deliver;
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
    changed: broadcast::Sender<()>,
    port: u16,
    watcher: Mutex<Option<Debouncer<RecommendedWatcher>>>,
    watched: Mutex<HashSet<PathBuf>>,
}

/// A delivered message: picked up once its receiver is seen working after it arrived.
/// Rebuilt from the records at start.
struct Pending {
    unit: String,
    task: u32,
    finding: Option<String>,
    delivery: Delivery,
    /// The receiver was seen not working since the message went out.
    armed: bool,
    sent: std::time::Instant,
}

/// How long a delivered message is watched for its pick-up; the receiver may vanish from
/// the runtime for a while (a restart, a moved pane) and come back.
const WATCH: Duration = Duration::from_secs(30 * 60);

type Shared = Arc<App>;

/// How often the runtime's agents are read.
const POLL: Duration = Duration::from_secs(2);

/// Serves on an already bound listener until Ctrl-C.
pub async fn serve(listener: tokio::net::TcpListener, store: Store, runtime: Arc<dyn Runtime>, warning: Option<String>) -> anyhow::Result<()> {
    let port = listener.local_addr()?.port();
    let (changed, _) = broadcast::channel(16);
    let app = Arc::new(App {
        store: Mutex::new(store),
        cache: Mutex::default(),
        runtime,
        warning,
        live: Mutex::new(None),
        pending: Mutex::default(),
        changed: changed.clone(),
        port,
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
                    out.push(Pending { unit: u.id.clone(), task: t.n, finding: None, delivery: d, armed: false, sent: now });
                }
                for f in &t.findings {
                    if let Some(d) = delivered(&f.delivery) {
                        out.push(Pending { unit: u.id.clone(), task: t.n, finding: Some(f.found.id.clone()), delivery: d, armed: false, sent: now });
                    }
                }
            }
        }
        out
    };
    app.pending.lock().unwrap().extend(pending);
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
                    picked.push((p.unit.clone(), p.task, p.finding.clone(), Delivery { step: DeliveryStep::Picked, ..p.delivery.clone() }));
                    false
                }
                Some(a) => {
                    p.armed |= a.state != AgentState::Working;
                    p.sent.elapsed() < WATCH
                }
            }
        });
        let recorded = !picked.is_empty();
        for (unit, task, finding, delivery) in picked {
            if let Err(e) = self.store.lock().unwrap().append("workriver", Body::DeliveryUpdated { unit, task, finding, delivery }) {
                tracing::warn!("cannot record a pick-up: {e}");
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
        let body = self.complete(body)?;
        let e = self.store.lock().unwrap().append(actor, body)?;
        self.deliver(&e);
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
    fn complete(&self, body: Body) -> Result<Body, ApiError> {
        Ok(match body {
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
                t.agent = t.agent.map(|a| self.session_of(&a));
                Body::TaskCreated(t)
            }
            Body::TaskStarted { unit, task, agent } => Body::TaskStarted { unit, task, agent: agent.map(|a| self.session_of(&a)) },
            other => other,
        })
    }

    /// A live agent's session by its name; anything else as it is.
    fn session_of(&self, agent: &str) -> String {
        let live = self.live().unwrap_or_default();
        live.iter().find(|a| a.name == agent).map(|a| a.session.clone()).unwrap_or_else(|| agent.to_string())
    }

    /// Sends the message a user's input calls for, and records how far it got.
    fn deliver(&self, e: &core::Event) {
        let live = self.live();
        let plan = deliver::plan(self.store.lock().unwrap().state(), live.as_deref(), e);
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
            self.pending.lock().unwrap().push(Pending { unit: plan.unit.clone(), task: plan.task, finding: plan.finding.clone(), delivery: delivery.clone(), armed: !working, sent: std::time::Instant::now() });
        }
        let body = Body::DeliveryUpdated { unit: plan.unit, task: plan.task, finding: plan.finding, delivery };
        if let Err(e) = self.store.lock().unwrap().append("workriver", body) {
            tracing::warn!("cannot record a delivery: {e}");
        }
    }

    /// Who and where the CLI is: the agent at its runtime place (recorded by name the first
    /// time), and with `intent`, the intent following its group (created on first use).
    fn here(&self, place: &str, intent: bool) -> Result<serde_json::Value, ApiError> {
        let at = self.runtime.locate(place).map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
        if let Some(a) = &at.agent {
            let known = self.store.lock().unwrap().state().name_of(&a.session) == Some(a.name.as_str());
            if !known {
                self.record("workriver", Body::AgentSeen { session: a.session.clone(), name: a.name.clone(), harness: a.harness.clone() })?;
            }
        }
        let intent = match (&at.group, intent) {
            (Some(g), true) => Some(self.intent_here(g)?),
            _ => None,
        };
        Ok(json!({ "actor": at.agent.as_ref().map(|a| &a.session), "name": at.agent.as_ref().map(|a| &a.name), "intent": intent }))
    }

    fn intent_here(&self, g: &RuntimeRef) -> Result<String, ApiError> {
        let found = self.store.lock().unwrap().state().intent_for(g).map(|i| (i.id.clone(), i.runtime_ref.clone()));
        match found {
            Some((id, r)) if r.as_ref() == Some(g) => Ok(id),
            // Found by its label: the group's id (or label) changed; follow it.
            Some((id, _)) => {
                self.record("workriver", Body::IntentLinked { intent: id.clone(), runtime_ref: g.clone() })?;
                Ok(id)
            }
            None => {
                let e = self.record("workriver", Body::IntentCreated { id: String::new(), title: g.label.clone(), runtime_ref: Some(g.clone()) })?;
                let (Some(id), _, _) = e.body.refs() else { unreachable!("an intent has an id") };
                Ok(id)
            }
        }
    }
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
        Ok(Json(snapshot::build(&state, &mut cache, app.live().as_deref())))
    })
    .await
}

async fn state(State(app): State<Shared>) -> Json<core::State> {
    Json(app.store.lock().unwrap().state().clone())
}

async fn status(State(app): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    let store = app.store.lock().unwrap();
    let s = store.state();
    Ok(Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "port": app.port,
        "db": store.path(),
        "events": store.count()?,
        "intents": s.intents.len(),
        "units": s.units.len(),
        "runtime": { "connected": app.live().is_some(), "warning": app.warning },
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
}

async fn gate(State(app): State<Shared>, Json(g): Json<Gate>) -> Result<Json<core::Event>, ApiError> {
    let body = Body::GateSubmitted { unit: g.unit, task: g.task, result: g.result, note: g.note, findings: g.findings };
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
        app.runtime.focus(&app.session_of(&f.agent)).map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

#[derive(Deserialize)]
struct Here {
    place: String,
    #[serde(default)]
    intent: bool,
}

/// The CLI asks who and where it is: see `App::here`.
async fn here(State(app): State<Shared>, Json(h): Json<Here>) -> Result<Json<serde_json::Value>, ApiError> {
    blocking(move || app.here(&h.place, h.intent).map(Json)).await
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
