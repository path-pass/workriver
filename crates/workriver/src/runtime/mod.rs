// Outside programs that run the agents, behind one facade: how the service gets its runtime,
// where this process runs, and the sentences that name the runtime's own things. The one
// adapter today is picked here; nothing outside this folder imports it.
use std::sync::Arc;

use workriver_core::runtime::Runtime;

use crate::common::config::Options;

pub mod follow;
mod herdr;
pub mod ssh;

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

/// A machine's options with where its runtime's socket is over there (none: asked of the
/// machine), as the adapter keeps them in the machine's setting.
pub fn socket_option(socket: Option<String>) -> Options {
    adapter::remote::options(socket)
}

/// Of the options a request gave for a machine, those the adapter knows.
pub fn known_options(given: &Options) -> Options {
    adapter::remote::known(given)
}

/// A group's id on a machine, told apart from this machine's: `w1@label`.
pub fn remote_ref(id: &str, machine: &str) -> String {
    format!("{id}@{machine}")
}
