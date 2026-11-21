// Tasks: made, started, delivered, amended or dropped, and whose they are.
use crate::*;

use super::findings::{numbered_after, open};

/// A dropped task is no longer worked on: refused, saying why it was dropped.
pub(super) fn not_dropped(unit: &str, t: &Task) -> Result<(), Rejected> {
    match &t.dropped {
        Some(d) => no(format!("{unit}#{} was dropped as no longer needed ({}); it is not started, delivered or submitted any more", t.n, d.reason)),
        None => Ok(()),
    }
}

/// What a delivery or its correction may carry.
fn check_output(t: &Task, evidence: &[Evidence], findings: &[NewFinding]) -> Result<(), Rejected> {
    if !findings.is_empty() && matches!(t.kind, TaskKind::Implement | TaskKind::Design | TaskKind::Research) {
        return no("findings belong to a review or tests task; record a review task and deliver them there");
    }
    if evidence.iter().any(|e| blank(&e.command)) {
        return no("evidence needs the command that was run");
    }
    for f in findings {
        if blank(&f.problem) || blank(&f.impact) || f.r#where.iter().all(|w| blank(w)) {
            return no("a finding needs a problem, an impact and at least one where (a file path)");
        }
    }
    Ok(())
}

impl State {
    /// The tasks of a unit, or an intent's intent-level tasks.
    pub fn tasks_of(&self, owner: &str) -> Option<&Vec<Task>> {
        self.units.iter().find(|u| u.id == owner).map(|u| &u.tasks).or_else(|| self.intents.iter().find(|i| i.id == owner).map(|i| &i.tasks))
    }

    /// A task by its owner (a unit, or an intent for intent-level work) and number.
    pub fn task(&self, unit: &str, n: u32) -> Result<&Task, Rejected> {
        let Some(tasks) = self.tasks_of(unit) else { return no(format!("no unit or intent {unit}")) };
        match tasks.iter().find(|t| t.n == n) {
            Some(t) => Ok(t),
            None => no(format!("no task {unit}#{n}")),
        }
    }

    /// Whether `actor` is the agent a task is assigned to or was done by (by session, or by
    /// the name it was assigned under).
    pub(crate) fn is_agent_of(&self, t: &Task, actor: &str) -> bool {
        t.agent.as_deref().is_some_and(|a| a == actor || Some(a) == self.name_of(actor))
    }

    fn prepare_task(&self, mut t: NewTask) -> Result<NewTask, Rejected> {
        // A task is in a unit, or (research, design and their reviews and gates) in an intent.
        let (owner, intent, count) = if let Ok(u) = self.unit(&t.unit) {
            if u.merged.is_some() {
                return no(format!("unit {} is merged; record new work in a new unit", u.id));
            }
            if !u.kind.is_code() && matches!(t.kind, TaskKind::Implement | TaskKind::Tests) {
                return no(format!("{} is a research unit: it takes research, design, review and gate tasks; implement and tests tasks belong to a unit with a branch", u.id));
            }
            (u.id.clone(), u.intent.clone(), u.tasks.len())
        } else {
            // Before 2026-11-17 research and design went into the intent itself (`i3#1`); those
            // tasks still replay and work, but new ones go into a research unit.
            let i = self.intent(&t.unit).map_err(|_| Rejected(format!("no unit {}", t.unit)))?;
            return no(format!(
                "{} is an intent: tasks go into a unit. For research or design, record a research unit first: workriver unit add {} --kind research --title \"...\", then workriver task add u<N> ...",
                i.id, i.id
            ));
        };
        if let Some(i) = self.intents.iter().find(|i| i.id == intent)
            && matches!(i.state, IntentState::Done | IntentState::Dropped)
        {
            return no(format!("intent {intent} is {}; it takes no new tasks", name(&i.state)));
        }
        // A blocker stands where the task it blocks starts: it follows what that task follows.
        if let Some(b) = t.blocks {
            if t.kind != TaskKind::Gate {
                return no("only a gate blocks a task: --kind gate --gate deploy|test|decide");
            }
            let stuck = self.task(&owner, b)?;
            if stuck.kind == TaskKind::Gate || stuck.state.over() {
                return no(format!("{owner}#{b} is {}: a blocker goes before a task still to do or running", if stuck.kind == TaskKind::Gate { "a gate".to_string() } else { name(&stuck.state) }));
            }
            if t.inputs.is_empty() {
                t.inputs = if stuck.inputs.is_empty() { vec![Input { from: "start".into(), primary: true }] } else { stuck.inputs.clone() };
            }
        }
        match t.kind {
            TaskKind::Implement | TaskKind::Gate | TaskKind::Design | TaskKind::Research if blank(&t.title) => {
                return no("an implement, design or research task, or a gate, needs a title");
            }
            TaskKind::Gate if t.gate.is_none() => return no("a gate needs its kind: deploy or test"),
            TaskKind::Implement | TaskKind::Gate if t.blind => return no("--blind is for looking at work without being told about it: a review, tests, research or design task"),
            TaskKind::Review if t.reviewer.is_none() => t.reviewer = Some(Reviewer::CodeReview),
            _ => {}
        }
        if t.kind != TaskKind::Review {
            t.reviewer = None;
        }
        if t.kind != TaskKind::Gate {
            t.gate = None;
        }
        for c in &mut t.causes {
            if c.unit.as_deref() == Some(owner.as_str()) {
                c.unit = None;
            }
            // Task 0: findings of the project's backlog (`f3`).
            if c.task == 0 {
                for id in &c.findings {
                    let f = self.finding(&FindingRef { unit: String::new(), task: 0, finding: id.clone() })?;
                    if !f.state.leads_to_work() {
                        return no(format!("backlog finding {id} is {}; batch it first (Do together in the Backlog)", name(&f.state)));
                    }
                }
                continue;
            }
            let at = c.unit.clone().unwrap_or_else(|| owner.clone());
            let by = self.task(&at, c.task)?;
            if matches!(by.kind, TaskKind::Implement | TaskKind::Design | TaskKind::Research) {
                return no(format!("{at}#{} is an implement, design or research task; causes are findings of reviews, tests or gates", c.task));
            }
            if c.findings.is_empty() {
                return no(format!("the cause {at}#{} names no finding", c.task));
            }
            for id in &c.findings {
                match by.findings.iter().find(|f| &f.found.id == id) {
                    None => return no(format!("{at}#{} has no finding {id}", c.task)),
                    Some(f) if !f.state.leads_to_work() => {
                        return no(format!(
                            "finding {id} of {at}#{} is neither accepted, revised nor scheduled; decide it first (workriver finding decide {at}#{} {id} accept)",
                            c.task, c.task
                        ));
                    }
                    Some(_) => {}
                }
            }
        }
        // Inputs default to the causes in the same unit (the first one primary), or to the start.
        if t.inputs.is_empty() {
            t.inputs = t.causes.iter().filter(|c| c.unit.is_none() && c.task != 0).enumerate().map(|(k, c)| Input { from: c.task.to_string(), primary: k == 0 }).collect();
        }
        // The first task starts at the start; a later one must say what it follows, else it
        // would hang off the start beside the first, away from the work it continues.
        if t.inputs.is_empty() && count > 0 {
            return no(format!("{owner} already has tasks: say which one this follows with --after <n> (the latest is #{count}), or --after start to run beside the first"));
        }
        if t.inputs.is_empty() {
            t.inputs.push(Input { from: "start".into(), primary: true });
        }
        for i in &mut t.inputs {
            let from = i.from.trim().trim_start_matches('#').to_string();
            if from != "start" {
                let n: u32 = from.parse().map_err(|_| Rejected(format!("input {:?} is neither \"start\" nor a task number", i.from)))?;
                self.task(&owner, n)?;
            }
            i.from = from;
        }
        if !t.inputs.iter().any(|i| i.primary) {
            t.inputs[0].primary = true;
        }
        self.check_choice(&owner, &t)?;
        t.n = count as u32 + 1;
        t.intent = intent;
        Ok(t)
    }

    /// A choice gate's place: a decide gate, after no design and blocking nothing; and no task
    /// hangs after one not answered yet (an answer is not a go: the manager adds what follows).
    fn check_choice(&self, owner: &str, t: &NewTask) -> Result<(), Rejected> {
        let inputs = || t.inputs.iter().filter_map(|i| i.from.parse::<u32>().ok()).filter_map(|n| self.task(owner, n).ok());
        if let Some(open) = inputs().find(|g| g.choice.is_some() && !g.state.over()) {
            return no(format!("{owner}#{} asks the user to choose; add what follows once it is answered", open.n));
        }
        let Some(c) = &t.choice else { return Ok(()) };
        if t.kind != TaskKind::Gate || t.gate != Some(GateKind::Decide) {
            return no("options are for a decide gate: --kind gate --gate decide --option ID=LABEL");
        }
        if t.blocks.is_some() {
            return no("a blocker carries no choices; tell your manager: workriver manager tell <task> --text \"...\"");
        }
        // As `designs_asked` sees it: a design before it, directly or through reviews.
        if inputs().any(|b| b.kind == TaskKind::Design || (b.kind == TaskKind::Review && !self.designs_asked(owner, b.n).is_empty())) {
            return no("a design is confirmed with a decide gate without --option (--go); put the choices in --text");
        }
        c.check()
    }

    pub(crate) fn task_mut(&mut self, unit: &str, n: u32) -> Option<&mut Task> {
        let tasks = match self.units.iter_mut().find(|u| u.id == unit) {
            Some(u) => &mut u.tasks,
            None => &mut self.intents.iter_mut().find(|i| i.id == unit)?.tasks,
        };
        tasks.iter_mut().find(|t| t.n == n)
    }

    /// The rules of a task's events: `prepare`, for these kinds.
    pub(crate) fn prepare_tasks(&self, who: &Principal, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        let actor = who.actor();
        let user = *who == Principal::User;
        Ok(match body {
            TaskCreated(t) => {
                // A blocker is put by the agent stuck on that task, its manager or the user.
                if let Some(b) = t.blocks
                    && !user
                    && let Ok(stuck) = self.task(&t.unit, b)
                    && !self.is_agent_of(stuck, actor)
                    && self.intent_of(&t.unit).and_then(|i| self.manager_of(i)) != Some(actor)
                {
                    let r = format!("{}#{b}", t.unit);
                    return no(format!("a blocker before {r} is put by the agent of {r}, the manager of its intent, or the user; to tell the manager: workriver manager tell {r} --text \"...\""));
                }
                // Options to pick from are asked by the intent's manager or the user.
                if t.choice.is_some() && !user && self.intent_of(&t.unit).and_then(|i| self.manager_of(i)) != Some(actor) {
                    let intent = self.intent_of(&t.unit).unwrap_or_default();
                    return no(format!("choices are asked by the manager of intent {intent} (or the user); tell your manager: workriver manager tell <task> --text \"...\""));
                }
                TaskCreated(self.prepare_task(t)?)
            }
            TaskStarted { unit, task, agent } => {
                let t = self.task(&unit, task)?;
                not_dropped(&unit, t)?;
                if t.kind == TaskKind::Gate {
                    return no(format!("{unit}#{task} is a gate: only the user does it, with gate submit"));
                }
                if t.state != TaskState::Todo {
                    return no(format!("{unit}#{task} is {}, not todo", name(&t.state)));
                }
                TaskStarted { unit, task, agent }
            }
            TaskDelivered { unit, task, note, evidence, findings, attachments } => {
                let t = self.task(&unit, task)?;
                not_dropped(&unit, t)?;
                if t.kind == TaskKind::Gate {
                    return no(format!("{unit}#{task} is a gate: the user submits it with gate submit"));
                }
                if t.state == TaskState::Done {
                    return no(format!("{unit}#{task} is already delivered; to correct it, run `workriver task deliver {unit}#{task} --amend` with what replaces it"));
                }
                check_output(t, &evidence, &findings)?;
                self.stored(evidence.iter().filter_map(|e| e.attachment.as_ref()).chain(findings.iter().flat_map(|f| &f.attachments)).chain(&attachments))?;
                if t.kind == TaskKind::Design && blank(&note) {
                    return no("a design task delivers its design as the note: --note \"...\"");
                }
                if t.kind == TaskKind::Research && blank(&note) && attachments.is_empty() {
                    return no("a research task delivers what it found: a --note, and its report attached (workriver attach --dir)");
                }
                TaskDelivered { unit, task, note, evidence, findings: numbered(findings), attachments }
            }
            TaskDropped { unit, task, reason } => {
                let t = self.task(&unit, task)?;
                not_dropped(&unit, t)?;
                if t.state == TaskState::Done {
                    return no(format!("{unit}#{task} is done; only work not done yet is dropped"));
                }
                if blank(&reason) {
                    return no("dropping a task needs a reason: --reason \"...\" (e.g. superseded by u1#17)");
                }
                // The manager re-plans; a worker takes back only what it asked for itself.
                let manager = self.intent_of(&unit).and_then(|i| self.manager_of(i)) == Some(actor);
                if !user && !manager && t.by != actor {
                    return no(format!("{unit}#{task} is dropped by the manager of its intent or the user (or the agent that recorded it); tell your manager it is no longer needed: workriver manager tell {unit}#{task} --text \"...\""));
                }
                TaskDropped { unit, task, reason }
            }
            TaskAmended { unit, task, note, evidence, findings, attachments, add } => {
                let t = self.task(&unit, task)?;
                if t.kind == TaskKind::Gate || t.state != TaskState::Done {
                    return no(format!("{unit}#{task} is not a delivered task; only a delivered task can be amended"));
                }
                if note.is_none() && evidence.is_none() && findings.is_none() && attachments.is_empty() && add.is_empty() {
                    return no("nothing to amend: give a note, evidence or findings");
                }
                if findings.is_some() && !add.is_empty() {
                    return no("either replace the findings (--findings) or add to them (--add-findings), not both");
                }
                if findings.is_some() && t.findings.iter().any(|f| f.state != FindingState::Open) {
                    return no(format!(
                        "some findings of {unit}#{task} are already decided, so they cannot be replaced; add new ones with `workriver task deliver {unit}#{task} --amend --add-findings FILE`, or add to a finding with `workriver finding note {unit}#{task} <id> --text \"...\"`"
                    ));
                }
                // New findings are the reviewer's to report (or the user's), as the first were.
                if !add.is_empty() && !user && !self.is_agent_of(t, actor) {
                    return no(format!("findings of {unit}#{task} are added by the agent that delivered it"));
                }
                check_output(t, evidence.as_deref().unwrap_or_default(), findings.as_deref().unwrap_or(&add))?;
                self.stored(
                    evidence.iter().flatten().filter_map(|e| e.attachment.as_ref()).chain(findings.iter().flatten().chain(&add).flat_map(|f| &f.attachments)).chain(&attachments),
                )?;
                let add = numbered_after(add, &t.findings);
                TaskAmended { unit, task, note, evidence, findings: findings.map(numbered), attachments, add }
            }
            TaskNoted { unit, task, text, attachments } => {
                let t = self.task(&unit, task)?;
                // The manager plans the work and the user owns it: nobody else changes what it asks.
                let manager = self.intent_of(&unit).and_then(|i| self.manager_of(i)) == Some(actor);
                if !user && !manager {
                    return no(format!("a requirement is added to {unit}#{task} by the manager of its intent or the user; to tell the manager: workriver manager tell {unit}#{task} --text \"...\""));
                }
                not_dropped(&unit, t)?;
                // Delivered work is judged as it was asked; more is new work.
                if t.state == TaskState::Done {
                    return no(format!(
                        "{unit}#{task} is already {}; what it asked is not changed after that: record the rest as new work (workriver task add {unit} --after {task} ...)",
                        if t.kind == TaskKind::Gate { "submitted" } else { "delivered" }
                    ));
                }
                if blank(&text) && attachments.is_empty() {
                    return no("a requirement needs its text: --text \"...\" (or a file: --attach FILE)");
                }
                self.stored(attachments.iter())?;
                TaskNoted { unit, task, text, attachments }
            }
            other => unreachable!("{} is not one of a task's kinds", name_of_kind(&other)),
        })
    }

    /// What a task's events change: `apply`, for these kinds.
    pub(crate) fn apply_tasks(&mut self, e: &Event) {
        use Body::*;
        let at = &e.at;
        let agent_of = |actor: &str| (actor != "user").then(|| actor.to_string());
        match &e.body {
            TaskCreated(t) => {
                // Before 0.2 a task named no intent: its unit's.
                let intent = if t.intent.is_empty() { self.intent_of(&t.unit).unwrap_or_default().to_string() } else { t.intent.clone() };
                let tasks = match self.units.iter_mut().find(|u| u.id == t.unit) {
                    Some(u) => Some(&mut u.tasks),
                    None => self.intents.iter_mut().find(|i| i.id == t.unit).map(|i| &mut i.tasks),
                };
                if let Some(tasks) = tasks {
                    tasks.push(Task {
                        n: t.n,
                        intent,
                        kind: t.kind,
                        title: t.title.clone(),
                        text: t.text.clone(),
                        inputs: t.inputs.clone(),
                        causes: t.causes.clone(),
                        agent: t.agent.clone(),
                        reviewer: t.reviewer,
                        gate: t.gate,
                        steps: t.steps.clone(),
                        plans: t.plans.clone(),
                        blind: t.blind,
                        blocks: t.blocks,
                        choice: t.choice.clone(),
                        // A gate opens (waits for the user) once what it follows is done.
                        state: TaskState::Todo,
                        by: e.actor.clone(),
                        at: at.clone(),
                        started: None,
                        done: None,
                        note: String::new(),
                        evidence: vec![],
                        findings: vec![],
                        result: None,
                        selected: vec![],
                        delivery: None,
                        amended: None,
                        read_before: vec![],
                        waiting_since: None,
                        attachments: vec![],
                        dropped: None,
                        notes: vec![],
                    })
                }
                // More work for a unit closed as done: it is open again, its state from its tasks.
                if let Some(u) = self.units.iter_mut().find(|u| u.id == t.unit)
                    && let Some(was) = u.done.take()
                {
                    u.reopened = Some(was);
                }
                // The task it blocks follows it from now on.
                if let Some(b) = t.blocks
                    && let Some(stuck) = self.task_mut(&t.unit, b)
                {
                    stuck.inputs.push(Input { from: t.n.to_string(), primary: false });
                }
            }
            TaskStarted { unit, task, agent } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.state = TaskState::Running;
                    t.started = Some(at.clone());
                    t.agent = agent.clone().or_else(|| agent_of(&e.actor)).or(t.agent.take());
                }
            }
            TaskDelivered { unit, task, note, evidence, findings, attachments } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.attachments.extend(attachments.iter().cloned());
                    t.state = TaskState::Done;
                    t.done = Some(at.clone());
                    t.note = note.clone();
                    t.evidence = evidence.clone();
                    t.findings = findings.iter().cloned().map(open).collect();
                    if t.agent.is_none() {
                        t.agent = agent_of(&e.actor);
                    }
                }
                // Scheduled findings this work answers are done with.
                let causes: Vec<FindingRef> = self
                    .task(unit, *task)
                    .map(|t| {
                        t.causes
                            .iter()
                            .flat_map(|c| {
                                let at = if c.task == 0 { String::new() } else { c.unit.clone().unwrap_or_else(|| unit.clone()) };
                                c.findings.iter().map(move |f| FindingRef { unit: at.clone(), task: c.task, finding: f.clone() })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                for r in causes {
                    if let Some(f) = self.finding_mut(&r).filter(|f| f.state == FindingState::Scheduled) {
                        f.state = FindingState::Done;
                    }
                }
            }
            TaskDropped { unit, task, reason } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.state = TaskState::Dropped;
                    t.dropped = Some(Closed { at: at.clone(), by: e.actor.clone(), reason: reason.clone() });
                }
            }
            TaskAmended { unit, task, note, evidence, findings, attachments, add } => {
                // The reviews of it under way or done read the text this replaces.
                let started: Vec<u32> = self
                    .tasks_of(unit)
                    .map(|ts| ts.iter().filter(|r| r.kind == TaskKind::Review && r.state != TaskState::Todo && r.dropped.is_none() && self.before(unit, r.n).contains(task)).map(|r| r.n).collect())
                    .unwrap_or_default();
                if let Some(th) = self.threads.iter_mut().find(|th| th.discuss && &th.unit == unit && th.task == *task) {
                    th.amended = true;
                }
                if let Some(t) = self.task_mut(unit, *task) {
                    for r in started {
                        if !t.read_before.contains(&r) {
                            t.read_before.push(r);
                        }
                    }
                    t.findings.extend(add.iter().cloned().map(open));
                    t.attachments.extend(attachments.iter().cloned());
                    if let Some(n) = note {
                        t.note = n.clone();
                    }
                    if let Some(e) = evidence {
                        t.evidence = e.clone();
                    }
                    if let Some(f) = findings {
                        t.findings = f.iter().cloned().map(open).collect();
                    }
                    t.amended = Some(at.clone());
                }
            }
            TaskNoted { unit, task, text, attachments } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.notes.push(FindingNote { by: e.actor.clone(), at: at.clone(), text: text.clone(), attachments: attachments.clone() });
                }
            }
            other => unreachable!("{} is not one of a task's kinds", name_of_kind(other)),
        }
    }
}
