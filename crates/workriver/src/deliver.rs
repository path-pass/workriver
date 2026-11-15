// Where the user's inputs go once recorded, and what the message says. Pure: the daemon
// sends the message and records how far it got.
use workriver_core::runtime::RuntimeAgent;
use workriver_core::{Body, Decision, Event, GateResult, ReasonCode, State, Task, TaskKind, Who, name};

/// A message to deliver about one input: a decision (`finding` set) or a gate result.
#[derive(Debug, PartialEq)]
pub struct Plan {
    pub unit: String,
    pub task: u32,
    pub finding: Option<String>,
    /// The receiving session, or why there is none.
    pub to: Result<String, (ReasonCode, String)>,
    pub message: String,
}

/// The delivery an event calls for: only the user's decisions and gate results get one.
pub fn plan(state: &State, live: Option<&[RuntimeAgent]>, e: &Event) -> Option<Plan> {
    match &e.body {
        Body::FindingDecided { unit, task, finding, decision, note, by: Who::User } => {
            // Accepted work goes to the manager, who hands it out; anything else to whoever
            // did the work that was looked at.
            let to = target(state, live, unit, *task, *decision != Decision::Accept);
            Some(Plan { unit: unit.clone(), task: *task, finding: Some(finding.clone()), to, message: decision_message(state, unit, *task, finding, *decision, note)? })
        }
        Body::GateSubmitted { unit, task, .. } if e.actor == "user" => {
            let to = target(state, live, unit, *task, true);
            Some(Plan { unit: unit.clone(), task: *task, finding: None, to, message: gate_message(state, unit, *task)? })
        }
        _ => None,
    }
}

/// The agent of the work `task` follows if `worker` and it is live, else the intent's manager.
fn target(state: &State, live: Option<&[RuntimeAgent]>, unit: &str, task: u32, worker: bool) -> Result<String, (ReasonCode, String)> {
    let Some(live) = live else { return Err((ReasonCode::RuntimeDown, "the agent runtime (Herdr) is not connected".into())) };
    let is_live = |s: &str| live.iter().any(|a| a.session == s);
    let u = state.unit(unit).map_err(|e| (ReasonCode::RuntimeError, e.0))?;
    if worker {
        let follows = state.task(unit, task).ok().and_then(|t| followed(state, unit, t)).and_then(|t| t.agent.clone());
        if let Some(a) = follows.filter(|a| is_live(a)) {
            return Ok(a);
        }
    }
    let intent = state.intents.iter().find(|i| i.id == u.intent);
    match intent.and_then(|i| i.manager.clone()) {
        Some(m) if is_live(&m) => Ok(m),
        Some(m) => Err((ReasonCode::ManagerGone, format!("the manager of intent {} ({}) is not running", u.intent, state.name_of(&m).unwrap_or(&m)))),
        None => Err((ReasonCode::NoManager, format!("intent {} has no manager; the manager registers with `workriver manager here`", u.intent))),
    }
}

/// The task `t` follows directly.
fn followed<'a>(state: &'a State, unit: &str, t: &Task) -> Option<&'a Task> {
    let from = t.inputs.iter().find(|i| i.primary)?.from.parse().ok()?;
    state.task(unit, from).ok()
}

/// How a task reads in a message: `u1#2 "title"`, or `u1#4 (code review of u1#2 "title")`.
fn label(state: &State, unit: &str, t: &Task) -> String {
    let of = || followed(state, unit, t).map(|f| format!(" of {}", label(state, unit, f))).unwrap_or_default();
    match t.kind {
        TaskKind::Review => format!("{unit}#{} ({}{})", t.n, t.reviewer.map(|r| name(&r).replace('-', " ")).unwrap_or_default(), of()),
        TaskKind::Tests => format!("{unit}#{} (tests{})", t.n, of()),
        _ => format!("{unit}#{} \"{}\"", t.n, t.title),
    }
}

fn decision_message(state: &State, unit: &str, n: u32, finding: &str, decision: Decision, note: &str) -> Option<String> {
    let u = state.unit(unit).ok()?;
    let t = state.task(unit, n).ok()?;
    let f = t.findings.iter().find(|f| f.found.id == finding)?;
    let verb = match decision {
        Decision::Accept => "accepted",
        Decision::Reject => "rejected",
        Decision::Defer => "deferred",
    };
    let mut m = vec![format!("[workriver] The user {verb} finding {finding} from {} in unit {unit} \"{}\".", label(state, unit, t), u.title)];
    m.push(format!("Problem: {}", sentence(&f.found.problem)));
    if !f.found.impact.is_empty() {
        m.push(format!("If left: {}", sentence(&f.found.impact)));
    }
    if !f.found.r#where.is_empty() {
        m.push(format!("Where: {}.", f.found.r#where.join(", ")));
    }
    if !note.is_empty() {
        m.push(format!("Note from the user: {}", sentence(note)));
    }
    m.push(match decision {
        Decision::Accept => format!(
            "Next: hand out work for it, e.g. `workriver task add {unit} --title \"<what to do>\" --cause {n}:{finding} --agent <agent>`, then give that task to an agent with the worker instructions (`workriver guide worker`)."
        ),
        Decision::Reject => "No work is needed for it.".into(),
        Decision::Defer => "Leave it for now; it will come up again later.".into(),
    });
    Some(paragraph(m))
}

fn gate_message(state: &State, unit: &str, n: u32) -> Option<String> {
    let t = state.task(unit, n).ok()?;
    let problem = t.result == Some(GateResult::Problem);
    let mut m = vec![format!(
        "[workriver] The user finished gate {}: {}.",
        label(state, unit, t),
        if problem { "found a problem" } else { "done" }
    )];
    if !t.note.is_empty() {
        m.push(format!("Note from the user: {}", sentence(&t.note)));
    }
    if problem {
        m.push("Findings:".into());
        for f in &t.findings {
            let mut line = format!("{} ({}): {}.", f.found.id, name(&f.found.severity), f.found.problem.trim_end_matches('.'));
            if !f.found.impact.is_empty() {
                line += &format!(" If left: {}", sentence(&f.found.impact));
            }
            if !f.found.r#where.is_empty() {
                line += &format!(" Where: {}.", f.found.r#where.join(", "));
            }
            m.push(line);
        }
        m.push(format!(
            "The user decides on each finding in workriver; accepted ones reach the manager, who hands out work with `--cause {n}:<id>`. Until then the work behind gate {unit}#{n} waits."
        ));
    } else {
        m.push("You can go on with the work that waited on it.".into());
    }
    Some(paragraph(m))
}

/// Text that ends like a sentence, so the paragraph below reads as one.
fn sentence(s: &str) -> String {
    let s = s.trim();
    if s.ends_with(['.', '!', '?']) { s.to_string() } else { format!("{s}.") }
}

/// The message as one paragraph: a terminal without bracketed paste submits at each newline.
fn paragraph(lines: Vec<String>) -> String {
    lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use workriver_core::runtime::AgentState;
    use workriver_core::{Event, Store};

    use super::*;

    fn agent(session: &str) -> RuntimeAgent {
        RuntimeAgent { session: session.into(), harness: "claude".into(), state: AgentState::Idle, cwd: String::new(), group: None, name: session.into() }
    }

    fn at(store: &mut Store, actor: &str, v: serde_json::Value) -> Event {
        store.append(actor, serde_json::from_value(v).unwrap()).unwrap()
    }

    #[test]
    fn picks_the_receiver_and_writes_the_message() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(&dir.path().join("w.db")).unwrap();
        at(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Chat"}}));
        at(&mut s, "user", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "Mentions", "repo": "/r", "branch": "b", "base": "main"}}));
        at(&mut s, "worker", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Find mentions"}}));
        at(&mut s, "worker", json!({"kind": "task.started", "body": {"unit": "u1", "task": 1}}));
        at(&mut s, "rev", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "inputs": [{"from": "1"}]}}));
        at(&mut s, "rev", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 2, "findings": [
            {"severity": "P2", "problem": "Twice", "impact": "Two notices", "where": ["a.ts"]},
            {"severity": "P3", "problem": "Emoji", "impact": "Missed", "where": ["a.ts"]}]}}));
        at(&mut s, "worker", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "test", "title": "Try it", "inputs": [{"from": "1"}]}}));

        let live = [agent("worker"), agent("boss")];
        let accept = at(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "accept", "by": "user"}}));
        // No manager yet: the accepted finding has nowhere to go, and says why.
        let p = plan(s.state(), Some(&live), &accept).unwrap();
        assert_eq!(p.to.unwrap_err().0, ReasonCode::NoManager);
        assert!(!p.message.contains('\n'), "one paragraph: {}", p.message);
        assert!(p.message.contains("--cause 2:P2-1"), "{}", p.message);
        assert!(p.message.contains("u1#2 (code review of u1#1 \"Find mentions\")"), "{}", p.message);
        assert_eq!(plan(s.state(), None, &accept).unwrap().to.unwrap_err().0, ReasonCode::RuntimeDown);

        at(&mut s, "boss", json!({"kind": "manager.registered", "body": {"intent": "i1", "agent": "boss"}}));
        assert_eq!(plan(s.state(), Some(&live), &accept).unwrap().to, Ok("boss".into()));
        // A rejection goes to whoever did the reviewed work while it is live, else the manager.
        let reject = at(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P3-1", "decision": "reject", "by": "user"}}));
        assert_eq!(plan(s.state(), Some(&live), &reject).unwrap().to, Ok("worker".into()));
        assert_eq!(plan(s.state(), Some(&[agent("boss")]), &reject).unwrap().to, Ok("boss".into()));
        // A gate result goes to the agent of the work the gate follows.
        let gate = at(&mut s, "user", json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 3, "result": "problem", "note": "Mostly fine",
            "findings": [{"severity": "P1", "problem": "Crashes"}]}}));
        let p = plan(s.state(), Some(&live), &gate).unwrap();
        assert_eq!(p.to, Ok("worker".into()));
        assert!(p.message.contains("found a problem") && p.message.contains("P1-1 (P1): Crashes."), "{}", p.message);
        // An agent's own decision is not delivered.
        let own = at(&mut s, "rev", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 3, "finding": "P1-1", "decision": "defer", "by": "agent"}}));
        assert_eq!(plan(s.state(), Some(&live), &own), None);
    }
}
