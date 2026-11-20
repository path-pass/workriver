// The daemon: one process on 127.0.0.1 that records events, reads git, and serves the
// web UI with its data and a stream of "something changed".
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
use crate::ask;
use crate::server::access::*;
use crate::server::app::*;
use crate::server::http::*;
use crate::server::views;

/// How long after the decision a temporary agent is closed, so its own command can return.
pub(super) const CLOSE_DELAY: Duration = Duration::from_secs(3);

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
