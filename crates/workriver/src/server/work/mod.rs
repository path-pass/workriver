// The work through the service: recording an event, a task shown in full or dropped,
// decisions on findings, a gate's result, a chore done, and bringing an agent's terminal to
// the front.
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::json;
use workriver_core::{self as core, Body, Principal};

use crate::server::delivery;
use crate::server::files::{self, store};
use crate::server::access::*;
use crate::server::app::*;
use crate::server::http::*;

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
            let path = store::path(&app.home, &a.id, &a.name)?;
            Some(if remote {
                files::fetch_command(&path.file_name()?.to_string_lossy())
            } else {
                path.display().to_string()
            })
        };
        let mut shown = crate::server::views::task::task(&state, &ws, &unit, n, &file)?;
        // Started or delivered: its agent's harness and model, and where the harness keeps
        // the session (one still to do has none: its agent's session is other work's).
        let begun = state.task(&unit, n).ok().filter(|t| t.started.is_some() || t.done.is_some());
        shown.trace = begun.and_then(|t| Some(app.sessions.trace(&state, t.agent.as_deref()?, t.model.as_deref())));
        shown.link = format!("{}/{unit}/{n}", app.page());
        Ok(Json(shown))
    })
    .await
}

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
    // A start names the place the service opened for it: only the start action records one.
    if matches!(body, Body::WorkspaceManagerStarted { .. }) {
        return Err(bad("a manager is started only by the start action, which opens its terminal and records it: the page's Start manager (a workspace's line)".into()));
    }
    blocking(move || {
        let who = app.who(&caller, a.named.as_deref())?;
        let e = app.record(&who, body)?;
        let mut out = serde_json::to_value(&e).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        // A requirement added to a task: whether its agent was told, apart from the note.
        if let Body::TaskNoted { .. } = &e.body {
            out["told"] = json!(app.noted(&e));
        }
        // A manager that closes its intent is told, in place of a message, to tell the user.
        if let Body::IntentStateChanged { intent, state: core::IntentState::Done, .. } = &e.body {
            // Recorded: a store that cannot be read now only leaves the reminder out.
            let read = got(app.store.lock().unwrap().recorded());
            if let Ok(Some(all)) = read
                && all.manager_of(intent) == Some(e.actor.as_str())
            {
                out["next"] = json!(delivery::said_done());
            }
        }
        Ok(Json(out))
    })
    .await
}

// ---- other machines ----

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
    /// A choice gate answered: the value picked.
    #[serde(default)]
    selected: Vec<String>,
}

pub(super) async fn gate(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(g): Json<Gate>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    let body = Body::GateSubmitted { unit: g.unit, task: g.task, result: g.result, note: g.note, findings: g.findings, attachments: g.attachments, selected: g.selected };
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
pub(super) struct Focus {
    agent: String,
}

pub(super) async fn focus(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(f): Json<Focus>) -> Result<axum::response::Response, ApiError> {
    app.user(&caller)?;
    blocking(move || {
        // The page knows agents by name.
        let session = app.session_of(&f.agent);
        app.runtime.focus(&session).map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
        // On another machine it comes to the front there, in that machine's runtime: the user's own
        // client stays where it is (the runtime has no way to switch it), so the page says where.
        let there = app.live().unwrap_or_default().into_iter().find(|a| a.session == session && a.machine.is_some());
        Ok(match there {
            Some(a) => Json(json!({ "machine": a.machine, "group": a.group.map(|g| g.label), "agent": a.name })).into_response(),
            None => StatusCode::NO_CONTENT.into_response(),
        })
    })
    .await
}
