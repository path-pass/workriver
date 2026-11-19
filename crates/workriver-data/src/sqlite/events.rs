// An event as a stored row, both ways: written (its kind, its body without the kind, and its
// refs for the indexed columns) and read back, each row decoded as it was written and
// applied in order.
use rusqlite::Connection;

use workriver_core::{Body, Event, State};

use super::Error;

/// Writes one event as a row; its sequence number.
pub(super) fn insert(conn: &Connection, at: &str, actor: &str, body: &Body) -> Result<i64, Error> {
    let (intent, unit, task) = body.refs();
    let json = serde_json::to_value(body).expect("events serialize");
    conn.execute(
        "INSERT INTO events (at, kind, actor, intent, unit, task, body) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        (at, json["kind"].as_str(), actor, intent, unit, task, json["body"].to_string()),
    )?;
    Ok(conn.last_insert_rowid())
}

/// The state, rebuilt by adding every stored event in order.
// ponytail: full replay at every start; add snapshots when it gets slow.
pub(super) fn replay(conn: &Connection) -> Result<State, Error> {
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
