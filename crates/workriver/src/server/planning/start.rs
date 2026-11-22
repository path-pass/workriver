// A manager started from the page, on the user's click: a place opened in the project's
// folder, the start recorded with that place (`workspace.manager_started`, the user's), the
// harness started there, and, off the request, a wait for what it brings up, which is seated
// (`workspace.manager_start_seated`, workriver's) or not, with why
// (`workspace.manager_start_ended`). The rules tie the seating to the start by sequence
// (core's planning/projects.rs). Also the default harness and model.
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use workriver_core::runtime::{AgentState, RuntimeAgent, RuntimeError};
use workriver_core::{self as core, Body, Principal, StartEnd};

use crate::common::config::StartDefault;
use crate::common::config::harness::{self, Harness};
use crate::server::access::*;
use crate::server::app::*;
use crate::server::http::*;

/// How often a pending start's place is looked at.
const STEP: Duration = Duration::from_millis(500);

/// How many of the last lines of a start's place are read for a problem its harness reported.
const SCREEN: u32 = 200;

/// The user's start of a manager for a workspace: the harness (a kind of the table) and the
/// model, over the seat they were shown. Nothing else is taken from the request.
#[derive(Deserialize)]
pub(in crate::server) struct StartManager {
    pub(in crate::server) workspace: String,
    pub(in crate::server) harness: String,
    #[serde(default)]
    pub(in crate::server) model: String,
    #[serde(default)]
    pub(in crate::server) replaces: Option<core::SeatRef>,
}

/// What a harness of the table is on this machine, for the page: offered, or why not.
#[derive(Serialize)]
pub(in crate::server) struct Offer {
    kind: &'static str,
    aliases: &'static [&'static str],
    /// None: offered. Else a key: `not_installed`, `not_integrated`, `runtime` (the runtime
    /// cannot be asked now).
    #[serde(skip_serializing_if = "Option::is_none")]
    why: Option<&'static str>,
}

impl App {
    /// Each harness of the table as this machine's runtime has it.
    pub(in crate::server) fn offers(&self) -> Vec<Offer> {
        let seen = self.runtime.harnesses(None);
        harness::HARNESSES
            .iter()
            .map(|h| {
                let why = match &seen {
                    Err(_) => Some("runtime"),
                    Ok(all) => match all.iter().find(|s| s.kind == h.kind) {
                        Some(s) if s.installed && s.integrated => None,
                        Some(s) if s.installed => Some("not_integrated"),
                        _ => Some("not_installed"),
                    },
                };
                Offer { kind: h.kind, aliases: h.aliases, why }
            })
            .collect()
    }

    /// The harness of the table that `kind` names, when it is offered here, with the arguments
    /// for `model`; refused in words otherwise.
    fn offered(&self, kind: &str, model: &str) -> Result<(&'static Harness, Vec<String>), ApiError> {
        let Some(h) = harness::harness(kind) else {
            let all: Vec<&str> = harness::HARNESSES.iter().map(|h| h.kind).collect();
            return Err(bad(format!("no harness {kind:?}: one of {}", all.join(", "))));
        };
        let args = h.args(model).map_err(bad)?;
        match self.offers().into_iter().find(|o| o.kind == h.kind).and_then(|o| o.why) {
            None => Ok((h, args)),
            Some("runtime") => Err(ApiError(StatusCode::SERVICE_UNAVAILABLE, "the agent runtime cannot be asked now".into())),
            Some("not_integrated") => Err(bad(format!("{kind}: the agent runtime's integration for it is not installed or not current here"))),
            Some(_) => Err(bad(format!("{kind} is not installed here"))),
        }
    }

    /// The user's click: looked at first, then a place opened in the project's folder, the
    /// start recorded naming it (refused: the place is closed again), then the harness
    /// started there and waited for off the request. Answers with the recorded start.
    pub(in crate::server) fn start_manager(self: &Arc<Self>, who: &Principal, s: StartManager) -> Result<core::Event, ApiError> {
        let (h, args) = self.offered(&s.harness, &s.model)?;
        // The rules, asked before anything is opened (only early: the record decides).
        let all = self.store.lock().unwrap().recorded()?;
        let w = all.workspace(&s.workspace)?.clone();
        let shell = core::Shell { pid: 0, start: 0 };
        let probe = Body::WorkspaceManagerStarted { workspace: s.workspace.clone(), harness: h.kind.into(), model: s.model.clone(), folder: String::new(), place: "?".into(), shell, replaces: s.replaces.clone() };
        let Body::WorkspaceManagerStarted { folder, .. } = all.prepare(who, probe)? else { unreachable!() };
        drop(all);
        if !std::path::Path::new(&folder).is_dir() {
            return Err(bad(format!("{folder}, the folder of {}'s project, is not a folder any more", s.workspace)));
        }
        let cannot = |e: RuntimeError| ApiError(StatusCode::SERVICE_UNAVAILABLE, format!("cannot open a terminal for the manager: {e}"));
        let group = w.name.clone();
        let place = self.runtime.open_place(None, &group, &folder).map_err(cannot)?;
        // The place's shell, as for a pin: a place whose shell is not known cannot be told
        // from another one later.
        let read = self.runtime.shell_processes(None, &group, &place).ok().and_then(|(pid, _)| Some((pid, self.procs.chain(None, pid).ok()?.first()?.start)));
        let Some((pid, start)) = read else {
            self.close_empty(&group, &place);
            return Err(ApiError(StatusCode::SERVICE_UNAVAILABLE, format!("the shell of the terminal opened for the manager ({place}) cannot be read; nothing was started")));
        };
        let shell = core::Shell { pid, start };
        let body = Body::WorkspaceManagerStarted { workspace: s.workspace.clone(), harness: h.kind.into(), model: s.model, folder: String::new(), place: place.clone(), shell, replaces: s.replaces };
        let e = match self.record(who, body) {
            Ok(e) => e,
            Err(err) => {
                self.close_empty(&group, &place);
                return Err(err);
            }
        };
        // A name of this start's own: an earlier start's agent may still run under its name.
        let name = core::ManagerStart::name_of(&s.workspace, e.seq as u64);
        let up = Up { workspace: s.workspace, seq: e.seq as u64, group, place, shell, harness: h, name, args };
        let app = self.clone();
        std::thread::spawn(move || app.bring_up(up));
        Ok(e)
    }

    /// Closes a place this start opened, only while it is still a plain shell (the runtime
    /// checks that); one that holds anything by now is left, and that is only logged.
    fn close_empty(&self, group: &str, place: &str) {
        if let Err(e) = self.runtime.close_shell(None, group, place) {
            tracing::info!("the terminal {place} opened for a manager is left open: {e}");
        }
    }

    /// Starts the harness at the start's place and waits for what it brings up; how it ended
    /// is recorded here, at the wait's one exit, unless a seat event ended it first.
    fn bring_up(&self, up: Up) {
        let Some((reason, words)) = self.wait_up(&up) else { return };
        let body = Body::ManagerStartEnded { workspace: up.workspace.clone(), start: up.seq, reason, words };
        let recorded = self.store.lock().unwrap().record(&Principal::Service, body);
        match recorded {
            Ok(_) => {
                let _ = self.changed.send(());
            }
            Err(e) => tracing::info!("the start of a manager for {} is not ended as {reason:?}: {e}", up.workspace),
        }
    }

    /// None: seated, or its start dropped by a seat event; else why it ended.
    fn wait_up(&self, up: &Up) -> Option<(StartEnd, String)> {
        match self.runtime.start_in(None, &up.place, up.harness.kind, &up.name, &up.args) {
            // Waiting for a person (the harness's own first-run question) or still launching:
            // its place is kept and the wait goes on.
            Ok(()) | Err(RuntimeError::NotReady(_)) => {}
            Err(e) => {
                self.close_empty(&up.group, &up.place);
                return Some((StartEnd::Runtime, e.to_string()));
            }
        }
        let _ = self.changed.send(());
        let until = Instant::now() + self.waits.start;
        let mut first = true;
        loop {
            let read = got(self.store.lock().unwrap().workspace(&up.workspace));
            match read {
                Ok(Some(w)) if w.start.as_ref().is_some_and(|s| s.seq == up.seq && s.ended.is_none()) => {}
                // A seat event dropped it (or it ended): nothing is seated by it any more.
                Ok(_) => return None,
                Err(_) => {}
            }
            match self.runtime.roots(None, Some(&up.place)) {
                Ok(roots) => match roots.into_iter().find(|r| r.place == up.place) {
                    None => return Some((StartEnd::NoAgent, format!("its terminal ({}) was closed", up.place))),
                    Some(r) if r.shell != Some(up.shell.pid) => return Some((StartEnd::NoAgent, format!("its terminal ({}) has another shell now", up.place))),
                    Some(r) => match r.at.agent {
                        None if first => {
                            self.close_empty(&up.group, &up.place);
                            return Some((StartEnd::NoAgent, format!("{} did not show up in its terminal", up.harness.kind)));
                        }
                        None => return Some((StartEnd::NoAgent, format!("{} left its terminal before it had a session", up.harness.kind))),
                        // Ready, with a session: seated unless its screen shows a problem it
                        // reported as it started (a model it does not know), which ends it.
                        Some(a) if self.runtime.has_session(&a) && matches!(a.state, AgentState::Idle | AgentState::Done) => match self.runtime.read_place(None, &up.place, SCREEN) {
                            Ok(screen) => match up.harness.startup_error(&screen) {
                                Some(words) => return Some((StartEnd::StartupError, words)),
                                None => return self.seat_start(up, &a),
                            },
                            Err(e) => tracing::debug!("cannot read the terminal of a manager's start now: {e}"),
                        },
                        // Blocked on a question, or still launching: waited for.
                        Some(_) => {}
                    },
                },
                // Not read now (not the same as gone): read again.
                Err(e) => tracing::debug!("cannot look at the terminal of a manager's start now: {e}"),
            }
            first = false;
            if Instant::now() >= until {
                return Some((StartEnd::NoSession, format!("no session to seat within {} s", self.waits.start.as_secs())));
            }
            std::thread::sleep(STEP);
        }
    }

    /// Seats what the start brought up: seen first (so the records know it from now on),
    /// then the seating, which the rules accept only as this start's end. Refused: why.
    fn seat_start(&self, up: &Up, a: &RuntimeAgent) -> Option<(StartEnd, String)> {
        if let Err(e) = self.seen(a) {
            return Some((StartEnd::Refused, e.1));
        }
        let shell = core::Shell { pid: 0, start: 0 };
        let body = Body::ManagerStartSeated { workspace: up.workspace.clone(), start: up.seq, session: a.session.clone(), harness: a.harness.clone(), folder: String::new(), place: up.place.clone(), shell };
        match self.record(&Principal::Service, body) {
            Ok(_) => None,
            Err(e) => Some((StartEnd::Refused, e.1)),
        }
    }

    /// The starts a previous run left pending have lost their wait: each is ended now,
    /// before the service serves, so none can be seated late and none blocks a new start.
    pub(in crate::server) fn end_left_starts(&self) {
        let left = got(self.store.lock().unwrap().pending_starts()).ok().flatten().unwrap_or_default();
        for (workspace, start) in left {
            let body = Body::ManagerStartEnded { workspace: workspace.clone(), start, reason: StartEnd::Restarted, words: String::new() };
            if let Err(e) = self.store.lock().unwrap().record(&Principal::Service, body) {
                tracing::warn!("cannot end the start of a manager for {workspace} left by the previous run: {e}");
            }
        }
    }
}

/// A start the service waits on: where it was opened, and what it starts there.
struct Up {
    workspace: String,
    seq: u64,
    group: String,
    place: String,
    shell: core::Shell,
    harness: &'static Harness,
    name: String,
    args: Vec<String>,
}

/// The default harness and model, with each harness as offered here.
#[derive(Serialize)]
pub(in crate::server) struct StartSettings {
    harness: String,
    model: String,
    harnesses: Vec<Offer>,
}

fn start_settings(app: &App) -> StartSettings {
    let d = crate::runtime::Settings::start_default(&app.home);
    StartSettings { harness: d.harness, model: d.model, harnesses: app.offers() }
}

/// The user's default harness and model, and what is offered here (Settings, the start dialog).
pub(in crate::server) async fn start_get(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Json<StartSettings>, ApiError> {
    app.user(&caller)?;
    blocking(move || Ok(Json(start_settings(&app)))).await
}

/// Sets them; the user's alone, kept in settings.json like the alert thresholds. The harness
/// must be offered here and the model of a form a start takes.
pub(in crate::server) async fn start_set(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(d): Json<StartDefault>) -> Result<Json<StartSettings>, ApiError> {
    app.user(&caller)?;
    blocking(move || {
        app.offered(&d.harness, &d.model)?;
        crate::runtime::Settings::set_start_default(&app.home, d).map_err(bad)?;
        Ok(Json(start_settings(&app)))
    })
    .await
}
