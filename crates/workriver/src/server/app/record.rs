// Recording an event as whoever the service found the caller to be, and what follows a
// record once it is committed: who is told.
use super::*;

/// How long an assignment by name waits for the followed machines' agents read again.
const NAME_READ: Duration = Duration::from_secs(1);

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
    ///
    /// A seat is set by the user's seating, or by the end of the user's own start of a manager
    /// (`ManagerStartSeated`, recorded here as workriver's): the service reads where that
    /// session is as it does for the user's seating, and the rules accept it only for the
    /// workspace's pending start. Both are recorded in the same order against a message's
    /// send (`appointing`), the seat before read first, and told the same way.
    pub(in crate::server) fn record(&self, who: &Principal, mut body: Body) -> Result<core::Event, ApiError> {
        // A gate and what follows it (its hook fails the discussions it ends) in order with a
        // discussion's last check and send (`App::hand`): no question it ends is typed in after.
        // Held while what follows hands notices out, which take `appointing`: `discussing`
        // first, always (`App::discussing`).
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
        // What a start brought up, where its machine's runtime shows it now: the folder and the
        // root (its place and shell) the rules compare with the start's.
        if let (Body::ManagerStartSeated { session, harness, folder, place, shell, .. }, Principal::Service) = (&mut body, who) {
            let seat = self.seating(session)?;
            let Some((at, pid, start)) = seat.root() else { return Err(bad(format!("{session} is in a terminal whose shell is not known: it is not seated"))) };
            if seat.machine.is_some() {
                return Err(bad(format!("{session} runs on machine {}: a start seats only on this one", seat.machine.as_deref().unwrap_or_default())));
            }
            (*folder, *place, *shell) = (seat.folder.clone(), at, core::Shell { pid, start });
            if let Some(h) = seat.harness.clone() {
                *harness = h;
            }
            pin = Some(seat);
        }
        let body = self.complete(actor, body, machine)?;
        let (committed, before) = {
            // A seat change in order with a message's last check and send (`App::hand`): no
            // message of the appointment it ends is typed in after it is committed. It waits for
            // a send under way, which finds its place under the lock (over ssh for a seat on
            // another machine). Held for the commit only: nothing here takes `discussing` or a
            // writer under it.
            let _order = matches!(body, Body::WorkspaceManagerSet { .. } | Body::WorkspaceClosed { .. } | Body::ManagerStartSeated { .. }).then(|| self.appointing.lock().unwrap());
            let mut store = self.store.lock().unwrap();
            // The seat it changes, read in the same order and under the same lock: the one told
            // it no longer manages. Not read: nothing is recorded (no workspace: the rules say).
            let before = match &body {
                Body::WorkspaceManagerSet { workspace, .. } | Body::WorkspaceClosed { workspace } | Body::ManagerStartSeated { workspace, .. } => got(store.workspace(workspace)).map_err(|Unread(why)| ApiError(StatusCode::INTERNAL_SERVER_ERROR, why))?.and_then(|w| w.manager),
                _ => None,
            };
            let committed = store.record(who, body)?;
            // A project comes with its first workspace, named as it: under the same lock.
            if let Body::ProjectAdded { id, title, .. } = &committed.event.body
                && let Err(e) = store.record(who, Body::WorkspaceCreated { id: String::new(), name: title.clone(), project: id.clone() })
            {
                tracing::warn!("cannot record the first workspace of project {id}: {e}");
            }
            (committed, before)
        };
        let e = committed.event;
        if let Some(seat) = pin {
            self.seated(seat);
        }
        // The user changed an active intent: its manager hears of it as committed (its version
        // then), now or once it can take it (`hand`).
        if let Some((to, message)) = delivery::edited(&committed.recorded, &e) {
            self.hand(Held::notice(&committed.recorded, &to, message, None));
        }
        self.deliver(&e);
        match &e.body {
            Body::WorkspaceManagerSet { workspace, .. } | Body::WorkspaceClosed { workspace } => self.tell_of_seat(&committed.recorded, workspace, before.as_ref()),
            // Seated by a start: what the page proposed for the seat is moot.
            Body::ManagerStartSeated { workspace, .. } => {
                self.proposals.lock().unwrap().remove(workspace);
                self.tell_of_seat(&committed.recorded, workspace, before.as_ref());
            }
            Body::QuestionAnswered { question, .. } => {
                self.answered(question);
                self.tell_of_discussion(&e);
            }
            Body::QuestionAsked { id, discuss: true, .. } => self.discuss(id),
            // The open questions of the discussions it ends fail, so no answer or amendment of
            // a discussion comes after what the user confirmed.
            Body::GateSubmitted { .. } => {
                self.discussions_over();
            }
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
            if let Err(e) = self.type_at(&old.session, old.machine.as_deref(), &fresh_key(), &text) {
                tracing::info!("cannot tell {} that it no longer manages {workspace}: {e}", old.session);
            }
        }
        // The same appointment confirmed again says nothing new.
        if let Some(m) = now.filter(|m| before.is_none_or(|old| old.since != m.since)) {
            self.tell_of_update(&m.session, true);
            let text = delivery::seated(all, w);
            if let Outcome::Held(why) = self.hand(Held::notice(all, &m.session, text, None)) {
                tracing::info!("{} is told that it manages {workspace} once it can be: {why}", m.session);
            }
        }
    }

    /// What the service checks or fills in before an event is recorded.
    fn complete(&self, actor: &str, body: Body, machine: Option<&str>) -> Result<Body, ApiError> {
        Ok(match body {
            // ponytail: a project is a folder of this machine. Give projects a machine when
            // that is not enough.
            Body::ProjectAdded { .. } if machine.is_some() => return Err(bad("register project folders on the machine workriver runs on".into())),
            Body::ProjectAdded { id, path, title } => {
                // A whole path, from / or ~/ (the service's home): anything else would be read
                // against the folder the service was started in.
                let path = match path.strip_prefix("~/") {
                    Some(rest) => std::env::home_dir().ok_or_else(|| bad("the service's home folder is not known: give the folder's whole path, from /".into()))?.join(rest).to_string_lossy().into_owned(),
                    None if path.starts_with('/') => path,
                    None => return Err(bad(format!("{path:?}: give the folder's whole path, from / or ~/"))),
                };
                let folder = std::fs::canonicalize(&path).map_err(|e| bad(format!("{path}: {e}")))?;
                if !folder.is_dir() {
                    return Err(bad(format!("{path} is a file: a project is a folder")));
                }
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
            Body::UnitCreated { id, intent, title, purpose, repo, branch, base, machine: named, kind, template, revision, placed } => {
                let machine = place(named.as_deref(), machine);
                let top = self.repository(machine.as_deref(), &repo, &base)?;
                Body::UnitCreated { id, intent, title, purpose, repo: top, branch, base, machine, kind, template, revision, placed }
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
            // Find the registered project of a project chore.
            Body::ChoreCreated { id, intent, project, unit, title, steps } if intent.is_empty() && !project.is_empty() => {
                let project = self.project_folder(&project, machine)?;
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
                t.agent = t.agent.map(|a| self.task_session(&a)).transpose()?;
                Body::TaskCreated(t)
            }
            Body::TaskStarted { unit, task, agent, model } => Body::TaskStarted { unit, task, agent: agent.map(|a| self.task_session(&a)).transpose()?, model },
            // A placed task filled for an agent: resolved as `task add` resolves it.
            Body::TaskFilled { unit, task, title, text, agent, inputs, plans, steps, blind, choice } => {
                Body::TaskFilled { unit, task, title, text, agent: agent.map(|a| self.task_session(&a)).transpose()?, inputs, plans, steps, blind, choice }
            }
            other => other,
        })
    }

    /// Resolve a task's assignee from the runtime now: an exact session or a unique name
    /// (`named`). A session (live, or one the records know) is taken as it is, with no read
    /// waited for. A name is looked for after every followed machine was read
    /// again (`Machines::refresh`), for a second at most: a second agent of that name that
    /// appeared on one since its last read is seen. A machine not read now (its connection
    /// down, or no answer in that second) refuses the name, never resolved from its older
    /// list: the agent is then named by its session. With no machine followed nothing is read
    /// again and nothing waits.
    fn task_session(&self, agent: &str) -> Result<String, ApiError> {
        let mut live = self.runtime.agents().unwrap_or_default();
        if live.iter().any(|a| a.session == agent) {
            return Ok(agent.to_string());
        }
        let all = self.store.lock().unwrap().recorded()?;
        if delivery::known_session(&all, agent) {
            return Ok(agent.to_string());
        }
        if let Some(machines) = &self.machines {
            if let Some(m) = machines.refresh(NAME_READ).first() {
                return Err(ApiError(StatusCode::SERVICE_UNAVAILABLE, format!("machine {m} did not answer: name the agent by its session (`workriver agents` lists them)")));
            }
            live = self.runtime.agents().unwrap_or_default();
        }
        match named(&live, agent)[..] {
            [] => Ok(agent.to_string()),
            [one] => Ok(one.session.clone()),
            ref more => Err(several(agent, more)),
        }
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
            let held: Vec<Held> = to.into_iter().map(|(agent, text)| Held::notice(&all, &agent, text, None)).collect();
            drop(all);
            // Given now, or held until the receiver can take it (`hand`).
            for h in held {
                self.hand(h);
            }
            return;
        }
        let Ok(Some(all)) = got(self.store.lock().unwrap().recorded()) else { return };
        let Some(plan) = delivery::plan(&all, live.as_deref(), &self.home, e) else { return };
        match plan.to.clone() {
            Err(why) => self.planned(plan, Err(why)),
            // Given now, or held (recorded so, with why) until the receiver can take it.
            Ok(to) => {
                let h = Held::notice(&all, &to, plan.message.clone(), Some(plan));
                drop(all);
                self.hand(h);
            }
        }
    }

    /// Tells a task's agent of a requirement added to it (`task.noted`, recorded), and says how
    /// far that got, in the service's words: the note stands either way. One that cannot be
    /// given now (its agent waits for a person, is on a blind task or is not listed) is held
    /// and given at the first poll it can be (`hand`), while the task is still started and
    /// with that agent.
    pub(in crate::server) fn noted(&self, e: &core::Event) -> String {
        let read = got(self.store.lock().unwrap().recorded());
        let Ok(Some(all)) = read else { return "its agent was not told: the records cannot be read now".into() };
        // Read now: the agent may have started since the last poll. A read that fails finds
        // no name, and `hand` holds the note until a read works.
        let live = self.runtime.agents();
        let n = match delivery::task_noted(&all, live.as_deref().unwrap_or_default(), &self.home, e) {
            Ok(n) => n,
            Err(why) => return format!("its agent was not told: {why}"),
        };
        let task = e.body.refs().2.unwrap_or_default();
        let h = Held { name: n.name.clone(), ..Held::new(&all, For::Note, &task, &n.to, n.message, None) };
        drop(all);
        match self.hand(h) {
            Outcome::Given => format!("{} was told in its terminal", n.name),
            Outcome::Held(why) => format!("not told yet: {why}: it is told once it can be"),
            Outcome::Ended(why) => format!("its agent was not told: {why}"),
        }
    }

    /// Records where a planned delivery is: not delivered, with why (it had nowhere to go, or
    /// could not be given); else delivered, and watched for its pick-up.
    pub(in crate::server) fn planned(&self, plan: delivery::Plan, sent: Result<(), (ReasonCode, String)>) {
        let (to, message) = (plan.to.as_ref().ok().cloned(), plan.message.clone());
        let delivery = match plan.to.clone().and(sent) {
            Ok(()) => Delivery { step: DeliveryStep::Delivered, to, reason_code: None, reason: None, message },
            Err((code, reason)) => Delivery { step: DeliveryStep::NotDelivered, to, reason_code: Some(code), reason: Some(reason), message },
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

/// The machine a unit is put on: the one named ("local": the one workriver runs on), else
/// the caller's.
fn place(named: Option<&str>, callers: Option<&str>) -> Option<String> {
    match named {
        Some("local") => None,
        Some(m) => Some(m.to_string()),
        None => callers.map(str::to_string),
    }
}
