//! A synthetic history, as old and new databases hold it, replayed to the state it has
//! always added up to. The guard for the restructure (docs/design/restructure.md): no
//! event, default or serde attribute may change what a stored record means.
//!
//! `records/fixtures/events.jsonl` is the history, one stored event per line: old
//! encodings first (no intent state, a task with no intent, an intent-level task, a
//! decision with no `by`, a branch named before the rule), then every event kind of today.
//! `records/fixtures/expected-state.json` is what it adds up to: the state, the command
//! runs (never serialized with it), the blind view, how each event is written again, and
//! the ids the rules hand out next. To write it again after a deliberate change of
//! meaning (never during a move): `WORKRIVER_BLESS=1 cargo test -p workriver-core replay`.
use serde_json::{Value, json};

use crate::*;

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/records/fixtures").join(name)
}

/// The fixture's events, decoded as replay decodes a stored row (`kind` and `body` columns).
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

/// The history added up in order, with `apply` only (replay never runs today's rules).
fn folded(events: &[Event]) -> State {
    let mut state = State::default();
    for e in events {
        state.apply(e);
    }
    state
}

/// What the expected file holds for a state.
fn seen(state: &State, events: &[Event]) -> Value {
    let runs: Vec<Value> = state
        .runs
        .iter()
        .map(|r| json!({ "id": r.id, "command": r.command, "dir": r.dir, "machine": r.machine, "unit": r.unit, "by": r.by, "from": r.from, "step": r.step, "at": r.at, "exit": r.exit, "review": r.review.as_ref().map(|(v, t)| json!([v, t])) }))
        .collect();
    let next = |who: Principal, v: Value| match state.prepare(&who, serde_json::from_value(v).unwrap()) {
        Ok(b) => serde_json::to_value(b).unwrap(),
        Err(e) => json!({ "rejected": e.to_string() }),
    };
    json!({
        "state": state,
        "runs": runs,
        "blind": { "s-blind": state.blind_view("s-blind"), "s-i": state.blind_view("s-i") },
        "managers": { "i1": state.manager_of("i1"), "i2": state.manager_of("i2") },
        "encoded": events.iter().map(|e| serde_json::to_value(&e.body).unwrap()).collect::<Vec<_>>(),
        "next": [
            next(Principal::User, json!({ "kind": "intent.created", "body": { "title": "Next", "project": "/r/chat", "state": "planned" } })),
            next(Principal::User, json!({ "kind": "project.added", "body": { "path": "/r/other", "title": "Other" } })),
            next(Principal::User, json!({ "kind": "unit.created", "body": { "intent": "i2", "title": "More", "repo": "/r/chat", "branch": "feat/more", "base": "main" } })),
            next(Principal::User, json!({ "kind": "task.created", "body": { "unit": "u4", "kind": "implement", "title": "Again", "inputs": [{ "from": "2" }] } })),
            next(Principal::User, json!({ "kind": "task.created", "body": { "unit": "i1", "kind": "research", "title": "On the intent" } })),
            next(Principal::User, json!({ "kind": "chore.created", "body": { "intent": "i2", "title": "Another" } })),
            next(Principal::User, json!({ "kind": "finding.added", "body": { "project": "/r/chat", "finding": { "severity": "P3", "problem": "x", "impact": "y", "where": ["a"] } } })),
            next(Principal::User, json!({ "kind": "question.asked", "body": { "unit": "u2", "task": 4, "text": "And?" } })),
            next(Principal::User, json!({ "kind": "message.sent", "body": { "intent": "i2", "text": "Hello" } })),
            next(Principal::Agent { session: "s-r2".into(), machine: None }, json!({ "kind": "task.delivered", "body": { "unit": "u2", "task": 2 } })),
        ],
    })
}

#[test]
fn a_synthetic_history_replays_to_the_same_state() {
    let events = history();
    let state = folded(&events);
    let got = seen(&state, &events);
    let path = fixture("expected-state.json");
    if std::env::var("WORKRIVER_BLESS").is_ok_and(|v| v == "1") {
        std::fs::write(&path, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap();
    }
    let want: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for key in ["state", "runs", "blind", "managers", "encoded", "next"] {
        assert_eq!(serde_json::to_string_pretty(&got[key]).unwrap(), serde_json::to_string_pretty(&want[key]).unwrap(), "{key} differs from {}", path.display());
    }
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
