// Chores: what an agent asks of the user that blocks nobody.
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Chore {
    pub id: String,
    /// Empty for project chores and old chores recorded without an intent or project.
    pub intent: String,
    #[serde(default)]
    pub project: String,
    pub unit: Option<String>,
    pub title: String,
    pub by: String,
    pub steps: Vec<Step>,
    pub done: bool,
    pub at: String,
    /// The note to the manager that the user was asked.
    #[serde(default)]
    pub delivery: Option<Delivery>,
}

impl State {
    /// The rules of a chore's events: `prepare`, for these kinds.
    pub(crate) fn prepare_chores(&self, _who: &Principal, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        Ok(match body {
            ChoreCreated { intent, project, unit, title, steps, .. } => {
                if !intent.is_empty() {
                    self.intent(&intent)?;
                } else if project.is_empty() {
                    return no("a chore needs an intent or a project");
                }
                if let Some(u) = &unit
                    && (intent.is_empty() || self.unit(u)?.intent != intent)
                {
                    return no(format!("unit {u} is not part of intent {intent:?}"));
                }
                if blank(&title) {
                    return no("a chore needs a title");
                }
                let project = if intent.is_empty() { project } else { String::new() };
                ChoreCreated { id: format!("c{}", self.chores.len() + 1), intent, project, unit, title, steps }
            }
            ChoreDone { chore } => match self.chores.iter().find(|c| c.id == chore) {
                None => return no(format!("no chore {chore}")),
                Some(c) if c.done => return no(format!("chore {chore} is already done")),
                Some(_) => ChoreDone { chore },
            },
            other => unreachable!("{} is not one of a chore's kinds", name_of_kind(&other)),
        })
    }

    /// What a chore's events change: `apply`, for these kinds.
    pub(crate) fn apply_chores(&mut self, e: &Event) {
        use Body::*;
        let at = &e.at;
        match &e.body {
            ChoreCreated { id, intent, project, unit, title, steps } => self.chores.push(Chore {
                id: id.clone(),
                intent: intent.clone(),
                project: project.clone(),
                unit: unit.clone(),
                title: title.clone(),
                by: e.actor.clone(),
                steps: steps.clone(),
                done: false,
                at: at.clone(),
                delivery: None,
            }),
            ChoreDone { chore } => {
                if let Some(c) = self.chores.iter_mut().find(|c| &c.id == chore) {
                    c.done = true;
                }
            }
            other => unreachable!("{} is not one of a chore's kinds", name_of_kind(other)),
        }
    }
}
