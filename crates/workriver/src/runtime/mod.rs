// Outside programs that run the agents, behind one facade: how the service gets its runtime,
// where this process runs, and the sentences that name the runtime's own things. The one
// adapter today is picked here; nothing outside this folder imports it.
use std::sync::Arc;

use workriver_core::runtime::Runtime;

use crate::common::config;

pub mod follow;
mod acp;
mod herdr;
pub mod ssh;

pub(crate) fn host(dir: &std::path::Path) -> anyhow::Result<()> {
    acp::run(dir)
}

/// Today's only adapter.
use herdr as adapter;

/// The adapter's sentences: content only, never read back for who someone is.
pub use adapter::words;

/// This machine's runtime, and a line on what is wrong with it when it does not answer as
/// expected (the service still starts).
pub fn connect(terminal_app: String) -> (Arc<dyn Runtime>, Option<String>) {
    adapter::connect(terminal_app)
}

/// Where this process runs, as the runtime's environment names it: an opaque place, sent
/// along to be cross-checked only.
pub fn caller_place() -> Option<String> {
    adapter::place_from_env()
}

/// What the adapter keeps for a followed machine, read and checked by its own rules; the
/// settings with it: here the settings of common/ meet today's adapter.
pub type MachineOptions = adapter::remote::Options;
pub type Setting = config::Setting<MachineOptions>;
pub type Settings = config::Settings<MachineOptions>;

/// A machine's options with where its runtime's socket is over there (none: asked of the
/// machine).
#[cfg(test)]
pub fn socket_option(socket: Option<String>) -> MachineOptions {
    MachineOptions { socket }
}

/// The sockets of a followed machine's runtime that its connection brings here, each as
/// (here, there), for its runtime socket over there.
pub fn forwards(home: &std::path::Path, label: &str, there: &str) -> Vec<(String, String)> {
    adapter::remote::forwards(home, label, there)
}

/// The same as `machine add` sends them to the service, to go into its request.
pub fn socket_request(socket: Option<String>) -> serde_json::Value {
    adapter::remote::request(socket)
}

/// A group's id on a machine, told apart from this machine's: `w1@label`.
pub fn remote_ref(id: &str, machine: &str) -> String {
    format!("{id}@{machine}")
}
