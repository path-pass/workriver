// The service's HTTP side: the two route tables (this machine's listener, and the allow-list
// of a followed machine's), the guard every request passes, the session routes, errors and
// the page's files. Transport only: who may do what is each handler's own check.
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{Method, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Deserialize;
use serde_json::json;
use workriver_core as core;

use crate::server::access::auth;
use crate::server::access::*;
use crate::server::app::*;
use crate::server::handlers::*;

mod assets;
mod error;
mod guard;
mod session;

use assets::*;
pub(super) use error::*;
pub(super) use guard::*;
use session::*;

/// Every route, on this machine's listener. Nothing answers a caller that is nobody: what
/// acts as the user or changes where workriver reaches asks for the user in its handler
/// (`App::user`); recording and reading ask who the caller is (`App::who`, `App::view`: the
/// user, or the agent at the place the calling process runs at, which on a blind task sees
/// only its own). Only `status` and the session routes answer anyone local; `here` answers
/// any local process.
pub(super) fn router(app: Shared, via: Via) -> Router {
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
        .route("/api/run/{id}/review", post(run_review))
        .route("/api/here", post(here))
        .route("/api/ask", post(ask_why))
        .route("/api/thread/end", post(end_thread))
        .route("/api/message", post(message))
        .route("/api/messages/read", post(messages_read))
        .route("/api/attachments", post(attach).layer(DefaultBodyLimit::max(crate::common::wire::MAX_ATTACHMENT)))
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
    let who = req.extensions().get::<Caller>().map_or("none", |c| match (&c.machine, &c.chain, c.session) {
        (Some(_), ..) => "machine",
        (None, _, true) => "page",
        (None, Some(_), _) => "cli",
        _ => "none",
    });
    let res = next.run(req).await;
    let ms = started.elapsed().as_millis() as u64;
    let bytes = axum::body::HttpBody::size_hint(res.body()).exact().unwrap_or(0);
    tracing::info!(target: "perf", kind = "http", method, route, status = res.status().as_u16(), ms, bytes, who, slow = ms >= crate::common::config::defaults::SLOW_MS);
    res
}

/// The routes of a followed machine's listener: only what an agent's CLI over there needs.
/// An allow-list: who it is, recording as that agent, asking, attachments, reading the
/// board, and a command's review from the agent started for it there. Nothing here is ever done as the user (no session, no token), and what the user
/// alone does (decisions, gates, machines, links, projects, focus) is not routed at all.
pub(super) fn machine_router(app: Shared, via: Via) -> Router {
    Router::new()
        .route("/api/here", post(here))
        .route("/api/append", post(append))
        .route("/api/ask", post(ask_why))
        .route("/api/attachments", post(attach).layer(DefaultBodyLimit::max(crate::common::wire::MAX_ATTACHMENT)))
        .route("/api/attachments/inbox", post(inbox))
        .route("/api/attachments/{id}", get(attachment))
        .route("/api/workspace", get(workspace))
        .route("/api/state", get(state))
        .route("/api/task", get(task_show))
        .route("/api/status", get(status))
        // Its own load only (the handlers keep a machine's caller to that machine).
        .route("/api/metrics", get(metrics))
        .route("/api/metrics/history", get(metrics_history))
        .route("/api/run/{id}/review", post(run_review))
        .route_layer(middleware::from_fn(timing))
        .fallback(|| async { (StatusCode::FORBIDDEN, "not available to another machine: its agents record their own work, ask, attach and read the board; everything else is done on the machine workriver runs on") })
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .layer(Extension(via))
        .with_state(app)
}
