// Attachments: the handlers that take a file, hand one out and make a task's inbox; the
// bytes on disk (store.rs); and what an agent on another machine is told about a stored
// file: it reaches it through its machine's connection.
use std::path::Path;

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::json;
use workriver_core::Body;

use crate::common::home::PORT_FILE;
use crate::server::files;
use crate::server::access::*;
use crate::server::app::*;
use crate::server::http::*;

pub(super) mod lifecycle;
pub(super) mod store;
pub(super) mod target;

/// Where a file in the daemon's store is for an agent on a machine: through its tunnel, on
/// the port of the connection there is when it is fetched (a shell fills it in).
fn attachment_url(id: &str) -> String {
    format!("http://127.0.0.1:$(cat ~/{PORT_FILE})/api/attachments/{id}")
}

/// How an agent on another machine gets a file of the daemon's store: a command, since the
/// service answers no reader that is nobody and only the CLI shows which process asks.
pub(super) fn fetch_command(name: &str) -> String {
    format!("`workriver attach fetch {name}`")
}

/// A message for an agent on another machine: every path into the attachment store
/// (`dir` + `<id>.<ext>`) becomes the command that fetches it through its tunnel.
pub(super) fn links(text: &str, dir: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(k) = rest.find(dir) {
        out += &rest[..k];
        let after = &rest[k + dir.len()..];
        // The file's name: its id, a dot, its extension.
        let id = after.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(after.len());
        let ext = after[id..].strip_prefix('.').map(|e| 1 + e.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(e.len())).unwrap_or(0);
        out += &format!("{} (run it: the file is saved in the folder you are in)", fetch_command(&after[..id + ext]));
        rest = &after[id + ext..];
    }
    out + rest
}

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
pub(super) struct Upload {
    name: String,
    #[serde(default, rename = "as")]
    named: Option<String>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    task: Option<u32>,
    #[serde(default)]
    finding: Option<String>,
}

/// A file into the store, its body raw (`?name=…`, the type from Content-Type), optionally
/// linked to a task or a finding. Answers its id and path.
pub(super) async fn attach(State(app): State<Shared>, Extension(via): Extension<Caller>, Query(u): Query<Upload>, headers: axum::http::HeaderMap, body: axum::body::Bytes) -> Result<Json<serde_json::Value>, ApiError> {
    let given = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(str::to_string);
    blocking(move || {
        let who = app.who(&via, u.named.as_deref())?;
        // Only the last part of the name: it is shown, never used as a path.
        let name = Path::new(&u.name).file_name().map(|n| n.to_string_lossy().into_owned()).filter(|n| !n.is_empty()).ok_or_else(|| bad("an attachment needs a file name".into()))?;
        let mime = crate::common::files::mime(&name, given.as_deref());
        let (id, path) = store::put(&app.home, &body, &name).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("cannot store {name}: {e}")))?;
        let size = body.len() as u64;
        app.record(&who, Body::AttachmentAdded { id: id.clone(), name: name.clone(), mime: mime.clone(), size, unit: u.unit, task: u.task, finding: u.finding })?;
        // To a caller on another machine the file is where its tunnel shows it.
        let path = match &via.machine {
            Some(_) => json!(files::attachment_url(&path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())),
            None => json!(path),
        };
        Ok(Json(json!({ "id": id, "path": path, "name": name, "type": mime, "size": size })))
    })
    .await
}

/// A stored file. Only images show in the page; anything else downloads, and nothing is sniffed.
pub(super) async fn attachment(State(app): State<Shared>, Extension(caller): Extension<Caller>, UrlPath(id): UrlPath<String>, Query(r): Query<Reader>) -> Result<Response, ApiError> {
    // An agent on another machine is given the link with the file's extension.
    let id = id.split('.').next().unwrap_or_default().to_string();
    // Of what the caller may see: an agent on a blind task, only what it attached itself.
    let viewer = app.clone();
    let state = blocking(move || viewer.view(&caller, &r)).await?;
    let found = state.attachments.iter().find(|a| a.id == id).cloned();
    let a = found.ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("no attachment {id}")))?;
    let file = store::path(&app.home, &a.id, &a.name).ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("no attachment {id}")))?;
    let bytes = blocking(move || std::fs::read(&file).map_err(|e| ApiError(StatusCode::NOT_FOUND, format!("attachment {id}: {e}")))).await?;
    let image = a.mime.starts_with("image/") && a.mime != "image/svg+xml";
    let disposition = if image { "inline".to_string() } else { format!("attachment; filename=\"{}\"", a.name.replace(['"', '\\', '\r', '\n'], "_")) };
    let mime = if image { a.mime.clone() } else { "application/octet-stream".to_string() };
    Ok(([(header::CONTENT_TYPE, mime), (header::CONTENT_DISPOSITION, disposition), (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string())], bytes).into_response())
}

#[derive(Deserialize)]
pub(super) struct Inbox {
    unit: String,
    task: u32,
}

/// A task's staging folder for screenshots and the like, made if needed.
pub(super) async fn inbox(State(app): State<Shared>, Extension(via): Extension<Caller>, Json(i): Json<Inbox>) -> Result<Json<serde_json::Value>, ApiError> {
    blocking(move || {
        // Only someone (the user, or an agent from its own place) gets a folder made and named.
        app.who(&via, None)?;
        app.store.lock().unwrap().task(&i.unit, i.task)?;
        // A caller on another machine keeps the folder on its own disk; the CLI makes it there.
        if via.machine.is_some() {
            return Ok(Json(json!({ "elsewhere": true })));
        }
        let d = crate::common::home::inbox(&app.home, &i.unit, i.task);
        crate::common::home::mkdir(&d).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("cannot make {}: {e}", d.display())))?;
        Ok(Json(json!({ "dir": d })))
    })
    .await
}

// ---- the attachment rule: the user's ----

/// How many files and bytes the rule would take now from the records as they are.
fn due_now(app: &App, rule: &crate::common::config::attachments::Rule) -> Result<serde_json::Value, ApiError> {
    if rule.after == crate::common::config::attachments::After::Keep {
        return Ok(json!({ "files": 0, "bytes": 0 }));
    }
    let state = app.store.lock().unwrap().recorded()?;
    let due = state.attachments_due(&rule.conditions(), std::time::SystemTime::now());
    Ok(json!({ "files": due.len(), "bytes": due.iter().map(|a| a.size).sum::<u64>() }))
}

/// The Attachments tab: the rule (or why the one in the settings runs nothing), what the
/// store holds, and the run: its last result, whether one is on, when the next is due.
pub(super) async fn rule_get(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Json<serde_json::Value>, ApiError> {
    blocking(move || {
        app.user(&caller)?;
        let (rule, error) = match crate::runtime::Settings::attachment_rule(&app.home) {
            Ok(r) => (json!(r), None),
            Err(why) => (serde_json::Value::Null, Some(why)),
        };
        let (files, bytes) = store::usage(&app.home);
        let job = app.schedule.last().into_iter().find(|j| j.name == lifecycle::JOB.name);
        let job = serde_json::to_value(job).unwrap_or_default();
        Ok(Json(json!({ "rule": rule, "error": error, "usage": { "files": files, "bytes": bytes }, "lastRun": job["last"], "running": job["running"].as_bool().unwrap_or(false), "next": job["next"] })))
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RuleSet {
    rule: serde_json::Value,
    #[serde(default)]
    dry_run: bool,
}

/// Saves the rule, checked as it is read and with its folder checked (and saved canonical);
/// with `dryRun`, nothing is saved. Either way, what it would take now.
pub(super) async fn rule_set(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(r): Json<RuleSet>) -> Result<Json<serde_json::Value>, ApiError> {
    blocking(move || {
        app.user(&caller)?;
        let mut rule = crate::common::config::attachments::Rule::of(&r.rule).map_err(bad)?;
        if let Some(t) = rule.target.take() {
            rule.target = Some(target::check(&t, &app.home).map_err(bad)?);
        }
        if !r.dry_run {
            crate::runtime::Settings::set_attachment_rule(&app.home, &rule).map_err(bad)?;
        }
        Ok(Json(json!({ "rule": rule, "due": due_now(&app, &rule)? })))
    })
    .await
}

/// Runs the rule now: the run's result, or 409 while one is on.
pub(super) async fn rule_run(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Json<crate::server::schedule::Ran>, ApiError> {
    blocking(move || {
        app.user(&caller)?;
        match app.run_job(lifecycle::JOB.name, crate::server::schedule::By::User) {
            Ok(ran) => Ok(Json(ran)),
            Err(crate::server::schedule::NotRun::Busy) => Err(ApiError(StatusCode::CONFLICT, "a run is in progress; its result shows when it ends".into())),
            Err(crate::server::schedule::NotRun::NoJob) => Err(ApiError(StatusCode::NOT_FOUND, "this service does not run the attachment rule".into())),
        }
    })
    .await
}
