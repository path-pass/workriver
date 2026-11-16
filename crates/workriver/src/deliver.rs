// Where the user's inputs go once recorded, and what the message says. Pure: the daemon
// sends the message and records how far it got.
use workriver_core::runtime::RuntimeAgent;
use workriver_core::{Body, Decision, Event, GateKind, GateResult, ReasonCode, State, Task, TaskKind, Who, name};

/// A message to deliver: about a user's decision (`finding` set) or gate result, about a
/// delivered task or a gate asked for (to the manager), or about a chore (`chore` set).
#[derive(Debug, PartialEq)]
pub struct Plan {
    pub unit: String,
    pub task: u32,
    pub finding: Option<String>,
    pub chore: Option<String>,
    /// The receiving session, or why there is none.
    pub to: Result<String, (ReasonCode, String)>,
    pub message: String,
}

/// The delivery an event calls for, if any.
/// `home`: the data directory, where stored attachments are listed from.
pub fn plan(state: &State, live: Option<&[RuntimeAgent]>, home: &std::path::Path, e: &Event) -> Option<Plan> {
    let files = |unit: &str, n: u32, finding: Option<&str>| files(state, home, unit, n, finding);
    let on = |unit: &str, task: u32, finding: Option<String>, to, message| Plan { unit: unit.into(), task, finding, chore: None, to, message };
    let intent_of = |unit: &str| state.intent_of(unit).map(str::to_string);
    // Whether the actor is someone other than the intent's manager (a worker, or the user).
    let not_manager = |intent: &str| state.manager_of(intent) != Some(e.actor.as_str());
    match &e.body {
        // Every decision goes to the manager, who decides what the workers hear of it.
        Body::FindingDecided { unit, task, finding, decision, note, by: Who::User } => {
            let to = target(state, live, &intent_of(unit)?, None);
            Some(on(unit, *task, Some(finding.clone()), to, decision_message(state, unit, *task, finding, *decision, note)? + &files(unit, *task, Some(finding))))
        }
        // A gate result goes to the agent waiting behind the gate, else the manager; a design
        // decision always to the manager, who hands out what follows.
        Body::GateSubmitted { unit, task, .. } if e.actor == "user" => {
            let gate = state.task(unit, *task).ok();
            let decide = gate.is_some_and(|g| g.gate == Some(GateKind::Decide));
            let waiting = gate.filter(|_| !decide).and_then(|t| followed(state, unit, t)).and_then(|t| t.agent.clone());
            let to = target(state, live, &intent_of(unit)?, waiting.as_deref());
            Some(on(unit, *task, None, to, gate_message(state, unit, *task)? + &files(unit, *task, None)))
        }
        Body::TaskDelivered { unit, task, .. } if not_manager(&intent_of(unit)?) => {
            let to = target(state, live, &intent_of(unit)?, None);
            Some(on(unit, *task, None, to, delivered_message(state, &e.actor, unit, *task)? + &files(unit, *task, None)))
        }
        Body::TaskCreated(t) if t.kind == TaskKind::Gate && e.actor != "user" && not_manager(&intent_of(&t.unit)?) => {
            let to = target(state, live, &intent_of(&t.unit)?, None);
            let gate = state.task(&t.unit, t.n).ok()?;
            let message = format!(
                "[workriver] {} asked the user for gate {} ({}); the work after it waits until the user submits it. Check with `workriver board {}`.",
                who(state, &e.actor),
                label(state, &t.unit, gate),
                t.gate.map(|g| name(&g)).unwrap_or_default(),
                t.unit
            );
            Some(on(&t.unit, t.n, None, to, message))
        }
        // Deferred findings the user picked to do together: one message to the manager.
        Body::FindingsBatched { intent, findings } => {
            let first = findings.first()?;
            let to = target(state, live, intent, None);
            Some(Plan { unit: first.unit.clone(), task: first.task, finding: Some(first.finding.clone()), chore: None, to, message: batch_message(state, intent, findings)? })
        }
        Body::ChoreCreated { id, intent, title, .. } if e.actor != "user" && not_manager(intent) => Some(Plan {
            unit: String::new(),
            task: 0,
            finding: None,
            chore: Some(id.clone()),
            to: target(state, live, intent, None),
            message: format!("[workriver] {} asked the user for chore {id} \"{title}\"; it blocks nobody.", who(state, &e.actor)),
        }),
        _ => None,
    }
}

fn batch_message(state: &State, intent: &str, refs: &[workriver_core::FindingRef]) -> Option<String> {
    let i = state.intents.iter().find(|i| i.id == intent)?;
    let mut m = vec![format!("[workriver] The user picked {} deferred findings of intent {intent} \"{}\" to do together:", refs.len(), i.title)];
    let mut causes = vec![];
    for r in refs {
        let f = state.finding(r).ok()?;
        let at = if r.task == 0 {
            let src = state.backlog.iter().find(|b| b.finding.found.id == r.finding).map(|b| b.source.clone()).unwrap_or_default();
            causes.push(format!("--cause {}", r.finding));
            if src.is_empty() { format!("{} (backlog)", r.finding) } else { format!("{} (backlog, from {src})", r.finding) }
        } else {
            causes.push(format!("--cause {}#{}:{}", r.unit, r.task, r.finding));
            format!("{}#{} {}", r.unit, r.task, r.finding)
        };
        let mut line = format!("{at} ({}): {}", name(&f.found.severity), sentence(&f.found.problem));
        if !f.found.impact.is_empty() {
            line += &format!(" If left: {}", sentence(&f.found.impact));
        }
        if !f.found.r#where.is_empty() {
            line += &format!(" Where: {}.", f.found.r#where.join(", "));
        }
        if let Some(n) = f.note.as_deref().filter(|n| !n.is_empty()) {
            line += &format!(" Note: {}", sentence(n));
        }
        m.push(line);
    }
    m.push(format!(
        "Next: one cleanup unit for all of them (`workriver unit add {intent} --title \"Cleanup: <what>\" --branch <branch>`), then a task with every finding as a cause: `workriver task add <unit> --title \"<outcome>\" {} --agent <agent>`.",
        causes.join(" ")
    ));
    Some(paragraph(m))
}

/// " Attachments: <paths>." for a finding, or for a task (its own, its checks' and its
/// findings'), so the receiver can open them; empty when there are none.
fn files(state: &State, home: &std::path::Path, unit: &str, n: u32, finding: Option<&str>) -> String {
    let Ok(t) = state.task(unit, n) else { return String::new() };
    let ids: Vec<&String> = match finding {
        Some(id) => t.findings.iter().filter(|f| f.found.id == id).flat_map(|f| &f.found.attachments).collect(),
        None => t.attachments.iter().chain(t.evidence.iter().filter_map(|e| e.attachment.as_ref())).chain(t.findings.iter().flat_map(|f| &f.found.attachments)).collect(),
    };
    let paths: Vec<String> = ids
        .into_iter()
        .filter_map(|id| state.attachments.iter().find(|a| &a.id == id))
        .map(|a| crate::attachments::path(home, &a.id, &a.name).display().to_string())
        .collect();
    if paths.is_empty() { String::new() } else { format!(" Attachments: {}.", paths.join(", ")) }
}

/// `first` if it is live, else the intent's manager.
fn target(state: &State, live: Option<&[RuntimeAgent]>, intent: &str, first: Option<&str>) -> Result<String, (ReasonCode, String)> {
    let Some(live) = live else { return Err((ReasonCode::RuntimeDown, "the agent runtime (Herdr) is not connected".into())) };
    let is_live = |s: &str| live.iter().any(|a| a.session == s);
    if let Some(a) = first.filter(|a| is_live(a)) {
        return Ok(a.to_string());
    }
    match state.manager_of(intent).map(str::to_string) {
        Some(m) if is_live(&m) => Ok(m),
        Some(m) => Err((ReasonCode::ManagerGone, format!("the manager of intent {intent} ({}) is not running", state.name_of(&m).unwrap_or(&m)))),
        None => Err((ReasonCode::NoManager, format!("intent {intent} has no manager; the manager registers with `workriver manager here`"))),
    }
}

/// The name an actor is shown by.
fn who(state: &State, actor: &str) -> String {
    state.name_of(actor).unwrap_or(actor).to_string()
}

/// What the manager hears when someone delivers a task.
fn delivered_message(state: &State, actor: &str, unit: &str, n: u32) -> Option<String> {
    let t = state.task(unit, n).ok()?;
    let mut m = vec![format!("[workriver] {} delivered by {}.", label(state, unit, t), who(state, actor))];
    if !t.note.is_empty() {
        m.push(format!("Note: {}", sentence(&t.note)));
    }
    let passed = t.evidence.iter().filter(|e| e.result == workriver_core::CheckResult::Passed).count();
    let failed = t.evidence.len() - passed;
    m.push(if t.evidence.is_empty() { "No checks run.".to_string() } else { format!("Evidence: {passed} passed, {failed} failed.") });
    if !t.findings.is_empty() {
        let per: Vec<String> = ["P1", "P2", "P3"]
            .iter()
            .filter_map(|s| {
                let k = t.findings.iter().filter(|f| name(&f.found.severity) == *s).count();
                (k > 0).then(|| format!("{s} {k}"))
            })
            .collect();
        m.push(format!("Findings: {} (waiting for the user's decision).", per.join(", ")));
    }
    m.push(format!("Next: hand out the next task, or see the unit with `workriver board {unit}`."));
    Some(paragraph(m))
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
    let u = state.unit_or_intent(unit)?;
    let t = state.task(unit, n).ok()?;
    let f = t.findings.iter().find(|f| f.found.id == finding)?;
    let from = format!("from {} in {} {unit} \"{}\"", label(state, unit, t), if u.branch.is_empty() { "intent" } else { "unit" }, u.title);
    let mut m = vec![match decision {
        Decision::Revise => format!("[workriver] The user wants finding {finding} {from} changed their way: {} Follow the note, not the finding's suggestion.", sentence(note)),
        Decision::Accept => format!("[workriver] The user accepted finding {finding} {from}."),
        Decision::Reject => format!("[workriver] The user rejected finding {finding} {from}."),
        Decision::Defer => format!("[workriver] The user deferred finding {finding} {from}."),
    }];
    m.push(format!("Problem: {}", sentence(&f.found.problem)));
    if !f.found.impact.is_empty() {
        m.push(format!("If left: {}", sentence(&f.found.impact)));
    }
    if !f.found.r#where.is_empty() {
        m.push(format!("Where: {}.", f.found.r#where.join(", ")));
    }
    if !note.is_empty() && decision != Decision::Revise {
        m.push(format!("Note from the user: {}", sentence(note)));
    }
    let design = t.gate == Some(GateKind::Decide);
    m.push(match decision {
        // From a design decision: the design is done again, and confirmed again.
        Decision::Accept | Decision::Revise if design => format!(
            "Next: revise the design: `workriver task add {unit} --kind design --title \"<revised design>\" --cause {n}:{finding} --agent <agent>`, then a new `--kind gate --gate decide` after it; the implementation hangs after the new gate, not after {unit}#{n}."
        ),
        Decision::Accept | Decision::Revise => format!(
            "Next: hand out work for it, e.g. `workriver task add {unit} --title \"<what to do>\" --cause {n}:{finding} --agent <agent>`, then give that task to an agent with the worker instructions (`workriver guide worker`)."
        ),
        Decision::Reject => "No work is needed for it; tell whoever waits on it if anyone does.".into(),
        Decision::Defer => "Leave it for now; it waits in the project's backlog until the user picks it.".into(),
    });
    Some(paragraph(m))
}

fn gate_message(state: &State, unit: &str, n: u32) -> Option<String> {
    let t = state.task(unit, n).ok()?;
    let problem = t.result == Some(GateResult::Problem);
    let decide = t.gate == Some(GateKind::Decide);
    let mut m = vec![match (decide, problem) {
        (true, false) => format!("[workriver] The user confirmed the design in gate {}.", label(state, unit, t)),
        (true, true) => format!("[workriver] The user did not confirm the design in gate {}: revise the design.", label(state, unit, t)),
        (false, p) => format!("[workriver] The user finished gate {}: {}.", label(state, unit, t), if p { "found a problem" } else { "done" }),
    }];
    if !t.note.is_empty() {
        m.push(format!("{} {}", if decide && !problem { "The decision:" } else { "Note from the user:" }, sentence(&t.note)));
    }
    if decide && !problem {
        m.push(format!(
            "Next: hand out the implementation accordingly, after this gate: `workriver task add {unit} --title \"<outcome>\" --after {n} --agent <agent>`, citing the decision in the task's text."
        ));
    } else if problem {
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
        m.push(if decide {
            format!(
                "The user decides on each finding; for each accepted or revised one, a revised design task (`--kind design --cause {n}:<id>`) and a new decide gate after it; the implementation hangs after the new gate, not after {unit}#{n}."
            )
        } else {
            format!(
                "The user decides on each finding in workriver; accepted or revised ones reach the manager, who hands out work with `--cause {n}:<id>`. Until then the work behind gate {unit}#{n} waits."
            )
        });
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
        RuntimeAgent { session: session.into(), harness: "claude".into(), state: AgentState::Idle, cwd: String::new(), group: None, name: session.into(), topic: None, machine: None }
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
        let p = plan(s.state(), Some(&live), std::path::Path::new("/h"), &accept).unwrap();
        assert_eq!(p.to.unwrap_err().0, ReasonCode::NoManager);
        assert!(!p.message.contains('\n'), "one paragraph: {}", p.message);
        assert!(p.message.contains("--cause 2:P2-1"), "{}", p.message);
        assert!(p.message.contains("u1#2 (code review of u1#1 \"Find mentions\")"), "{}", p.message);
        assert_eq!(plan(s.state(), None, std::path::Path::new("/h"), &accept).unwrap().to.unwrap_err().0, ReasonCode::RuntimeDown);

        at(&mut s, "boss", json!({"kind": "manager.registered", "body": {"intent": "i1", "agent": "boss"}}));
        assert_eq!(plan(s.state(), Some(&live), std::path::Path::new("/h"), &accept).unwrap().to, Ok("boss".into()));
        // Every decision goes to the manager, a rejection too.
        let reject = at(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P3-1", "decision": "reject", "by": "user"}}));
        assert_eq!(plan(s.state(), Some(&live), std::path::Path::new("/h"), &reject).unwrap().to, Ok("boss".into()));
        // A gate result goes to the agent of the work the gate follows.
        let gate = at(&mut s, "user", json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 3, "result": "problem", "note": "Mostly fine",
            "findings": [{"severity": "P1", "problem": "Crashes"}]}}));
        let p = plan(s.state(), Some(&live), std::path::Path::new("/h"), &gate).unwrap();
        assert_eq!(p.to, Ok("worker".into()));
        assert!(p.message.contains("found a problem") && p.message.contains("P1-1 (P1): Crashes."), "{}", p.message);
        // A worker's delivery and the gates and chores it asks for are told to the manager.
        at(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Fix twice", "causes": [{"task": 2, "findings": ["P2-1"]}]}}));
        let done = at(&mut s, "worker", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 4, "note": "Deduplicated",
            "evidence": [{"command": "npm test", "result": "passed", "summary": "7 passed"}, {"command": "npm run lint", "result": "failed", "summary": "1 error"}]}}));
        let p = plan(s.state(), Some(&live), std::path::Path::new("/h"), &done).unwrap();
        assert_eq!((p.to, p.finding, p.task), (Ok("boss".into()), None, 4));
        assert!(p.message.starts_with("[workriver] u1#4 \"Fix twice\" delivered by worker. Note: Deduplicated. Evidence: 1 passed, 1 failed."), "{}", p.message);
        assert!(p.message.contains("`workriver board u1`"), "{}", p.message);
        let asked = at(&mut s, "worker", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "deploy", "title": "Deploy", "inputs": [{"from": "4"}]}}));
        assert!(plan(s.state(), Some(&live), std::path::Path::new("/h"), &asked).unwrap().message.contains("worker asked the user for gate u1#5 \"Deploy\" (deploy)"));
        let chore = at(&mut s, "worker", json!({"kind": "chore.created", "body": {"intent": "i1", "title": "Rotate the key"}}));
        let p = plan(s.state(), Some(&live), std::path::Path::new("/h"), &chore).unwrap();
        assert_eq!((p.chore.as_deref(), p.to), (Some("c1"), Ok("boss".into())));
        // The manager's own records are not told back to it.
        let own_gate = at(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "test", "title": "Try", "inputs": [{"from": "4"}]}}));
        assert_eq!(plan(s.state(), Some(&live), std::path::Path::new("/h"), &own_gate), None);
        // A revision is told as the user's way; a design decision as such.
        at(&mut s, "worker", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "design", "title": "Design it", "inputs": [{"from": "1"}]}}));
        at(&mut s, "worker", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 7, "note": "One table"}}));
        at(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "decide", "title": "Confirm it", "inputs": [{"from": "7"}]}}));
        let confirmed = at(&mut s, "user", json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 8, "result": "problem", "note": "Not yet",
            "findings": [{"severity": "P2", "problem": "Too rigid"}]}}));
        let p = plan(s.state(), Some(&live), std::path::Path::new("/h"), &confirmed).unwrap();
        assert_eq!(p.to, Ok("boss".into()), "a design decision goes to the manager");
        assert!(p.message.contains("did not confirm the design") && p.message.contains("revise the design"), "{}", p.message);
        let revise = at(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 8, "finding": "P2-1", "decision": "revise", "note": "Two tables", "by": "user"}}));
        let p = plan(s.state(), Some(&live), std::path::Path::new("/h"), &revise).unwrap();
        assert!(p.message.contains("changed their way: Two tables. Follow the note, not the finding's suggestion."), "{}", p.message);
        assert!(p.message.contains("--kind design") && p.message.contains("--cause 8:P2-1") && p.message.contains("--gate decide"), "{}", p.message);
        // An agent's own decision is not delivered.
        let own = at(&mut s, "rev", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 3, "finding": "P1-1", "decision": "defer", "by": "agent"}}));
        assert_eq!(plan(s.state(), Some(&live), std::path::Path::new("/h"), &own), None);
    }
}
