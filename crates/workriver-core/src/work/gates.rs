// Gates: tasks only the user does, opened once what they follow is done.
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum GateKind {
    Deploy,
    Test,
    /// The user discusses a design (asking why) and confirms it; the note is the decision.
    Decide,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum GateResult {
    Done,
    Problem,
}

impl State {
    /// A gate waits for the user only once every task it follows is done; until then it is
    /// still to do, asks nobody for anything, and counts nowhere. It waits from then on.
    pub(crate) fn open_gates(&mut self, at: &str) {
        let todo = |ts: &[Task]| ts.iter().filter(|t| t.kind == TaskKind::Gate && t.state == TaskState::Todo).map(|t| t.n).collect::<Vec<_>>();
        let owners: Vec<(String, Vec<u32>)> = self.units.iter().map(|u| (u.id.clone(), todo(&u.tasks))).chain(self.intents.iter().map(|i| (i.id.clone(), todo(&i.tasks)))).filter(|(_, g)| !g.is_empty()).collect();
        for (owner, gates) in owners {
            for n in gates {
                let ready = self.task(&owner, n).is_ok_and(|t| self.unfinished_inputs(&owner, t).is_empty());
                if ready && let Some(t) = self.task_mut(&owner, n) {
                    t.state = TaskState::Waiting;
                    t.waiting_since = Some(at.to_string());
                }
            }
        }
    }
}
