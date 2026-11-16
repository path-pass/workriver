// Where the user's inputs go once recorded, and what the message says. Pure: the daemon
// sends the message and records how far it got.
//
// Every message has the same shape: workriver's own words (what happened, what to do
// next), then what others wrote, quoted (see quote.rs). "The user …" is said only of
// events the user recorded, which the rules check (`Body::user_only`, `Who::User`).
use workriver_core::runtime::RuntimeAgent;
use workriver_core::{Body, Decision, Event, Finding, GateKind, GateResult, ReasonCode, State, Task, TaskKind, Who, name};

use crate::quote::{self, quoted, users};

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
        Body::FindingDecided { unit, task, finding, decision, note, by: Who::User, was } => {
            let to = target(state, live, &intent_of(unit)?, None);
            Some(on(unit, *task, Some(finding.clone()), to, decision_message(state, unit, *task, finding, *decision, note, *was)?.text(&files(unit, *task, Some(finding)))))
        }
        // A gate result goes to the agent waiting behind the gate, else the manager; a design
        // decision always to the manager, who hands out what follows.
        Body::GateSubmitted { unit, task, .. } if e.actor == "user" => {
            let gate = state.task(unit, *task).ok();
            let decide = gate.is_some_and(|g| g.gate == Some(GateKind::Decide));
            // Never to an agent on a blind task: it is told nothing until it delivers.
            let waiting = gate.filter(|_| !decide).and_then(|t| followed(state, unit, t)).and_then(|t| t.agent.clone()).filter(|a| state.blind_tasks(a).is_empty());
            let to = target(state, live, &intent_of(unit)?, waiting.as_deref());
            Some(on(unit, *task, None, to, gate_message(state, unit, *task)?.text(&files(unit, *task, None))))
        }
        Body::TaskDelivered { unit, task, .. } if not_manager(&intent_of(unit)?) => {
            let to = target(state, live, &intent_of(unit)?, None);
            Some(on(unit, *task, None, to, delivered_message(state, &e.actor, unit, *task)?.text(&files(unit, *task, None))))
        }
        // Findings added to a delivered review: told like a delivery.
        Body::TaskAmended { unit, task, add, .. } if !add.is_empty() && not_manager(&intent_of(unit)?) => {
            let to = target(state, live, &intent_of(unit)?, None);
            Some(on(unit, *task, None, to, added_message(state, &e.actor, unit, *task, add.len())?.text("")))
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
        // A chore of no intent has no manager to tell.
        Body::ChoreCreated { id, intent, title, .. } if !intent.is_empty() && e.actor != "user" && not_manager(intent) => Some(Plan {
            unit: String::new(),
            task: 0,
            finding: None,
            chore: Some(id.clone()),
            to: target(state, live, intent, None),
            message: format!("[workriver] {} asked the user for chore {id} \"{}\"; it blocks nobody.", who(state, &e.actor), quote::name(title)),
        }),
        _ => None,
    }
}

/// A message in two parts: workriver's own words, then what others wrote, quoted.
#[derive(Default)]
struct Msg {
    own: Vec<String>,
    quotes: Vec<String>,
}

impl Msg {
    /// One paragraph (a terminal without bracketed paste submits at each newline): what
    /// happened and what to do, where the files are, then the quotes.
    fn text(self, files: &str) -> String {
        let own = paragraph(self.own) + files;
        paragraph([vec![own], self.quotes].concat())
    }
}

/// A finding's fields as one text, to be quoted.
fn finding_text(f: &Finding) -> String {
    let mut t = format!("Problem: {}", sentence(&f.found.problem));
    if !f.found.impact.is_empty() {
        t += &format!(" If left: {}", sentence(&f.found.impact));
    }
    if !f.found.r#where.is_empty() {
        t += &format!(" Where: {}.", f.found.r#where.join(", "));
    }
    t
}

/// A finding quoted from whoever wrote it: the agent of the task that found it, or (a
/// gate's) the user.
fn quoted_finding(state: &State, lead: &str, t: &Task, f: &Finding) -> String {
    match (t.kind, t.agent.as_deref()) {
        (TaskKind::Gate, _) => users(&format!("{lead}, as the user wrote it"), &finding_text(f)),
        (_, Some(a)) => quoted(&format!("{lead}, quoted from {}", who(state, a)), &finding_text(f)),
        (_, None) => quoted(&format!("{lead}, quoted from its report"), &finding_text(f)),
    }
}

fn batch_message(state: &State, intent: &str, refs: &[workriver_core::FindingRef]) -> Option<String> {
    let i = state.intents.iter().find(|i| i.id == intent)?;
    let mut m = Msg::default();
    let (mut causes, mut list) = (vec![], vec![]);
    for r in refs {
        let f = state.finding(r).ok()?;
        if r.task == 0 {
            let b = state.backlog.iter().find(|b| b.finding.found.id == r.finding)?;
            causes.push(format!("--cause {}", r.finding));
            list.push(format!("{} ({}, backlog)", r.finding, name(&f.found.severity)));
            let from = if b.source.is_empty() { String::new() } else { format!(" From: {}.", b.source) };
            let note = f.note.as_deref().filter(|n| !n.is_empty()).map(|n| format!(" Note: {}", sentence(n))).unwrap_or_default();
            m.quotes.push(quoted(&format!("{}, quoted from the backlog as {} added it", r.finding, who(state, &b.by)), &format!("{}{from}{note}", finding_text(f))));
        } else {
            let at = format!("{}#{} {}", r.unit, r.task, r.finding);
            causes.push(format!("--cause {}#{}:{}", r.unit, r.task, r.finding));
            list.push(format!("{at} ({})", name(&f.found.severity)));
            m.quotes.push(quoted_finding(state, &at, state.task(&r.unit, r.task).ok()?, f));
        }
    }
    m.own.push(format!("[workriver] The user picked {} deferred findings of intent {intent} \"{}\" to do together: {}.", refs.len(), quote::name(&i.title), list.join(", ")));
    m.own.push(format!(
        "Next: one cleanup unit for all of them (`workriver unit add {intent} --title \"Cleanup: <what>\" --branch <branch>`), then a task with every finding as a cause: `workriver task add <unit> --title \"<outcome>\" {} --agent <agent>`.",
        causes.join(" ")
    ));
    Some(m.text(""))
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

/// The name an actor is shown by. A name is someone's choice: kept short and on one line.
fn who(state: &State, actor: &str) -> String {
    quote::name(state.name_of(actor).unwrap_or(actor))
}

/// What the manager hears when someone delivers a task.
fn delivered_message(state: &State, actor: &str, unit: &str, n: u32) -> Option<Msg> {
    let t = state.task(unit, n).ok()?;
    let mut m = Msg::default();
    m.own.push(format!("[workriver] {} delivered by {}.", label(state, unit, t), who(state, actor)));
    let passed = t.evidence.iter().filter(|e| e.result == workriver_core::CheckResult::Passed).count();
    let failed = t.evidence.len() - passed;
    // Design and research hand in a note, not checks.
    if !t.evidence.is_empty() {
        m.own.push(format!("Evidence: {passed} passed, {failed} failed."));
    } else if !matches!(t.kind, TaskKind::Design | TaskKind::Research) {
        m.own.push("No checks run.".to_string());
    }
    if !t.findings.is_empty() {
        m.own.push(format!("Findings: {}", whose(state, unit, n, &t.findings.iter().collect::<Vec<_>>())));
    }
    m.own.push(format!("Next: hand out the next task, or see the unit with `workriver board {unit}`."));
    if !t.note.is_empty() {
        m.quotes.push(quoted(&format!("Its note, quoted from {}", who(state, actor)), &t.note));
    }
    m.own.push(details(unit, n));
    Some(m)
}

/// How many findings per severity, and whose they are to decide: the user's, unless the
/// user handed this intent's findings to its manager. Ends a sentence that names them.
fn whose(state: &State, unit: &str, n: u32, list: &[&Finding]) -> String {
    let per: Vec<String> = ["P1", "P2", "P3"]
        .iter()
        .filter_map(|s| {
            let k = list.iter().filter(|f| name(&f.found.severity) == *s).count();
            (k > 0).then(|| format!("{s} {k}"))
        })
        .collect();
    let policy = state.intent_of(unit).and_then(|i| state.intents.iter().find(|x| x.id == i)).map(|i| i.findings).unwrap_or_default();
    if policy.managers() {
        format!(
            "{}. They are yours to decide: `workriver finding decide {unit}#{n} <id> accept|revise|reject|defer [--note \"...\"]` (`workriver board {unit}` lists them); then hand out work for what you accept. The user may decide any of them instead, or change what you decided.",
            per.join(", ")
        )
    } else {
        format!("{} (waiting for the user's decision: the user decides this intent's findings).", per.join(", "))
    }
}

/// What the manager hears when a reviewer adds findings to a review already delivered.
fn added_message(state: &State, actor: &str, unit: &str, n: u32, added: usize) -> Option<Msg> {
    let t = state.task(unit, n).ok()?;
    let new: Vec<&Finding> = t.findings.iter().rev().take(added).rev().collect();
    let mut m = Msg::default();
    m.own.push(format!("[workriver] {} added {added} findings to {}: {}", who(state, actor), label(state, unit, t), whose(state, unit, n, &new)));
    m.own.push(format!("The earlier findings of it stay as they are. See them with `workriver board {unit}`."));
    m.own.push(details(unit, n));
    Some(m)
}

/// Who is told that something was added to a finding, and what. The agents of the work it
/// led to that is still open (they must meet the new requirement) and the intent's manager;
/// while nobody decided it yet, the manager only when it is the manager's to decide (the
/// user sees it on the page). Never the one who wrote it, never an agent on a blind task.
pub fn notes(state: &State, live: Option<&[RuntimeAgent]>, home: &std::path::Path, e: &Event) -> Vec<(String, String)> {
    let Body::FindingNoted { unit, task, finding, text, attachments } = &e.body else { return vec![] };
    let (Ok(t), Some(live)) = (state.task(unit, *task), live) else { return vec![] };
    let Some(f) = t.findings.iter().find(|f| &f.found.id == finding) else { return vec![] };
    let intent = state.intent_of(unit).unwrap_or_default();
    let policy = state.intents.iter().find(|i| i.id == intent).map(|i| i.findings).unwrap_or_default();
    let by = who(state, &e.actor);
    let of = format!("finding {finding} of {}", label(state, unit, t));
    let files: Vec<String> = attachments.iter().filter_map(|id| state.attachments.iter().find(|a| &a.id == id)).map(|a| crate::attachments::path(home, &a.id, &a.name).display().to_string()).collect();
    let files = if files.is_empty() { String::new() } else { format!(" Attachments: {}.", files.join(", ")) };
    let said = if e.actor == "user" { users("The user wrote", text) } else { quoted(&format!("What was added, quoted from {by}"), text) };
    let open: Vec<(String, &Task)> = state.follow_ups(unit, *task, finding).into_iter().filter(|(_, x)| x.state != workriver_core::TaskState::Done).collect();
    let mut out: Vec<(String, String)> = vec![];
    for (owner, x) in &open {
        if let Some(a) = x.agent.as_deref() {
            out.push((a.to_string(), format!("[workriver] {by} added to {of}, a cause of your task {owner}#{}: your work must meet it too. {}{files} {said}", x.n, details(owner, x.n))));
        }
    }
    let waiting = f.state == workriver_core::FindingState::Open;
    if !waiting || policy.managers() {
        let about = match (waiting, open.is_empty()) {
            (true, _) => "it is yours to decide".to_string(),
            (false, true) => format!("it is {}; no open task answers it", name(&f.state)),
            (false, false) => format!("it leads to {}, whose agents were told", open.iter().map(|(o, x)| format!("{o}#{}", x.n)).collect::<Vec<_>>().join(", ")),
        };
        if let Some(m) = state.manager_of(intent) {
            out.push((m.to_string(), format!("[workriver] {by} added to {of} ({about}). {}{files} {said}", details(unit, *task))));
        }
    }
    out.retain(|(to, _)| to != &e.actor && live.iter().any(|a| &a.session == to) && state.blind_tasks(to).is_empty());
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// The last of workriver's own words in a message about a task: where all of it is. Before
/// any quote, so that nobody's quoted text can pass for it.
fn details(owner: &str, n: u32) -> String {
    format!("Details: `workriver task show {owner}#{n}`.")
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
        _ => format!("{unit}#{} \"{}\"", t.n, quote::name(&t.title)),
    }
}

/// `was`: the user changes a decision already made (the manager's, or their own).
fn decision_message(state: &State, unit: &str, n: u32, finding: &str, decision: Decision, note: &str, was: Option<workriver_core::FindingState>) -> Option<Msg> {
    let u = state.unit_or_intent(unit)?;
    let t = state.task(unit, n).ok()?;
    let f = t.findings.iter().find(|f| f.found.id == finding)?;
    let from = format!("from {} in {} {unit} \"{}\"", label(state, unit, t), if u.branch.is_empty() { "intent" } else { "unit" }, quote::name(&u.title));
    let mut m = Msg::default();
    m.own.push(match decision {
        Decision::Revise => format!("[workriver] The user wants finding {finding} {from} fixed their way: follow what the user wrote below, not the finding's suggestion."),
        Decision::Accept => format!("[workriver] The user accepted finding {finding} {from}."),
        Decision::Reject => format!("[workriver] The user rejected finding {finding} {from}."),
        Decision::Defer => format!("[workriver] The user deferred finding {finding} {from}."),
    });
    if let Some(was) = was {
        // What was built on the earlier decision, so the manager can stop or adjust it.
        let built: Vec<String> = state.follow_ups(unit, n, finding).iter().map(|(o, x)| format!("{o}#{} ({})", x.n, name(&x.state))).collect();
        m.own.push(format!(
            "This changes the earlier decision (it was {}); the user's decision stands.{}",
            name(&was),
            if built.is_empty() { String::new() } else { format!(" Work already handed out for it: {}. Stop or adjust what is still open, and tell its agents.", built.join(", ")) }
        ));
    }
    let design = t.gate == Some(GateKind::Decide);
    m.own.push(match decision {
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
    m.quotes.push(quoted_finding(state, "The finding", t, f));
    if !note.trim().is_empty() {
        m.quotes.push(users("The user wrote", note));
    }
    m.own.push(details(unit, n));
    Some(m)
}

fn gate_message(state: &State, unit: &str, n: u32) -> Option<Msg> {
    let t = state.task(unit, n).ok()?;
    let problem = t.result == Some(GateResult::Problem);
    let decide = t.gate == Some(GateKind::Decide);
    let mut m = Msg::default();
    m.own.push(match (decide, problem) {
        (true, false) => format!("[workriver] The user confirmed the design in gate {}.", label(state, unit, t)),
        (true, true) => format!("[workriver] The user did not confirm the design in gate {}: revise the design.", label(state, unit, t)),
        (false, p) => format!("[workriver] The user finished gate {}: {}.", label(state, unit, t), if p { "found a problem" } else { "done" }),
    });
    if decide && !problem {
        m.own.push(format!(
            "Next: hand out the implementation accordingly, after this gate: `workriver task add {unit} --title \"<outcome>\" --after {n} --agent <agent>`, citing the user's decision in the task's text."
        ));
    } else if problem {
        let ids: Vec<String> = t.findings.iter().map(|f| format!("{} ({})", f.found.id, name(&f.found.severity))).collect();
        m.own.push(format!("Findings: {}.", ids.join(", ")));
        m.own.push(if decide {
            format!(
                "The user decides on each finding; for each accepted or revised one, a revised design task (`--kind design --cause {n}:<id>`) and a new decide gate after it; the implementation hangs after the new gate, not after {unit}#{n}."
            )
        } else {
            format!(
                "The user decides on each finding in workriver; accepted or revised ones reach the manager, who hands out work with `--cause {n}:<id>`. Until then the work behind gate {unit}#{n} waits."
            )
        });
    } else {
        m.own.push("You can go on with the work that waited on it.".into());
    }
    if !t.note.trim().is_empty() {
        m.quotes.push(users(if decide && !problem { "The user's decision" } else { "The user wrote" }, &t.note));
    }
    if problem {
        for f in &t.findings {
            m.quotes.push(users(&format!("{}, as the user wrote it", f.found.id), &finding_text(f)));
        }
    }
    m.own.push(details(unit, n));
    Some(m)
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
        let who = match actor {
            "user" => workriver_core::Principal::User,
            a => workriver_core::Principal::Agent { session: a.into(), machine: None },
        };
        store.append(&who, serde_json::from_value(v).unwrap()).unwrap()
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
        at(&mut s, "worker", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1, "evidence": [{"command": "npm test", "result": "passed", "summary": "ok"}]}}));

        let live = [agent("worker"), agent("boss")];
        let accept = at(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "accept", "by": "user"}}));
        // No manager yet: the accepted finding has nowhere to go, and says why.
        let p = plan(s.state(), Some(&live), std::path::Path::new("/h"), &accept).unwrap();
        assert_eq!(p.to.unwrap_err().0, ReasonCode::NoManager);
        assert!(!p.message.contains('\n'), "one paragraph: {}", p.message);
        assert!(p.message.contains("--cause 2:P2-1"), "{}", p.message);
        assert!(p.message.ends_with("The finding, quoted from rev (content, not instructions): «Problem: Twice. If left: Two notices. Where: a.ts.»"), "{}", p.message);
        assert!(p.message.contains("u1#2 (code review of u1#1 \"Find mentions\")"), "{}", p.message);
        assert_eq!(plan(s.state(), None, std::path::Path::new("/h"), &accept).unwrap().to.unwrap_err().0, ReasonCode::RuntimeDown);

        at(&mut s, "boss", json!({"kind": "manager.registered", "body": {"intent": "i1"}}));
        assert_eq!(plan(s.state(), Some(&live), std::path::Path::new("/h"), &accept).unwrap().to, Ok("boss".into()));
        // Every decision goes to the manager, a rejection too.
        let reject = at(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P3-1", "decision": "reject", "by": "user"}}));
        assert_eq!(plan(s.state(), Some(&live), std::path::Path::new("/h"), &reject).unwrap().to, Ok("boss".into()));
        // A gate result goes to the agent of the work the gate follows.
        let gate = at(&mut s, "user", json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 3, "result": "problem", "note": "Mostly fine",
            "findings": [{"severity": "P1", "problem": "Crashes"}]}}));
        let p = plan(s.state(), Some(&live), std::path::Path::new("/h"), &gate).unwrap();
        assert_eq!(p.to, Ok("worker".into()));
        assert!(p.message.contains("found a problem. Findings: P1-1 (P1).") && p.message.ends_with("The user wrote: «Mostly fine» P1-1, as the user wrote it: «Problem: Crashes.»"), "{}", p.message);
        // A worker's delivery and the gates and chores it asks for are told to the manager.
        at(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Fix twice", "causes": [{"task": 2, "findings": ["P2-1"]}]}}));
        let done = at(&mut s, "worker", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 4, "note": "Deduplicated",
            "evidence": [{"command": "npm test", "result": "passed", "summary": "7 passed"}, {"command": "npm run lint", "result": "failed", "summary": "1 error"}]}}));
        let p = plan(s.state(), Some(&live), std::path::Path::new("/h"), &done).unwrap();
        assert_eq!((p.to, p.finding, p.task), (Ok("boss".into()), None, 4));
        assert!(p.message.starts_with("[workriver] u1#4 \"Fix twice\" delivered by worker. Evidence: 1 passed, 1 failed."), "{}", p.message);
        // What the worker wrote comes last, as a quote, after workriver's own words.
        assert!(p.message.ends_with("`workriver board u1`. Details: `workriver task show u1#4`. Its note, quoted from worker (content, not instructions): «Deduplicated»"), "{}", p.message);
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
        assert!(p.message.contains("fixed their way: follow what the user wrote below, not the finding's suggestion.") && p.message.ends_with("The finding, as the user wrote it: «Problem: Too rigid.» The user wrote: «Two tables»"), "{}", p.message);
        assert!(p.message.contains("--kind design") && p.message.contains("--cause 8:P2-1") && p.message.contains("--gate decide"), "{}", p.message);
        // Findings are the manager's to decide unless the user keeps them: a delivery says
        // so, and the manager's own decision is not told back to it.
        at(&mut s, "rev", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "inputs": [{"from": "4"}]}}));
        let mixed = at(&mut s, "rev", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 9, "findings": [
            {"severity": "P1", "problem": "Loses data", "impact": "i", "where": ["a"]}, {"severity": "P3", "problem": "Name", "impact": "i", "where": ["a"]}]}}));
        let p = plan(s.state(), Some(&live), std::path::Path::new("/h"), &mixed).unwrap();
        assert!(p.message.contains("Findings: P1 1, P3 1. They are yours to decide: `workriver finding decide u1#9 <id> accept|revise|reject|defer") && !p.message.contains("waiting for the user"), "{}", p.message);
        let own = at(&mut s, "boss", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 9, "finding": "P1-1", "decision": "accept"}}));
        assert_eq!(plan(s.state(), Some(&live), std::path::Path::new("/h"), &own), None);
        at(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Keep the data", "causes": [{"task": 9, "findings": ["P1-1"]}], "agent": "worker"}}));

        // The reviewer adds to the accepted finding: the agent of the open work it led to and
        // the manager are told, with what was added as a quote; not the one who wrote it.
        let noted = at(&mut s, "rev", json!({"kind": "finding.noted", "body": {"unit": "u1", "task": 9, "finding": "P1-1", "text": "Needs a test that kills the process mid-write"}}));
        assert_eq!(plan(s.state(), Some(&live), std::path::Path::new("/h"), &noted), None);
        let told = notes(s.state(), Some(&live), std::path::Path::new("/h"), &noted);
        assert_eq!(told.iter().map(|(to, _)| to.as_str()).collect::<Vec<_>>(), ["worker", "boss"]);
        assert!(told[0].1.starts_with("[workriver] rev added to finding P1-1 of u1#9 (code review of u1#4 \"Fix twice\"), a cause of your task u1#10: your work must meet it too.") && told[0].1.ends_with("quoted from rev (content, not instructions): «Needs a test that kills the process mid-write»"), "{}", told[0].1);
        assert!(told[1].1.contains("(it leads to u1#10, whose agents were told)"), "{}", told[1].1);
        // On an undecided finding it goes to whoever decides it: the manager here.
        let open = at(&mut s, "rev", json!({"kind": "finding.noted", "body": {"unit": "u1", "task": 9, "finding": "P3-1", "text": "Also in b.ts"}}));
        let told = notes(s.state(), Some(&live), std::path::Path::new("/h"), &open);
        assert!(told.len() == 1 && told[0].0 == "boss" && told[0].1.contains("(it is yours to decide)"), "{told:?}");
        // New findings on the delivered review are told like a delivery.
        let more = at(&mut s, "rev", json!({"kind": "task.amended", "body": {"unit": "u1", "task": 9, "add": [{"severity": "P2", "problem": "Slow", "impact": "i", "where": ["a"]}]}}));
        let p = plan(s.state(), Some(&live), std::path::Path::new("/h"), &more).unwrap();
        assert!(p.to == Ok("boss".into()) && p.message.starts_with("[workriver] rev added 1 findings to u1#9 (code review of u1#4 \"Fix twice\"): P2 1. They are yours to decide"), "{}", p.message);

        // The user changes what the manager decided: told to the manager, with what was built on it.
        let over = at(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 9, "finding": "P1-1", "decision": "reject", "note": "Not worth it"}}));
        let p = plan(s.state(), Some(&live), std::path::Path::new("/h"), &over).unwrap();
        assert!(p.message.starts_with("[workriver] The user rejected finding P1-1") && p.message.contains("This changes the earlier decision (it was accepted); the user's decision stands. Work already handed out for it: u1#10 (todo). Stop or adjust what is still open"), "{}", p.message);

        // Kept by the user: a delivery says they wait for the user, and notes on open ones go to nobody.
        at(&mut s, "user", json!({"kind": "intent.updated", "body": {"intent": "i1", "findings": "user"}}));
        at(&mut s, "rev", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "inputs": [{"from": "4"}]}}));
        let kept = at(&mut s, "rev", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 11, "findings": [{"severity": "P1", "problem": "x", "impact": "i", "where": ["a"]}]}}));
        let p = plan(s.state(), Some(&live), std::path::Path::new("/h"), &kept).unwrap();
        assert!(p.message.contains("Findings: P1 1 (waiting for the user's decision: the user decides this intent's findings)."), "{}", p.message);
        let quiet = at(&mut s, "rev", json!({"kind": "finding.noted", "body": {"unit": "u1", "task": 11, "finding": "P1-1", "text": "More"}}));
        assert!(notes(s.state(), Some(&live), std::path::Path::new("/h"), &quiet).is_empty());
    }
}
