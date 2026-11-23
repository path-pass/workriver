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
        let (q, unit, task_agent, manager) = {
            let Ok(q) = s.question(id).cloned() else { return };
            let Ok(u) = s.unit(&q.unit).cloned() else { return };
            let task_agent = s.task(&q.unit, q.task).ok().and_then(|t| t.agent.clone());
            let intent = s.intents.iter().find(|i| i.id == u.intent);
            let manager = intent.and_then(|i| s.manager_of(&i.id)).map(str::to_string);
            (q, u, task_agent, manager)
        };
        drop(s);
        let live_one = |s: &Option<String>| s.as_ref().and_then(|s| live.iter().find(|a| &a.session == s)).cloned();
        let (worker, boss) = (live_one(&task_agent), live_one(&manager));
        // A temporary agent opens beside the one it serves, in that one's group of the
        // runtime (shown only; no record holds it): on the unit's machine, its manager's for
        // work here and the task's agent's for work elsewhere; else wherever either runs.
        let there = |a: &Option<RuntimeAgent>| a.as_ref().filter(|a| a.machine == unit.machine).and_then(|a| a.group.clone());
        let beside = if unit.machine.is_none() { there(&boss).or_else(|| there(&worker)) } else { there(&worker).or_else(|| there(&boss)) };
        let Some(group) = beside.or_else(|| worker.as_ref().and_then(|a| a.group.clone())).or_else(|| boss.as_ref().and_then(|a| a.group.clone())) else {
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
        // A research unit has no branch: its project folder is where to read.
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

    /// The user's message to a manager whose seat is lost: typed into no terminal that only
    /// reports its session.
    fn seat_lost(&self, h: &Held) {
        self.message_not_delivered(&h.question, h.copy(), Some(&h.to), ReasonCode::ManagerGone, &format!("the manager {} does not run in the terminal it was seated in; confirm its new terminal (the workspace's Manager), then write again", h.name))
    }

    /// Whether what is held for `to` is for a session, found by it alone: always a notice and
    /// the user's message (or its copy); a duty and a discussion's question when `to` is a
    /// session as recorded (`names_a_session`), else by the name a task's agent was given as.
    /// Only a temporary agent's question goes by the name workriver gave it.
    pub(in crate::server) fn by_session(&self, kind: For, to: &str) -> bool {
        kind.by_session() || (matches!(kind, For::Duty | For::Discussion) && self.names_a_session(to))
    }

    /// The user's message is for one appointment: its seat was given to another meanwhile.
    fn seat_changed(&self, h: &Held) {
        self.message_not_delivered(&h.question, h.copy(), Some(&h.to), ReasonCode::SeatChanged, &format!("the seat was given anew before {} could be given the message; write to the manager again", h.name))
    }

    /// Gives a question's message to the agent that answers it (`to`: a session or a live
    /// agent's name). One waiting for a person in its terminal (blocked) cannot take
    /// it: it is held and given once the agent is free (`poll`), or the question fails with
    /// the reason when its time is up. A new agent the runtime does not take prompts for yet
    /// (it refuses, or shows no state) is tried again at each poll, for `Waits::ready` from
    /// the first try; after that it waits for the user like a blocked one.
    pub(in crate::server) fn hand(&self, mut h: Held) {
        let by_session = self.by_session(h.kind, &h.to);
        let is = |a: &RuntimeAgent| a.session == h.to || (!by_session && a.name == h.to);
        let not_ready = |h: &mut Held| {
            h.not_ready.get_or_insert_with(|| std::time::Instant::now() + self.waits.ready);
        };
        // A manager on a blind task is told nothing of other work: the user's message waits
        // (`poll` tries again) until that task is delivered. Checked at every try, since it may
        // have become blind while the message was held. Unread, it is not given either.
        // The user's message is for one appointment: its seat given to another meanwhile
        // (another session, or the same one seated anew), it is not delivered, and never
        // re-routed to whoever holds the seat now. Checked at every try.
        if matches!(h.kind, For::Message | For::Broadcast) {
            match self.still_appointed(&h.question, &h.to) {
                Some(true) => {}
                Some(false) => return self.seat_changed(&h),
                None => {
                    h.not_ready = None;
                    self.held.lock().unwrap().push(h);
                    return;
                }
            }
        }
        // Nor is it typed into a terminal that only reports the manager's session: its own
        // is gone, and the user has not confirmed another. As the last read found it, at
        // once; a seat lost since is found where it would be typed (`place_of`).
        if matches!(h.kind, For::Message | For::Broadcast) && self.seat_found_lost(&h.to) {
            return self.seat_lost(&h);
        }
        if matches!(h.kind, For::Message | For::Broadcast | For::Notice | For::Discussion | For::Duty) && !self.blind_free(&h.to) {
            h.not_ready = None;
            self.held.lock().unwrap().push(h);
            return;
        }
        let live = self.runtime.agents();
        let found = live.as_ref().ok().and_then(|l| l.iter().find(|a| is(a)));
        // Its session gone, a notice is given to nobody: the runtime would look for an agent
        // of that name next. (A runtime that does not answer says so itself, below.)
        if h.kind == For::Notice && live.is_ok() && found.is_none() {
            let gone = RuntimeError::NoAgent(h.name.clone());
            return self.told(h, Err(gone));
        }
        // A discussion's agent is named as the runtime lists it when it is given the question.
        if let (For::Discussion, Some(a)) = (h.kind, found) {
            h.name = a.name.clone();
        }
        // What is for a session is typed at that session alone (`send_by_session`: a seated
        // one where its seat is held to), never at whatever row was found: what that
        // machine's runtime answers is kept (one that drops before typing stays one that
        // typed nothing). A temporary agent's question by the name workriver gave it.
        let sent = |h: &Held| if by_session { self.send_by_session(&h.to, &h.text) } else { self.send(&h.to, &h.text) };
        match found.map(|a| a.state) {
            Some(AgentState::Blocked) => h.not_ready = None,
            // No state yet: not typed into, whatever the terminal shows.
            Some(AgentState::Unknown) => not_ready(&mut h),
            // A discussion never interrupts its agent's work; one not listed (the runtime not
            // read, or gone) waits for `poll`, which alone says it is gone.
            Some(AgentState::Working) | None if h.kind == For::Discussion => h.not_ready = None,
            None if h.kind == For::Duty => h.not_ready = None,
            // The user's message to a manager not listed (the runtime not read, or not listed
            // as it is given): it waits for `poll`, which alone says it is gone, from a read.
            None if matches!(h.kind, For::Message | For::Broadcast) => h.not_ready = None,
            _ => {
                // A discussion's question: the records are read again right before typing in, in
                // order with a gate's record (`App::discussing`, held by both): one closed
                // meanwhile goes nowhere, one whose gate was submitted fails now, and with
                // records that cannot be read it waits. A gate recorded later comes after the
                // send, which then stands as asked before the user decided. The user's message
                // alike with a seat change (`App::appointing`): its appointment ended meanwhile,
                // it is not delivered; a seat changed later comes after the send.
                // Every other kind in order with a seat change too, so what is sent to a manager
                // is typed into the seat as it is: the lock, then where it is held to
                // (`place_of`), then the agent's writer.
                let message = matches!(h.kind, For::Message | For::Broadcast);
                let _order = match h.kind {
                    For::Discussion => self.discussing.lock().unwrap(),
                    _ => self.appointing.lock().unwrap(),
                };
                let asked = match h.kind {
                    For::Discussion => self.still_asked(&h.question),
                    _ if message => self.still_appointed(&h.question, &h.to),
                    _ => Some(true),
                };
                match asked {
                    Some(false) if message => return self.seat_changed(&h),
                    Some(false) => return,
                    None => h.not_ready = None,
                    Some(true) => match sent(&h) {
                        Ok(()) if matches!(h.kind, For::Message | For::Broadcast) => return self.message_delivery(&h.question, h.copy(), Delivery { step: DeliveryStep::Delivered, to: Some(h.to.clone()), reason_code: None, reason: None, message: h.text.clone() }),
                        Ok(()) if h.kind == For::Discussion => return self.discussion_given(&h),
                        Ok(()) if h.kind == For::Duty => return,
                        Ok(()) if h.kind == For::Notice => return self.told(h, Ok(())),
                        Ok(()) => return,
                        Err(RuntimeError::Blocked(_)) => h.not_ready = None,
                        Err(RuntimeError::NotReady(_)) => not_ready(&mut h),
                        // The user's message to a manager whose runtime cannot be reached now:
                        // nothing was typed, so it waits and is tried again (`poll`) until the
                        // manager takes it or is gone. Not one that may have been typed: that is
                        // not given twice.
                        Err(RuntimeError::None | RuntimeError::Unreachable(_)) if matches!(h.kind, For::Message | For::Broadcast) => h.not_ready = None,
                        // Its seat lost since the last read: not where it is held to.
                        Err(RuntimeError::NoAgent(_)) if message && self.seat_machine(&h.to).is_ok_and(|s| s.is_some()) => return self.seat_lost(&h),
                        // Never lost while the service runs: tried again at the next poll.
                        Err(e) if h.kind == For::Duty => {
                            tracing::warn!("cannot give {} what it must act on yet: {e}", h.name);
                            h.not_ready = None
                        }
                        Err(e) => {
                            return match h.kind {
                                For::Message | For::Broadcast => self.message_not_delivered(&h.question, h.copy(), Some(&h.to), ReasonCode::RuntimeError, &format!("cannot give {} the message: {e}", h.name)),
                                For::Question => self.fail_question(&h.question, ReasonCode::RuntimeError, &format!("cannot give {} the question: {e}", h.name)),
                                For::Review => self.review_failed(&h.question, &format!("cannot give {} the command: {e}", h.name)),
                                For::Notice => self.told(h, Err(e)),
                                For::Discussion => self.drop_question(&h.question, ReasonCode::RuntimeError, &format!("cannot give {} the question: {e}", h.name)),
                                For::Duty => unreachable!("a duty is held, never dropped"),
                            };
                        }
                    },
                }
            }
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
        // By who it is in the runtime too (the last poll): a task may be its by a name only.
        let live = self.live().unwrap_or_default();
        match read {
            Ok(Some(s)) if !delivery::blind(&s, &live, session) => None,
            Ok(Some(s)) => {
                let name = s.name_of(session).or_else(|| live.iter().find(|a| a.session == session).map(|a| a.name.as_str())).unwrap_or(session);
                Some(format!("{name} is on a blind task: it is told once that task is delivered"))
            }
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
