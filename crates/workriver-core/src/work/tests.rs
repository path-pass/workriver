use serde_json::json;

use crate::records::tests::*;
use crate::*;

/// An architecture review is a review like the others: recorded under its name, kept when
/// the events are read again, and it reports findings.
#[test]
fn an_architecture_review_round_trips() {
    let mut s = Records::default();
    seeded(&mut s);
    s.append(&"boss".into(), body(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "reviewer": "architecture-review", "inputs": [{"from": "1"}]}}))).unwrap();
    s.append(&"arch".into(), body(json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 3, "findings": [{"severity": "P2", "problem": "The scan reads the store", "impact": "Layers cross", "where": ["scan/lib.rs"]}]}}))).unwrap();
    let st = replayed(&s);
    let review = st.task("u1", 3).unwrap();
    assert_eq!((review.kind, review.reviewer, review.findings.len()), (TaskKind::Review, Some(Reviewer::ArchitectureReview), 1));
    assert_eq!(serde_json::to_value(&st).unwrap()["units"][0]["tasks"][2]["reviewer"], "architecture-review");
    assert_eq!(name(&Reviewer::ArchitectureReview), "architecture-review");
}

/// What a task comes after, which gate asks about a design, and which design revises it.
#[test]
fn designs_their_gates_and_their_revisions() {
    let mut s = Records::default();
    fn put(s: &mut Records, actor: &str, v: serde_json::Value) {
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
    let st = |s: &Records| s.state().clone();
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
fn accepted_finding_leads_to_task_with_derived_inputs() {
    let mut s = Records::default();
    seeded(&mut s);
    s.append(&"user".into(), body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-2", "decision": "accept", "by": "user"}}))).unwrap();
    let e = s.append(&"manager".into(), body(json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "fix", "causes": [{"task": 2, "findings": ["P2-2"]}]}}))).unwrap();
    let Body::TaskCreated(t) = e.body else { panic!() };
    assert_eq!(t.n, 3);
    assert_eq!(t.inputs, [Input { from: "2".into(), primary: true }]);
}

#[test]
fn corrections_after_delivery() {
    let mut s = Records::default();
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
    // The goal: the user's only.
    let other = s.append(&"agent-b".into(), body(json!({"kind": "intent.updated", "body": {"intent": "i1", "goal": "x"}}))).unwrap_err().to_string();
    assert!(other.contains("are the user's"), "{other}");
    s.append(&"user".into(), body(json!({"kind": "intent.updated", "body": {"intent": "i1", "goal": "Let people mention each other"}}))).unwrap();
    assert_eq!(s.state().intents[0].goal, "Let people mention each other");
}

#[test]
fn design_tasks_decide_gates_and_revised_findings() {
    let mut s = Records::default();
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

/// Deferred findings, from a task or added to the backlog, are batched (scheduled), and done
/// when the task they caused is delivered; or dropped with a reason.
#[test]
fn deferred_batches() {
    let mut s = Records::default();
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

/// After a review is delivered and its findings decided, its reviewer can still add to a
/// finding (never rewriting it) and add new findings, numbered after the ones there.
#[test]
fn findings_grow_after_they_are_decided() {
    let mut s = Records::default();
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
    let mut s = Records::default();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "A"}})).unwrap();
    add(&mut s, "user", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "U", "repo": "/r", "branch": "feat/f", "base": "main"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Build"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Docs", "inputs": [{"from": "start"}]}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "deploy", "title": "Deploy", "inputs": [{"from": "1"}, {"from": "2", "primary": false}]}})).unwrap();
    let gate = |s: &Records| s.state().task("u1", 3).unwrap().clone();
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
    let mut s = Records::default();
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
    let mut s = Records::default();
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

/// A task no longer needed is dropped, never deleted: by the intent's manager, the user, or
/// the agent that recorded it, with a reason, while it is not done. It then holds nothing
/// up (a gate after it opens, its blind agent sees again), leaves what waits for the user,
/// is not started, delivered or submitted any more, and replays the same.
#[test]
fn a_task_no_longer_needed_is_dropped() {
    let mut s = Records::default();
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
    let mut s = Records::default();
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
    let mut s = Records::default();
    seeded(&mut s);
    let task = |v: serde_json::Value| json!({"kind": "task.created", "body": v});
    let e = add(&mut s, "boss", task(json!({"unit": "u1", "kind": "implement", "title": "More"}))).unwrap_err();
    assert!(e.contains("u1 already has tasks") && e.contains("--after <n> (the latest is #2)") && e.contains("--after start"), "{e}");
    let inputs = |s: &Records, u: &str, n: u32| s.state().task(u, n).unwrap().inputs.iter().map(|i| i.from.clone()).collect::<Vec<_>>();
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
