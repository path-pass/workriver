// A followed machine as the service reports it: to the page, and to `workriver machine list`.
use serde::Serialize;

/// A machine as the page and `workriver machine list` show it.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MachineState {
    pub label: String,
    pub ssh: String,
    pub enabled: bool,
    /// The port over there that leads here, while connected (written there to
    /// `~/.config/workriver/port`).
    pub port: Option<u16>,
    /// The connection is down: since when (RFC 3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub down_since: Option<String>,
    /// The next attempt to connect, in seconds from now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_in: Option<u64>,
    /// Its agents can be read now.
    pub reachable: bool,
    /// Why not, in ssh's or the runtime's words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The first attempt to connect has not finished yet: neither up nor failed.
    pub connecting: bool,
    /// The connection (both forwards) is up.
    pub tunnel: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tunnel_error: Option<String>,
    pub agents: usize,
}
