// Core's data port, as this storage keeps it: what `WorkStore` promises whoever holds one,
// asked through the trait only. (What is SQLite's own is in sqlite/tests.rs.)
use serde_json::{Value, json};
use workriver_core::ports::{DataError, Revision, WorkStore};
use workriver_core::*;

use super::Store;

fn body(v: Value) -> Body {
    serde_json::from_value(v).unwrap()
}

fn agent_a() -> Principal {
    Principal::Agent { session: "agent-a".into(), machine: None }
}

/// A store in `dir`, held as the service holds it.
fn open(dir: &std::path::Path) -> Box<dyn WorkStore> {
    Box::new(Store::open(&dir.join("w.db")).unwrap())
}

/// An intent, a unit of it and a task: three events.
fn seed(s: &mut dyn WorkStore) {
    for v in [
        json!({"kind": "intent.created", "body": {"title": "Chat v2"}}),
        json!({"kind": "unit.created", "body": {"intent": "i1", "title": "Mentions", "repo": "/r", "branch": "feat/mention", "base": "main"}}),
        json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Find mentions"}}),
    ] {
        // An intent made active is the user's to add (an agent's starts in the roadmap), and
        // so is a unit of an intent with no manager.
        let who = if v["kind"] == "task.created" { agent_a() } else { Principal::User };
        s.record(&who, body(v)).unwrap();
    }
}

/// A record gives back the event as it was committed (the id the rules gave, who it was
/// recorded as) and the revision it made; what is read afterwards is at that revision.
#[test]
fn a_record_returns_the_committed_change_and_its_revision() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    assert_eq!(s.recorded().unwrap().revision(), Revision(0));
    let done = s.record(&agent_a(), body(json!({"kind": "intent.created", "body": {"title": "Chat v2", "state": "planned"}}))).unwrap();
    assert_eq!(done.revision, Revision(1));
    assert_eq!((done.event.seq, done.event.actor.as_str()), (1, "agent-a"));
    assert!(matches!(&done.event.body, Body::IntentCreated { id, .. } if id == "i1"), "the rules gave the id: {:?}", done.event.body);
    let now = s.recorded().unwrap();
    assert_eq!(now.revision(), done.revision);
    assert_eq!(now.intents.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["i1"]);
    let user = s.record(&Principal::User, body(json!({"kind": "intent.created", "body": {"title": "Search"}}))).unwrap();
    assert_eq!((user.revision, user.event.actor.as_str()), (Revision(2), "user"));
}

/// What was read is the reader's: a later record does not reach it.
#[test]
fn what_was_read_stays_at_its_revision() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    seed(&mut *s);
    let before = s.recorded().unwrap();
    let kept = before.clone().into_state();
    s.record(&agent_a(), body(json!({"kind": "task.started", "body": {"unit": "u1", "task": 1}}))).unwrap();
    assert_eq!((before.revision(), s.recorded().unwrap().revision()), (Revision(3), Revision(4)));
    assert_eq!(*before, kept, "the earlier read did not change");
    assert_ne!(*s.recorded().unwrap(), kept, "the new one has the start");
}

/// A request the rules refuse is the caller's error and changes nothing: not the revision,
/// not what is read, not the history, not the next id.
#[test]
fn a_refused_request_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    seed(&mut *s);
    let before = s.recorded().unwrap();
    let err = s.record(&agent_a(), body(json!({"kind": "task.started", "body": {"unit": "u9", "task": 1}}))).unwrap_err();
    assert!(matches!(err, DataError::Rejected(_)), "{err:?}");
    // What only the user may record is refused for an agent here too, whoever asks the store.
    let err = s.record(&agent_a(), body(json!({"kind": "project.added", "body": {"path": "/r", "title": "R"}}))).unwrap_err();
    assert!(matches!(&err, DataError::Rejected(r) if r.0.starts_with("only the user does this")), "{err:?}");
    assert_eq!(s.recorded().unwrap(), before);
    assert_eq!(s.history(Revision(0), 100).unwrap().events.len(), 3);
    assert_eq!(s.status().unwrap().events, 3);
    let next = s.record(&agent_a(), body(json!({"kind": "intent.created", "body": {"title": "Search", "state": "planned"}}))).unwrap();
    assert!(matches!(&next.event.body, Body::IntentCreated { id, .. } if id == "i2"), "{:?}", next.event.body);
    assert_eq!(next.revision, Revision(4));
}

/// The history is read on from where the last read ended, oldest first, each event as it
/// was committed; a store opened again gives the same history and the same work.
#[test]
fn history_is_read_by_cursor_and_survives_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    let first = s.record(&agent_a(), body(json!({"kind": "intent.created", "body": {"title": "Chat v2", "state": "planned"}}))).unwrap().event;
    let page = s.history(Revision(0), 1).unwrap();
    assert_eq!((page.events.as_slice(), page.next), ([first].as_slice(), Revision(1)));
    seed(&mut *s);
    let rest = s.history(page.next, 2).unwrap();
    assert_eq!((rest.events.iter().map(|e| e.seq).collect::<Vec<_>>(), rest.next), (vec![2, 3], Revision(3)));
    let end = s.history(Revision(4), 10).unwrap();
    assert_eq!((end.events.len(), end.next), (0, Revision(4)), "nothing after the last: the cursor stays");
    let all = s.history(Revision(0), usize::MAX).unwrap();
    assert_eq!(all.events.len(), 4);
    let work = s.recorded().unwrap();
    drop(s);
    let again = open(dir.path());
    assert_eq!(again.history(Revision(0), usize::MAX).unwrap(), all);
    assert_eq!(again.recorded().unwrap(), work, "the same work at the same revision");
}

/// One record by its id is the reader's own copy; one that is not there is refused in the
/// rules' words.
#[test]
fn one_record_is_read_by_its_id() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    seed(&mut *s);
    s.record(&Principal::Service, body(json!({"kind": "agent.seen", "body": {"session": "agent-a", "name": "ann", "harness": "claude"}}))).unwrap();
    assert_eq!(s.unit("u1").unwrap().branch, "feat/mention");
    assert_eq!(s.task("u1", 1).unwrap().title, "Find mentions");
    assert_eq!(s.agent_name("agent-a").unwrap().as_deref(), Some("ann"));
    assert_eq!(s.agent_name("agent-b").unwrap(), None);
    let all = s.recorded().unwrap();
    // Not there: refused in the rules' words, the caller's error.
    let refused = |e: DataError| match e {
        DataError::Rejected(r) => r.0,
        e => panic!("{e:?}"),
    };
    assert_eq!(refused(s.unit("u9").unwrap_err()), all.unit("u9").unwrap_err().0);
    assert_eq!(refused(s.task("u1", 7).unwrap_err()), all.task("u1", 7).unwrap_err().0);
    assert_eq!(refused(s.question("q1").unwrap_err()), all.question("q1").unwrap_err().0);
    // The others say there is none.
    assert_eq!(s.agent("agent-a").unwrap().map(|a| a.harness).as_deref(), Some("claude"));
    assert_eq!((s.agent("agent-b").unwrap(), s.thread("q1").unwrap(), s.run("r1").unwrap(), s.project("p1").unwrap()), (None, None, None, None));
    assert_eq!(s.units().unwrap().iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["u1"]);
    assert!(!s.manages("agent-a").unwrap());
}

/// What the storage says of itself: where it is, how many events, how far it has got.
#[test]
fn the_storage_reports_its_status() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    let status = s.status().unwrap();
    assert_eq!((status.location.as_path(), status.events, status.revision), (dir.path().join("w.db").as_path(), 0, Revision(0)));
    seed(&mut *s);
    let status = s.status().unwrap();
    assert_eq!((status.events, status.revision), (3, Revision(3)));
}
