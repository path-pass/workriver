// What an agent may see: on an undelivered blind task, its own task, unit and intent; else
// everything but the conversations of workspaces it does not manage.
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
            // A seat is an agent's machine and folder: cleared like any other agent's.
            workspaces: self.workspaces.iter().map(|w| Workspace { manager: w.manager.as_ref().map(|m| if m.session == session { m.clone() } else { Seat { machine: None, folder: String::new(), ..m.clone() } }), ..w.clone() }).collect(),
            reduced: true,
            ..State::default()
        })
    }

    /// What an agent that is on no blind task may see: everything recorded, but of the
    /// conversations with the user only its own. `self` is the whole state; it is cut down:
    ///
    /// - the messages of the workspace whose seat `session` holds, all of them;
    /// - of a message to every manager, only when a copy went to its appointment now: the
    ///   message, its own copy and its own replies. Nothing of the other copies (their
    ///   workspace, manager, delivery) or their replies;
    /// - the read marks of that workspace, none other;
    /// - every attachment but one that only the parts cut away name: one that anything kept
    ///   names too, or that the agent stored itself, stays.
    ///
    /// Intents, units, tasks, findings, gates, chores, questions, threads and agents stay as
    /// they are: only the conversations are scoped by workspace.
    pub fn conversations_of(mut self, session: &str) -> State {
        let seat = self.seat_of(session).and_then(|w| Some((w.id.clone(), w.manager.as_ref()?.since)));
        let mine = |id: &str| seat.as_ref().is_some_and(|(w, _)| w == id);
        let copied = |r: &Recipient| r.manager == session && seat.as_ref().is_some_and(|(w, since)| *w == r.workspace && *since == r.since);
        // What the parts cut away point at, before they go.
        let named = |c: &[AskContext]| c.iter().filter(|c| c.kind == "attachment").filter_map(|c| c.id.clone()).collect::<Vec<_>>();
        let mut cut: Vec<String> = vec![];
        let mut kept = vec![];
        for mut m in std::mem::take(&mut self.messages) {
            let keep = if m.to.is_empty() { mine(&m.workspace) } else { m.to.iter().any(copied) };
            if !keep {
                cut.extend(named(&m.context));
                continue;
            }
            if !m.to.is_empty() {
                m.to.retain(copied);
                m.replies.retain(|r| r.by == session);
            }
            kept.push(m);
        }
        self.messages = kept;
        self.read.retain(|w, _| mine(w));
        if !cut.is_empty() {
            // Whatever else names one keeps it: looked for in everything kept, as it is
            // written, so that no kind of record is forgotten here.
            let rest = serde_json::to_string(&State { attachments: vec![], ..self.clone() }).unwrap_or_default();
            self.attachments.retain(|a| a.by == session || !cut.contains(&a.id) || rest.contains(&a.id));
        }
        self
    }
}
