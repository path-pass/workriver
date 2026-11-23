//! A synthetic history, as old and new databases hold it, replayed to the state it has
//! always added up to. A guard (docs/design/guards.md): no
//! event, default or serde attribute may change what a stored record means.
//!
//! `records/fixtures/events.jsonl` is the history, one stored event per line: old
//! encodings first (no intent state, a task with no intent, a decision with no `by`, a branch named before the rule), then every event kind of today.
//! `records/fixtures/expected-state.json` is what it adds up to: the state, the command
//! runs and the intents' versions (never serialized with it), the blind view, how each event is written again, and
//! the ids the rules hand out next. To write it again after a deliberate change of
//! meaning (never during a move): `WORKRIVER_BLESS=1 cargo test -p workriver-core replay`.
//! The same history stored as rows of an old database is workriver-data's test.
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
        "edits": state.intents.iter().map(|i| (i.id.clone(), serde_json::to_value(&i.edits).unwrap())).collect::<serde_json::Map<_, _>>(),
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
            next(Principal::User, json!({ "kind": "question.asked", "body": { "unit": "u3", "task": 1, "text": "And a fourth?", "discuss": true } })),
            next(Principal::Agent { session: "s-r2".into(), machine: None }, json!({ "kind": "task.delivered", "body": { "unit": "u2", "task": 2 } })),
            next(Principal::Service, json!({ "kind": "attachment.removed", "body": { "ids": ["a1"], "how": "deleted", "rule": { "days": 1, "only": "all", "minMb": 0 } } })),
            next(Principal::Agent { session: "s-i".into(), machine: None }, json!({ "kind": "attachment.restored", "body": { "id": "00000000000000f2" } })),
            next(Principal::Service, json!({ "kind": "attachment.restored", "body": { "id": "00000000000000f2" } })),
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
    for key in ["state", "runs", "edits", "blind", "managers", "encoded", "next"] {
        assert_eq!(serde_json::to_string_pretty(&got[key]).unwrap(), serde_json::to_string_pretty(&want[key]).unwrap(), "{key} differs from {}", path.display());
    }
}

/// One enumeration of who holds a file: what the blind view keeps "as an attachment" is the
/// attached part of `attachment_holders`, on a history with every kind of record.
#[test]
fn the_blind_views_held_files_are_the_attached_holders() {
    let state = folded(&history());
    let attached: std::collections::HashSet<String> = state.attachment_holders().into_iter().filter(|h| h.how == Named::Attached).map(|h| h.id.to_string()).collect();
    assert!(!attached.is_empty());
    assert_eq!(state.held_attachments(), attached);
}
