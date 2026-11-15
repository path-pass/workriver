use serde_json::json;

use crate::*;

fn body(v: serde_json::Value) -> Body {
    serde_json::from_value(v).unwrap()
}

/// An intent, a unit, an implement task delivered with evidence and a review with two findings.
fn seeded(store: &mut Store) {
    let steps = [
        json!({"kind": "intent.created", "body": {"title": "Chat v2"}}),
        json!({"kind": "unit.created", "body": {"intent": "i1", "title": "Mentions", "repo": "/r", "branch": "mention", "base": "main"}}),
        json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Find mentions"}}),
        json!({"kind": "task.started", "body": {"unit": "u1", "task": 1}}),
        json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1, "evidence": [{"command": "npm test", "result": "passed", "summary": "6 passed"}]}}),
        json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "inputs": [{"from": "1"}]}}),
        json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 2, "findings": [
            {"severity": "P2", "problem": "Duplicates", "impact": "Two notices", "where": ["app/mention.ts"]},
            {"severity": "P2", "problem": "Emoji names", "impact": "Not found", "where": ["app/mention.ts"]},
        ]}}),
    ];
    for s in steps {
        store.append("agent-a", body(s)).unwrap();
    }
}

fn temp_store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("w.db")).unwrap();
    (dir, store)
}

#[test]
fn rules_assign_ids_and_accept_good_input() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    let review = s.state().task("u1", 2).unwrap();
    let ids: Vec<_> = review.findings.iter().map(|f| f.found.id.as_str()).collect();
    assert_eq!(ids, ["P2-1", "P2-2"]);
    assert_eq!(review.reviewer, Some(Reviewer::CodeReview));
    assert_eq!(s.state().task("u1", 1).unwrap().agent.as_deref(), Some("agent-a"));
}

#[test]
fn rules_reject_bad_input() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    // A second review (#3), left open for the delivery below.
    s.append("agent-b", body(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review"}}))).unwrap();
    let mut rejects = |v: serde_json::Value, says: &str| {
        let err = s.append("agent-a", body(v)).unwrap_err().to_string();
        assert!(err.contains(says), "{err:?} does not say {says:?}");
    };
    rejects(json!({"kind": "unit.created", "body": {"intent": "i9", "title": "x", "repo": "/r", "branch": "b", "base": "main"}}), "no intent i9");
    rejects(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "x", "inputs": [{"from": "7"}]}}), "no task u1#7");
    // A finding without a where.
    rejects(
        json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 3, "findings": [{"severity": "P1", "problem": "p", "impact": "i"}]}}),
        "at least one where",
    );
    // Work for a finding nobody accepted.
    rejects(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "fix", "causes": [{"task": 2, "findings": ["P2-1"]}]}}), "not accepted");
    rejects(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P9-1", "decision": "accept", "by": "user"}}), "no finding P9-1");
    rejects(json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 1, "result": "done"}}), "not a waiting gate");
    rejects(json!({"kind": "task.started", "body": {"unit": "u1", "task": 1}}), "is done, not todo");
    // Decisions only on open findings.
    s.append("user", body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "reject", "by": "user"}}))).unwrap();
    let err = s.append("user", body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "accept", "by": "user"}})));
    assert!(err.unwrap_err().to_string().contains("already rejected"));
}

#[test]
fn accepted_finding_leads_to_task_with_derived_inputs() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    s.append("user", body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-2", "decision": "accept", "by": "user"}}))).unwrap();
    let e = s.append("manager", body(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "fix", "causes": [{"task": 2, "findings": ["P2-2"]}]}}))).unwrap();
    let Body::TaskCreated(t) = e.body else { panic!() };
    assert_eq!(t.n, 3);
    assert_eq!(t.inputs, [Input { from: "2".into(), primary: true }]);
}

#[test]
fn replay_rebuilds_the_same_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("w.db");
    let before = {
        let mut s = Store::open(&path).unwrap();
        seeded(&mut s);
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
    seeded(&mut Store::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    for sql in ["UPDATE events SET actor = 'x'", "DELETE FROM events"] {
        let err = conn.execute(sql, []).unwrap_err().to_string();
        assert!(err.contains("append-only"), "{sql}: {err}");
    }
}
