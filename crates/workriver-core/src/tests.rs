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
    rejects(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "fix", "causes": [{"task": 2, "findings": ["P2-1"]}]}}), "neither accepted nor revised");
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

#[test]
fn corrections_after_delivery() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    // Delivering twice points at --amend.
    let again = s.append("agent-a", body(json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1}}))).unwrap_err().to_string();
    assert!(again.contains("--amend"), "{again}");
    s.append("agent-a", body(json!({"kind": "task.amended", "body": {"unit": "u1", "task": 1, "note": "Better regex",
        "evidence": [{"command": "npm test", "result": "passed", "summary": "7 passed"}]}}))).unwrap();
    let t = s.state().task("u1", 1).unwrap();
    assert_eq!((t.note.as_str(), t.evidence[0].summary.as_str()), ("Better regex", "7 passed"));
    assert!(t.amended.is_some());
    // Findings can be replaced while none is decided, and are numbered again.
    s.append("agent-a", body(json!({"kind": "task.amended", "body": {"unit": "u1", "task": 2, "findings": [
        {"severity": "P1", "problem": "Crash", "impact": "Down", "where": ["a.ts"]}]}}))).unwrap();
    assert_eq!(s.state().task("u1", 2).unwrap().findings[0].found.id, "P1-1");
    s.append("user", body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P1-1", "decision": "defer", "by": "user"}}))).unwrap();
    let late = s.append("agent-a", body(json!({"kind": "task.amended", "body": {"unit": "u1", "task": 2, "findings": []}}))).unwrap_err().to_string();
    assert!(late.contains("already decided") && late.contains("new review or tests task"), "{late}");
    // The goal: the user or the intent's manager only.
    let other = s.append("agent-b", body(json!({"kind": "intent.updated", "body": {"intent": "i1", "goal": "x"}}))).unwrap_err().to_string();
    assert!(other.contains("only the user or the manager"), "{other}");
    s.append("user", body(json!({"kind": "intent.updated", "body": {"intent": "i1", "goal": "Let people mention each other"}}))).unwrap();
    assert_eq!(s.state().intents[0].goal, "Let people mention each other");
}

#[test]
fn questions_are_answered_by_their_own_agent_or_the_user() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    let e = s.append("user", body(json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": "Why a regex?",
        "context": [{"kind": "lines", "label": "a.ts L3–4", "path": "app/a.ts", "from": 3, "to": 4}]}}))).unwrap();
    let Body::QuestionAsked { id, .. } = e.body else { panic!() };
    assert_eq!(id, "q1");
    assert!(s.append("user", body(json!({"kind": "question.asked", "body": {"unit": "u1", "task": 9, "text": "?"}}))).is_err());
    s.append("workriver", body(json!({"kind": "question.started", "body": {"question": "q1", "agent": "s-why", "name": "why-q1"}}))).unwrap();
    // Another agent may not answer; the started one may, known by its name even under a new session.
    let other = s.append("agent-b", body(json!({"kind": "question.answered", "body": {"question": "q1", "text": "x"}}))).unwrap_err().to_string();
    assert!(other.contains("why-q1"), "{other}");
    s.append("workriver", body(json!({"kind": "agent.seen", "body": {"session": "s-why-2", "name": "why-q1", "harness": "claude"}}))).unwrap();
    s.append("s-why-2", body(json!({"kind": "question.answered", "body": {"question": "q1", "text": "It is short.", "asked": "agent-a"}}))).unwrap();
    let q = s.state().question("q1").unwrap();
    assert_eq!((q.answer.as_deref(), q.asked.as_deref(), q.answered_by.as_deref()), (Some("It is short."), Some("agent-a"), Some("s-why-2")));
    let again = s.append("user", body(json!({"kind": "question.answered", "body": {"question": "q1", "text": "y"}}))).unwrap_err().to_string();
    // The task's own agent may answer questions about its work.
    s.append("user", body(json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": "Own?"}}))).unwrap();
    s.append("agent-a", body(json!({"kind": "question.answered", "body": {"question": "q2", "text": "Mine."}}))).unwrap();
    assert!(again.contains("already answered"), "{again}");
    // A question can fail once, while open; the user can still answer an open one.
    s.append("user", body(json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": "And?"}}))).unwrap();
    s.append("workriver", body(json!({"kind": "question.failed", "body": {"question": "q3", "reason_code": "timeout", "reason": "no answer in 10 minutes"}}))).unwrap();
    assert!(s.append("user", body(json!({"kind": "question.answered", "body": {"question": "q3", "text": "late"}}))).is_err());
}

#[test]
fn design_tasks_decide_gates_and_revised_findings() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    s.append("boss", body(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "design", "title": "Design the store"}}))).unwrap();
    let no_note = s.append("w", body(json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 3}}))).unwrap_err().to_string();
    assert!(no_note.contains("design as the note"), "{no_note}");
    s.append("w", body(json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 3, "note": "One table."}}))).unwrap();
    s.append("boss", body(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "decide", "title": "Confirm the design", "inputs": [{"from": "3"}]}}))).unwrap();
    assert_eq!(s.state().task("u1", 4).unwrap().gate, Some(GateKind::Decide));
    // Revise needs a note, and then leads to work like an accepted finding.
    let bare = s.append("user", body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "revise", "by": "user"}}))).unwrap_err().to_string();
    assert!(bare.contains("revise needs a note"), "{bare}");
    s.append("user", body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "revise", "note": "Use a set", "by": "user"}}))).unwrap();
    assert_eq!(s.state().task("u1", 2).unwrap().findings[0].state, FindingState::Revised);
    s.append("boss", body(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Dedupe with a set", "causes": [{"task": 2, "findings": ["P2-1"]}]}}))).unwrap();
}

#[test]
fn attachments_are_stored_before_they_are_used() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    let unknown = s.append("user", body(json!({"kind": "task.amended", "body": {"unit": "u1", "task": 1, "attachments": ["abc"]}}))).unwrap_err().to_string();
    assert!(unknown.contains("no stored attachment abc"), "{unknown}");
    s.append("agent-a", body(json!({"kind": "attachment.added", "body": {"id": "abc", "name": "a.png", "type": "image/png", "size": 3}}))).unwrap();
    s.append("agent-a", body(json!({"kind": "task.amended", "body": {"unit": "u1", "task": 1, "attachments": ["abc"]}}))).unwrap();
    // Added again only to link it to a finding: one record, two links.
    s.append("agent-a", body(json!({"kind": "attachment.added", "body": {"id": "abc", "name": "a.png", "type": "image/png", "size": 3, "unit": "u1", "task": 2, "finding": "P2-1"}}))).unwrap();
    assert!(s.append("agent-a", body(json!({"kind": "attachment.added", "body": {"id": "x", "name": "x", "type": "", "size": 1, "unit": "u1", "task": 2, "finding": "P9-9"}}))).is_err());
    assert_eq!(s.state().attachments.len(), 1);
    assert_eq!(s.state().task("u1", 1).unwrap().attachments, ["abc"]);
    assert_eq!(s.state().task("u1", 2).unwrap().findings[0].found.attachments, ["abc"]);
}
