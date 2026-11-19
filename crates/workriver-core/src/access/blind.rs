// What an agent on an undelivered blind task may see: its own task, unit and intent.
use crate::planning::*;
use crate::records::*;

impl State {
    /// The blind tasks an agent is on: assigned to it or started by it, not delivered yet.
    pub fn blind_tasks(&self, session: &str) -> Vec<(String, u32)> {
        let name = self.name_of(session);
        let mine = |t: &&Task| t.blind && !t.state.over() && t.agent.as_deref().is_some_and(|a| a == session || Some(a) == name);
        let units = self.units.iter().flat_map(|u| u.tasks.iter().filter(mine).map(|t| (u.id.clone(), t.n)));
        units.chain(self.intents.iter().flat_map(|i| i.tasks.iter().filter(mine).map(|t| (i.id.clone(), t.n)))).collect()
    }

    /// What an agent on a blind task may see until it delivers: its task (as handed out),
    /// the task's unit and intent, what it attached itself. None for anyone else.
    ///
    /// Everything else is left out: other tasks with their output, notes, evidence, findings
    /// and decisions, questions, deliveries, the backlog, other units and intents.
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
        let intents = self
            .intents
            .iter()
            .filter(|i| mine.iter().any(|(o, _)| o == &i.id) || units.iter().any(|u| u.intent == i.id))
            .map(|i| Intent { note: String::new(), reason: None, tasks: own(&i.id, &i.tasks), ..i.clone() })
            .collect();
        Some(State {
            intents,
            units,
            agents: self.agents.clone(),
            attachments: self.attachments.iter().filter(|a| a.by == session).cloned().collect(),
            projects: self.projects.clone(),
            workspaces: self.workspaces.clone(),
            links: self.links.clone(),
            reduced: true,
            ..State::default()
        })
    }
}
