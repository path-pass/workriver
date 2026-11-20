// The data port: what the service asks of whatever keeps the work, in workriver's own
// words. The one implementation is workriver-data; nothing here names it, and nothing of a
// database crosses: no SQL, connection, transaction, table or row id.
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use crate::access::{Principal, Rejected};
use crate::records::{Body, Event, State};
use crate::{Question, Task, Unit};

/// How far the record has got: the number of its last committed change (that event's
/// `seq`), 0 before the first. History is read by it too: it is the cursor.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct Revision(pub u64);

/// Everything recorded, as it was at one revision. It is the reader's own: what is recorded
/// afterwards does not change it, and it borrows nothing of the store that gave it.
#[derive(Clone, PartialEq, Debug)]
pub struct Recorded {
    revision: Revision,
    state: Arc<State>,
}

impl Recorded {
    /// For a store: its state at `revision`. Sharing it costs nothing; the store copies its
    /// own before changing it while a reader still holds this one.
    pub fn new(revision: Revision, state: Arc<State>) -> Recorded {
        Recorded { revision, state }
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    /// The state itself, to hand on or change (a blind view, `/api/state`).
    pub fn into_state(self) -> State {
        Arc::unwrap_or_clone(self.state)
    }
}

impl Deref for Recorded {
    type Target = State;

    fn deref(&self) -> &State {
        &self.state
    }
}

/// A request that was recorded: the event as it was committed (ids and numbers given by the
/// rules, the time, who it was recorded as), and the revision it made.
#[derive(Clone, PartialEq, Debug)]
pub struct Committed {
    pub event: Event,
    pub revision: Revision,
}

/// A stretch of the history, oldest first, and where to read on from.
#[derive(Clone, PartialEq, Debug)]
pub struct History {
    pub events: Vec<Event>,
    /// The revision of the last event given (`after` itself when there was none).
    pub next: Revision,
}

/// What the storage says of itself, for `workriver status`.
#[derive(Clone, PartialEq, Debug)]
pub struct DataStatus {
    /// Where the record is kept, as its storage names it (a file).
    pub location: PathBuf,
    /// How many events are stored.
    pub events: u64,
    pub revision: Revision,
}

/// Why the store did not do what was asked. The texts are shown as they are.
#[derive(Debug, thiserror::Error)]
pub enum DataError {
    /// The request breaks a rule: the caller's doing. Nothing was written.
    #[error(transparent)]
    Rejected(#[from] Rejected),
    /// The storage could not be read or written; in the storage's words.
    #[error("{0}")]
    Unavailable(String),
    /// What is stored cannot be read as what was recorded.
    #[error("{0}")]
    Corrupt(String),
}

/// Keeps the work: reads of it at one revision, the one way to record, its history and what
/// the storage says of itself.
///
/// The reads are the service's own. Who asks and what they may see is settled before a read
/// (the service's `App::who` and `App::view`, which cuts a blind agent's view with core's
/// rule); no read takes a caller's word, and none is answered to a request directly.
///
/// A read does not fail: a store answers from what it holds at its revision. One that had to
/// go to its storage for a read would change this contract, not hide an error behind it.
pub trait WorkStore: Send {
    /// Everything recorded, at the revision of this call: for what needs the whole of it
    /// (the page, a message's plan, the reminders, what a previous run left).
    fn recorded(&self) -> Recorded;

    /// Records `request` as `who`: checked against the rules on what is recorded now
    /// (`State::prepare`), then committed and added. `who` is who the caller was found to
    /// be, never who it says it is. A refused or failed request changes nothing: not the
    /// record, not what is read, not the next id. There is no other way to write.
    fn record(&mut self, who: &Principal, request: Body) -> Result<Committed, DataError>;

    /// Up to `limit` events recorded after `after`, oldest first.
    fn history(&self, after: Revision, limit: usize) -> Result<History, DataError>;

    fn status(&self) -> Result<DataStatus, DataError>;

    // ---- one record, by its id: a store need not put the whole work together for these ----

    fn unit(&self, id: &str) -> Result<Unit, Rejected> {
        self.recorded().unit(id).cloned()
    }

    fn task(&self, unit: &str, n: u32) -> Result<Task, Rejected> {
        self.recorded().task(unit, n).cloned()
    }

    fn question(&self, id: &str) -> Result<Question, Rejected> {
        self.recorded().question(id).cloned()
    }

    /// The name the agent of `session` goes by, if it was ever seen.
    fn agent_name(&self, session: &str) -> Option<String> {
        self.recorded().name_of(session).map(str::to_string)
    }
}
