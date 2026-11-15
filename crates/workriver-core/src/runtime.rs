// What workriver needs from the program that runs the agents, in workriver's own words.
// The one implementation planned is Herdr's, in the outer crate; nothing here names it.
use serde::Serialize;

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
    /// The runtime's id for the session; what `focus` and `send` take.
    pub session: String,
    /// Which harness runs it, e.g. "claude" or "codex".
    pub harness: String,
    pub state: AgentState,
    pub cwd: String,
    /// The runtime's id for the group the agent works in; one group is one intent.
    pub intent: Option<String>,
    /// The name shown and recorded for the agent.
    pub name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("no agent runtime is connected")]
    None,
    #[error("no agent {0}")]
    NoAgent(String),
    #[error("{0}")]
    Failed(String),
}

/// Sync on purpose: the daemon calls it from a blocking task.
pub trait Runtime: Send + Sync {
    fn agents(&self) -> Result<Vec<RuntimeAgent>, RuntimeError>;
    /// Bring the agent's terminal to the front.
    fn focus(&self, agent: &str) -> Result<(), RuntimeError>;
    /// Type a message into the agent's terminal.
    fn send(&self, agent: &str, text: &str) -> Result<(), RuntimeError>;
}

/// Used when no runtime is configured: no agents are known and none can be reached.
pub struct NoRuntime;

impl Runtime for NoRuntime {
    fn agents(&self) -> Result<Vec<RuntimeAgent>, RuntimeError> {
        Err(RuntimeError::None)
    }
    fn focus(&self, _: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::None)
    }
    fn send(&self, _: &str, _: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::None)
    }
}
