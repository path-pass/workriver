// Questions and talk through the service: asking about a task, the user's discussion of a
// delivered result with its agent, the user's message to a manager and what was read; and
// closing a temporary agent a moment after its work is done.
use std::path::Path;
use std::time::Duration;

use axum::extract::State;
use axum::{Extension, Json};
use serde::Deserialize;
use workriver_core::runtime::{AgentState, RuntimeAgent, RuntimeError};
use workriver_core::{self as core, Body, Delivery, DeliveryStep, Principal, ReasonCode};
use workriver_scan as scan;

use crate::runtime::words;
use crate::server::delivery;
use crate::server::views;
use crate::server::access::*;
use crate::server::app::*;
use crate::server::http::*;

mod discussions;

pub(super) use discussions::gone_from;
mod messages;
pub(super) mod prompts;
mod questions;


/// How long after the decision a temporary agent is closed, so its own command can return.
pub(super) const CLOSE_DELAY: Duration = Duration::from_secs(3);

/// The CLI asks who and where it is: see `App::here`.
#[derive(Deserialize)]
pub(super) struct Ask {
    #[serde(default, rename = "as")]
    named: Option<String>,
    unit: String,
    task: u32,
    text: String,
    /// The thread it continues; none starts a new one.
    #[serde(default)]
    thread: Option<String>,
    #[serde(default)]
    context: Vec<core::AskContext>,
    /// Ask the thread's agent for the conclusion instead.
    #[serde(default)]
    conclude: bool,
}

#[derive(Deserialize)]
pub(super) struct Say {
    /// The intent whose workspace's manager it goes to; or, without one, the workspace's label.
    #[serde(default)]
    intent: String,
    #[serde(default)]
    workspace: String,
    /// An answer may come with no words of the user's.
    #[serde(default)]
    text: String,
    /// What the user picked: a unit, one of its tasks, and the rest (files stored first).
    #[serde(default)]
    unit: String,
    #[serde(default)]
    task: u32,
    #[serde(default)]
    context: Vec<core::AskContext>,
    /// The user's answer to a manager's options: which message (or reply), and the value.
    #[serde(default)]
    answer: Option<core::Answer>,
}

/// The user writes to a manager from the page, with what they picked and attached; it is
/// typed into the manager's terminal.
pub(super) async fn message(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(m): Json<Say>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    let body = Body::MessageSent { id: String::new(), intent: m.intent, workspace: m.workspace, manager: String::new(), text: m.text, unit: m.unit, task: m.task, context: m.context, answer: m.answer };
    blocking(move || app.record(&who, body).map(Json)).await
}

#[derive(Deserialize)]
pub(super) struct Read {
    workspace: String,
    at: String,
}

/// The page shows the user a workspace's conversation with its manager: read up to `at`.
pub(super) async fn messages_read(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(r): Json<Read>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || app.record(&who, Body::MessagesRead { workspace: r.workspace, at: r.at }).map(Json)).await
}

/// The user's question in the discussion of a delivered design or research.
#[derive(Deserialize)]
pub(super) struct Discuss {
    unit: String,
    task: u32,
    #[serde(default)]
    text: String,
    #[serde(default)]
    context: Vec<core::AskContext>,
    /// Ask the task's agent for the conclusion instead.
    #[serde(default)]
    conclude: bool,
}

/// The user discusses a delivered design or research with its own agent: recorded, then
/// given to that agent (`App::discuss`, from the record).
pub(super) async fn discuss(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(a): Json<Discuss>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    let body = Body::QuestionAsked { id: String::new(), thread: String::new(), unit: a.unit, task: a.task, text: a.text, context: a.context, conclude: a.conclude, discuss: true };
    blocking(move || app.record(&who, body).map(Json)).await
}

#[derive(Deserialize)]
pub(super) struct DiscussionRead {
    unit: String,
    task: u32,
    at: String,
}

/// The page shows the user a task's discussion: read up to `at`.
pub(super) async fn discussion_read(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(r): Json<DiscussionRead>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || app.record(&who, Body::QuestionsRead { unit: r.unit, task: r.task, at: r.at }).map(Json)).await
}

/// The user asks why: recorded at once; the temporary agent starts in the background.
pub(super) async fn ask_why(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(a): Json<Ask>) -> Result<Json<core::Event>, ApiError> {
    let body = Body::QuestionAsked { id: String::new(), thread: a.thread.unwrap_or_default(), unit: a.unit, task: a.task, text: a.text, context: a.context, conclude: a.conclude, discuss: false };
    let starter = app.clone();
    let e = blocking(move || {
        let who = app.who(&caller, a.named.as_deref())?;
        app.record(&who, body)
    })
    .await?;
    if let Body::QuestionAsked { id, .. } = &e.body {
        let id = id.clone();
        tokio::task::spawn_blocking(move || starter.start_question(&id));
    }
    Ok(Json(e))
}

impl App {
    /// Closes a temporary agent a moment later, so a `workriver answer` it runs can finish.
    pub(super) fn close_later(&self, name: String) {
        let runtime = self.runtime.clone();
        std::thread::spawn(move || {
            std::thread::sleep(CLOSE_DELAY);
            if let Err(e) = runtime.close(&name) {
                tracing::warn!("cannot close {name}: {e}");
            }
        });
    }
}
