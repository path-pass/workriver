// The attachment lifecycle's rules: who holds a file (as an attachment or by its id in a
// text), when it is due under a rule, and the service's removal and restore.
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};

use crate::records::tests::*;
use crate::*;

const DAY: Duration = Duration::from_secs(24 * 3600);

/// `days` from now.
fn after(days: u64) -> SystemTime {
    SystemTime::now() + DAY * days as u32
}

fn rule(days: u32) -> Conditions {
    Conditions { days, only: Only::All, min_mb: 0 }
}

/// Stores `id` (an image of `size` bytes) as `who`, linked to nothing.
fn stored(s: &mut Records, who: &str, id: &str, size: u64) {
    add(s, who, json!({"kind": "attachment.added", "body": {"id": id, "name": format!("{id}.png"), "type": "image/png", "size": size}})).unwrap();
}

fn due(s: &Records, r: &Conditions, at: SystemTime) -> Vec<String> {
    s.state().attachments_due(r, at).iter().map(|a| a.id.clone()).collect()
}

fn close(s: &mut Records, intent: &str) {
    add(s, "user", json!({"kind": "intent.state_changed", "body": {"intent": intent, "state": "done"}})).unwrap();
}

fn removed(ids: &[&str], how: &str, to: Option<Value>) -> Value {
    let mut b = json!({"ids": ids, "how": how, "rule": {"days": 1, "only": "all", "minMb": 0}});
    if let Some(to) = to {
        b["to"] = to;
    }
    json!({"kind": "attachment.removed", "body": b})
}

const A: &str = "00000000000000a1";
const B: &str = "00000000000000b1";

#[test]
fn an_intent_is_closed_when_done_or_dropped_and_not_before() {
    let mut s = Records::default();
    seeded(&mut s);
    assert_eq!(s.state().intent("i1").unwrap().closed, None);
    close(&mut s, "i1");
    let at = s.state().intent("i1").unwrap().closed.clone().expect("closed when done");
    assert_eq!(at, s.events().last().unwrap().at);
    // Dropped from the roadmap: closed too; a planned one is not.
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Later", "state": "planned"}})).unwrap();
    assert_eq!(s.state().intent("i2").unwrap().closed, None);
    add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i2", "state": "dropped", "reason": "not needed"}})).unwrap();
    assert_eq!(s.state().intent("i2").unwrap().closed.as_ref(), Some(&s.events().last().unwrap().at));
}

#[test]
fn a_file_of_an_active_intent_is_never_due_and_one_of_a_closed_one_after_its_days() {
    let mut s = Records::default();
    seeded(&mut s);
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": A, "name": "a.png", "type": "image/png", "size": 3, "unit": "u1", "task": 1}})).unwrap();
    assert!(due(&s, &rule(1), after(4000)).is_empty(), "an active intent holds it however old");
    close(&mut s, "i1");
    assert!(due(&s, &rule(30), after(29)).is_empty());
    assert_eq!(due(&s, &rule(30), after(31)), [A]);
}

#[test]
fn a_file_of_two_intents_follows_the_later_close() {
    let mut s = Records::default();
    seeded(&mut s);
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Other"}})).unwrap();
    add(&mut s, "user", json!({"kind": "unit.created", "body": {"intent": "i2", "title": "More", "repo": "/r", "branch": "feat/more", "base": "main"}})).unwrap();
    add(&mut s, "user", json!({"kind": "task.created", "body": {"unit": "u2", "kind": "implement", "title": "x"}})).unwrap();
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": A, "name": "a.png", "type": "image/png", "size": 3, "unit": "u1", "task": 1}})).unwrap();
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": A, "name": "a.png", "type": "image/png", "size": 3, "unit": "u2", "task": 1}})).unwrap();
    close(&mut s, "i1");
    assert!(due(&s, &rule(1), after(100)).is_empty(), "i2 is still active");
    close(&mut s, "i2");
    assert_eq!(due(&s, &rule(1), after(2)), [A]);
}

#[test]
fn a_file_named_by_nothing_is_free_by_its_own_time_and_the_filters_apply() {
    let mut s = Records::default();
    seeded(&mut s);
    stored(&mut s, "agent-a", A, 3);
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": B, "name": "b.txt", "type": "text/plain", "size": 3 * 1024 * 1024}})).unwrap();
    assert!(due(&s, &rule(2), after(1)).is_empty());
    assert_eq!(due(&s, &rule(2), after(3)), [A, B]);
    assert_eq!(due(&s, &Conditions { only: Only::Pictures, ..rule(2) }, after(3)), [A]);
    assert_eq!(due(&s, &Conditions { min_mb: 2, ..rule(2) }, after(3)), [B]);
    assert!(due(&s, &Conditions { min_mb: 3, ..rule(2) }, after(3)).is_empty(), "over the size, not at it");
}

#[test]
fn only_the_service_removes_and_only_what_is_stored_free_and_not_gone() {
    let mut s = Records::default();
    seeded(&mut s);
    stored(&mut s, "agent-a", A, 3);
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": B, "name": "b.png", "type": "image/png", "size": 3, "unit": "u1", "task": 1}})).unwrap();
    for who in ["user", "agent-a"] {
        assert!(add(&mut s, who, removed(&[A], "deleted", None)).unwrap_err().contains("only workriver records this"), "{who}");
    }
    assert!(add(&mut s, "workriver", removed(&["00000000000000c1"], "deleted", None)).unwrap_err().contains("no stored attachment 00000000000000c1"));
    assert!(add(&mut s, "workriver", removed(&[B], "deleted", None)).unwrap_err().contains("is still held"), "an active intent's file");
    assert!(add(&mut s, "workriver", removed(&[], "deleted", None)).is_err(), "no ids");
    assert!(add(&mut s, "workriver", removed(&[A, A], "deleted", None)).is_err(), "twice");
    assert!(add(&mut s, "workriver", json!({"kind": "attachment.removed", "body": {"ids": [A], "how": "deleted", "rule": {"days": 0, "only": "all", "minMb": 0}}})).is_err(), "a rule of no days");
    assert!(add(&mut s, "workriver", removed(&[A], "moved", None)).unwrap_err().contains("where"), "a move says where");
    assert!(add(&mut s, "workriver", removed(&[A], "moved", Some(json!({"kind": " ", "path": "/x"})))).is_err(), "a target has a kind");
    assert!(add(&mut s, "workriver", removed(&[A], "deleted", Some(json!({"kind": "folder", "path": "/x"})))).is_err(), "a delete goes nowhere");
    add(&mut s, "workriver", removed(&[A], "deleted", None)).unwrap();
    let gone = s.state().attachments.iter().find(|a| a.id == A).unwrap().gone.clone().unwrap();
    assert_eq!((gone.how, gone.to, gone.at), (Removed::Deleted, None, s.events().last().unwrap().at.clone()));
    assert!(add(&mut s, "workriver", removed(&[A], "deleted", None)).unwrap_err().contains("already"), "gone already");
    assert!(due(&s, &rule(1), after(10)).is_empty(), "a gone file is not due");
}

#[test]
fn a_move_keeps_its_target_and_only_the_services_restore_brings_a_file_back() {
    let mut s = Records::default();
    seeded(&mut s);
    stored(&mut s, "agent-a", A, 3);
    let to = json!({"kind": "folder", "path": "/Users/me/OneDrive/wr"});
    add(&mut s, "workriver", removed(&[A], "moved", Some(to.clone()))).unwrap();
    let gone = |s: &Records| s.state().attachments.iter().find(|a| a.id == A).unwrap().gone.clone();
    assert_eq!(serde_json::to_value(gone(&s).unwrap().to).unwrap(), to, "the target as given, kind and all");
    // An agent's (or the user's) attachment.added only links it: still gone, its target kept.
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": A, "name": "a.png", "type": "image/png", "size": 3, "unit": "u1", "task": 1}})).unwrap();
    add(&mut s, "user", json!({"kind": "attachment.added", "body": {"id": A, "name": "a.png", "type": "image/png", "size": 3}})).unwrap();
    assert_eq!(serde_json::to_value(gone(&s).unwrap().to).unwrap(), to);
    assert_eq!(s.state().task("u1", 1).unwrap().attachments, [A]);
    let restore = |id: &str| json!({"kind": "attachment.restored", "body": {"id": id}});
    for who in ["user", "agent-a"] {
        assert!(add(&mut s, who, restore(A)).unwrap_err().contains("only workriver records this"), "{who}");
    }
    assert!(add(&mut s, "workriver", restore("00000000000000c1")).is_err(), "unknown");
    stored(&mut s, "agent-a", B, 3);
    assert!(add(&mut s, "workriver", restore(B)).unwrap_err().contains("is not gone"));
    add(&mut s, "workriver", restore(A)).unwrap();
    let a = s.state().attachments.iter().find(|a| a.id == A).unwrap().clone();
    assert_eq!((a.gone, a.at), (None, s.events().last().unwrap().at.clone()), "stored again, its clock starts again");
    assert_eq!(replayed(&s), *s.state(), "the same from the stored events");
}

#[test]
fn a_recorded_size_is_at_most_the_stores_limit() {
    let mut s = Records::default();
    let over = MAX_ATTACHMENT as u64 + 1;
    let e = add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": A, "name": "a.png", "type": "image/png", "size": over}})).unwrap_err();
    assert!(e.contains("larger than"), "{e}");
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": A, "name": "a.png", "type": "image/png", "size": MAX_ATTACHMENT as u64}})).unwrap();
}

#[test]
fn a_message_holds_its_file_by_its_intent_or_its_workspace() {
    let mut s = Records::default();
    seeded(&mut s);
    managed(&mut s, "boss", "i1");
    stored(&mut s, "user", A, 3);
    stored(&mut s, "user", B, 3);
    let file = |id: &str| json!({"kind": "attachment", "label": "f.png", "id": id});
    add(&mut s, "user", json!({"kind": "message.sent", "body": {"intent": "i1", "text": "Look", "context": [file(A)]}})).unwrap();
    add(&mut s, "user", json!({"kind": "message.sent", "body": {"workspace": "ws1", "text": "And this", "context": [file(B)]}})).unwrap();
    assert!(due(&s, &rule(1), after(100)).is_empty(), "i1 runs in ws1");
    assert!(add(&mut s, "workriver", removed(&[A], "deleted", None)).unwrap_err().contains("is still held"));
    close(&mut s, "i1");
    assert!(due(&s, &rule(30), after(29)).is_empty());
    assert_eq!(due(&s, &rule(30), after(31)), [A, B], "ws1 has no active intent now");
}

#[test]
fn a_question_holds_its_file_by_its_units_intent() {
    let mut s = Records::default();
    seeded(&mut s);
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": A, "name": "a.png", "type": "image/png", "size": 3, "unit": "u1", "task": 1}})).unwrap();
    add(&mut s, "user", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": "What is it?", "context": [{"kind": "attachment", "label": "a.png", "id": A}]}})).unwrap();
    add(&mut s, "agent-a", json!({"kind": "question.answered", "body": {"question": "q1", "text": "A shot."}})).unwrap();
    assert!(due(&s, &rule(1), after(100)).is_empty());
    let held: Vec<_> = s.state().attachment_holders().into_iter().filter(|h| h.id == A).map(|h| (h.how, h.by)).collect();
    assert!(held.contains(&(Named::Attached, Holder::Intent("i1".into()))), "{held:?}");
}

/// A file linked to nothing, named in a text by its path, the fetch command another machine
/// gets or the page's link, is held by that text's record; a longer hash that contains an
/// id holds nothing.
#[test]
fn an_id_in_a_text_holds_the_file() {
    let shapes = [format!("/Users/me/workriver-data/attachments/{A}.png"), format!("`workriver attach fetch {A}.png`"), format!("/api/attachments/{A}")];
    let events = |text: &str| -> Vec<Value> {
        vec![
            json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Fix", "text": text, "inputs": [{"from": "2"}]}}),
            json!({"kind": "finding.noted", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "text": text}}),
            json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": text}}),
            json!({"kind": "intent.updated", "body": {"intent": "i1", "note": text}}),
        ]
    };
    for shape in &shapes {
        for (k, e) in events(shape).into_iter().enumerate() {
            let mut s = Records::default();
            seeded(&mut s);
            stored(&mut s, "agent-a", A, 3);
            add(&mut s, "user", e).unwrap();
            let held: Vec<_> = s.state().attachment_holders().into_iter().filter(|h| h.id == A).map(|h| (h.how, h.by)).collect();
            assert_eq!(held, [(Named::InText, Holder::Intent("i1".into()))], "{shape} in text {k}");
            assert!(due(&s, &rule(1), after(100)).is_empty(), "{shape} in text {k}");
        }
    }
    let mut s = Records::default();
    seeded(&mut s);
    stored(&mut s, "agent-a", A, 3);
    add(&mut s, "user", json!({"kind": "finding.noted", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "text": format!("commit 0123{A}4567abcdef0123456789ab")}})).unwrap();
    assert!(s.state().attachment_holders().is_empty(), "part of a longer run");
    assert_eq!(due(&s, &rule(1), after(2)), [A]);
}

#[test]
fn records_of_no_intent_hold_until_they_are_closed() {
    let mut s = Records::default();
    seeded(&mut s);
    stored(&mut s, "user", A, 3);
    let held = |s: &Records| s.state().attachment_holders().into_iter().filter(|h| h.id == A).map(|h| h.by).collect::<Vec<_>>();
    // A chore of no intent: until it is done.
    add(&mut s, "agent-a", json!({"kind": "chore.created", "body": {"title": format!("Look at {A}.png")}})).unwrap();
    assert_eq!(held(&s), [Holder::Record { open: true }]);
    assert!(due(&s, &rule(1), after(10)).is_empty());
    let chore = s.state().chores[0].id.clone();
    add(&mut s, "user", json!({"kind": "chore.done", "body": {"chore": chore}})).unwrap();
    assert_eq!(held(&s), [Holder::Record { open: false }]);
    assert_eq!(due(&s, &rule(1), after(2)), [A], "by its own time once closed");
    // A question of no unit: until it is answered (and its discussion ended).
    let mut s = Records::default();
    seeded(&mut s);
    stored(&mut s, "user", A, 3);
    // (Today's rules give every question a unit; a record without one is still read.)
    let mut s = recorded_before(s, "user", json!({"kind": "question.asked", "body": {"id": "q1", "thread": "q1", "unit": "", "task": 0, "text": "What is in it?", "context": [{"kind": "attachment", "label": "a.png", "id": A}]}}));
    assert_eq!(held(&s), [Holder::Record { open: true }]);
    add(&mut s, "workriver", json!({"kind": "question.failed", "body": {"question": "q1", "reason_code": "timeout", "reason": "no answer"}})).unwrap();
    assert_eq!(held(&s), [Holder::Record { open: false }]);
    // A backlog finding: until it is dropped; by its intent once batched.
    let mut s = Records::default();
    seeded(&mut s);
    stored(&mut s, "user", A, 3);
    project(&mut s, "/r");
    add(&mut s, "user", json!({"kind": "finding.added", "body": {"project": "/r", "finding": {"severity": "P3", "problem": "x", "impact": "y", "where": ["a"], "attachments": [A]}}})).unwrap();
    assert_eq!(held(&s), [Holder::Record { open: true }]);
    let f = s.state().backlog[0].finding.found.id.clone();
    add(&mut s, "user", json!({"kind": "findings.batched", "body": {"intent": "i1", "findings": [{"finding": f}]}})).unwrap();
    assert_eq!(held(&s), [Holder::Intent("i1".into())]);
}

#[test]
fn a_dropped_backlog_finding_frees_its_file() {
    let mut s = Records::default();
    seeded(&mut s);
    stored(&mut s, "user", A, 3);
    project(&mut s, "/r");
    add(&mut s, "user", json!({"kind": "finding.added", "body": {"project": "/r", "finding": {"severity": "P3", "problem": "x", "impact": "y", "where": ["a"], "attachments": [A]}}})).unwrap();
    let f = s.state().backlog[0].finding.found.id.clone();
    add(&mut s, "user", json!({"kind": "finding.dropped", "body": {"finding": f, "reason": "stale"}})).unwrap();
    let held: Vec<_> = s.state().attachment_holders().into_iter().filter(|h| h.id == A).map(|h| h.by).collect();
    assert_eq!(held, [Holder::Record { open: false }]);
}

/// The guard for texts: in a state with one of every record that can hold, an id written into
/// any string of any of them holds the file. The strings are found by walking the serialized
/// state, not listed here, so a field added later is checked by itself.
#[test]
fn every_text_of_every_holding_record_holds_an_id_written_in_it() {
    let s = every_holder();
    let base = serde_json::to_value(s.state()).unwrap();
    let mut pointers = vec![];
    for key in ["intents", "units", "chores", "questions", "threads", "messages", "backlog"] {
        assert!(base[key].as_array().is_some_and(|a| !a.is_empty()), "the state has no {key}");
        strings(&base[key], &format!("/{key}"), &mut pointers);
    }
    let text = format!("see /api/attachments/{A}");
    let mut checked = 0;
    for p in &pointers {
        let mut v = base.clone();
        *v.pointer_mut(p).unwrap() = json!(text);
        // A string that is not free text (an enum's word, a time) does not read back as one.
        let Ok(state) = serde_json::from_value::<State>(v) else { continue };
        checked += 1;
        assert!(state.attachment_holders().iter().any(|h| h.id == A && h.how == Named::InText), "an id in {p} holds nothing");
    }
    assert!(checked > 40, "only {checked} strings checked");
}

/// Every string's JSON pointer under `v`.
fn strings(v: &Value, at: &str, out: &mut Vec<String>) {
    match v {
        Value::String(_) => out.push(at.to_string()),
        Value::Array(a) => a.iter().enumerate().for_each(|(k, x)| strings(x, &format!("{at}/{k}"), out)),
        Value::Object(o) => o.iter().for_each(|(k, x)| strings(x, &format!("{at}/{}", k.replace('~', "~0").replace('/', "~1")), out)),
        _ => {}
    }
}

/// One of every record that can hold a file, with its texts filled in.
fn every_holder() -> Records {
    let mut s = Records::default();
    seeded(&mut s);
    managed(&mut s, "boss", "i1");
    stored(&mut s, "user", A, 3);
    stored(&mut s, "user", B, 3);
    let steps = [
        ("user", json!({"kind": "intent.updated", "body": {"intent": "i1", "goal": "A goal", "note": "A note"}})),
        ("agent-a", json!({"kind": "finding.noted", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "text": "A detail"}})),
        ("user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-2", "decision": "reject", "note": "No"}})),
        ("boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "design", "title": "Design", "text": "Draw it", "inputs": [{"from": "2"}]}})),
        ("user", json!({"kind": "task.noted", "body": {"unit": "u1", "task": 3, "text": "More", "attachments": [B]}})),
        ("boss", json!({"kind": "task.started", "body": {"unit": "u1", "task": 3}})),
        ("boss", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 3, "note": "Done"}})),
        ("user", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 3, "text": "Why?", "discuss": true, "context": [{"kind": "attachment", "label": "b", "id": B}]}})),
        ("boss", json!({"kind": "question.answered", "body": {"question": "q1", "text": "Because."}})),
        ("user", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": "How?"}})),
        ("user", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "thread": "q2", "text": "Sum up", "conclude": true}})),
        ("agent-a", json!({"kind": "question.answered", "body": {"question": "q2", "text": "So."}})),
        ("agent-a", json!({"kind": "question.answered", "body": {"question": "q3", "text": "In short", "conclusion": true}})),
        ("user", json!({"kind": "message.sent", "body": {"intent": "i1", "text": "Hello", "context": [{"kind": "attachment", "label": "b", "id": B}]}})),
        ("boss", json!({"kind": "message.replied", "body": {"message": "m1", "text": "Hi"}})),
        ("boss", json!({"kind": "chore.created", "body": {"intent": "i1", "title": "Rotate", "steps": [{"text": "Run it", "command": "make key"}]}})),
        ("user", json!({"kind": "finding.added", "body": {"project": "/r", "source": "a review", "finding": {"severity": "P3", "problem": "x", "impact": "y", "where": ["a"]}}})),
    ];
    for (who, e) in steps {
        add(&mut s, who, e.clone()).unwrap_or_else(|err| panic!("{e}: {err}"));
    }
    s
}

/// Stored long ago and stored or named again now (an `attachment.added` for the same
/// content): its clock starts again from the later one.
#[test]
fn a_file_added_again_waits_its_days_from_the_last_add() {
    let mut s = Records::default();
    seeded(&mut s);
    let mut s = recorded_before(s, "agent-a", json!({"kind": "attachment.added", "body": {"id": A, "name": "a.png", "type": "image/png", "size": 3}}));
    assert_eq!(due(&s, &rule(30), after(0)), [A], "free since long ago");
    stored(&mut s, "agent-b", A, 3);
    assert_eq!(s.state().attachments[0].at, "2026-01-01T00:00:00Z", "the first store's time stays as it was");
    assert!(due(&s, &rule(30), after(2)).is_empty(), "added again now");
    assert_eq!(due(&s, &rule(30), after(31)), [A]);
}
