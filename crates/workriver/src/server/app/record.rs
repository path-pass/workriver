// Recording an event as whoever the service found the caller to be, and what follows a
// record once it is committed: who is told.
use super::*;

/// Said to an agent whose start names a workspace it does not run in.
const START_IN_YOUR_OWN: &str = "an agent starts intents in the workspace it runs in: run `workriver intent start <i>` from your own terminal";

impl App {
    /// Records the name a live agent goes by, the first time and when it changes.
    pub(in crate::server) fn seen(&self, a: &RuntimeAgent) -> Result<(), ApiError> {
        let known = self.store.lock().unwrap().agent_name(&a.session)?.as_deref() == Some(a.name.as_str());
        if !known {
            self.record(&Principal::Service, Body::AgentSeen { session: a.session.clone(), name: a.name.clone(), harness: a.harness.clone() })?;
        }
        Ok(())
    }

    /// The workspace the runtime has an agent in, for an event of its own that names one: the
    /// event must name that one (by id and machine), and it is recorded as the runtime has it.
    /// `mine`: the group of the place the caller was found at, as its machine's runtime gave
    /// it then (`App::who_in`), never as a regular read of the agents last had it. What a
    /// workspace is called is the runtime's to say, since a label is what finds a
    /// workspace's record (and a link), so the one a request gives is never recorded nor
    /// checked against. A caller in no workspace, or naming another, is refused with `why`.
    fn callers_workspace(mine: Option<&core::RuntimeRef>, named: &core::RuntimeRef, why: &str) -> Result<core::RuntimeRef, ApiError> {
        mine.filter(|m| m.id == named.id && m.machine == named.machine).cloned().ok_or_else(|| ApiError(StatusCode::FORBIDDEN, why.into()))
    }

    /// Refuses a runtime group whose record is another open workspace's. A group is matched
    /// by its label only to stand for the same workspace under a new id (the runtime
    /// restarted); a record whose own id the runtime still lists is another open workspace
    /// that has the same label, and recording this group would give it that one's record:
    /// its manager, its intents, its project. The rules cannot tell the two apart (they know
    /// no runtime), so this is checked here, for every event that names a group, whoever
    /// sends it. A group of another machine matched through a link is not checked: the link
    /// is the user's, made by label. `open`: the groups of `g`'s machine as its runtime gave
    /// them for this request (`Runtime::groups_on`), or why it could not: then this refuses,
    /// since a list that was not read does not say that the record's workspace closed.
    fn not_another_open(record: Option<&core::Workspace>, g: &core::RuntimeRef, open: &Result<Vec<core::RuntimeRef>, RuntimeError>) -> Result<(), ApiError> {
        let Some(w) = record.filter(|w| w.group.id != g.id && w.group.machine == g.machine) else { return Ok(()) };
        let groups = open.as_ref().map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
        if groups.iter().any(|x| x.id == w.group.id && x.machine == w.group.machine) {
            return Err(ApiError(
                StatusCode::CONFLICT,
                format!(
                    "workspace {} is already recorded for another open workspace ({}); two open workspaces have one name, and workriver tells workspaces apart by name: ask the user to rename one of them",
                    g.label, w.group.id
                ),
            ));
        }
        Ok(())
    }

    /// One workspace, one manager: an agent manages the workspace it runs in (not the one its
    /// CLI names), and takes it over only from a manager no longer running. Checked on
    /// `state`, what is recorded as the registration is: `g` is the workspace it sets the
    /// manager of, `live` the agents running.
    fn may_manage(state: &core::State, session: &str, g: &core::RuntimeRef, live: &[RuntimeAgent]) -> Result<(), ApiError> {
        let current = state.workspace_of(g).and_then(|w| w.manager.as_deref()).filter(|m| *m != session);
        if let Some(other) = current.and_then(|m| live.iter().find(|a| a.session == m)) {
            return Err(ApiError(
                StatusCode::CONFLICT,
                format!("workspace {} has a manager running: {}. A workspace has one manager; tell the user, who closes that agent first if you are to take over", g.label, other.name),
            ));
        }
        Ok(())
    }

    /// Records an event as `who`, found by `App::who`, never taken from the caller's word.
    /// An agent's own registration or start goes through `record_in`, with its workspace.
    pub(in crate::server) fn record(&self, who: &Principal, body: Body) -> Result<core::Event, ApiError> {
        self.record_in(who, None, body)
    }

    /// `record`, for a request whose caller was found in workspace `mine` (`App::who_in`).
    ///
    /// Who decides where an intent runs (docs/security.md) is held here in two steps. First
    /// what the runtime says is read, with nothing locked: the workspace of an agent's own
    /// registration or start is set to the one it runs in, and the groups open on the named
    /// group's machine are asked of that machine now. Then, with the store locked, the group
    /// is checked against what is recorded and the event is recorded: one step, so nothing
    /// recorded in between (another workspace of the same name, the intent moved) is missed.
    pub(in crate::server) fn record_in(&self, who: &Principal, mine: Option<&core::RuntimeRef>, mut body: Body) -> Result<core::Event, ApiError> {
        // A gate and what follows it (its hook fails the discussions it ends) in order with a
        // discussion's last check and send (`App::hand`): no question it ends is typed in after.
        let _order = matches!(body, Body::GateSubmitted { .. }).then(|| self.discussing.lock().unwrap());
        let (actor, machine) = (who.actor(), who.machine());
        let cannot = |e: RuntimeError| ApiError(StatusCode::SERVICE_UNAVAILABLE, e.to_string());
        let own = match (&body, who) {
            (Body::ManagerRegistered { .. } | Body::IntentStateChanged { state: core::IntentState::Active, workspace: Some(_), .. }, Principal::Agent { session, machine }) => Some((session, machine)),
            _ => None,
        };
        // An agent named with `--as` (tests and development, where the service was started to
        // allow it) was found at no place: its workspace is where the runtime lists it now.
        let listed = match own {
            Some((session, machine)) if mine.is_none() && self.allow_as => self.runtime.agents().map_err(cannot)?.into_iter().find(|a| &a.session == session && &a.machine == machine).and_then(|a| a.group),
            _ => None,
        };
        let mine = mine.or(listed.as_ref());
        // The agents running, for a registration: a manager that still runs keeps its seat.
        let mut live = None;
        match &mut body {
            Body::ManagerRegistered { workspace, .. } if own.is_some() => {
                if let Some(g) = workspace {
                    *g = Self::callers_workspace(mine, g, words::MANAGE_YOUR_OWN)?;
                }
                live = Some(self.runtime.agents().map_err(cannot)?);
            }
            // An agent starts intents in the workspace the runtime has it in, recorded as the
            // runtime has it: a start with its own id and another label would relabel its record.
            Body::IntentStateChanged { workspace: Some(g), .. } if own.is_some() => *g = Self::callers_workspace(mine, g, START_IN_YOUR_OWN)?,
            _ => {}
        }
        // The group the event names to record, with the groups open on its machine now. A
        // registration that names only an intent: the caller's own, which the intent's
        // workspace must be (checked below, on what is recorded then).
        let named = match &body {
            Body::IntentStateChanged { state: core::IntentState::Active, workspace: g, .. } | Body::IntentCreated { runtime_ref: g, .. } => g.clone(),
            Body::IntentLinked { runtime_ref: g, .. } | Body::WorkspaceProjectSet { workspace: g, .. } => Some(g.clone()),
            Body::ManagerRegistered { workspace, .. } if own.is_some() => workspace.clone().or_else(|| mine.cloned()),
            _ => None,
        };
        let named = named.map(|g| {
            let open = self.runtime.groups_on(g.machine.as_deref());
            (g, open)
        });
        let body = self.complete(actor, body, machine)?;
        let committed = {
            let mut store = self.store.lock().unwrap();
            match (&body, &live, &named) {
                (Body::ManagerRegistered { intent, workspace, .. }, Some(live), _) => {
                    let state = store.recorded()?;
                    // Before 0.2 a manager named its intent; it manages that intent's workspace,
                    // which is the record this then writes: it must be the caller's own.
                    let theirs = || intent.as_deref().and_then(|i| state.intents.iter().find(|x| x.id == i)?.workspace.clone());
                    if let Some(g) = workspace.clone().or_else(theirs) {
                        Self::callers_workspace(mine, &g, words::MANAGE_YOUR_OWN)?;
                        // The caller's own machine's groups were asked: there is no other case.
                        let (_, open) = named.as_ref().ok_or_else(|| ApiError(StatusCode::FORBIDDEN, words::MANAGE_YOUR_OWN.into()))?;
                        Self::not_another_open(state.workspace_of(&g), &g, open)?;
                        Self::may_manage(&state, actor, &g, live)?;
                    }
                }
                (_, _, Some((g, open))) => Self::not_another_open(store.workspace_in(g)?.as_ref(), g, open)?,
                _ => {}
            }
            store.record(who, body)?
        };
        let e = committed.event;
        // The user changed an active intent: its manager hears of it as committed (its version
        // then), now or once its blind task is delivered (`tell`).
        if let Some((to, message)) = delivery::edited(&committed.recorded, &e)
            && let Ok(Err(err)) = self.tell(&to, &message, None)
        {
            tracing::warn!("cannot tell {to} of the edit of {}: {err}", e.body.refs().0.unwrap_or_default());
        }
        self.deliver(&e);
        match &e.body {
            Body::ManagerRegistered { agent, .. } => self.tell_of_update(agent, true),
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

    /// What only git can check or fill in before an event is recorded.
    fn complete(&self, actor: &str, body: Body, machine: Option<&str>) -> Result<Body, ApiError> {
        Ok(match body {
            // ponytail: a project is a folder of this machine; a repository on another
            // machine is a project of its own. Give projects a machine when that is not enough.
            Body::ProjectAdded { .. } if machine.is_some() => return Err(bad("register project folders on the machine workriver runs on; a repository on another machine is a project of its own".into())),
            Body::ProjectAdded { id, path, title } => {
                let folder = std::fs::canonicalize(&path).map_err(|e| bad(format!("{path}: {e}")))?;
                let title = if title.trim().is_empty() { folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() } else { title };
                Body::ProjectAdded { id, path: folder.to_string_lossy().into_owned(), title }
            }
            Body::FindingAdded { project, intent, finding, source, note } => Body::FindingAdded { project: self.project_folder(&project, machine)?, intent, finding, source, note },
            Body::IntentCreated { id, title, goal, runtime_ref, project, state, note, external, kind, size, tags } => {
                let project = if project.is_empty() { project } else { self.project_folder(&project, machine)? };
                Body::IntentCreated { id, title, goal, runtime_ref, project, state, note, external, kind, size, tags }
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
            Body::TaskStarted { unit, task, agent } => Body::TaskStarted { unit, task, agent: agent.map(|a| self.session_of(&a, Some(actor))) },
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
