// The one SQLite file: an append-only table of events, and the state rebuilt from it. It
// keeps the work as core's data port asks (`WorkStore`): nothing of SQLite leaves this folder.
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{Connection, TransactionBehavior};

use workriver_core::ports::{Committed, DataError, DataStatus, History, Recorded, Revision, WorkStore};
use workriver_core::{Body, Event, Principal, Rejected, State, now};

mod events;
mod migrations;

use events::{insert, read, replay};
use migrations::MIGRATIONS;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Rejected(#[from] Rejected),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("stored event {seq} cannot be read: {err}")]
    Corrupt { seq: i64, err: serde_json::Error },
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// What the port says of a failure: a broken rule stays one, an unreadable row is corrupt,
/// anything else (SQLite, the file) is the storage not available, in the same words.
impl From<Error> for DataError {
    fn from(e: Error) -> DataError {
        match e {
            Error::Rejected(r) => DataError::Rejected(r),
            e @ Error::Corrupt { .. } => DataError::Corrupt(e.to_string()),
            e => DataError::Unavailable(e.to_string()),
        }
    }
}

pub struct Store {
    conn: Connection,
    /// What the stored events add up to. Shared with whoever read it (`recorded`): it is
    /// copied before the next event is added while a reader still holds it.
    state: Arc<State>,
    /// The number of the last stored event.
    last: i64,
    path: PathBuf,
}

impl Store {

    pub fn open(path: &Path) -> Result<Store, Error> {
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.busy_timeout(Duration::from_secs(5))?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let ran: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (k, sql) in MIGRATIONS.iter().enumerate().skip(ran as usize) {
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", k as i64 + 1)?;
        }
        tx.commit()?;
        let (state, last) = replay(&conn)?;
        Ok(Store { conn, state: Arc::new(state), last, path: path.to_path_buf() })
    }

    /// The state as the store holds it, for whoever has the store itself (tests that open
    /// the file beside a service). It is not on the port: the service cannot ask for it.
    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn count(&self) -> Result<i64, Error> {
        Ok(self.conn.query_row("SELECT count(*) FROM events", [], |r| r.get(0))?)
    }

    /// Checks `body` against the rules, stores it and adds it to the state.
    /// `who` is who the caller was found to be, never who it says it is.
    fn append(&mut self, who: &Principal, body: Body) -> Result<Event, Error> {
        let body = self.state.prepare(who, body)?;
        let actor = who.actor();
        let at = now();
        let seq = insert(&self.conn, &at, actor, &body)?;
        let e = Event { seq, at, actor: actor.to_string(), body };
        Arc::make_mut(&mut self.state).apply(&e);
        self.last = seq;
        Ok(e)
    }

    fn revision(&self) -> Revision {
        Revision(self.last as u64)
    }
}

impl WorkStore for Store {
    fn recorded(&self) -> Recorded {
        Recorded::new(self.revision(), self.state.clone())
    }

    fn record(&mut self, who: &Principal, request: Body) -> Result<Committed, DataError> {
        let event = self.append(who, request)?;
        Ok(Committed { event, revision: self.revision() })
    }

    fn history(&self, after: Revision, limit: usize) -> Result<History, DataError> {
        let events = read(&self.conn, i64::try_from(after.0).unwrap_or(i64::MAX), i64::try_from(limit).unwrap_or(i64::MAX))?;
        let next = events.last().map_or(after, |e| Revision(e.seq as u64));
        Ok(History { events, next })
    }

    fn status(&self) -> Result<DataStatus, DataError> {
        Ok(DataStatus { location: self.path.clone(), events: self.count()? as u64, revision: self.revision() })
    }
}

#[cfg(test)]
mod tests;
