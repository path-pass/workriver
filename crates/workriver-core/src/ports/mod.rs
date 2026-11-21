// What core asks of the outside, in workriver's own words: contracts only, no adapter.
// (The runtime's is `runtime.rs`, at the crate's root since before there was a folder.)
mod data;
#[cfg(test)]
mod tests;

pub use data::{Committed, DataError, DataStatus, Delivered, Discussed, History, Recorded, Revision, WorkStore};
