// The agents workriver holds itself (hosted): the routes of `workriver agent …` and of the
// experimental switch. The user starts, answers and closes; a workspace's seated manager
// prompts, reads and cancels its own workers; nobody else does anything (`State::drives`).
// Every request reads the records and the caller's rights anew: a page of a conversation is
// a request of its own. The conversation stays in the host's journal.
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::{Value, json};
use workriver_core::{self as core, Body, Principal};

use crate::common::config::harness;
use crate::runtime::{HostError, Settings};
use crate::server::access::*;
use crate::server::app::*;
use crate::server::delivery::quote;
use crate::server::http::*;

mod leftovers;
mod lifecycle;

pub(in crate::server) use lifecycle::Hosting;
use lifecycle::CLOSE_WAIT;

/// What a page of a conversation holds at most.
const PAGE: usize = 1000;
/// The bytes of a page of a journal no host reads for us (the host's own budget).
const PAGE_BYTES: usize = 2 * 1024 * 1024 - 1024;

#[derive(Deserialize)]
pub(in crate::server) struct Start {
    workspace: String,
    name: String,
    #[serde(default = "codex")]
    harness: String,
    #[serde(default)]
    model: String,
    cwd: String,
    auth: String,
}

fn codex() -> String {
    "codex".into()
}

#[derive(Deserialize)]
pub(in crate::server) struct Said {
    text: String,
}

#[derive(Deserialize)]
pub(in crate::server) struct Answering {
    request: u64,
    decision: core::hosted::Answer,
}

#[derive(Deserialize)]
pub(in crate::server) struct Page {
    #[serde(default)]
    after: u64,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Deserialize)]
pub(in crate::server) struct Switch {
    on: bool,
}

impl App {
    /// The hosted agent `agent` names (its id, or an open one's name) as the records have it
    /// now, if `who` may drive it (`State::drives`): refused alike whether it exists or not.
    /// An agent on an undelivered blind task drives nothing.
    fn driven(&self, who: &Principal, agent: &str) -> Result<core::HostedAgent, ApiError> {
        let all = self.store.lock().unwrap().recorded()?;
        let blind = matches!(who, Principal::Agent { session, .. } if all.blind_view(session).is_some());
        match all.hosted_named(agent) {
            Ok(a) if !blind && all.drives(who, a) => Ok(a.clone()),
            _ => Err(ApiError(StatusCode::FORBIDDEN, format!("no hosted agent {agent} that you may drive: the user drives every one, a workspace's manager its own workers"))),
        }
    }

    /// What a hosted agent is now: its records, and its host's word while it runs.
    fn shown(&self, a: &core::HostedAgent) -> Value {
        let (state, host) = self.state_of(a);
        json!({
            "id": a.id, "name": a.name, "workspace": a.workspace, "harness": a.harness, "model": a.started.as_ref().map(|s| &s.model).filter(|m| !m.is_empty()).unwrap_or(&a.model),
            "cwd": a.cwd, "auth": a.auth, "reserved": a.at, "state": state, "session": a.session(),
            "place": a.started.as_ref().map(|s| a.place(s.generation)), "host": a.started.as_ref().map(|s| s.host.pid),
            "version": a.started.as_ref().map(|s| &s.version), "account": a.started.as_ref().map(|s| &s.account), "approvals": a.started.as_ref().map(|s| &s.approvals),
            "ended": a.ended, "closed": a.closed, "pending": host.map(|h| h["pending"].clone()),
        })
    }

    /// Its state in one word (`starting`, `idle`, `working`, `blocked`, `closing`,
    /// `unconfirmed`, `stopped`, `incompatible`, `unknown`, `ended`, `closed`), with its
    /// host's status when it answered.
    fn state_of(&self, a: &core::HostedAgent) -> (String, Option<Value>) {
        if a.ended.is_some() {
            return ("ended".into(), None);
        }
        if a.closed.is_some() {
            return ("closed".into(), None);
        }
        let Some(s) = &a.started else { return ("starting".into(), None) };
        let (closing, stopped, incompatible) = {
            let h = self.hosting.lock().unwrap();
            (h.closing.get(&a.id).copied(), h.stopped.contains(&a.id), h.incompatible.contains(&a.id))
        };
        if incompatible {
            return ("incompatible".into(), None);
        }
        if let Some(at) = closing {
            return (if at.elapsed() > CLOSE_WAIT { "unconfirmed" } else { "closing" }.into(), None);
        }
        if stopped || !self.hosts.alive(&a.id) {
            return ("stopped".into(), None);
        }
        match self.hosts.call(&a.id, "status", json!({}), Duration::from_secs(5)) {
            Ok(st) if st["generation"] == s.generation => (st["state"].as_str().unwrap_or("unknown").into(), Some(st)),
            _ => ("unknown".into(), None),
        }
    }

    /// A message to a hosted agent from the user, or from its manager as the records say the
    /// sender is (`quote::said`), aimed at its place: once, with a key of its own. Checked
    /// and sent under `appointing`, so a seat change and the send are in order.
    fn prompt_hosted(&self, who: &Principal, agent: &str, text: &str) -> Result<Value, ApiError> {
        let _order = self.appointing.lock().unwrap();
        let a = self.driven(who, agent)?;
        let session = a.session().filter(|_| a.running()).ok_or_else(|| bad(format!("{} is not running", a.id)))?.to_string();
        // The user's own prompt is theirs, as typed into a terminal; a manager's says who sends it.
        let text = match who {
            Principal::Agent { session, .. } => quote::said(&self.store.lock().unwrap().agent_name(session)?.unwrap_or_else(|| session.clone()), "your manager", text),
            _ => text.to_string(),
        };
        match self.type_at(&session, None, &fresh_key(), &text) {
            Ok(()) => Ok(json!({"agent": a.id, "sent": true})),
            Err(e @ (workriver_core::runtime::RuntimeError::Busy(_) | workriver_core::runtime::RuntimeError::Blocked(_))) => Err(ApiError(StatusCode::CONFLICT, format!("{} takes no message now: {e}; try again once it is idle", a.id))),
            Err(e) => Err(ApiError(StatusCode::SERVICE_UNAVAILABLE, format!("{} was not given the message: {e}", a.id))),
        }
    }

    /// A page of its conversation: from its host while it runs, else from its journal.
    fn read_hosted(&self, a: &core::HostedAgent, p: &Page) -> Result<Value, ApiError> {
        let limit = p.limit.unwrap_or(200).clamp(1, PAGE);
        if a.running() && self.hosts.alive(&a.id) {
            match self.hosts.call(&a.id, "read", json!({"after": p.after, "limit": limit}), Duration::from_secs(10)) {
                Ok(page) => return Ok(page),
                Err(HostError::Refused(why)) => return Err(ApiError(StatusCode::SERVICE_UNAVAILABLE, format!("{} cannot read its journal: {why}", a.id))),
                Err(HostError::Unreachable(_)) => {}
            }
        }
        crate::runtime::journal_page(&self.hosts.journal(&a.id), p.after, limit, PAGE_BYTES).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("cannot read the journal of {}: {e:#}", a.id)))
    }

    /// The user's answer to one approval request, by its key: the host settles it first; only
    /// a settlement it took is recorded.
    fn answer_hosted(&self, a: &core::HostedAgent, x: &Answering) -> Result<Value, ApiError> {
        let s = a.started.as_ref().filter(|_| a.running()).ok_or_else(|| bad(format!("{} is not running", a.id)))?;
        let option = match x.decision {
            core::hosted::Answer::Allow => "allow",
            core::hosted::Answer::Reject => "reject",
        };
        let params = json!({"generation": s.generation, "request": x.request, "optionId": option});
        match self.hosts.call(&a.id, "answer", params, Duration::from_secs(5)) {
            Ok(r) => {
                self.record(&Principal::Service, Body::AgentAnswered { agent: a.id.clone(), request: x.request, decision: x.decision })?;
                Ok(json!({"agent": a.id, "request": x.request, "decision": option, "replied": r["replied"]}))
            }
            Err(HostError::Refused(why)) => Err(bad(format!("{} did not take the answer: {}", a.id, refusal(&why)))),
            Err(e) => Err(ApiError(StatusCode::SERVICE_UNAVAILABLE, format!("{} did not take the answer: {e}", a.id))),
        }
    }
}

/// A host's refusal of an answer, in the user's words.
fn refusal(why: &str) -> String {
    match why {
        "settled" => "that request is not waiting for an answer of that kind (settled, or not offered)".into(),
        "gone" => "it is closing, or that is another generation's request".into(),
        other => other.into(),
    }
}

/// Starts a hosted agent: the user's alone, while the switch is on. Answers with the
/// reservation at once; the start goes on in the background (`agent show` tells how it ends).
pub(in crate::server) async fn start(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(s): Json<Start>) -> Result<Json<Value>, ApiError> {
    blocking(move || {
        let who = app.user(&caller)?;
        if !Settings::hosted(&app.home) {
            return Err(bad("the hosted runtime is off: the user turns it on with `workriver agent hosted on` (an experiment)".into()));
        }
        if !harness::harness(&s.harness).is_some_and(|h| h.hosted) {
            return Err(bad(format!("{:?} cannot be hosted yet: only codex", s.harness)));
        }
        if !std::path::Path::new(&s.cwd).is_dir() {
            return Err(bad(format!("{} is not a folder here", s.cwd)));
        }
        let body = Body::AgentStartReserved { id: String::new(), workspace: s.workspace, name: s.name, harness: s.harness, model: s.model, cwd: s.cwd, auth: s.auth };
        let e = app.record(&who, body)?;
        let Body::AgentStartReserved { id, .. } = &e.body else { unreachable!() };
        let a = app.store.lock().unwrap().recorded()?.hosted_agent(id)?.clone();
        let bg = app.clone();
        std::thread::spawn(move || bg.start_hosted(a));
        Ok(Json(json!({"agent": id, "state": "starting"})))
    })
    .await
}

/// The hosted agents the caller may drive, with their state.
pub(in crate::server) async fn list(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Json<Value>, ApiError> {
    blocking(move || {
        let who = app.who(&caller, None)?;
        let all = app.store.lock().unwrap().recorded()?;
        let blind = matches!(&who, Principal::Agent { session, .. } if all.blind_view(session).is_some());
        let mine: Vec<core::HostedAgent> = all.hosted.iter().filter(|a| !blind && (who == Principal::User || all.drives(&who, a))).cloned().collect();
        drop(all);
        Ok(Json(json!({"on": Settings::hosted(&app.home), "agents": mine.iter().map(|a| app.shown(a)).collect::<Vec<_>>()})))
    })
    .await
}

pub(in crate::server) async fn show(State(app): State<Shared>, Extension(caller): Extension<Caller>, Path(agent): Path<String>) -> Result<Json<Value>, ApiError> {
    blocking(move || {
        let who = app.who(&caller, None)?;
        let a = app.driven(&who, &agent)?;
        Ok(Json(app.shown(&a)))
    })
    .await
}

pub(in crate::server) async fn prompt(State(app): State<Shared>, Extension(caller): Extension<Caller>, Path(agent): Path<String>, Json(s): Json<Said>) -> Result<Json<Value>, ApiError> {
    blocking(move || {
        let who = app.who(&caller, None)?;
        app.prompt_hosted(&who, &agent, &s.text).map(Json)
    })
    .await
}

pub(in crate::server) async fn read(State(app): State<Shared>, Extension(caller): Extension<Caller>, Path(agent): Path<String>, Query(p): Query<Page>) -> Result<Json<Value>, ApiError> {
    blocking(move || {
        let who = app.who(&caller, None)?;
        let a = app.driven(&who, &agent)?;
        app.read_hosted(&a, &p).map(Json)
    })
    .await
}

pub(in crate::server) async fn status(State(app): State<Shared>, Extension(caller): Extension<Caller>, Path(agent): Path<String>) -> Result<Json<Value>, ApiError> {
    blocking(move || {
        let who = app.who(&caller, None)?;
        let a = app.driven(&who, &agent)?;
        let (state, host) = app.state_of(&a);
        Ok(Json(json!({"agent": a.id, "state": state, "pending": host.as_ref().map_or(json!({}), |h| h["pending"].clone()), "head": host.as_ref().map(|h| h["head"].clone())})))
    })
    .await
}

pub(in crate::server) async fn cancel(State(app): State<Shared>, Extension(caller): Extension<Caller>, Path(agent): Path<String>) -> Result<Json<Value>, ApiError> {
    blocking(move || {
        let who = app.who(&caller, None)?;
        let a = app.driven(&who, &agent)?;
        let s = a.started.as_ref().filter(|_| a.running()).ok_or_else(|| bad(format!("{} is not running", a.id)))?;
        match app.hosts.call(&a.id, "cancel", json!({"generation": s.generation}), Duration::from_secs(15)) {
            Ok(r) => Ok(Json(json!({"agent": a.id, "state": r["state"]}))),
            Err(e) => Err(ApiError(StatusCode::SERVICE_UNAVAILABLE, format!("{} was not cancelled: {e}", a.id))),
        }
    })
    .await
}

/// The user's answer to an approval request; nobody else answers one (a manager neither).
pub(in crate::server) async fn answer(State(app): State<Shared>, Extension(caller): Extension<Caller>, Path(agent): Path<String>, Json(x): Json<Answering>) -> Result<Json<Value>, ApiError> {
    blocking(move || {
        let who = app.user(&caller)?;
        let a = app.driven(&who, &agent)?;
        app.answer_hosted(&a, &x).map(Json)
    })
    .await
}

/// The user's close; answers once the host took it (`closing`), the outcome is recorded from
/// its journal.
pub(in crate::server) async fn close(State(app): State<Shared>, Extension(caller): Extension<Caller>, Path(agent): Path<String>) -> Result<Json<Value>, ApiError> {
    blocking(move || {
        let who = app.user(&caller)?;
        let a = app.driven(&who, &agent)?;
        app.close_hosted(&a).map(Json)
    })
    .await
}

#[derive(Deserialize)]
pub(in crate::server) struct Ending {
    #[serde(default)]
    pid: Option<u32>,
    #[serde(default)]
    all: bool,
}

/// The processes of the hosted agents: each running one's, and those left over (the user's:
/// it shows what runs as the user's account).
pub(in crate::server) async fn processes(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Json<Value>, ApiError> {
    blocking(move || {
        app.user(&caller)?;
        leftovers::list_shown(&app).map(Json)
    })
    .await
}

/// The user ends one leftover process, or all of them; nothing else ends one.
pub(in crate::server) async fn processes_close(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(e): Json<Ending>) -> Result<Json<Value>, ApiError> {
    blocking(move || {
        app.user(&caller)?;
        if e.pid.is_none() && !e.all {
            return Err(bad("name a leftover process (`pid`) or all of them (`all`)".into()));
        }
        app.end_leftovers(e.pid).map(Json)
    })
    .await
}

/// Whether the hosted runtime is on: anyone who reads may know it.
pub(in crate::server) async fn switch_get(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Json<Value>, ApiError> {
    blocking(move || {
        app.who(&caller, None)?;
        Ok(Json(json!({"on": Settings::hosted(&app.home)})))
    })
    .await
}

/// The user turns the hosted runtime on or off. Off stops nothing that runs: hosted agents
/// still running are read, answered and closed; only new starts are refused.
pub(in crate::server) async fn switch_set(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(s): Json<Switch>) -> Result<Json<Value>, ApiError> {
    blocking(move || {
        app.user(&caller)?;
        Settings::set_hosted(&app.home, s.on).map_err(bad)?;
        Ok(Json(json!({"on": s.on})))
    })
    .await
}
