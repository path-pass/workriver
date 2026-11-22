// The port's own promises, on a store that is not there: what the provided reads make of
// a storage that cannot be read. (What a real storage keeps is workriver-data's to test.)
use super::*;
use crate::{Body, Principal};

/// A store whose storage cannot be read.
struct Down;

fn gone<T>() -> Result<T, DataError> {
    Err(DataError::Unavailable("the disk is gone".into()))
}

impl WorkStore for Down {
    fn recorded(&self) -> Result<Recorded, DataError> {
        gone()
    }
    fn record(&mut self, _: &Principal, _: Body) -> Result<Committed, DataError> {
        gone()
    }
    fn history(&self, _: Revision, _: usize) -> Result<History, DataError> {
        gone()
    }
    fn status(&self) -> Result<DataStatus, DataError> {
        gone()
    }
    fn intent_events(&self, _: &str) -> Result<Vec<crate::Event>, DataError> {
        gone()
    }
}

/// Every read of a store that cannot be read says so, in the storage's words: never "no
/// such unit" (the caller's error), never an empty answer.
#[test]
fn every_read_of_an_unavailable_store_says_so() {
    let s: Box<dyn WorkStore> = Box::new(Down);
    let said = |what: &str, e: Option<DataError>| match e {
        Some(DataError::Unavailable(why)) => assert_eq!(why, "the disk is gone", "{what}"),
        other => panic!("{what}: {other:?}"),
    };
    said("recorded", s.recorded().err());
    said("unit", s.unit("u1").err());
    said("task", s.task("u1", 1).err());
    said("question", s.question("q1").err());
    said("thread", s.thread("q1").err());
    said("agent", s.agent("a").err());
    said("agent_name", s.agent_name("a").err());
    said("run", s.run("r1").err());
    said("project", s.project("p1").err());
    said("workspace", s.workspace("ws1").err());
    said("seat_of", s.seat_of("a").err());
    said("manages", s.manages("a").err());
    said("units", s.units().err());
    said("delivered", s.delivered().err());
    said("open_questions", s.open_questions().err());
    said("open_discussions", s.open_discussions().err());
    said("held_deliveries", s.held_deliveries().err());
    said("held_messages", s.held_messages().err());
    said("thread_agents", s.thread_agents().err());
    said("intent_events", s.intent_events("i1").err());
}

/// A store that holds `Records` in memory: the provided reads over what the rules made.
struct Kept(crate::records::tests::Records);

impl WorkStore for Kept {
    fn recorded(&self) -> Result<Recorded, DataError> {
        Ok(Recorded::new(Revision(self.0.events().len() as u64), std::sync::Arc::new(self.0.state().clone())))
    }
    fn record(&mut self, _: &Principal, _: Body) -> Result<Committed, DataError> {
        unimplemented!()
    }
    fn history(&self, _: Revision, _: usize) -> Result<History, DataError> {
        unimplemented!()
    }
    fn status(&self) -> Result<DataStatus, DataError> {
        unimplemented!()
    }
    fn intent_events(&self, _: &str) -> Result<Vec<crate::Event>, DataError> {
        unimplemented!()
    }
}

/// /p registered, ws1 of it with boss seated, i1 of /p started there.
fn managed_intent() -> crate::records::tests::Records {
    use crate::records::tests::*;
    let mut s = Records::default();
    let ws = workspace(&mut s, "chat", "/p");
    seat(&mut s, &ws, "boss");
    add(&mut s, "user", serde_json::json!({"kind": "intent.created", "body": {"title": "One", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", start("i1", &ws)).unwrap();
    s
}

/// A line of workriver's own in a conversation ("the manager is back") was given to nobody
/// and waits for nobody: it is not a held message, which a start would record as not
/// delivered. The user's message that got no delivery is.
#[test]
fn a_notice_line_is_not_a_held_message() {
    use crate::records::tests::*;
    let mut s = managed_intent();
    add(&mut s, "workriver", serde_json::json!({"kind": "workspace.manager_back", "body": {"workspace": "ws1", "session": "boss"}})).unwrap();
    add(&mut s, "user", serde_json::json!({"kind": "message.sent", "body": {"intent": "i1", "text": "Still there?"}})).unwrap();
    let notice = s.state().messages.iter().find(|m| m.notice.is_some()).unwrap().id.clone();
    let held = Kept(s).held_messages().unwrap();
    assert!(!held.iter().any(|(id, _)| *id == notice), "{held:?}");
    assert_eq!(held.len(), 1, "the user's message: {held:?}");
}

/// What a start watches again (`delivered`) and what it records as lost (`held_deliveries`)
/// walk the same deliveries: a batch's message, recorded on its first finding, is one of
/// them when that finding is in the backlog.
#[test]
fn a_backlog_findings_delivery_is_watched_again_like_any_other() {
    use crate::records::tests::*;
    let mut s = managed_intent();
    add(&mut s, "user", serde_json::json!({"kind": "finding.added", "body": {"project": "/p", "intent": "i1", "finding": {"severity": "P3", "problem": "x", "impact": "y", "where": ["a"]}}})).unwrap();
    let f = s.state().backlog[0].finding.found.id.clone();
    let at = |step: &str| serde_json::json!({"kind": "delivery.updated", "body": {"unit": "", "task": 0, "finding": f, "step": step, "to": "boss", "message": "pick these up"}});
    add(&mut s, "workriver", at("delivered")).unwrap();
    let delivered = Kept(s).delivered().unwrap();
    assert_eq!(delivered.iter().map(|d| (d.task, d.finding.clone())).collect::<Vec<_>>(), [(0, Some(f.clone()))], "{delivered:?}");
}
