// The defaults a user or a later version may tune, in one place. A default that belongs to
// a stored type's shape stays on that type (its serde default).

/// A ready unit is merged within a working day when it goes through push, pull request and
/// merge (each a gate the user answers), and a merge git sees is recorded within seconds; a
/// day without one is a squash merge git cannot see, or a pull request stuck or abandoned.
/// Any less would ask again about a pull request left overnight for review.
pub const READY_HOURS: u32 = 24;

/// In the records of real use a worker got its next task a median 4 minutes after it
/// delivered, 15 times in 21 within 30 minutes; the other 6 waited 37 minutes to 15 hours.
/// Thirty stays quiet through the usual quick hand-on and tells of a worker left holding
/// memory for hours; telling is all it does, the manager decides.
pub const IDLE_MINUTES: u32 = 30;

/// See the service's `delivery/reminders.rs`: why ten.
pub const STALL_MINUTES: u32 = 10;
/// The most the stall threshold may be set to: four hours.
pub const STALL_MAX: u32 = 240;

/// Anything slower than this is marked so in the timing log.
pub const SLOW_MS: u64 = 500;
