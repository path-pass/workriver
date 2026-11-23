// Gates: tasks only the user does, opened once what they follow is done.
use serde::{Deserialize, Serialize};

use crate::*;

use super::findings::open;
use super::tasks::{filled, not_dropped};

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
    /// A choice gate's question answered: an option picked (`selected`). Not a go: nothing
    /// follows from it but what the manager adds.
    Answered,
}

/// A step placed from a template, as a gate lists it: its id and task, and for one done or
/// left out who did it (a gate: "user") and, left out, why.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StepMark {
    pub id: String,
    pub n: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// What a gate of a unit with placed steps lists of them, under its template's name. Worked
/// out for a reader (the page, `task show`, the board); never recorded.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GateSteps {
    pub template: String,
    /// The user's template (its descriptions are the user's words), not the built-in.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub users: bool,
    /// Delivered or submitted, with who did it.
    pub done: Vec<StepMark>,
    /// Dropped, with who and why.
    pub left_out: Vec<StepMark>,
    /// Neither, gates aside: what the gate goes ahead of.
    pub not_done: Vec<StepMark>,
}

impl State {
    /// What gate `n` of `unit` lists of the unit's placed steps: a placed gate those before
    /// it, a gate added by hand all of them (so one put in front of the flow shows what it
    /// jumps). None for a blocker, for a task that is no gate, and in a unit with no step
    /// placed. A step left out never reads as done.
    pub fn gate_steps(&self, unit: &str, n: u32) -> Option<GateSteps> {
        let u = self.unit(unit).ok()?;
        let g = u.tasks.iter().find(|t| t.n == n)?;
        if g.kind != TaskKind::Gate || g.blocks.is_some() || !u.tasks.iter().any(|t| t.placed.is_some()) {
            return None;
        }
        let before = g.placed.is_some().then(|| self.before(unit, n));
        let listed = u.tasks.iter().filter(|t| t.n != n && t.placed.is_some() && before.as_ref().is_none_or(|b| b.contains(&t.n)));
        let mark = |t: &Task, by: Option<String>, reason: Option<String>| StepMark { id: t.placed.as_ref().map(|p| p.id.clone()).unwrap_or_default(), n: t.n, by, reason };
        let mut out = GateSteps { template: u.template.clone(), users: u.revision.is_some(), done: vec![], left_out: vec![], not_done: vec![] };
        for t in listed {
            match (&t.dropped, t.state) {
                (Some(d), _) => out.left_out.push(mark(t, Some(d.by.clone()), Some(d.reason.clone()))),
                (None, TaskState::Done) => out.done.push(mark(t, Some(if t.kind == TaskKind::Gate { "user".into() } else { t.agent.clone().unwrap_or_default() }), None)),
                (None, _) if t.kind != TaskKind::Gate => out.not_done.push(mark(t, None, None)),
                _ => {}
            }
        }
        Some(out)
    }

    /// The placed steps of a unit nobody filled or dropped yet, in order: what its manager
    /// still has to hand out (or leave out, with the reason).
    pub fn steps_to_fill(&self, unit: &str) -> Vec<StepMark> {
        let tasks = self.tasks_of(unit).map(Vec::as_slice).unwrap_or_default();
        tasks.iter().filter(|t| t.unfilled() && t.dropped.is_none()).map(|t| StepMark { id: t.placed.as_ref().map(|p| p.id.clone()).unwrap_or_default(), n: t.n, by: None, reason: None }).collect()
    }

    /// The placed steps of a unit neither done nor dropped, gates included: what a unit
    /// closed now goes without.
    pub fn steps_not_done(&self, unit: &str) -> Vec<StepMark> {
        let tasks = self.tasks_of(unit).map(Vec::as_slice).unwrap_or_default();
        tasks.iter().filter(|t| t.placed.is_some() && !t.state.over()).map(|t| StepMark { id: t.placed.as_ref().map(|p| p.id.clone()).unwrap_or_default(), n: t.n, by: None, reason: None }).collect()
    }

    /// A gate waits for the user only once every task it follows is done; until then it is
    /// still to do, asks nobody for anything, and counts nowhere. It waits from then on. A
    /// gate placed from a template waits for its fill as well.
    pub(crate) fn open_gates(&mut self, at: &str) {
        let todo = |ts: &[Task]| ts.iter().filter(|t| t.kind == TaskKind::Gate && t.state == TaskState::Todo && !t.unfilled()).map(|t| t.n).collect::<Vec<_>>();
        let owners: Vec<(String, Vec<u32>)> = self.units.iter().map(|u| (u.id.clone(), todo(&u.tasks))).filter(|(_, g)| !g.is_empty()).collect();
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
            GateSubmitted { unit, task, result, note, findings, attachments, selected } => {
                self.stored(attachments.iter().chain(findings.iter().flat_map(|f| &f.attachments)))?;
                let t = self.task(&unit, task)?;
                not_dropped(&unit, t)?;
                filled(&unit, t)?;
                if t.kind == TaskKind::Gate && t.state == TaskState::Todo {
                    return no(format!("gate {unit}#{task} is not open yet: it follows {}, not done yet", self.unfinished_inputs(&unit, t).join(", ")));
                }
                if t.kind != TaskKind::Gate || t.state != TaskState::Waiting {
                    return no(format!("{unit}#{task} is not a waiting gate"));
                }
                match (result, &t.choice) {
                    (GateResult::Problem, _) if findings.is_empty() => return no("a gate with a problem needs at least one finding"),
                    (GateResult::Done | GateResult::Answered, _) if !findings.is_empty() => {
                        return no(format!("a gate that is {} carries no findings; submit it as problem", name(&result)));
                    }
                    (GateResult::Done, Some(_)) => return no(format!("{unit}#{task} asks the user to choose; submit answered --select ID")),
                    (GateResult::Answered, None) => return no(format!("{unit}#{task} has no choices; submit done or problem")),
                    (GateResult::Answered, Some(c)) => c.check_answer(&selected, &note)?,
                    _ if !selected.is_empty() => return no("--select goes with answered"),
                    _ => {}
                }
                // The user at a gate may not know where the problem sits; a problem is enough.
                if findings.iter().any(|f| blank(&f.problem)) {
                    return no("a finding needs a problem");
                }
                GateSubmitted { unit, task, result, note, findings: numbered(findings), attachments, selected }
            }
            other => unreachable!("{} is not one of a gate's kinds", name_of_kind(&other)),
        })
    }

    /// What a gate's events change: `apply`, for these kinds.
    pub(crate) fn apply_gates(&mut self, e: &Event) {
        use Body::*;
        let at = &e.at;
        match &e.body {
            GateSubmitted { unit, task, result, note, findings, attachments, selected } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.state = TaskState::Done;
                    t.done = Some(at.clone());
                    t.result = Some(*result);
                    t.selected = selected.clone();
                    t.note = note.clone();
                    t.findings = findings.iter().cloned().map(open).collect();
                    t.attachments.extend(attachments.iter().cloned());
                }
            }
            other => unreachable!("{} is not one of a gate's kinds", name_of_kind(other)),
        }
    }
}
