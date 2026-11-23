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

/// Who records a seeded event: an active intent is the user's to add, and so is a unit of
/// an intent with no manager; the rest agent-a's.
fn by(b: &Body) -> Principal {
    if matches!(b, Body::IntentCreated { .. } | Body::UnitCreated { .. }) { Principal::User } else { agent_a() }
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
        json!({"kind": "project.added", "body": {"path": "/p", "title": "P"}}),
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
    assert_eq!(got.iter().map(|e| e.seq).collect::<Vec<_>>(), [2, 3, 4, 6, 7, 10, 11], "its own (after the project's event), i2's and i3's: no move, nothing of i4");
    let all = s.history(workriver_core::ports::Revision(0), 100).unwrap().events;
    assert_eq!(intent_versions(&got, "i1"), intent_versions(&all, "i1"));
    assert_eq!(intent_text(&got, "i1", 2), intent_text(&all, "i1", 2));
    assert_eq!(intent_text(&got, "i1", 3).unwrap().note, s.state().intent("i1").unwrap().note);
    assert!(s.intent_events("i9").unwrap().is_empty());
}

/// An intent's kind, size and tags through the stored rows: set, cleared (null, []) and left
/// out each come back as they were written, after the store is opened again, in the state, its
/// history and a version's text; an unrelated edit keeps them.
#[test]
fn kind_size_and_tags_survive_storing_and_reopening() {
    use workriver_core::ports::WorkStore;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("w.db");
    {
        let mut s = Store::open(&path).unwrap();
        for v in [
            json!({"kind": "project.added", "body": {"path": "/p", "title": "P"}}),
            json!({"kind": "intent.created", "body": {"title": "Login", "project": "/p", "state": "planned", "kind": "fix", "size": "s", "tags": ["Security"]}}),
            json!({"kind": "intent.updated", "body": {"intent": "i1", "goal": "No lockout", "base": 1}}),
            json!({"kind": "intent.updated", "body": {"intent": "i1", "kind": null, "tags": [], "base": 2}}),
            json!({"kind": "intent.updated", "body": {"intent": "i1", "kind": "feature", "base": 3}}),
        ] {
            s.append(&Principal::User, body(v)).unwrap();
        }
    }
    let s = Store::open(&path).unwrap();
    let i = s.state().intent("i1").unwrap();
    assert_eq!((i.kind, i.size, i.tags.len(), i.goal.as_str()), (Some(IntentKind::Feature), Some(IntentSize::S), 0, "No lockout"));
    let all = s.history(workriver_core::ports::Revision(0), 100).unwrap().events;
    let Body::IntentUpdated { kind, size, tags, .. } = &all[3].body else { panic!() };
    assert_eq!((kind, size, tags), (&Some(None), &None, &Some(vec![])), "null and [] are read back as written, an absent size as absent");
    let Body::IntentUpdated { kind, tags, .. } = &all[2].body else { panic!() };
    assert_eq!((kind, tags), (&None, &None), "an unrelated edit carries none of them");
    let got = s.intent_events("i1").unwrap();
    let seen: Vec<_> = intent_versions(&got, "i1").iter().map(|v| (v.kind, v.tags.clone())).collect();
    assert_eq!(seen, [(Some(Some(IntentKind::Fix)), Some(vec!["security".to_string()])), (None, None), (Some(None), Some(vec![])), (Some(Some(IntentKind::Feature)), None)]);
    let v1 = intent_text(&got, "i1", 1).unwrap();
    assert_eq!((v1.kind, v1.size, v1.tags), (Some(IntentKind::Fix), Some(IntentSize::S), vec!["security".to_string()]));
}

/// A file as the version before virtual workspaces left it: the first schema step only.
fn before_the_format(path: &std::path::Path, events: usize) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(super::migrations::MIGRATIONS[0]).unwrap();
    conn.pragma_update(None, "user_version", 1).unwrap();
    for _ in 0..events {
        conn.execute("INSERT INTO events (at, kind, actor, body) VALUES ('2026-01-01T00:00:00Z', 'intent.created', 'user', '{\"id\":\"i1\",\"title\":\"Old\"}')", []).unwrap();
    }
}

fn version(path: &std::path::Path) -> (i64, bool) {
    let conn = rusqlite::Connection::open(path).unwrap();
    let marked = conn.query_row("SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 'format')", [], |r| r.get(0)).unwrap();
    (conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap(), marked)
}

/// The product reads only the new form: a store with events and without the format's step
/// is refused, naming the script and the commit that holds it, and opening it does not mark it; a new store, and one
/// that holds no events yet, is marked.
#[test]
fn a_store_without_the_format_mark_is_refused_and_never_marked() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.db");
    before_the_format(&old, 1);
    for _ in 0..2 {
        let err = Store::open(&old).err().expect("refused").to_string();
        assert!(err.contains("scripts/migrate-virtual-workspaces as commit 7cdf576") && err.contains("Nothing was changed"), "{err}");
        assert_eq!(version(&old), (1, false), "left as it is");
    }
    let empty = dir.path().join("empty.db");
    before_the_format(&empty, 0);
    Store::open(&empty).unwrap();
    assert_eq!(version(&empty), (2, true), "nothing to migrate: marked");
    let new = dir.path().join("new.db");
    let mut s = Store::open(&new).unwrap();
    assert_eq!(version(&new), (2, true));
    s.append(&Principal::User, body(json!({"kind": "intent.created", "body": {"title": "New", "state": "planned"}}))).unwrap();
    drop(s);
    assert_eq!(Store::open(&new).unwrap().state().intents.len(), 1, "a marked store with events opens");
}
