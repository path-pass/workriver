use serde_json::json;

use crate::*;

pub(crate) fn body(v: serde_json::Value) -> Body {
    serde_json::from_value(v).unwrap()
}

/// What the store does with the rules, without its SQL (that is workriver-data's to test): each
/// event prepared against the state, recorded in order and applied; and the state rebuilt from
/// the records alone, each read back from its stored form and applied again.
#[derive(Default)]
pub(crate) struct Records {
    state: State,
    events: Vec<Event>,
}

impl Records {
    pub(crate) fn append(&mut self, who: &Principal, body: Body) -> Result<Event, Rejected> {
        let body = self.state.prepare(who, body)?;
        let e = Event { seq: self.events.len() as i64 + 1, at: now(), actor: who.actor().to_string(), body };
        self.state.apply(&e);
        self.events.push(e.clone());
        Ok(e)
    }

    pub(crate) fn state(&self) -> &State {
        &self.state
    }
}

/// An intent, a unit, an implement task delivered with evidence and a review with two findings.
pub(crate) fn seed() -> Vec<Body> {
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
    steps.into_iter().map(body).collect()
}

pub(crate) fn seeded(s: &mut Records) {
    for b in seed() {
        s.append(&"agent-a".into(), b).unwrap();
    }
}

#[test]
fn rules_assign_ids_and_accept_good_input() {
    let mut s = Records::default();
    seeded(&mut s);
    let review = s.state().task("u1", 2).unwrap();
    let ids: Vec<_> = review.findings.iter().map(|f| f.found.id.as_str()).collect();
    assert_eq!(ids, ["P2-1", "P2-2"]);
    assert_eq!(review.reviewer, Some(Reviewer::CodeReview));
    assert_eq!(s.state().task("u1", 1).unwrap().agent.as_deref(), Some("agent-a"));
}

#[test]
fn rules_reject_bad_input() {
    let mut s = Records::default();
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

pub(crate) fn add(s: &mut Records, actor: &str, v: serde_json::Value) -> Result<Event, String> {
    s.append(&actor.into(), body(v)).map_err(|e| e.to_string())
}

/// An event as an old database has it, straight into the records past today's rules (the
/// body as it was stored, ids and numbers filled in); the state is rebuilt from them.
pub(crate) fn recorded_before(mut s: Records, actor: &str, v: serde_json::Value) -> Records {
    s.events.push(Event { seq: s.events.len() as i64 + 1, at: "2026-01-01T00:00:00Z".into(), actor: actor.into(), body: body(v) });
    s.state = replayed(&s);
    s
}

/// The state rebuilt from the records alone: each event read back from its stored form
/// (`kind` and `body`), applied in order.
pub(crate) fn replayed(s: &Records) -> State {
    let mut state = State::default();
    for e in &s.events {
        let stored = serde_json::to_value(&e.body).unwrap();
        state.apply(&Event { body: serde_json::from_value(stored).unwrap(), ..e.clone() });
    }
    state
}

// Replay of a synthetic history to the state it has always added up to.
mod replay;
