// The commands the user ran from the page, as recorded: what, where, who wrote it, how it
// ended and what its review said.
use serde::{Deserialize, Serialize};

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
