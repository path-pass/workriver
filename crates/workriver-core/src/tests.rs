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
        store.append(&"agent-a".into(), body(s)).unwrap();
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
    s.append(&"agent-b".into(), body(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review"}}))).unwrap();
    let mut rejects = |v: serde_json::Value, says: &str| {
        let err = s.append(&"agent-a".into(), body(v)).unwrap_err().to_string();
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
    rejects(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "fix", "causes": [{"task": 2, "findings": ["P2-1"]}]}}), "neither accepted, revised nor scheduled");
    rejects(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P9-1", "decision": "accept", "by": "user"}}), "no finding P9-1");
    rejects(json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 1, "result": "done"}}), "only the user does this (gate.submitted)");

    rejects(json!({"kind": "task.started", "body": {"unit": "u1", "task": 1}}), "is done, not todo");
    // Decisions only on open findings.
    s.append(&"user".into(), body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "reject", "by": "user"}}))).unwrap();
    let err = s.append(&"user".into(), body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "accept", "by": "user"}})));
    assert!(err.unwrap_err().to_string().contains("already rejected"));
    let err = s.append(&"user".into(), body(json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 1, "result": "done"}}))).unwrap_err().to_string();
    assert!(err.contains("not a waiting gate"), "{err}");
}

#[test]
fn accepted_finding_leads_to_task_with_derived_inputs() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    s.append(&"user".into(), body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-2", "decision": "accept", "by": "user"}}))).unwrap();
    let e = s.append(&"manager".into(), body(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "fix", "causes": [{"task": 2, "findings": ["P2-2"]}]}}))).unwrap();
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
    let again = s.append(&"agent-a".into(), body(json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1}}))).unwrap_err().to_string();
    assert!(again.contains("--amend"), "{again}");
    s.append(&"agent-a".into(), body(json!({"kind": "task.amended", "body": {"unit": "u1", "task": 1, "note": "Better regex",
        "evidence": [{"command": "npm test", "result": "passed", "summary": "7 passed"}]}}))).unwrap();
    let t = s.state().task("u1", 1).unwrap();
    assert_eq!((t.note.as_str(), t.evidence[0].summary.as_str()), ("Better regex", "7 passed"));
    assert!(t.amended.is_some());
    // Findings can be replaced while none is decided, and are numbered again.
    s.append(&"agent-a".into(), body(json!({"kind": "task.amended", "body": {"unit": "u1", "task": 2, "findings": [
        {"severity": "P1", "problem": "Crash", "impact": "Down", "where": ["a.ts"]}]}}))).unwrap();
    assert_eq!(s.state().task("u1", 2).unwrap().findings[0].found.id, "P1-1");
    s.append(&"user".into(), body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P1-1", "decision": "defer", "by": "user"}}))).unwrap();
    let late = s.append(&"agent-a".into(), body(json!({"kind": "task.amended", "body": {"unit": "u1", "task": 2, "findings": []}}))).unwrap_err().to_string();
    assert!(late.contains("already decided") && late.contains("new review or tests task"), "{late}");
    // The goal: the user or the intent's manager only.
    let other = s.append(&"agent-b".into(), body(json!({"kind": "intent.updated", "body": {"intent": "i1", "goal": "x"}}))).unwrap_err().to_string();
    assert!(other.contains("only the user or the manager"), "{other}");
    s.append(&"user".into(), body(json!({"kind": "intent.updated", "body": {"intent": "i1", "goal": "Let people mention each other"}}))).unwrap();
    assert_eq!(s.state().intents[0].goal, "Let people mention each other");
}

#[test]
fn questions_are_answered_by_their_own_agent_or_the_user() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    let e = s.append(&"user".into(), body(json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": "Why a regex?",
        "context": [{"kind": "lines", "label": "a.ts L3–4", "path": "app/a.ts", "from": 3, "to": 4}]}}))).unwrap();
    let Body::QuestionAsked { id, .. } = e.body else { panic!() };
    assert_eq!(id, "q1");
    assert!(s.append(&"user".into(), body(json!({"kind": "question.asked", "body": {"unit": "u1", "task": 9, "text": "?"}}))).is_err());
    s.append(&"workriver".into(), body(json!({"kind": "question.started", "body": {"question": "q1", "agent": "s-why", "name": "why-q1"}}))).unwrap();
    // Another agent may not answer; the started one may, known by its name even under a new session.
    let other = s.append(&"agent-b".into(), body(json!({"kind": "question.answered", "body": {"question": "q1", "text": "x"}}))).unwrap_err().to_string();
    assert!(other.contains("why-q1"), "{other}");
    s.append(&"workriver".into(), body(json!({"kind": "agent.seen", "body": {"session": "s-why-2", "name": "why-q1", "harness": "claude"}}))).unwrap();
    s.append(&"s-why-2".into(), body(json!({"kind": "question.answered", "body": {"question": "q1", "text": "It is short.", "asked": "agent-a"}}))).unwrap();
    let q = s.state().question("q1").unwrap();
    assert_eq!((q.answer.as_deref(), q.asked.as_deref(), q.answered_by.as_deref()), (Some("It is short."), Some("agent-a"), Some("s-why-2")));
    let again = s.append(&"user".into(), body(json!({"kind": "question.answered", "body": {"question": "q1", "text": "y"}}))).unwrap_err().to_string();
    // The task's own agent may answer questions about its work.
    s.append(&"user".into(), body(json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": "Own?"}}))).unwrap();
    s.append(&"agent-a".into(), body(json!({"kind": "question.answered", "body": {"question": "q2", "text": "Mine."}}))).unwrap();
    assert!(again.contains("already answered"), "{again}");
    // A question can fail once, while open; the user can still answer an open one.
    s.append(&"user".into(), body(json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": "And?"}}))).unwrap();
    s.append(&"workriver".into(), body(json!({"kind": "question.failed", "body": {"question": "q3", "reason_code": "timeout", "reason": "no answer in 10 minutes"}}))).unwrap();
    assert!(s.append(&"user".into(), body(json!({"kind": "question.answered", "body": {"question": "q3", "text": "late"}}))).is_err());
}

#[test]
fn design_tasks_decide_gates_and_revised_findings() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    s.append(&"boss".into(), body(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "design", "title": "Design the store"}}))).unwrap();
    let no_note = s.append(&"w".into(), body(json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 3}}))).unwrap_err().to_string();
    assert!(no_note.contains("design as the note"), "{no_note}");
    s.append(&"w".into(), body(json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 3, "note": "One table."}}))).unwrap();
    s.append(&"boss".into(), body(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "decide", "title": "Confirm the design", "inputs": [{"from": "3"}]}}))).unwrap();
    assert_eq!(s.state().task("u1", 4).unwrap().gate, Some(GateKind::Decide));
    // Revise needs a note, and then leads to work like an accepted finding.
    let bare = s.append(&"user".into(), body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "revise", "by": "user"}}))).unwrap_err().to_string();
    assert!(bare.contains("revise needs a note"), "{bare}");
    s.append(&"user".into(), body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "revise", "note": "Use a set", "by": "user"}}))).unwrap();
    assert_eq!(s.state().task("u1", 2).unwrap().findings[0].state, FindingState::Revised);
    s.append(&"boss".into(), body(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Dedupe with a set", "causes": [{"task": 2, "findings": ["P2-1"]}]}}))).unwrap();
}

#[test]
fn attachments_are_stored_before_they_are_used() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    let unknown = s.append(&"user".into(), body(json!({"kind": "task.amended", "body": {"unit": "u1", "task": 1, "attachments": ["abc"]}}))).unwrap_err().to_string();
    assert!(unknown.contains("no stored attachment abc"), "{unknown}");
    s.append(&"agent-a".into(), body(json!({"kind": "attachment.added", "body": {"id": "abc", "name": "a.png", "type": "image/png", "size": 3}}))).unwrap();
    s.append(&"agent-a".into(), body(json!({"kind": "task.amended", "body": {"unit": "u1", "task": 1, "attachments": ["abc"]}}))).unwrap();
    // Added again only to link it to a finding: one record, two links.
    s.append(&"agent-a".into(), body(json!({"kind": "attachment.added", "body": {"id": "abc", "name": "a.png", "type": "image/png", "size": 3, "unit": "u1", "task": 2, "finding": "P2-1"}}))).unwrap();
    assert!(s.append(&"agent-a".into(), body(json!({"kind": "attachment.added", "body": {"id": "x", "name": "x", "type": "", "size": 1, "unit": "u1", "task": 2, "finding": "P9-9"}}))).is_err());
    assert_eq!(s.state().attachments.len(), 1);
    assert_eq!(s.state().task("u1", 1).unwrap().attachments, ["abc"]);
    assert_eq!(s.state().task("u1", 2).unwrap().findings[0].found.attachments, ["abc"]);
}

fn add(s: &mut Store, actor: &str, v: serde_json::Value) -> Result<Event, String> {
    s.append(&actor.into(), body(v)).map_err(|e| e.to_string())
}

/// The state rebuilt from the records alone.
fn replayed(s: &Store) -> State {
    Store::open(s.path()).unwrap().state().clone()
}

/// Intents move planned → active → done (or dropped); a workspace's manager starts them,
/// and the workspace's current intent is where its new work goes.
#[test]
fn intent_states_and_workspaces() {
    let (_dir, mut s) = temp_store();
    let w1 = json!({"id": "w1", "label": "chat"});
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Mentions", "goal": "Tag people", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Search", "project": "/p", "state": "planned"}})).unwrap();
    assert_eq!(s.state().roadmap, ["i1", "i2"]);
    // Starting needs the workspace's manager, in that workspace.
    let e = add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": w1}})).unwrap_err();
    assert!(e.contains("manager here"), "{e}");
    let e = add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active"}})).unwrap_err();
    assert!(e.contains("intent start i1"), "{e}");
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": w1}})).unwrap();
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": w1}})).unwrap();
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i2", "state": "active", "workspace": w1}})).unwrap();
    let st = s.state();
    assert!(st.roadmap.is_empty());
    assert_eq!(st.intent_for(&RuntimeRef { id: "w1".into(), label: "chat".into(), machine: None }).map(|i| i.id.as_str()), Some("i2"), "the last started is current");
    assert_eq!((st.manager_of("i1"), st.manager_of("i2")), (Some("boss"), Some("boss")), "both active intents have the workspace's manager");
    // Done: no longer current, no new units; done cannot start again; dropping needs a reason.
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i2", "state": "done"}})).unwrap();
    assert!(s.state().intent_for(&RuntimeRef { id: "w1".into(), label: "chat".into(), machine: None }).is_none());
    let e = add(&mut s, "boss", json!({"kind": "unit.created", "body": {"intent": "i2", "title": "x", "repo": "/r", "branch": "b", "base": "main"}})).unwrap_err();
    assert!(e.contains("is done"), "{e}");
    let e = add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i2", "state": "active", "workspace": w1}})).unwrap_err();
    assert!(e.contains("cannot become active"), "{e}");
    let e = add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "dropped"}})).unwrap_err();
    assert!(e.contains("reason"), "{e}");
    add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "dropped", "reason": "not needed"}})).unwrap();
    assert_eq!(s.state().intent("i1").unwrap().reason.as_deref(), Some("not needed"));
}

/// The roadmap keeps the order intents were added in, and moves change it.
#[test]
fn roadmap_order_and_moves() {
    let (_dir, mut s) = temp_store();
    for t in ["A", "B", "C", "D"] {
        add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": t, "project": "/p", "state": "planned", "note": "line 1\nhttps://example.com/x", "external": {"system": "jira", "key": "ABC-1"}}})).unwrap();
    }
    assert_eq!(s.state().roadmap, ["i1", "i2", "i3", "i4"]);
    add(&mut s, "user", json!({"kind": "intent.moved", "body": {"intent": "i4", "before": "i1"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.moved", "body": {"intent": "i1", "after": "i3"}})).unwrap();
    assert_eq!(s.state().roadmap, ["i4", "i2", "i3", "i1"]);
    assert!(add(&mut s, "user", json!({"kind": "intent.moved", "body": {"intent": "i1", "before": "i1"}})).is_err());
    assert!(add(&mut s, "user", json!({"kind": "intent.moved", "body": {"intent": "i1"}})).is_err());
    let i = s.state().intent("i1").unwrap().clone();
    assert_eq!((i.note.as_str(), i.external.as_ref().map(|e| e.key.as_str())), ("line 1\nhttps://example.com/x", Some("ABC-1")));
    // Order and external fields survive a replay.
    let replayed = replayed(&s);
    assert_eq!(replayed.roadmap, s.state().roadmap);
    assert_eq!(replayed.intent("i1").unwrap().external, i.external);
}

/// Research and design belong to an intent without a unit; research hands in a note or a report.
#[test]
fn intent_level_research() {
    let (_dir, mut s) = temp_store();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Sync", "project": "/p", "state": "planned"}})).unwrap();
    let e = add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "i1", "kind": "implement", "title": "x"}})).unwrap_err();
    assert!(e.contains("belong to a unit"), "{e}");
    let t = add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "i1", "kind": "research", "title": "Compare sync engines"}})).unwrap();
    assert_eq!(t.body.refs(), (Some("i1".into()), Some("i1".into()), Some("i1#1".into())));
    let e = add(&mut s, "r", json!({"kind": "task.delivered", "body": {"unit": "i1", "task": 1}})).unwrap_err();
    assert!(e.contains("research task delivers"), "{e}");
    let e = add(&mut s, "r", json!({"kind": "task.delivered", "body": {"unit": "i1", "task": 1, "note": "n", "findings": [{"severity": "P2", "problem": "p", "impact": "i", "where": ["a"]}]}})).unwrap_err();
    assert!(e.contains("findings"), "{e}");
    add(&mut s, "r", json!({"kind": "task.delivered", "body": {"unit": "i1", "task": 1, "note": "Use CRDTs; see report"}})).unwrap();
    let st = s.state();
    let t = st.task("i1", 1).unwrap();
    assert_eq!((t.state, t.intent.as_str()), (TaskState::Done, "i1"));
    // A unit's tasks record their intent too.
    add(&mut s, "boss", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "Engine", "repo": "/r", "branch": "b", "base": "main"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "x"}})).unwrap();
    assert_eq!(s.state().task("u1", 1).unwrap().intent, "i1");
}

/// Deferred findings, from a task or added to the backlog, are batched (scheduled), and done
/// when the task they caused is delivered; or dropped with a reason.
#[test]
fn deferred_batches() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    let w1 = json!({"id": "w1", "label": "chat"});
    add(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "defer", "by": "user"}})).unwrap();
    add(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-2", "decision": "defer", "by": "user"}})).unwrap();
    let f = add(&mut s, "user", json!({"kind": "finding.added", "body": {"project": "/r", "finding": {"severity": "P1", "problem": "Old leak"}, "source": "## feature-x 遗留", "note": "修法: close it", "external": null}})).unwrap();
    assert_eq!(f.body, body(json!({"kind": "finding.added", "body": {"project": "/r", "finding": {"id": "f1", "severity": "P1", "problem": "Old leak"}, "source": "## feature-x 遗留", "note": "修法: close it"}})));
    let e = add(&mut s, "user", json!({"kind": "finding.added", "body": {"project": "/r", "finding": {"severity": "P1", "problem": " "}}})).unwrap_err();
    assert!(e.contains("problem"), "{e}");
    // Batching needs the intent active.
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Later", "project": "/r", "state": "planned"}})).unwrap();
    let e = add(&mut s, "user", json!({"kind": "findings.batched", "body": {"intent": "i2", "findings": [{"finding": "f1"}]}})).unwrap_err();
    assert!(e.contains("start it first"), "{e}");
    let e = add(&mut s, "user", json!({"kind": "findings.batched", "body": {"intent": "i1", "findings": [{"unit": "u1", "task": 1, "finding": "P2-1"}]}})).unwrap_err();
    assert!(e.contains("no finding"), "{e}");
    let batch = json!({"kind": "findings.batched", "body": {"intent": "i1", "findings": [{"unit": "u1", "task": 2, "finding": "P2-1"}, {"finding": "f1"}]}});
    // A legacy intent (created before 0.2) is active.
    assert_eq!(s.state().intent("i1").unwrap().state, IntentState::Active);
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": w1}})).unwrap();
    add(&mut s, "user", batch).unwrap();
    let st = s.state();
    assert_eq!(st.task("u1", 2).unwrap().findings[0].state, FindingState::Scheduled);
    assert_eq!(st.backlog[0].finding.state, FindingState::Scheduled);
    assert_eq!(st.backlog[0].intent.as_deref(), Some("i1"));
    // One cleanup unit, one task with both as causes; delivering it finishes them.
    add(&mut s, "boss", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "Cleanup", "repo": "/r", "branch": "c", "base": "main"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u2", "kind": "implement", "title": "Fix both", "causes": [{"unit": "u1", "task": 2, "findings": ["P2-1"]}, {"task": 0, "findings": ["f1"]}]}})).unwrap();
    assert_eq!(s.state().task("u2", 1).unwrap().inputs[0].from, "start", "causes in other units are not inputs");
    add(&mut s, "w", json!({"kind": "task.delivered", "body": {"unit": "u2", "task": 1}})).unwrap();
    let st = s.state();
    assert_eq!((st.task("u1", 2).unwrap().findings[0].state, st.backlog[0].finding.state), (FindingState::Done, FindingState::Done));
    // The other deferred one is dropped, with a reason.
    assert!(add(&mut s, "user", json!({"kind": "finding.dropped", "body": {"unit": "u1", "task": 2, "finding": "P2-2", "reason": ""}})).is_err());
    add(&mut s, "user", json!({"kind": "finding.dropped", "body": {"unit": "u1", "task": 2, "finding": "P2-2", "reason": "obsolete"}})).unwrap();
    assert_eq!(s.state().task("u1", 2).unwrap().findings[1].state, FindingState::Dropped);
    assert!(add(&mut s, "user", json!({"kind": "finding.dropped", "body": {"finding": "f1", "reason": "x"}})).is_err(), "done is done");
    let replayed = replayed(&s);
    assert_eq!(serde_json::to_value(&replayed).unwrap(), serde_json::to_value(s.state()).unwrap());
}

/// Records from before projects and intent states replay onto the new model: an intent made
/// for a runtime group is active and that group's current intent, its manager the group's.
#[test]
fn old_records_map_onto_workspaces() {
    let (_dir, mut s) = temp_store();
    let w = json!({"id": "wM", "label": "workriver"});
    add(&mut s, "workriver", json!({"kind": "intent.created", "body": {"id": "", "title": "workriver", "goal": "", "runtime_ref": w}})).unwrap();
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"intent": "i1", "agent": "boss"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "ja", "repo": "/r", "branch": "ja-ui", "base": "main"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "impl"}})).unwrap();
    let st = replayed(&s);
    let i = st.intent("i1").unwrap();
    assert_eq!((i.state, i.workspace.as_ref().map(|g| g.id.as_str())), (IntentState::Active, Some("wM")));
    assert_eq!(st.workspaces.len(), 1);
    assert_eq!((st.workspaces[0].manager.as_deref(), st.workspaces[0].current.as_deref()), (Some("boss"), Some("i1")));
    assert_eq!(st.manager_of("i1"), Some("boss"));
    assert_eq!(st.task("u1", 1).unwrap().intent, "i1");
}

/// Workspaces of two machines are never one by their labels alone: only a recorded link
/// makes a remote workspace the local one (one manager, the same current intent).
#[test]
fn remote_workspaces_are_linked_explicitly() {
    let (_dir, mut s) = temp_store();
    let local = json!({"id": "w1", "label": "chat"});
    let remote = RuntimeRef { id: "w1@box".into(), label: "chat".into(), machine: Some("box".into()) };
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "A", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": local}})).unwrap();
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": local}})).unwrap();
    assert!(s.state().workspace_of(&remote).is_none(), "the same label on another machine is another workspace");
    assert!(s.state().intent_for(&remote).is_none());
    add(&mut s, "user", json!({"kind": "workspace.linked", "body": {"machine": "box", "workspace": "chat", "local": "chat"}})).unwrap();
    assert!(add(&mut s, "user", json!({"kind": "workspace.linked", "body": {"machine": "box", "workspace": "chat", "local": "other"}})).is_err());
    let st = replayed(&s);
    assert_eq!(st.intent_for(&remote).map(|i| i.id.as_str()), Some("i1"));
    assert_eq!(st.workspace_of(&remote).and_then(|w| w.manager.as_deref()), Some("boss"));
    // A manager on the other machine registers for the linked workspace: the one record, which stays the local one.
    add(&mut s, "far", json!({"kind": "manager.registered", "body": {"agent": "far", "workspace": remote}})).unwrap();
    let st = s.state();
    assert_eq!((st.workspaces.len(), st.workspaces[0].manager.as_deref(), st.workspaces[0].group.id.as_str()), (1, Some("far"), "w1"));
    assert_eq!(st.manager_of("i1"), Some("far"));
    add(&mut s, "user", json!({"kind": "workspace.unlinked", "body": {"machine": "box", "workspace": "chat"}})).unwrap();
    assert!(s.state().workspace_of(&remote).is_none());
}

/// What is the user's is the user's whoever asks: an agent cannot submit a gate, finish a
/// chore, batch or drop findings, link workspaces or add projects; its decision on a finding
/// is recorded as an agent's whatever it claims; and it cannot call itself the user.
#[test]
fn only_the_user_does_what_is_the_users() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    add(&mut s, "agent-a", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "test", "title": "Try it", "inputs": [{"from": "1"}]}})).unwrap();
    add(&mut s, "agent-a", json!({"kind": "chore.created", "body": {"intent": "i1", "title": "Rotate the key"}})).unwrap();
    add(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-2", "decision": "defer"}})).unwrap();
    let theirs = [
        json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 3, "result": "done"}}),
        json!({"kind": "chore.done", "body": {"chore": "c1"}}),
        json!({"kind": "findings.batched", "body": {"intent": "i1", "findings": [{"unit": "u1", "task": 2, "finding": "P2-2"}]}}),
        json!({"kind": "finding.dropped", "body": {"unit": "u1", "task": 2, "finding": "P2-2", "reason": "x"}}),
        json!({"kind": "workspace.linked", "body": {"machine": "box", "workspace": "a", "local": "a"}}),
        json!({"kind": "project.added", "body": {"path": "/p", "title": "P"}}),
    ];
    for who in ["agent-a", "workriver"] {
        for v in &theirs {
            let e = add(&mut s, who, v.clone()).unwrap_err();
            assert!(e.contains("only the user does this"), "{who}: {e}");
        }
    }
    let impostor = Principal::Agent { session: "user".into(), machine: None };
    assert!(s.append(&impostor, body(theirs[0].clone())).is_err());
    assert!(s.append(&impostor, body(json!({"kind": "task.started", "body": {"unit": "u1", "task": 1}}))).is_err(), "nor record anything under that name");
    // An agent may decide a finding, but never as the user.
    let e = add(&mut s, "agent-a", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "accept", "by": "user"}})).unwrap();
    assert_eq!(e.body, body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "accept", "by": "agent"}})));
    // A manager is an agent registering itself.
    let w = json!({"id": "w1", "label": "chat"});
    assert!(add(&mut s, "agent-a", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": w}})).is_err());
    assert!(add(&mut s, "user", json!({"kind": "manager.registered", "body": {"workspace": w}})).is_err());
    let e = add(&mut s, "agent-a", json!({"kind": "manager.registered", "body": {"workspace": w}})).unwrap();
    assert_eq!(e.body, body(json!({"kind": "manager.registered", "body": {"agent": "agent-a", "workspace": w}})));
    // The user does all of it.
    for v in theirs {
        add(&mut s, "user", v).unwrap();
    }
}
