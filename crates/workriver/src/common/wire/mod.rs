// What the service and the CLI (and the page) exchange: values and shapes only, no logic.
mod machines;
mod message;
mod metrics;
mod task;
pub mod workspace;

pub use machines::*;
pub use message::*;
pub use metrics::*;
pub use task::*;

/// What the service answers a proof it no longer has, in the header the proof came in: the
/// CLI then fetches a new one and repeats its request.
pub const EXPIRED: &str = "expired";

/// The largest attachment taken, by the service and (before sending it) the CLI: core's
/// number, which its rules hold a recorded size to.
/// ponytail: raise it, or stream uploads, if real files need more.
pub const MAX_ATTACHMENT: usize = workriver_core::MAX_ATTACHMENT;
