// The daemon: one process on 127.0.0.1 that records events, reads git, and serves the
// web UI with its data and a stream of "something changed".
use std::path::Path;
use std::time::Duration;

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::json;
use workriver_core::runtime::{AgentState, RuntimeAgent, RuntimeError};
use workriver_core::{self as core, Body, Delivery, DeliveryStep, Principal, ReasonCode};
use workriver_scan as scan;

use crate::runtime::words;
use crate::server::access::auth;
use crate::server::delivery;
use crate::server::files::{self, store};
use crate::{ask, run};
use crate::server::access::*;
use crate::server::app::*;
use crate::server::http::*;
use crate::server::views;

/// A run's shell to close: the run, its machine (none: this one), its place.
type Shell = (String, Option<String>, String);

/// A command the user prepared to run, then ran (`place` set).
#[derive(Clone)]
pub(super) struct Run {
    command: String,
    dir: String,
    machine: Option<String>,
    unit: Option<String>,
    /// Who wrote it: an agent's session, or "user".
    by: String,
    /// Whose step it is (`u1#3`, `c2`) and which (from 1); none: typed by the user.
    from: Option<String>,
    step: Option<u32>,
    /// That step's text, as its author wrote it: what the command is for.
    step_text: Option<String>,
    /// The language of the user's page, for the review.
    lang: Option<String>,
    prepared: std::time::Instant,
    place: Option<String>,
    /// Its place was closed by workriver, is about to be, or is no longer a run's shell: no
    /// terminal to open any more. Taken back when the shell was left (busy, or not reached).
    closed: bool,
    output: String,
    exit: Option<i32>,
    /// Its place is still read.
    watching: bool,
    /// `command.ran` is recorded: its review, once there, is recorded too.
    ran: bool,
    pub(super) review: Review,
}

/// What a workriver agent says a command will do, before the user runs it: it only warns.
#[derive(Clone, Default)]
pub(super) struct Review {
    /// The agent started for it: only it may record the review; closed once it did.
    agent: Option<RuntimeAgent>,
    /// When it is given up.
    pub(super) until: Option<std::time::Instant>,
    verdict: Option<core::Verdict>,
    text: String,
    /// Why there is no review.
    failed: Option<String>,
}

impl Review {
    pub(super) fn pending(&self) -> bool {
        self.verdict.is_none() && self.failed.is_none()
    }
}

/// How long after the decision a temporary agent is closed, so its own command can return.
const CLOSE_DELAY: Duration = Duration::from_secs(3);

impl App {
    /// Starts the temporary agent for a question and gives it the prompt; any failure is
    /// recorded on the question. Blocking: it waits for the agent to be ready.
    fn start_question(&self, id: &str) {
        let fail = |code, why: String| self.fail_question(id, code, &why);
        // Read fresh: a temporary agent started since the last poll counts.
        let Ok(live) = self.runtime.agents() else { return fail(ReasonCode::RuntimeDown, words::NOT_CONNECTED.into()) };
        // Every question, the user's or an agent's, goes to a temporary agent; it may ask the
        // task's own agent while that one is idle (its prompt says how).
        // A follow-up goes to the thread's agent while it lives, which keeps its context.
        let current = {
            let store = self.store.lock().unwrap();
            let s = store.state();
            s.question(id).ok().and_then(|q| s.threads.iter().find(|t| t.id == q.thread)).and_then(|t| Some((t.id.clone(), t.agent.clone()?, t.name.clone()?)))
        };
        if let Some((thread, agent, name)) = current.filter(|(_, _, n)| live.iter().any(|a| &a.name == n)) {
            self.idle.lock().unwrap().retain(|(t, _, _)| *t != thread);
            if let Err(e) = self.record(&Principal::Service, Body::QuestionStarted { question: id.into(), agent, name: name.clone(), temporary: true }) {
                tracing::warn!("question {id}: {}", e.1);
            }
            self.deadlines.lock().unwrap().push((id.to_string(), std::time::Instant::now() + self.waits.answer));
            let text = {
                let store = self.store.lock().unwrap();
                store.state().question(id).ok().map(|q| ask::follow_up(store.state(), q, &self.home)).unwrap_or_default()
            };
            self.hand(Held { question: id.into(), kind: For::Question, to: name.clone(), name, text, not_ready: None });
            return;
        }
        let (q, unit, task_agent, manager, group) = {
            let store = self.store.lock().unwrap();
            let s = store.state();
            let Ok(q) = s.question(id).cloned() else { return };
            let Some(u) = s.unit_or_intent(&q.unit) else { return };
            let task_agent = s.task(&q.unit, q.task).ok().and_then(|t| t.agent.clone());
            let intent = s.intents.iter().find(|i| i.id == u.intent);
            let manager = intent.and_then(|i| s.manager_of(&i.id)).map(str::to_string);
            let group = intent.and_then(|i| i.workspace.clone());
            (q, u, task_agent, manager, group)
        };
        let live_one = |s: &Option<String>| s.as_ref().and_then(|s| live.iter().find(|a| &a.session == s)).cloned();
        let (worker, boss) = (live_one(&task_agent), live_one(&manager));
        // Work on another machine is read there: the group of that machine that is the
        // intent's workspace (linked to it), else where the task's agent runs.
        let group = match &unit.machine {
            None => group,
            Some(m) => {
                let state = self.store.lock().unwrap().state().clone();
                let same = |g: &core::RuntimeRef| group.as_ref().is_some_and(|w| state.workspace_of(g).is_some_and(|a| state.workspace_of(w).is_some_and(|b| a.group == b.group)));
                let there = |g: &&core::RuntimeRef| g.machine.as_ref() == Some(m);
                let groups = self.groups.lock().unwrap().clone();
                groups.iter().filter(there).find(|g| same(g)).cloned().or_else(|| worker.as_ref().and_then(|a| a.group.clone()).filter(|g| there(&g)))
            }
        };
        let Some(group) = group.or_else(|| worker.as_ref().and_then(|a| a.group.clone())).or_else(|| boss.as_ref().and_then(|a| a.group.clone())) else {
            return fail(ReasonCode::NoWorkspace, words::no_group(&unit.intent));
        };
        // Which harness: the task's agent's, else the manager's, else claude.
        let known = |s: &Option<String>| s.as_ref().and_then(|s| self.store.lock().unwrap().state().agents.iter().find(|a| &a.session == s).map(|a| a.harness.clone()));
        let harness = worker.map(|a| a.harness).or_else(|| known(&task_agent)).or_else(|| boss.map(|a| a.harness)).or_else(|| known(&manager)).unwrap_or_else(|| "claude".into());
        // Intent-level work has no branch: its project folder is where to read.
        let at = self.runner(unit.machine.as_deref()).ok().filter(|_| !unit.branch.is_empty());
        let dir = at.as_ref().and_then(|at| scan::worktree_of(at, Path::new(&unit.repo), &unit.branch).ok().flatten()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| unit.repo.clone());
        // A question about the whole unit reads its commits itself (its prompt says how).
        let commits = at.filter(|_| q.task != 0).map(|at| task_commits(&at, &unit, q.task, &mut self.cache.lock().unwrap())).unwrap_or_default();
        let name = ask::agent_name(id);
        let agent = match self.runtime.start_temporary(&group.id, &harness, &name, &dir) {
            Ok(a) => a,
            Err(e) => return fail(if matches!(e, RuntimeError::None) { ReasonCode::RuntimeDown } else { ReasonCode::RuntimeError }, format!("cannot start {harness} as {name}: {e}")),
        };
        if let Err(e) = self.record(&Principal::Service, Body::QuestionStarted { question: id.into(), agent: agent.session.clone(), name: agent.name.clone(), temporary: true }) {
            tracing::warn!("question {id}: {}", e.1);
        }
        self.deadlines.lock().unwrap().push((id.to_string(), std::time::Instant::now() + self.waits.answer));
        let prompt = ask::prompt(self.store.lock().unwrap().state(), &q, &dir, &commits, &self.home).unwrap_or_default();
        // A harness opening a folder it has not seen first asks a person (trust this folder,
        // approve the directory): the prompt waits for that, it is never answered for them.
        // The agent may come back not ready yet (Unknown): `hand` holds the prompt.
        self.hand(Held { question: id.into(), kind: For::Question, to: agent.name.clone(), name: agent.name, text: prompt, not_ready: None });
    }

    /// Gives a question's message to the agent that answers it (`to`: a session or a live
    /// agent's name). One waiting for a person in its terminal (blocked) cannot take
    /// it: it is held and given once the agent is free (`poll`), or the question fails with
    /// the reason when its time is up. A new agent the runtime does not take prompts for yet
    /// (it refuses, or shows no state) is tried again at each poll, for `Waits::ready` from
    /// the first try; after that it waits for the user like a blocked one.
    pub(super) fn hand(&self, mut h: Held) {
        let is = |a: &RuntimeAgent| a.session == h.to || a.name == h.to;
        let not_ready = |h: &mut Held| {
            h.not_ready.get_or_insert_with(|| std::time::Instant::now() + self.waits.ready);
        };
        match self.runtime.agents().ok().and_then(|l| l.into_iter().find(is)).map(|a| a.state) {
            Some(AgentState::Blocked) => h.not_ready = None,
            // No state yet: not typed into, whatever the terminal shows.
            Some(AgentState::Unknown) => not_ready(&mut h),
            _ => match self.send(&h.to, &h.text) {
                Ok(()) if h.kind == For::Message => return self.message_delivery(&h.question, Delivery { step: DeliveryStep::Delivered, to: Some(h.to.clone()), reason_code: None, reason: None, message: h.text }),
                Ok(()) => return,
                Err(RuntimeError::Blocked(_)) => h.not_ready = None,
                Err(RuntimeError::NotReady(_)) => not_ready(&mut h),
                Err(e) => {
                    return match h.kind {
                        For::Message => self.message_not_delivered(&h.question, Some(&h.to), ReasonCode::RuntimeError, &format!("cannot give {} the message: {e}", h.name)),
                        For::Question => self.fail_question(&h.question, ReasonCode::RuntimeError, &format!("cannot give {} the question: {e}", h.name)),
                        For::Review => self.review_failed(&h.question, &format!("cannot give {} the command: {e}", h.name)),
                    };
                }
            },
        }
        self.held.lock().unwrap().push(h);
        let _ = self.changed.send(());
    }

    /// The user wrote to a manager: typed into its terminal now, or held while it waits for a
    /// person there (`hand`), or recorded as not delivered when it is not running.
    pub(super) fn tell_manager(&self, id: &str) {
        let Some((to, text)) = ({
            let store = self.store.lock().unwrap();
            store.state().message(id).ok().map(|m| (m.manager.clone(), delivery::user_message(store.state(), m)))
        }) else {
            return;
        };
        let name = self.store.lock().unwrap().state().name_of(&to).unwrap_or(&to).to_string();
        match self.runtime.agents() {
            Err(_) => self.message_not_delivered(id, None, ReasonCode::RuntimeDown, words::NOT_CONNECTED),
            Ok(live) if !live.iter().any(|a| a.session == to) => self.message_not_delivered(id, None, ReasonCode::ManagerGone, &format!("the manager {name} is not running")),
            Ok(_) => self.hand(Held { question: id.into(), kind: For::Message, to, name, text, not_ready: None }),
        }
    }

    /// Records where the user's message to a manager is.
    fn message_delivery(&self, id: &str, delivery: Delivery) {
        self.held.lock().unwrap().retain(|h| h.question != id);
        if let Err(e) = self.record(&Principal::Service, Body::MessageDelivery { id: id.into(), delivery }) {
            tracing::warn!("message {id}: {}", e.1);
        }
    }

    pub(super) fn message_not_delivered(&self, id: &str, to: Option<&str>, code: ReasonCode, why: &str) {
        let message = {
            let store = self.store.lock().unwrap();
            store.state().message(id).ok().map(|m| delivery::user_message(store.state(), m)).unwrap_or_default()
        };
        self.message_delivery(id, Delivery { step: DeliveryStep::NotDelivered, to: to.map(str::to_string), reason_code: Some(code), reason: Some(why.into()), message });
    }

    /// Records a question as failed (if still open) and closes its temporary agent, which
    /// did not answer.
    pub(super) fn fail_question(&self, id: &str, code: ReasonCode, why: &str) {
        self.drop_question(id, code, why);
        // Only an agent started for it is closed; a task's own agent is not.
        let name = self.store.lock().unwrap().state().question(id).ok().filter(|q| q.temporary).and_then(|q| q.name.clone());
        if let Some(name) = name {
            self.close_later(name);
        }
    }

    /// Records a question as failed (if still open) and stops waiting for it.
    pub(super) fn drop_question(&self, id: &str, code: ReasonCode, why: &str) {
        let body = Body::QuestionFailed { question: id.into(), reason_code: code, reason: why.into() };
        if let Err(e) = self.record(&Principal::Service, body) {
            tracing::warn!("question {id}: {}", e.1);
        }
        self.deadlines.lock().unwrap().retain(|(q, _)| q != id);
        self.held.lock().unwrap().retain(|h| h.question != id);
    }

    /// An answer came: the thread's agent waits for a follow-up until it has been idle too long.
    pub(super) fn answered(&self, id: &str) {
        self.deadlines.lock().unwrap().retain(|(q, _)| q != id);
        self.held.lock().unwrap().retain(|h| h.question != id);
        // An agent that asked hears the answer in its terminal.
        let back = {
            let store = self.store.lock().unwrap();
            let s = store.state();
            s.question(id).ok().filter(|q| q.by != "user").map(|q| (q.by.clone(), ask::answer_back(s, q)))
        };
        if let Some((asker, text)) = back
            && let Err(e) = self.send(&asker, &text)
        {
            tracing::warn!("cannot give {asker} the answer to {id}: {e}");
        }
        let found = {
            let store = self.store.lock().unwrap();
            let s = store.state();
            s.question(id).ok().and_then(|q| s.threads.iter().find(|t| t.id == q.thread)).map(|t| (t.id.clone(), t.name.clone()))
        };
        if let Some((thread, Some(name))) = found {
            let mut idle = self.idle.lock().unwrap();
            idle.retain(|(t, _, _)| *t != thread);
            idle.push((thread, name, std::time::Instant::now() + self.waits.idle));
        }
    }

    /// The user ended a thread: its agent goes now.
    pub(super) fn end_thread(&self, thread: &str) {
        self.idle.lock().unwrap().retain(|(t, _, _)| t != thread);
        let name = self.store.lock().unwrap().state().threads.iter().find(|t| t.id == thread).and_then(|t| t.name.clone());
        if let Some(name) = name {
            self.close_later(name);
        }
    }

    /// Closes a temporary agent a moment later, so a `workriver answer` it runs can finish.
    fn close_later(&self, name: String) {
        let runtime = self.runtime.clone();
        std::thread::spawn(move || {
            std::thread::sleep(CLOSE_DELAY);
            if let Err(e) = runtime.close(&name) {
                tracing::warn!("cannot close {name}: {e}");
            }
        });
    }

    /// A project as given (`p1`, or a folder) as its folder: a registered project's, or the
    /// repository's top folder for a repository outside every registered project.
    pub(super) fn project_folder(&self, given: &str, machine: Option<&str>) -> Result<String, ApiError> {
        let state = self.store.lock().unwrap().state().clone();
        if let Some(p) = state.projects.iter().find(|p| p.id == given) {
            return Ok(p.path.clone());
        }
        // On another machine: the repository's top folder there.
        if machine.is_some() {
            let at = self.runner(machine)?;
            return scan::toplevel(&at, Path::new(given)).map(|t| t.to_string_lossy().into_owned()).map_err(|_| bad(format!("{given} is not a git repository on that machine; name a project of this machine by its id (p1)")));
        }
        let path = std::fs::canonicalize(given).map_err(|e| bad(format!("project {given:?}: no such id or folder ({e})")))?;
        let path = path.to_string_lossy().into_owned();
        if let Some(p) = views::project_of(&state, &path) {
            return Ok(p.path.clone());
        }
        scan::toplevel(&scan::Runner::Local, Path::new(&path)).map(|t| t.to_string_lossy().into_owned()).map_err(|_| bad(format!("{path} is neither a registered project nor a git repository; register it with `workriver project add`")))
    }
}


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
            let path = store::path(&app.home, &a.id, &a.name);
            Some(if remote {
                files::fetch_command(&path.file_name()?.to_string_lossy())
            } else {
                path.display().to_string()
            })
        };
        let mut shown = crate::server::views::task::task(&state, &ws, &unit, n, &file)?;
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
    blocking(move || {
        let who = app.who(&caller, a.named.as_deref())?;
        let e = app.record(&who, body)?;
        let mut out = serde_json::to_value(&e).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        // A manager that starts an intent of another project than its workspace's is told so.
        if let Body::IntentStateChanged { intent, state: core::IntentState::Active, workspace: Some(g), .. } = &e.body {
            let live = app.runtime.agents().unwrap_or_default();
            if let Some(why) = views::other_project(app.store.lock().unwrap().state(), &live, g, intent) {
                out["warning"] = json!(format!("{why}. If you do not work on that project, do not split it: ask the user with a decide gate first (`workriver guide how ask-user`)."));
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
}

pub(super) async fn gate(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(g): Json<Gate>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    let body = Body::GateSubmitted { unit: g.unit, task: g.task, result: g.result, note: g.note, findings: g.findings, attachments: g.attachments };
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
pub(super) struct Focus {
    agent: String,
}

pub(super) async fn focus(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(f): Json<Focus>) -> Result<axum::response::Response, ApiError> {
    app.user(&caller)?;
    blocking(move || {
        // The page knows agents by name.
        let session = app.session_of(&f.agent, None);
        app.runtime.focus(&session).map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
        // On another machine it comes to the front there, in that machine's runtime: the user's own
        // client stays where it is (the runtime has no way to switch it), so the page says where.
        let there = app.live().unwrap_or_default().into_iter().find(|a| a.session == session && a.machine.is_some());
        Ok(match there {
            Some(a) => Json(json!({ "machine": a.machine, "workspace": a.group.map(|g| g.label), "agent": a.name })).into_response(),
            None => StatusCode::NO_CONTENT.into_response(),
        })
    })
    .await
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
            if let Some(why) = views::other_project(app.store.lock().unwrap().state(), &live, &group, &s.intent) {
                return Err(ApiError(StatusCode::CONFLICT, format!("{why}: its manager would get work from a project it is not in. Pick a workspace of that project, or confirm starting it there anyway (confirmOtherProject)")));
            }
        }
        let e = app.record(&who, Body::IntentStateChanged { intent: s.intent.clone(), state: core::IntentState::Active, reason: None, workspace: Some(group) })?;
        let live = app.runtime.agents().ok();
        let plan = delivery::started(app.store.lock().unwrap().state(), live.as_deref(), &s.intent);
        let name = |to: &str| app.store.lock().unwrap().state().name_of(to).unwrap_or(to).to_string();
        let delivery = match plan {
            Ok((to, message)) => match app.send(&to, &message) {
                Ok(()) => Delivery { step: DeliveryStep::Delivered, to: Some(name(&to)), reason_code: None, reason: None, message },
                Err(err) => Delivery { step: DeliveryStep::NotDelivered, to: Some(name(&to)), reason_code: Some(if matches!(err, RuntimeError::None) { ReasonCode::RuntimeDown } else { ReasonCode::RuntimeError }), reason: Some(err.to_string()), message },
            },
            Err((code, reason)) => Delivery { step: DeliveryStep::NotDelivered, to: None, reason_code: Some(code), reason: Some(reason), message: String::new() },
        };
        // As the page names a delivery's fields.
        let delivery = json!({ "step": delivery.step, "to": delivery.to, "reasonCode": delivery.reason_code, "reason": delivery.reason, "message": delivery.message });
        Ok(Json(json!({ "event": e, "delivery": delivery })))
    })
    .await
}

#[derive(Deserialize)]
pub(super) struct RunPrepare {
    command: String,
    /// Where it runs: a unit's work tree (on its machine), or a project's folder.
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    project: Option<String>,
    /// Whose step it is: a task's (`u1#3`) or a chore's (`c2`); none: the user's own.
    #[serde(default)]
    from: Option<String>,
    /// Which of its steps (from 1).
    #[serde(default)]
    step: Option<u32>,
    /// The language of the page (`en`, `zh-CN`): its review is written in it.
    #[serde(default)]
    lang: Option<String>,
}

/// How long a prepared command may wait for the user's confirmation.
const CONFIRM: Duration = Duration::from_secs(600);

/// A run as the page shows it: in the confirm dialog, then with what its place shows.
/// `waiting`: its review agent, when it waits for a person in its terminal (a first-run
/// prompt such as "trust this folder?"): the page says so and leads there.
fn run_view(id: &str, r: &Run, state: &core::State, waiting: Option<&RuntimeAgent>) -> serde_json::Value {
    let by = if r.by == "user" { "user" } else { state.name_of(&r.by).unwrap_or(&r.by) };
    let v = &r.review;
    let review = match (&v.verdict, &v.failed) {
        (Some(verdict), _) => json!({ "state": "done", "verdict": verdict, "text": v.text }),
        (None, Some(why)) => json!({ "state": "failed", "reason": why }),
        (None, None) => match waiting {
            Some(a) => json!({ "state": "blocked", "agent": a.name, "session": a.session }),
            None => json!({ "state": "pending" }),
        },
    };
    json!({ "id": id, "command": r.command, "dir": r.dir, "machine": r.machine, "unit": r.unit, "by": by,
            "started": r.place.is_some(), "output": r.output, "exit": r.exit, "watching": r.watching,
            "kept": true, "focusable": r.place.is_some() && !r.closed, "review": review })
}

/// A run this service no longer follows (it restarted since): only what the record says,
/// without its output or its terminal.
fn recorded_view(r: &core::CommandRun, state: &core::State) -> serde_json::Value {
    let by = if r.by == "user" { "user" } else { state.name_of(&r.by).unwrap_or(&r.by) };
    let review = r.review.as_ref().map(|(verdict, text)| json!({ "state": "done", "verdict": verdict, "text": text }));
    json!({ "id": r.id, "command": r.command, "dir": r.dir, "machine": r.machine, "unit": r.unit, "by": by,
            "started": true, "output": "", "exit": r.exit, "watching": false, "kept": false, "focusable": false, "review": review })
}

impl App {
    /// The user's recent runs, newest first: the record's, with what this service still
    /// follows of them (whether a place is read, where it is).
    pub(super) fn recent_runs(&self, state: &core::State) -> Vec<views::RunOut> {
        let live = self.runs.lock().unwrap();
        state
            .runs
            .iter()
            .rev()
            .map(|r| {
                let here = live.get(&r.id);
                let exit = r.exit.or(here.and_then(|x| x.exit));
                let now = if exit.is_some() { "done" } else if here.is_some_and(|x| x.watching) { "running" } else { "unknown" };
                views::RunOut {
                    id: r.id.clone(),
                    command: r.command.clone(),
                    dir: r.dir.clone(),
                    machine: r.machine.clone(),
                    unit: r.unit.clone(),
                    from: r.from.clone(),
                    step: r.step,
                    at: r.at.clone(),
                    state: now,
                    exit,
                    focusable: here.is_some_and(|x| x.place.is_some() && !x.closed),
                    verdict: r.review.as_ref().map(|(v, _)| *v).or(here.and_then(|x| x.review.verdict)),
                }
            })
            .collect()
    }
}

/// The user is about to run a command: where it would run and who wrote it, for the confirm
/// dialog. Nothing runs here; `run_command` runs it, and only with this exact text.
pub(super) async fn run_prepare(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(p): Json<RunPrepare>) -> Result<Json<serde_json::Value>, ApiError> {
    app.user(&caller)?;
    blocking(move || {
        run::check(&p.command).map_err(bad)?;
        let state = app.store.lock().unwrap().state().clone();
        let (dir, machine, unit) = match (&p.unit, &p.project) {
            (Some(u), _) => {
                let u = state.unit(u)?;
                let at = app.runner(u.machine.as_deref())?;
                // A research unit has no work tree: its project's folder.
                let tree = scan::worktree_of(&at, Path::new(&u.repo), &u.branch).ok().flatten().filter(|_| u.has_code());
                (tree.map(|d| d.to_string_lossy().into_owned()).unwrap_or_else(|| u.repo.clone()), u.machine.clone(), Some(u.id.clone()))
            }
            (None, Some(f)) => {
                if !state.projects.iter().any(|x| &x.path == f) && !state.intents.iter().any(|i| &i.project == f) {
                    return Err(bad(format!("{f} is not a project's folder")));
                }
                // A repository on another machine is a project of its own: it runs there.
                let machine = state.units.iter().find(|u| Path::new(&u.repo).starts_with(f)).and_then(|u| u.machine.clone());
                (f.clone(), machine, None)
            }
            (None, None) => return Err(bad("say where it runs: a unit (its work tree) or a project's folder".into())),
        };
        let step = p.from.as_ref().and(p.step);
        let (by, steps) = match p.from.as_deref() {
            None => ("user".to_string(), &[][..]),
            Some(f) => match core::parse_task_ref(f) {
                Some((u, n)) => state.task(u, n).map(|t| (t.by.clone(), &t.steps[..]))?,
                None => state.chores.iter().find(|c| c.id == f).map(|c| (c.by.clone(), &c.steps[..])).ok_or_else(|| bad(format!("no chore {f}")))?,
            },
        };
        let step_text = step.and_then(|n| steps.get(n.checked_sub(1)? as usize)).map(|s| s.text.clone());
        let id = auth::random_hex(8).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let r = Run { command: p.command, dir, machine, unit, by, from: p.from, step, step_text, lang: p.lang, prepared: std::time::Instant::now(), place: None, closed: false, output: String::new(), exit: None, watching: false, ran: false, review: Review::default() };
        let view = run_view(&id, &r, &state, None);
        {
            let mut runs = app.runs.lock().unwrap();
            // Kept: what waits for its confirmation or its review, what is watched, the recent
            // runs' output, and an ended run whose shell the next run has still to close.
            runs.retain(|id, r| match r.place {
                None => r.prepared.elapsed() < CONFIRM || r.review.pending(),
                Some(_) => r.watching || (r.exit.is_some() && !r.closed) || state.runs.iter().any(|x| &x.id == id),
            });
            runs.insert(id.clone(), r);
        }
        // Every command is reviewed, in the background: the user may run it before that ends.
        std::thread::spawn(move || app.review_run(&id));
        Ok(Json(view))
    })
    .await
}

#[derive(Deserialize)]
pub(super) struct RunConfirmed {
    id: String,
    /// The exact text the user was shown and confirmed.
    command: String,
}

/// Runs a prepared command once the user confirmed it: typed into a plain shell of the user's,
/// in workriver's own runtime group, on the machine of its directory. Recorded (not its
/// output); its end is recorded when its place shows it.
pub(super) async fn run_command(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(c): Json<RunConfirmed>) -> Result<Json<serde_json::Value>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || {
        // Claimed under the lock: one confirmation runs once.
        let r = {
            let mut runs = app.runs.lock().unwrap();
            let r = runs.get_mut(&c.id).filter(|r| r.place.is_none() && !r.watching && r.prepared.elapsed() < CONFIRM);
            let r = r.ok_or_else(|| bad("nothing confirmed to run under this id (it ran already, or was shown too long ago): open it again".into()))?;
            if r.command != c.command {
                return Err(bad("this is not the command that was confirmed; nothing ran".into()));
            }
            r.watching = true;
            r.clone()
        };
        // The shells of earlier runs that ended go now, in the background; a failed one
        // stayed until here. Each is claimed under the lock: closed once.
        let ended: Vec<Shell> = {
            let mut runs = app.runs.lock().unwrap();
            let open = runs.iter_mut().filter(|(_, r)| r.exit.is_some() && !r.closed);
            open.filter_map(|(id, r)| {
                let place = r.place.clone()?;
                r.closed = true;
                Some((id.clone(), r.machine.clone(), place))
            })
            .collect()
        };
        let closer = app.clone();
        std::thread::spawn(move || closer.close_shells(ended, Duration::ZERO));
        let line = run::line(&r.dir, &r.command, &c.id);
        let place = match app.runtime.run_shell(r.machine.as_deref(), run::GROUP, &r.dir, &line) {
            Ok(p) => p,
            Err(e) => {
                // Nothing to review any more.
                let gone = app.runs.lock().unwrap().remove(&c.id).filter(|x| x.review.pending());
                if let Some(a) = gone.and_then(|x| x.review.agent) {
                    app.close_later(a.name);
                }
                return Err(ApiError(StatusCode::SERVICE_UNAVAILABLE, format!("nothing ran: {e}")));
            }
        };
        if let Some(x) = app.runs.lock().unwrap().get_mut(&c.id) {
            x.place = Some(place);
        }
        app.record(&who, Body::CommandRan { id: c.id.clone(), command: r.command, dir: r.dir, machine: r.machine, unit: r.unit, by: r.by, from: r.from, step: r.step })
            .map_err(|e| ApiError(e.0, format!("it runs, but could not be recorded: {}", e.1)))?;
        // A review that came before it ran is recorded now; one still to come, when it comes.
        let reviewed = app.runs.lock().unwrap().get_mut(&c.id).and_then(|x| {
            x.ran = true;
            Some((x.review.verdict?, x.review.text.clone()))
        });
        if let Some((verdict, text)) = reviewed {
            app.record_review(&c.id, verdict, text);
        }
        let watcher = app.clone();
        let id = c.id.clone();
        std::thread::spawn(move || watcher.watch_run(&id));
        Ok(Json(json!({ "id": c.id })))
    })
    .await
}

/// A run: what its place shows now, and its exit status once it ended.
pub(super) async fn run_read(State(app): State<Shared>, Extension(caller): Extension<Caller>, UrlPath(id): UrlPath<String>) -> Result<Json<serde_json::Value>, ApiError> {
    app.user(&caller)?;
    let live = app.runs.lock().unwrap().get(&id).cloned();
    let store = app.store.lock().unwrap();
    let state = store.state();
    match (live, state.runs.iter().find(|r| r.id == id)) {
        (Some(r), _) => {
            // Its review agent as the runtime last saw it, when it waits for a person.
            let waiting = r.review.agent.as_ref().filter(|_| r.review.pending()).and_then(|a| app.live()?.into_iter().find(|x| x.name == a.name && x.machine == a.machine && x.state == AgentState::Blocked));
            Ok(Json(run_view(&id, &r, state, waiting.as_ref())))
        }
        // Recorded before this service started: its output was never read here.
        (None, Some(r)) => Ok(Json(recorded_view(r, state))),
        (None, None) => Err(ApiError(StatusCode::NOT_FOUND, format!("no run {id}"))),
    }
}

/// Brings a run's terminal to the front.
pub(super) async fn run_focus(State(app): State<Shared>, Extension(caller): Extension<Caller>, UrlPath(id): UrlPath<String>) -> Result<axum::response::Response, ApiError> {
    app.user(&caller)?;
    let r = app.runs.lock().unwrap().get(&id).cloned().ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("no run {id}")))?;
    let place = r.place.ok_or_else(|| bad(format!("run {id} has not started")))?;
    if r.closed {
        return Err(bad(format!("run {id} ended and its terminal was closed; its output is here")));
    }
    blocking(move || {
        app.runtime.focus_place(r.machine.as_deref(), &place).map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
        // On another machine: in front there; the page says where (see `focus`).
        Ok(match r.machine {
            Some(m) => Json(json!({ "machine": m, "workspace": run::GROUP })).into_response(),
            None => StatusCode::NO_CONTENT.into_response(),
        })
    })
    .await
}

impl App {
    /// Reads a run's place every second until it shows the end (then records the exit status),
    /// its place is gone, or a day has passed.
    fn watch_run(&self, id: &str) {
        let Some((machine, place)) = self.runs.lock().unwrap().get(id).and_then(|r| Some((r.machine.clone(), r.place.clone()?))) else { return };
        let (until, mut misses, mut well) = (std::time::Instant::now() + Duration::from_secs(24 * 3600), 0, false);
        // A place that cannot be read five times running is gone (closed, or its shell exited).
        while std::time::Instant::now() < until && misses < 5 {
            std::thread::sleep(Duration::from_secs(1));
            let Ok(text) = self.runtime.read_place(machine.as_deref(), &place, run::LINES) else {
                misses += 1;
                continue;
            };
            misses = 0;
            let (output, end) = run::read(&text, id);
            if let Some(r) = self.runs.lock().unwrap().get_mut(id) {
                (r.output, r.exit) = (output, end);
                // Ended well: its shell is to go, said with the end so no reader sees one
                // without the other.
                r.closed |= end == Some(0);
            }
            if let Some(exit) = end {
                if let Err(e) = self.record(&Principal::Service, Body::CommandEnded { id: id.to_string(), exit }) {
                    tracing::warn!("cannot record run {id} ended: {}", e.1);
                }
                // Ended well: its output is kept above; its shell goes, below.
                well = exit == 0;
                break;
            }
        }
        if let Some(r) = self.runs.lock().unwrap().get_mut(id) {
            r.watching = false;
        }
        // No longer followed: the page's recent runs show it (an end recorded says so too).
        let _ = self.changed.send(());
        if well {
            self.close_shells(vec![(id.to_string(), machine, place)], CLOSE_DELAY);
        }
    }

    /// Closes, after `wait`, the shells of runs that ended and were claimed for it (`closed`
    /// set): each on its own machine, and only one with nothing left running in it. A shell
    /// that is busy (a background job of the run's, or the user went on working there), or
    /// that could not be asked or closed just now, is left and given back: the next run tries
    /// again. One that is gone, or no longer a run's shell, is never asked about again. Only
    /// places `run_shell` gave a run are ever passed on; a run whose end was not seen
    /// (running, waiting for input) is never claimed. Blocking.
    // ponytail: the places are in memory, so shells left by a service that restarted stay
    // until the user closes them; recording a run's place would let the next start close them.
    fn close_shells(&self, shells: Vec<Shell>, wait: Duration) {
        std::thread::sleep(wait);
        let mut left = false;
        for (id, machine, place) in shells {
            let done = self.shell_idle(machine.as_deref(), &place).and_then(|idle| match idle {
                true => self.runtime.close_shell(machine.as_deref(), run::GROUP, &place).map(|()| true),
                false => Ok(false),
            });
            match done {
                Ok(true) => continue,
                Err(RuntimeError::NoPlace(why)) => {
                    tracing::debug!("the shell of run {id} is not closed: {why}");
                    continue;
                }
                Ok(false) => tracing::debug!("the shell of run {id} is busy: left open"),
                Err(e) => tracing::warn!("cannot close the shell of run {id} now: {e}"),
            }
            if let Some(r) = self.runs.lock().unwrap().get_mut(&id) {
                r.closed = false;
                left = true;
            }
        }
        if left {
            let _ = self.changed.send(());
        }
    }

    /// Whether nothing is left running in a run's shell: nothing but the shell in front, and
    /// nothing the shell started still alive (a job in the background is not in front, so the
    /// machine's own process list is read for it).
    // ponytail: a shell whose prompt keeps a helper process of its own alive always counts as
    // busy and is never closed for the user; safe, and none of the stock shells does.
    fn shell_idle(&self, machine: Option<&str>, place: &str) -> Result<bool, RuntimeError> {
        let (shell, front) = self.runtime.shell_processes(machine, run::GROUP, place)?;
        if front.iter().any(|p| *p != shell) {
            return Ok(false);
        }
        let runner = self.runner(machine).map_err(|e| RuntimeError::Failed(e.1))?;
        let busy = crate::server::machines::metrics::has_children(&runner, shell).map_err(|e| RuntimeError::Failed(format!("cannot list the processes: {e}")))?;
        Ok(!busy)
    }

    /// Starts the agent that tells the user what a prepared command will do, where it would
    /// run (in workriver's own runtime group of that machine), and gives it the command; any
    /// failure is kept on the run. Blocking: it waits for the agent to be ready.
    fn review_run(&self, id: &str) {
        let Some(r) = self.runs.lock().unwrap().get(id).cloned() else { return };
        // The author's harness, which that machine has; else claude.
        let (harness, by) = {
            let store = self.store.lock().unwrap();
            let s = store.state();
            let harness = s.agents.iter().find(|a| a.session == r.by).map(|a| a.harness.clone()).filter(|h| !h.is_empty()).unwrap_or_else(|| "claude".into());
            (harness, if r.by == "user" { "user".to_string() } else { s.name_of(&r.by).unwrap_or(&r.by).to_string() })
        };
        let name = run::reviewer(id);
        let agent = match self.runtime.start_temporary_at(r.machine.as_deref(), run::GROUP, &harness, &name, &r.dir) {
            Ok(a) => a,
            Err(e) => return self.review_failed(id, &format!("cannot start {harness} as {name}: {e}")),
        };
        {
            let mut runs = self.runs.lock().unwrap();
            match runs.get_mut(id) {
                Some(x) => (x.review.agent, x.review.until) = (Some(agent.clone()), Some(std::time::Instant::now() + self.waits.answer)),
                None => {
                    drop(runs);
                    return self.close_later(agent.name);
                }
            }
        }
        let text = run::review_prompt(id, &r.command, &r.dir, &by, r.step_text.as_deref(), r.lang.as_deref());
        // As a question's prompt: held while the agent waits for a person (a first-run prompt).
        self.hand(Held { question: id.into(), kind: For::Review, to: agent.name.clone(), name: agent.name, text, not_ready: None });
    }

    /// There will be no review of run `id`: why, for the page; its agent goes.
    pub(super) fn review_failed(&self, id: &str, why: &str) {
        self.held.lock().unwrap().retain(|h| !(h.kind == For::Review && h.question == id));
        let agent = {
            let mut runs = self.runs.lock().unwrap();
            let Some(r) = runs.get_mut(id).filter(|r| r.review.pending()) else { return };
            r.review.failed = Some(why.into());
            r.review.agent.clone()
        };
        if let Some(a) = agent {
            self.close_later(a.name);
        }
        let _ = self.changed.send(());
    }

    /// Records run `id`'s review, as workriver: it checked who gave it.
    fn record_review(&self, id: &str, verdict: core::Verdict, text: String) {
        if let Err(e) = self.record(&Principal::Service, Body::CommandReviewed { id: id.into(), verdict, text }) {
            tracing::warn!("cannot record the review of run {id}: {}", e.1);
        }
    }
}

#[derive(Deserialize)]
pub(super) struct RunReview {
    #[serde(default, rename = "as")]
    named: Option<String>,
    verdict: core::Verdict,
    text: String,
}

/// The review of a command the user is about to run, from the agent workriver started for it
/// and from nobody else (not even the user): shown in the run's dialog, recorded with the run.
/// On a machine's listener too, for a review agent of that machine.
pub(super) async fn run_review(State(app): State<Shared>, Extension(caller): Extension<Caller>, UrlPath(id): UrlPath<String>, Json(v): Json<RunReview>) -> Result<Json<serde_json::Value>, ApiError> {
    blocking(move || {
        let who = app.who(&caller, v.named.as_deref())?;
        let text = v.text.trim().to_string();
        if text.is_empty() || text.chars().count() > run::REVIEW_CAP {
            return Err(bad(format!("a review says what the command does, in at most {} characters", run::REVIEW_CAP)));
        }
        // The agent at the caller's place, as the runtime names it now: an agent started on a
        // first-run prompt was known by its place then and by its harness's own session since.
        let named = match &who {
            Principal::Agent { session, machine } => app.runtime.agents().ok().and_then(|l| l.into_iter().find(|a| &a.session == session && &a.machine == machine)).map(|a| a.name),
            _ => None,
        };
        let (agent, ran) = {
            let mut runs = app.runs.lock().unwrap();
            let r = runs.get_mut(&id).ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("no run {id} waits for a review")))?;
            let theirs = r.review.agent.as_ref().is_some_and(|a| {
                matches!(&who, Principal::Agent { session, machine } if *machine == a.machine && (*session == a.session || named.as_deref() == Some(a.name.as_str())))
            });
            if !theirs {
                return Err(ApiError(StatusCode::FORBIDDEN, format!("only the agent workriver started to review run {id} records its review")));
            }
            if !r.review.pending() {
                return Err(bad(format!("run {id} has its review already, or it was given up")));
            }
            (r.review.verdict, r.review.text) = (Some(v.verdict), text.clone());
            (r.review.agent.clone(), r.ran)
        };
        app.held.lock().unwrap().retain(|h| !(h.kind == For::Review && h.question == id));
        if ran {
            app.record_review(&id, v.verdict, text);
        } else {
            let _ = app.changed.send(());
        }
        // Its work is done; a moment later, so this command returns first.
        if let Some(a) = agent {
            app.close_later(a.name);
        }
        Ok(Json(json!({ "id": id })))
    })
    .await
}

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
    let file = store::path(&app.home, &a.id, &a.name);
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
        app.store.lock().unwrap().state().task(&i.unit, i.task)?;
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

#[derive(Deserialize)]
pub(super) struct EndThread {
    thread: String,
}

pub(super) async fn end_thread(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(t): Json<EndThread>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || app.record(&who, Body::ThreadEnded { thread: t.thread }).map(Json)).await
}

#[derive(Deserialize)]
pub(super) struct Say {
    /// The intent whose workspace's manager it goes to; or, without one, the workspace's label.
    #[serde(default)]
    intent: String,
    #[serde(default)]
    workspace: String,
    text: String,
}

/// The user writes to a manager from the page; it is typed into the manager's terminal.
pub(super) async fn message(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(m): Json<Say>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || app.record(&who, Body::MessageSent { id: String::new(), intent: m.intent, workspace: m.workspace, manager: String::new(), text: m.text }).map(Json)).await
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

/// The user asks why: recorded at once; the temporary agent starts in the background.
pub(super) async fn ask_why(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(a): Json<Ask>) -> Result<Json<core::Event>, ApiError> {
    let body = Body::QuestionAsked { id: String::new(), thread: a.thread.unwrap_or_default(), unit: a.unit, task: a.task, text: a.text, context: a.context, conclude: a.conclude };
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

/// A task's commits on its unit's branch, as (short id, subject).
fn task_commits(at: &scan::Runner, u: &core::Unit, task: u32, cache: &mut views::Cache) -> Vec<(String, String)> {
    let repo = Path::new(&u.repo);
    let hashes = if u.merged.is_some() && !u.merged_commits.is_empty() {
        u.merged_commits.clone()
    } else {
        views::unit_commits(at, u, cache).map(|(l, _)| l.into_iter().map(|c| c.hash).collect()).unwrap_or_default()
    };
    let want = format!("{}#{task}", u.id);
    hashes.iter().filter_map(|h| scan::commit(at, repo, h).ok()).filter(|c| c.task.as_deref() == Some(want.as_str())).map(|c| (c.id, c.subject)).collect()
}

/// The CLI asks who and where it is: see `App::here`. Nothing it sends is read for that.
pub(super) async fn here(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Json<serde_json::Value>, ApiError> {
    blocking(move || app.here(&caller).map(Json)).await
}

// ---- the web UI, built into the binary (read from disk in debug builds) ----
