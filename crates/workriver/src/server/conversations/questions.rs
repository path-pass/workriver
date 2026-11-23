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
            self.hand(Held::temporary(For::Question, id, name, text));
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
        self.hand(Held::temporary(For::Question, id, agent.name, prompt));
    }

    /// Gives `h` now, against a read of the agents made now (`give`), or holds it.
    pub(in crate::server) fn hand(&self, h: Held) -> Outcome {
        let live = self.runtime.agents();
        self.give(h, live.as_deref())
    }

    /// One try at giving `h` against `live`, one read of the agents (the poll gives everything
    /// held its own): the same five steps for every kind, and what differs by kind is its
    /// `For::policy` and whether it has a record of its own (`Held::recorded`). What has a
    /// record ends in it; what has none is never dropped while it is still wanted.
    pub(in crate::server) fn give(&self, mut h: Held, live: Result<&[RuntimeAgent], &RuntimeError>) -> Outcome {
        let p = h.kind.policy();
        let read = got(self.store.lock().unwrap().recorded());
        let Ok(Some(all)) = read else { return self.hold(h, UNREAD.into()) };
        // 1. Still wanted: its appointment, its question, its task.
        if let Err((code, why)) = wanted(&all, &h, live.ok()) {
            return self.ended(h, code, why);
        }
        let live = match live {
            Ok(live) => live,
            Err(e) => return self.hold(h, e.to_string()),
        };
        // 2. Nothing of other work for an agent on a blind task (a note of that task itself is
        // its own).
        if p.by_session && !own_blind(&all, &h, live) && delivery::blind(&all, live, &h.to) {
            let why = format!("{} is on a blind task: it is told once that task is delivered", h.name);
            return self.hold(h, why);
        }
        // 3. Its receiver in this read; blind too, the agent a name found (step 2 again).
        let row = match self.receiver(&all, &h.to, p.by_session, live) {
            Ok(Some(row)) => row,
            Ok(None) => return self.failed(h, RuntimeError::NoAgent(String::new())),
            Err(e) => return self.failed(h, e),
        };
        if p.by_session && row.session != h.to && !own_blind(&all, &h, live) && delivery::blind(&all, live, &row.session) {
            let why = format!("{} is on a blind task: it is told once that task is delivered", h.name);
            return self.hold(h, why);
        }
        drop(all);
        if h.kind == For::Discussion {
            h.name = row.name.clone();
        }
        // 4. Its state: one that waits for a person, or shows no state yet, is not typed into;
        // nor is one at work, for a discussion's question.
        let name = h.name.clone();
        match row.state {
            AgentState::Blocked => return self.hold(h, format!("{name} waits for a person in its terminal")),
            AgentState::Unknown => return self.not_ready(h, format!("{name} is not ready for a message yet")),
            AgentState::Working if !p.working => return self.hold(h, format!("{name} is at work")),
            _ => {}
        }
        // 5. Typed, in order with a gate's record (a discussion's question: `discussing`) or a
        // seat change (everything else: `appointing`), and still wanted under that lock, read
        // again: whichever comes first, the other sees it. The lock, then where the receiver
        // is (`place_of`, in `type_at`), then its writer; a seat change waits for this send,
        // over ssh too.
        let _order = match h.kind {
            For::Discussion => self.discussing.lock().unwrap(),
            _ => self.appointing.lock().unwrap(),
        };
        let read = got(self.store.lock().unwrap().recorded());
        let Ok(Some(all)) = read else { return self.hold(h, UNREAD.into()) };
        if let Err((code, why)) = wanted(&all, &h, Some(live)) {
            return self.ended(h, code, why);
        }
        drop(all);
        match self.type_at(&row.session, row.machine.as_deref(), &h.text) {
            Ok(()) => self.given(h),
            Err(e) => self.failed(h, e),
        }
    }

    /// Who `to` is in `live`. A session the records know (`delivery::known_session`) is that
    /// session alone, when `by_session`; anything else is a name (a temporary agent's, or a
    /// task's agent given as a name): the live agents `named` by it. A seated session is
    /// listed only at the place its seat is held to; a seat the last read of its machine did
    /// not find there (lost, or not read) is read again now (`place_of`) before it counts as
    /// not listed. Several: `Ambiguous`. A seat not read: that error.
    pub(in crate::server) fn receiver(&self, all: &core::State, to: &str, by_session: bool, live: &[RuntimeAgent]) -> Result<Option<RuntimeAgent>, RuntimeError> {
        let rows: Vec<&RuntimeAgent> = if by_session && delivery::known_session(all, to) { live.iter().filter(|a| a.session == to).collect() } else { delivery::named(live, to) };
        if let Some(seat) = all.seat_of(to).filter(|w| !w.closed).and_then(|w| w.manager.as_ref()).map(|m| m.machine.clone()) {
            let at = match self.place_as_read(to, seat.as_deref(), seat.as_deref()) {
                Ok(place) => Ok(place),
                Err(_) => self.place_of(to, seat.as_deref()),
            };
            return match at {
                Ok(_) => Ok(rows.into_iter().find(|a| a.machine == seat).cloned()),
                Err(RuntimeError::NoAgent(_)) => Ok(None),
                Err(e) => Err(e),
            };
        }
        match rows[..] {
            [] => Ok(None),
            [one] => Ok(Some(one.clone())),
            _ => Err(RuntimeError::Ambiguous(format!("{} live agents are {to}", rows.len()))),
        }
    }

    /// What a try that typed nothing, or may have, comes to. Not free (waits for a person, not
    /// ready) or nothing typed: held. Gone (not listed, its seat lost), at two places, or
    /// maybe typed: what has a record of its own ends in it (never typed twice); what has
    /// none is held (and typed again). A discussion's question is gone only by
    /// `discussions_gone`.
    fn failed(&self, h: Held, e: RuntimeError) -> Outcome {
        let ends = h.recorded() && (h.kind != For::Discussion || !matches!(e, RuntimeError::NoAgent(_)));
        match e {
            RuntimeError::Blocked(why) => self.hold(h, why),
            RuntimeError::NotReady(why) => self.not_ready(h, why),
            RuntimeError::None | RuntimeError::Unreachable(_) => self.hold(h, e.to_string()),
            RuntimeError::NoAgent(_) if ends => {
                let seated = self.seat_machine(&h.to).is_ok_and(|s| s.is_some());
                let why = match seated {
                    true => format!("the manager {} does not run in the terminal it was seated in; confirm its new terminal (the workspace's Manager), then write again", h.name),
                    false => format!("{} is not running", h.name),
                };
                let code = if matches!(h.kind, For::Question | For::Review) { ReasonCode::RuntimeError } else { ReasonCode::ManagerGone };
                self.ended(h, code, why)
            }
            e if ends => {
                let why = format!("cannot give it to {}: {e}", h.name);
                self.ended(h, ReasonCode::RuntimeError, why)
            }
            RuntimeError::NoAgent(_) => {
                let why = format!("{} is not running", h.name);
                self.hold(h, why)
            }
            e => {
                tracing::warn!("cannot give {} what it is held for yet: {e}", h.name);
                self.hold(h, e.to_string())
            }
        }
    }

    /// Held, and tried again at each poll.
    fn hold(&self, h: Held, why: String) -> Outcome {
        self.park(Held { not_ready: None, ..h }, why)
    }

    /// Held for an agent not ready for a prompt yet: tried again at each poll for
    /// `Waits::ready` from the first try; after that it waits for the user like a blocked one.
    fn not_ready(&self, mut h: Held, why: String) -> Outcome {
        h.not_ready.get_or_insert_with(|| std::time::Instant::now() + self.waits.ready);
        self.park(h, why)
    }

    /// Into `App.held`. A planned delivery is recorded held, with why, before it first goes
    /// there: the page shows it held, and a restart before it is given records it not
    /// delivered (`recover`).
    fn park(&self, mut h: Held, why: String) -> Outcome {
        if let Some(plan) = h.about.as_ref().filter(|_| !h.recorded_held) {
            let delivery = Delivery { step: DeliveryStep::Held, to: Some(h.to.clone()), reason_code: None, reason: Some(why.clone()), message: plan.message.clone() };
            self.record_delivery(plan, delivery);
            h.recorded_held = true;
        }
        self.held.lock().unwrap().push(h);
        let _ = self.changed.send(());
        Outcome::Held(why)
    }

    /// Typed: recorded where it has a record of its own.
    fn given(&self, h: Held) -> Outcome {
        match h.kind {
            For::Message | For::Broadcast => self.message_delivery(&h.question, h.copy(), Delivery { step: DeliveryStep::Delivered, to: Some(h.to.clone()), reason_code: None, reason: None, message: h.text.clone() }),
            For::Discussion => self.discussion_given(&h),
            For::Notice => {
                if let Some(plan) = h.about {
                    self.planned(plan, Ok(()));
                }
            }
            For::Question | For::Review | For::Note => {}
        }
        Outcome::Given
    }

    /// It will not be given: recorded so where it has a record of its own, else logged.
    fn ended(&self, h: Held, code: ReasonCode, why: String) -> Outcome {
        match h.kind {
            For::Message | For::Broadcast => self.message_not_delivered(&h.question, h.copy(), Some(&h.to), code, &why),
            For::Question => self.fail_question(&h.question, code, &why),
            For::Review => self.review_failed(&h.question, &why),
            For::Discussion => self.drop_question(&h.question, code, &why),
            For::Notice | For::Note => match h.about {
                Some(plan) => self.planned(plan, Err((code, why.clone()))),
                None => tracing::info!("not given to {}: {why}", h.name),
            },
        }
        Outcome::Ended(why)
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
        // An agent that asked hears the answer in its terminal; held until it can take it
        // (on a blind task it took since: until that is delivered), never lost.
        let back = {
            let s = got(self.store.lock().unwrap().recorded()).ok().flatten();
            s.as_ref().and_then(|s| s.question(id).ok().filter(|q| q.by != "user").map(|q| Held::notice(s, &q.by, prompts::answer_back(s, q), None)))
        };
        if let Some(h) = back {
            self.hand(h);
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

/// The task a note is about (`u1#3`), as recorded.
fn note_task<'a>(all: &'a core::State, h: &Held) -> Option<&'a core::Task> {
    let (u, n) = h.question.split_once('#').filter(|_| h.kind == For::Note)?;
    all.task(u, n.parse().ok()?).ok()
}

/// Whether task `t` is with `to`: given to that session, or to a name that this read finds
/// it by (`delivery::running`). Not read: as given.
fn holds(all: &core::State, live: Option<&[RuntimeAgent]>, t: &core::Task, to: &str) -> bool {
    t.agent.as_deref().is_some_and(|a| a == to || live.is_none_or(|l| delivery::running(all, l, a).is_some_and(|x| x.session == to)))
}

/// What `give` holds for when the records cannot be read.
const UNREAD: &str = "the records cannot be read now";

/// Whether `h` is still wanted, by the records `all`: the appointment `to` held when it was
/// handed (`Held::seat`); a message's own appointment; a discussion's question still open
/// with its gate not submitted; a note's task still started and with `to` (`live`: this
/// try's read, for a task given to a name). Err: why it is over.
fn wanted(all: &core::State, h: &Held, live: Option<&[RuntimeAgent]>) -> Result<(), (ReasonCode, String)> {
    let message = matches!(h.kind, For::Message | For::Broadcast);
    let appointed = h.seat.as_ref().is_none_or(|(w, since)| all.appointed(w, &h.to, *since));
    let mine = || match all.message(&h.question) {
        Ok(m) => match m.to.iter().find(|r| r.manager == h.to) {
            Some(r) => all.appointed(&r.workspace, &r.manager, r.since),
            None => m.manager == h.to && all.appointed(&m.workspace, &m.manager, m.since),
        },
        Err(_) => false,
    };
    if !appointed || message && !mine() {
        let words = if message { "the message; write to the manager again" } else { "it" };
        return Err((ReasonCode::SeatChanged, format!("the seat was given anew before {} could be given {words}", h.name)));
    }
    match h.kind {
        For::Discussion => match all.question(&h.question) {
            Ok(q) if q.open() => super::discussions::over(all, q).map_or(Ok(()), |why| Err((ReasonCode::GateSubmitted, why))),
            _ => Err((ReasonCode::GateSubmitted, "the discussion's question is closed".into())),
        },
        For::Note => match note_task(all, h).filter(|t| t.state == core::TaskState::Running && holds(all, live, t, &h.to)) {
            Some(_) => Ok(()),
            None => Err((ReasonCode::RuntimeError, format!("{} is not started and with {} any more", h.question, h.name))),
        },
        _ => Ok(()),
    }
}

/// A note of the receiver's own blind task: it hears of it, blind as it is.
fn own_blind(all: &core::State, h: &Held, live: &[RuntimeAgent]) -> bool {
    note_task(all, h).is_some_and(|t| t.blind && holds(all, Some(live), t, &h.to))
}
