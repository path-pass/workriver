// Gates: tasks only the user does, opened once what they follow is done.
use serde::{Deserialize, Serialize};

use crate::*;

use super::findings::open;
use super::tasks::not_dropped;

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

    /// The rules of a gate's events: `prepare`, for these kinds.
    pub(crate) fn prepare_gates(&self, _who: &Principal, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        Ok(match body {
            GateSubmitted { unit, task, result, note, findings, attachments } => {
                self.stored(attachments.iter().chain(findings.iter().flat_map(|f| &f.attachments)))?;
                let t = self.task(&unit, task)?;
                not_dropped(&unit, t)?;
                if t.kind == TaskKind::Gate && t.state == TaskState::Todo {
                    return no(format!("gate {unit}#{task} is not open yet: it follows {}, not done yet", self.unfinished_inputs(&unit, t).join(", ")));
                }
                if t.kind != TaskKind::Gate || t.state != TaskState::Waiting {
                    return no(format!("{unit}#{task} is not a waiting gate"));
                }
                match result {
                    GateResult::Problem if findings.is_empty() => return no("a gate with a problem needs at least one finding"),
                    GateResult::Done if !findings.is_empty() => return no("a gate that is done carries no findings; submit it as problem"),
                    _ => {}
                }
                // The user at a gate may not know where the problem sits; a problem is enough.
                if findings.iter().any(|f| blank(&f.problem)) {
                    return no("a finding needs a problem");
                }
                GateSubmitted { unit, task, result, note, findings: numbered(findings), attachments }
            }
            other => unreachable!("{} is not one of a gate's kinds", name_of_kind(&other)),
        })
    }

    /// What a gate's events change: `apply`, for these kinds.
    pub(crate) fn apply_gates(&mut self, e: &Event) {
        use Body::*;
        let at = &e.at;
        match &e.body {
            GateSubmitted { unit, task, result, note, findings, attachments } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.state = TaskState::Done;
                    t.done = Some(at.clone());
                    t.result = Some(*result);
                    t.note = note.clone();
                    t.findings = findings.iter().cloned().map(open).collect();
                    t.attachments.extend(attachments.iter().cloned());
                }
            }
            other => unreachable!("{} is not one of a gate's kinds", name_of_kind(other)),
        }
    }
}
