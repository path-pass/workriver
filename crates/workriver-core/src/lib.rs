//! workriver's core: the events, the state they add up to, the write rules, the SQLite
//! store and the runtime interface. Knows nothing about the network, the UI or Herdr.
pub mod access;
pub mod planning;
pub mod records;
pub mod runtime;
pub mod store;

pub use access::*;
pub use planning::*;
pub use records::*;
pub use store::{Error, Store, data_dir, now};

#[cfg(test)]
mod tests;
