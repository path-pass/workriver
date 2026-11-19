//! workriver's storage: one SQLite file, an append-only table of events, and the state
//! rebuilt from it through core's rules. The service opens it; nothing else knows SQL.
mod sqlite;

pub use sqlite::{Error, Store};
