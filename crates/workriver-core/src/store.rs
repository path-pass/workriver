// The one SQLite file: an append-only table of events, and the state rebuilt from it.
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, TransactionBehavior};

use crate::model::{Body, Event, Rejected, State};

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

/// Schema steps, applied in order; `PRAGMA user_version` counts how many ran.
const MIGRATIONS: &[&str] = &["
    CREATE TABLE events (
        seq INTEGER PRIMARY KEY,
        at TEXT NOT NULL,
        kind TEXT NOT NULL,
        actor TEXT NOT NULL,
        intent TEXT,
        unit TEXT,
        task TEXT,
        body TEXT NOT NULL CHECK (json_valid(body))
    );
    CREATE INDEX events_unit ON events(unit, seq);
    CREATE TRIGGER events_no_update BEFORE UPDATE ON events
        BEGIN SELECT RAISE(ABORT, 'events are append-only'); END;
    CREATE TRIGGER events_no_delete BEFORE DELETE ON events
        BEGIN SELECT RAISE(ABORT, 'events are append-only'); END;
"];

/// Where workriver keeps its data: `$WORKRIVER_HOME`, or `~/workriver-data`.
pub fn data_dir() -> PathBuf {
    match std::env::var_os("WORKRIVER_HOME") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => std::env::home_dir().unwrap_or_default().join("workriver-data"),
    }
}

/// Now, as the times in the data are written: `2026-11-16T03:04:05Z`.
pub fn now() -> String {
    humantime::format_rfc3339_seconds(std::time::SystemTime::now()).to_string()
}

pub struct Store {
    conn: Connection,
    state: State,
    path: PathBuf,
}

impl Store {
    /// Opens `workriver.db` in the data directory, creating both (0700 / 0600) if needed.
    pub fn open_default() -> Result<Store, Error> {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        let dir = data_dir();
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
        let path = dir.join("workriver.db");
        // Created here so it starts private; SQLite gives its -wal and -shm files the same mode.
        std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(&path)?;
        Store::open(&path)
    }

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
        let state = replay(&conn)?;
        Ok(Store { conn, state, path: path.to_path_buf() })
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn count(&self) -> Result<i64, Error> {
        Ok(self.conn.query_row("SELECT count(*) FROM events", [], |r| r.get(0))?)
    }

    /// Checks `body` against the rules, stores it and adds it to the state.
    pub fn append(&mut self, actor: &str, body: Body) -> Result<Event, Error> {
        let body = self.state.prepare(actor, body)?;
        let (intent, unit, task) = body.refs();
        let json = serde_json::to_value(&body).expect("events serialize");
        let at = now();
        self.conn.execute(
            "INSERT INTO events (at, kind, actor, intent, unit, task, body) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            (&at, json["kind"].as_str(), actor, intent, unit, task, json["body"].to_string()),
        )?;
        let e = Event { seq: self.conn.last_insert_rowid(), at, actor: actor.to_string(), body };
        self.state.apply(&e);
        Ok(e)
    }
}

/// The state, rebuilt by adding every stored event in order.
// ponytail: full replay at every start; add snapshots when it gets slow.
fn replay(conn: &Connection) -> Result<State, Error> {
    let mut state = State::default();
    let mut stmt = conn.prepare("SELECT seq, at, actor, kind, body FROM events ORDER BY seq")?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let seq: i64 = r.get(0)?;
        let kind: String = r.get(3)?;
        let body: String = r.get(4)?;
        let body = serde_json::from_str(&body)
            .and_then(|b: serde_json::Value| serde_json::from_value(serde_json::json!({ "kind": kind, "body": b })))
            .map_err(|err| Error::Corrupt { seq, err })?;
        state.apply(&Event { seq, at: r.get(1)?, actor: r.get(2)?, body });
    }
    Ok(state)
}
