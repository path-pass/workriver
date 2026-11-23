// An agent's terminal from the page: the screen and the dialog on it (`/api/agent/screen`),
// keys pressed to answer it (`/api/agent/keys`, press.rs), and the terminal itself as a
// stream (`/api/agent/view`, view.rs); and, for every message workriver types, the read of
// the screen before it (marks.rs). All three routes are the user's alone and on no
// machine's listener: an agent reads no screen of another agent through workriver.
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Digest;
use workriver_core::runtime::{AgentState, RuntimeError};
use workriver_core::screens::{self, Dialog, Key, Screen};

use crate::server::access::*;
use crate::server::app::*;
use crate::server::http::*;

mod marks;
mod press;
mod view;

pub(in crate::server) use marks::Marks;
pub(in crate::server) use view::agent_view;

/// What the service keeps of agents' terminals while it runs, in memory only: the key its
/// digests are drawn with (new at each start, so no digest of an earlier run matches), the
/// presses sent to each agent, one writer per agent at a time, the agents whose screen
/// shows a question the runtime does not report, and how many terminal views are open.
pub(in crate::server) struct Terminals {
    key: String,
    presses: Mutex<HashMap<String, u64>>,
    writers: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    pub(in crate::server) marks: Arc<Mutex<Marks>>,
    views: Arc<AtomicUsize>,
}

/// An agent as these requests name it: its session on a machine (none: this one).
pub(in crate::server) fn agent_key(session: &str, machine: Option<&str>) -> String {
    format!("{}\n{session}", machine.unwrap_or(""))
}

impl Terminals {
    pub(in crate::server) fn new() -> Terminals {
        let key = crate::server::access::auth::random_hex(32).unwrap_or_else(|_| format!("{:?}{}", std::time::SystemTime::now(), std::process::id()));
        Terminals { key, presses: Mutex::default(), writers: Mutex::default(), marks: Arc::default(), views: Arc::default() }
    }

    /// The presses sent to the agent so far.
    fn count(&self, session: &str, machine: Option<&str>) -> u64 {
        self.presses.lock().unwrap().get(&agent_key(session, machine)).copied().unwrap_or(0)
    }

    /// What binds a press to the screen the user saw: SHA-256 over this run's key, the agent,
    /// the presses sent to it before the screen was read (`count`, taken under its writer with
    /// the read) and the whole visible screen as read. A request made before a press never
    /// matches after it, even when the next dialog looks the same.
    pub(in crate::server) fn digest(&self, session: &str, machine: Option<&str>, count: u64, text: &str) -> String {
        let agent = agent_key(session, machine);
        let d = sha2::Sha256::new().chain_update(self.key.as_bytes()).chain_update([0]).chain_update(agent.as_bytes()).chain_update([0]).chain_update(count.to_string()).chain_update([0]).chain_update(text.as_bytes()).finalize();
        d.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// A key went to the agent, or may have.
    fn pressed(&self, session: &str, machine: Option<&str>) {
        *self.presses.lock().unwrap().entry(agent_key(session, machine)).or_default() += 1;
    }

    /// The one writer of an agent: held while a press or a message is under way, so the two
    /// never interleave.
    pub(in crate::server) fn writer(&self, session: &str, machine: Option<&str>) -> Arc<Mutex<()>> {
        self.writers.lock().unwrap().entry(agent_key(session, machine)).or_default().clone()
    }
}

/// Which agent: its session, on a machine (empty or none: this one).
#[derive(Deserialize)]
pub(in crate::server) struct Which {
    agent: String,
    #[serde(default)]
    machine: Option<String>,
}

impl Which {
    fn machine(&self) -> Option<&str> {
        self.machine.as_deref().filter(|m| !m.is_empty() && *m != "local")
    }
}

/// A refusal of these routes, for the page to say in the user's language (`code`), with the
/// service's words and, when the screen was read, the screen as it is now.
pub(in crate::server) struct Refusal {
    status: StatusCode,
    code: &'static str,
    reason: String,
    screen: Option<Box<ScreenView>>,
}

impl Refusal {
    fn new(status: StatusCode, code: &'static str, reason: impl Into<String>) -> Refusal {
        Refusal { status, code, reason: reason.into(), screen: None }
    }
}

impl From<ApiError> for Refusal {
    fn from(e: ApiError) -> Refusal {
        Refusal::new(e.0, "refused", e.1)
    }
}

/// A runtime's error before anything was sent.
impl From<RuntimeError> for Refusal {
    fn from(e: RuntimeError) -> Refusal {
        match e {
            RuntimeError::NoAgent(_) => Refusal::new(StatusCode::NOT_FOUND, "gone", "It is gone: no live agent has that session there"),
            RuntimeError::Ambiguous(why) => Refusal::new(StatusCode::CONFLICT, "two", format!("It shows in two places; answer it in its terminal ({why})")),
            RuntimeError::Unsupported => Refusal::new(StatusCode::SERVICE_UNAVAILABLE, "runtime", "the runtime cannot read or answer an agent's terminal"),
            e => Refusal::new(StatusCode::SERVICE_UNAVAILABLE, "runtime", e.to_string()),
        }
    }
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        (self.status, [(header::CACHE_CONTROL, "no-store")], Json(json!({ "code": self.code, "reason": self.reason, "screen": self.screen }))).into_response()
    }
}

/// A screen as the page is given it: every piece of its text marked (`screens::marked`),
/// with its digest, the dialog read off it, the runtime's reason, and whether and since when
/// the agent waits (the runtime says blocked, or only its screen shows a question).
#[derive(Serialize, Clone)]
pub(in crate::server) struct ScreenView {
    text: String,
    cols: u16,
    rows: u16,
    digest: String,
    dialog: Option<Dialog>,
    #[serde(skip_serializing_if = "Option::is_none")]
    why: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    waiting: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    since: Option<String>,
}

/// A dialog with each piece of its text marked; its rows stay as read.
fn shown(mut d: Dialog) -> Dialog {
    for o in &mut d.options {
        o.words = screens::marked(&o.words);
        o.more = o.more.iter().map(|m| screens::marked(m)).collect();
    }
    for h in &mut d.hints {
        h.words = screens::marked(&h.words);
    }
    d.above = d.above.map(|a| screens::marked(&a));
    d.question = d.question.map(|q| screens::marked(&q));
    d
}

/// The two GETs here do something (`/view` starts a child and takes a slot, `/screen` reads a
/// place), and the guard lets a GET through on the session cookie alone: a page on another
/// port of this host is the same site, and its requests carry the cookie. So a request that
/// came with the cookie must say it comes from our own page; a missing header is refused
/// too. The user's bearer token (the CLI) is not a page's to add.
fn from_our_page(caller: &Caller, headers: &HeaderMap) -> Result<(), Refusal> {
    let ours = headers.get("sec-fetch-site").is_some_and(|v| v == "same-origin");
    if caller.session && !caller.token && !ours {
        return Err(Refusal::new(StatusCode::FORBIDDEN, "origin", "only from workriver's own page"));
    }
    Ok(())
}

impl App {
    /// Whether the agent waits now, as last polled: the runtime says blocked ("blocked"), or
    /// only its screen shows a question ("screen"); and since when.
    pub(in crate::server) fn waiting(&self, session: &str, machine: Option<&str>) -> Option<(&'static str, Option<String>)> {
        let live = self.live()?;
        let a = live.into_iter().find(|a| a.session == session && a.machine.as_deref() == machine)?;
        let marks = self.terminals.marks.lock().unwrap();
        let since = self.blocked.lock().unwrap().get(session).cloned();
        match (marks.on.get(&agent_key(session, machine)), a.state) {
            (Some(at), _) => Some(("screen", Some(at.clone()))),
            (None, AgentState::Blocked) => Some(("blocked", since)),
            _ => None,
        }
    }

    /// The screen at `place` as the page gets it, read when `count` presses had been sent: under
    /// the agent's writer, so no press was under way then. The runtime's reason is asked after.
    pub(in crate::server) fn screen_view(&self, session: &str, machine: Option<&str>, place: &str, count: u64, screen: Screen, why: bool) -> ScreenView {
        let (waiting, since) = self.waiting(session, machine).map(|(w, s)| (Some(w), s)).unwrap_or_default();
        let why = (why && waiting == Some("blocked")).then(|| self.runtime.why_blocked(machine, place).ok().flatten()).flatten();
        ScreenView {
            digest: self.terminals.digest(session, machine, count, &screen.text),
            dialog: screens::parse(&screen.text).map(shown),
            text: screens::marked(&screen.text),
            cols: screen.cols,
            rows: screen.rows,
            why: why.map(|w| screens::marked(&w)),
            waiting,
            since,
        }
    }
}

/// `GET /api/agent/screen`: what the agent's terminal shows now, its digest and the dialog on
/// it, read at the one place that shows the session; a seated manager's at the place its
/// seat is held to, as the last tick found it ("gone" while the seat is lost).
pub(in crate::server) async fn agent_screen(State(app): State<Shared>, Extension(caller): Extension<Caller>, headers: HeaderMap, Query(q): Query<Which>) -> Result<Response, Refusal> {
    app.user(&caller)?;
    from_our_page(&caller, &headers)?;
    let view = tokio::task::spawn_blocking(move || -> Result<ScreenView, Refusal> {
        let machine = q.machine();
        // A seated manager's where the tick found its seat held; any other's, the one place
        // that shows it.
        let place = app.seat_place(&q.agent, machine).unwrap_or_else(|| app.runtime.resolve(machine, &q.agent))?;
        // The screen and the presses before it, together: a press under way ends first (it
        // holds the writer until its keys took effect), and one after it counts past it.
        let (count, screen) = {
            let writer = app.terminals.writer(&q.agent, machine);
            let _one = writer.lock().unwrap();
            (app.terminals.count(&q.agent, machine), app.runtime.screen(machine, &place)?)
        };
        Ok(app.screen_view(&q.agent, machine, &place, count, screen, true))
    })
    .await
    .map_err(|e| Refusal::new(StatusCode::INTERNAL_SERVER_ERROR, "failed", e.to_string()))??;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(view)).into_response())
}

/// What the page asks to press: an option of the dialog by its number in the list (from 1),
/// or one key.
#[derive(Debug, PartialEq)]
pub(in crate::server) enum Press {
    Option(usize),
    Key(Key),
}

/// `POST /api/agent/keys` `{ agent, machine, shown, press: { option: 2 } | { key: "escape" } }`.
pub(in crate::server) async fn agent_keys(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(v): Json<Value>) -> Result<Response, Refusal> {
    app.user(&caller)?;
    let text = |k: &str| v[k].as_str().map(str::to_string);
    let invalid = |why: &str| Refusal::new(StatusCode::BAD_REQUEST, "invalid", why);
    let (Some(agent), Some(shown)) = (text("agent"), text("shown")) else { return Err(invalid("name the agent by its session, and the digest of the screen shown")) };
    let machine = text("machine").filter(|m| !m.is_empty() && m != "local");
    let p = &v["press"];
    let press = match (p.get("option"), p.get("key")) {
        (Some(o), None) => Press::Option(o.as_u64().filter(|n| (1..=9).contains(n)).ok_or_else(|| invalid("an option is its number, 1 to 9"))? as usize),
        (None, Some(k)) => Press::Key(k.as_str().and_then(Key::named).ok_or_else(|| invalid("a key is one of up, down, enter, escape, tab, 1-9; never text"))?),
        _ => return Err(invalid("press one option or one key")),
    };
    let done = tokio::task::spawn_blocking(move || app.answer(&agent, machine.as_deref(), &shown, press)).await.map_err(|e| Refusal::new(StatusCode::INTERNAL_SERVER_ERROR, "failed", e.to_string()))??;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(done)).into_response())
}
