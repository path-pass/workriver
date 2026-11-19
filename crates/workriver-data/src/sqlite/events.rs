// Reading the stored events back: each row decoded as it was written, applied in order.
use rusqlite::Connection;

use workriver_core::{Event, State};

use super::Error;

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
