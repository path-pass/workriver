// What an agent on an undelivered blind task may see: its own task, unit and intent.
use crate::*;

impl State {
    /// The blind tasks an agent is on: assigned to it or started by it, not delivered yet.
    pub fn blind_tasks(&self, session: &str) -> Vec<(String, u32)> {
        let name = self.name_of(session);
        let mine = |t: &&Task| t.blind && !t.state.over() && t.agent.as_deref().is_some_and(|a| a == session || Some(a) == name);
        let units = self.units.iter().flat_map(|u| u.tasks.iter().filter(mine).map(|t| (u.id.clone(), t.n)));
        units.chain(self.intents.iter().flat_map(|i| i.tasks.iter().filter(mine).map(|t| (i.id.clone(), t.n)))).collect()
    }

    /// What an agent on a blind task may see until it delivers: its task (as handed out),
    /// the task's unit and intent (with what was added to what the task asks), what it
    /// attached itself. None for anyone else.
    ///
    /// Everything else is left out: other tasks with their output, notes, evidence, findings
    /// and decisions, questions, deliveries, the backlog, other units and intents, and where
    /// other agents work (their working directory and machine; their names stay).
    pub fn blind_view(&self, session: &str) -> Option<State> {
        let mine = self.blind_tasks(session);
        if mine.is_empty() {
            return None;
        }
        let own = |owner: &str, tasks: &[Task]| -> Vec<Task> {
            tasks
                .iter()
                .filter(|t| mine.iter().any(|(o, n)| o == owner && *n == t.n))
                // What the task follows and answers would name other work.
                .map(|t| Task { causes: vec![], delivery: None, ..t.clone() })
                .collect()
        };
        // Not where the unit was before, nor what was linked: both carry others' words.
        let units: Vec<Unit> = self.units.iter().filter(|u| mine.iter().any(|(o, _)| o == &u.id)).map(|u| Unit { tasks: own(&u.id, &u.tasks), earlier: vec![], links: vec![], ..u.clone() }).collect();
        let intents: Vec<Intent> = self
            .intents
            .iter()
            .filter(|i| mine.iter().any(|(o, _)| o == &i.id) || units.iter().any(|u| u.intent == i.id))
            .map(|i| Intent { note: String::new(), reason: None, tasks: own(&i.id, &i.tasks), edits: Edits::default(), ..i.clone() })
            .collect();
        // What was added to its own task is part of what it asks, files included.
        let added: Vec<String> = units.iter().flat_map(|u| &u.tasks).chain(intents.iter().flat_map(|i| &i.tasks)).flat_map(|t| &t.notes).flat_map(|n| n.attachments.iter().cloned()).collect();
        Some(State {
            intents,
            units,
            // Others by name only: where another session is kept is none of its business.
            agents: self.agents.iter().map(|a| if a.session == session { a.clone() } else { Agent { cwd: String::new(), machine: None, ..a.clone() } }).collect(),
            attachments: self.attachments.iter().filter(|a| a.by == session || added.contains(&a.id)).cloned().collect(),
            projects: self.projects.clone(),
            workspaces: self.workspaces.clone(),
            links: self.links.clone(),
            reduced: true,
            ..State::default()
        })
    }
}
