//! workriver's core: the events, the state they add up to, the write rules, the SQLite
//! store and the runtime interface. Knows nothing about the network, the UI or Herdr.
pub mod model;
pub mod runtime;
pub mod store;

pub use model::*;
pub use store::{Error, Store, data_dir, now};

#[cfg(test)]
mod tests;
