// Recording an event as whoever the service found the caller to be, and what follows a
// record once it is committed: who is told.
use super::*;

impl App {
    /// Records the name a live agent goes by, the first time and when it changes; with the
    /// working directory it was first seen in (a harness may keep the session by it, so a
    /// later one does not replace it) and its machine.
    pub(in crate::server) fn seen(&self, a: &RuntimeAgent) -> Result<(), ApiError> {
        let was = self.store.lock().unwrap().agent(&a.session)?;
        let cwd = was.as_ref().map(|w| w.cwd.clone()).filter(|c| !c.is_empty()).unwrap_or_else(|| a.cwd.clone());
        let known = was.is_some_and(|w| w.name == a.name && w.cwd == cwd && w.machine == a.machine);
        if !known {
            self.record(&Principal::Service, Body::AgentSeen { session: a.session.clone(), name: a.name.clone(), harness: a.harness.clone(), cwd, machine: a.machine.clone() })?;
        }
        Ok(())
    }

    /// Records an event as `who`, found by `App::who`, never taken from the caller's word.
    ///
    /// Who decides where an intent runs and who manages (docs/security.md) is the rules' to
    /// check, against the store, when the event is recorded (`State::prepare`): a workspace an
    /// event names is one of workriver's own, found by its id; nothing the runtime says of
    /// its groups is read for it. What only the service can read is filled in first
    /// (`complete`, and for a seating the root of the session being seated, `seating`).
    pub(in crate::server) fn record(&self, who: &Principal, mut body: Body) -> Result<core::Event, ApiError> {
        // A gate and what follows it (its hook fails the discussions it ends) in order with a
        // discussion's last check and send (`App::hand`): no question it ends is typed in after.
        let _order = matches!(body, Body::GateSubmitted { .. }).then(|| self.discussing.lock().unwrap());
        let (actor, machine) = (who.actor(), who.machine());
        // The user seats a session: where its machine's runtime shows it now is read here,
        // and it is pinned there once the seating is recorded. Anyone else's is the rules'
        // to refuse, with nothing read or pinned for it.
        let mut pin = None;
        if let (Body::WorkspaceManagerSet { manager: Some(agent), machine, folder, .. }, Principal::User) = (&mut body, who) {
            let seat = self.seating(agent)?;
            (*agent, *machine, *folder) = (seat.session.clone(), seat.machine.clone(), seat.folder.clone());
            pin = Some(seat);
        }
        let before = match &body {
            Body::WorkspaceManagerSet { workspace, .. } | Body::WorkspaceClosed { workspace } => self.store.lock().unwrap().workspace(workspace).ok().and_then(|w| w.manager),
            _ => None,
        };
        let body = self.complete(actor, body, machine)?;
        let committed = {
            // A seat change in order with a message's last check and send (`App::hand`): no
            // message of the appointment it ends is typed in after it is committed.
            let _order = matches!(body, Body::WorkspaceManagerSet { .. } | Body::WorkspaceClosed { .. }).then(|| self.appointing.lock().unwrap());
            let mut store = self.store.lock().unwrap();
            let committed = store.record(who, body)?;
            // A project comes with its first workspace, named as it: under the same lock.
            if let Body::ProjectAdded { id, title, .. } = &committed.event.body
                && let Err(e) = store.record(who, Body::WorkspaceCreated { id: String::new(), name: title.clone(), project: id.clone() })
            {
                tracing::warn!("cannot record the first workspace of project {id}: {e}");
            }
            committed
        };
        let e = committed.event;
        if let Some(seat) = pin {
            self.seated(seat);
        }
        // The user changed an active intent: its manager hears of it as committed (its version
        // then), now or once its blind task is delivered (`tell`).
        if let Some((to, message)) = delivery::edited(&committed.recorded, &e)
            && let Ok(Err(err)) = self.tell(&to, &message, None)
        {
            tracing::warn!("cannot tell {to} of the edit of {}: {err}", e.body.refs().0.unwrap_or_default());
        }
        self.deliver(&e);
        match &e.body {
            Body::WorkspaceManagerSet { workspace, .. } | Body::WorkspaceClosed { workspace } => self.tell_of_seat(&committed.recorded, workspace, before.as_ref()),
            Body::QuestionAnswered { question, .. } => {
                self.answered(question);
                self.tell_of_discussion(&e);
            }
            Body::QuestionAsked { id, discuss: true, .. } => self.discuss(id),
            Body::GateSubmitted { unit, task, .. } => self.gate_submitted(unit, *task),
            Body::TaskAmended { .. } => self.tell_of_discussion(&e),
            Body::ThreadEnded { thread } => self.end_thread(thread),
            Body::MessageSent { id, .. } => self.tell_manager(id),
            _ => {}
        }
        // A unit's branch may have got a work tree of its own since; watching again is cheap.
        // One on another machine is read again in the background.
        if let (_, Some(unit), _) = e.body.refs() {
            let found = got(self.store.lock().unwrap().unit(&unit)).ok().flatten().filter(|u| u.has_code()).map(|u| (u.machine.is_some(), u.repo.clone(), u.branch.clone()));
            match found {
                Some((false, repo, branch)) => self.watch(Path::new(&repo), &branch),
                Some((true, ..)) => {
                    let _ = self.remote_kick.send(());
                }
                None => {}
            }
        }
        let _ = self.changed.send(());
        Ok(e)
    }

    /// A seat changed (seated, emptied, its workspace closed), as recorded: the session now
    /// seated is told that the user made it the manager, with what it manages and where the
    /// role is written; the one it replaced, that it no longer manages. A send that fails is
    /// logged: the seat stands either way, and the page shows who holds it.
    fn tell_of_seat(&self, all: &core::State, workspace: &str, before: Option<&core::Seat>) {
        let Ok(w) = all.workspace(workspace) else { return };
        let now = w.manager.as_ref();
        if let Some(old) = before.filter(|old| now.is_none_or(|m| m.session != old.session)) {
            let text = delivery::unseated(all, w);
            if let Err(e) = self.send_to(&old.session, old.machine.as_deref(), &text) {
                tracing::info!("cannot tell {} that it no longer manages {workspace}: {e}", old.session);
            }
        }
        // The same appointment confirmed again says nothing new.
        if let Some(m) = now.filter(|m| before.is_none_or(|old| old.since != m.since)) {
            self.tell_of_update(&m.session, true);
            let text = delivery::seated(all, w);
            if let Ok(Err(e)) | Err(e) = self.tell(&m.session, &text, None).map(|sent| sent.map_err(|e| e.to_string())) {
                tracing::info!("cannot tell {} that it manages {workspace}: {e}", m.session);
            }
        }
    }

    /// What only git can check or fill in before an event is recorded.
    fn complete(&self, actor: &str, body: Body, machine: Option<&str>) -> Result<Body, ApiError> {
        Ok(match body {
            // ponytail: a project is a folder of this machine. Give projects a machine when
            // that is not enough.
            Body::ProjectAdded { .. } if machine.is_some() => return Err(bad("register project folders on the machine workriver runs on".into())),
            Body::ProjectAdded { id, path, title } => {
                let folder = std::fs::canonicalize(&path).map_err(|e| bad(format!("{path}: {e}")))?;
                let title = if title.trim().is_empty() { folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() } else { title };
                Body::ProjectAdded { id, path: folder.to_string_lossy().into_owned(), title }
            }
            Body::FindingAdded { project, intent, finding, source, note } => Body::FindingAdded { project: self.project_folder(&project, machine)?, intent, finding, source, note },
            Body::IntentCreated { id, title, goal, project, state, note, external, kind, size, tags } => {
                let project = if project.is_empty() { project } else { self.project_folder(&project, machine)? };
                Body::IntentCreated { id, title, goal, project, state, note, external, kind, size, tags }
            }
            // The user's start that says no workspace: the one open workspace of the intent's
            // project, when there is exactly one; else the rules ask for it.
            Body::IntentStateChanged { intent, state: core::IntentState::Active, reason, workspace: None } if actor == "user" => {
                let all = self.store.lock().unwrap().recorded()?;
                let project = all.intents.iter().find(|i| i.id == intent).map(|i| all.intent_project(i)).filter(|p| !p.is_empty());
                let open: Vec<&core::Workspace> = all.workspaces.iter().filter(|w| !w.closed && all.workspace_project(w).is_some_and(|p| Some(&p.path) == project.as_ref())).collect();
                let workspace = match open[..] {
                    [one] => Some(one.id.clone()),
                    _ => None,
                };
                Body::IntentStateChanged { intent, state: core::IntentState::Active, reason, workspace }
            }
            // The repository is where the work happens: on the machine named, else on the
            // one the unit was recorded from. It is checked there, over ssh for another machine.
            // Research has no repository: nothing to check.
            b @ Body::UnitCreated { kind: core::UnitKind::Research, .. } => b,
            Body::UnitCreated { id, intent, title, purpose, repo, branch, base, machine: named, kind } => {
                let machine = place(named.as_deref(), machine);
                let top = self.repository(machine.as_deref(), &repo, &base)?;
                Body::UnitCreated { id, intent, title, purpose, repo: top, branch, base, machine, kind }
            }
            // What is not given stays as it is; `machine` is always said (the CLI sends it).
            Body::UnitRelocated { unit, machine: named, repo, branch, base, .. } => {
                let u = self.store.lock().unwrap().unit(&unit)?;
                let machine = match named.as_deref() {
                    None => u.machine.clone(),
                    named => place(named, None),
                };
                let (repo, branch, base) = (if repo.is_empty() { u.repo.clone() } else { repo }, if branch.is_empty() { u.branch.clone() } else { branch }, if base.is_empty() { u.base.clone() } else { base });
                let top = self.repository(machine.as_deref(), &repo, &base)?;
                // What is on the place it leaves is read once, now: the old branch is not
                // read again, and what only it had would otherwise vanish without a word.
                // Their patch ids are taken too, all at once, so that their rebased copies are
                // told without reading the old branch again.
                let previous = match self.runner(u.machine.as_deref()) {
                    Ok(at) if (&u.machine, &u.repo, &u.branch, &u.base) != (&machine, &top, &branch, &base) => {
                        let list = views::unit_commits(&at, &u, &mut self.cache.lock().unwrap()).map(|(list, _)| list).unwrap_or_default();
                        let ids = scan::patch_ids(&at, Path::new(&u.repo), &list.iter().map(|c| c.hash.clone()).collect::<Vec<_>>()).unwrap_or_default();
                        list.into_iter()
                            .map(|c| {
                                let linked = u.links.iter().find(|l| l.hash == c.hash).map(|l| l.task);
                                let named = c.task.as_deref().and_then(core::parse_task_ref).filter(|(x, _)| *x == u.id).map(|(_, n)| n);
                                core::SeenCommit { task: linked.or(named), patch_id: ids.get(&c.hash).cloned().unwrap_or_default(), hash: c.hash, subject: c.subject, date: c.date }
                            })
                            .collect()
                    }
                    _ => vec![],
                };
                Body::UnitRelocated { unit, machine, repo: top, branch, base, previous }
            }
            // The commit is looked up where the unit is: it must be one of the unit's, and
            // what a rebased copy of it will be known by is taken down.
            Body::CommitLinked { unit, hash, task, .. } => {
                let u = self.store.lock().unwrap().unit(&unit)?;
                let at = self.runner(u.machine.as_deref())?;
                let repo = Path::new(&u.repo);
                let full = scan::resolve(&at, repo, &hash).map_err(|_| bad(format!("no commit {hash} in {}", u.repo)))?;
                let named = views::unit_commits(&at, &u, &mut self.cache.lock().unwrap()).map_err(|e| bad(e.to_string()))?.0;
                let Some(c) = named.into_iter().find(|c| c.hash == full) else {
                    return Err(bad(format!("commit {hash} is not one of unit {unit}'s: not on branch {} since {}", u.branch, u.base)));
                };
                let patch_id = scan::patch_id(&at, repo, &full).unwrap_or_default();
                Body::CommitLinked { unit, hash: full, patch_id, subject: c.subject, date: c.date, task }
            }
            // A chore of no intent belongs to a project, if one is given or can be told.
            Body::ChoreCreated { id, intent, project, unit, title, steps } if intent.is_empty() && !project.is_empty() => {
                let project = self.project_folder(&project, machine).unwrap_or_default();
                Body::ChoreCreated { id, intent, project, unit, title, steps }
            }
            Body::UnitMerged { unit, commits } if commits.is_empty() => {
                // The branch's commits now, or as last seen if they are already merged away.
                let u = self.store.lock().unwrap().unit(&unit)?;
                let at = self.runner(u.machine.as_deref())?;
                let commits = views::unit_commits(&at, &u, &mut self.cache.lock().unwrap()).map_err(|e| bad(e.to_string()))?.0.into_iter().map(|c| c.hash).collect();
                Body::UnitMerged { unit, commits }
            }
            // An agent given by its name is recorded by its session.
            Body::TaskCreated(mut t) => {
                t.agent = t.agent.map(|a| self.session_of(&a, Some(actor)));
                Body::TaskCreated(t)
            }
            Body::TaskStarted { unit, task, agent, model } => Body::TaskStarted { unit, task, agent: agent.map(|a| self.session_of(&a, Some(actor))), model },
            other => other,
        })
    }

    /// Sends the message a user's input calls for, and records how far it got.
    fn deliver(&self, e: &core::Event) {
        // An intent's edit is told by `record`, from the state it made.
        if matches!(e.body, Body::IntentUpdated { .. }) {
            return;
        }
        let live = self.live();
        // An addition to a finding goes to several at once; it is on the page either way, so
        // what does not arrive is only logged.
        // ponytail: these are not followed up as "delivered / picked up" like decisions are.
        // A dropped task likewise: its agent (to stop) and the manager.
        // Work over: the manager hears which of its workers it may close.
        // A requirement added to a task: `noted`, which says how far it got.
        if matches!(e.body, Body::FindingNoted { .. } | Body::TaskDropped { .. } | Body::UnitMerged { .. } | Body::UnitDone { .. } | Body::IntentStateChanged { .. } | Body::ManagerTold { .. }) {
            let Ok(Some(all)) = got(self.store.lock().unwrap().recorded()) else { return };
            let to = {
                match e.body {
                    Body::FindingNoted { .. } => delivery::notes(&all, e, &self.home),
                    Body::TaskDropped { .. } => delivery::dropped(&all, live.as_deref(), e),
                    Body::ManagerTold { .. } => delivery::told(&all, e),
                    _ => delivery::finished(&all, live.as_deref(), e),
                }
            };
            drop(all);
            // What the receiver must act on: held while it is on a blind task, waits for a
            // person, is not listed or cannot be sent to; given once it can take it.
            for (agent, text) in to {
                self.hand(Held { question: String::new(), kind: For::Duty, to: agent.clone(), name: agent, text, not_ready: None, about: None });
            }
            return;
        }
        let Ok(Some(all)) = got(self.store.lock().unwrap().recorded()) else { return };
        let Some(plan) = delivery::plan(&all, live.as_deref(), &self.home, e) else { return };
        drop(all);
        match plan.to.clone() {
            Err(_) => self.planned(plan, Ok(())),
            // Given now, or held while the receiver is on a blind task in progress: recorded
            // where it is once given (`told`).
            Ok(to) => {
                let message = plan.message.clone();
                let _ = self.tell(&to, &message, Some(plan));
            }
        }
    }

    /// Tells a task's agent of a requirement added to it (`task.noted`, recorded), and says how
    /// far that got, in the service's words: the note stands either way. One that waits (its
    /// agent is on a blind task) is kept, and looked at again at each poll (`give_notes`).
    /// An agent not seen running may have started on another machine since that machine was
    /// last read: reading the machines again can take as long as the slowest of them, so it
    /// is not done while the request waits. The answer says it is not told yet, and the
    /// search goes on from a thread of its own (`find_noted`).
    pub(in crate::server) fn noted(self: &Arc<Self>, e: &core::Event) -> String {
        match self.give_note(e, None) {
            Note::Told(name) => format!("{name} was told in its terminal"),
            Note::Not(why) => format!("its agent was not told: {why}"),
            Note::Unseen(why) => {
                let (app, e) = (self.clone(), e.clone());
                std::thread::spawn(move || app.find_noted(&e));
                format!("not told yet: {why}: it is told once it is found on a machine workriver follows")
            }
            Note::Waits(to, why) => {
                // Unreadable records, or a runtime that does not answer: nobody to keep it for.
                match to {
                    Some(to) => self.notes.lock().unwrap().push((e.clone(), to)),
                    None => return format!("its agent was not told: {why}"),
                }
                format!("not told yet: {why}")
            }
        }
    }

    /// Looks for the agent of a task a requirement was added to on the machines followed,
    /// read again now, and tells it: tried after each machine was read, by this thread
    /// alone, so it is told once, and as soon as its own machine answered. What it comes to
    /// is logged: the command was answered long ago.
    fn find_noted(&self, e: &core::Event) {
        let Some(machines) = &self.machines else { return };
        let (_, unit, task) = e.body.refs();
        let about = task.or(unit).unwrap_or_default();
        let mut unseen = true;
        machines.refresh(|| {
            match self.give_note(e, None) {
                Note::Unseen(_) => return false,
                Note::Told(_) => {}
                Note::Waits(Some(to), _) => self.notes.lock().unwrap().push((e.clone(), to)),
                Note::Waits(None, why) | Note::Not(why) => tracing::info!("a requirement added to {about} is not told to its agent: {why}"),
            }
            unseen = false;
            true
        });
        if unseen {
            tracing::info!("a requirement added to {about} is not told to its agent: it runs on no machine workriver follows");
        }
    }

    /// The requirements whose agent was not told yet, looked at again (the poll): given,
    /// kept, or given up for good, which is only logged (it is on the task either way).
    pub(in crate::server) fn give_notes(&self) {
        let waiting = std::mem::take(&mut *self.notes.lock().unwrap());
        for (e, to) in waiting {
            match self.give_note(&e, Some(&to)) {
                Note::Told(_) => {}
                Note::Waits(..) => self.notes.lock().unwrap().push((e, to)),
                // Seen before, since it was kept for it: gone now.
                Note::Unseen(why) | Note::Not(why) => {
                    let (_, unit, task) = e.body.refs();
                    tracing::info!("a requirement added to {} is not told to its agent after all: {why}", task.or(unit).unwrap_or_default());
                }
            }
        }
    }

    /// One try at telling a task's agent of a requirement added to it, worked out from what
    /// is recorded and who runs now, each time: the task must still be started and with its
    /// agent, which is found by its session in the runtime now, not in the last poll (it may
    /// have started since). One not found while machines are followed may run on one of
    /// them, not read since it started: `Unseen`, for the caller to read them again or not.
    /// `held`: the agent it was kept for; a task that is with another one by now is that
    /// one's to read, not told here. Its own blind task an agent hears of at once; of
    /// anything else once it is on no blind task. The message goes to that session on that
    /// machine alone, with files as its machine can get them.
    fn give_note(&self, e: &core::Event, held: Option<&delivery::Receiver>) -> Note {
        // What cannot be looked at now: a note already kept stays so; a new one is not told.
        let unknown = |why: String| Note::Waits(held.cloned(), why);
        let read = got(self.store.lock().unwrap().recorded());
        let Ok(Some(all)) = read else { return unknown("the records cannot be read now".into()) };
        let live = match self.runtime.agents() {
            Ok(live) => live,
            Err(err) => return unknown(err.to_string()),
        };
        let n = match delivery::task_noted(&all, &live, &self.home, e) {
            Ok(n) => n,
            Err(no) => match no.gone.filter(|_| self.machines.as_ref().is_some_and(|m| m.any_up())) {
                Some(name) => return Note::Unseen(format!("{name} is not seen running")),
                None => return Note::Not(no.why),
            },
        };
        if held.is_some_and(|to| to != &n.to) {
            return Note::Not(format!("the task is with {} now", n.name));
        }
        if !n.own_blind && delivery::blind(&all, &live, &n.to.session) {
            return Note::Waits(Some(n.to), format!("{} is on a blind task: it is told once that task is delivered", n.name));
        }
        drop(all);
        match self.send_to(&n.to.session, n.to.machine.as_deref(), &n.message) {
            Ok(()) => Note::Told(n.name),
            // Waiting for a person in its terminal: one that is kept is tried again.
            Err(RuntimeError::Blocked(why) | RuntimeError::NotReady(why)) if held.is_some() => Note::Waits(Some(n.to), why),
            Err(err) => {
                tracing::warn!("cannot tell {} of a requirement added: {err}", n.name);
                Note::Not(format!("cannot reach {}: {err}", n.name))
            }
        }
    }

    /// Tells an agent something workriver has to say (an answer it waits for, a delivery's
    /// message, an intent started in its workspace, an escalation): now, unless it is on a
    /// blind task in progress, which is told nothing of other work; then it is held and given
    /// once that task is delivered (`poll`, `hand`), and this gives `Err` with why, in the
    /// service's words. An unreadable store holds it too. The one rule for every such send.
    /// `about`: a planned delivery, recorded where it is: held (with why) now, and delivered
    /// or not once given (`told`), which is then what it came to (here `Ok(Ok(()))`).
    pub(in crate::server) fn tell(&self, to: &str, text: &str, about: Option<delivery::Plan>) -> Result<Result<(), RuntimeError>, String> {
        let held = Held { question: String::new(), kind: For::Notice, to: to.into(), name: to.into(), text: text.into(), not_ready: None, about };
        if let Some(why) = self.held_because(to) {
            // The page shows it held; a restart before it is given marks it not delivered (`recover`).
            if let Some(plan) = &held.about {
                let delivery = Delivery { step: DeliveryStep::Held, to: Some(to.into()), reason_code: None, reason: Some(why.clone()), message: plan.message.clone() };
                self.record_delivery(plan, delivery);
            }
            self.held.lock().unwrap().push(held);
            return Err(why);
        }
        let sent = self.send(to, text);
        if held.about.is_none() {
            return Ok(sent);
        }
        self.told(held, sent);
        Ok(Ok(()))
    }

    /// A notice was given (or could not be): a planned delivery is recorded where it is;
    /// anything else that failed is logged.
    pub(in crate::server) fn told(&self, h: Held, sent: Result<(), RuntimeError>) {
        match h.about {
            Some(plan) => self.planned(plan, sent),
            None => {
                if let Err(e) = sent {
                    tracing::warn!("cannot tell {}: {e}", h.to);
                }
            }
        }
    }

    /// Records where a planned delivery is: not delivered when it had nowhere to go (`sent`
    /// is then not looked at) or its send failed; else delivered, and watched for its pick-up.
    fn planned(&self, plan: delivery::Plan, sent: Result<(), RuntimeError>) {
        let message = plan.message.clone();
        let delivery = match (&plan.to, sent) {
            (Err((code, reason)), _) => Delivery { step: DeliveryStep::NotDelivered, to: None, reason_code: Some(*code), reason: Some(reason.clone()), message },
            (Ok(to), Ok(())) => Delivery { step: DeliveryStep::Delivered, to: Some(to.clone()), reason_code: None, reason: None, message },
            (Ok(to), Err(err)) => {
                let code = match err {
                    RuntimeError::None => ReasonCode::RuntimeDown,
                    RuntimeError::NoAgent(_) => ReasonCode::ManagerGone,
                    _ => ReasonCode::RuntimeError,
                };
                Delivery { step: DeliveryStep::NotDelivered, to: Some(to.clone()), reason_code: Some(code), reason: Some(err.to_string()), message }
            }
        };
        let live = self.live();
        if delivery.step == DeliveryStep::Delivered {
            let working = live.iter().flatten().any(|a| Some(&a.session) == delivery.to.as_ref() && a.state == AgentState::Working);
            self.pending.lock().unwrap().push(Pending {
                unit: plan.unit.clone(),
                task: plan.task,
                finding: plan.finding.clone(),
                chore: plan.chore.clone(),
                delivery: delivery.clone(),
                armed: !working,
                sent: std::time::Instant::now(),
            });
        }
        self.record_delivery(&plan, delivery);
    }

    /// Records where a planned delivery is.
    pub(in crate::server) fn record_delivery(&self, plan: &delivery::Plan, delivery: Delivery) {
        let body = Body::DeliveryUpdated { unit: plan.unit.clone(), task: plan.task, finding: plan.finding.clone(), chore: plan.chore.clone(), delivery };
        if let Err(e) = self.store.lock().unwrap().record(&Principal::Service, body) {
            tracing::warn!("cannot record a delivery: {e}");
        }
    }
}

/// How far telling a task's agent of an added requirement got: told (the name it goes by);
/// not yet, with why, and the agent it is kept for (none: nobody could be found to keep it
/// for); its agent not seen running, though it may be on a machine not read since it
/// started; or not, with why, for good.
enum Note {
    Told(String),
    Unseen(String),
    Waits(Option<delivery::Receiver>, String),
    Not(String),
}

/// The machine a unit is put on: the one named ("local": the one workriver runs on), else
/// the caller's.
fn place(named: Option<&str>, callers: Option<&str>) -> Option<String> {
    match named {
        Some("local") => None,
        Some(m) => Some(m.to_string()),
        None => callers.map(str::to_string),
    }
}
