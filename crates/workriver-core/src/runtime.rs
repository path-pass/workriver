// What workriver needs from the program that runs the agents, in workriver's own words.
// The one implementation is Herdr's, in the outer crate; nothing here names it.
use serde::Serialize;

use crate::records::RuntimeRef;

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
    /// The machine it runs on; none: this one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
}

/// What is at a place (a runtime-specific string).
#[derive(Serialize, Clone, PartialEq, Debug, Default)]
pub struct Place {
    /// The agent running there, if any (a plain shell has none).
    pub agent: Option<RuntimeAgent>,
    pub group: Option<RuntimeRef>,
}

/// A place with the processes at its root, as the ids its operating system gives them:
/// every process started at the place descends from one of them.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Root {
    pub place: String,
    /// The place's own shell, when the runtime knows it.
    pub shell: Option<u32>,
    /// What is in front there now: the root when no shell is known. It changes.
    pub front: Vec<u32>,
    /// What is at the place.
    pub at: Place,
}

impl Root {
    /// The processes a caller at this place descends from.
    pub fn pids(&self) -> Vec<u32> {
        match self.shell {
            Some(s) => vec![s],
            None => self.front.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("no agent runtime is connected")]
    None,
    #[error("no live agent {0}")]
    NoAgent(String),
    #[error("{0}")]
    Failed(String),
    /// The agent waits for a person in its terminal and takes no message now; the runtime's words.
    #[error("{0}")]
    Blocked(String),
    /// The agent is starting and takes no message yet; the runtime's words.
    #[error("{0}")]
    NotReady(String),
    /// A run's shell is no longer there, or its place is no longer a run's shell.
    #[error("{0}")]
    NoPlace(String),
}

/// Sync on purpose: the daemon calls it from a blocking task.
pub trait Runtime: Send + Sync {
    /// The live agents, read fresh.
    fn agents(&self) -> Result<Vec<RuntimeAgent>, RuntimeError>;
    /// What is at `place` on `machine` (none: this one), read fresh.
    fn locate(&self, machine: Option<&str>, place: &str) -> Result<Place, RuntimeError>;
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
    /// Each live agent on `machine` (none: this one) with the ids the operating system gives
    /// the processes at the root of its place (its shell): every process the agent starts
    /// descends from one of them. Read only when a machine alert needs it; by default, none.
    fn processes(&self, _machine: Option<&str>) -> Result<Vec<(RuntimeAgent, Vec<u32>)>, RuntimeError> {
        Ok(vec![])
    }
    /// The root of `place` on `machine` (none: this one), or of every place there when no
    /// place is named, read fresh; a place that is not there is not in the answer. This is
    /// how the service finds the place a calling process really runs at. By default, no
    /// place has a root the runtime knows.
    fn roots(&self, _machine: Option<&str>, _place: Option<&str>) -> Result<Vec<Root>, RuntimeError> {
        Ok(vec![])
    }
    /// The roots of the places on `machine` (none: this one) that hold an agent, read fresh:
    /// what the service reads on every tick. By default, `roots` of every place, kept to those.
    fn agent_roots(&self, machine: Option<&str>) -> Result<Vec<Root>, RuntimeError> {
        Ok(self.roots(machine, None)?.into_iter().filter(|r| r.at.agent.is_some()).collect())
    }
    /// Types `line` into a new plain shell of the user's (no agent) on `machine` (none: this
    /// one), in a place of its own inside the group named `group` (made on first use), started
    /// in `dir`, and submits it; returns the place. By default, no runtime can.
    fn run_shell(&self, _machine: Option<&str>, _group: &str, _dir: &str, _line: &str) -> Result<String, RuntimeError> {
        Err(RuntimeError::None)
    }
    /// What runs at a place `run_shell` returned, on `machine` (none: this one), as the ids its
    /// operating system gives them: the shell, and what is in front of it there (the shell
    /// itself while it waits at its prompt). `NoPlace` as for `close_shell`. By default, no
    /// runtime can.
    fn shell_processes(&self, _machine: Option<&str>, _group: &str, _place: &str) -> Result<(u32, Vec<u32>), RuntimeError> {
        Err(RuntimeError::None)
    }
    /// Closes a place `run_shell` returned, on `machine` (none: this one), and the shell in
    /// it with whatever still runs there: only while it is still a plain shell (no agent) in
    /// the group named `group`; one that is gone, or is anything else by now, is `NoPlace` and
    /// is left alone. By default, no runtime can.
    fn close_shell(&self, _machine: Option<&str>, _group: &str, _place: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::None)
    }
    /// As `start_temporary`, on `machine` (none: this one) in the group labelled `group`, made
    /// on first use as for `run_shell`. By default, no runtime can.
    fn start_temporary_at(&self, _machine: Option<&str>, _group: &str, _harness: &str, _name: &str, _cwd: &str) -> Result<RuntimeAgent, RuntimeError> {
        Err(RuntimeError::None)
    }
    /// The last `lines` lines a place on `machine` shows, as plain text.
    fn read_place(&self, _machine: Option<&str>, _place: &str, _lines: u32) -> Result<String, RuntimeError> {
        Err(RuntimeError::None)
    }
    /// Brings a place on `machine` to the front.
    fn focus_place(&self, _machine: Option<&str>, _place: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::None)
    }
    /// Every group there is, with or without agents; by default, those of the agents.
    fn groups(&self) -> Result<Vec<RuntimeRef>, RuntimeError> {
        let mut all: Vec<RuntimeRef> = vec![];
        for g in self.agents()?.into_iter().filter_map(|a| a.group) {
            if !all.contains(&g) {
                all.push(g);
            }
        }
        Ok(all)
    }
}

/// Used when no runtime is configured: no agents are known and none can be reached.
pub struct NoRuntime;

impl Runtime for NoRuntime {
    fn agents(&self) -> Result<Vec<RuntimeAgent>, RuntimeError> {
        Err(RuntimeError::None)
    }
    fn locate(&self, _: Option<&str>, _: &str) -> Result<Place, RuntimeError> {
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
    fn roots(&self, _: Option<&str>, _: Option<&str>) -> Result<Vec<Root>, RuntimeError> {
        Err(RuntimeError::None)
    }
}
