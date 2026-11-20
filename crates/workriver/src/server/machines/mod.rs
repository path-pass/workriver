// Other machines as the service observes them: their load (metrics/), and the handlers
// that list, add, follow and remove a machine and set the alert thresholds and the stall time.
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::json;

use crate::server::access::*;
use crate::server::app::*;
use crate::server::http::*;

pub(super) mod metrics;

/// Follows a machine: a listener of its own here (the far end of its tunnel), then its
/// connection and the reading of its agents.
pub(super) async fn follow(app: &Shared, label: &str, setting: crate::runtime::Setting) -> Result<(), ApiError> {
    let Some(machines) = app.machines.clone() else { return Err(bad("this service follows no other machines".into())) };
    let local_port = listen(app, label, 0).await?;
    tracing::info!("machine {label}: its tunnel leads to 127.0.0.1:{local_port} here");
    machines.start(label, setting, local_port);
    Ok(())
}

/// A machine's own listener; returns its port here.
pub(super) async fn listen(app: &Shared, label: &str, port: u16) -> Result<u16, ApiError> {
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
