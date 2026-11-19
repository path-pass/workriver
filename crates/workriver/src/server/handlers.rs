// The daemon: one process on 127.0.0.1 that records events, reads git, and serves the
// web UI with its data and a stream of "something changed".
use std::collections::HashSet;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{StatusCode, header};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::{broadcast, watch};
use tokio_stream::{Stream, StreamExt, wrappers::{BroadcastStream, WatchStream}};
use workriver_core::runtime::{AgentState, Runtime, RuntimeAgent, RuntimeError};
use workriver_core::{self as core, Body, Delivery, DeliveryStep, Principal, ReasonCode};
use workriver_data::Store;
use workriver_scan as scan;

use crate::runtime::follow::Machines;
use crate::runtime::words;
use crate::server::machines::metrics::Metrics;
use crate::server::access::auth::{self, Auth};
use crate::server::access::caller::{self, Callers};
use crate::server::access::pins::Pins;
use crate::common::process::Procs;
use crate::server::delivery;
use crate::server::files::{self, store};
use crate::{ask, run};
use crate::server::access::*;
use crate::server::http::*;
use crate::server::views::{self as snapshot, Cache};

pub struct App {
    pub(super) store: Mutex<Store>,
    cache: Mutex<Cache>,
    /// Wakes the background read of repositories on other machines.
    remote_kick: std::sync::mpsc::Sender<()>,
    pub(super) runtime: Arc<dyn Runtime>,
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
    stalls: Mutex<std::collections::HashMap<(String, u32), delivery::reminders::Watch>>,
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
    pub(super) auth: Auth,
    /// A caller may name the agent it records as (`--as`), and one at no place needs no
    /// terminal to be the user: for tests and development only.
    pub(super) allow_as: bool,
    /// The proofs given to the processes that connected to the caller socket.
    pub(super) callers: Arc<Callers>,
    /// Reads a process's chain of parents: this system's (a test gives its own).
    pub(super) procs: Arc<dyn Procs>,
    /// One session, one place: the root each session was first seen at.
    pub(super) pins: Pins,
    /// Held while the list of managers told of this build is read and written.
    told: Mutex<()>,
    /// Commands the user is about to run (prepared, shown in the confirm dialog) and those run,
    /// by id, with what their place last showed.
    // ponytail: in memory; a restart forgets them (what ran stays in the record).
    runs: Mutex<std::collections::HashMap<String, Run>>,
}

/// A run's shell to close: the run, its machine (none: this one), its place.
type Shell = (String, Option<String>, String);

/// A command the user prepared to run, then ran (`place` set).
#[derive(Clone)]
struct Run {
    command: String,
    dir: String,
    machine: Option<String>,
    unit: Option<String>,
    /// Who wrote it: an agent's session, or "user".
    by: String,
    /// Whose step it is (`u1#3`, `c2`) and which (from 1); none: typed by the user.
    from: Option<String>,
    step: Option<u32>,
    /// That step's text, as its author wrote it: what the command is for.
    step_text: Option<String>,
    /// The language of the user's page, for the review.
    lang: Option<String>,
    prepared: std::time::Instant,
    place: Option<String>,
    /// Its place was closed by workriver, is about to be, or is no longer a run's shell: no
    /// terminal to open any more. Taken back when the shell was left (busy, or not reached).
    closed: bool,
    output: String,
    exit: Option<i32>,
    /// Its place is still read.
    watching: bool,
    /// `command.ran` is recorded: its review, once there, is recorded too.
    ran: bool,
    review: Review,
}

/// What a workriver agent says a command will do, before the user runs it: it only warns.
#[derive(Clone, Default)]
struct Review {
    /// The agent started for it: only it may record the review; closed once it did.
    agent: Option<RuntimeAgent>,
    /// When it is given up.
    until: Option<std::time::Instant>,
    verdict: Option<core::Verdict>,
    text: String,
    /// Why there is no review.
    failed: Option<String>,
}

impl Review {
    fn pending(&self) -> bool {
        self.verdict.is_none() && self.failed.is_none()
    }
}

/// This build of the service, as `workriver status` and the guides name it.
pub const BUILD: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("WORKRIVER_COMMIT"), ")");

/// What else the daemon is started with.
pub struct Options {
    pub warning: Option<String>,
    pub waits: Waits,
    pub machines: Option<Arc<Machines>>,
    pub allow_as: bool,
    /// Read the machines' CPU and memory (the service; tests that do not look at it do not).
    pub sample: bool,
    /// What the service reads of processes: the system's (`process::System`), or a test's.
    pub procs: Arc<dyn Procs>,
    /// The proofs it gives on its caller socket, each for a process's chain.
    pub callers: Arc<Callers>,
}

/// A question's message (or the user's message to a manager) held for its agent, which
/// waits for a person in its terminal.
// ponytail: in memory; after a restart the question fails as `restarted`, like any open one.
struct Held {
    /// The question's id; the user's message's; or the run's whose command is to be reviewed.
    question: String,
    kind: For,
    /// The session or live name it goes to, and the name to say.
    to: String,
    name: String,
    text: String,
    /// A new agent not ready for a prompt: retried at each poll until then; after it, it may
    /// sit on a prompt the runtime does not report and waits for the user like a blocked one.
    not_ready: Option<std::time::Instant>,
}

/// What a held message is for.
#[derive(Clone, Copy, PartialEq)]
enum For {
    Question,
    /// The user's message to a manager.
    Message,
    /// A command for its review agent (`App::review_run`).
    Review,
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
/// the runtime for a while (a restart, a moved place) and come back.
const WATCH: Duration = Duration::from_secs(30 * 60);

pub(super) type Shared = Arc<App>;

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

/// Following other machines too when `machines` is given (`runtime` is then `machines` itself).
pub async fn serve_with(listener: tokio::net::TcpListener, store: Store, runtime: Arc<dyn Runtime>, options: Options) -> anyhow::Result<()> {
    let Options { warning, waits, machines, allow_as, sample, procs, callers } = options;
    let port = listener.local_addr()?.port();
    let store_home = store.path().parent().map(Path::to_path_buf).unwrap_or_default();
    let (changed, _) = broadcast::channel(16);
    let shared = Arc::new(Mutex::new(snapshot::Shared::default()));
    let (kick, kicked) = std::sync::mpsc::channel::<()>();
    let auth = Auth::open(&store_home).with_context(|| format!("cannot make or read the user token in {}", store_home.display()))?;
    if allow_as {
        tracing::warn!("{}", words::AS_ALLOWED);
    }
    caller::listen(&store_home, callers.clone(), procs.clone())?;
    let metrics = Metrics::new(store_home.clone(), scan::Runner::Local, machines.clone(), runtime.clone(), changed.clone(), crate::server::machines::metrics::STEP);
    if sample {
        metrics.start();
    }
    let app = Arc::new(App {
        metrics,
        auth,
        allow_as,
        callers,
        procs,
        pins: Pins::open(&store_home),
        told: Mutex::default(),
        store: Mutex::new(store),
        cache: Mutex::new(Cache::following(machines.clone()).sharing(shared.clone()).remembering(store_home.join(crate::common::home::SCAN_CACHE).join(crate::common::home::SCAN_CACHE_LOCAL))),
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
        stall_minutes: crate::runtime::Settings::stall_minutes(&store_home).into(),
        ready_hours: crate::runtime::Settings::ready_hours(&store_home),
        idle_minutes: crate::runtime::Settings::idle_minutes(&store_home),
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
    let mut bg = Cache::following(app.machines.clone()).sharing(shared).remembering(app.home.join(crate::common::home::SCAN_CACHE).join(crate::common::home::SCAN_CACHE_REMOTE));
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
            tracing::info!(target: "perf", kind = "remote_read", ms, recomputed = bg.recomputed, reused = bg.reused, changed, slow = ms >= crate::common::config::defaults::SLOW_MS);
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
    let held: Vec<String> = app.store.lock().unwrap().state().messages.iter().filter(|m| m.delivery.is_none() && !m.by_manager).map(|m| m.id.clone()).collect();
    for id in held {
        app.message_not_delivered(&id, None, ReasonCode::Restarted, "the service restarted while the message waited for its manager");
    }
    // ponytail: a thread's temporary agent from a previous run is closed rather than adopted;
    // the next question starts a new one with the thread so far.
    let left: Vec<String> = app.store.lock().unwrap().state().threads.iter().filter_map(|t| t.name.clone()).collect();
    for name in left {
        let _ = app.runtime.close(&name);
    }
    // So is an agent a previous run started to review a command: what it would say is lost.
    let left: Vec<String> = app.runtime.agents().unwrap_or_default().into_iter().filter(|a| a.name.starts_with(&run::reviewer("")) && a.group.as_ref().is_some_and(|g| g.label == run::GROUP)).map(|a| a.name).collect();
    for name in left {
        let _ = app.runtime.close(&name);
    }
    // Nothing that signs anyone in is ever written here: this output is on a screen (a
    // terminal of the runtime's) that agents can read. The user opens the page with `workriver open`.
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
        self.see_roots();
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
                    None => match h.kind {
                        For::Message => self.message_not_delivered(&h.question, Some(&h.to), ReasonCode::ManagerGone, &format!("{} closed before it could be given the message", h.name)),
                        For::Question => self.fail_question(&h.question, ReasonCode::RuntimeError, &format!("{} closed before it could be given the question", h.name)),
                        For::Review => self.review_failed(&h.question, &format!("{} closed before it could be given the command", h.name)),
                    },
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
        // Reviews of commands not given in time: their agents go.
        let late: Vec<String> = self.runs.lock().unwrap().iter().filter(|(_, r)| r.review.pending() && r.review.until.is_some_and(|u| u <= now)).map(|(id, _)| id.clone()).collect();
        for id in late {
            self.review_failed(&id, &format!("no review within {} seconds", self.waits.answer.as_secs()));
        }
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

    /// Works out which running tasks' agents stopped (see `reminders::step`): the agent itself is
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
            let (next, found) = delivery::reminders::step(state, live, &idle, threshold, self.waits.minute, &read, &stalls);
            for f in &found {
                let to = match f.act {
                    delivery::reminders::Act::Remind => Some(f.agent.as_str()),
                    delivery::reminders::Act::Escalate => {
                        let stopped = |m: &str| m == f.agent || state.name_of(m) == Some(f.agent.as_str());
                        state.intent_of(&f.owner).and_then(|i| state.manager_of(i)).filter(|m| !stopped(m) && live.iter().any(|a| a.session == *m && a.state != AgentState::Unknown))
                    }
                };
                if let Some(to) = to
                    && let Some(text) = delivery::reminders::message(state, f)
                {
                    tell.push((to.to_string(), text));
                }
            }
            let seen = |m: &std::collections::HashMap<(String, u32), delivery::reminders::Watch>| m.iter().filter_map(|(k, w)| Some((k.clone(), w.stall.clone()?))).collect::<Vec<_>>();
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

    /// Workers idle with no task for `idle_minutes` (see `reminders::idle_workers`): their manager
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
            delivery::reminders::idle_workers(state, live, &idle, threshold, &read)
                .into_iter()
                .filter(|w| !told.contains(&(w.session.clone(), w.since.clone())) && reachable(&w.manager))
                .filter_map(|w| Some((w.manager.clone(), (w.session.clone(), w.since.clone()), delivery::reminders::idle_message(state, &w)?)))
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
            // A spike is never merged: nothing to remind of.
            for u in state.units.iter().filter(|u| u.has_code() && u.kind.merges()) {
                let Some(since) = state.ready_since(u) else { continue };
                let Some(to) = state.manager_of(&u.intent).filter(|m| live.iter().any(|a| a.session == *m && a.state != AgentState::Unknown) && state.blind_tasks(m).is_empty()) else { continue };
                let (id, branch) = (&u.id, delivery::quote::name(&u.branch));
                if let Some(s) = squash.get(id) {
                    let key = format!("squash {id} {}", s.tip);
                    let short: String = s.commit.chars().take(9).collect();
                    let text = format!(
                        "[workriver] {id} ({branch}) looks merged into {} by squash: every change of it is there (commit {short} matches). If so, record it: `workriver unit merged {id}`; if not, ignore this. How: `workriver guide how closing-units`.",
                        delivery::quote::name(&s.into)
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
    pub(super) fn page(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Where git runs for a repository on `machine`; an error while it cannot be reached.
    pub(super) fn runner(&self, machine: Option<&str>) -> Result<scan::Runner, ApiError> {
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
            Some(_) => self.runtime.send(to, &files::links(text, &format!("{}/", crate::common::home::attachments_dir(&self.home).display()))),
            None => self.runtime.send(to, text),
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
        tracing::info!(target: "perf", kind = "snapshot", trigger, light, ms, recomputed = cache.recomputed, reused = cache.reused, git = scan::CALLS.load(std::sync::atomic::Ordering::Relaxed) - calls, slow = ms >= crate::common::config::defaults::SLOW_MS);
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
    pub(super) fn tell_of_update(&self, session: &str, fresh: bool) {
        if !fresh && !self.store.lock().unwrap().state().workspaces.iter().any(|w| w.manager.as_deref() == Some(session)) {
            return;
        }
        let _one = self.told.lock().unwrap();
        let path = self.home.join(crate::common::home::GUIDE_TOLD);
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
    pub(super) fn seen(&self, a: &RuntimeAgent) -> Result<(), ApiError> {
        let known = self.store.lock().unwrap().state().name_of(&a.session) == Some(a.name.as_str());
        if !known {
            self.record(&Principal::Service, Body::AgentSeen { session: a.session.clone(), name: a.name.clone(), harness: a.harness.clone() })?;
        }
        Ok(())
    }

    /// One workspace, one manager: an agent manages the workspace it runs in (not the one its
    /// CLI names), and takes it over only from a manager no longer running.
    fn may_manage(&self, session: &str, machine: Option<&str>, intent: Option<&str>, workspace: Option<&core::RuntimeRef>) -> Result<(), ApiError> {
        let state = self.store.lock().unwrap().state().clone();
        // Before 0.2 a manager named its intent; it manages that intent's workspace.
        let Some(g) = workspace.cloned().or_else(|| intent.and_then(|i| state.intents.iter().find(|x| x.id == i)?.workspace.clone())) else { return Ok(()) };
        let live = self.runtime.agents().map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
        let me = live.iter().find(|a| a.session == session && a.machine.as_deref() == machine);
        if !me.and_then(|a| a.group.as_ref()).is_some_and(|m| m.id == g.id && m.machine == g.machine) {
            return Err(ApiError(StatusCode::FORBIDDEN, words::MANAGE_YOUR_OWN.into()));
        }
        let current = state.workspace_of(&g).and_then(|w| w.manager.as_deref()).filter(|m| *m != session);
        if let Some(other) = current.and_then(|m| live.iter().find(|a| a.session == m)) {
            return Err(ApiError(
                StatusCode::CONFLICT,
                format!("workspace {} has a manager running: {}. A workspace has one manager; tell the user, who closes that agent first if you are to take over", g.label, other.name),
            ));
        }
        Ok(())
    }

    /// Records an event as `who`, found by `App::who`, never taken from the caller's word.
    fn record(&self, who: &Principal, body: Body) -> Result<core::Event, ApiError> {
        let (actor, machine) = (who.actor(), who.machine());
        if let (Body::ManagerRegistered { intent, workspace, .. }, Principal::Agent { session, machine }) = (&body, who) {
            self.may_manage(session, machine.as_deref(), intent.as_deref(), workspace.as_ref())?;
        }
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
    pub(super) fn session_of(&self, agent: &str, near: Option<&str>) -> String {
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
        let Ok(live) = self.runtime.agents() else { return fail(ReasonCode::RuntimeDown, words::NOT_CONNECTED.into()) };
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
            self.hand(Held { question: id.into(), kind: For::Question, to: name.clone(), name, text, not_ready: None });
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
            return fail(ReasonCode::NoWorkspace, words::no_group(&unit.intent));
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
        self.hand(Held { question: id.into(), kind: For::Question, to: agent.name.clone(), name: agent.name, text: prompt, not_ready: None });
    }

    /// Gives a question's message to the agent that answers it (`to`: a session or a live
    /// agent's name). One waiting for a person in its terminal (blocked) cannot take
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
                Ok(()) if h.kind == For::Message => return self.message_delivery(&h.question, Delivery { step: DeliveryStep::Delivered, to: Some(h.to.clone()), reason_code: None, reason: None, message: h.text }),
                Ok(()) => return,
                Err(RuntimeError::Blocked(_)) => h.not_ready = None,
                Err(RuntimeError::NotReady(_)) => not_ready(&mut h),
                Err(e) => {
                    return match h.kind {
                        For::Message => self.message_not_delivered(&h.question, Some(&h.to), ReasonCode::RuntimeError, &format!("cannot give {} the message: {e}", h.name)),
                        For::Question => self.fail_question(&h.question, ReasonCode::RuntimeError, &format!("cannot give {} the question: {e}", h.name)),
                        For::Review => self.review_failed(&h.question, &format!("cannot give {} the command: {e}", h.name)),
                    };
                }
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
            store.state().message(id).ok().map(|m| (m.manager.clone(), delivery::user_message(store.state(), m)))
        }) else {
            return;
        };
        let name = self.store.lock().unwrap().state().name_of(&to).unwrap_or(&to).to_string();
        match self.runtime.agents() {
            Err(_) => self.message_not_delivered(id, None, ReasonCode::RuntimeDown, words::NOT_CONNECTED),
            Ok(live) if !live.iter().any(|a| a.session == to) => self.message_not_delivered(id, None, ReasonCode::ManagerGone, &format!("the manager {name} is not running")),
            Ok(_) => self.hand(Held { question: id.into(), kind: For::Message, to, name, text, not_ready: None }),
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
            store.state().message(id).ok().map(|m| delivery::user_message(store.state(), m)).unwrap_or_default()
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
                    Body::FindingNoted { .. } => delivery::notes(store.state(), live.as_deref(), &self.home, e),
                    Body::TaskDropped { .. } => delivery::dropped(store.state(), live.as_deref(), e),
                    Body::ManagerTold { .. } => delivery::told(store.state(), live.as_deref(), e),
                    _ => delivery::finished(store.state(), live.as_deref(), e),
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
        let plan = delivery::plan(self.store.lock().unwrap().state(), live.as_deref(), &self.home, e);
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

    /// Who and where the CLI is: the agent at the place its process runs at (recorded by name
    /// the first time), that place's group, and the group's current intent, if any. At no
    /// place: nobody, in no group. Answered without the token, but not without a process.
    fn here(&self, caller: &Caller) -> Result<serde_json::Value, ApiError> {
        if caller.machine.is_none() && caller.chain.is_none() {
            return Err(ApiError(StatusCode::UNAUTHORIZED, words::NO_PROCESS.into()));
        }
        let chain = self.chain_of(caller)?;
        let Some(root) = self.place(caller, &chain)? else {
            return Ok(json!({ "actor": null, "name": null, "group": null, "intent": null }));
        };
        let at = &root.at;
        if let Some(a) = &at.agent {
            self.one_place(caller.machine.as_deref(), &root, a, &chain)?;
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
        if let Some(p) = snapshot::project_of(&state, &path) {
            return Ok(p.path.clone());
        }
        scan::toplevel(&scan::Runner::Local, Path::new(&path)).map(|t| t.to_string_lossy().into_owned()).map_err(|_| bad(format!("{path} is neither a registered project nor a git repository; register it with `workriver project add`")))
    }
}


pub(super) async fn workspace(State(app): State<Shared>, Extension(caller): Extension<Caller>, Query(r): Query<Reader>) -> Result<Json<snapshot::Workspace>, ApiError> {
    blocking(move || {
        let state = app.view(&caller, &r)?;
        let mut ws = app.snapshot(&state, r.light, "workspace");
        // Machines in alert are the user's to see (Needs you), like their CPU and memory.
        // So are the commands they ran from the page.
        if app.user(&caller).is_ok() {
            ws.alerts = app.metrics.alerts();
            ws.runs = app.recent_runs(&state);
        }
        let found = std::mem::take(&mut app.cache.lock().unwrap().merged_now);
        app.record_merges(found);
        Ok(Json(ws))
    })
    .await
}

/// One unit's file texts, which the snapshot leaves out: what the page's code layer shows.
/// Scoped like `/api/workspace`. Not on a machine's listener: no CLI reads it.
pub(super) async fn unit_code(State(app): State<Shared>, Extension(caller): Extension<Caller>, UrlPath(id): UrlPath<String>, Query(r): Query<Reader>) -> Result<Json<snapshot::Code>, ApiError> {
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
pub(super) struct ShowQuery {
    /// `u1#3`; none: the task the calling agent is on.
    #[serde(default)]
    task: Option<String>,
    #[serde(default, rename = "as")]
    named: Option<String>,
}

/// One task with everything its reader may see (`workriver task show`). Scoped like every
/// read; attachments are paths on this machine and links through the tunnel on another.
pub(super) async fn task_show(State(app): State<Shared>, Extension(caller): Extension<Caller>, Query(q): Query<ShowQuery>) -> Result<Json<crate::common::wire::TaskShow>, ApiError> {
    blocking(move || {
        let r = Reader { named: q.named, light: true, have: String::new() };
        let who = app.who(&caller, r.named.as_deref())?;
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
                        let list: Vec<String> = several.iter().map(|(u, n, t)| format!("{u}#{n} \"{}\"", delivery::quote::name(t))).collect();
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
            let path = store::path(&app.home, &a.id, &a.name);
            Some(if remote {
                files::fetch_command(&path.file_name()?.to_string_lossy())
            } else {
                path.display().to_string()
            })
        };
        let mut shown = crate::server::views::task::task(&state, &ws, &unit, n, &file)?;
        shown.link = format!("{}/{unit}/{n}", app.page());
        Ok(Json(shown))
    })
    .await
}

pub(super) async fn state(State(app): State<Shared>, Extension(caller): Extension<Caller>, Query(r): Query<Reader>) -> Result<Json<core::State>, ApiError> {
    blocking(move || app.view(&caller, &r).map(Json)).await
}

pub(super) async fn status(State(app): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    let store = app.store.lock().unwrap();
    let s = store.state();
    let (files, bytes) = store::usage(&app.home);
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
        "attachments": { "dir": crate::common::home::attachments_dir(&app.home), "files": files, "bytes": bytes },
    })))
}

/// "Something changed", for the page: the user's.
pub(super) async fn events(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>, ApiError> {
    app.user(&caller)?;
    // A lagging client missed some, which is still one change.
    // It ends when the service stops, or it would hold the stopping up for good.
    let stop = WatchStream::new(app.stopping.subscribe()).filter(|s| *s).map(|_| false);
    let stream = BroadcastStream::new(app.changed.subscribe()).map(|_| true).merge(stop).take_while(|go| *go).map(|_| Ok(SseEvent::default().event("changed").data("changed")));
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ---- the user's session in the page ----

#[derive(Deserialize)]
pub(super) struct Append {
    /// `--as`: taken only where the daemon was started to allow it.
    #[serde(default, rename = "as")]
    named: Option<String>,
    kind: String,
    #[serde(default)]
    body: serde_json::Value,
}

/// Any event, as the CLI or the page records it, as whoever the caller is found to be.
pub(super) async fn append(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(a): Json<Append>) -> Result<Json<serde_json::Value>, ApiError> {
    let body: Body = serde_json::from_value(json!({ "kind": a.kind, "body": a.body })).map_err(|e| bad(format!("{} event: {e}", a.kind)))?;
    blocking(move || {
        let who = app.who(&caller, a.named.as_deref())?;
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
async fn follow(app: &Shared, label: &str, setting: crate::runtime::Setting) -> Result<(), ApiError> {
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
pub(super) async fn machines(State(app): State<Shared>, Extension(caller): Extension<Caller>, Query(r): Query<Reader>) -> Result<Json<Vec<crate::common::wire::MachineState>>, ApiError> {
    app.who(&caller, r.named.as_deref())?;
    Ok(Json(app.machines.as_ref().map(|m| m.list()).unwrap_or_default()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MachineSet {
    label: String,
    /// add, enable, disable or remove.
    action: String,
    #[serde(default)]
    ssh: String,
    /// The runtime adapter's options for the machine, under its own keys and by its own checks.
    #[serde(flatten)]
    options: crate::runtime::MachineOptions,
}

/// CPU and memory of this machine and the followed ones, with the thresholds: read by the
/// user (the page) and by an agent at its place here (to weigh where to put work); set by
/// the user alone. Not on a machine's listener.
pub(super) async fn metrics(State(app): State<Shared>, Extension(caller): Extension<Caller>, Query(r): Query<Reader>) -> Result<Json<crate::common::wire::Report>, ApiError> {
    app.who(&caller, r.named.as_deref())?;
    let mut report = app.metrics.report();
    // An agent on another machine sees that machine alone: nothing of this one or the others.
    if let Some(m) = caller.machine.as_deref() {
        report.machines.retain(|x| !x.here && x.label == m);
    }
    Ok(Json(report))
}

#[derive(Deserialize)]
pub(super) struct HistoryQuery {
    machine: String,
    #[serde(default = "a_day")]
    hours: u32,
    #[serde(default, rename = "as")]
    named: Option<String>,
}

fn a_day() -> u32 {
    24
}

/// One machine's minutes over the last `hours` (1 to 168), read as `GET /api/metrics` is:
/// the user, or an agent at its place on this machine.
pub(super) async fn metrics_history(State(app): State<Shared>, Extension(caller): Extension<Caller>, Query(q): Query<HistoryQuery>) -> Result<Json<serde_json::Value>, ApiError> {
    app.who(&caller, q.named.as_deref())?;
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
pub(super) async fn metrics_thresholds(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(t): Json<crate::common::config::Thresholds>) -> Result<Json<crate::common::wire::Report>, ApiError> {
    app.user(&caller)?;
    app.metrics.set_thresholds(t).map_err(bad)?;
    Ok(Json(app.metrics.report()))
}

#[derive(Deserialize)]
pub(super) struct StallSetting {
    minutes: u32,
}

/// After how many idle minutes a running task's agent counts as stopped; the page's Settings.
pub(super) async fn stall_get(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Json<serde_json::Value>, ApiError> {
    app.user(&caller)?;
    Ok(Json(json!({ "minutes": app.stall_minutes.load(std::sync::atomic::Ordering::Relaxed) })))
}

/// Sets it; the user's alone, kept in settings.json like the alert thresholds.
pub(super) async fn stall_set(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(s): Json<StallSetting>) -> Result<Json<serde_json::Value>, ApiError> {
    app.user(&caller)?;
    crate::runtime::Settings::set_stall_minutes(&app.home, s.minutes).map_err(bad)?;
    app.stall_minutes.store(s.minutes, std::sync::atomic::Ordering::Relaxed);
    Ok(Json(json!({ "minutes": s.minutes })))
}

/// Adds, enables, disables or removes a machine; only from this machine.
pub(super) async fn machine_set(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(m): Json<MachineSet>) -> Result<Json<Vec<crate::common::wire::MachineState>>, ApiError> {
    app.user(&caller)?;
    let Some(all) = app.machines.clone() else { return Err(bad("this service follows no other machines".into())) };
    let stop = |label: &str| {
        all.stop(label);
        if let Some(l) = app.listeners.lock().unwrap().remove(label) {
            l.abort();
        }
    };
    match m.action.as_str() {
        "add" => all.add(&m.label, crate::runtime::Setting { ssh: m.ssh, options: m.options, port: 0, enabled: false }).map_err(bad)?,
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
pub(super) struct Decide {
    unit: String,
    task: u32,
    finding: String,
    decision: core::Decision,
    #[serde(default)]
    note: String,
}

pub(super) async fn decide(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(d): Json<Decide>) -> Result<Json<core::Event>, ApiError> {
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
pub(super) struct Gate {
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

pub(super) async fn gate(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(g): Json<Gate>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    let body = Body::GateSubmitted { unit: g.unit, task: g.task, result: g.result, note: g.note, findings: g.findings, attachments: g.attachments };
    blocking(move || app.record(&who, body).map(Json)).await
}

#[derive(Deserialize)]
pub(super) struct ChoreDone {
    chore: String,
}

#[derive(Deserialize)]
pub(super) struct DropTask {
    unit: String,
    task: u32,
    reason: String,
}

/// The user marks a task no longer needed from the page (agents use `workriver task drop`).
pub(super) async fn drop_task(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(d): Json<DropTask>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || app.record(&who, Body::TaskDropped { unit: d.unit, task: d.task, reason: d.reason }).map(Json)).await
}

pub(super) async fn chore_done(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(c): Json<ChoreDone>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || app.record(&who, Body::ChoreDone { chore: c.chore }).map(Json)).await
}

#[derive(Deserialize)]
pub(super) struct Merge {
    into: String,
    from: Vec<String>,
    /// A new title for the intent that stays.
    #[serde(default)]
    title: Option<String>,
}

/// Planned intents folded into one (`workriver backlog merge`, or the roadmap in the page).
pub(super) async fn merge(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(m): Json<Merge>) -> Result<Json<core::Event>, ApiError> {
    // The user, or an agent at its place: the event rules keep the user's own intents the user's.
    let who = app.who(&caller, None)?;
    blocking(move || app.record(&who, Body::IntentMerged { intent: m.into, from: m.from, title: m.title }).map(Json)).await
}

#[derive(Deserialize)]
pub(super) struct Focus {
    agent: String,
}

pub(super) async fn focus(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(f): Json<Focus>) -> Result<axum::response::Response, ApiError> {
    app.user(&caller)?;
    blocking(move || {
        // The page knows agents by name.
        let session = app.session_of(&f.agent, None);
        app.runtime.focus(&session).map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
        // On another machine it comes to the front there, in that machine's runtime: the user's own
        // client stays where it is (the runtime has no way to switch it), so the page says where.
        let there = app.live().unwrap_or_default().into_iter().find(|a| a.session == session && a.machine.is_some());
        Ok(match there {
            Some(a) => Json(json!({ "machine": a.machine, "workspace": a.group.map(|g| g.label), "agent": a.name })).into_response(),
            None => StatusCode::NO_CONTENT.into_response(),
        })
    })
    .await
}

#[derive(Deserialize)]
pub(super) struct StartIntent {
    intent: String,
    /// The runtime group it starts in, by id.
    workspace: String,
    /// The user was told the workspace works on another project and starts it there anyway.
    #[serde(default, rename = "confirmOtherProject")]
    confirm_other_project: bool,
}

/// The user starts a planned intent in a workspace from the page, as its manager would with
/// `workriver intent start`; the workspace's manager is told to split it.
pub(super) async fn start_intent(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(s): Json<StartIntent>) -> Result<Json<serde_json::Value>, ApiError> {
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
        let plan = delivery::started(app.store.lock().unwrap().state(), live.as_deref(), &s.intent);
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
pub(super) struct RunPrepare {
    command: String,
    /// Where it runs: a unit's work tree (on its machine), or a project's folder.
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    project: Option<String>,
    /// Whose step it is: a task's (`u1#3`) or a chore's (`c2`); none: the user's own.
    #[serde(default)]
    from: Option<String>,
    /// Which of its steps (from 1).
    #[serde(default)]
    step: Option<u32>,
    /// The language of the page (`en`, `zh-CN`): its review is written in it.
    #[serde(default)]
    lang: Option<String>,
}

/// How long a prepared command may wait for the user's confirmation.
const CONFIRM: Duration = Duration::from_secs(600);

/// A run as the page shows it: in the confirm dialog, then with what its place shows.
/// `waiting`: its review agent, when it waits for a person in its terminal (a first-run
/// prompt such as "trust this folder?"): the page says so and leads there.
fn run_view(id: &str, r: &Run, state: &core::State, waiting: Option<&RuntimeAgent>) -> serde_json::Value {
    let by = if r.by == "user" { "user" } else { state.name_of(&r.by).unwrap_or(&r.by) };
    let v = &r.review;
    let review = match (&v.verdict, &v.failed) {
        (Some(verdict), _) => json!({ "state": "done", "verdict": verdict, "text": v.text }),
        (None, Some(why)) => json!({ "state": "failed", "reason": why }),
        (None, None) => match waiting {
            Some(a) => json!({ "state": "blocked", "agent": a.name, "session": a.session }),
            None => json!({ "state": "pending" }),
        },
    };
    json!({ "id": id, "command": r.command, "dir": r.dir, "machine": r.machine, "unit": r.unit, "by": by,
            "started": r.place.is_some(), "output": r.output, "exit": r.exit, "watching": r.watching,
            "kept": true, "focusable": r.place.is_some() && !r.closed, "review": review })
}

/// A run this service no longer follows (it restarted since): only what the record says,
/// without its output or its terminal.
fn recorded_view(r: &core::CommandRun, state: &core::State) -> serde_json::Value {
    let by = if r.by == "user" { "user" } else { state.name_of(&r.by).unwrap_or(&r.by) };
    let review = r.review.as_ref().map(|(verdict, text)| json!({ "state": "done", "verdict": verdict, "text": text }));
    json!({ "id": r.id, "command": r.command, "dir": r.dir, "machine": r.machine, "unit": r.unit, "by": by,
            "started": true, "output": "", "exit": r.exit, "watching": false, "kept": false, "focusable": false, "review": review })
}

impl App {
    /// The user's recent runs, newest first: the record's, with what this service still
    /// follows of them (whether a place is read, where it is).
    fn recent_runs(&self, state: &core::State) -> Vec<snapshot::RunOut> {
        let live = self.runs.lock().unwrap();
        state
            .runs
            .iter()
            .rev()
            .map(|r| {
                let here = live.get(&r.id);
                let exit = r.exit.or(here.and_then(|x| x.exit));
                let now = if exit.is_some() { "done" } else if here.is_some_and(|x| x.watching) { "running" } else { "unknown" };
                snapshot::RunOut {
                    id: r.id.clone(),
                    command: r.command.clone(),
                    dir: r.dir.clone(),
                    machine: r.machine.clone(),
                    unit: r.unit.clone(),
                    from: r.from.clone(),
                    step: r.step,
                    at: r.at.clone(),
                    state: now,
                    exit,
                    focusable: here.is_some_and(|x| x.place.is_some() && !x.closed),
                    verdict: r.review.as_ref().map(|(v, _)| *v).or(here.and_then(|x| x.review.verdict)),
                }
            })
            .collect()
    }
}

/// The user is about to run a command: where it would run and who wrote it, for the confirm
/// dialog. Nothing runs here; `run_command` runs it, and only with this exact text.
pub(super) async fn run_prepare(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(p): Json<RunPrepare>) -> Result<Json<serde_json::Value>, ApiError> {
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
        let step = p.from.as_ref().and(p.step);
        let (by, steps) = match p.from.as_deref() {
            None => ("user".to_string(), &[][..]),
            Some(f) => match core::parse_task_ref(f) {
                Some((u, n)) => state.task(u, n).map(|t| (t.by.clone(), &t.steps[..]))?,
                None => state.chores.iter().find(|c| c.id == f).map(|c| (c.by.clone(), &c.steps[..])).ok_or_else(|| bad(format!("no chore {f}")))?,
            },
        };
        let step_text = step.and_then(|n| steps.get(n.checked_sub(1)? as usize)).map(|s| s.text.clone());
        let id = auth::random_hex(8).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let r = Run { command: p.command, dir, machine, unit, by, from: p.from, step, step_text, lang: p.lang, prepared: std::time::Instant::now(), place: None, closed: false, output: String::new(), exit: None, watching: false, ran: false, review: Review::default() };
        let view = run_view(&id, &r, &state, None);
        {
            let mut runs = app.runs.lock().unwrap();
            // Kept: what waits for its confirmation or its review, what is watched, the recent
            // runs' output, and an ended run whose shell the next run has still to close.
            runs.retain(|id, r| match r.place {
                None => r.prepared.elapsed() < CONFIRM || r.review.pending(),
                Some(_) => r.watching || (r.exit.is_some() && !r.closed) || state.runs.iter().any(|x| &x.id == id),
            });
            runs.insert(id.clone(), r);
        }
        // Every command is reviewed, in the background: the user may run it before that ends.
        std::thread::spawn(move || app.review_run(&id));
        Ok(Json(view))
    })
    .await
}

#[derive(Deserialize)]
pub(super) struct RunConfirmed {
    id: String,
    /// The exact text the user was shown and confirmed.
    command: String,
}

/// Runs a prepared command once the user confirmed it: typed into a plain shell of the user's,
/// in workriver's own runtime group, on the machine of its directory. Recorded (not its
/// output); its end is recorded when its place shows it.
pub(super) async fn run_command(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(c): Json<RunConfirmed>) -> Result<Json<serde_json::Value>, ApiError> {
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
        // The shells of earlier runs that ended go now, in the background; a failed one
        // stayed until here. Each is claimed under the lock: closed once.
        let ended: Vec<Shell> = {
            let mut runs = app.runs.lock().unwrap();
            let open = runs.iter_mut().filter(|(_, r)| r.exit.is_some() && !r.closed);
            open.filter_map(|(id, r)| {
                let place = r.place.clone()?;
                r.closed = true;
                Some((id.clone(), r.machine.clone(), place))
            })
            .collect()
        };
        let closer = app.clone();
        std::thread::spawn(move || closer.close_shells(ended, Duration::ZERO));
        let line = run::line(&r.dir, &r.command, &c.id);
        let place = match app.runtime.run_shell(r.machine.as_deref(), run::GROUP, &r.dir, &line) {
            Ok(p) => p,
            Err(e) => {
                // Nothing to review any more.
                let gone = app.runs.lock().unwrap().remove(&c.id).filter(|x| x.review.pending());
                if let Some(a) = gone.and_then(|x| x.review.agent) {
                    app.close_later(a.name);
                }
                return Err(ApiError(StatusCode::SERVICE_UNAVAILABLE, format!("nothing ran: {e}")));
            }
        };
        if let Some(x) = app.runs.lock().unwrap().get_mut(&c.id) {
            x.place = Some(place);
        }
        app.record(&who, Body::CommandRan { id: c.id.clone(), command: r.command, dir: r.dir, machine: r.machine, unit: r.unit, by: r.by, from: r.from, step: r.step })
            .map_err(|e| ApiError(e.0, format!("it runs, but could not be recorded: {}", e.1)))?;
        // A review that came before it ran is recorded now; one still to come, when it comes.
        let reviewed = app.runs.lock().unwrap().get_mut(&c.id).and_then(|x| {
            x.ran = true;
            Some((x.review.verdict?, x.review.text.clone()))
        });
        if let Some((verdict, text)) = reviewed {
            app.record_review(&c.id, verdict, text);
        }
        let watcher = app.clone();
        let id = c.id.clone();
        std::thread::spawn(move || watcher.watch_run(&id));
        Ok(Json(json!({ "id": c.id })))
    })
    .await
}

/// A run: what its place shows now, and its exit status once it ended.
pub(super) async fn run_read(State(app): State<Shared>, Extension(caller): Extension<Caller>, UrlPath(id): UrlPath<String>) -> Result<Json<serde_json::Value>, ApiError> {
    app.user(&caller)?;
    let live = app.runs.lock().unwrap().get(&id).cloned();
    let store = app.store.lock().unwrap();
    let state = store.state();
    match (live, state.runs.iter().find(|r| r.id == id)) {
        (Some(r), _) => {
            // Its review agent as the runtime last saw it, when it waits for a person.
            let waiting = r.review.agent.as_ref().filter(|_| r.review.pending()).and_then(|a| app.live()?.into_iter().find(|x| x.name == a.name && x.machine == a.machine && x.state == AgentState::Blocked));
            Ok(Json(run_view(&id, &r, state, waiting.as_ref())))
        }
        // Recorded before this service started: its output was never read here.
        (None, Some(r)) => Ok(Json(recorded_view(r, state))),
        (None, None) => Err(ApiError(StatusCode::NOT_FOUND, format!("no run {id}"))),
    }
}

/// Brings a run's terminal to the front.
pub(super) async fn run_focus(State(app): State<Shared>, Extension(caller): Extension<Caller>, UrlPath(id): UrlPath<String>) -> Result<axum::response::Response, ApiError> {
    app.user(&caller)?;
    let r = app.runs.lock().unwrap().get(&id).cloned().ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("no run {id}")))?;
    let place = r.place.ok_or_else(|| bad(format!("run {id} has not started")))?;
    if r.closed {
        return Err(bad(format!("run {id} ended and its terminal was closed; its output is here")));
    }
    blocking(move || {
        app.runtime.focus_place(r.machine.as_deref(), &place).map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
        // On another machine: in front there; the page says where (see `focus`).
        Ok(match r.machine {
            Some(m) => Json(json!({ "machine": m, "workspace": run::GROUP })).into_response(),
            None => StatusCode::NO_CONTENT.into_response(),
        })
    })
    .await
}

impl App {
    /// Reads a run's place every second until it shows the end (then records the exit status),
    /// its place is gone, or a day has passed.
    fn watch_run(&self, id: &str) {
        let Some((machine, place)) = self.runs.lock().unwrap().get(id).and_then(|r| Some((r.machine.clone(), r.place.clone()?))) else { return };
        let (until, mut misses, mut well) = (std::time::Instant::now() + Duration::from_secs(24 * 3600), 0, false);
        // A place that cannot be read five times running is gone (closed, or its shell exited).
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
                // Ended well: its shell is to go, said with the end so no reader sees one
                // without the other.
                r.closed |= end == Some(0);
            }
            if let Some(exit) = end {
                if let Err(e) = self.record(&Principal::Service, Body::CommandEnded { id: id.to_string(), exit }) {
                    tracing::warn!("cannot record run {id} ended: {}", e.1);
                }
                // Ended well: its output is kept above; its shell goes, below.
                well = exit == 0;
                break;
            }
        }
        if let Some(r) = self.runs.lock().unwrap().get_mut(id) {
            r.watching = false;
        }
        // No longer followed: the page's recent runs show it (an end recorded says so too).
        let _ = self.changed.send(());
        if well {
            self.close_shells(vec![(id.to_string(), machine, place)], CLOSE_DELAY);
        }
    }

    /// Closes, after `wait`, the shells of runs that ended and were claimed for it (`closed`
    /// set): each on its own machine, and only one with nothing left running in it. A shell
    /// that is busy (a background job of the run's, or the user went on working there), or
    /// that could not be asked or closed just now, is left and given back: the next run tries
    /// again. One that is gone, or no longer a run's shell, is never asked about again. Only
    /// places `run_shell` gave a run are ever passed on; a run whose end was not seen
    /// (running, waiting for input) is never claimed. Blocking.
    // ponytail: the places are in memory, so shells left by a service that restarted stay
    // until the user closes them; recording a run's place would let the next start close them.
    fn close_shells(&self, shells: Vec<Shell>, wait: Duration) {
        std::thread::sleep(wait);
        let mut left = false;
        for (id, machine, place) in shells {
            let done = self.shell_idle(machine.as_deref(), &place).and_then(|idle| match idle {
                true => self.runtime.close_shell(machine.as_deref(), run::GROUP, &place).map(|()| true),
                false => Ok(false),
            });
            match done {
                Ok(true) => continue,
                Err(RuntimeError::NoPlace(why)) => {
                    tracing::debug!("the shell of run {id} is not closed: {why}");
                    continue;
                }
                Ok(false) => tracing::debug!("the shell of run {id} is busy: left open"),
                Err(e) => tracing::warn!("cannot close the shell of run {id} now: {e}"),
            }
            if let Some(r) = self.runs.lock().unwrap().get_mut(&id) {
                r.closed = false;
                left = true;
            }
        }
        if left {
            let _ = self.changed.send(());
        }
    }

    /// Whether nothing is left running in a run's shell: nothing but the shell in front, and
    /// nothing the shell started still alive (a job in the background is not in front, so the
    /// machine's own process list is read for it).
    // ponytail: a shell whose prompt keeps a helper process of its own alive always counts as
    // busy and is never closed for the user; safe, and none of the stock shells does.
    fn shell_idle(&self, machine: Option<&str>, place: &str) -> Result<bool, RuntimeError> {
        let (shell, front) = self.runtime.shell_processes(machine, run::GROUP, place)?;
        if front.iter().any(|p| *p != shell) {
            return Ok(false);
        }
        let runner = self.runner(machine).map_err(|e| RuntimeError::Failed(e.1))?;
        let busy = crate::server::machines::metrics::has_children(&runner, shell).map_err(|e| RuntimeError::Failed(format!("cannot list the processes: {e}")))?;
        Ok(!busy)
    }

    /// Starts the agent that tells the user what a prepared command will do, where it would
    /// run (in workriver's own runtime group of that machine), and gives it the command; any
    /// failure is kept on the run. Blocking: it waits for the agent to be ready.
    fn review_run(&self, id: &str) {
        let Some(r) = self.runs.lock().unwrap().get(id).cloned() else { return };
        // The author's harness, which that machine has; else claude.
        let (harness, by) = {
            let store = self.store.lock().unwrap();
            let s = store.state();
            let harness = s.agents.iter().find(|a| a.session == r.by).map(|a| a.harness.clone()).filter(|h| !h.is_empty()).unwrap_or_else(|| "claude".into());
            (harness, if r.by == "user" { "user".to_string() } else { s.name_of(&r.by).unwrap_or(&r.by).to_string() })
        };
        let name = run::reviewer(id);
        let agent = match self.runtime.start_temporary_at(r.machine.as_deref(), run::GROUP, &harness, &name, &r.dir) {
            Ok(a) => a,
            Err(e) => return self.review_failed(id, &format!("cannot start {harness} as {name}: {e}")),
        };
        {
            let mut runs = self.runs.lock().unwrap();
            match runs.get_mut(id) {
                Some(x) => (x.review.agent, x.review.until) = (Some(agent.clone()), Some(std::time::Instant::now() + self.waits.answer)),
                None => {
                    drop(runs);
                    return self.close_later(agent.name);
                }
            }
        }
        let text = run::review_prompt(id, &r.command, &r.dir, &by, r.step_text.as_deref(), r.lang.as_deref());
        // As a question's prompt: held while the agent waits for a person (a first-run prompt).
        self.hand(Held { question: id.into(), kind: For::Review, to: agent.name.clone(), name: agent.name, text, not_ready: None });
    }

    /// There will be no review of run `id`: why, for the page; its agent goes.
    fn review_failed(&self, id: &str, why: &str) {
        self.held.lock().unwrap().retain(|h| !(h.kind == For::Review && h.question == id));
        let agent = {
            let mut runs = self.runs.lock().unwrap();
            let Some(r) = runs.get_mut(id).filter(|r| r.review.pending()) else { return };
            r.review.failed = Some(why.into());
            r.review.agent.clone()
        };
        if let Some(a) = agent {
            self.close_later(a.name);
        }
        let _ = self.changed.send(());
    }

    /// Records run `id`'s review, as workriver: it checked who gave it.
    fn record_review(&self, id: &str, verdict: core::Verdict, text: String) {
        if let Err(e) = self.record(&Principal::Service, Body::CommandReviewed { id: id.into(), verdict, text }) {
            tracing::warn!("cannot record the review of run {id}: {}", e.1);
        }
    }
}

#[derive(Deserialize)]
pub(super) struct RunReview {
    #[serde(default, rename = "as")]
    named: Option<String>,
    verdict: core::Verdict,
    text: String,
}

/// The review of a command the user is about to run, from the agent workriver started for it
/// and from nobody else (not even the user): shown in the run's dialog, recorded with the run.
/// On a machine's listener too, for a review agent of that machine.
pub(super) async fn run_review(State(app): State<Shared>, Extension(caller): Extension<Caller>, UrlPath(id): UrlPath<String>, Json(v): Json<RunReview>) -> Result<Json<serde_json::Value>, ApiError> {
    blocking(move || {
        let who = app.who(&caller, v.named.as_deref())?;
        let text = v.text.trim().to_string();
        if text.is_empty() || text.chars().count() > run::REVIEW_CAP {
            return Err(bad(format!("a review says what the command does, in at most {} characters", run::REVIEW_CAP)));
        }
        // The agent at the caller's place, as the runtime names it now: an agent started on a
        // first-run prompt was known by its place then and by its harness's own session since.
        let named = match &who {
            Principal::Agent { session, machine } => app.runtime.agents().ok().and_then(|l| l.into_iter().find(|a| &a.session == session && &a.machine == machine)).map(|a| a.name),
            _ => None,
        };
        let (agent, ran) = {
            let mut runs = app.runs.lock().unwrap();
            let r = runs.get_mut(&id).ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("no run {id} waits for a review")))?;
            let theirs = r.review.agent.as_ref().is_some_and(|a| {
                matches!(&who, Principal::Agent { session, machine } if *machine == a.machine && (*session == a.session || named.as_deref() == Some(a.name.as_str())))
            });
            if !theirs {
                return Err(ApiError(StatusCode::FORBIDDEN, format!("only the agent workriver started to review run {id} records its review")));
            }
            if !r.review.pending() {
                return Err(bad(format!("run {id} has its review already, or it was given up")));
            }
            (r.review.verdict, r.review.text) = (Some(v.verdict), text.clone());
            (r.review.agent.clone(), r.ran)
        };
        app.held.lock().unwrap().retain(|h| !(h.kind == For::Review && h.question == id));
        if ran {
            app.record_review(&id, v.verdict, text);
        } else {
            let _ = app.changed.send(());
        }
        // Its work is done; a moment later, so this command returns first.
        if let Some(a) = agent {
            app.close_later(a.name);
        }
        Ok(Json(json!({ "id": id })))
    })
    .await
}

/// The CLI asks who and where it is: see `App::here`.
#[derive(Deserialize)]
pub(super) struct Ask {
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
pub(super) struct Upload {
    name: String,
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
pub(super) async fn attach(State(app): State<Shared>, Extension(via): Extension<Caller>, Query(u): Query<Upload>, headers: axum::http::HeaderMap, body: axum::body::Bytes) -> Result<Json<serde_json::Value>, ApiError> {
    let given = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(str::to_string);
    blocking(move || {
        let who = app.who(&via, u.named.as_deref())?;
        // Only the last part of the name: it is shown, never used as a path.
        let name = Path::new(&u.name).file_name().map(|n| n.to_string_lossy().into_owned()).filter(|n| !n.is_empty()).ok_or_else(|| bad("an attachment needs a file name".into()))?;
        let mime = crate::common::files::mime(&name, given.as_deref());
        let (id, path) = store::put(&app.home, &body, &name).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("cannot store {name}: {e}")))?;
        let size = body.len() as u64;
        app.record(&who, Body::AttachmentAdded { id: id.clone(), name: name.clone(), mime: mime.clone(), size, unit: u.unit, task: u.task, finding: u.finding })?;
        // To a caller on another machine the file is where its tunnel shows it.
        let path = match &via.machine {
            Some(_) => json!(files::attachment_url(&path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())),
            None => json!(path),
        };
        Ok(Json(json!({ "id": id, "path": path, "name": name, "type": mime, "size": size })))
    })
    .await
}

/// A stored file. Only images show in the page; anything else downloads, and nothing is sniffed.
pub(super) async fn attachment(State(app): State<Shared>, Extension(caller): Extension<Caller>, UrlPath(id): UrlPath<String>, Query(r): Query<Reader>) -> Result<Response, ApiError> {
    // An agent on another machine is given the link with the file's extension.
    let id = id.split('.').next().unwrap_or_default().to_string();
    // Of what the caller may see: an agent on a blind task, only what it attached itself.
    let viewer = app.clone();
    let state = blocking(move || viewer.view(&caller, &r)).await?;
    let found = state.attachments.iter().find(|a| a.id == id).cloned();
    let a = found.ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("no attachment {id}")))?;
    let file = store::path(&app.home, &a.id, &a.name);
    let bytes = blocking(move || std::fs::read(&file).map_err(|e| ApiError(StatusCode::NOT_FOUND, format!("attachment {id}: {e}")))).await?;
    let image = a.mime.starts_with("image/") && a.mime != "image/svg+xml";
    let disposition = if image { "inline".to_string() } else { format!("attachment; filename=\"{}\"", a.name.replace(['"', '\\', '\r', '\n'], "_")) };
    let mime = if image { a.mime.clone() } else { "application/octet-stream".to_string() };
    Ok(([(header::CONTENT_TYPE, mime), (header::CONTENT_DISPOSITION, disposition), (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string())], bytes).into_response())
}

#[derive(Deserialize)]
pub(super) struct Inbox {
    unit: String,
    task: u32,
}

/// A task's staging folder for screenshots and the like, made if needed.
pub(super) async fn inbox(State(app): State<Shared>, Extension(via): Extension<Caller>, Json(i): Json<Inbox>) -> Result<Json<serde_json::Value>, ApiError> {
    blocking(move || {
        // Only someone (the user, or an agent from its own place) gets a folder made and named.
        app.who(&via, None)?;
        app.store.lock().unwrap().state().task(&i.unit, i.task)?;
        // A caller on another machine keeps the folder on its own disk; the CLI makes it there.
        if via.machine.is_some() {
            return Ok(Json(json!({ "elsewhere": true })));
        }
        let d = crate::common::home::inbox(&app.home, &i.unit, i.task);
        crate::common::home::mkdir(&d).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("cannot make {}: {e}", d.display())))?;
        Ok(Json(json!({ "dir": d })))
    })
    .await
}

#[derive(Deserialize)]
pub(super) struct EndThread {
    thread: String,
}

pub(super) async fn end_thread(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(t): Json<EndThread>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || app.record(&who, Body::ThreadEnded { thread: t.thread }).map(Json)).await
}

#[derive(Deserialize)]
pub(super) struct Say {
    /// The intent whose workspace's manager it goes to; or, without one, the workspace's label.
    #[serde(default)]
    intent: String,
    #[serde(default)]
    workspace: String,
    text: String,
}

/// The user writes to a manager from the page; it is typed into the manager's terminal.
pub(super) async fn message(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(m): Json<Say>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || app.record(&who, Body::MessageSent { id: String::new(), intent: m.intent, workspace: m.workspace, manager: String::new(), text: m.text }).map(Json)).await
}

#[derive(Deserialize)]
pub(super) struct Read {
    workspace: String,
    at: String,
}

/// The page shows the user a workspace's conversation with its manager: read up to `at`.
pub(super) async fn messages_read(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(r): Json<Read>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || app.record(&who, Body::MessagesRead { workspace: r.workspace, at: r.at }).map(Json)).await
}

/// The user asks why: recorded at once; the temporary agent starts in the background.
pub(super) async fn ask_why(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(a): Json<Ask>) -> Result<Json<core::Event>, ApiError> {
    let body = Body::QuestionAsked { id: String::new(), thread: a.thread.unwrap_or_default(), unit: a.unit, task: a.task, text: a.text, context: a.context, conclude: a.conclude };
    let starter = app.clone();
    let e = blocking(move || {
        let who = app.who(&caller, a.named.as_deref())?;
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

/// The CLI asks who and where it is: see `App::here`. Nothing it sends is read for that.
pub(super) async fn here(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Json<serde_json::Value>, ApiError> {
    blocking(move || app.here(&caller).map(Json)).await
}

// ---- the web UI, built into the binary (read from disk in debug builds) ----

/// Binds 127.0.0.1:`port`, or says plainly why not.
pub async fn bind(port: u16) -> anyhow::Result<tokio::net::TcpListener> {
    tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .with_context(|| format!("cannot listen on 127.0.0.1:{port}; is workriver (or something else) already running there? Pick another with --port"))
}
