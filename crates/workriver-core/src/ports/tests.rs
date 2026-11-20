// The port's own promises, on a store that is not there: what the provided reads make of
// a storage that cannot be read. (What a real storage keeps is workriver-data's to test.)
use super::*;
use crate::{Body, Principal, RuntimeRef};

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
    said("intent_in", s.intent_in(&RuntimeRef { id: "w1".into(), label: "chat".into(), machine: None }).err());
    said("manages", s.manages("a").err());
    said("units", s.units().err());
    said("delivered", s.delivered().err());
    said("open_questions", s.open_questions().err());
    said("held_messages", s.held_messages().err());
    said("thread_agents", s.thread_agents().err());
    said("intent_events", s.intent_events("i1").err());
}
