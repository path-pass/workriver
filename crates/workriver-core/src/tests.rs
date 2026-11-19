use serde_json::json;

use crate::*;

fn body(v: serde_json::Value) -> Body {
    serde_json::from_value(v).unwrap()
}

/// An intent, a unit, an implement task delivered with evidence and a review with two findings.
fn seeded(store: &mut Store) {
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
    for s in steps {
        store.append(&"agent-a".into(), body(s)).unwrap();
    }
}

fn temp_store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("w.db")).unwrap();
    (dir, store)
}

/// An architecture review is a review like the others: recorded under its name, kept when
/// the events are read again, and it reports findings.
#[test]
fn an_architecture_review_round_trips() {
    let (dir, mut s) = temp_store();
    seeded(&mut s);
    s.append(&"boss".into(), body(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "reviewer": "architecture-review", "inputs": [{"from": "1"}]}}))).unwrap();
    s.append(&"arch".into(), body(json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 3, "findings": [{"severity": "P2", "problem": "The scan reads the store", "impact": "Layers cross", "where": ["scan/lib.rs"]}]}}))).unwrap();
    drop(s);
    let s = Store::open(&dir.path().join("w.db")).unwrap();
    let review = s.state().task("u1", 3).unwrap();
    assert_eq!((review.kind, review.reviewer, review.findings.len()), (TaskKind::Review, Some(Reviewer::ArchitectureReview), 1));
    assert_eq!(serde_json::to_value(s.state()).unwrap()["units"][0]["tasks"][2]["reviewer"], "architecture-review");
    assert_eq!(name(&Reviewer::ArchitectureReview), "architecture-review");
}

/// What a task comes after, which gate asks about a design, and which design revises it.
#[test]
fn designs_their_gates_and_their_revisions() {
    let (_dir, mut s) = temp_store();
    fn put(s: &mut Store, actor: &str, v: serde_json::Value) {
        s.append(&actor.into(), body(v)).unwrap();
    }
    let task = |kind: &str, after: &[&str], more: serde_json::Value| {
        let mut b = json!({"unit": "u1", "kind": kind, "title": kind, "inputs": after.iter().map(|f| json!({"from": f})).collect::<Vec<_>>()});
        b.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        json!({"kind": "task.created", "body": b})
    };
    let found = |n: u32| json!({"kind": "task.delivered", "body": {"unit": "u1", "task": n, "findings": [{"severity": "P2", "problem": "x", "impact": "y", "where": ["a"]}]}});
    put(&mut s, "boss", json!({"kind": "intent.created", "body": {"title": "Chat"}}));
    put(&mut s, "boss", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "Mentions", "repo": "/r", "branch": "feat/m", "base": "main"}}));
    put(&mut s, "boss", task("design", &["start"], json!({}))); // 1: an older design, unrelated
    put(&mut s, "boss", task("design", &["1"], json!({}))); // 2
    put(&mut s, "boss", task("review", &["2"], json!({"reviewer": "architecture-review"}))); // 3: of the design
    put(&mut s, "boss", task("gate", &["3"], json!({"gate": "decide"}))); // 4: confirm design 2
    put(&mut s, "boss", task("implement", &["4"], json!({}))); // 5
    put(&mut s, "boss", task("review", &["5"], json!({}))); // 6: of the code
    put(&mut s, "boss", task("gate", &["6"], json!({"gate": "decide", "title": "Merge?"}))); // 7
    put(&mut s, "rev", found(3));
    put(&mut s, "rev", found(6));
    put(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 3, "finding": "P2-1", "decision": "accept"}}));
    put(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 6, "finding": "P2-1", "decision": "accept"}}));
    let st = |s: &Store| s.state().clone();
    let state = st(&s);
    assert_eq!((state.before("u1", 5), state.before("u1", 1), state.before("u1", 99)), (vec![1, 2, 3, 4], vec![], vec![]));
    // Through reviews only; not through implementing, a gate, or another design.
    assert_eq!((state.designs_asked("u1", 4), state.designs_asked("u1", 3), state.designs_asked("u1", 2)), (vec![2], vec![2], vec![1]));
    assert!(state.designs_asked("u1", 7).is_empty() && state.designs_asked("u1", 6).is_empty() && state.designs_asked("u1", 5).is_empty());
    // A later question to merge is not the design's gate; a design nobody was asked about has none.
    assert_eq!((state.design_gate("u1", 2).map(|g| g.n), state.design_gate("u1", 1).map(|g| g.n)), (Some(4), None));
    assert!(state.revised_by("u1", 2).is_empty());

    // A design answering a finding about the code revises nothing.
    put(&mut s, "boss", task("design", &[], json!({"causes": [{"task": 6, "findings": ["P2-1"]}]}))); // 8
    assert!(st(&s).revised_by("u1", 2).is_empty() && st(&s).revised_by("u1", 1).is_empty());
    // One answering the design's review, its gate, or a finding of the design itself does;
    // not the design before that one, and not once it is dropped.
    put(&mut s, "boss", task("design", &[], json!({"causes": [{"task": 3, "findings": ["P2-1"]}]}))); // 9
    let state = st(&s);
    assert_eq!(state.revised_by("u1", 2).iter().map(|(o, t)| (o.as_str(), t.n)).collect::<Vec<_>>(), [("u1", 9)]);
    assert!(state.revised_by("u1", 1).is_empty());
    put(&mut s, "user", json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 4, "result": "problem", "findings": [{"severity": "P2", "problem": "No"}]}}));
    put(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 4, "finding": "P2-1", "decision": "accept"}}));
    put(&mut s, "boss", task("design", &[], json!({"causes": [{"task": 4, "findings": ["P2-1"]}]}))); // 10
    put(&mut s, "user", json!({"kind": "task.dropped", "body": {"unit": "u1", "task": 9, "reason": "one is enough"}}));
    assert_eq!(st(&s).revised_by("u1", 2).iter().map(|(_, t)| t.n).collect::<Vec<_>>(), [10]);
    // Asked again after the revision: the new design's gate is its own, the old one keeps its.
    put(&mut s, "boss", task("gate", &["10"], json!({"gate": "decide"}))); // 11
    let state = st(&s);
    assert_eq!((state.design_gate("u1", 10).map(|g| g.n), state.design_gate("u1", 2).map(|g| g.n)), (Some(11), Some(4)));
    // What stands in a design's place: nothing until a revision is delivered, then the
    // newest delivered one, followed through its own revisions.
    assert_eq!(state.design_in_place("u1", 2), None);
    put(&mut s, "des", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 10, "note": "Second"}}));
    assert_eq!((st(&s).design_in_place("u1", 2), st(&s).design_in_place("u1", 10), st(&s).design_in_place("u1", 1)), (Some(("u1".to_string(), 10)), None, None));
    put(&mut s, "user", json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 11, "result": "problem", "findings": [{"severity": "P2", "problem": "No"}]}}));
    put(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 11, "finding": "P2-1", "decision": "accept"}}));
    put(&mut s, "boss", task("design", &[], json!({"causes": [{"task": 11, "findings": ["P2-1"]}]}))); // 12
    assert_eq!(st(&s).design_in_place("u1", 2), Some(("u1".to_string(), 10)), "the third is not delivered yet");
    put(&mut s, "des", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 12, "note": "Third"}}));
    assert_eq!((st(&s).design_in_place("u1", 2), st(&s).design_in_place("u1", 10)), (Some(("u1".to_string(), 12)), Some(("u1".to_string(), 12))));
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
    s.append(&"agent-b".into(), body(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "inputs": [{"from": "1"}]}}))).unwrap();
    let mut rejects = |v: serde_json::Value, says: &str| {
        let err = s.append(&"agent-a".into(), body(v)).unwrap_err().to_string();
        assert!(err.contains(says), "{err:?} does not say {says:?}");
    };
    rejects(json!({"kind": "unit.created", "body": {"intent": "i9", "title": "x", "repo": "/r", "branch": "feat/b", "base": "main"}}), "no intent i9");
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
    let err = s.append(&"user".into(), body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "reject", "by": "user"}})));
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
    assert!(late.contains("already decided") && late.contains("--amend --add-findings"), "{late}");
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
    s.append(&"boss".into(), body(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "design", "title": "Design the store", "inputs": [{"from": "2"}]}}))).unwrap();
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

/// A question may point at an attachment shown with its task (its own, its findings', those of
/// the findings it answers), never at another's; a blind task not delivered, not even at those
/// of the findings it answers.
#[test]
fn questions_point_only_at_their_tasks_attachments() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    let file = |id: &str, link: serde_json::Value| {
        let mut b = json!({"id": id, "name": format!("{id}.md"), "type": "text/markdown", "size": 3});
        b.as_object_mut().unwrap().extend(link.as_object().unwrap().clone());
        json!({"kind": "attachment.added", "body": b})
    };
    add(&mut s, "agent-a", file("own", json!({"unit": "u1", "task": 1}))).unwrap();
    add(&mut s, "agent-a", file("found", json!({"unit": "u1", "task": 2, "finding": "P2-1"}))).unwrap();
    add(&mut s, "agent-a", file("loose", json!({}))).unwrap();
    let ask = |task: u32, id: Option<&str>| json!({"kind": "question.asked", "body": {"unit": "u1", "task": task, "text": "What does it say?", "context": [{"kind": "attachment", "label": "report.md", "id": id}]}});
    add(&mut s, "user", ask(1, Some("own"))).unwrap();
    assert_eq!(s.state().questions[0].context[0].id.as_deref(), Some("own"));
    for wrong in [Some("found"), Some("loose"), Some("nope"), None] {
        assert!(add(&mut s, "user", ask(1, wrong)).unwrap_err().contains("is not one of u1#1's"), "{wrong:?}");
    }
    add(&mut s, "user", ask(2, Some("found"))).unwrap();
    // Work that answers a finding shows that finding's attachments.
    add(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "accept"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Dedupe", "causes": [{"task": 2, "findings": ["P2-1"]}]}})).unwrap();
    add(&mut s, "user", ask(3, Some("found"))).unwrap();
    assert_eq!(s.state().attachments_of("u1", 3), ["found"]);
    // A blind review in progress: nothing of other work, not even through the user's question.
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "reviewer": "security-review", "inputs": [{"from": "3"}], "causes": [{"task": 2, "findings": ["P2-1"]}], "agent": "sec", "blind": true}})).unwrap();
    assert!(s.state().attachments_of("u1", 4).is_empty());
    assert!(add(&mut s, "user", ask(4, Some("found"))).unwrap_err().contains("is not one of u1#4's"));
    add(&mut s, "sec", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 4, "findings": []}})).unwrap();
    add(&mut s, "user", ask(4, Some("found"))).unwrap();
    assert!(replayed(&s).questions.iter().all(|q| q.context[0].kind == "attachment"));
}

/// The user's question may also point at a file the user stored for it (dropped, pasted or
/// picked in the page): about any task, or the unit. An agent's question may not, and nobody
/// points at a file only an agent stored and no task shows.
#[test]
fn the_users_question_points_at_files_the_user_stored() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    let file = |id: &str| json!({"kind": "attachment.added", "body": {"id": id, "name": format!("{id}.log"), "type": "text/plain", "size": 3}});
    add(&mut s, "user", file("mine")).unwrap();
    add(&mut s, "agent-a", file("loose")).unwrap();
    let ask = |task: u32, id: &str| json!({"kind": "question.asked", "body": {"unit": "u1", "task": task, "text": "What does it say?", "context": [{"kind": "attachment", "label": "x.log", "id": id}]}});
    add(&mut s, "user", ask(1, "mine")).unwrap();
    add(&mut s, "user", ask(2, "mine")).unwrap();
    add(&mut s, "user", ask(0, "mine")).unwrap();
    assert!(add(&mut s, "agent-a", ask(1, "mine")).unwrap_err().contains("is not one of u1#1's"), "an agent's question");
    assert!(add(&mut s, "user", ask(1, "loose")).unwrap_err().contains("is not one of u1#1's"), "stored by an agent only");
    assert!(add(&mut s, "user", ask(1, "never")).unwrap_err().contains("is not one of u1#1's"), "never stored");
    // The same content stored by an agent first (one file per content): the user's once they store it too.
    add(&mut s, "user", file("loose")).unwrap();
    add(&mut s, "user", ask(1, "loose")).unwrap();
    assert_eq!(s.state().attachments.len(), 2);
    assert_eq!(replayed(&s).attachments, s.state().attachments);
}

fn add(s: &mut Store, actor: &str, v: serde_json::Value) -> Result<Event, String> {
    s.append(&actor.into(), body(v)).map_err(|e| e.to_string())
}

/// An event as an old database has it, straight into the records past today's rules (the
/// body as it was stored, ids and numbers filled in); the store is opened again from them.
fn recorded_before(s: Store, actor: &str, v: serde_json::Value) -> Store {
    let (intent, unit, task) = body(v.clone()).refs();
    let conn = rusqlite::Connection::open(s.path()).unwrap();
    conn.execute(
        "INSERT INTO events (at, kind, actor, intent, unit, task, body) VALUES ('2026-01-01T00:00:00Z', ?1, ?2, ?3, ?4, ?5, ?6)",
        (v["kind"].as_str(), actor, intent, unit, task, v["body"].to_string()),
    )
    .unwrap();
    drop(conn);
    Store::open(s.path()).unwrap()
}

/// A unit's branch is its own and says what it holds; units named before that rule still
/// load from the records and keep working.
#[test]
fn unit_branches_take_a_prefix_but_old_ones_replay() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    let unit = |branch: &str, base: &str| json!({"kind": "unit.created", "body": {"intent": "i1", "title": "x", "repo": "/r", "branch": branch, "base": base}});
    for (branch, base, says) in [("ja-ui", "main", "starts with feat/, chore/ or fix/"), ("feat/", "main", "and a name"), ("feature/x", "main", "e.g. --branch feat/x"), ("main", "main", "must not be its base"), ("feat/x", "feat/x", "must not be its base"), ("main", "origin/main", "must not be its base")] {
        let e = add(&mut s, "user", unit(branch, base)).unwrap_err();
        assert!(e.contains(says), "{branch} from {base}: {e}");
    }
    for branch in ["feat/x", "chore/deps", "fix/login"] {
        add(&mut s, "user", unit(branch, "main")).unwrap();
    }
    let moved = |branch: &str, base: &str| json!({"kind": "unit.relocated", "body": {"unit": "u2", "repo": "/r", "branch": branch, "base": base}});
    assert!(add(&mut s, "user", moved("x2", "main")).unwrap_err().contains("starts with feat/"));
    add(&mut s, "user", moved("fix/x2", "main")).unwrap();

    // Written before the rule: straight into the records, as an old database has it.
    let conn = rusqlite::Connection::open(s.path()).unwrap();
    conn.execute("INSERT INTO events (at, kind, actor, intent, unit, body) VALUES ('2026-01-01T00:00:00Z', 'unit.created', 'user', 'i1', 'u5', ?1)", [json!({"id": "u5", "intent": "i1", "title": "ja", "repo": "/r", "branch": "ja-ui", "base": "main"}).to_string()]).unwrap();
    drop(conn);
    let mut s = Store::open(s.path()).unwrap();
    assert_eq!(s.state().unit("u5").unwrap().branch, "ja-ui");
    add(&mut s, "user", json!({"kind": "task.created", "body": {"unit": "u5", "kind": "implement", "title": "Translate"}})).unwrap();
    // Moved to another base, its name stays; a new name follows the rule.
    add(&mut s, "user", json!({"kind": "unit.relocated", "body": {"unit": "u5", "repo": "/r", "branch": "ja-ui", "base": "develop"}})).unwrap();
    assert!(add(&mut s, "user", json!({"kind": "unit.relocated", "body": {"unit": "u5", "repo": "/r", "branch": "ja-ui-2", "base": "develop"}})).unwrap_err().contains("starts with feat/"));
    assert_eq!(replayed(&s).unit("u5").unwrap().base, "develop");
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
    assert_eq!(s.state().intent_for(&RuntimeRef { id: "w1".into(), label: "chat".into(), machine: None }).map(|i| i.id.as_str()), Some("i1"), "the other active intent there is current again");
    let e = add(&mut s, "boss", json!({"kind": "unit.created", "body": {"intent": "i2", "title": "x", "repo": "/r", "branch": "feat/b", "base": "main"}})).unwrap_err();
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

/// Planned intents split too finely are merged into one: the others are dropped and what
/// they said goes into its note, in the order given; it keeps its place in the roadmap.
#[test]
fn planned_intents_merge() {
    let (_dir, mut s) = temp_store();
    let plan = |title: &str, project: &str| json!({"kind": "intent.created", "body": {"title": title, "goal": format!("{title} goal"), "project": project, "state": "planned"}});
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Search", "goal": "Find things", "project": "/p", "state": "planned", "note": "Own note\n"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Filters", "goal": "By date", "project": "/p", "state": "planned", "note": "see #12", "external": {"system": "jira", "key": "ABC-2", "url": "https://x/ABC-2"}}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Ranking", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", plan("Other project", "/q")).unwrap(); // i4
    add(&mut s, "user", plan("Has a task", "/p")).unwrap(); // i5
    // Tasks of the intent itself, from before research units.
    let mut s = recorded_before(s, "boss", json!({"kind": "task.created", "body": {"unit": "i5", "intent": "i5", "n": 1, "kind": "research", "title": "Look", "inputs": [{"from": "start"}]}}));
    add(&mut s, "user", plan("Has a chore", "/p")).unwrap(); // i6
    add(&mut s, "user", json!({"kind": "chore.created", "body": {"intent": "i6", "title": "Rotate"}})).unwrap();
    add(&mut s, "user", plan("Has a finding", "/p")).unwrap(); // i7
    add(&mut s, "user", json!({"kind": "finding.added", "body": {"project": "/p", "intent": "i7", "finding": {"severity": "P2", "problem": "Slow"}}})).unwrap();
    add(&mut s, "user", plan("Dropped", "/p")).unwrap(); // i8
    add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i8", "state": "dropped", "reason": "no"}})).unwrap();
    add(&mut s, "user", plan("Last", "/p")).unwrap(); // i9
    add(&mut s, "user", json!({"kind": "intent.moved", "body": {"intent": "i9", "before": "i1"}})).unwrap();
    let merge = |into: &str, from: &[&str]| json!({"kind": "intent.merged", "body": {"intent": into, "from": from}});

    for (into, from, says) in [
        ("i1", &[][..], "name the planned intents"),
        ("i1", &["i1"][..], "named twice"),
        ("i1", &["i2", "i2"][..], "named twice"),
        ("i1", &["i99"][..], "no intent i99"),
        ("i99", &["i2"][..], "no intent i99"),
        ("i8", &["i2"][..], "i8 is dropped"),
        ("i1", &["i8"][..], "i8 is dropped"),
        ("i1", &["i4"][..], "another project"),
        ("i1", &["i5"][..], "has tasks"),
        ("i1", &["i6"][..], "has chores"),
        ("i1", &["i7"][..], "has backlog findings"),
    ] {
        let e = add(&mut s, "user", merge(into, from)).unwrap_err();
        assert!(e.contains(says), "{into} <- {from:?}: {e}");
    }
    // What the user added, only the user merges; what agents added (an import), agents may tidy.
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": {"id": "w1", "label": "chat"}}})).unwrap();
    assert!(add(&mut s, "boss", merge("i1", &["i2"])).unwrap_err().contains("i1 was added by the user"));
    add(&mut s, "importer", plan("Import a", "/p")).unwrap(); // i10
    add(&mut s, "importer", plan("Import b", "/p")).unwrap(); // i11
    assert!(add(&mut s, "boss", merge("i10", &["i11", "i2"])).unwrap_err().contains("i2 was added by the user"));
    assert_eq!(s.state().intent("i10").unwrap().by, "importer");
    add(&mut s, "boss", merge("i10", &["i11"])).unwrap();
    assert_eq!(s.state().intent("i11").unwrap().reason.as_deref(), Some("merged into i10"));

    let before = s.state().roadmap.clone();
    add(&mut s, "user", merge("i1", &["i3", "i2"])).unwrap();
    let st = s.state();
    assert_eq!(st.intent("i1").unwrap().note, "Own note\n\nMerged from i3: Ranking\n\nMerged from i2: Filters\nBy date\nsee #12\njira:ABC-2 https://x/ABC-2");
    for f in ["i2", "i3"] {
        let x = st.intent(f).unwrap();
        assert_eq!((x.state, x.reason.as_deref()), (IntentState::Dropped, Some("merged into i1")));
    }
    let after: Vec<_> = before.iter().filter(|x| !["i2", "i3"].contains(&x.as_str())).cloned().collect();
    assert_eq!(st.roadmap, after, "the target keeps its place");
    assert_eq!(st.roadmap[..2], ["i9", "i1"]);
    assert_eq!(st.intent("i1").unwrap().external, None, "the target keeps its own external id");
    assert!(add(&mut s, "user", merge("i1", &["i2"])).unwrap_err().contains("i2 is dropped"), "merged once");
    assert_eq!(s.state().intent("i1").unwrap().title, "Search", "no title given: the target keeps its own");
    // A new title for the merged whole: never blank; given, the target takes it.
    let titled = |title: &str| json!({"kind": "intent.merged", "body": {"intent": "i1", "from": ["i9"], "title": title}});
    assert!(add(&mut s, "user", titled("  ")).unwrap_err().contains("the new title is empty"));
    add(&mut s, "user", titled(" Search and filters ")).unwrap();
    assert_eq!(s.state().intent("i1").unwrap().title, "Search and filters");
    assert!(s.state().intent("i1").unwrap().note.ends_with("Merged from i9: Last\nLast goal"));
    let replayed = replayed(&s);
    assert_eq!(serde_json::to_value(&replayed).unwrap(), serde_json::to_value(s.state()).unwrap());
}

/// Research and design are a unit of their own with no branch; research hands in a note or a
/// report. Nothing new goes into an intent itself, but its tasks recorded before that replay
/// and keep working.
#[test]
fn research_units_and_old_intent_level_work() {
    let (_dir, mut s) = temp_store();
    add(&mut s, "user", json!({"kind": "project.added", "body": {"path": "/p", "title": "P"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Sync", "project": "p1", "state": "planned"}})).unwrap();
    for kind in ["research", "design", "review", "implement"] {
        let e = add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "i1", "kind": kind, "title": "x"}})).unwrap_err();
        assert!(e.contains("tasks go into a unit") && e.contains("workriver unit add i1 --kind research"), "{e}");
    }
    let research = |extra: serde_json::Value| {
        let mut b = json!({"intent": "i1", "title": "Compare sync engines", "kind": "research"});
        b.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        json!({"kind": "unit.created", "body": b})
    };
    for extra in [json!({"branch": "feat/x"}), json!({"repo": "/r"}), json!({"base": "main"}), json!({"machine": "box"})] {
        assert!(add(&mut s, "boss", research(extra.clone())).unwrap_err().contains("has no repository, branch"), "{extra}");
    }
    let e = add(&mut s, "boss", research(json!({}))).unwrap();
    assert_eq!(e.body.refs(), (Some("i1".into()), Some("u1".into()), None));
    let u = s.state().unit("u1").unwrap().clone();
    assert_eq!((u.kind, u.repo.as_str(), u.branch.as_str(), u.has_code()), (UnitKind::Research, "/p", "", false), "it reads in the project's folder");
    // No implement or tests work, no commits, no merge, no move.
    for kind in ["implement", "tests"] {
        assert!(add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": kind, "title": "x"}})).unwrap_err().contains("is a research unit"));
    }
    assert!(add(&mut s, "user", json!({"kind": "unit.merged", "body": {"unit": "u1"}})).unwrap_err().contains("unit done u1"));
    assert!(add(&mut s, "user", json!({"kind": "unit.relocated", "body": {"unit": "u1", "repo": "/r", "branch": "feat/x", "base": "main"}})).unwrap_err().contains("research unit"));
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "research", "title": "Compare sync engines"}})).unwrap();
    assert!(add(&mut s, "user", json!({"kind": "commit.linked", "body": {"unit": "u1", "hash": "abc", "task": 1}})).unwrap_err().contains("research unit"));
    let e = add(&mut s, "r", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1}})).unwrap_err();
    assert!(e.contains("research task delivers"), "{e}");
    let e = add(&mut s, "r", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1, "note": "n", "findings": [{"severity": "P2", "problem": "p", "impact": "i", "where": ["a"]}]}})).unwrap_err();
    assert!(e.contains("findings"), "{e}");
    add(&mut s, "r", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1, "note": "Use CRDTs; see report"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "decide", "title": "Confirm", "inputs": [{"from": "1"}]}})).unwrap();
    assert_eq!(s.state().progress(&s.state().unit("u1").unwrap().clone()), Progress::Waiting);
    add(&mut s, "user", json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 2, "result": "done", "note": "CRDTs"}})).unwrap();
    assert_eq!(s.state().progress(&s.state().unit("u1").unwrap().clone()), Progress::Ready);
    add(&mut s, "user", json!({"kind": "unit.done", "body": {"unit": "u1", "reason": "decided"}})).unwrap();
    assert_eq!(s.state().unit("u1").unwrap().tasks.len(), 2);

    // Recorded before research units: the intent's own tasks replay and still take the
    // rest of their life (start, deliver, decide, a cause of work in a unit).
    let s = recorded_before(s, "boss", json!({"kind": "task.created", "body": {"unit": "i1", "intent": "i1", "n": 1, "kind": "research", "title": "Look", "inputs": [{"from": "start"}]}}));
    let s = recorded_before(s, "boss", json!({"kind": "task.created", "body": {"unit": "i1", "intent": "i1", "n": 2, "kind": "review", "reviewer": "code-review", "inputs": [{"from": "1"}]}}));
    let mut s = s;
    let t = s.state().task("i1", 1).unwrap();
    assert_eq!((t.state, t.intent.as_str()), (TaskState::Todo, "i1"));
    add(&mut s, "r", json!({"kind": "task.started", "body": {"unit": "i1", "task": 1}})).unwrap();
    add(&mut s, "r", json!({"kind": "task.delivered", "body": {"unit": "i1", "task": 1, "note": "Found it"}})).unwrap();
    add(&mut s, "r2", json!({"kind": "task.delivered", "body": {"unit": "i1", "task": 2, "findings": [{"severity": "P2", "problem": "p", "impact": "i", "where": ["a"]}]}})).unwrap();
    add(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "i1", "task": 2, "finding": "P2-1", "decision": "accept"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "Engine", "repo": "/r", "branch": "feat/b", "base": "main"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u2", "kind": "implement", "title": "Fix", "causes": [{"unit": "i1", "task": 2, "findings": ["P2-1"]}]}})).unwrap();
    assert_eq!(s.state().task("u2", 1).unwrap().intent, "i1", "a unit's tasks record their intent too");
    assert!(add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "i1", "kind": "research", "title": "More", "inputs": [{"from": "1"}]}})).unwrap_err().contains("tasks go into a unit"));
    assert_eq!(serde_json::to_value(replayed(&s)).unwrap(), serde_json::to_value(s.state()).unwrap());
}

/// A spike is code on a spike/ branch of its own: it takes every kind of task and commits,
/// moves like a code unit, is never merged, and is closed with what was learned.
#[test]
fn spike_units_keep_code_but_are_never_merged() {
    let (_dir, mut s) = temp_store();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Sync"}})).unwrap();
    let unit = |kind: &str, branch: &str| json!({"kind": "unit.created", "body": {"intent": "i1", "title": "Try CRDTs", "repo": "/r", "branch": branch, "base": "main", "kind": kind}});
    for (kind, branch, says) in [("spike", "feat/crdt", "--branch spike/crdt"), ("spike", "spike/", "a spike's branch starts with spike/"), ("spike", "main", "must not be its base"), ("code", "spike/crdt", "spike/<name> is for --kind spike")] {
        let e = add(&mut s, "boss", unit(kind, branch)).unwrap_err();
        assert!(e.contains(says), "{kind} {branch}: {e}");
    }
    add(&mut s, "boss", unit("spike", "spike/crdt")).unwrap();
    let u = s.state().unit("u1").unwrap().clone();
    assert_eq!((u.kind, u.branch.as_str(), u.has_code(), u.kind.merges()), (UnitKind::Spike, "spike/crdt", true, false));
    // Every kind of task.
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Prototype"}})).unwrap();
    for (kind, extra) in [("research", json!({"title": "Measure"})), ("review", json!({})), ("tests", json!({})), ("design", json!({"title": "Shape"})), ("gate", json!({"gate": "decide", "title": "Look"}))] {
        let mut b = json!({"unit": "u1", "kind": kind, "inputs": [{"from": "1"}]});
        b.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        add(&mut s, "boss", json!({"kind": "task.created", "body": b})).unwrap_or_else(|e| panic!("{kind}: {e}"));
    }
    add(&mut s, "user", json!({"kind": "commit.linked", "body": {"unit": "u1", "hash": "abc", "task": 1}})).unwrap();
    let moved = |branch: &str| json!({"kind": "unit.relocated", "body": {"unit": "u1", "repo": "/r", "branch": branch, "base": "main"}});
    assert!(add(&mut s, "user", moved("feat/crdt2")).unwrap_err().contains("spike/crdt2"));
    add(&mut s, "user", moved("spike/crdt2")).unwrap();
    let e = add(&mut s, "user", json!({"kind": "unit.merged", "body": {"unit": "u1"}})).unwrap_err();
    assert!(e.contains("is a spike") && e.contains("workriver unit done u1 --reason \"<what was learned>\""), "{e}");
    add(&mut s, "user", json!({"kind": "unit.done", "body": {"unit": "u1", "reason": "CRDTs are 3x slower"}})).unwrap();
    assert_eq!(s.state().progress(&s.state().unit("u1").unwrap().clone()), Progress::Done);
    assert_eq!(serde_json::to_value(replayed(&s)).unwrap(), serde_json::to_value(s.state()).unwrap());
    assert_eq!(replayed(&s).unit("u1").unwrap().kind, UnitKind::Spike);
}

/// The intent's manager or the user adds a paragraph to its note; nobody else.
#[test]
fn a_note_grows_by_paragraphs() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    let more = |text: &str| json!({"kind": "intent.updated", "body": {"intent": "i1", "add_note": text}});
    assert!(add(&mut s, "agent-b", more("x")).unwrap_err().contains("only the user or the manager"));
    assert!(add(&mut s, "user", more("  ")).unwrap_err().contains("empty"));
    add(&mut s, "user", more("First: see https://x/1\n")).unwrap();
    assert_eq!(s.state().intent("i1").unwrap().note, "First: see https://x/1");
    add(&mut s, "user", more("Second\nline")).unwrap();
    assert_eq!(s.state().intent("i1").unwrap().note, "First: see https://x/1\n\nSecond\nline");
    assert_eq!(replayed(&s).intent("i1").unwrap().note, s.state().intent("i1").unwrap().note);
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
    add(&mut s, "boss", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "Cleanup", "repo": "/r", "branch": "chore/c", "base": "main"}})).unwrap();
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
    add(&mut s, "boss", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "ja", "repo": "/r", "branch": "feat/ja-ui", "base": "main"}})).unwrap();
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
        json!({"kind": "project.renamed", "body": {"project": "/p", "title": "Q"}}),
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
    // A manager is an agent registering itself.
    let w = json!({"id": "w1", "label": "chat"});
    assert!(add(&mut s, "agent-a", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": w}})).is_err());
    assert!(add(&mut s, "user", json!({"kind": "manager.registered", "body": {"workspace": w}})).is_err());
    let e = add(&mut s, "agent-a", json!({"kind": "manager.registered", "body": {"workspace": w}})).unwrap();
    assert_eq!(e.body, body(json!({"kind": "manager.registered", "body": {"agent": "agent-a", "workspace": w}})));
    add(&mut s, "agent-a", json!({"kind": "manager.registered", "body": {"intent": "i1"}})).unwrap();
    // The intent's manager decides findings unless the user keeps them (nobody else says
    // which); no other agent ever does; and what an agent decides is an agent's decision.
    let decide = |d: &str| json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": d, "by": "user"}});
    assert_eq!(s.state().intent("i1").unwrap().findings, FindingsPolicy::Manager, "the default, for records from before too");
    assert!(add(&mut s, "agent-b", decide("accept")).unwrap_err().contains("for the manager of intent i1"));
    assert!(add(&mut s, "agent-a", json!({"kind": "intent.updated", "body": {"intent": "i1", "findings": "user"}})).unwrap_err().contains("only the user says who decides"));
    add(&mut s, "user", json!({"kind": "intent.updated", "body": {"intent": "i1", "findings": "user"}})).unwrap();
    assert!(add(&mut s, "agent-a", decide("accept")).unwrap_err().contains("is the user's to decide (intent i1: findings user)"));
    // The policy by severity is gone (severity is the reviewer's own word): its records are the stricter one.
    add(&mut s, "user", json!({"kind": "intent.updated", "body": {"intent": "i1", "findings": "manager-except-p1"}})).unwrap();
    assert_eq!(replayed(&s).intent("i1").unwrap().findings, FindingsPolicy::User);
    add(&mut s, "user", json!({"kind": "intent.updated", "body": {"intent": "i1", "findings": "manager"}})).unwrap();
    let e = add(&mut s, "agent-a", decide("reject")).unwrap();
    assert_eq!(e.body, body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "reject", "by": "agent"}})));
    // A decision is made once, except by the user, who may change the manager's (or their own).
    assert!(add(&mut s, "agent-a", decide("accept")).unwrap_err().contains("only the user changes a decision"));
    let over = add(&mut s, "user", decide("accept")).unwrap();
    assert_eq!(over.body, body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "accept", "by": "user", "was": "rejected"}})));
    assert!(add(&mut s, "user", decide("accept")).unwrap_err().contains("already accepted"));
    let f = replayed(&s).task("u1", 2).unwrap().findings[0].clone();
    assert_eq!((f.state, f.decided_by), (FindingState::Accepted, Some(Who::User)));
    // The user does all of it.
    for v in theirs {
        add(&mut s, "user", v).unwrap();
    }
}

/// An agent on a blind task sees its own task, its unit and its intent, nothing of anyone
/// else's work, and neither asks nor is asked; delivering ends it.
#[test]
fn a_blind_task_sees_only_its_own() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    assert!(add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "x", "blind": true}})).unwrap_err().contains("--blind is for"));
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "reviewer": "security-review", "text": "Look for ways around the mention check", "inputs": [{"from": "1"}], "agent": "sec", "blind": true}})).unwrap();
    add(&mut s, "boss", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "Other unit", "repo": "/r2", "branch": "feat/o", "base": "main"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "chore.created", "body": {"intent": "i1", "title": "Rotate the key"}})).unwrap();
    add(&mut s, "user", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": "Why a regex?"}})).unwrap();
    add(&mut s, "user", json!({"kind": "finding.added", "body": {"project": "/r", "finding": {"severity": "P2", "problem": "Old leak"}}})).unwrap();
    add(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "accept", "note": "Yes, fix"}})).unwrap();
    for (who, id) in [("sec", "aaaaaaaaaaaaaaaa"), ("agent-a", "bbbbbbbbbbbbbbbb")] {
        add(&mut s, who, json!({"kind": "attachment.added", "body": {"id": id, "name": "shot.png", "type": "image/png", "size": 3}})).unwrap();
    }

    assert_eq!(s.state().blind_tasks("sec"), [("u1".to_string(), 3)]);
    assert!(s.state().blind_view("agent-a").is_none() && s.state().blind_view("boss").is_none(), "only the agent on the blind task");
    let v = s.state().blind_view("sec").unwrap();
    assert!(v.reduced);
    assert_eq!(v.units.iter().map(|u| (u.id.as_str(), u.title.as_str(), u.branch.as_str(), u.tasks.len())).collect::<Vec<_>>(), [("u1", "Mentions", "feat/mention", 1)]);
    assert_eq!((v.units[0].tasks[0].n, v.units[0].tasks[0].text.as_str()), (3, "Look for ways around the mention check"));
    assert_eq!((v.intents.len(), v.intents[0].title.as_str()), (1, "Chat v2"));
    assert!(v.chores.is_empty() && v.questions.is_empty() && v.threads.is_empty() && v.backlog.is_empty() && v.roadmap.is_empty());
    assert_eq!(v.attachments.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["aaaaaaaaaaaaaaaa"], "only what it attached itself");
    let text = serde_json::to_string(&v).unwrap();
    for hidden in ["Find mentions", "npm test", "6 passed", "Duplicates", "Emoji names", "Two notices", "Yes, fix", "Other unit", "Rotate the key", "Why a regex", "Old leak"] {
        assert!(!text.contains(hidden), "{hidden} is hidden: {text}");
    }
    // It neither asks, nor is asked by another agent; the user may ask.
    assert!(add(&mut s, "sec", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": "What did the author intend?"}})).unwrap_err().contains("tell your manager"));
    assert!(add(&mut s, "agent-a", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 3, "text": "Seen my note?"}})).unwrap_err().contains("blind task in progress"));
    add(&mut s, "user", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 3, "text": "How far are you?"}})).unwrap();
    // Delivered: everything is back, and the task stays marked.
    add(&mut s, "sec", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 3, "findings": [{"severity": "P1", "problem": "Bypass", "impact": "i", "where": ["a"]}]}})).unwrap();
    assert!(s.state().blind_view("sec").is_none());
    assert!(s.state().task("u1", 3).unwrap().blind);
    assert!(replayed(&s).task("u1", 3).unwrap().blind);
}

/// Where a unit's work is: on the recorder's own machine by default; on another only by the
/// user, or by the intent's manager where a workspace there is linked with the intent's.
/// It can be corrected afterwards. And chores may belong to a project or to nothing.
#[test]
fn units_are_where_the_work_is() {
    let (_dir, mut s) = temp_store();
    let w1 = json!({"id": "w1", "label": "chat"});
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "A", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"workspace": w1}})).unwrap();
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": w1}})).unwrap();
    let unit = |machine: Option<&str>| json!({"kind": "unit.created", "body": {"intent": "i1", "title": "U", "repo": "/srv/app", "branch": "feat/f", "base": "main", "machine": machine}});
    // The manager is on this machine, the workers on box.
    let e = add(&mut s, "boss", unit(Some("box"))).unwrap_err();
    assert!(e.contains("workriver workspace link box:<workspace there> chat"), "{e}");
    add(&mut s, "user", json!({"kind": "workspace.linked", "body": {"machine": "box", "workspace": "work", "local": "chat"}})).unwrap();
    assert!(add(&mut s, "worker", unit(Some("box"))).unwrap_err().contains("only the user or the manager"));
    add(&mut s, "boss", unit(Some("box"))).unwrap();
    assert_eq!(s.state().unit("u1").unwrap().machine.as_deref(), Some("box"));
    // An agent over there records a unit over there without more ado; the user, anywhere.
    let far = Principal::Agent { session: "far".into(), machine: Some("box".into()) };
    s.append(&far, body(unit(Some("box")))).unwrap();
    assert!(s.append(&far, body(unit(Some("other")))).is_err());
    add(&mut s, "user", unit(Some("other"))).unwrap();
    add(&mut s, "boss", unit(None)).unwrap();

    // Recorded in the wrong place: moved, by the manager or the user only.
    let moved = json!({"kind": "unit.relocated", "body": {"unit": "u4", "machine": "box", "repo": "/home/me/app", "branch": "feat/feature", "base": "main"}});
    assert!(add(&mut s, "worker", moved.clone()).unwrap_err().contains("moves its units"));
    add(&mut s, "boss", moved).unwrap();
    let u = replayed(&s).unit("u4").unwrap().clone();
    assert_eq!((u.machine.as_deref(), u.repo.as_str(), u.branch.as_str()), (Some("box"), "/home/me/app", "feat/feature"));
    assert!(add(&mut s, "boss", json!({"kind": "unit.relocated", "body": {"unit": "u4", "machine": "nowhere", "repo": "/x", "branch": "feat/f", "base": "main"}})).unwrap_err().contains("no workspace on nowhere is linked"));

    // Chores: an intent's, a project's, nobody's.
    add(&mut s, "boss", json!({"kind": "chore.created", "body": {"intent": "i1", "title": "Rotate the key"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "chore.created", "body": {"project": "/p", "title": "Renew the certificate"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "chore.created", "body": {"title": "Pay the invoice"}})).unwrap();
    assert!(add(&mut s, "boss", json!({"kind": "chore.created", "body": {"project": "/p", "unit": "u1", "title": "x"}})).is_err(), "a unit's chore is its intent's");
    let c = &replayed(&s).chores;
    assert_eq!(c.iter().map(|c| (c.intent.as_str(), c.project.as_str())).collect::<Vec<_>>(), [("i1", ""), ("", "/p"), ("", "")]);
}

/// After a review is delivered and its findings decided, its reviewer can still add to a
/// finding (never rewriting it) and add new findings, numbered after the ones there.
#[test]
fn findings_grow_after_they_are_decided() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"intent": "i1"}})).unwrap();
    add(&mut s, "agent-b", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "inputs": [{"from": "1"}]}})).unwrap();
    add(&mut s, "agent-b", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 3, "findings": [{"severity": "P1", "problem": "Bypass", "impact": "Anyone gets in", "where": ["auth.rs"]}]}})).unwrap();
    add(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 3, "finding": "P1-1", "decision": "accept"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Close the bypass", "causes": [{"task": 3, "findings": ["P1-1"]}], "agent": "fixer"}})).unwrap();

    // A note: by its reviewer, the manager or the user; not by anyone else; never empty.
    let note = |text: &str| json!({"kind": "finding.noted", "body": {"unit": "u1", "task": 3, "finding": "P1-1", "text": text}});
    assert!(add(&mut s, "fixer", note("I would rather not")).unwrap_err().contains("the agent that delivered it, the manager of its intent, or the user"));
    assert!(add(&mut s, "agent-b", note(" ")).unwrap_err().contains("a note needs its text"));
    assert!(add(&mut s, "agent-b", json!({"kind": "finding.noted", "body": {"unit": "u1", "task": 3, "finding": "P9-9", "text": "x"}})).unwrap_err().contains("no finding P9-9"));
    add(&mut s, "agent-b", note("The fix needs a test with an expired token")).unwrap();
    add(&mut s, "boss", note("Agreed, and one for a revoked token")).unwrap();
    add(&mut s, "user", note("Do both")).unwrap();
    let st = replayed(&s);
    let f = &st.task("u1", 3).unwrap().findings[0];
    assert_eq!((f.found.problem.as_str(), f.state), ("Bypass", FindingState::Accepted), "the finding itself is as reported");
    assert_eq!(f.notes.iter().map(|n| (n.by.as_str(), n.text.as_str())).collect::<Vec<_>>(), [("agent-b", "The fix needs a test with an expired token"), ("boss", "Agreed, and one for a revoked token"), ("user", "Do both")]);
    assert_eq!(st.follow_ups("u1", 3, "P1-1").iter().map(|(u, t)| (u.as_str(), t.n)).collect::<Vec<_>>(), [("u1", 4)]);

    // More findings: replacing is refused once any is decided, adding is not, and they are
    // numbered after the ones there, per severity; by the reviewer or the user only.
    let more = json!({"kind": "task.amended", "body": {"unit": "u1", "task": 3, "add": [
        {"severity": "P1", "problem": "Second bypass", "impact": "i", "where": ["a"]}, {"severity": "P3", "problem": "Name", "impact": "i", "where": ["a"]}, {"severity": "P1", "problem": "Third", "impact": "i", "where": ["a"]}]}});
    assert!(add(&mut s, "boss", more.clone()).unwrap_err().contains("added by the agent that delivered it"));
    assert!(add(&mut s, "agent-b", json!({"kind": "task.amended", "body": {"unit": "u1", "task": 3, "add": [{"severity": "P2", "problem": "No where"}]}})).is_err());
    add(&mut s, "agent-b", more).unwrap();
    let st = replayed(&s);
    let t = st.task("u1", 3).unwrap();
    assert_eq!(t.findings.iter().map(|f| (f.found.id.as_str(), f.state)).collect::<Vec<_>>(), [("P1-1", FindingState::Accepted), ("P1-2", FindingState::Open), ("P3-1", FindingState::Open), ("P1-3", FindingState::Open)]);
    assert_eq!(t.findings[0].notes.len(), 3, "what was there is untouched");
    // A blind agent sees none of it.
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "reviewer": "security-review", "inputs": [{"from": "1"}], "agent": "sec", "blind": true}})).unwrap();
    assert!(!serde_json::to_string(&s.state().blind_view("sec").unwrap()).unwrap().contains("expired token"));
}

/// A gate planned ahead is still to do until every task it follows is done: it asks nobody
/// for anything, holds up no unit, cannot be submitted; then it waits, from that moment.
/// Records made before replay the same way.
#[test]
fn a_gate_waits_only_once_what_it_follows_is_done() {
    let (_dir, mut s) = temp_store();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "A"}})).unwrap();
    add(&mut s, "user", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "U", "repo": "/r", "branch": "feat/f", "base": "main"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Build"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Docs", "inputs": [{"from": "start"}]}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "deploy", "title": "Deploy", "inputs": [{"from": "1"}, {"from": "2", "primary": false}]}})).unwrap();
    let gate = |s: &Store| s.state().task("u1", 3).unwrap().clone();
    assert_eq!((gate(&s).state, gate(&s).waiting_since.clone()), (TaskState::Todo, None));
    assert_eq!(s.state().progress(&s.state().units[0]), Progress::Working, "a gate to do holds nothing up");
    let e = add(&mut s, "user", json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 3, "result": "done"}})).unwrap_err();
    assert!(e.contains("gate u1#3 is not open yet: it follows u1#1, u1#2, not done yet"), "{e}");
    for n in [1, 2] {
        add(&mut s, "w", json!({"kind": "task.started", "body": {"unit": "u1", "task": n}})).unwrap();
    }
    add(&mut s, "w", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1, "evidence": [{"command": "t", "result": "passed", "summary": "ok"}]}})).unwrap();
    assert_eq!(gate(&s).state, TaskState::Todo, "one of the two is done");
    let last = add(&mut s, "w", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 2, "evidence": [{"command": "t", "result": "passed", "summary": "ok"}]}})).unwrap();
    assert_eq!((gate(&s).state, gate(&s).waiting_since.clone()), (TaskState::Waiting, Some(last.at.clone())), "it waits from when the last one was done");
    assert_eq!(s.state().progress(&s.state().units[0]), Progress::Waiting);
    assert_eq!(replayed(&s).task("u1", 3).unwrap().state, TaskState::Waiting);
    add(&mut s, "user", json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 3, "result": "done"}})).unwrap();
    // A gate whose inputs are done when it is made waits at once.
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "test", "title": "Try", "inputs": [{"from": "1"}]}})).unwrap();
    assert_eq!(s.state().task("u1", 4).unwrap().state, TaskState::Waiting);
}

/// A unit closed as done takes a new task as more work: it is open again, its state worked out
/// from its tasks as before (the closing is kept as what it was, `reopened`); a merged one
/// takes none.
#[test]
fn a_task_added_to_a_done_unit_opens_it_again() {
    let (_dir, mut s) = temp_store();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "A"}})).unwrap();
    add(&mut s, "user", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "U", "repo": "/r", "branch": "feat/f", "base": "main"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Build"}})).unwrap();
    add(&mut s, "w", json!({"kind": "task.started", "body": {"unit": "u1", "task": 1}})).unwrap();
    add(&mut s, "w", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1, "evidence": [{"command": "t", "result": "passed", "summary": "ok"}]}})).unwrap();
    add(&mut s, "user", json!({"kind": "unit.done", "body": {"unit": "u1", "reason": "enough"}})).unwrap();
    assert_eq!(s.state().progress(&s.state().units[0]), Progress::Done);
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "One more", "inputs": [{"from": "1"}]}})).unwrap();
    let u = s.state().units[0].clone();
    assert_eq!((u.done.is_none(), s.state().progress(&u)), (true, Progress::Working), "open again, working on the new task");
    assert_eq!(u.reopened.as_ref().map(|c| c.reason.as_str()), Some("enough"), "what it was closed as is kept");
    assert_eq!(replayed(&s).progress(&replayed(&s).units[0]), Progress::Working);
    // Closed again once that is through.
    add(&mut s, "w", json!({"kind": "task.started", "body": {"unit": "u1", "task": 2}})).unwrap();
    add(&mut s, "w", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 2, "evidence": [{"command": "t", "result": "passed", "summary": "ok"}]}})).unwrap();
    add(&mut s, "user", json!({"kind": "unit.done", "body": {"unit": "u1", "reason": "now"}})).unwrap();
    assert_eq!(s.state().progress(&s.state().units[0]), Progress::Done);
}

/// A running task stuck on what only the user can do: a gate put before it (a blocker) waits
/// for the user at once and the task now follows it, so the gate's result goes to its agent.
/// Its own agent, its manager or the user puts it there; nobody else.
#[test]
fn a_blocker_is_put_before_a_running_task() {
    let (_dir, mut s) = temp_store();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "A"}})).unwrap();
    add(&mut s, "user", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "U", "repo": "/r", "branch": "feat/f", "base": "main"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Build"}})).unwrap();
    add(&mut s, "w", json!({"kind": "task.started", "body": {"unit": "u1", "task": 1}})).unwrap();
    add(&mut s, "w", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1, "evidence": [{"command": "t", "result": "passed", "summary": "ok"}]}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Push", "inputs": [{"from": "1"}]}})).unwrap();
    add(&mut s, "w", json!({"kind": "task.started", "body": {"unit": "u1", "task": 2}})).unwrap();
    let blocker = || json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "test", "title": "Allow push", "blocks": 2}});
    let e = add(&mut s, "x", blocker()).unwrap_err();
    assert!(e.contains("a blocker before u1#2 is put by the agent of u1#2"), "{e}");
    let e = add(&mut s, "w", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "X", "blocks": 2}})).unwrap_err();
    assert!(e.contains("only a gate blocks a task"), "{e}");
    let e = add(&mut s, "w", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "test", "title": "Y", "blocks": 1}})).unwrap_err();
    assert!(e.contains("u1#1 is done"), "{e}");
    add(&mut s, "w", blocker()).unwrap();
    let gate = s.state().task("u1", 3).unwrap().clone();
    assert_eq!((gate.state, gate.blocks), (TaskState::Waiting, Some(2)), "it waits for the user at once");
    assert_eq!(gate.inputs, vec![Input { from: "1".into(), primary: true }], "it stands where the blocked task starts");
    let stuck = s.state().task("u1", 2).unwrap().clone();
    assert!(stuck.inputs.contains(&Input { from: "3".into(), primary: false }), "the blocked task follows it: {:?}", stuck.inputs);
    assert_eq!(stuck.state, TaskState::Running, "and goes on running");
    assert_eq!(replayed(&s).task("u1", 2).unwrap().inputs, stuck.inputs);
    // The user may put one too.
    add(&mut s, "user", blocker()).unwrap();
}

#[test]
fn the_user_renames_a_project() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("w.db");
    {
        let mut s = Store::open(&path).unwrap();
        add(&mut s, "user", json!({"kind": "project.added", "body": {"path": "/p", "title": "P"}})).unwrap();
        assert!(add(&mut s, "user", json!({"kind": "project.renamed", "body": {"project": "/nope", "title": "Q"}})).unwrap_err().contains("no project /nope"));
        assert!(add(&mut s, "user", json!({"kind": "project.renamed", "body": {"project": "p1", "title": " "}})).is_err());
        // By folder or by id; recorded by id.
        let e = add(&mut s, "user", json!({"kind": "project.renamed", "body": {"project": "/p", "title": "Q"}})).unwrap();
        assert_eq!(e.body, body(json!({"kind": "project.renamed", "body": {"project": "p1", "title": "Q"}})));
        add(&mut s, "user", json!({"kind": "project.renamed", "body": {"project": "p1", "title": "R"}})).unwrap();
    }
    assert_eq!(Store::open(&path).unwrap().state().projects[0].title, "R", "replayed");
}

/// A task no longer needed is dropped, never deleted: by the intent's manager, the user, or
/// the agent that recorded it, with a reason, while it is not done. It then holds nothing
/// up (a gate after it opens, its blind agent sees again), leaves what waits for the user,
/// is not started, delivered or submitted any more, and replays the same.
#[test]
fn a_task_no_longer_needed_is_dropped() {
    let (_dir, mut s) = temp_store();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "A"}})).unwrap();
    add(&mut s, "user", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "U", "repo": "/r", "branch": "feat/f", "base": "main"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"intent": "i1"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Build", "agent": "w"}})).unwrap();
    add(&mut s, "w", json!({"kind": "task.started", "body": {"unit": "u1", "task": 1}})).unwrap();
    add(&mut s, "w", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1, "evidence": [{"command": "t", "result": "passed", "summary": "ok"}]}})).unwrap();
    // Asked twice: the branch changed after the first merge gate.
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "decide", "title": "Merge u1 into main?", "inputs": [{"from": "1"}]}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "decide", "title": "Merge u1 into main? (again)", "inputs": [{"from": "1"}]}})).unwrap();
    add(&mut s, "w", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Docs", "inputs": [{"from": "1"}]}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "After the merge", "inputs": [{"from": "2"}]}})).unwrap();
    let drop = |n: u32, reason: &str| json!({"kind": "task.dropped", "body": {"unit": "u1", "task": n, "reason": reason}});

    // Who, and with what.
    assert!(add(&mut s, "boss", drop(2, " ")).unwrap_err().contains("needs a reason"));
    assert!(add(&mut s, "x", drop(2, "mine now")).unwrap_err().contains("by the manager of its intent or the user"));
    assert!(add(&mut s, "w", drop(2, "not mine")).unwrap_err().contains("by the manager of its intent or the user"));
    assert!(add(&mut s, "boss", drop(1, "late")).unwrap_err().contains("u1#1 is done"));
    add(&mut s, "w", drop(4, "folded into #1")).unwrap();
    let e = add(&mut s, "boss", drop(2, "superseded by u1#3")).unwrap();
    let gate = s.state().task("u1", 2).unwrap().clone();
    assert_eq!(gate.state, TaskState::Dropped);
    assert_eq!(gate.dropped, Some(Closed { at: e.at.clone(), by: "boss".into(), reason: "superseded by u1#3".into() }));
    assert!(add(&mut s, "boss", drop(2, "again")).unwrap_err().contains("was dropped as no longer needed (superseded by u1#3)"));

    // Nothing waits on it: what follows it may start; only the second gate waits for the user.
    assert!(s.state().unfinished_inputs("u1", s.state().task("u1", 5).unwrap()).is_empty());
    let waiting: Vec<u32> = s.state().units[0].tasks.iter().filter(|t| t.state == TaskState::Waiting).map(|t| t.n).collect();
    assert_eq!(waiting, [3]);
    assert!(add(&mut s, "user", json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 2, "result": "done"}})).unwrap_err().contains("was dropped"));
    assert!(add(&mut s, "w", json!({"kind": "task.started", "body": {"unit": "u1", "task": 4}})).unwrap_err().contains("u1#4 was dropped"));
    assert!(add(&mut s, "w", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 4, "note": "x"}})).unwrap_err().contains("u1#4 was dropped"));
    add(&mut s, "user", json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 3, "result": "done"}})).unwrap();
    assert!(add(&mut s, "user", drop(3, "late")).unwrap_err().contains("is done"), "a submitted gate is done");

    // A gate opens once what it follows is dropped.
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Polish", "inputs": [{"from": "1"}]}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "test", "title": "Try", "inputs": [{"from": "6"}]}})).unwrap();
    assert_eq!(s.state().task("u1", 7).unwrap().state, TaskState::Todo);
    add(&mut s, "user", drop(6, "not worth it")).unwrap();
    assert_eq!(s.state().task("u1", 7).unwrap().state, TaskState::Waiting);

    // A blind agent whose task is dropped is no longer blind.
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "inputs": [{"from": "1"}], "agent": "rev", "blind": true}})).unwrap();
    assert!(!s.state().blind_tasks("rev").is_empty());
    add(&mut s, "boss", drop(8, "the code changed")).unwrap();
    assert!(s.state().blind_tasks("rev").is_empty());

    let again = replayed(&s);
    assert_eq!(again.task("u1", 2).unwrap(), &gate);
    assert_eq!(again.task("u1", 7).unwrap().state, TaskState::Waiting);
}

/// Work handed out for an accepted finding and then dropped answers nothing: the unit waits
/// for work on the finding again.
#[test]
fn a_dropped_follow_up_answers_nothing() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    for f in ["P2-1", "P2-2"] {
        add(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": f, "decision": "accept"}})).unwrap();
    }
    for f in ["P2-1", "P2-2"] {
        add(&mut s, "user", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": f, "causes": [{"task": 2, "findings": [f]}]}})).unwrap();
    }
    add(&mut s, "user", json!({"kind": "task.dropped", "body": {"unit": "u1", "task": 4, "reason": "fixed by #3"}})).unwrap();
    add(&mut s, "w", json!({"kind": "task.started", "body": {"unit": "u1", "task": 3}})).unwrap();
    add(&mut s, "w", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 3, "evidence": [{"command": "t", "result": "passed", "summary": "ok"}]}})).unwrap();
    assert_eq!(s.state().progress(&s.state().units[0]), Progress::Waiting);
    // Dropping a task leaves the findings it was about as they are.
    assert_eq!(s.state().task("u1", 2).unwrap().findings[1].state, FindingState::Accepted);
}

/// The first task of a unit or intent starts at the start; a later one says what it follows
/// (a task, `start` to run beside the first, or a cause in the same unit), else it is refused
/// rather than hung off the start away from the work it continues.
#[test]
fn a_later_task_says_what_it_follows() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    let task = |v: serde_json::Value| json!({"kind": "task.created", "body": v});
    let e = add(&mut s, "boss", task(json!({"unit": "u1", "kind": "implement", "title": "More"}))).unwrap_err();
    assert!(e.contains("u1 already has tasks") && e.contains("--after <n> (the latest is #2)") && e.contains("--after start"), "{e}");
    let inputs = |s: &Store, u: &str, n: u32| s.state().task(u, n).unwrap().inputs.iter().map(|i| i.from.clone()).collect::<Vec<_>>();
    add(&mut s, "boss", task(json!({"unit": "u1", "kind": "implement", "title": "Beside", "inputs": [{"from": "start"}]}))).unwrap();
    assert_eq!(inputs(&s, "u1", 3), ["start"]);
    add(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "accept"}})).unwrap();
    add(&mut s, "boss", task(json!({"unit": "u1", "kind": "implement", "title": "Fix", "causes": [{"task": 2, "findings": ["P2-1"]}]}))).unwrap();
    assert_eq!(inputs(&s, "u1", 4), ["2"]);
    // A research unit the same: the first starts at the start, the next must say; a cause in
    // another unit is not one of its tasks.
    add(&mut s, "boss", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "Look around", "kind": "research"}})).unwrap();
    add(&mut s, "boss", task(json!({"unit": "u2", "kind": "research", "title": "Look"}))).unwrap();
    assert_eq!(inputs(&s, "u2", 1), ["start"]);
    let e = add(&mut s, "boss", task(json!({"unit": "u2", "kind": "research", "title": "Again", "causes": [{"unit": "u1", "task": 2, "findings": ["P2-1"]}]}))).unwrap_err();
    assert!(e.contains("u2 already has tasks"), "{e}");
    add(&mut s, "boss", task(json!({"unit": "u2", "kind": "research", "title": "Again", "inputs": [{"from": "1"}]}))).unwrap();
}

/// The user asks about a unit itself (task 0): its own threads, attachments of its tasks;
/// agents still ask about a task. Old questions (every one names a task) replay unchanged.
#[test]
fn the_user_asks_about_a_unit() {
    let (_dir, mut s) = temp_store();
    seeded(&mut s);
    let ask = |task: u32, thread: &str| json!({"kind": "question.asked", "body": {"unit": "u1", "task": task, "thread": thread, "text": "Why this order?", "context": [{"kind": "unit", "label": "Mentions"}]}});
    let e = add(&mut s, "user", ask(0, "")).unwrap();
    assert_eq!(e.body.refs(), (None, Some("u1".into()), None), "about the unit, no task");
    // Without `task` at all: the unit too.
    add(&mut s, "user", json!({"kind": "question.asked", "body": {"unit": "u1", "text": "And?"}})).unwrap();
    assert_eq!((s.state().questions[1].task, s.state().threads[1].task), (0, 0));
    // A follow-up stays in the unit's thread; a task's thread is another conversation.
    add(&mut s, "user", ask(0, "q1")).unwrap();
    assert!(add(&mut s, "user", ask(1, "q1")).unwrap_err().contains("about another task"));
    add(&mut s, "user", ask(1, "")).unwrap();
    assert!(add(&mut s, "user", ask(0, "q4")).unwrap_err().contains("about another task"));
    // Agents do not ask about whole units; nobody asks about a unit that is not there.
    assert!(add(&mut s, "agent-b", ask(0, "")).unwrap_err().contains("workriver ask <unit>#<n>"));
    assert!(add(&mut s, "user", json!({"kind": "question.asked", "body": {"unit": "u9", "text": "?"}})).unwrap_err().contains("no unit u9"));
    // An attachment shown with one of its tasks may be pointed at; another one not.
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": "rep", "name": "r.md", "type": "text/markdown", "size": 1, "unit": "u1", "task": 2}})).unwrap();
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": "loose", "name": "x.md", "type": "text/markdown", "size": 1}})).unwrap();
    let at = |id: &str| json!({"kind": "question.asked", "body": {"unit": "u1", "task": 0, "text": "?", "context": [{"kind": "attachment", "label": "r.md", "id": id}]}});
    add(&mut s, "user", at("rep")).unwrap();
    assert!(add(&mut s, "user", at("loose")).unwrap_err().contains("not one of u1#0's"));
    // Answered by the agent started for it, with when.
    add(&mut s, "workriver", json!({"kind": "question.started", "body": {"question": "q1", "agent": "s-why", "name": "why-q1"}})).unwrap();
    add(&mut s, "s-why", json!({"kind": "question.answered", "body": {"question": "q1", "text": "Parse first."}})).unwrap();
    assert!(s.state().question("q1").unwrap().answered_at.is_some());
    assert_eq!(replayed(&s), *s.state());
}

/// The user writes to a workspace's manager; only that manager replies, workriver alone
/// records where the message is, and it all replays.
#[test]
fn the_user_talks_to_a_manager() {
    let (_dir, mut s) = temp_store();
    let w1 = json!({"id": "w1", "label": "chat"});
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Mentions", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Search", "state": "planned"}})).unwrap();
    let say = |intent: &str, workspace: &str| json!({"kind": "message.sent", "body": {"intent": intent, "workspace": workspace, "manager": "forged", "text": "Pause u2 until the API settles."}});
    // Nobody to tell yet.
    assert!(add(&mut s, "user", say("i1", "")).unwrap_err().contains("intent i1 has no manager"));
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": w1}})).unwrap();
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": w1}})).unwrap();
    // Only the user writes; the manager is the workspace's, whatever the request says.
    assert!(add(&mut s, "boss", say("i1", "")).unwrap_err().contains("only the user does this (message.sent)"));
    let e = add(&mut s, "user", say("i1", "")).unwrap();
    let Body::MessageSent { id, workspace, manager, .. } = &e.body else { panic!() };
    assert_eq!((id.as_str(), workspace.as_str(), manager.as_str()), ("m1", "chat", "boss"));
    assert_eq!(e.body.refs().0.as_deref(), Some("i1"));
    // By workspace alone, when nothing of an intent is picked.
    add(&mut s, "user", say("", "chat")).unwrap();
    assert!(add(&mut s, "user", say("", "nowhere")).unwrap_err().contains("workspace \"nowhere\" has no manager"));
    assert!(add(&mut s, "user", say("i2", "")).unwrap_err().contains("intent i2 has no manager"), "planned: it runs nowhere");
    // Where it is: workriver's word only.
    let delivery = |m: &str| json!({"kind": "message.delivery", "body": {"id": m, "step": "delivered", "to": "boss", "reason": null, "message": "[workriver] ..."}});
    assert!(add(&mut s, "boss", delivery("m1")).unwrap_err().contains("only workriver"));
    assert!(add(&mut s, "user", delivery("m1")).unwrap_err().contains("only workriver"));
    assert!(add(&mut s, "workriver", delivery("m9")).unwrap_err().contains("no message m9"));
    add(&mut s, "workriver", delivery("m1")).unwrap();
    assert_eq!(s.state().message("m1").unwrap().delivery.as_ref().map(|d| d.step), Some(DeliveryStep::Delivered));
    // Replies: the manager it went to, and nobody else.
    let reply = |m: &str| json!({"kind": "message.replied", "body": {"message": m, "text": "Paused."}});
    assert!(add(&mut s, "worker", reply("m1")).unwrap_err().contains("only that manager replies"));
    assert!(add(&mut s, "user", reply("m1")).unwrap_err().contains("only that manager replies"));
    assert!(add(&mut s, "boss", reply("m9")).unwrap_err().contains("no message m9"));
    add(&mut s, "boss", reply("m1")).unwrap();
    add(&mut s, "boss", reply("m1")).unwrap();
    let m = s.state().message("m1").unwrap();
    assert_eq!((m.replies.len(), m.replies[0].by.as_str(), m.replies[0].text.as_str()), (2, "boss", "Paused."));
    assert_eq!(replayed(&s), *s.state());
}

/// A workspace's manager starts a message to the user (into its workspace's conversation);
/// nobody else does. Only the user records reading a conversation; it only moves forward,
/// and it all replays.
#[test]
fn a_manager_says_and_the_user_reads() {
    let (_dir, mut s) = temp_store();
    let w1 = json!({"id": "w1", "label": "chat"});
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Mentions", "state": "planned"}})).unwrap();
    let say = |text: &str| json!({"kind": "message.said", "body": {"workspace": "forged", "text": text}});
    assert!(add(&mut s, "boss", say("Hi")).unwrap_err().contains("only a workspace's manager"), "not registered yet");
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": w1}})).unwrap();
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": w1}})).unwrap();
    // A worker is pointed to its manager; the user writes from the page; nothing blank.
    assert!(add(&mut s, "worker", say("Hi")).unwrap_err().contains("workriver manager tell"));
    assert!(add(&mut s, "user", say("Hi")).unwrap_err().contains("workriver's page"));
    assert!(add(&mut s, "boss", say("  ")).unwrap_err().contains("--text"));
    // Into the workspace it manages, about its current intent, whatever the request says.
    let e = add(&mut s, "boss", say("u1 is merged.")).unwrap();
    let Body::MessageSaid { id, intent, workspace, .. } = &e.body else { panic!() };
    assert_eq!((id.as_str(), intent.as_str(), workspace.as_str()), ("m1", "i1", "chat"));
    assert_eq!(e.body.refs().0.as_deref(), Some("i1"));
    let m = s.state().message("m1").unwrap();
    assert!(m.by_manager && m.delivery.is_none());
    assert_eq!((m.manager.as_str(), m.text.as_str()), ("boss", "u1 is merged."));
    // The user answers it from the same box: an ordinary message; the manager may follow up.
    add(&mut s, "user", json!({"kind": "message.sent", "body": {"workspace": "chat", "text": "Thanks."}})).unwrap();
    add(&mut s, "boss", json!({"kind": "message.replied", "body": {"message": "m1", "text": "Also u2."}})).unwrap();
    // Reading: the user's alone; a time, kept as the service writes times; never backwards.
    let read = |at: &str| json!({"kind": "messages.read", "body": {"workspace": "chat", "at": at}});
    assert!(add(&mut s, "boss", read("2026-11-18T10:00:00Z")).unwrap_err().contains("only the user does this (messages.read)"));
    assert!(add(&mut s, "user", read("soon")).unwrap_err().contains("not a time"));
    add(&mut s, "user", read("2026-11-18T10:00:05.250Z")).unwrap();
    assert_eq!(s.state().read["chat"], "2026-11-18T10:00:05Z");
    add(&mut s, "user", read("2026-11-18T09:00:00Z")).unwrap();
    assert_eq!(s.state().read["chat"], "2026-11-18T10:00:05Z");
    assert_eq!(replayed(&s), *s.state());
}
