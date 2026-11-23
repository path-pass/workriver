// The App: what the service holds while it runs (the store, the runtime, who is signed in,
// what is pending), with the few things every part asks of it. Its locks are explicit: each
// field that is shared says so by its type.
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::StatusCode;
use notify_debouncer_mini::Debouncer;
use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use tokio::sync::{broadcast, watch};
use workriver_core::ports::{DataError, WorkStore};
use workriver_core::runtime::{AgentState, Runtime, RuntimeAgent, RuntimeError};
use workriver_core::{self as core, Body, Delivery, DeliveryStep, Principal, ReasonCode};
use workriver_scan as scan;

use crate::common::BUILD;
use crate::common::process::Procs;
use crate::runtime::follow::Machines;
use crate::server::Waits;
use crate::server::access::auth::Auth;
use crate::server::access::caller::Callers;
use crate::server::access::pins::Pins;
use crate::server::delivery;
use crate::server::files;
use crate::server::machines::metrics::Metrics;
use crate::server::views::{self, Cache};
use crate::server::commands::*;
use crate::server::http::*;

mod lifecycle;
mod progress;
mod record;

pub(super) use lifecycle::*;
pub(super) use progress::*;

pub(super) struct App {
    /// What keeps the work, as core's data port: the service knows nothing else of it.
    pub(super) store: Mutex<Box<dyn WorkStore>>,
    pub(super) cache: Mutex<Cache>,
    /// Wakes the background read of repositories on other machines.
    pub(super) remote_kick: std::sync::mpsc::Sender<()>,
    pub(super) runtime: Arc<dyn Runtime>,
    /// What was wrong with the runtime at start, if anything.
    pub(super) warning: Option<String>,
    /// The live agents as last polled; None while the runtime cannot be reached.
    pub(super) live: Mutex<Option<Vec<RuntimeAgent>>>,
    /// Delivered messages waiting to be picked up.
    pub(super) pending: Mutex<Vec<Pending>>,
    /// Questions waiting for their temporary agent's answer, with when they give up.
    pub(super) deadlines: Mutex<Vec<(String, std::time::Instant)>>,
    /// Questions whose agent waits for a person in its terminal: given once it is free.
    pub(super) held: Mutex<Vec<Held>>,
    /// Requirements added to tasks whose agent is not told yet (it is on a blind task): the
    /// event, and the agent it is for. Looked at again at each poll (`App::give_notes`).
    // ponytail: in memory, like `held`; after a restart the agent reads it at `task show`.
    pub(super) notes: Mutex<Vec<(core::Event, delivery::Receiver)>>,
    /// Threads whose temporary agent waits for a follow-up: name, and when it is closed.
    pub(super) idle: Mutex<Vec<(String, String, std::time::Instant)>>,
    pub(super) waits: Waits,
    /// When each live agent was first seen blocked, by session.
    // ponytail: in memory; a restart makes a blocked agent's wait start again.
    pub(super) blocked: Mutex<std::collections::HashMap<String, String>>,
    /// Orders a discussion's last check and send against a gate's record: whichever comes
    /// first, the other sees it (`App::hand`, `App::record`).
    pub(in crate::server) discussing: Mutex<()>,
    /// Orders the user's message's last check of its appointment and send against a seat
    /// change's record: whichever comes first, the other sees it (`App::hand`, `App::record`).
    /// Taken before an agent's writer (`Terminals::writer`, held while a message is typed or
    /// a key pressed), never while one is held.
    pub(in crate::server) appointing: Mutex<()>,
    /// The machine each session was last listed on (none: this one), and the sessions gone:
    /// last listed on a machine whose read now is complete without them. What a discussion's
    /// author being gone is judged on.
    // ponytail: in memory; after a restart a session not listed again is unknown, never gone.
    pub(super) seen_on: Mutex<std::collections::HashMap<String, Option<String>>>,
    pub(in crate::server) gone: Mutex<std::collections::HashSet<String>>,
    /// When each live agent was first seen idle or done, by session (and when, to measure it).
    // ponytail: in memory, like `blocked`; a restart starts every idle wait again.
    pub(super) idle_since: Mutex<std::collections::HashMap<String, (String, std::time::Instant)>>,
    /// Running tasks whose agent stopped (reminded, or escalated), by (unit, task),
    /// as of the last poll.
    pub(super) stalls: Mutex<std::collections::HashMap<(String, u32), delivery::reminders::Watch>>,
    /// Minutes an agent may sit idle on a running task (settings.json; the user sets it).
    pub(super) stall_minutes: std::sync::atomic::AtomicU32,
    /// Minutes a worker may sit idle with no task before its manager is told (settings.json,
    /// by hand; read at start).
    pub(super) idle_minutes: u32,
    /// Idle workers whose manager was told, by session and idle since: once per idle period.
    // ponytail: in memory; a restart tells again of a worker still idle.
    pub(super) idle_told: Mutex<HashSet<(String, String)>>,
    /// Hours a unit may stay ready to merge before its manager is asked to check it
    /// (settings.json, by hand; read at start).
    pub(super) ready_hours: u32,
    /// What the scans found that the reminders need: units that look squash-merged.
    pub(super) shared: Arc<Mutex<views::Shared>>,
    /// Which merge reminders went out (`merge_reminders`), kept in a file next to the database.
    pub(super) merge_told: Mutex<std::collections::BTreeSet<String>>,
    pub(super) changed: broadcast::Sender<()>,
    /// Set once the service is stopping: open event streams end, so nothing holds it up.
    pub(super) stopping: watch::Sender<bool>,
    pub(super) port: u16,
    /// The data directory: the database's folder, which holds the attachment store too.
    pub(super) home: PathBuf,
    /// Where harnesses keep their sessions on this machine (a task's trace).
    pub(super) sessions: views::trace::Sessions,
    pub(super) watcher: Mutex<Option<Debouncer<RecommendedWatcher>>>,
    pub(super) watched: Mutex<HashSet<PathBuf>>,
    /// The other machines, when the runtime follows any.
    pub(super) machines: Option<Arc<Machines>>,
    /// CPU and memory of this machine and the followed ones, and their alerts.
    pub(super) metrics: Arc<Metrics>,
    /// Each followed machine's own listener, to end it when the machine is no longer followed.
    pub(super) listeners: Mutex<std::collections::HashMap<String, tokio::task::AbortHandle>>,
    /// The workspaces (by id) whose seated manager is not at the root it was seated at, as
    /// last read (`App::see_roots`): each with the session the page proposes to the user for
    /// the seat, if there is one.
    pub(super) proposals: Mutex<std::collections::HashMap<String, Option<String>>>,
    /// Where each seated session is, as the last read of its seat's machine found it
    /// (`App::see_roots_on`), by session: that machine, and the place, or none where the seat
    /// is lost. No entry: that machine was not read, or its last read failed.
    pub(in crate::server) seats: Mutex<std::collections::HashMap<String, SeatFound>>,
    /// Which machines' connections were up at the last poll: the page hears of a change at once.
    pub(super) tunnels: Mutex<Vec<(String, bool)>>,
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
    pub(super) told: Mutex<()>,
    /// Commands the user is about to run (prepared, shown in the confirm dialog) and those run,
    /// by id, with what their place last showed.
    // ponytail: in memory; a restart forgets them (what ran stays in the record).
    pub(super) runs: Mutex<std::collections::HashMap<String, Run>>,
    /// Agents' terminals: digests, presses, writers, the screens that show a question, views.
    pub(in crate::server) terminals: crate::server::terminals::Terminals,
    /// Wakes the poll before its tick: the runtime told of a change (`Runtime::watch`).
    pub(in crate::server) wake: Arc<tokio::sync::Notify>,
}

pub(super) type Shared = Arc<App>;

/// Where a seated session was found by the last read of its seat's machine: that machine
/// (none: this one), and the place, or none where the seat is lost.
pub(in crate::server) type SeatFound = (Option<String>, Option<String>);

/// A read that failed, already logged: the storage's words, for whoever has to say why
/// something did not happen.
pub(in crate::server) struct Unread(pub(in crate::server) String);

/// A read made where no request can be failed for it any more: after a request's record is
/// committed, in the poll, at start-up. (Before the commit a handler uses `?`: the request
/// fails, 500.) `Ok(None)`: what was asked for is not there. `Err`: the store could not be
/// read; it is logged here, and the caller skips its step or ends it as it ends a failure.
/// It never goes on with a value in place of the one it could not read. Take the answer
/// into a `let` before matching on it: the store's lock is then free for what the arms do
/// (recording a failure locks it again).
pub(in crate::server) fn got<T>(answer: Result<T, DataError>) -> Result<Option<T>, Unread> {
    match answer {
        Ok(t) => Ok(Some(t)),
        Err(DataError::Rejected(_)) => Ok(None),
        Err(e) => {
            tracing::warn!("cannot read the records: {e}");
            Err(Unread(format!("cannot read the records: {e}")))
        }
    }
}

impl App {
    pub(super) fn live(&self) -> Option<Vec<RuntimeAgent>> {
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

    /// Sends a message to an agent, given by its session or by its name: one try, nothing
    /// kept. One on another machine cannot open files of this machine: paths into the
    /// attachment store become links through its tunnel (`type_at`).
    ///
    /// A session is decided before any name (`names_a_session`): an agent that takes a
    /// session id as its name is not it (`send_by_session`). Only a `to` the records know no
    /// session by is a name, turned into its session in a fresh read: exactly one live agent
    /// by it (`named`).
    pub(super) fn send(&self, to: &str, text: &str) -> Result<(), RuntimeError> {
        if self.names_a_session(to) {
            return self.send_by_session(to, text);
        }
        let live = self.runtime.agents().map_err(untyped)?;
        match named(&live, to)[..] {
            [a] => self.type_at(&a.session, a.machine.as_deref(), text),
            [] => Err(RuntimeError::NoAgent(to.into())),
            ref more => Err(RuntimeError::Ambiguous(format!("{} live agents are {to}", more.len()))),
        }
    }

    /// Sends a message to the agent with session `to`, found by that session alone: a seated
    /// one at its seat's place on the seat's machine (`place_of`), any other on the one
    /// machine that lists it now. One that is not listed is not replaced by an agent going by
    /// its name; one listed on two machines gets nothing (which is meant cannot be told).
    pub(in crate::server) fn send_by_session(&self, to: &str, text: &str) -> Result<(), RuntimeError> {
        if let Some(machine) = self.seat_machine(to)? {
            return self.type_at(to, machine.as_deref(), text);
        }
        let live = self.runtime.agents().map_err(untyped)?;
        match live.iter().filter(|a| a.session == to).collect::<Vec<_>>()[..] {
            [a] => self.type_at(to, a.machine.as_deref(), text),
            [] => Err(RuntimeError::NoAgent(to.into())),
            ref more => Err(RuntimeError::Ambiguous(format!("{to} is listed on {} machines", more.len()))),
        }
    }

    /// A message as an agent on `machine` can use it: paths into the attachment store are,
    /// on another machine, the command that fetches the file there.
    pub(in crate::server) fn readable_on<'a>(&self, machine: Option<&str>, text: &'a str) -> std::borrow::Cow<'a, str> {
        match machine {
            Some(_) => files::links(text, &format!("{}/", crate::common::home::attachments_dir(&self.home).display())).into(),
            None => text.into(),
        }
    }

    /// For focus and development `--as`: an exact session or the first matching name in
    /// the last poll. If neither matches, keep the input.
    pub(super) fn session_of(&self, agent: &str) -> String {
        let live = self.live().unwrap_or_default();
        if live.iter().any(|a| a.session == agent) {
            return agent.to_string();
        }
        live.into_iter().find(|a| a.name == agent).map_or_else(|| agent.to_string(), |a| a.session)
    }
}

/// What the runtime answered before anything was typed: a failure there typed nothing.
pub(in crate::server) fn untyped(e: RuntimeError) -> RuntimeError {
    match e {
        RuntimeError::Failed(why) => RuntimeError::Unreachable(why),
        e => e,
    }
}

/// The live agents `agent` names: those with this session, else those of this name. Each
/// caller says what none and several mean to it.
pub(in crate::server) fn named<'a>(live: &'a [RuntimeAgent], agent: &str) -> Vec<&'a RuntimeAgent> {
    let exact: Vec<&RuntimeAgent> = live.iter().filter(|a| a.session == agent).collect();
    if exact.is_empty() { live.iter().filter(|a| a.name == agent).collect() } else { exact }
}

/// Refuses a name several running agents have, listing them by session.
pub(in crate::server) fn several(agent: &str, named: &[&RuntimeAgent]) -> ApiError {
    let list = named.iter().map(|a| format!("{} ({}{})", a.session, a.name, a.machine.as_ref().map(|m| format!(", on {m}")).unwrap_or_default())).collect::<Vec<_>>().join(", ");
    bad(format!("several running agents are {agent}: name one by its session: {list}"))
}
