//! workriver's core: the events, the state they add up to, the write rules, the runtime
//! interface and the data port. Knows nothing about the network, the UI, storage or the
//! agent runtime (what keeps the work implements `ports::WorkStore`: workriver-data, the
//! SQLite store; the data directory is the outer crate's common/home.rs).
pub mod access;
pub mod choices;
pub mod conversations;
pub mod hosted;
pub mod planning;
pub mod ports;
pub mod records;
pub mod runtime;
pub mod screens;
pub mod work;

pub use access::*;
pub use choices::*;
pub use conversations::*;
pub use planning::*;
pub use hosted::HostedAgent;
pub use records::*;
pub use runtime::RuntimeRef;
pub use work::*;

/// Now, as the times in the data are written: `2026-11-16T03:04:05Z`.
pub fn now() -> String {
    humantime::format_rfc3339_seconds(std::time::SystemTime::now()).to_string()
}
