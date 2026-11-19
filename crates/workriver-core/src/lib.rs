//! workriver's core: the events, the state they add up to, the write rules and the runtime
//! interface. Knows nothing about the network, the UI, storage or Herdr (the SQLite store
//! is workriver-data).
use std::path::PathBuf;

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

/// Where workriver keeps its data: `$WORKRIVER_HOME`, or `~/workriver-data`.
pub fn data_dir() -> PathBuf {
    match std::env::var_os("WORKRIVER_HOME") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => std::env::home_dir().unwrap_or_default().join("workriver-data"),
    }
}

/// Now, as the times in the data are written: `2026-11-16T03:04:05Z`.
pub fn now() -> String {
    humantime::format_rfc3339_seconds(std::time::SystemTime::now()).to_string()
}
