// The SQLite file itself: what it keeps, that it only grows, and that what an old database
// holds reopens to the state core's rules add it up to.
use serde_json::{Value, json};
use workriver_core::*;

use super::Store;

fn body(v: Value) -> Body {
    serde_json::from_value(v).unwrap()
}

/// The agent the seeded events are recorded as.
fn agent_a() -> Principal {
    Principal::Agent { session: "agent-a".into(), machine: None }
}

/// Who records a seeded event: an active intent is the user's to add, the rest agent-a's.
fn by(b: &Body) -> Principal {
    if matches!(b, Body::IntentCreated { .. }) { Principal::User } else { agent_a() }
}

fn seed() -> Vec<Body> {
    let steps = [
        json!({"kind": "intent.created", "body": {"title": "Chat v2"}}),
        json!({"kind": "unit.created", "body": {"intent": "i1", "title": "Mentions", "repo": "/r", "branch": "feat/mention", "base": "main"}}),
        json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Find mentions"}}),
        json!({"kind": "task.started", "body": {"unit": "u1", "task": 1}}),
        json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1, "evidence": [{"command": "npm test", "result": "passed", "summary": "6 passed"}]}}),
        json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "inputs": [{"from": "1"}]}}),
        json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 2, "findings": [
            {"severity": "P2", "problem": "Duplicates", "impact": "Two notices", "where": ["app/mention.ts"]},
            {"severity": "P2", "problem": "Emoji names", "impact": "Not found", "where": ["app/mention.ts"]},
        ]}}),
    ];
    steps.into_iter().map(body).collect()
}

#[test]
fn replay_rebuilds_the_same_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("w.db");
    let before = {
        let mut s = Store::open(&path).unwrap();
        for b in seed() {
            s.append(&by(&b), b).unwrap();
        }
        s.state().clone()
    };
    let again = Store::open(&path).unwrap();
    assert_eq!(again.state(), &before);
    assert_eq!(again.count().unwrap(), 7);
}

#[test]
fn events_are_append_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("w.db");
    let mut s = Store::open(&path).unwrap();
    for b in seed() {
        s.append(&by(&b), b).unwrap();
    }
    let conn = rusqlite::Connection::open(&path).unwrap();
    for sql in ["UPDATE events SET actor = 'x'", "DELETE FROM events"] {
        let err = conn.execute(sql, []).unwrap_err().to_string();
        assert!(err.contains("append-only"), "{sql}: {err}");
    }
}

/// A file of core's replay fixtures, read where it is (never copied).
fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../workriver-core/src/records/fixtures").join(name)
}

/// Core's synthetic history (`events.jsonl`), one stored row per line, decoded as replay
/// decodes a row.
fn history() -> Vec<Event> {
    let text = std::fs::read_to_string(fixture("events.jsonl")).unwrap();
    text.lines()
        .map(|line| {
            let row: Value = serde_json::from_str(line).unwrap();
            let body = serde_json::from_value(json!({ "kind": row["kind"], "body": row["body"] })).unwrap_or_else(|e| panic!("event {}: {e}", row["seq"]));
            Event { seq: row["seq"].as_i64().unwrap(), at: row["at"].as_str().unwrap().into(), actor: row["actor"].as_str().unwrap().into(), body }
        })
        .collect()
}

/// The history added up with core's `apply` alone.
fn folded(events: &[Event]) -> State {
    let mut state = State::default();
    for e in events {
        state.apply(e);
    }
    state
}

/// The same history stored as rows of an old database: opened, it replays to the same state,
/// keeps every row, and a reopened store reads it again identically.
#[test]
fn the_stored_history_reopens_to_the_same_state() {
    let events = history();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("w.db");
    drop(Store::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    for e in &events {
        let (intent, unit, task) = e.body.refs();
        let v = serde_json::to_value(&e.body).unwrap();
        let line: Value = serde_json::from_str(std::fs::read_to_string(fixture("events.jsonl")).unwrap().lines().nth(e.seq as usize - 1).unwrap()).unwrap();
        // The body as the old database wrote it, not as it is written today.
        conn.execute("INSERT INTO events (seq, at, kind, actor, intent, unit, task, body) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)", (e.seq, &e.at, v["kind"].as_str(), &e.actor, intent, unit, task, line["body"].to_string())).unwrap();
    }
    drop(conn);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.count().unwrap(), events.len() as i64);
    assert_eq!(store.state(), &folded(&events));
    drop(store);
    assert_eq!(Store::open(&path).unwrap().state(), &folded(&events));
}

/// An intent's history is read from its own events and those of the intents merged into
/// it, transitively; nothing else. Rebuilt from them, it is what the whole record gives.
#[test]
fn an_intents_history_is_read_with_what_was_merged_into_it() {
    use workriver_core::ports::WorkStore;
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(&dir.path().join("w.db")).unwrap();
    for v in [
        json!({"kind": "intent.created", "body": {"title": "Search", "project": "/p", "state": "planned"}}),
        json!({"kind": "intent.created", "body": {"title": "Emoji", "note": "By name", "project": "/p", "state": "planned"}}),
        json!({"kind": "intent.created", "body": {"title": "Codes", "project": "/p", "state": "planned"}}),
        json!({"kind": "intent.created", "body": {"title": "Elsewhere", "project": "/p", "state": "planned"}}),
        json!({"kind": "intent.updated", "body": {"intent": "i3", "add_note": "Shortcodes"}}),
        json!({"kind": "intent.merged", "body": {"intent": "i2", "from": ["i3"]}}),
        json!({"kind": "intent.updated", "body": {"intent": "i4", "add_note": "Not ours"}}),
        json!({"kind": "intent.moved", "body": {"intent": "i1", "before": "i2"}}),
        json!({"kind": "intent.merged", "body": {"intent": "i1", "from": ["i2"], "title": "Search and emoji"}}),
        json!({"kind": "intent.updated", "body": {"intent": "i1", "add_note": "After"}}),
    ] {
        s.append(&Principal::User, body(v)).unwrap();
    }
    let got = s.intent_events("i1").unwrap();
    assert_eq!(got.iter().map(|e| e.seq).collect::<Vec<_>>(), [1, 2, 3, 5, 6, 9, 10], "its own, i2's and i3's: no move, nothing of i4");
    let all = s.history(workriver_core::ports::Revision(0), 100).unwrap().events;
    assert_eq!(intent_versions(&got, "i1"), intent_versions(&all, "i1"));
    assert_eq!(intent_text(&got, "i1", 2), intent_text(&all, "i1", 2));
    assert_eq!(intent_text(&got, "i1", 3).unwrap().note, s.state().intent("i1").unwrap().note);
    assert!(s.intent_events("i9").unwrap().is_empty());
}
