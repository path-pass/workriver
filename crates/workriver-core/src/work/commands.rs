// The commands the user ran from the page, as recorded: what, where, who wrote it, how it
// ended and what its review said.
use serde::{Deserialize, Serialize};

use crate::*;

/// What a review of a command the user is about to run found it to be.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// It does what it says, nothing risky.
    Ok,
    /// Worth a look: changes outside its folder, network, installs, deletes.
    Caution,
    /// Destructive or irreversible.
    Danger,
}

/// How many of the latest command runs the state keeps.
pub const RUNS_KEPT: usize = 20;

/// A command the user ran from the page, as recorded, with its exit once it was seen.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct CommandRun {
    pub id: String,
    pub command: String,
    pub dir: String,
    pub machine: Option<String>,
    pub unit: Option<String>,
    /// Who wrote it: an agent's session, or "user".
    pub by: String,
    /// Whose step it was (`u1#3`, `c2`) and which step (from 1); none: typed by the user.
    pub from: Option<String>,
    pub step: Option<u32>,
    pub at: String,
    pub exit: Option<i32>,
    /// What its review found, and said.
    pub review: Option<(Verdict, String)>,
}

impl State {
    /// The rules of a command's events: `prepare`, for these kinds.
    pub(crate) fn prepare_commands(&self, who: &Principal, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        Ok(match body {
            CommandRan { id, command, dir, machine, unit, by, from, step } => {
                if blank(&id) || blank(&command) || blank(&dir) {
                    return no("a command run needs its id, its text and its directory");
                }
                if let Some(u) = &unit {
                    self.unit(u)?;
                }
                CommandRan { id, command, dir, machine, unit, by, from, step }
            }
            CommandEnded { id, exit } => {
                if *who != Principal::Service {
                    return no("only workriver records how a command ended");
                }
                CommandEnded { id, exit }
            }
            CommandReviewed { id, verdict, text } => {
                if *who != Principal::Service {
                    return no("only workriver records a command's review, from the agent it started for it");
                }
                if blank(&text) {
                    return no("a review says what the command does");
                }
                CommandReviewed { id, verdict, text }
            }
            other => unreachable!("{} is not one of a command's kinds", name_of_kind(&other)),
        })
    }

    /// What a command's events change: `apply`, for these kinds.
    pub(crate) fn apply_commands(&mut self, e: &Event) {
        use Body::*;
        let at = &e.at;
        match &e.body {
            // The latest few only (the user's recent runs); never their output.
            CommandRan { id, command, dir, machine, unit, by, from, step } => {
                self.runs.push(CommandRun { id: id.clone(), command: command.clone(), dir: dir.clone(), machine: machine.clone(), unit: unit.clone(), by: by.clone(), from: from.clone(), step: *step, at: at.clone(), exit: None, review: None });
                let over = self.runs.len().saturating_sub(RUNS_KEPT);
                self.runs.drain(..over);
            }
            CommandEnded { id, exit } => {
                if let Some(r) = self.runs.iter_mut().find(|r| &r.id == id) {
                    r.exit = Some(*exit);
                }
            }
            CommandReviewed { id, verdict, text } => {
                if let Some(r) = self.runs.iter_mut().find(|r| &r.id == id) {
                    r.review = Some((*verdict, text.clone()));
                }
            }
            other => unreachable!("{} is not one of a command's kinds", name_of_kind(other)),
        }
    }
}
