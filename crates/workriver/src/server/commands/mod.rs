// The user's commands from the page: prepared and shown, run in a shell of the user's,
// read while they run, brought to the front. Only the user runs one: each handler asks.
use std::path::Path;
use std::time::Duration;

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::json;
use workriver_core::runtime::{AgentState, RuntimeAgent};
use workriver_core::{self as core, Body};
use workriver_scan as scan;

use crate::server::access::auth;
use crate::server::views;
use crate::server::access::*;
use crate::server::app::*;
use crate::server::http::*;
use crate::server::conversations::*;

pub(super) mod review;
pub(super) mod shell;

pub(super) use review::run_review;

/// A run's shell to close: the run, its machine (none: this one), its place.
type Shell = (String, Option<String>, String);

/// A command the user prepared to run, then ran (`place` set).
#[derive(Clone)]
pub(super) struct Run {
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
    pub(super) review: Review,
}

/// What a workriver agent says a command will do, before the user runs it: it only warns.
#[derive(Clone, Default)]
pub(super) struct Review {
    /// The agent started for it: only it may record the review; closed once it did.
    agent: Option<RuntimeAgent>,
    /// When it is given up.
    pub(super) until: Option<std::time::Instant>,
    verdict: Option<core::Verdict>,
    text: String,
    /// Why there is no review.
    failed: Option<String>,
}

impl Review {
    pub(super) fn pending(&self) -> bool {
        self.verdict.is_none() && self.failed.is_none()
    }
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

/// The user is about to run a command: where it would run and who wrote it, for the confirm
/// dialog. Nothing runs here; `run_command` runs it, and only with this exact text.
pub(super) async fn run_prepare(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(p): Json<RunPrepare>) -> Result<Json<serde_json::Value>, ApiError> {
    app.user(&caller)?;
    blocking(move || {
        shell::check(&p.command).map_err(bad)?;
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
        let line = shell::line(&r.dir, &r.command, &c.id);
        let place = match app.runtime.run_shell(r.machine.as_deref(), shell::GROUP, &r.dir, &line) {
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
            Some(m) => Json(json!({ "machine": m, "workspace": shell::GROUP })).into_response(),
            None => StatusCode::NO_CONTENT.into_response(),
        })
    })
    .await
}

impl App {
    /// The user's recent runs, newest first: the record's, with what this service still
    /// follows of them (whether a place is read, where it is).
    pub(super) fn recent_runs(&self, state: &core::State) -> Vec<views::RunOut> {
        let live = self.runs.lock().unwrap();
        state
            .runs
            .iter()
            .rev()
            .map(|r| {
                let here = live.get(&r.id);
                let exit = r.exit.or(here.and_then(|x| x.exit));
                let now = if exit.is_some() { "done" } else if here.is_some_and(|x| x.watching) { "running" } else { "unknown" };
                views::RunOut {
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
