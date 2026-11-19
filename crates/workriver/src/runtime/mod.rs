// Outside programs that run the agents, behind one facade: how the service gets its runtime,
// where this process runs, and the sentences that name the runtime's own things. The one
// adapter today is picked here; nothing outside this folder imports it.
use std::sync::Arc;

use workriver_core::runtime::Runtime;

mod herdr;

/// Today's only adapter.
use herdr as adapter;

/// The adapter's sentences: content only, never read back for who someone is.
pub use adapter::words;

/// This machine's runtime, and a line on what is wrong with it when it does not answer as
/// expected (the service still starts).
pub fn connect(terminal_app: String) -> (Arc<dyn Runtime>, Option<String>) {
    let runtime = adapter::Herdr::new(adapter::socket_path(), terminal_app);
    let warning = runtime.check();
    (Arc::new(runtime), warning)
}

/// Where this process runs, as the runtime's environment names it: an opaque place, sent
/// along to be cross-checked only.
pub fn caller_place() -> Option<String> {
    adapter::place_from_env()
}

/// The runtime of another machine, behind a socket forwarded from it.
pub fn remote(socket: std::path::PathBuf, machine: &str, terminal_app: String) -> Arc<dyn Runtime> {
    Arc::new(adapter::Herdr::on(socket, machine, terminal_app))
}

/// A group's id on a machine, told apart from this machine's: `w1@label`.
pub fn remote_ref(id: &str, machine: &str) -> String {
    format!("{id}@{machine}")
}
