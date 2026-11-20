// The service process: where it is put together and started (the concrete storage and
// runtime meet here, and nowhere else in the service), and the folders of its parts.
// handlers.rs holds what phase 5c moves to the domain folders.
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use notify_debouncer_mini::{DebounceEventResult, new_debouncer};
use tokio::sync::{broadcast, watch};
use workriver_core::runtime::Runtime;
use workriver_data::Store;
use workriver_scan as scan;

use crate::runtime::follow::Machines;
use crate::runtime::words;
use crate::server::machines::metrics::Metrics;
use crate::server::access::auth::Auth;
use crate::server::access::caller::{self, Callers};
use crate::server::access::pins::Pins;
use crate::common::process::Procs;
use crate::ask;
use crate::server::views::Cache;

mod access;
mod app;
pub(super) mod delivery;
pub(super) mod files;
mod handlers;
mod http;
mod machines;
mod views;
#[cfg(test)]
mod tests;

use app::*;
use handlers::*;
use http::*;

/// What else the daemon is started with.
pub(super) struct Options {
    pub(super) warning: Option<String>,
    pub(super) waits: Waits,
    pub(super) machines: Option<Arc<Machines>>,
    pub(super) allow_as: bool,
    /// Read the machines' CPU and memory (the service; tests that do not look at it do not).
    pub(super) sample: bool,
    /// What the service reads of processes: the system's (`process::System`), or a test's.
    pub(super) procs: Arc<dyn Procs>,
    /// The proofs it gives on its caller socket, each for a process's chain.
    pub(super) callers: Arc<Callers>,
}

/// Serves on an already bound listener until Ctrl-C.
/// How long a temporary agent may take to answer, and to wait idle for a follow-up.
#[derive(Clone, Copy)]
pub(super) struct Waits {
    answer: Duration,
    idle: Duration,
    /// How long a new agent's first message is retried every poll while it is not ready;
    /// after that it is shown waiting for the user, like a blocked one.
    ready: Duration,
    /// How long one minute of the stall threshold lasts (a minute; less in tests).
    minute: Duration,
    /// How long one hour of the ready threshold lasts (an hour; less in tests).
    hour: Duration,
}

impl Default for Waits {
    fn default() -> Self {
        Waits { answer: ask::TIMEOUT, idle: ask::IDLE, ready: ask::READY, minute: Duration::from_secs(60), hour: Duration::from_secs(3600) }
    }
}

/// Following other machines too when `machines` is given (`runtime` is then `machines` itself).
pub(super) async fn serve_with(listener: tokio::net::TcpListener, store: Store, runtime: Arc<dyn Runtime>, options: Options) -> anyhow::Result<()> {
    let Options { warning, waits, machines, allow_as, sample, procs, callers } = options;
    let port = listener.local_addr()?.port();
    let store_home = store.path().parent().map(Path::to_path_buf).unwrap_or_default();
    let (changed, _) = broadcast::channel(16);
    let shared = Arc::new(Mutex::new(views::Shared::default()));
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
    app.read_remote(shared, kicked);
    app.recover();
    // Nothing that signs anyone in is ever written here: this output is on a screen (a
    // terminal of the runtime's) that agents can read. The user opens the page with `workriver open`.
    tracing::info!("workriver on http://127.0.0.1:{port}  (open it as you with `workriver open`, from a terminal agents do not read)");
    let stop = app.clone().stop();
    let mut stopping = app.stopping.subscribe();
    let server = axum::serve(listener, router(app, Via { machine: None, port, local: port })).with_graceful_shutdown(stop);
    tokio::select! {
        r = server => r?,
        _ = async { let _ = stopping.wait_for(|s| *s).await; tokio::time::sleep(GRACE).await } => tracing::warn!("requests still open after {} s; stopping anyway", GRACE.as_secs()),
    }
    Ok(())
}

/// Binds 127.0.0.1:`port`, or says plainly why not.
pub(super) async fn bind(port: u16) -> anyhow::Result<tokio::net::TcpListener> {
    tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .with_context(|| format!("cannot listen on 127.0.0.1:{port}; is workriver (or something else) already running there? Pick another with --port"))
}
