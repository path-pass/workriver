// Tasks: made, started, delivered, amended or dropped, and whose they are.
use crate::*;

/// A dropped task is no longer worked on: refused, saying why it was dropped.
pub(crate) fn not_dropped(unit: &str, t: &Task) -> Result<(), Rejected> {
    match &t.dropped {
        Some(d) => no(format!("{unit}#{} was dropped as no longer needed ({}); it is not started, delivered or submitted any more", t.n, d.reason)),
        None => Ok(()),
    }
}

/// What a delivery or its correction may carry.
pub(crate) fn check_output(t: &Task, evidence: &[Evidence], findings: &[NewFinding]) -> Result<(), Rejected> {
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

    pub(crate) fn prepare_task(&self, mut t: NewTask) -> Result<NewTask, Rejected> {
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
        t.n = count as u32 + 1;
        t.intent = intent;
        Ok(t)
    }

    pub(crate) fn task_mut(&mut self, unit: &str, n: u32) -> Option<&mut Task> {
        let tasks = match self.units.iter_mut().find(|u| u.id == unit) {
            Some(u) => &mut u.tasks,
            None => &mut self.intents.iter_mut().find(|i| i.id == unit)?.tasks,
        };
        tasks.iter_mut().find(|t| t.n == n)
    }
}
