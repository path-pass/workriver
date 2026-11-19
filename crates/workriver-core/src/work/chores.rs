// Chores: what an agent asks of the user that blocks nobody.
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Chore {
    pub id: String,
    /// Empty: a project's chore (`project`), or nobody's.
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
