// Intents, projects and workspaces through the service: starting an intent from the page,
// merging planned ones, and the project folder a caller's place stands for.
use std::path::Path;

use axum::extract::State;
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::json;
use workriver_core::runtime::RuntimeError;
use workriver_core::{self as core, Body, Delivery, DeliveryStep, ReasonCode};
use workriver_scan as scan;

use crate::server::delivery;
use crate::server::views;
use crate::server::access::*;
use crate::server::app::*;
use crate::server::http::*;

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
            let all = app.store.lock().unwrap().recorded()?;
            if let Some(why) = views::other_project(&all, &live, &group, &s.intent) {
                return Err(ApiError(StatusCode::CONFLICT, format!("{why}: its manager would get work from a project it is not in. Pick a workspace of that project, or confirm starting it there anyway (confirmOtherProject)")));
            }
        }
        let e = app.record(&who, Body::IntentStateChanged { intent: s.intent.clone(), state: core::IntentState::Active, reason: None, workspace: Some(group) })?;
        let live = app.runtime.agents().ok();
        // The intent is started from here on: a store that cannot be read now does not make
        // this a failed request. Its manager is then not told, and the answer says so.
        let read = got(app.store.lock().unwrap().recorded());
        let (plan, names) = match read {
            Ok(Some(all)) => (delivery::started(&all, live.as_deref(), &s.intent), Some(all)),
            Ok(None) => (Err((ReasonCode::RuntimeError, "cannot read the records".to_string())), None),
            Err(Unread(why)) => (Err((ReasonCode::RuntimeError, why)), None),
        };
        // An agent with no name on record goes by its session, as everywhere.
        let name = |to: &str| names.as_ref().and_then(|all| all.name_of(to)).unwrap_or(to).to_string();
        // Like every notice to a manager: not while it is on a blind task (then held, and the
        // answer says so in the service's words; it is given once that task is delivered).
        let delivery = match plan {
            Ok((to, message)) => match app.tell(&to, &message, None) {
                Ok(Ok(())) => Delivery { step: DeliveryStep::Delivered, to: Some(name(&to)), reason_code: None, reason: None, message },
                Ok(Err(err)) => Delivery { step: DeliveryStep::NotDelivered, to: Some(name(&to)), reason_code: Some(if matches!(err, RuntimeError::None) { ReasonCode::RuntimeDown } else { ReasonCode::RuntimeError }), reason: Some(err.to_string()), message },
                Err(why) => Delivery { step: DeliveryStep::Held, to: Some(name(&to)), reason_code: None, reason: Some(why), message },
            },
            Err((code, reason)) => Delivery { step: DeliveryStep::NotDelivered, to: None, reason_code: Some(code), reason: Some(reason), message: String::new() },
        };
        // As the page names a delivery's fields.
        let delivery = json!({ "step": delivery.step, "to": delivery.to, "reasonCode": delivery.reason_code, "reason": delivery.reason, "message": delivery.message });
        Ok(Json(json!({ "event": e, "delivery": delivery })))
    })
    .await
}

impl App {
    /// A project as given (`p1`, or a folder) as its folder: a registered project's, or the
    /// repository's top folder for a repository outside every registered project.
    pub(super) fn project_folder(&self, given: &str, machine: Option<&str>) -> Result<String, ApiError> {
        if let Some(p) = self.store.lock().unwrap().project(given)? {
            return Ok(p.path);
        }
        // On another machine: the repository's top folder there.
        if machine.is_some() {
            let at = self.runner(machine)?;
            return scan::toplevel(&at, Path::new(given)).map(|t| t.to_string_lossy().into_owned()).map_err(|_| bad(format!("{given} is not a git repository on that machine; name a project of this machine by its id (p1)")));
        }
        let path = std::fs::canonicalize(given).map_err(|e| bad(format!("project {given:?}: no such id or folder ({e})")))?;
        let path = path.to_string_lossy().into_owned();
        let all = self.store.lock().unwrap().recorded()?;
        if let Some(p) = views::project_of(&all, &path) {
            return Ok(p.path.clone());
        }
        scan::toplevel(&scan::Runner::Local, Path::new(&path)).map(|t| t.to_string_lossy().into_owned()).map_err(|_| bad(format!("{path} is neither a registered project nor a git repository; register it with `workriver project add`")))
    }
}
