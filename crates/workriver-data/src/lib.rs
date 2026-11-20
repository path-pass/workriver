//! workriver's storage: one SQLite file, an append-only table of events, and the state
//! rebuilt from it through core's rules. `Store` implements core's data port
//! (`workriver_core::ports::WorkStore`), which is all the service knows of it once it is
//! opened; nothing else knows SQL.
mod sqlite;
#[cfg(test)]
mod tests;

pub use sqlite::{Error, Store};
