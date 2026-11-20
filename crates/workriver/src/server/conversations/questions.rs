// A question's life in the service: its agent started and given the prompt, the message
// held while that agent cannot take it, the answer taken back to who asked, failure, a
// thread ended.
use super::*;

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

impl App {
    /// Starts the temporary agent for a question and gives it the prompt; any failure is
    /// recorded on the question. Blocking: it waits for the agent to be ready.
    pub(super) fn start_question(&self, id: &str) {
        let fail = |code, why: String| self.fail_question(id, code, &why);
        // Read fresh: a temporary agent started since the last poll counts.
        let Ok(live) = self.runtime.agents() else { return fail(ReasonCode::RuntimeDown, words::NOT_CONNECTED.into()) };
        // Every question, the user's or an agent's, goes to a temporary agent; it may ask the
        // task's own agent while that one is idle (its prompt says how).
        // A follow-up goes to the thread's agent while it lives, which keeps its context.
        // Whether it is one is read, not assumed: a store that cannot be read starts no agent.
        let unread = |Unread(why)| fail(ReasonCode::RuntimeError, why);
        let current = match self.thread_of(id) {
            Ok(t) => t.and_then(|t| Some((t.id, t.agent?, t.name?))),
            Err(e) => return unread(e),
        };
        if let Some((thread, agent, name)) = current.filter(|(_, _, n)| live.iter().any(|a| &a.name == n)) {
            self.idle.lock().unwrap().retain(|(t, _, _)| *t != thread);
            if let Err(e) = self.record(&Principal::Service, Body::QuestionStarted { question: id.into(), agent, name: name.clone(), temporary: true }) {
                tracing::warn!("question {id}: {}", e.1);
            }
            self.deadlines.lock().unwrap().push((id.to_string(), std::time::Instant::now() + self.waits.answer));
            // Nothing is given in place of a text that could not be read: the question fails,
            // and the thread's agent is closed as one that did not answer, by the name in hand.
            let read = got(self.store.lock().unwrap().recorded());
            let text = match read {
                Ok(all) => all.as_ref().and_then(|all| all.question(id).ok().map(|q| prompts::follow_up(all, q, &self.home))).unwrap_or_default(),
                Err(Unread(why)) => return self.fail_started(id, name, &why),
            };
            self.hand(Held { question: id.into(), kind: For::Question, to: name.clone(), name, text, not_ready: None, about: None });
            return;
        }
        let read = got(self.store.lock().unwrap().recorded());
        let s = match read {
            Ok(Some(s)) => s,
            Ok(None) => return,
            Err(e) => return unread(e),
        };
        let (q, unit, task_agent, manager, group) = {
            let Ok(q) = s.question(id).cloned() else { return };
            let Some(u) = s.unit_or_intent(&q.unit) else { return };
            let task_agent = s.task(&q.unit, q.task).ok().and_then(|t| t.agent.clone());
            let intent = s.intents.iter().find(|i| i.id == u.intent);
            let manager = intent.and_then(|i| s.manager_of(&i.id)).map(str::to_string);
            let group = intent.and_then(|i| i.workspace.clone());
            (q, u, task_agent, manager, group)
        };
        drop(s);
        let live_one = |s: &Option<String>| s.as_ref().and_then(|s| live.iter().find(|a| &a.session == s)).cloned();
        let (worker, boss) = (live_one(&task_agent), live_one(&manager));
        // Work on another machine is read there: the group of that machine that is the
        // intent's workspace (linked to it), else where the task's agent runs.
        let group = match &unit.machine {
            None => group,
            Some(m) => {
                let read = got(self.store.lock().unwrap().recorded());
                let state = match read {
                    Ok(Some(state)) => state,
                    Ok(None) => return,
                    Err(e) => return unread(e),
                };
                let same = |g: &core::RuntimeRef| group.as_ref().is_some_and(|w| state.workspace_of(g).is_some_and(|a| state.workspace_of(w).is_some_and(|b| a.group == b.group)));
                let there = |g: &&core::RuntimeRef| g.machine.as_ref() == Some(m);
                let groups = self.groups.lock().unwrap().clone();
                groups.iter().filter(there).find(|g| same(g)).cloned().or_else(|| worker.as_ref().and_then(|a| a.group.clone()).filter(|g| there(&g)))
            }
        };
        let Some(group) = group.or_else(|| worker.as_ref().and_then(|a| a.group.clone())).or_else(|| boss.as_ref().and_then(|a| a.group.clone())) else {
            return fail(ReasonCode::NoWorkspace, words::no_group(&unit.intent));
        };
        // Which harness: the task's agent's, else the manager's, else claude. Claude only
        // when it is known that neither is on record: with a store that cannot be read, none.
        let known = |s: &Option<String>| match s {
            Some(s) => got(self.store.lock().unwrap().agent(s)).map(|a| a.flatten().map(|a| a.harness)),
            None => Ok(None),
        };
        let harness = (|| {
            if let Some(a) = worker {
                return Ok(a.harness);
            }
            if let Some(h) = known(&task_agent)? {
                return Ok(h);
            }
            if let Some(a) = boss {
                return Ok(a.harness);
            }
            Ok(known(&manager)?.unwrap_or_else(|| "claude".into()))
        })();
        let harness = match harness {
            Ok(h) => h,
            Err(e) => return unread(e),
        };
        // Intent-level work has no branch: its project folder is where to read.
        let at = self.runner(unit.machine.as_deref()).ok().filter(|_| !unit.branch.is_empty());
        let dir = self.work_tree(&unit);
        // A question about the whole unit reads its commits itself (its prompt says how).
        let commits = at.filter(|_| q.task != 0).map(|at| task_commits(&at, &unit, q.task, &mut self.cache.lock().unwrap())).unwrap_or_default();
        let name = prompts::agent_name(id);
        let agent = match self.runtime.start_temporary(&group.id, &harness, &name, &dir) {
            Ok(a) => a,
            Err(e) => return fail(if matches!(e, RuntimeError::None) { ReasonCode::RuntimeDown } else { ReasonCode::RuntimeError }, format!("cannot start {harness} as {name}: {e}")),
        };
        if let Err(e) = self.record(&Principal::Service, Body::QuestionStarted { question: id.into(), agent: agent.session.clone(), name: agent.name.clone(), temporary: true }) {
            tracing::warn!("question {id}: {}", e.1);
        }
        self.deadlines.lock().unwrap().push((id.to_string(), std::time::Instant::now() + self.waits.answer));
        // Nothing is given in place of a prompt that could not be read: the question fails,
        // and the agent just started is closed as one that did not answer, by the name in
        // hand (the store that cannot be read would not give it).
        let read = got(self.store.lock().unwrap().recorded());
        let prompt = match read {
            Ok(all) => all.and_then(|all| prompts::prompt(&all, &q, &dir, &commits, &self.home)).unwrap_or_default(),
            Err(Unread(why)) => return self.fail_started(id, agent.name, &why),
        };
        // A harness opening a folder it has not seen first asks a person (trust this folder,
        // approve the directory): the prompt waits for that, it is never answered for them.
        // The agent may come back not ready yet (Unknown): `hand` holds the prompt.
        self.hand(Held { question: id.into(), kind: For::Question, to: agent.name.clone(), name: agent.name, text: prompt, not_ready: None, about: None });
    }

    /// Gives a question's message to the agent that answers it (`to`: a session or a live
    /// agent's name). One waiting for a person in its terminal (blocked) cannot take
    /// it: it is held and given once the agent is free (`poll`), or the question fails with
    /// the reason when its time is up. A new agent the runtime does not take prompts for yet
    /// (it refuses, or shows no state) is tried again at each poll, for `Waits::ready` from
    /// the first try; after that it waits for the user like a blocked one.
    pub(in crate::server) fn hand(&self, mut h: Held) {
        let is = |a: &RuntimeAgent| a.session == h.to || a.name == h.to;
        let not_ready = |h: &mut Held| {
            h.not_ready.get_or_insert_with(|| std::time::Instant::now() + self.waits.ready);
        };
        // A manager on a blind task is told nothing of other work: the user's message waits
        // (`poll` tries again) until that task is delivered. Checked at every try, since it may
        // have become blind while the message was held. Unread, it is not given either.
        if matches!(h.kind, For::Message | For::Notice | For::Discussion | For::Duty) && !self.blind_free(&h.to) {
            h.not_ready = None;
            self.held.lock().unwrap().push(h);
            return;
        }
        let found = self.runtime.agents().ok().and_then(|l| l.into_iter().find(is));
        // A discussion's agent is named as the runtime lists it when it is given the question.
        if let (For::Discussion, Some(a)) = (h.kind, &found) {
            h.name = a.name.clone();
        }
        match found.map(|a| a.state) {
            Some(AgentState::Blocked) => h.not_ready = None,
            // No state yet: not typed into, whatever the terminal shows.
            Some(AgentState::Unknown) => not_ready(&mut h),
            // A discussion never interrupts its agent's work; one not listed (the runtime not
            // read, or gone) waits for `poll`, which alone says it is gone.
            Some(AgentState::Working) | None if h.kind == For::Discussion => h.not_ready = None,
            None if h.kind == For::Duty => h.not_ready = None,
            _ => match self.send(&h.to, &h.text) {
                Ok(()) if h.kind == For::Message => return self.message_delivery(&h.question, Delivery { step: DeliveryStep::Delivered, to: Some(h.to.clone()), reason_code: None, reason: None, message: h.text }),
                Ok(()) if h.kind == For::Discussion => return self.discussion_given(&h),
                Ok(()) if h.kind == For::Duty => return,
                Ok(()) if h.kind == For::Notice => return self.told(h, Ok(())),
                Ok(()) => return,
                Err(RuntimeError::Blocked(_)) => h.not_ready = None,
                Err(RuntimeError::NotReady(_)) => not_ready(&mut h),
                // Never lost while the service runs: tried again at the next poll.
                Err(e) if h.kind == For::Duty => {
                    tracing::warn!("cannot tell {} of a discussion yet: {e}", h.name);
                    h.not_ready = None
                }
                Err(e) => {
                    return match h.kind {
                        For::Message => self.message_not_delivered(&h.question, Some(&h.to), ReasonCode::RuntimeError, &format!("cannot give {} the message: {e}", h.name)),
                        For::Question => self.fail_question(&h.question, ReasonCode::RuntimeError, &format!("cannot give {} the question: {e}", h.name)),
                        For::Review => self.review_failed(&h.question, &format!("cannot give {} the command: {e}", h.name)),
                        For::Notice => self.told(h, Err(e)),
                        For::Discussion => self.drop_question(&h.question, ReasonCode::RuntimeError, &format!("cannot give {} the question: {e}", h.name)),
                        For::Duty => unreachable!("a duty is held, never dropped"),
                    };
                }
            },
        }
        self.held.lock().unwrap().push(h);
        let _ = self.changed.send(());
    }

    /// Whether `session` is known, as recorded now, to be on no blind task in progress.
    pub(in crate::server) fn blind_free(&self, session: &str) -> bool {
        self.held_because(session).is_none()
    }

    /// Why nothing of other work may be given to `session` now, in the service's words: it
    /// is on a blind task in progress, or the records that would say cannot be read.
    pub(in crate::server) fn held_because(&self, session: &str) -> Option<String> {
        let read = got(self.store.lock().unwrap().recorded());
        match read {
            Ok(Some(s)) if s.blind_tasks(session).is_empty() => None,
            Ok(Some(s)) => Some(format!("{} is on a blind task: it is told once that task is delivered", s.name_of(session).unwrap_or(session))),
            Ok(None) | Err(_) => Some("the records cannot be read now: it is told once they can".into()),
        }
    }

    /// The thread a question belongs to, as recorded now.
    fn thread_of(&self, question: &str) -> Result<Option<core::Thread>, Unread> {
        let Some(q) = got(self.store.lock().unwrap().question(question))? else { return Ok(None) };
        Ok(got(self.store.lock().unwrap().thread(&q.thread))?.flatten())
    }

    /// A question whose temporary agent runs (`name`) and whose records cannot be read: it
    /// fails as one whose start failed, and that agent is closed. By the name the caller
    /// holds, never by reading it back, and whether or not the failure could be recorded
    /// (`drop_question` logs that): no agent is left running for a question nobody gave it.
    fn fail_started(&self, id: &str, name: String, why: &str) {
        self.drop_question(id, ReasonCode::RuntimeError, why);
        self.close_later(name);
    }

    /// Records a question as failed (if still open) and closes its temporary agent, which
    /// did not answer.
    pub(in crate::server) fn fail_question(&self, id: &str, code: ReasonCode, why: &str) {
        self.drop_question(id, code, why);
        // Only an agent started for it is closed; a task's own agent is not.
        let name = got(self.store.lock().unwrap().question(id)).ok().flatten().filter(|q| q.temporary).and_then(|q| q.name.clone());
        if let Some(name) = name {
            self.close_later(name);
        }
    }

    /// Records a question as failed (if still open) and stops waiting for it.
    pub(in crate::server) fn drop_question(&self, id: &str, code: ReasonCode, why: &str) {
        let body = Body::QuestionFailed { question: id.into(), reason_code: code, reason: why.into() };
        if let Err(e) = self.record(&Principal::Service, body) {
            tracing::warn!("question {id}: {}", e.1);
        }
        self.deadlines.lock().unwrap().retain(|(q, _)| q != id);
        self.held.lock().unwrap().retain(|h| h.question != id);
    }

    /// An answer came: the thread's agent waits for a follow-up until it has been idle too long.
    pub(in crate::server) fn answered(&self, id: &str) {
        self.deadlines.lock().unwrap().retain(|(q, _)| q != id);
        self.held.lock().unwrap().retain(|h| h.question != id);
        // An agent that asked hears the answer in its terminal.
        let back = {
            let s = got(self.store.lock().unwrap().recorded()).ok().flatten();
            s.as_ref().and_then(|s| s.question(id).ok().filter(|q| q.by != "user").map(|q| (q.by.clone(), prompts::answer_back(s, q))))
        };
        // Not while it is on a blind task it took since: held until that is delivered, never lost.
        if let Some((asker, text)) = back
            && let Ok(Err(e)) = self.tell(&asker, &text, None)
        {
            tracing::warn!("cannot give {asker} the answer to {id}: {e}");
        }
        let found = self.thread_of(id).ok().flatten().map(|t| (t.id, t.name));
        if let Some((thread, Some(name))) = found {
            let mut idle = self.idle.lock().unwrap();
            idle.retain(|(t, _, _)| *t != thread);
            idle.push((thread, name, std::time::Instant::now() + self.waits.idle));
        }
    }

    /// The user ended a thread: its agent goes now.
    pub(in crate::server) fn end_thread(&self, thread: &str) {
        self.idle.lock().unwrap().retain(|(t, _, _)| t != thread);
        let name = got(self.store.lock().unwrap().thread(thread)).ok().flatten().flatten().and_then(|t| t.name);
        if let Some(name) = name {
            self.close_later(name);
        }
    }
}
