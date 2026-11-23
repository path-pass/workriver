use serde_json::json;

use crate::*;

pub(crate) fn body(v: serde_json::Value) -> Body {
    serde_json::from_value(v).unwrap()
}

/// What the store does with the rules, without its SQL (that is workriver-data's to test): each
/// event prepared against the state, recorded in order and applied; and the state rebuilt from
/// the records alone, each read back from its stored form and applied again.
pub(crate) struct Records {
    state: State,
    events: Vec<Event>,
}

/// The templates of no steps the user saved for the tests, one per kind of unit: a unit made
/// with one has no task placed, so the tests number their own tasks from 1.
pub(crate) const BARE: [(&str, &str); 3] = [("bare", "code"), ("bare-research", "research"), ("bare-spike", "spike")];

impl Default for Records {
    fn default() -> Records {
        let mut s = Records { state: State::default(), events: vec![] };
        for (name, unit) in BARE {
            s.append(&Principal::User, body(json!({"kind": "template.set", "body": {"name": name, "description": "Nothing placed: for the tests.", "unit": unit, "steps": []}}))).unwrap();
        }
        s
    }
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

    /// Every event recorded so far, in order.
    pub(crate) fn events(&self) -> &[Event] {
        &self.events
    }
}

/// An intent, a unit, an implement task delivered with evidence and a review with two findings.
pub(crate) fn seed() -> Vec<Body> {
    let steps = [
        json!({"kind": "intent.created", "body": {"title": "Chat v2"}}),
        json!({"kind": "unit.created", "body": {"intent": "i1", "title": "Mentions", "repo": "/r", "branch": "feat/mention", "base": "main", "template": "bare"}}),
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
        // An active intent is made by the user (an agent's starts in the roadmap), and so
        // is the unit of an intent that has no manager.
        let who = if matches!(b, Body::IntentCreated { .. } | Body::UnitCreated { .. }) { "user" } else { "agent-a" };
        s.append(&who.into(), b).unwrap();
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
    rejects(json!({"kind": "unit.created", "body": {"template": "bare", "intent": "i9", "title": "x", "repo": "/r", "branch": "feat/b", "base": "main"}}), "no intent i9");
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

/// Registers `path` as a project unless it is one already; its id.
pub(crate) fn project(s: &mut Records, path: &str) -> String {
    if let Some(p) = s.state().projects.iter().find(|p| p.path == path) {
        return p.id.clone();
    }
    let title = path.trim_start_matches('/').to_uppercase();
    match add(s, "user", json!({"kind": "project.added", "body": {"path": path, "title": title}})).unwrap().body {
        Body::ProjectAdded { id, .. } => id,
        _ => unreachable!(),
    }
}

/// A new workspace called `name` in the project of folder `path` (registered when it is
/// not); its id.
pub(crate) fn workspace(s: &mut Records, name: &str, path: &str) -> String {
    let p = project(s, path);
    match add(s, "user", json!({"kind": "workspace.created", "body": {"name": name, "project": p}})).unwrap().body {
        Body::WorkspaceCreated { id, .. } => id,
        _ => unreachable!(),
    }
}

/// The user seats `session` (none: empties the seat) in workspace `ws`, over the seat as it
/// is, on `machine` in `folder`.
pub(crate) fn seat_at(s: &mut Records, ws: &str, session: Option<&str>, machine: Option<&str>, folder: &str) -> Result<Event, String> {
    let replaces = s.state().workspace(ws).map_err(|e| e.to_string())?.manager.as_ref().map(Seat::shown);
    add(s, "user", json!({"kind": "workspace.manager_set", "body": {"workspace": ws, "manager": session, "machine": machine, "folder": folder, "replaces": replaces}}))
}

/// The user seats `session` in workspace `ws`, on this machine in `/w`.
pub(crate) fn seat(s: &mut Records, ws: &str, session: &str) {
    seat_at(s, ws, Some(session), None, "/w").unwrap();
}

/// The start of intent `i` in workspace `ws`, as an event.
pub(crate) fn start(i: &str, ws: &str) -> serde_json::Value {
    json!({"kind": "intent.state_changed", "body": {"intent": i, "state": "active", "workspace": ws}})
}

/// `agent` becomes the manager of `intent`, as the user's seating and start record it:
/// seated in the first open workspace of the intent's project (made when there is none; an
/// intent of no project: project /r, which it then takes), the intent started there by the
/// user (one active elsewhere is put back in the roadmap first).
pub(crate) fn managed(s: &mut Records, agent: &str, intent: &str) {
    let folder = Some(s.state().intent(intent).unwrap().project.clone()).filter(|p| !p.is_empty()).unwrap_or_else(|| "/r".into());
    let p = project(s, &folder);
    let ws = match s.state().workspaces.iter().find(|w| w.project == p && !w.closed) {
        Some(w) => w.id.clone(),
        None => workspace(s, "chat", &folder),
    };
    if s.state().workspace(&ws).unwrap().manager.as_ref().is_none_or(|m| m.session != agent) {
        seat(s, &ws, agent);
    }
    if s.state().intent(intent).unwrap().state == IntentState::Active {
        add(s, "user", json!({"kind": "intent.state_changed", "body": {"intent": intent, "state": "planned"}})).unwrap();
    }
    add(s, "user", start(intent, &ws)).unwrap();
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
