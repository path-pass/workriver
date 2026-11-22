// Intents, projects and workspaces through the service: starting an intent from the page,
// merging planned ones, and the project folder a caller's place stands for.
use axum::extract::State;
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::json;
use workriver_core::runtime::RuntimeError;
use workriver_core::{self as core, Body, Delivery, DeliveryStep, ReasonCode};

use crate::server::delivery;
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
    /// The workspace it starts in, by its id (`ws1`).
    workspace: String,
}

/// The user starts a planned intent in a workspace of its project from the page, as its
/// manager would with `workriver intent start`; the workspace's manager is told to split it.
pub(super) async fn start_intent(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(s): Json<StartIntent>) -> Result<Json<serde_json::Value>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || {
        let e = app.record(&who, Body::IntentStateChanged { intent: s.intent.clone(), state: core::IntentState::Active, reason: None, workspace: Some(s.workspace.clone()) })?;
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

/// What the user does to a workspace from the page or the CLI: make one in a project, rename
/// it, close it, open it again, seat its manager, or move an active intent into it.
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub(super) enum WorkspaceSet {
    Add {
        project: String,
        name: String,
    },
    Rename {
        workspace: String,
        name: String,
    },
    Close {
        workspace: String,
    },
    Open {
        workspace: String,
    },
    /// `manager`: a session, or the name of the one running agent that has it; none empties
    /// the seat. `replaces`: the seat the user was shown.
    Manager {
        workspace: String,
        #[serde(default)]
        manager: Option<String>,
        #[serde(default)]
        replaces: Option<core::SeatRef>,
    },
    /// An active intent runs in this workspace from now on.
    Move {
        intent: String,
        workspace: String,
    },
}

/// The user's own acts on workspaces: never an agent's, and not on a machine's listener.
pub(super) async fn workspace_set(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(w): Json<WorkspaceSet>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    let body = move |app: &App| -> Result<Body, ApiError> {
        Ok(match w {
        // A project by its id, or by a folder inside it.
        WorkspaceSet::Add { project, name } => Body::WorkspaceCreated { id: String::new(), name, project: app.project_folder(&project, None)? },
        WorkspaceSet::Rename { workspace, name } => Body::WorkspaceRenamed { workspace, name },
        WorkspaceSet::Close { workspace } => Body::WorkspaceClosed { workspace },
        WorkspaceSet::Open { workspace } => Body::WorkspaceOpened { workspace },
        WorkspaceSet::Manager { workspace, manager, replaces } => Body::WorkspaceManagerSet { workspace, manager, machine: None, folder: String::new(), replaces },
        WorkspaceSet::Move { intent, workspace } => Body::IntentWorkspaceSet { intent, workspace },
        })
    };
    blocking(move || app.record(&who, body(&app)?).map(Json)).await
}

impl App {
    /// A project as given (`p1`, or a folder) as its folder: a registered project's. A
    /// repository is part of a registered project or of none: one outside every registered
    /// project is refused, here and on another machine, saying what to do.
    pub(super) fn project_folder(&self, given: &str, machine: Option<&str>) -> Result<String, ApiError> {
        if let Some(p) = self.store.lock().unwrap().project(given)? {
            return Ok(p.path);
        }
        let unregistered = |path: &str| bad(format!("{path} is in no registered project: name a project by its id (`workriver project list`), or ask the user to add it first (`workriver project add <folder>`, from the user's terminal)"));
        // A folder of another machine is none of this machine's projects.
        if machine.is_some() {
            return Err(unregistered(given));
        }
        let path = std::fs::canonicalize(given).map_err(|e| bad(format!("project {given:?}: no such id or folder ({e})")))?;
        let path = path.to_string_lossy().into_owned();
        let all = self.store.lock().unwrap().recorded()?;
        match all.project_of(&path) {
            Some(p) => Ok(p.path.clone()),
            None => Err(unregistered(&path)),
        }
    }
}
