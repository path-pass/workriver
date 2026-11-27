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

/// How many units count as open on the page (the last whose code it read): their uncommitted
/// work is read with its texts in every snapshot that reads it again, so their line counts are
/// there. A page keeps the code of its last two units; two pages, four.
pub const OPENED_UNITS: usize = 4;

/// Anything slower than this is marked so in the timing log.
pub const SLOW_MS: u64 = 500;

/// A task's session file not found by its id (moved, archived elsewhere, deleted) is looked
/// for again after five minutes, not on every snapshot: each look lists the harness's
/// folders, which grow with every session it kept. A file that turns up shows within that.
pub const SESSION_RECHECK_SECS: u64 = 300;

/// How long the service goes on reading the body of a request it has answered without
/// reading all of it (a refusal from the headers alone): as long as the CLI waits for an
/// upload, the slowest request it sends. A client still sending after that has given up.
pub const LEFT_BODY_SECS: u64 = 120;

/// How many terminal views may be open at once, across every page: one observe process each
/// (about 9 MB), and a working terminal costs its runtime about 3 % of a core to stream; a
/// page shows one at a time.
pub const VIEWS_MAX: usize = 4;
/// How many frames of a terminal view wait for a slow page before its stream is ended (it is
/// opened again from a whole frame).
pub const VIEW_QUEUE: usize = 256;
/// A frame larger than this (base64) is not passed on, and its stream ends there (the changes
/// after it would be drawn on nothing): a whole 120×40 screen is 5–10 KB.
pub const VIEW_FRAME_MAX: usize = 1024 * 1024;
/// How many frames a runtime's terminal stream reads ahead of the view that takes them.
pub const VIEW_READ_AHEAD: usize = 64;
