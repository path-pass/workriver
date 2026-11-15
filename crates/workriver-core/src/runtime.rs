// What workriver needs from the program that runs the agents, in workriver's own words.
// The one implementation is Herdr's, in the outer crate; nothing here names it.
use serde::Serialize;

use crate::model::RuntimeRef;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    Working,
    Idle,
    Done,
    Blocked,
    Unknown,
}

/// One agent session the runtime knows about.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct RuntimeAgent {
    /// The harness's own session id; what workriver records as the actor, and what
    /// `focus` and `send` take.
    pub session: String,
    /// Which harness runs it, e.g. "claude" or "codex".
    pub harness: String,
    pub state: AgentState,
    pub cwd: String,
    /// The group the agent works in; one group is one intent.
    pub group: Option<RuntimeRef>,
    /// The name shown for the agent.
    pub name: String,
    /// What the session is about, in the harness's words (its terminal title), if known.
    pub topic: Option<String>,
}

/// What is at a place the CLI found in its environment (a runtime-specific string).
#[derive(Serialize, Clone, PartialEq, Debug, Default)]
pub struct Place {
    /// The agent running there, if any (a plain shell has none).
    pub agent: Option<RuntimeAgent>,
    pub group: Option<RuntimeRef>,
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("no agent runtime is connected")]
    None,
    #[error("no live agent {0}")]
    NoAgent(String),
    #[error("{0}")]
    Failed(String),
}

/// Sync on purpose: the daemon calls it from a blocking task.
pub trait Runtime: Send + Sync {
    /// The live agents, read fresh.
    fn agents(&self) -> Result<Vec<RuntimeAgent>, RuntimeError>;
    /// What is at `place`, read fresh.
    fn locate(&self, place: &str) -> Result<Place, RuntimeError>;
    /// Bring the agent's terminal to the front.
    fn focus(&self, session: &str) -> Result<(), RuntimeError>;
    /// Type a message into the agent's terminal and submit it; does not wait for an answer.
    /// `agent` is a session or a live agent's name.
    fn send(&self, agent: &str, text: &str) -> Result<(), RuntimeError>;
    /// Start a temporary agent of `harness`, named `name`, in a place of its own inside
    /// `group`, working in `cwd`; returns it once it is ready for a prompt.
    fn start_temporary(&self, group: &str, harness: &str, name: &str, cwd: &str) -> Result<RuntimeAgent, RuntimeError>;
    /// Close an agent started with `start_temporary`, and its place (session or name).
    fn close(&self, agent: &str) -> Result<(), RuntimeError>;
}

/// Used when no runtime is configured: no agents are known and none can be reached.
pub struct NoRuntime;

impl Runtime for NoRuntime {
    fn agents(&self) -> Result<Vec<RuntimeAgent>, RuntimeError> {
        Err(RuntimeError::None)
    }
    fn locate(&self, _: &str) -> Result<Place, RuntimeError> {
        Err(RuntimeError::None)
    }
    fn focus(&self, _: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::None)
    }
    fn send(&self, _: &str, _: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::None)
    }
    fn start_temporary(&self, _: &str, _: &str, _: &str, _: &str) -> Result<RuntimeAgent, RuntimeError> {
        Err(RuntimeError::None)
    }
    fn close(&self, _: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::None)
    }
}
