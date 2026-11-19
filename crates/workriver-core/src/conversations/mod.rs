// What is said and handed over: questions and their threads, messages between the user and
// managers, the agents seen, attachments, and where each message workriver sends is.
use serde::{Deserialize, Serialize};

use crate::*;

mod attachments;
mod messages;
mod questions;

pub use attachments::*;
pub use messages::*;
pub use questions::*;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum DeliveryStep {
    NotDelivered,
    Delivered,
    /// The receiver started working after it arrived.
    Picked,
}

/// Why a message was not delivered.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum ReasonCode {
    /// The intent has no manager registered.
    NoManager,
    /// The manager is not running.
    ManagerGone,
    /// The agent runtime cannot be reached.
    RuntimeDown,
    /// The runtime refused or failed; `reason` holds its words.
    RuntimeError,
    /// The intent follows no runtime group, and no agent of it runs: nowhere to start one.
    NoWorkspace,
    /// No answer came in time.
    Timeout,
    /// The service restarted while waiting.
    Restarted,
    /// The agent waited for a person in its terminal (a permission or first-run prompt) the
    /// whole time: the message never reached it.
    Blocked,
    /// A new agent never became ready to take its first message.
    NotReady,
}

/// Where the message about a user's input (a decision, a gate result) is.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Delivery {
    pub step: DeliveryStep,
    /// The receiving session; None when there was none.
    pub to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<ReasonCode>,
    /// Why it was not delivered, in words (the runtime's own, for a runtime error).
    pub reason: Option<String>,
    pub message: String,
}

impl State {
    /// The rules of a delivery's events: `prepare`, for these kinds.
    pub(crate) fn prepare_deliveries(&self, _who: &Principal, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        Ok(match body {
            DeliveryUpdated { unit, task, finding, chore, delivery } => {
                if let Some(c) = &chore {
                    if !self.chores.iter().any(|x| &x.id == c) {
                        return no(format!("no chore {c}"));
                    }
                } else if task == 0 {
                    // A backlog finding's (a batch's message is recorded on its first finding).
                    self.finding(&FindingRef { unit: String::new(), task: 0, finding: finding.clone().unwrap_or_default() })?;
                } else {
                    let t = self.task(&unit, task)?;
                    if let Some(id) = &finding
                        && !t.findings.iter().any(|f| &f.found.id == id)
                    {
                        return no(format!("{unit}#{task} has no finding {id}"));
                    }
                }
                DeliveryUpdated { unit, task, finding, chore, delivery }
            }
            other => unreachable!("{} is not one of a delivery's kinds", name_of_kind(&other)),
        })
    }

    /// What a delivery's events change: `apply`, for these kinds.
    pub(crate) fn apply_deliveries(&mut self, e: &Event) {
        use Body::*;
        match &e.body {
            DeliveryUpdated { chore: Some(c), delivery, .. } => {
                if let Some(x) = self.chores.iter_mut().find(|x| &x.id == c) {
                    x.delivery = Some(delivery.clone());
                }
            }
            DeliveryUpdated { task: 0, finding: Some(f), delivery, .. } => {
                if let Some(x) = self.finding_mut(&FindingRef { unit: String::new(), task: 0, finding: f.clone() }) {
                    x.delivery = Some(delivery.clone());
                }
            }
            DeliveryUpdated { unit, task, finding, delivery, .. } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    match finding {
                        None => t.delivery = Some(delivery.clone()),
                        Some(id) => {
                            if let Some(f) = t.findings.iter_mut().find(|f| &f.found.id == id) {
                                f.delivery = Some(delivery.clone());
                            }
                        }
                    }
                }
            }
            other => unreachable!("{} is not one of a delivery's kinds", name_of_kind(other)),
        }
    }
}

#[cfg(test)]
mod tests;
