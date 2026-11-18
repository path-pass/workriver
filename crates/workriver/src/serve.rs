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
use tokio::sync::{broadcast, watch};
use tokio_stream::{Stream, StreamExt, wrappers::{BroadcastStream, WatchStream}};
use workriver_core::runtime::{AgentState, Runtime, RuntimeAgent, RuntimeError};
use workriver_core::{self as core, Body, Delivery, DeliveryStep, Principal, ReasonCode, Store};
use workriver_scan as scan;

use crate::machines::Machines;
use crate::metrics::Metrics;
use crate::auth::{self, Auth};
use crate::{ask, attachments, deliver, run};
use crate::snapshot::{self, Cache};

pub struct App {
    store: Mutex<Store>,
    cache: Mutex<Cache>,
    /// Wakes the background read of repositories on other machines.
    remote_kick: std::sync::mpsc::Sender<()>,
    runtime: Arc<dyn Runtime>,
    /// What was wrong with the runtime at start, if anything.
    warning: Option<String>,
    /// The live agents as last polled; None while the runtime cannot be reached.
    live: Mutex<Option<Vec<RuntimeAgent>>>,
    /// Delivered messages waiting to be picked up.
    pending: Mutex<Vec<Pending>>,
    /// Questions waiting for their temporary agent's answer, with when they give up.
    deadlines: Mutex<Vec<(String, std::time::Instant)>>,
    /// Questions whose agent waits for a person in its terminal: given once it is free.
    held: Mutex<Vec<Held>>,
    /// Threads whose temporary agent waits for a follow-up: name, and when it is closed.
    idle: Mutex<Vec<(String, String, std::time::Instant)>>,
    waits: Waits,
    /// When each live agent was first seen blocked, by session.
    // ponytail: in memory; a restart makes a blocked agent's wait start again.
    blocked: Mutex<std::collections::HashMap<String, String>>,
    /// When each live agent was first seen idle or done, by session (and when, to measure it).
    // ponytail: in memory, like `blocked`; a restart starts every idle wait again.
    idle_since: Mutex<std::collections::HashMap<String, (String, std::time::Instant)>>,
    /// Running tasks whose agent stopped (reminded, or escalated), by (unit or intent, task),
    /// as of the last poll.
    stalls: Mutex<std::collections::HashMap<(String, u32), crate::stall::Watch>>,
    /// Minutes an agent may sit idle on a running task (settings.json; the user sets it).
    stall_minutes: std::sync::atomic::AtomicU32,
    /// Minutes a worker may sit idle with no task before its manager is told (settings.json,
    /// by hand; read at start).
    idle_minutes: u32,
    /// Idle workers whose manager was told, by session and idle since: once per idle period.
    // ponytail: in memory; a restart tells again of a worker still idle.
    idle_told: Mutex<HashSet<(String, String)>>,
    /// Hours a unit may stay ready to merge before its manager is asked to check it
    /// (settings.json, by hand; read at start).
    ready_hours: u32,
    /// What the scans found that the reminders need: units that look squash-merged.
    shared: Arc<Mutex<snapshot::Shared>>,
    /// Which merge reminders went out (`merge_reminders`), kept in a file next to the database.
    merge_told: Mutex<std::collections::BTreeSet<String>>,
    changed: broadcast::Sender<()>,
    /// Set once the service is stopping: open event streams end, so nothing holds it up.
    stopping: watch::Sender<bool>,
    port: u16,
    /// The data directory: the database's folder, which holds the attachment store too.
    home: PathBuf,
    watcher: Mutex<Option<Debouncer<RecommendedWatcher>>>,
    watched: Mutex<HashSet<PathBuf>>,
    /// The other machines, when the runtime follows any.
    machines: Option<Arc<Machines>>,
    /// CPU and memory of this machine and the followed ones, and their alerts.
    metrics: Arc<Metrics>,
    /// Each followed machine's own listener, to end it when the machine is no longer followed.
    listeners: Mutex<std::collections::HashMap<String, tokio::task::AbortHandle>>,
    /// The runtime's groups as last polled, with or without agents.
    groups: Mutex<Vec<core::RuntimeRef>>,
    /// Which machines' connections were up at the last poll: the page hears of a change at once.
    tunnels: Mutex<Vec<(String, bool)>>,
    /// Who the user is: the token, sessions of the page, one-time codes.
    auth: Auth,
    /// A caller may name the agent it records as (`--as`): for tests and development only.
    allow_as: bool,
    /// Held while the list of managers told of this build is read and written.
    told: Mutex<()>,
    /// Commands the user is about to run (prepared, shown in the confirm dialog) and those run,
    /// by id, with what their pane last showed.
    // ponytail: in memory; a restart forgets them (what ran stays in the record).
    runs: Mutex<std::collections::HashMap<String, Run>>,
}

/// A command the user prepared to run, then ran (`place` set).
#[derive(Clone)]
struct Run {
    command: String,
    dir: String,
    machine: Option<String>,
    unit: Option<String>,
    /// Who wrote it: an agent's session, or "user".
    by: String,
    prepared: std::time::Instant,
    place: Option<String>,
    output: String,
    exit: Option<i32>,
    /// Its pane is still read.
    watching: bool,
}

/// This build of the service, as `workriver status` and the guides name it.
pub const BUILD: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("WORKRIVER_COMMIT"), ")");

/// What a request was found to be, before any handler sees it.
#[derive(Clone)]
struct Caller {
    /// The followed machine whose listener it came in on; none: this machine's.
    machine: Option<String>,
    /// It carries the user token, or a session of the page from our own origin. Never on a
    /// machine's listener: a caller on another machine is never the user.
    user: bool,
    /// The Herdr pane the CLI says it runs in, if any: with an agent there it is that
    /// agent's command and never the user's, token or not.
    place: Option<String>,
}

/// What else the daemon is started with.
pub struct Options {
    pub warning: Option<String>,
    pub waits: Waits,
    pub machines: Option<Arc<Machines>>,
    pub allow_as: bool,
    /// Read the machines' CPU and memory (the service; tests that do not look at it do not).
    pub sample: bool,
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

/// A question's message (or the user's message to a manager) held for its agent, which
/// waits for a person in its terminal.
// ponytail: in memory; after a restart the question fails as `restarted`, like any open one.
struct Held {
    /// The question's id, or with `message` the user's message's.
    question: String,
    message: bool,
    /// The session or live name it goes to, and the name to say.
    to: String,
    name: String,
    text: String,
    /// A new agent not ready for a prompt: retried at each poll until then; after it, it may
    /// sit on a prompt the runtime does not report and waits for the user like a blocked one.
    not_ready: Option<std::time::Instant>,
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

/// The merge reminders that went out, in the data directory.
const MERGE_TOLD: &str = "merge-told.json";

/// Serves on an already bound listener until Ctrl-C.
/// How long a temporary agent may take to answer, and to wait idle for a follow-up.
#[derive(Clone, Copy)]
pub struct Waits {
    pub answer: Duration,
    pub idle: Duration,
    /// How long a new agent's first message is retried every poll while it is not ready;
    /// after that it is shown waiting for the user, like a blocked one.
    pub ready: Duration,
    /// How long one minute of the stall threshold lasts (a minute; less in tests).
    pub minute: Duration,
    /// How long one hour of the ready threshold lasts (an hour; less in tests).
    pub hour: Duration,
}

impl Default for Waits {
    fn default() -> Self {
        Waits { answer: ask::TIMEOUT, idle: ask::IDLE, ready: ask::READY, minute: Duration::from_secs(60), hour: Duration::from_secs(3600) }
    }
}

/// As the tests start it: callers may say which agent they are.
#[cfg(test)]
pub async fn serve(listener: tokio::net::TcpListener, store: Store, runtime: Arc<dyn Runtime>, warning: Option<String>, waits: Waits) -> anyhow::Result<()> {
    serve_with(listener, store, runtime, Options { warning, waits, machines: None, allow_as: true, sample: false }).await
}

/// Following other machines too when `machines` is given (`runtime` is then `machines` itself).
pub async fn serve_with(listener: tokio::net::TcpListener, store: Store, runtime: Arc<dyn Runtime>, options: Options) -> anyhow::Result<()> {
    let Options { warning, waits, machines, allow_as, sample } = options;
    let port = listener.local_addr()?.port();
    let store_home = store.path().parent().map(Path::to_path_buf).unwrap_or_default();
    let (changed, _) = broadcast::channel(16);
    let shared = Arc::new(Mutex::new(snapshot::Shared::default()));
    let (kick, kicked) = std::sync::mpsc::channel::<()>();
    let auth = Auth::open(&store_home).with_context(|| format!("cannot make or read the user token in {}", store_home.display()))?;
    if allow_as {
        tracing::warn!("WORKRIVER_ALLOW_AS is set: any local caller may record as any agent (--as). For tests and development only.");
    }
    let metrics = Metrics::new(store_home.clone(), scan::Runner::Local, machines.clone(), runtime.clone(), changed.clone(), crate::metrics::STEP);
    if sample {
        metrics.start();
    }
    let app = Arc::new(App {
        metrics,
        auth,
        allow_as,
        told: Mutex::default(),
        store: Mutex::new(store),
        cache: Mutex::new(Cache::following(machines.clone()).sharing(shared.clone()).remembering(store_home.join("scan-cache").join("local.json"))),
        remote_kick: kick,
        machines,
        listeners: Mutex::default(),
        groups: Mutex::default(),
        tunnels: Mutex::default(),
        runtime,
        warning,
        live: Mutex::new(None),
        pending: Mutex::default(),
        deadlines: Mutex::default(),
        held: Mutex::default(),
        idle: Mutex::default(),
        waits,
        blocked: Mutex::default(),
        idle_since: Mutex::default(),
        stalls: Mutex::default(),
        stall_minutes: crate::machines::stall_minutes(&store_home).into(),
        ready_hours: crate::machines::ready_hours(&store_home),
        idle_minutes: crate::machines::idle_minutes(&store_home),
        idle_told: Mutex::default(),
        shared: shared.clone(),
        merge_told: Mutex::new(std::fs::read(store_home.join(MERGE_TOLD)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()),
        changed: changed.clone(),
        stopping: watch::channel(false).0,
        port,
        home: store_home,
        watcher: Mutex::new(None),
        watched: Mutex::default(),
        runs: Mutex::default(),
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
    let units: Vec<(String, String)> = app.store.lock().unwrap().state().units.iter().filter(|u| u.machine.is_none() && u.has_code()).map(|u| (u.repo.clone(), u.branch.clone())).collect();
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
        let _ = listen(&app, &m.label, 0).await;
    }
    // Repositories on other machines cannot be watched: they are read in the background,
    // every `REMOTE` and whenever a record touches one of their units, never while a request
    // waits; the page hears when something there changed.
    let remote = app.clone();
    let mut bg = Cache::following(app.machines.clone()).sharing(shared).remembering(app.home.join("scan-cache").join("remote.json"));
    bg.remote_reads = true;
    std::thread::spawn(move || {
        let mut wait = snapshot::REMOTE;
        loop {
            match kicked.recv_timeout(wait) {
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                _ => while kicked.try_recv().is_ok() {},
            }
            if *remote.stopping.borrow() {
                break;
            }
            let state = remote.store.lock().unwrap().state().clone();
            if !state.units.iter().any(|u| u.machine.is_some()) {
                continue;
            }
            let started = std::time::Instant::now();
            let (changed, skipped) = snapshot::refresh_remote(&state, &mut bg);
            // A machine not reachable yet (just started, reconnecting): looked at again soon.
            wait = if skipped { Duration::from_secs(1) } else { snapshot::REMOTE };
            bg.save();
            let found = std::mem::take(&mut bg.merged_now);
            remote.record_merges(found);
            let ms = started.elapsed().as_millis() as u64;
            tracing::info!(target: "perf", kind = "remote_read", ms, recomputed = bg.recomputed, reused = bg.reused, changed, slow = ms >= snapshot::SLOW_MS);
            if changed {
                let _ = remote.changed.send(());
            }
        }
    });
    let _ = app.remote_kick.send(());
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
    // Messages to a manager held by a previous run (its manager was waiting in its terminal) are lost.
    let held: Vec<String> = app.store.lock().unwrap().state().messages.iter().filter(|m| m.delivery.is_none()).map(|m| m.id.clone()).collect();
    for id in held {
        app.message_not_delivered(&id, None, ReasonCode::Restarted, "the service restarted while the message waited for its manager");
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
    let stop = {
        let app = app.clone();
        async move {
            stop_signal().await;
            tracing::info!("stopping: open requests get {} s", GRACE.as_secs());
            let _ = app.stopping.send(true);
            app.metrics.stop();
            if let Some(m) = &app.machines {
                m.stop_all();
            }
        }
    };
    let mut stopping = app.stopping.subscribe();
    let server = axum::serve(listener, router(app, Via { machine: None, port, local: port })).with_graceful_shutdown(stop);
    tokio::select! {
        r = server => r?,
        _ = async { let _ = stopping.wait_for(|s| *s).await; tokio::time::sleep(GRACE).await } => tracing::warn!("requests still open after {} s; stopping anyway", GRACE.as_secs()),
    }
    Ok(())
}

/// How long open requests may finish once the service is told to stop.
const GRACE: Duration = Duration::from_secs(2);

/// Returns at the first Ctrl-C or SIGTERM; a second one ends the process at once.
async fn stop_signal() {
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term => {}
    }
    tokio::spawn(async {
        if tokio::signal::ctrl_c().await.is_ok() {
            std::process::exit(130);
        }
    });
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
        let mut now = self.runtime.agents().ok();
        // A held question's new agent still not ready after its window waits for the user
        // (e.g. on a first-run prompt the runtime does not report): shown blocked.
        {
            let at = std::time::Instant::now();
            let held = self.held.lock().unwrap();
            for a in now.iter_mut().flatten().filter(|a| a.state == AgentState::Unknown) {
                if held.iter().any(|h| (a.session == h.to || a.name == h.to) && h.not_ready.is_some_and(|end| end <= at)) {
                    a.state = AgentState::Blocked;
                }
            }
        }
        let groups = self.runtime.groups().unwrap_or_default();
        let regrouped = std::mem::replace(&mut *self.groups.lock().unwrap(), groups.clone()) != groups;
        {
            let mut blocked = self.blocked.lock().unwrap();
            let still: Vec<&RuntimeAgent> = now.iter().flatten().filter(|a| a.state == AgentState::Blocked).collect();
            blocked.retain(|s, _| still.iter().any(|a| &a.session == s));
            for a in still {
                blocked.entry(a.session.clone()).or_insert_with(core::now);
            }
            let mut idle = self.idle_since.lock().unwrap();
            let still: Vec<&RuntimeAgent> = now.iter().flatten().filter(|a| matches!(a.state, AgentState::Idle | AgentState::Done)).collect();
            idle.retain(|s, _| still.iter().any(|a| &a.session == s));
            for a in still {
                idle.entry(a.session.clone()).or_insert_with(|| (core::now(), std::time::Instant::now()));
            }
        }
        let stalled = now.as_deref().is_some_and(|live| self.find_stalls(live));
        if let Some(live) = now.as_deref() {
            self.merge_reminders(live);
            self.idle_reminders(live);
        }
        let differs = {
            let mut live = self.live.lock().unwrap();
            let differs = *live != now;
            *live = now.clone();
            differs
        };
        let tunnels: Vec<(String, bool)> = self.machines.iter().flat_map(|m| m.states()).map(|m| (m.label, m.tunnel)).collect();
        let differs = differs || std::mem::replace(&mut *self.tunnels.lock().unwrap(), tunnels.clone()) != tunnels;
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
        // Held questions whose agent is free now are given to it; one whose agent is gone fails.
        if let Some(agents) = &now {
            let state = |h: &Held| agents.iter().find(|a| a.session == h.to || a.name == h.to).map(|a| a.state);
            let ready: Vec<Held> = {
                let mut held = self.held.lock().unwrap();
                let at = std::time::Instant::now();
                let free = |h: &Held| h.not_ready.is_some_and(|end| end > at) || !matches!(state(h), Some(AgentState::Blocked | AgentState::Working | AgentState::Unknown));
                let (ready, wait): (Vec<Held>, Vec<Held>) = held.drain(..).partition(free);
                *held = wait;
                ready
            };
            for h in ready {
                match state(&h) {
                    None if h.message => self.message_not_delivered(&h.question, Some(&h.to), ReasonCode::ManagerGone, &format!("{} closed before it could be given the message", h.name)),
                    None => self.fail_question(&h.question, ReasonCode::RuntimeError, &format!("{} closed before it could be given the question", h.name)),
                    Some(_) => self.hand(h),
                }
            }
        }
        // Questions whose temporary agent has not answered in time.
        let now = std::time::Instant::now();
        let late: Vec<String> = self.deadlines.lock().unwrap().iter().filter(|(_, at)| *at <= now).map(|(id, _)| id.clone()).collect();
        for id in &late {
            let held = self.held.lock().unwrap().iter().find(|h| &h.question == id).map(|h| (h.name.clone(), h.not_ready.is_some()));
            let secs = self.waits.answer.as_secs();
            match held {
                // Its tab stays: the user may be about to answer what it waits on.
                Some((name, true)) => self.drop_question(id, ReasonCode::NotReady, &format!("{name} never became ready for a prompt in {secs} seconds; the question never reached it, and its tab was left open for you")),
                Some((name, false)) => self.fail_question(id, ReasonCode::Blocked, &format!("{name} waited for an answer in its terminal for {secs} seconds; the question never reached it")),
                None => self.fail_question(id, ReasonCode::Timeout, &format!("no answer within {secs} seconds")),
            }
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
        if differs || recorded || regrouped || stalled {
            let _ = self.changed.send(());
        }
    }

    /// Works out which running tasks' agents stopped (see `stall::step`): the agent itself is
    /// reminded once; if that does not help, its intent's manager is told, once, unless the
    /// manager is the one that stopped (then, as with no manager, only the user sees it, in
    /// Needs you). Whether anything changed.
    fn find_stalls(&self, live: &[RuntimeAgent]) -> bool {
        let read = self.readable(live);
        let threshold = self.waits.minute * self.stall_minutes.load(std::sync::atomic::Ordering::Relaxed);
        let idle = self.idle_since.lock().unwrap().clone();
        let mut tell = vec![];
        let changed = {
            let store = self.store.lock().unwrap();
            let state = store.state();
            let mut stalls = self.stalls.lock().unwrap();
            let (next, found) = crate::stall::step(state, live, &idle, threshold, self.waits.minute, &read, &stalls);
            for f in &found {
                let to = match f.act {
                    crate::stall::Act::Remind => Some(f.agent.as_str()),
                    crate::stall::Act::Escalate => {
                        let stopped = |m: &str| m == f.agent || state.name_of(m) == Some(f.agent.as_str());
                        state.intent_of(&f.owner).and_then(|i| state.manager_of(i)).filter(|m| !stopped(m) && live.iter().any(|a| a.session == *m && a.state != AgentState::Unknown))
                    }
                };
                if let Some(to) = to
                    && let Some(text) = crate::stall::message(state, f)
                {
                    tell.push((to.to_string(), text));
                }
            }
            let seen = |m: &std::collections::HashMap<(String, u32), crate::stall::Watch>| m.iter().filter_map(|(k, w)| Some((k.clone(), w.stall.clone()?))).collect::<Vec<_>>();
            let mut was = seen(&stalls);
            let mut now = seen(&next);
            was.sort_by(|a, b| a.0.cmp(&b.0));
            now.sort_by(|a, b| a.0.cmp(&b.0));
            *stalls = next;
            was != now
        };
        for (to, text) in tell {
            if let Err(e) = self.send(&to, &text) {
                tracing::warn!("cannot tell {to} of a stopped worker: {e}");
            }
        }
        changed
    }

    /// Where the runtime was read just now and listed agents (None: this machine): elsewhere
    /// (a failed or empty read, a machine that cannot be reached, listed as last seen) agents
    /// are unknown.
    fn readable(&self, live: &[RuntimeAgent]) -> Vec<Option<String>> {
        let down: Vec<String> = self.machines.iter().flat_map(|m| m.states()).filter(|m| !m.reachable).map(|m| m.label).collect();
        let mut read: Vec<Option<String>> = live.iter().map(|a| a.machine.clone()).filter(|m| !m.as_ref().is_some_and(|m| down.contains(m))).collect();
        read.dedup();
        read
    }

    /// Workers idle with no task for `idle_minutes` (see `stall::idle_workers`): their manager
    /// is told, once per idle period, to close them if it has no work for them; workriver never
    /// closes them. Only a manager that is running and not on a blind task; until then nothing
    /// is marked told.
    fn idle_reminders(&self, live: &[RuntimeAgent]) {
        let threshold = self.waits.minute * self.idle_minutes;
        let idle = self.idle_since.lock().unwrap().clone();
        let read = self.readable(live);
        let mut told = self.idle_told.lock().unwrap();
        told.retain(|(s, since)| idle.get(s).is_some_and(|(x, _)| x == since));
        let tell: Vec<_> = {
            let store = self.store.lock().unwrap();
            let state = store.state();
            let reachable = |m: &str| live.iter().any(|a| a.session == m && a.state != AgentState::Unknown) && state.blind_tasks(m).is_empty();
            crate::stall::idle_workers(state, live, &idle, threshold, &read)
                .into_iter()
                .filter(|w| !told.contains(&(w.session.clone(), w.since.clone())) && reachable(&w.manager))
                .filter_map(|w| Some((w.manager.clone(), (w.session.clone(), w.since.clone()), crate::stall::idle_message(state, &w)?)))
                .collect()
        };
        for (to, key, text) in tell {
            match self.send(&to, &text) {
                Ok(()) => {
                    told.insert(key);
                }
                Err(e) => tracing::warn!("cannot tell {to} of an idle worker: {e}"),
            }
        }
    }

    /// Ready units that may have been merged where git cannot see it. One that looks
    /// squash-merged (`snapshot::Shared::squash`): its manager is told once per branch tip.
    /// One ready for longer than `ready_hours`: its manager is asked, once per ready period,
    /// to check its pull request. Only a manager that is running and not on a blind task;
    /// until then nothing is marked told. With no manager the page shows it in Needs you.
    fn merge_reminders(&self, live: &[RuntimeAgent]) {
        let squash = self.shared.lock().unwrap().squash.clone();
        let after = self.waits.hour * self.ready_hours;
        let mut tell = vec![];
        {
            let store = self.store.lock().unwrap();
            let state = store.state();
            let told = self.merge_told.lock().unwrap();
            for u in state.units.iter().filter(|u| u.has_code()) {
                let Some(since) = state.ready_since(u) else { continue };
                let Some(to) = state.manager_of(&u.intent).filter(|m| live.iter().any(|a| a.session == *m && a.state != AgentState::Unknown) && state.blind_tasks(m).is_empty()) else { continue };
                let (id, branch) = (&u.id, crate::quote::name(&u.branch));
                if let Some(s) = squash.get(id) {
                    let key = format!("squash {id} {}", s.tip);
                    let short: String = s.commit.chars().take(9).collect();
                    let text = format!(
                        "[workriver] {id} ({branch}) looks merged into {} by squash: every change of it is there (commit {short} matches). If so, record it: `workriver unit merged {id}`; if not, ignore this. How: `workriver guide how closing-units`.",
                        crate::quote::name(&s.into)
                    );
                    tell.push((to.to_string(), key, text));
                }
                let ago = humantime::parse_rfc3339(&since).ok().and_then(|t| t.elapsed().ok());
                if let Some(ago) = ago.filter(|a| *a >= after) {
                    let key = format!("ready {id} {since}");
                    let text = format!(
                        "[workriver] {id} ({branch}) has been ready to merge for {} h and git shows no merge. Check its pull request: merged (by squash, say): `workriver unit merged {id}`; still open: nothing to do; abandoned: `workriver unit done {id} --reason \"...\"`. How: `workriver guide how push-pr-merge`.",
                        ago.as_secs() / self.waits.hour.as_secs().max(1)
                    );
                    tell.push((to.to_string(), key, text));
                }
            }
            tell.retain(|(_, key, _)| !told.contains(key));
        }
        for (to, key, text) in tell {
            if let Err(e) = self.send(&to, &text) {
                tracing::warn!("cannot remind {to} of a merge: {e}");
                continue;
            }
            let mut told = self.merge_told.lock().unwrap();
            told.insert(key);
            let path = self.home.join(MERGE_TOLD);
            if let Err(e) = serde_json::to_vec_pretty(&*told).map_err(std::io::Error::other).and_then(|b| std::fs::write(&path, b)) {
                tracing::warn!("cannot write {}: {e}", path.display());
            }
        }
    }

    fn live(&self) -> Option<Vec<RuntimeAgent>> {
        self.live.lock().unwrap().clone()
    }

    /// The page's address on this machine, where the user opens it; a unit or task is a
    /// path under it (`/u1`, `/u1/3`).
    fn page(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Where git runs for a repository on `machine`; an error while it cannot be reached.
    fn runner(&self, machine: Option<&str>) -> Result<scan::Runner, ApiError> {
        match machine {
            None => Ok(scan::Runner::Local),
            Some(m) => self.machines.as_ref().and_then(|x| x.runner(m)).ok_or_else(|| {
                let since = self.machines.as_ref().and_then(|x| x.down_since(m)).filter(|s| !s.is_empty()).map(|s| format!(" (disconnected since {s})")).unwrap_or_default();
                ApiError(StatusCode::SERVICE_UNAVAILABLE, format!("machine {m} cannot be reached now{since}; see `workriver machine list`"))
            }),
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
        match machine {
            Some(_) => self.runtime.send(to, &crate::machines::links(text, &format!("{}/", attachments::dir(&self.home).display()))),
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
                self.tell_of_update(&a.session, false);
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
        // The token file can be read from an agent's pane on this machine; the CLI there says
        // where it runs, and what runs in an agent's pane is the agent's doing.
        if let Some(place) = caller.place.as_deref().filter(|_| caller.machine.is_none())
            && self.runtime.locate(None, place).is_ok_and(|at| at.agent.is_some())
        {
            return Err(ApiError(StatusCode::FORBIDDEN, "only the user does this: in the web UI, or with the CLI from a terminal that is not an agent's".into()));
        }
        match (&caller.machine, caller.user) {
            (None, true) => Ok(Principal::User),
            (Some(m), _) => Err(ApiError(StatusCode::FORBIDDEN, format!("a caller on machine {m} is an agent of that machine, never the user"))),
            (None, false) => Err(ApiError(StatusCode::UNAUTHORIZED, NOT_THE_USER.into())),
        }
    }

    /// What the page reads, of `state` as the reader may see it: files without their texts
    /// (`unit_code` has them). `light`: not even uncommitted work or trees (board, `task show`).
    fn snapshot(&self, state: &core::State, light: bool, trigger: &str) -> snapshot::Workspace {
        let started = std::time::Instant::now();
        let calls = scan::CALLS.load(std::sync::atomic::Ordering::Relaxed);
        let blocked = self.blocked.lock().unwrap().clone();
        let idle = self.idle_since.lock().unwrap().iter().map(|(s, (at, _))| (s.clone(), at.clone())).collect();
        let stalls = self.stalls.lock().unwrap().iter().filter_map(|(k, w)| Some((k.clone(), w.stall.clone()?))).collect();
        let groups = self.groups.lock().unwrap().clone();
        let live = self.live();
        let mut cache = self.cache.lock().unwrap();
        (cache.light, cache.bare) = (light, true);
        cache.ready_after = Some(self.waits.hour * self.ready_hours);
        let mut ws = snapshot::build(state, &mut cache, live.as_deref(), &blocked, &idle, &stalls, &groups);
        cache.save();
        if state.reduced {
            snapshot::hide(&mut ws);
        }
        let ms = started.elapsed().as_millis() as u64;
        tracing::info!(target: "perf", kind = "snapshot", trigger, light, ms, recomputed = cache.recomputed, reused = cache.reused, git = scan::CALLS.load(std::sync::atomic::Ordering::Relaxed) - calls, slow = ms >= snapshot::SLOW_MS);
        ws
    }

    /// Units git shows merged (a fast-forward, a merge commit) are recorded merged, with the
    /// commits they had, as `workriver unit merged` would.
    fn record_merges(&self, found: Vec<(String, Vec<String>)>) {
        for (unit, commits) in found {
            if let Err(e) = self.record(&Principal::Service, Body::UnitMerged { unit: unit.clone(), commits }) {
                tracing::warn!("cannot record unit {unit} merged: {}", e.1);
            }
        }
    }

    /// A manager that was at work before this build of the service is told, once, to read
    /// what changed: the skill it follows was read from an older workriver, and nothing else
    /// would tell it. `fresh`: it registers now, having just read this build's guide.
    // Which manager was told of which build is kept in a small file next to the database.
    fn tell_of_update(&self, session: &str, fresh: bool) {
        if !fresh && !self.store.lock().unwrap().state().workspaces.iter().any(|w| w.manager.as_deref() == Some(session)) {
            return;
        }
        let _one = self.told.lock().unwrap();
        let path = self.home.join("guide-told.json");
        let mut told: std::collections::BTreeMap<String, String> = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        if told.get(session).map(String::as_str) == Some(BUILD) {
            return;
        }
        let text = format!("[workriver] workriver was updated (it is now {BUILD}). Some things behave differently: run `workriver guide changes`, read it, then go on with what you were doing.");
        // Not reached now (not running, busy at a prompt): told at its next command instead.
        if fresh || self.send(session, &text).is_ok() {
            told.insert(session.to_string(), BUILD.to_string());
            if let Err(e) = serde_json::to_vec_pretty(&told).map_err(std::io::Error::other).and_then(|b| std::fs::write(&path, b)) {
                tracing::warn!("cannot write {}: {e}", path.display());
            }
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
            Body::ManagerRegistered { agent, .. } => self.tell_of_update(agent, true),
            Body::QuestionAnswered { question, .. } => self.answered(question),
            Body::ThreadEnded { thread } => self.end_thread(thread),
            Body::MessageSent { id, .. } => self.tell_manager(id),
            _ => {}
        }
        // A unit's branch may have got a work tree of its own since; watching again is cheap.
        // One on another machine is read again in the background.
        if let (_, Some(unit), _) = e.body.refs() {
            let found = self.store.lock().unwrap().state().unit(&unit).ok().filter(|u| u.has_code()).map(|u| (u.machine.is_some(), u.repo.clone(), u.branch.clone()));
            match found {
                Some((false, repo, branch)) => self.watch(Path::new(&repo), &branch),
                Some((true, ..)) => {
                    let _ = self.remote_kick.send(());
                }
                None => {}
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
            // Research has no repository: nothing to check.
            b @ Body::UnitCreated { kind: core::UnitKind::Research, .. } => b,
            Body::UnitCreated { id, intent, title, purpose, repo, branch, base, machine: named, kind } => {
                let machine = place(named.as_deref(), machine);
                let top = self.repository(machine.as_deref(), &repo, &base)?;
                Body::UnitCreated { id, intent, title, purpose, repo: top, branch, base, machine, kind }
            }
            // What is not given stays as it is; `machine` is always said (the CLI sends it).
            Body::UnitRelocated { unit, machine: named, repo, branch, base, .. } => {
                let u = self.store.lock().unwrap().state().unit(&unit)?.clone();
                let machine = match named.as_deref() {
                    None => u.machine.clone(),
                    named => place(named, None),
                };
                let (repo, branch, base) = (if repo.is_empty() { u.repo.clone() } else { repo }, if branch.is_empty() { u.branch.clone() } else { branch }, if base.is_empty() { u.base.clone() } else { base });
                let top = self.repository(machine.as_deref(), &repo, &base)?;
                // What is on the place it leaves is read once, now: the old branch is not
                // read again, and what only it had would otherwise vanish without a word.
                // Their patch ids are taken too, all at once, so that their rebased copies are
                // told without reading the old branch again.
                let previous = match self.runner(u.machine.as_deref()) {
                    Ok(at) if (&u.machine, &u.repo, &u.branch, &u.base) != (&machine, &top, &branch, &base) => {
                        let list = snapshot::unit_commits(&at, &u, &mut self.cache.lock().unwrap()).map(|(list, _)| list).unwrap_or_default();
                        let ids = scan::patch_ids(&at, Path::new(&u.repo), &list.iter().map(|c| c.hash.clone()).collect::<Vec<_>>()).unwrap_or_default();
                        list.into_iter()
                            .map(|c| {
                                let linked = u.links.iter().find(|l| l.hash == c.hash).map(|l| l.task);
                                let named = c.task.as_deref().and_then(core::parse_task_ref).filter(|(x, _)| *x == u.id).map(|(_, n)| n);
                                core::SeenCommit { task: linked.or(named), patch_id: ids.get(&c.hash).cloned().unwrap_or_default(), hash: c.hash, subject: c.subject, date: c.date }
                            })
                            .collect()
                    }
                    _ => vec![],
                };
                Body::UnitRelocated { unit, machine, repo: top, branch, base, previous }
            }
            // The commit is looked up where the unit is: it must be one of the unit's, and
            // what a rebased copy of it will be known by is taken down.
            Body::CommitLinked { unit, hash, task, .. } => {
                let u = self.store.lock().unwrap().state().unit(&unit)?.clone();
                let at = self.runner(u.machine.as_deref())?;
                let repo = Path::new(&u.repo);
                let full = scan::resolve(&at, repo, &hash).map_err(|_| bad(format!("no commit {hash} in {}", u.repo)))?;
                let named = snapshot::unit_commits(&at, &u, &mut self.cache.lock().unwrap()).map_err(|e| bad(e.to_string()))?.0;
                let Some(c) = named.into_iter().find(|c| c.hash == full) else {
                    return Err(bad(format!("commit {hash} is not one of unit {unit}'s: not on branch {} since {}", u.branch, u.base)));
                };
                let patch_id = scan::patch_id(&at, repo, &full).unwrap_or_default();
                Body::CommitLinked { unit, hash: full, patch_id, subject: c.subject, date: c.date, task }
            }
            // A chore of no intent belongs to a project, if one is given or can be told.
            Body::ChoreCreated { id, intent, project, unit, title, steps } if intent.is_empty() && !project.is_empty() => {
                let project = self.project_folder(&project, machine).unwrap_or_default();
                Body::ChoreCreated { id, intent, project, unit, title, steps }
            }
            Body::UnitMerged { unit, commits } if commits.is_empty() => {
                // The branch's commits now, or as last seen if they are already merged away.
                let u = self.store.lock().unwrap().state().unit(&unit)?.clone();
                let at = self.runner(u.machine.as_deref())?;
                let commits = snapshot::unit_commits(&at, &u, &mut self.cache.lock().unwrap()).map_err(|e| bad(e.to_string()))?.0.into_iter().map(|c| c.hash).collect();
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
        // Every question, the user's or an agent's, goes to a temporary agent; it may ask the
        // task's own agent while that one is idle (its prompt says how).
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
                store.state().question(id).ok().map(|q| ask::follow_up(store.state(), q, &self.home)).unwrap_or_default()
            };
            self.hand(Held { question: id.into(), message: false, to: name.clone(), name, text, not_ready: None });
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
        // A question about the whole unit reads its commits itself (its prompt says how).
        let commits = at.filter(|_| q.task != 0).map(|at| task_commits(&at, &unit, q.task, &mut self.cache.lock().unwrap())).unwrap_or_default();
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
        // A harness opening a folder it has not seen first asks a person (trust this folder,
        // approve the directory): the prompt waits for that, it is never answered for them.
        // The agent may come back not ready yet (Unknown): `hand` holds the prompt.
        self.hand(Held { question: id.into(), message: false, to: agent.name.clone(), name: agent.name, text: prompt, not_ready: None });
    }

    /// Gives a question's message to the agent that answers it (`to`: a session or a live
    /// agent's name). One waiting for a person in its terminal (Herdr: blocked) cannot take
    /// it: it is held and given once the agent is free (`poll`), or the question fails with
    /// the reason when its time is up. A new agent the runtime does not take prompts for yet
    /// (it refuses, or shows no state) is tried again at each poll, for `Waits::ready` from
    /// the first try; after that it waits for the user like a blocked one.
    fn hand(&self, mut h: Held) {
        let is = |a: &RuntimeAgent| a.session == h.to || a.name == h.to;
        let not_ready = |h: &mut Held| {
            h.not_ready.get_or_insert_with(|| std::time::Instant::now() + self.waits.ready);
        };
        match self.runtime.agents().ok().and_then(|l| l.into_iter().find(is)).map(|a| a.state) {
            Some(AgentState::Blocked) => h.not_ready = None,
            // No state yet: not typed into, whatever the terminal shows.
            Some(AgentState::Unknown) => not_ready(&mut h),
            _ => match self.send(&h.to, &h.text) {
                Ok(()) if h.message => return self.message_delivery(&h.question, Delivery { step: DeliveryStep::Delivered, to: Some(h.to.clone()), reason_code: None, reason: None, message: h.text }),
                Ok(()) => return,
                Err(RuntimeError::Blocked(_)) => h.not_ready = None,
                Err(RuntimeError::NotReady(_)) => not_ready(&mut h),
                Err(e) if h.message => return self.message_not_delivered(&h.question, Some(&h.to), ReasonCode::RuntimeError, &format!("cannot give {} the message: {e}", h.name)),
                Err(e) => return self.fail_question(&h.question, ReasonCode::RuntimeError, &format!("cannot give {} the question: {e}", h.name)),
            },
        }
        self.held.lock().unwrap().push(h);
        let _ = self.changed.send(());
    }

    /// The user wrote to a manager: typed into its terminal now, or held while it waits for a
    /// person there (`hand`), or recorded as not delivered when it is not running.
    fn tell_manager(&self, id: &str) {
        let Some((to, text)) = ({
            let store = self.store.lock().unwrap();
            store.state().message(id).ok().map(|m| (m.manager.clone(), deliver::user_message(store.state(), m)))
        }) else {
            return;
        };
        let name = self.store.lock().unwrap().state().name_of(&to).unwrap_or(&to).to_string();
        match self.runtime.agents() {
            Err(_) => self.message_not_delivered(id, None, ReasonCode::RuntimeDown, "the agent runtime (Herdr) is not connected"),
            Ok(live) if !live.iter().any(|a| a.session == to) => self.message_not_delivered(id, None, ReasonCode::ManagerGone, &format!("the manager {name} is not running")),
            Ok(_) => self.hand(Held { question: id.into(), message: true, to, name, text, not_ready: None }),
        }
    }

    /// Records where the user's message to a manager is.
    fn message_delivery(&self, id: &str, delivery: Delivery) {
        self.held.lock().unwrap().retain(|h| h.question != id);
        if let Err(e) = self.record(&Principal::Service, Body::MessageDelivery { id: id.into(), delivery }) {
            tracing::warn!("message {id}: {}", e.1);
        }
    }

    fn message_not_delivered(&self, id: &str, to: Option<&str>, code: ReasonCode, why: &str) {
        let message = {
            let store = self.store.lock().unwrap();
            store.state().message(id).ok().map(|m| deliver::user_message(store.state(), m)).unwrap_or_default()
        };
        self.message_delivery(id, Delivery { step: DeliveryStep::NotDelivered, to: to.map(str::to_string), reason_code: Some(code), reason: Some(why.into()), message });
    }

    /// Records a question as failed (if still open) and closes its temporary agent, which
    /// did not answer.
    fn fail_question(&self, id: &str, code: ReasonCode, why: &str) {
        self.drop_question(id, code, why);
        // Only an agent started for it is closed; a task's own agent is not.
        let name = self.store.lock().unwrap().state().question(id).ok().filter(|q| q.temporary).and_then(|q| q.name.clone());
        if let Some(name) = name {
            self.close_later(name);
        }
    }

    /// Records a question as failed (if still open) and stops waiting for it.
    fn drop_question(&self, id: &str, code: ReasonCode, why: &str) {
        let body = Body::QuestionFailed { question: id.into(), reason_code: code, reason: why.into() };
        if let Err(e) = self.record(&Principal::Service, body) {
            tracing::warn!("question {id}: {}", e.1);
        }
        self.deadlines.lock().unwrap().retain(|(q, _)| q != id);
        self.held.lock().unwrap().retain(|h| h.question != id);
    }

    /// An answer came: the thread's agent waits for a follow-up until it has been idle too long.
    fn answered(&self, id: &str) {
        self.deadlines.lock().unwrap().retain(|(q, _)| q != id);
        self.held.lock().unwrap().retain(|h| h.question != id);
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
        // An addition to a finding goes to several at once; it is on the page either way, so
        // what does not arrive is only logged.
        // ponytail: these are not followed up as "delivered / picked up" like decisions are.
        // A dropped task likewise: its agent (to stop) and the manager.
        // Work over: the manager hears which of its workers it may close.
        if matches!(e.body, Body::FindingNoted { .. } | Body::TaskDropped { .. } | Body::UnitMerged { .. } | Body::UnitDone { .. } | Body::IntentStateChanged { .. } | Body::ManagerTold { .. }) {
            let to = {
                let store = self.store.lock().unwrap();
                match e.body {
                    Body::FindingNoted { .. } => deliver::notes(store.state(), live.as_deref(), &self.home, e),
                    Body::TaskDropped { .. } => deliver::dropped(store.state(), live.as_deref(), e),
                    Body::ManagerTold { .. } => deliver::told(store.state(), live.as_deref(), e),
                    _ => deliver::finished(store.state(), live.as_deref(), e),
                }
            };
            for (agent, message) in to {
                if let Err(err) = self.send(&agent, &message) {
                    let (_, unit, task) = e.body.refs();
                    tracing::warn!("cannot tell {agent} of {}: {err}", task.or(unit).unwrap_or_default());
                }
            }
            return;
        }
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
        .route("/api/metrics", get(metrics))
        .route("/api/metrics/history", get(metrics_history))
        .route("/api/metrics/thresholds", post(metrics_thresholds))
        .route("/api/settings/stall", get(stall_get).post(stall_set))
        .route("/api/workspace", get(workspace))
        .route("/api/unit/{id}/code", get(unit_code))
        .route("/api/state", get(state))
        .route("/api/task", get(task_show))
        .route("/api/status", get(status))
        .route("/api/events", get(events))
        .route("/api/append", post(append))
        .route("/api/decide", post(decide))
        .route("/api/task/drop", post(drop_task))
        .route("/api/gate", post(gate))
        .route("/api/chore-done", post(chore_done))
        .route("/api/backlog/merge", post(merge))
        .route("/api/focus", post(focus))
        .route("/api/intent/start", post(start_intent))
        .route("/api/run/prepare", post(run_prepare))
        .route("/api/run", post(run_command))
        .route("/api/run/{id}", get(run_read))
        .route("/api/run/{id}/focus", post(run_focus))
        .route("/api/here", post(here))
        .route("/api/ask", post(ask_why))
        .route("/api/thread/end", post(end_thread))
        .route("/api/message", post(message))
        .route("/api/attachments", post(attach).layer(DefaultBodyLimit::max(attachments::MAX)))
        .route("/api/attachments/inbox", post(inbox))
        .route("/api/attachments/{id}", get(attachment))
        .route_layer(middleware::from_fn(timing))
        .fallback(ui)
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .layer(Extension(via))
        .with_state(app)
}

/// One line of the timing log per request: its route (the template, never the path or the
/// query), status, time, size and what kind of caller; nothing that names anyone.
async fn timing(req: Request, next: Next) -> Response {
    let started = std::time::Instant::now();
    let route = req.extensions().get::<axum::extract::MatchedPath>().map(|p| p.as_str().to_string()).unwrap_or_default();
    let method = req.method().to_string();
    let who = req.extensions().get::<Caller>().map_or("none", |c| match (&c.machine, &c.place, c.user) {
        (Some(_), ..) => "machine",
        (None, Some(_), _) => "pane",
        (None, None, true) => "user",
        _ => "none",
    });
    let res = next.run(req).await;
    let ms = started.elapsed().as_millis() as u64;
    let bytes = axum::body::HttpBody::size_hint(res.body()).exact().unwrap_or(0);
    tracing::info!(target: "perf", kind = "http", method, route, status = res.status().as_u16(), ms, bytes, who, slow = ms >= snapshot::SLOW_MS);
    res
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
        .route("/api/task", get(task_show))
        .route("/api/status", get(status))
        // Its own load only (the handlers keep a machine's caller to that machine).
        .route("/api/metrics", get(metrics))
        .route("/api/metrics/history", get(metrics_history))
        .route_layer(middleware::from_fn(timing))
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
    // Through a machine's tunnel the port over there is new with every connection: there,
    // any port of 127.0.0.1 or localhost (a name that resolves elsewhere is still refused).
    let fits = |h: &str| ours.iter().any(|o| o == h) || (via.machine.is_some() && h.rsplit_once(':').is_some_and(|(n, p)| matches!(n, "127.0.0.1" | "localhost") && p.parse::<u16>().is_ok()));
    let (allowed, user) = {
        let header = |h| req.headers().get(h).map(|v| v.to_str().unwrap_or(""));
        let host_ok = header(header::HOST).is_some_and(fits);
        let our_origin = header(header::ORIGIN).map(|o| o.strip_prefix("http://").is_some_and(fits));
        let get = req.method() == Method::GET;
        let origin_ok = get || our_origin.unwrap_or(true);
        let session = app.auth.session(header(header::COOKIE)) && (get || our_origin == Some(true));
        (host_ok && origin_ok, via.machine.is_none() && (app.auth.bearer(header(header::AUTHORIZATION)) || session))
    };
    if !allowed {
        return (StatusCode::FORBIDDEN, "workriver answers only on 127.0.0.1 or localhost, from its own page").into_response();
    }
    let place = req.headers().get("x-workriver-place").and_then(|v| v.to_str().ok()).filter(|p| !p.is_empty()).map(str::to_string);
    req.extensions_mut().insert(Caller { machine: via.machine, user, place });
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
    /// Without file texts, uncommitted work or trees.
    #[serde(default)]
    light: bool,
    /// A unit's code: the commits (short hashes, comma-separated) whose texts the reader
    /// has already; they never change.
    #[serde(default)]
    have: String,
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
        let mut ws = app.snapshot(&state, r.light, "workspace");
        // Machines in alert are the user's to see (Needs you), like their CPU and memory.
        if app.user(&caller).is_ok() {
            ws.alerts = app.metrics.alerts();
        }
        let found = std::mem::take(&mut app.cache.lock().unwrap().merged_now);
        app.record_merges(found);
        Ok(Json(ws))
    })
    .await
}

/// One unit's file texts, which the snapshot leaves out: what the page's code layer shows.
/// Scoped like `/api/workspace`. Not on a machine's listener: no CLI reads it.
async fn unit_code(State(app): State<Shared>, Extension(caller): Extension<Caller>, UrlPath(id): UrlPath<String>, Query(r): Query<Reader>) -> Result<Json<snapshot::Code>, ApiError> {
    blocking(move || {
        let state = app.view(&caller, &r)?;
        let started = std::time::Instant::now();
        let live = app.live();
        let code = {
            let mut cache = app.cache.lock().unwrap();
            (cache.light, cache.bare) = (false, false);
            let have: Vec<&str> = r.have.split(',').filter(|h| !h.is_empty()).collect();
            let code = snapshot::code(&state, &mut cache, live.as_deref(), &id, &have);
            cache.save();
            code
        };
        let found = std::mem::take(&mut app.cache.lock().unwrap().merged_now);
        app.record_merges(found);
        tracing::info!(target: "perf", kind = "code", unit = %id, ms = started.elapsed().as_millis() as u64);
        code.map(Json).ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("no unit {id}")))
    })
    .await
}

#[derive(Deserialize)]
struct ShowQuery {
    /// `u1#3`; none: the task the calling agent is on.
    #[serde(default)]
    task: Option<String>,
    #[serde(default)]
    place: Option<String>,
    #[serde(default, rename = "as")]
    named: Option<String>,
}

/// One task with everything its reader may see (`workriver task show`). Scoped like every
/// read; attachments are paths on this machine and links through the tunnel on another.
async fn task_show(State(app): State<Shared>, Extension(caller): Extension<Caller>, Query(q): Query<ShowQuery>) -> Result<Json<crate::show::TaskShow>, ApiError> {
    blocking(move || {
        let r = Reader { place: q.place.clone(), named: q.named, light: true, have: String::new() };
        let who = app.who(&caller, r.place.as_deref(), r.named.as_deref())?;
        let state = app.view(&caller, &r)?;
        let (unit, n) = match q.task.as_deref().filter(|t| !t.is_empty()) {
            Some(t) => core::parse_task_ref(t).map(|(u, n)| (u.to_string(), n)).ok_or_else(|| bad(format!("{t:?} is not a task; write it as <unit>#<n>, e.g. u1#3")))?,
            None => {
                let Principal::Agent { session, .. } = &who else { return Err(bad("name the task: workriver task show <unit>#<n>".into())) };
                // Started and not delivered: the task this agent is on.
                let on = |owner: &str, ts: &[core::Task]| ts.iter().filter(|t| t.agent.as_deref() == Some(session.as_str()) && t.state == core::TaskState::Running).map(|t| (owner.to_string(), t.n, t.title.clone())).collect::<Vec<_>>();
                let mine: Vec<(String, u32, String)> = state.units.iter().flat_map(|u| on(&u.id, &u.tasks)).chain(state.intents.iter().flat_map(|i| on(&i.id, &i.tasks))).collect();
                match &mine[..] {
                    [(u, n, _)] => (u.clone(), *n),
                    [] => return Err(ApiError(StatusCode::NOT_FOUND, "you have no task in progress (none started and not delivered); name one: workriver task show <unit>#<n> (`workriver board` lists them)".into())),
                    several => {
                        let list: Vec<String> = several.iter().map(|(u, n, t)| format!("{u}#{n} \"{}\"", crate::quote::name(t))).collect();
                        return Err(ApiError(StatusCode::CONFLICT, format!("you are on several tasks: {}; name one: workriver task show <unit>#<n>", list.join(", "))));
                    }
                }
            }
        };
        let ws = serde_json::to_value(app.snapshot(&state, true, "task")).unwrap_or_default();
        let found = std::mem::take(&mut app.cache.lock().unwrap().merged_now);
        app.record_merges(found);
        // A reader on another machine cannot open this machine's files: it gets a link it
        // can fetch through its tunnel, as the reader it is.
        let remote = caller.machine.is_some();
        let file = |id: &str| {
            let a = state.attachments.iter().find(|a| a.id == id)?;
            let path = attachments::path(&app.home, &a.id, &a.name);
            Some(if remote {
                format!("{}?place={}", crate::machines::attachment_url(&path.file_name()?.to_string_lossy()), q.place.as_deref().unwrap_or("$HERDR_PANE_ID"))
            } else {
                path.display().to_string()
            })
        };
        let mut shown = crate::show::task(&state, &ws, &unit, n, &file)?;
        shown.link = format!("{}/{unit}/{n}", app.page());
        Ok(Json(shown))
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
        "build": BUILD,
        "port": app.port,
        // The page as the user opens it, on this machine: `workriver link` builds on it, also
        // through a machine's tunnel.
        "url": app.page(),
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
    // It ends when the service stops, or it would hold the stopping up for good.
    let stop = WatchStream::new(app.stopping.subscribe()).filter(|s| *s).map(|_| false);
    let stream = BroadcastStream::new(app.changed.subscribe()).map(|_| true).merge(stop).take_while(|go| *go).map(|_| Ok(SseEvent::default().event("changed").data("changed")));
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
    Ok(Json(json!({ "url": format!("{}/#k={code}", app.page()) })))
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
async fn append(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(a): Json<Append>) -> Result<Json<serde_json::Value>, ApiError> {
    let body: Body = serde_json::from_value(json!({ "kind": a.kind, "body": a.body })).map_err(|e| bad(format!("{} event: {e}", a.kind)))?;
    blocking(move || {
        let who = app.who(&caller, a.place.as_deref(), a.named.as_deref())?;
        let e = app.record(&who, body)?;
        let mut out = serde_json::to_value(&e).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        // A manager that starts an intent of another project than its workspace's is told so.
        if let Body::IntentStateChanged { intent, state: core::IntentState::Active, workspace: Some(g), .. } = &e.body {
            let live = app.runtime.agents().unwrap_or_default();
            if let Some(why) = snapshot::other_project(app.store.lock().unwrap().state(), &live, g, intent) {
                out["warning"] = json!(format!("{why}. If you do not work on that project, do not split it: ask the user with a decide gate first (`workriver guide how ask-user`)."));
            }
        }
        Ok(Json(out))
    })
    .await
}

// ---- other machines ----

/// Follows a machine: a listener of its own here (the far end of its tunnel), then its
/// connection and the reading of its agents.
async fn follow(app: &Shared, label: &str, setting: crate::machines::Setting) -> Result<(), ApiError> {
    let Some(machines) = app.machines.clone() else { return Err(bad("this service follows no other machines".into())) };
    let local_port = listen(app, label, 0).await?;
    tracing::info!("machine {label}: its tunnel leads to 127.0.0.1:{local_port} here");
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

/// Read by the user and by agents of this machine (a refusal sends them here to see why a
/// machine cannot be reached); changed by the user alone.
async fn machines(State(app): State<Shared>, Extension(caller): Extension<Caller>, Query(r): Query<Reader>) -> Result<Json<Vec<crate::machines::MachineState>>, ApiError> {
    app.who(&caller, r.place.as_deref(), r.named.as_deref())?;
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
}

/// CPU and memory of this machine and the followed ones, with the thresholds: read by the
/// user (the page) and by an agent at its place here (to weigh where to put work); set by
/// the user alone. Not on a machine's listener.
async fn metrics(State(app): State<Shared>, Extension(caller): Extension<Caller>, Query(r): Query<Reader>) -> Result<Json<crate::metrics::Report>, ApiError> {
    app.who(&caller, r.place.as_deref(), r.named.as_deref())?;
    let mut report = app.metrics.report();
    // An agent on another machine sees that machine alone: nothing of this one or the others.
    if let Some(m) = caller.machine.as_deref() {
        report.machines.retain(|x| !x.here && x.label == m);
    }
    Ok(Json(report))
}

#[derive(Deserialize)]
struct HistoryQuery {
    machine: String,
    #[serde(default = "a_day")]
    hours: u32,
    #[serde(default)]
    place: Option<String>,
    #[serde(default, rename = "as")]
    named: Option<String>,
}

fn a_day() -> u32 {
    24
}

/// One machine's minutes over the last `hours` (1 to 168), read as `GET /api/metrics` is:
/// the user, or an agent at its place on this machine.
async fn metrics_history(State(app): State<Shared>, Extension(caller): Extension<Caller>, Query(q): Query<HistoryQuery>) -> Result<Json<serde_json::Value>, ApiError> {
    app.who(&caller, q.place.as_deref(), q.named.as_deref())?;
    if let Some(m) = caller.machine.as_deref().filter(|m| *m != q.machine) {
        return Err(ApiError(StatusCode::FORBIDDEN, format!("a caller on machine {m} reads that machine's history only")));
    }
    let m = app.metrics.clone();
    let (label, hours) = (q.machine.clone(), q.hours);
    let points = tokio::task::spawn_blocking(move || m.history(&label, hours)).await.map_err(|e| bad(e.to_string()))?.map_err(bad)?;
    Ok(Json(json!({ "machine": q.machine, "hours": q.hours, "points": points })))
}

/// Sets when a machine alerts; the user's alone. Kept in settings.json, not an event: a
/// preference like the list of machines, not a record of work.
async fn metrics_thresholds(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(t): Json<crate::metrics::Thresholds>) -> Result<Json<crate::metrics::Report>, ApiError> {
    app.user(&caller)?;
    app.metrics.set_thresholds(t).map_err(bad)?;
    Ok(Json(app.metrics.report()))
}

#[derive(Deserialize)]
struct StallSetting {
    minutes: u32,
}

/// After how many idle minutes a running task's agent counts as stopped; the page's Settings.
async fn stall_get(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Json<serde_json::Value>, ApiError> {
    app.user(&caller)?;
    Ok(Json(json!({ "minutes": app.stall_minutes.load(std::sync::atomic::Ordering::Relaxed) })))
}

/// Sets it; the user's alone, kept in settings.json like the alert thresholds.
async fn stall_set(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(s): Json<StallSetting>) -> Result<Json<serde_json::Value>, ApiError> {
    app.user(&caller)?;
    crate::machines::set_stall_minutes(&app.home, s.minutes).map_err(bad)?;
    app.stall_minutes.store(s.minutes, std::sync::atomic::Ordering::Relaxed);
    Ok(Json(json!({ "minutes": s.minutes })))
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
        "add" => all.add(&m.label, crate::machines::Setting { ssh: m.ssh, herdr_socket: m.herdr_socket, port: 0, enabled: false }).map_err(bad)?,
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
        was: None,
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

#[derive(Deserialize)]
struct DropTask {
    unit: String,
    task: u32,
    reason: String,
}

/// The user marks a task no longer needed from the page (agents use `workriver task drop`).
async fn drop_task(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(d): Json<DropTask>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || app.record(&who, Body::TaskDropped { unit: d.unit, task: d.task, reason: d.reason }).map(Json)).await
}

async fn chore_done(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(c): Json<ChoreDone>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || app.record(&who, Body::ChoreDone { chore: c.chore }).map(Json)).await
}

#[derive(Deserialize)]
struct Merge {
    into: String,
    from: Vec<String>,
    /// A new title for the intent that stays.
    #[serde(default)]
    title: Option<String>,
}

/// Planned intents folded into one (`workriver backlog merge`, or the roadmap in the page).
async fn merge(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(m): Json<Merge>) -> Result<Json<core::Event>, ApiError> {
    // The user, or an agent at its pane: the event rules keep the user's own intents the user's.
    let who = app.who(&caller, caller.place.as_deref(), None)?;
    blocking(move || app.record(&who, Body::IntentMerged { intent: m.into, from: m.from, title: m.title }).map(Json)).await
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
struct StartIntent {
    intent: String,
    /// The runtime group (Herdr workspace) it starts in, by id.
    workspace: String,
    /// The user was told the workspace works on another project and starts it there anyway.
    #[serde(default, rename = "confirmOtherProject")]
    confirm_other_project: bool,
}

/// The user starts a planned intent in a workspace from the page, as its manager would with
/// `workriver intent start`; the workspace's manager is told to split it.
async fn start_intent(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(s): Json<StartIntent>) -> Result<Json<serde_json::Value>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || {
        // The workspace as the runtime knows it, never as the page names it.
        let groups = app.runtime.groups().map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
        let group = groups.into_iter().find(|g| g.id == s.workspace).ok_or_else(|| bad(format!("no workspace {} in the runtime", s.workspace)))?;
        // Its manager would get work from a project it is not in: only once the user confirmed.
        if !s.confirm_other_project {
            let live = app.runtime.agents().unwrap_or_default();
            if let Some(why) = snapshot::other_project(app.store.lock().unwrap().state(), &live, &group, &s.intent) {
                return Err(ApiError(StatusCode::CONFLICT, format!("{why}: its manager would get work from a project it is not in. Pick a workspace of that project, or confirm starting it there anyway (confirmOtherProject)")));
            }
        }
        let e = app.record(&who, Body::IntentStateChanged { intent: s.intent.clone(), state: core::IntentState::Active, reason: None, workspace: Some(group) })?;
        let live = app.runtime.agents().ok();
        let plan = deliver::started(app.store.lock().unwrap().state(), live.as_deref(), &s.intent);
        let name = |to: &str| app.store.lock().unwrap().state().name_of(to).unwrap_or(to).to_string();
        let delivery = match plan {
            Ok((to, message)) => match app.send(&to, &message) {
                Ok(()) => Delivery { step: DeliveryStep::Delivered, to: Some(name(&to)), reason_code: None, reason: None, message },
                Err(err) => Delivery { step: DeliveryStep::NotDelivered, to: Some(name(&to)), reason_code: Some(if matches!(err, RuntimeError::None) { ReasonCode::RuntimeDown } else { ReasonCode::RuntimeError }), reason: Some(err.to_string()), message },
            },
            Err((code, reason)) => Delivery { step: DeliveryStep::NotDelivered, to: None, reason_code: Some(code), reason: Some(reason), message: String::new() },
        };
        // As the page names a delivery's fields.
        let delivery = json!({ "step": delivery.step, "to": delivery.to, "reasonCode": delivery.reason_code, "reason": delivery.reason, "message": delivery.message });
        Ok(Json(json!({ "event": e, "delivery": delivery })))
    })
    .await
}

#[derive(Deserialize)]
struct RunPrepare {
    command: String,
    /// Where it runs: a unit's work tree (on its machine), or a project's folder.
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    project: Option<String>,
    /// Whose step it is: a task's (`u1#3`) or a chore's (`c2`); none: the user's own.
    #[serde(default)]
    from: Option<String>,
}

/// How long a prepared command may wait for the user's confirmation.
const CONFIRM: Duration = Duration::from_secs(600);

/// A run as the page shows it: in the confirm dialog, then with what its pane shows.
fn run_view(id: &str, r: &Run, state: &core::State) -> serde_json::Value {
    let by = if r.by == "user" { "user" } else { state.name_of(&r.by).unwrap_or(&r.by) };
    json!({ "id": id, "command": r.command, "dir": r.dir, "machine": r.machine, "unit": r.unit, "by": by,
            "started": r.place.is_some(), "output": r.output, "exit": r.exit, "watching": r.watching })
}

/// The user is about to run a command: where it would run and who wrote it, for the confirm
/// dialog. Nothing runs here; `run_command` runs it, and only with this exact text.
async fn run_prepare(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(p): Json<RunPrepare>) -> Result<Json<serde_json::Value>, ApiError> {
    app.user(&caller)?;
    blocking(move || {
        run::check(&p.command).map_err(bad)?;
        let state = app.store.lock().unwrap().state().clone();
        let (dir, machine, unit) = match (&p.unit, &p.project) {
            (Some(u), _) => {
                let u = state.unit(u)?;
                let at = app.runner(u.machine.as_deref())?;
                // A research unit has no work tree: its project's folder.
                let tree = scan::worktree_of(&at, Path::new(&u.repo), &u.branch).ok().flatten().filter(|_| u.has_code());
                (tree.map(|d| d.to_string_lossy().into_owned()).unwrap_or_else(|| u.repo.clone()), u.machine.clone(), Some(u.id.clone()))
            }
            (None, Some(f)) => {
                if !state.projects.iter().any(|x| &x.path == f) && !state.intents.iter().any(|i| &i.project == f) {
                    return Err(bad(format!("{f} is not a project's folder")));
                }
                // A repository on another machine is a project of its own: it runs there.
                let machine = state.units.iter().find(|u| Path::new(&u.repo).starts_with(f)).and_then(|u| u.machine.clone());
                (f.clone(), machine, None)
            }
            (None, None) => return Err(bad("say where it runs: a unit (its work tree) or a project's folder".into())),
        };
        let by = match p.from.as_deref() {
            None => "user".to_string(),
            Some(f) => match core::parse_task_ref(f) {
                Some((u, n)) => state.task(u, n)?.by.clone(),
                None => state.chores.iter().find(|c| c.id == f).map(|c| c.by.clone()).ok_or_else(|| bad(format!("no chore {f}")))?,
            },
        };
        let id = auth::random_hex(8).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let r = Run { command: p.command, dir, machine, unit, by, prepared: std::time::Instant::now(), place: None, output: String::new(), exit: None, watching: false };
        let view = run_view(&id, &r, &state);
        let mut runs = app.runs.lock().unwrap();
        runs.retain(|_, r| r.place.is_some() || r.prepared.elapsed() < CONFIRM);
        runs.insert(id, r);
        Ok(Json(view))
    })
    .await
}

#[derive(Deserialize)]
struct RunConfirmed {
    id: String,
    /// The exact text the user was shown and confirmed.
    command: String,
}

/// Runs a prepared command once the user confirmed it: typed into a plain shell of the user's,
/// in workriver's own runtime group, on the machine of its directory. Recorded (not its
/// output); its end is recorded when its pane shows it.
async fn run_command(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(c): Json<RunConfirmed>) -> Result<Json<serde_json::Value>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || {
        // Claimed under the lock: one confirmation runs once.
        let r = {
            let mut runs = app.runs.lock().unwrap();
            let r = runs.get_mut(&c.id).filter(|r| r.place.is_none() && !r.watching && r.prepared.elapsed() < CONFIRM);
            let r = r.ok_or_else(|| bad("nothing confirmed to run under this id (it ran already, or was shown too long ago): open it again".into()))?;
            if r.command != c.command {
                return Err(bad("this is not the command that was confirmed; nothing ran".into()));
            }
            r.watching = true;
            r.clone()
        };
        let line = run::line(&r.dir, &r.command, &c.id);
        let place = match app.runtime.run_shell(r.machine.as_deref(), run::GROUP, &r.dir, &line) {
            Ok(p) => p,
            Err(e) => {
                app.runs.lock().unwrap().remove(&c.id);
                return Err(ApiError(StatusCode::SERVICE_UNAVAILABLE, format!("nothing ran: {e}")));
            }
        };
        if let Some(x) = app.runs.lock().unwrap().get_mut(&c.id) {
            x.place = Some(place);
        }
        app.record(&who, Body::CommandRan { id: c.id.clone(), command: r.command, dir: r.dir, machine: r.machine, unit: r.unit, by: r.by })
            .map_err(|e| ApiError(e.0, format!("it runs, but could not be recorded: {}", e.1)))?;
        let watcher = app.clone();
        let id = c.id.clone();
        std::thread::spawn(move || watcher.watch_run(&id));
        Ok(Json(json!({ "id": c.id })))
    })
    .await
}

/// A run: what its pane shows now, and its exit status once it ended.
async fn run_read(State(app): State<Shared>, Extension(caller): Extension<Caller>, UrlPath(id): UrlPath<String>) -> Result<Json<serde_json::Value>, ApiError> {
    app.user(&caller)?;
    let r = app.runs.lock().unwrap().get(&id).cloned().ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("no run {id}")))?;
    Ok(Json(run_view(&id, &r, app.store.lock().unwrap().state())))
}

/// Brings a run's terminal to the front.
async fn run_focus(State(app): State<Shared>, Extension(caller): Extension<Caller>, UrlPath(id): UrlPath<String>) -> Result<StatusCode, ApiError> {
    app.user(&caller)?;
    let r = app.runs.lock().unwrap().get(&id).cloned().ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("no run {id}")))?;
    let place = r.place.ok_or_else(|| bad(format!("run {id} has not started")))?;
    blocking(move || {
        app.runtime.focus_place(r.machine.as_deref(), &place).map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

impl App {
    /// Reads a run's pane every second until it shows the end (then records the exit status),
    /// its pane is gone, or a day has passed.
    fn watch_run(&self, id: &str) {
        let Some((machine, place)) = self.runs.lock().unwrap().get(id).and_then(|r| Some((r.machine.clone(), r.place.clone()?))) else { return };
        let (until, mut misses) = (std::time::Instant::now() + Duration::from_secs(24 * 3600), 0);
        // A pane that cannot be read five times running is gone (closed, or its shell exited).
        while std::time::Instant::now() < until && misses < 5 {
            std::thread::sleep(Duration::from_secs(1));
            let Ok(text) = self.runtime.read_place(machine.as_deref(), &place, run::LINES) else {
                misses += 1;
                continue;
            };
            misses = 0;
            let (output, end) = run::read(&text, id);
            if let Some(r) = self.runs.lock().unwrap().get_mut(id) {
                (r.output, r.exit) = (output, end);
            }
            if let Some(exit) = end {
                if let Err(e) = self.record(&Principal::Service, Body::CommandEnded { id: id.to_string(), exit }) {
                    tracing::warn!("cannot record run {id} ended: {}", e.1);
                }
                break;
            }
        }
        if let Some(r) = self.runs.lock().unwrap().get_mut(id) {
            r.watching = false;
        }
    }
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
            Some(_) => json!(crate::machines::attachment_url(&path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())),
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

#[derive(Deserialize)]
struct Say {
    /// The intent whose workspace's manager it goes to; or, without one, the workspace's label.
    #[serde(default)]
    intent: String,
    #[serde(default)]
    workspace: String,
    text: String,
}

/// The user writes to a manager from the page; it is typed into the manager's terminal.
async fn message(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(m): Json<Say>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || app.record(&who, Body::MessageSent { id: String::new(), intent: m.intent, workspace: m.workspace, manager: String::new(), text: m.text }).map(Json)).await
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
fn task_commits(at: &scan::Runner, u: &core::Unit, task: u32, cache: &mut snapshot::Cache) -> Vec<(String, String)> {
    let repo = Path::new(&u.repo);
    let hashes = if u.merged.is_some() && !u.merged_commits.is_empty() {
        u.merged_commits.clone()
    } else {
        snapshot::unit_commits(at, u, cache).map(|(l, _)| l.into_iter().map(|c| c.hash).collect()).unwrap_or_default()
    };
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
    let asset = Ui::get(path).filter(|_| !path.is_empty());
    // Built files carry their content's hash in their name: kept for good. The page itself
    // names the current ones, so it is asked for again every time.
    let cache = if asset.is_some() && path.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
    match asset.or_else(|| Ui::get("index.html")) {
        Some(f) => ([(header::CONTENT_TYPE, f.metadata.mimetype().to_string()), (header::CACHE_CONTROL, cache.to_string())], f.data).into_response(),
        None => (StatusCode::NOT_FOUND, "The web UI is not built: run `npm run build` in frontend/web, then build workriver again.").into_response(),
    }
}

/// Binds 127.0.0.1:`port`, or says plainly why not.
pub async fn bind(port: u16) -> anyhow::Result<tokio::net::TcpListener> {
    tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .with_context(|| format!("cannot listen on 127.0.0.1:{port}; is workriver (or something else) already running there? Pick another with --port"))
}
