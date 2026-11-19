//! workriver's core: the events, the state they add up to, the write rules and the runtime
//! interface. Knows nothing about the network, the UI, storage or Herdr (the SQLite store
//! is workriver-data; the data directory is the outer crate's common/home.rs).
pub mod access;
pub mod conversations;
pub mod planning;
pub mod records;
pub mod runtime;
pub mod work;

pub use access::*;
pub use conversations::*;
pub use planning::*;
pub use records::*;
pub use work::*;

/// Now, as the times in the data are written: `2026-11-16T03:04:05Z`.
pub fn now() -> String {
    humantime::format_rfc3339_seconds(std::time::SystemTime::now()).to_string()
}
