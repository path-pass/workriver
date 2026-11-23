// What the page and the CLI read: the workspace, a unit's code, the state, the status, the
// event stream, the machines' load and its history, a setting. Each asks who reads first.
use std::convert::Infallible;

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::json;
use tokio_stream::{Stream, StreamExt, wrappers::{BroadcastStream, WatchStream}};
use workriver_core as core;
use workriver_scan as scan;

use crate::common::BUILD;
use crate::server::app::Shared;
use crate::server::files::store;
use crate::server::views;
use crate::server::access::*;
use crate::server::app::*;
use crate::server::http::*;

pub(in crate::server) async fn workspace(State(app): State<Shared>, Extension(caller): Extension<Caller>, Query(r): Query<Reader>) -> Result<Json<views::Workspace>, ApiError> {
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
pub(in crate::server) async fn unit_code(State(app): State<Shared>, Extension(caller): Extension<Caller>, UrlPath(id): UrlPath<String>, Query(r): Query<Reader>) -> Result<Json<views::Code>, ApiError> {
    blocking(move || {
        let state = app.view(&caller, &r)?;
        let started = std::time::Instant::now();
        let live = app.live();
        let code = {
            let mut cache = app.cache.lock().unwrap();
            (cache.light, cache.bare) = (false, false);
            let have: Vec<&str> = r.have.split(',').filter(|h| !h.is_empty()).collect();
            let code = views::code(&state, &mut cache, live.as_deref(), &id, &have);
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

/// The records as the caller may see them (`App::view`), each intent with where its text
/// stands (`edits`: its version, who made it and when, the version that last changed each
/// field), which the state itself never serializes; its history is read on its own route.
/// The templates are the library a unit can name: the user's, and each built-in the user
/// has none of, marked built in (the records hold only the user's), so a CLI of another
/// build shows the steps this service places.
pub(in crate::server) async fn state(State(app): State<Shared>, Extension(caller): Extension<Caller>, Query(r): Query<Reader>) -> Result<Json<serde_json::Value>, ApiError> {
    blocking(move || {
        let state = app.view(&caller, &r)?;
        let mut out = serde_json::to_value(&state).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if let Some(intents) = out["intents"].as_array_mut() {
            for (shown, i) in intents.iter_mut().zip(&state.intents) {
                shown["edits"] = json!(i.edits);
            }
        }
        out["templates"] = json!(state.library());
        Ok(Json(out))
    })
    .await
}

/// Whether the service runs, and what it holds, for anyone: on a followed machine's listener
/// too, without asking who. So the jobs' last results (counts, times, the jobs' own words) are
/// left out there: no CLI over there reads them.
pub(in crate::server) async fn status(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Json<serde_json::Value>, ApiError> {
    let (s, storage) = {
        let store = app.store.lock().unwrap();
        (store.recorded()?, store.status()?)
    };
    let (files, bytes) = store::usage(&app.home);
    let mut out = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "commit": env!("WORKRIVER_COMMIT"),
        "build": BUILD,
        "port": app.port,
        // The page as the user opens it, on this machine: `workriver link` builds on it, also
        // through a machine's tunnel.
        "url": app.page(),
        "db": storage.location,
        "events": storage.events,
        "intents": s.intents.len(),
        "units": s.units.len(),
        "runtime": { "connected": app.live().is_some(), "warning": app.warning },
        "machines": app.machines.as_ref().map(|m| m.states()).unwrap_or_default(),
        "attachments": { "dir": crate::common::home::attachments_dir(&app.home), "files": files, "bytes": bytes },
        "jobs": app.schedule.last(),
    });
    if caller.machine.is_some()
        && let Some(o) = out.as_object_mut()
    {
        o.remove("jobs");
    }
    Ok(Json(out))
}

/// "Something changed", for the page: the user's.
pub(in crate::server) async fn events(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>, ApiError> {
    app.user(&caller)?;
    // A lagging client missed some, which is still one change.
    // It ends when the service stops, or it would hold the stopping up for good.
    let stop = WatchStream::new(app.stopping.subscribe()).filter(|s| *s).map(|_| false);
    let stream = BroadcastStream::new(app.changed.subscribe()).map(|_| true).merge(stop).take_while(|go| *go).map(|_| Ok(SseEvent::default().event("changed").data("changed")));
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ---- the user's session in the page ----

/// CPU and memory of this machine and the followed ones, with the thresholds: read by the
/// user (the page) and by an agent at its place here (to weigh where to put work); set by
/// the user alone. Not on a machine's listener.
pub(in crate::server) async fn metrics(State(app): State<Shared>, Extension(caller): Extension<Caller>, Query(r): Query<Reader>) -> Result<Json<crate::common::wire::Report>, ApiError> {
    app.who(&caller, r.named.as_deref())?;
    let mut report = app.metrics.report();
    // An agent on another machine sees that machine alone: nothing of this one or the others.
    if let Some(m) = caller.machine.as_deref() {
        report.machines.retain(|x| !x.here && x.label == m);
    }
    Ok(Json(report))
}

#[derive(Deserialize)]
pub(in crate::server) struct HistoryQuery {
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
pub(in crate::server) async fn metrics_history(State(app): State<Shared>, Extension(caller): Extension<Caller>, Query(q): Query<HistoryQuery>) -> Result<Json<serde_json::Value>, ApiError> {
    app.who(&caller, q.named.as_deref())?;
    if let Some(m) = caller.machine.as_deref().filter(|m| *m != q.machine) {
        return Err(ApiError(StatusCode::FORBIDDEN, format!("a caller on machine {m} reads that machine's history only")));
    }
    let m = app.metrics.clone();
    let (label, hours) = (q.machine.clone(), q.hours);
    let points = tokio::task::spawn_blocking(move || m.history(&label, hours)).await.map_err(|e| bad(e.to_string()))?.map_err(bad)?;
    Ok(Json(json!({ "machine": q.machine, "hours": q.hours, "points": points })))
}

/// After how many idle minutes a running task's agent counts as stopped; the page's Settings.
pub(in crate::server) async fn stall_get(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Json<serde_json::Value>, ApiError> {
    app.user(&caller)?;
    Ok(Json(json!({ "minutes": app.stall_minutes.load(std::sync::atomic::Ordering::Relaxed) })))
}

impl App {
    /// What the page reads, of `state` as the reader may see it: files without their texts
    /// (`unit_code` has them). `light`: not even uncommitted work or trees (board, `task show`).
    pub(in crate::server) fn snapshot(&self, state: &core::State, light: bool, trigger: &str) -> views::Workspace {
        let started = std::time::Instant::now();
        let calls = scan::CALLS.load(std::sync::atomic::Ordering::Relaxed);
        let blocked = self.blocked.lock().unwrap().clone();
        let idle = self.idle_since.lock().unwrap().iter().map(|(s, (at, _))| (s.clone(), at.clone())).collect();
        let stalls = self.stalls.lock().unwrap().iter().filter_map(|(k, w)| Some((k.clone(), w.stall.clone()?))).collect();
        let proposals = self.proposals.lock().unwrap().clone();
        let live = self.live();
        let mut cache = self.cache.lock().unwrap();
        (cache.light, cache.bare) = (light, true);
        cache.ready_after = Some(self.waits.hour * self.ready_hours);
        let mut ws = views::build(state, &mut cache, live.as_deref(), &blocked, &idle, &stalls, &proposals);
        views::mark_gone(&mut ws, &self.gone.lock().unwrap());
        let on_screen = self.on_screen();
        for s in ws.sessions.iter_mut().filter(|s| s.blocked_since.is_some()) {
            s.on_screen = on_screen.contains(&crate::server::terminals::agent_key(&s.session, s.machine.as_deref()));
        }
        // A seated manager's row waits only where its seat is held to, as last read: a row of
        // a lost seat, of one not read, or on another machine than the seat's offers no dialog
        // (what the user acts on is the workspace's "confirm its terminal").
        for s in ws.sessions.iter_mut() {
            let Some(m) = state.seat_of(&s.session).filter(|w| !w.closed).and_then(|w| w.manager.as_ref()) else { continue };
            if self.place_as_read(&s.session, m.machine.as_deref(), s.machine.as_deref()).is_err() {
                (s.blocked_since, s.on_screen) = (None, false);
            }
        }
        cache.save();
        // Each task an agent started or delivered: that agent's harness and model (where the
        // harness keeps the session is `task show`'s). One still to do has none: its agent's
        // session is still other work's.
        for u in ws.intents.iter_mut().flat_map(|i| i.units.iter_mut()) {
            for t in &mut u.tasks {
                let Some(begun) = t.id.parse().ok().and_then(|n| state.task(&u.id, n).ok()).filter(|x| x.started.is_some() || x.done.is_some()) else { continue };
                t.trace = t.agent_session.as_deref().map(|s| views::trace::ran(state, s, begun.model.as_deref()));
            }
        }
        if state.reduced {
            views::hide(&mut ws, state);
        }
        let ms = started.elapsed().as_millis() as u64;
        tracing::info!(target: "perf", kind = "snapshot", trigger, light, ms, recomputed = cache.recomputed, reused = cache.reused, git = scan::CALLS.load(std::sync::atomic::Ordering::Relaxed) - calls, slow = ms >= crate::common::config::defaults::SLOW_MS);
        ws
    }
}

#[derive(Deserialize)]
pub(in crate::server) struct HistoryAt {
    /// One version whole, with the one before it (for a diff); none: the list of versions.
    #[serde(default)]
    version: Option<u32>,
}

/// An intent's history, read by whoever may read the records (`App::view`) and is not on a
/// blind task: its versions, each with what its event set and who made it (`by`, and the
/// name an agent goes by), or with `?version=N` that version's whole text and the one
/// before it. Built from the intent's events (`intent_events`), not kept in the state.
pub(in crate::server) async fn intent_history(State(app): State<Shared>, Extension(caller): Extension<Caller>, UrlPath(id): UrlPath<String>, Query(r): Query<Reader>, Query(at): Query<HistoryAt>) -> Result<Json<serde_json::Value>, ApiError> {
    blocking(move || {
        let state = app.view(&caller, &r)?;
        if state.reduced {
            return Err(ApiError(StatusCode::FORBIDDEN, "you are on a blind task: an intent's history, which holds its note, is hidden from you until you deliver it".into()));
        }
        state.intent(&id)?;
        let events = app.store.lock().unwrap().intent_events(&id)?;
        if let Some(n) = at.version {
            let text = core::intent_text(&events, &id, n).ok_or_else(|| bad(format!("{id} has no version {n}")))?;
            let before = n.checked_sub(1).and_then(|b| core::intent_text(&events, &id, b));
            return Ok(Json(json!({ "intent": id, "n": n, "text": text, "before": before })));
        }
        let name = |by: &str| state.agents.iter().find(|a| a.session == by).map(|a| a.name.clone());
        let versions: Vec<serde_json::Value> = core::intent_versions(&events, &id)
            .into_iter()
            .map(|v| {
                let mut out = serde_json::to_value(&v).unwrap_or_default();
                out["name"] = json!(name(&v.by));
                out
            })
            .collect();
        Ok(Json(json!({ "intent": id, "versions": versions })))
    })
    .await
}
