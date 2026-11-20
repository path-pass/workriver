// Recording an event as whoever the service found the caller to be, and what follows a
// record once it is committed: who is told.
use super::*;

impl App {
    /// Records the name a live agent goes by, the first time and when it changes.
    pub(in crate::server) fn seen(&self, a: &RuntimeAgent) -> Result<(), ApiError> {
        let known = self.store.lock().unwrap().agent_name(&a.session)?.as_deref() == Some(a.name.as_str());
        if !known {
            self.record(&Principal::Service, Body::AgentSeen { session: a.session.clone(), name: a.name.clone(), harness: a.harness.clone() })?;
        }
        Ok(())
    }

    /// One workspace, one manager: an agent manages the workspace it runs in (not the one its
    /// CLI names), and takes it over only from a manager no longer running.
    fn may_manage(&self, session: &str, machine: Option<&str>, intent: Option<&str>, workspace: Option<&core::RuntimeRef>) -> Result<(), ApiError> {
        let state = self.store.lock().unwrap().recorded()?;
        // Before 0.2 a manager named its intent; it manages that intent's workspace.
        let Some(g) = workspace.cloned().or_else(|| intent.and_then(|i| state.intents.iter().find(|x| x.id == i)?.workspace.clone())) else { return Ok(()) };
        let live = self.runtime.agents().map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
        let me = live.iter().find(|a| a.session == session && a.machine.as_deref() == machine);
        if !me.and_then(|a| a.group.as_ref()).is_some_and(|m| m.id == g.id && m.machine == g.machine) {
            return Err(ApiError(StatusCode::FORBIDDEN, words::MANAGE_YOUR_OWN.into()));
        }
        let current = state.workspace_of(&g).and_then(|w| w.manager.as_deref()).filter(|m| *m != session);
        if let Some(other) = current.and_then(|m| live.iter().find(|a| a.session == m)) {
            return Err(ApiError(
                StatusCode::CONFLICT,
                format!("workspace {} has a manager running: {}. A workspace has one manager; tell the user, who closes that agent first if you are to take over", g.label, other.name),
            ));
        }
        Ok(())
    }

    /// Records an event as `who`, found by `App::who`, never taken from the caller's word.
    pub(in crate::server) fn record(&self, who: &Principal, body: Body) -> Result<core::Event, ApiError> {
        let (actor, machine) = (who.actor(), who.machine());
        if let (Body::ManagerRegistered { intent, workspace, .. }, Principal::Agent { session, machine }) = (&body, who) {
            self.may_manage(session, machine.as_deref(), intent.as_deref(), workspace.as_ref())?;
        }
        let body = self.complete(actor, body, machine)?;
        let e = self.store.lock().unwrap().record(who, body)?.event;
        self.deliver(&e);
        match &e.body {
            Body::ManagerRegistered { agent, .. } => self.tell_of_update(agent, true),
            Body::QuestionAnswered { question, .. } => self.answered(question),
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
            Body::IntentCreated { id, title, goal, runtime_ref, project, state, note, external } => {
                let project = if project.is_empty() { project } else { self.project_folder(&project, machine)? };
                Body::IntentCreated { id, title, goal, runtime_ref, project, state, note, external }
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
        // The user changed an active intent: its manager hears of it, or once its blind task is
        // delivered (`tell`).
        if matches!(e.body, Body::IntentUpdated { .. }) {
            let read = got(self.store.lock().unwrap().recorded());
            let Ok(Some(all)) = read else { return };
            if let Some((to, message)) = delivery::edited(&all, e)
                && let Ok(Err(err)) = self.tell(&to, &message, None)
            {
                tracing::warn!("cannot tell {to} of the edit of {}: {err}", e.body.refs().0.unwrap_or_default());
            }
            return;
        }
        let live = self.live();
        // An addition to a finding goes to several at once; it is on the page either way, so
        // what does not arrive is only logged.
        // ponytail: these are not followed up as "delivered / picked up" like decisions are.
        // A dropped task likewise: its agent (to stop) and the manager.
        // Work over: the manager hears which of its workers it may close.
        if matches!(e.body, Body::FindingNoted { .. } | Body::TaskDropped { .. } | Body::UnitMerged { .. } | Body::UnitDone { .. } | Body::IntentStateChanged { .. } | Body::ManagerTold { .. }) {
            let Ok(Some(all)) = got(self.store.lock().unwrap().recorded()) else { return };
            let to = {
                match e.body {
                    Body::FindingNoted { .. } => delivery::notes(&all, live.as_deref(), &self.home, e),
                    Body::TaskDropped { .. } => delivery::dropped(&all, live.as_deref(), e),
                    Body::ManagerTold { .. } => delivery::told(&all, live.as_deref(), e),
                    _ => delivery::finished(&all, live.as_deref(), e),
                }
            };
            for (agent, message) in to {
                if let Err(err) = self.send(&agent, &message) {
                    let (_, unit, task) = e.body.refs();
                    tracing::warn!("cannot tell {agent} of {}: {err}", task.or(unit).unwrap_or_default());
                }
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

/// The machine a unit is put on: the one named ("local": the one workriver runs on), else
/// the caller's.
fn place(named: Option<&str>, callers: Option<&str>) -> Option<String> {
    match named {
        Some("local") => None,
        Some(m) => Some(m.to_string()),
        None => callers.map(str::to_string),
    }
}
