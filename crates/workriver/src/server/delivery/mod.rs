// Where the user's inputs go once recorded, and what the message says. Pure: the daemon
// sends the message and records how far it got.
//
// Every message has the same shape: workriver's own words (what happened, what to do
// next), then what others wrote, quoted (see quote.rs). "The user …" is said only of
// events the user recorded, which the rules check (`Body::user_only`, `Who::User`).
use std::path::Path;

use workriver_core::runtime::RuntimeAgent;
use workriver_core::{AskContext, Body, Decision, Event, Finding, GateKind, GateResult, ReasonCode, State, Task, TaskKind, TaskState, Who, name};

use quote::{quoted, users};

mod messages;
pub(super) mod quote;
pub(super) mod reminders;

pub(super) use messages::*;

/// A message to deliver: about a user's decision (`finding` set) or gate result, about a
/// delivered task or a gate asked for (to the manager), or about a chore (`chore` set).
#[derive(Debug, PartialEq)]
pub(super) struct Plan {
    pub(super) unit: String,
    pub(super) task: u32,
    pub(super) finding: Option<String>,
    pub(super) chore: Option<String>,
    /// The receiving session, or why there is none.
    pub(super) to: Result<String, (ReasonCode, String)>,
    pub(super) message: String,
}

/// The delivery an event calls for, if any.
/// `home`: the data directory, where stored attachments are listed from.
pub(super) fn plan(state: &State, live: Option<&[RuntimeAgent]>, home: &std::path::Path, e: &Event) -> Option<Plan> {
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
        // A gate result goes to the agent waiting on the gate, else the manager; a design
        // decision always to the manager, who hands out what follows.
        Body::GateSubmitted { unit, task, .. } if e.actor == "user" => {
            let gate = state.task(unit, *task).ok();
            let decide = gate.is_some_and(|g| g.gate == Some(GateKind::Decide));
            // Never to an agent on a blind task: it is told nothing until it delivers.
            let waiting = gate.filter(|_| !decide).and_then(|g| waiting_on(state, unit, g)).filter(|a| state.blind_tasks(a).is_empty());
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
            // A blocker: which task is stuck, and what its agent wrote, quoted.
            if let Some(stuck) = t.blocks.and_then(|b| state.task(&t.unit, b).ok()) {
                let mut m = Msg::default();
                m.own.push(format!(
                    "[workriver] {} is stuck on {}: it put gate {} ({}) before it for the user, and that task now waits on it; the user's result goes to it. Nothing to do unless you change the plan (`workriver task drop {}#{} --reason \"...\"`, or hand the task to someone else). Check with `workriver board {}`.",
                    who(state, &e.actor),
                    label(state, &t.unit, stuck),
                    label(state, &t.unit, gate),
                    t.gate.map(|g| name(&g)).unwrap_or_default(),
                    t.unit,
                    t.n,
                    t.unit
                ));
                let said = if gate.text.is_empty() || gate.text == gate.title { gate.title.clone() } else { format!("{}: {}", gate.title, gate.text) };
                m.quotes.push(quoted(&format!("What {} asks the user", who(state, &e.actor)), &said));
                return Some(on(&t.unit, t.n, None, to, m.text("")));
            }
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

/// `first` if it is live, else the intent's manager.
fn target(state: &State, live: Option<&[RuntimeAgent]>, intent: &str, first: Option<&str>) -> Result<String, (ReasonCode, String)> {
    let Some(live) = live else { return Err((ReasonCode::RuntimeDown, crate::runtime::words::NOT_CONNECTED.into())) };
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

/// Who is told that something was added to a finding, and what. The agents of the work it
/// led to that is still open (they must meet the new requirement) and the intent's manager;
/// while nobody decided it yet, the manager only when it is the manager's to decide (the
/// user sees it on the page). Never the one who wrote it. Who can take it when (a blind task,
/// a person's prompt, not listed) is `App::hand`'s: held, never dropped (`For::Duty`).
pub(super) fn notes(state: &State, e: &Event, home: &std::path::Path) -> Vec<(String, String)> {
    let Body::FindingNoted { unit, task, finding, text, attachments } = &e.body else { return vec![] };
    let Ok(t) = state.task(unit, *task) else { return vec![] };
    let Some(f) = t.findings.iter().find(|f| &f.found.id == finding) else { return vec![] };
    let intent = state.intent_of(unit).unwrap_or_default();
    let policy = state.intents.iter().find(|i| i.id == intent).map(|i| i.findings).unwrap_or_default();
    let by = who(state, &e.actor);
    let of = format!("finding {finding} of {}", label(state, unit, t));
    let files: Vec<String> = attachments.iter().filter_map(|id| state.attachments.iter().find(|a| &a.id == id)).filter_map(|a| crate::server::files::store::path(home, &a.id, &a.name)).map(|p| p.display().to_string()).collect();
    let files = if files.is_empty() { String::new() } else { format!(" Attachments: {}.", files.join(", ")) };
    let said = if e.actor == "user" { users("The user wrote", text) } else { quoted(&format!("What was added, quoted from {by}"), text) };
    let open: Vec<(String, &Task)> = state.follow_ups(unit, *task, finding).into_iter().filter(|(_, x)| !x.state.over()).collect();
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
    out.retain(|(to, _)| to != &e.actor);
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// Who is told that a task was dropped: its agent (assigned or started), to stop it, and
/// the intent's manager, who plans; never the one who dropped it. The reason goes in as a
/// quote. Held, never dropped, like every notice an agent must act on (`For::Duty`).
pub(super) fn dropped(state: &State, live: Option<&[RuntimeAgent]>, e: &Event) -> Vec<(String, String)> {
    let Body::TaskDropped { unit, task, reason } = &e.body else { return vec![] };
    let Ok(t) = state.task(unit, *task) else { return vec![] };
    let live = live.unwrap_or_default();
    let user = e.actor == "user";
    let by = if user { "The user".to_string() } else { who(state, &e.actor) };
    let why = if user { users("The user wrote", reason) } else { quoted(&format!("Why, quoted from {by}"), reason) };
    let what = label(state, unit, t);
    let mut out: Vec<(String, String)> = vec![];
    // Assigned by session, or by the name the agent goes by.
    if let Some(a) = t.agent.as_deref().map(|a| live.iter().find(|x| x.session == a || x.name == a).map_or(a, |x| x.session.as_str())) {
        out.push((a.to_string(), format!("[workriver] {by} dropped your task {what}: it is no longer needed. Stop working on it and do not deliver it; keep what you committed. {} {why}", details(unit, *task))));
    }
    if let Some(m) = state.intent_of(unit).and_then(|i| state.manager_of(i)) {
        let agent = t.agent.as_deref().map(|a| format!(" Its agent {} was told to stop.", who(state, a))).unwrap_or_default();
        out.push((m.to_string(), format!("[workriver] {by} dropped {what} as no longer needed; what follows it no longer waits on it.{agent} Re-plan if anything still needs doing. {} {why}", details(unit, *task))));
    }
    out.retain(|(to, _)| to != &e.actor);
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// What an agent told the manager of a task's intent (`workriver manager tell`): to that
/// manager, the agent's words quoted; held while it cannot take it (`For::Duty`).
pub(super) fn told(state: &State, e: &Event) -> Vec<(String, String)> {
    let Body::ManagerTold { unit, task, text } = &e.body else { return vec![] };
    let Ok(t) = state.task(unit, *task) else { return vec![] };
    let Some(m) = state.intent_of(unit).and_then(|i| state.manager_of(i)) else { return vec![] };
    let by = who(state, &e.actor);
    vec![(
        m.to_string(),
        format!("[workriver] {by} tells you about {}. {} {}", label(state, unit, t), details(unit, *task), quoted(&format!("Quoted from {by}"), text)),
    )]
}

/// What the intent's manager hears of the user's discussion of a delivered design or
/// research: not each round, only what it must act on. An amendment of the result while the
/// gate that asks about it is not submitted (after a review that started, the gate would
/// confirm a text the review did not see: drop it, review again, a new gate; a review not
/// started yet reads the amended result), and the conclusion, quoted as the agent's. Never
/// to the one who did it. Who can take it when is `App::hand`'s (`For::Duty`): held while the
/// manager is on a blind task, never dropped.
pub(super) fn discussed(state: &State, live: Option<&[RuntimeAgent]>, e: &Event) -> Vec<(String, String)> {
    let live = live.unwrap_or_default();
    // As the runtime lists it, else as last recorded.
    let named = |s: &str| live.iter().find(|a| a.session == s).map(|a| quote::name(&a.name)).unwrap_or_else(|| who(state, s));
    let found = || -> Option<(String, String)> {
        let (unit, n) = match &e.body {
            Body::TaskAmended { unit, task, .. } => (unit.clone(), *task),
            Body::QuestionAnswered { question, conclusion: true, .. } => {
                let q = state.question(question).ok().filter(|q| q.discuss)?;
                (q.unit.clone(), q.task)
            }
            _ => return None,
        };
        let t = state.task(&unit, n).ok()?;
        let thread = state.discussion(&unit, n)?;
        let gate = state.result_gate(&unit, n).filter(|g| g.state != TaskState::Done);
        let what = label(state, &unit, t);
        let agent = t.agent.as_deref().map(named).unwrap_or_default();
        // The reviews between the result and its gate: one that started read the text before
        // the amendment; one not started yet reads the amended result.
        let reviews: Vec<&Task> = gate
            .and_then(|g| {
                let before = state.before(&unit, g.n);
                Some(state.tasks_of(&unit)?.iter().filter(|r| r.kind == TaskKind::Review && r.dropped.is_none() && before.contains(&r.n) && state.before(&unit, r.n).contains(&n)).collect())
            })
            .unwrap_or_default();
        let reviewed = reviews.iter().copied().find(|r| t.read_before.contains(&r.n));
        let pending = reviews.iter().copied().find(|r| r.state == TaskState::Todo);
        let again = |g: &Task, r: &Task| format!("{} reviewed the text before it: drop the gate (`workriver task drop {unit}#{} --reason \"...\"`), then add the review again and a new decide gate after it (`workriver guide how design-gate`).", label(state, &unit, r), g.n);
        let m = state.intent_of(&unit).and_then(|i| state.manager_of(i))?.to_string();
        let text = match &e.body {
            Body::TaskAmended { .. } => {
                let g = gate?;
                let next = match (reviewed, pending) {
                    (Some(r), _) => again(g, r),
                    (None, Some(r)) => format!("{} has not started: it reads the amended result, and the gate asks about that.", label(state, &unit, r)),
                    (None, None) => "The gate now asks about the amended result; nothing to do unless the change needs a review.".into(),
                };
                format!("[workriver] {} amended {what} while the user discusses it; its gate {} is not submitted yet. {next} {}", named(&e.actor), label(state, &unit, g), details(&unit, n))
            }
            Body::QuestionAnswered { text, .. } => {
                // Amended after the discussion began, in event order.
                let amended = t.amended.as_ref().filter(|_| thread.amended);
                let result = match amended {
                    Some(at) => format!("Its result was amended at {at}."),
                    None => "Its result is as delivered.".into(),
                };
                let next = match (gate, reviewed) {
                    (Some(g), Some(r)) => again(g, r),
                    (Some(g), None) => format!("the gate {} waits for the user.", label(state, &unit, g)),
                    (None, _) => format!("if work must start on this, add a decide gate after {unit}#{n} (`workriver guide how ask-user`); otherwise nothing."),
                };
                format!("[workriver] The user concluded the discussion of {what} with {agent} (thread {}). {result} Next: {next} {}", thread.id, quoted(&format!("Concluded, quoted from {agent}"), text))
            }
            _ => return None,
        };
        Some((m, text))
    };
    found().into_iter().filter(|(to, _)| to != &e.actor).collect()
}

/// Who is told that work is over (a unit merged or done, an intent done): its manager, to
/// tell the user with `workriver say` what came of it, and of the agents that worked on it
/// and are still running with no other task, to close them; workriver never closes them
/// itself. An intent the manager closed itself: its command's output says the first
/// (`said_done`), the message only the second. Held while the manager cannot take it
/// (`For::Duty`); the workers named are those of the runtime's list then.
pub(super) fn finished(state: &State, live: Option<&[RuntimeAgent]>, e: &Event) -> Vec<(String, String)> {
    let (what, owners, came): (String, Vec<String>, &str) = match &e.body {
        Body::UnitMerged { unit, .. } => (format!("{unit} is merged"), vec![unit.clone()], "what was merged"),
        Body::UnitDone { unit, .. } => (format!("{unit} is done"), vec![unit.clone()], "what it came to"),
        Body::IntentStateChanged { intent, state: workriver_core::IntentState::Done, .. } => {
            (format!("Intent {intent} is done"), state.units.iter().filter(|u| &u.intent == intent).map(|u| u.id.clone()).chain([intent.clone()]).collect(), INTENT_CAME)
        }
        _ => return vec![],
    };
    let Some(live) = live else { return vec![] };
    let Some(m) = owners.first().and_then(|o| state.intent_of(o)).and_then(|i| state.manager_of(i)) else { return vec![] };
    let said = matches!(e.body, Body::IntentStateChanged { .. }) && e.actor == m;
    let owners: Vec<&str> = owners.iter().map(String::as_str).collect();
    let names: Vec<String> = reminders::finished_workers(state, live, &owners).iter().map(|x| quote::name(&x.name)).collect();
    let say = (!said).then(|| say(came));
    let close = (!names.is_empty()).then(|| {
        let (are, them) = if names.len() == 1 { ("is", "it") } else { ("are", "them") };
        format!(
            "{} worked on it and {are} still running with no other task; close {them} if not needed (first check for uncommitted work in the work tree). How: `workriver guide how closing-workers`.",
            names.join(", ")
        )
    });
    let message = match (close, say) {
        (None, None) => return vec![],
        (Some(c), None) => format!("[workriver] {what}: {c}"),
        (None, Some(s)) => format!("[workriver] {what}. {s}"),
        (Some(c), Some(s)) => format!("[workriver] {what}: {c} {s}"),
    };
    vec![(m.to_string(), message)]
}

/// What the manager of an active intent hears when the user changes its title, goal or
/// note: the new version, the title and goal quoted as the user's, the note only pointed at
/// (it may be long). Nothing for who decides findings, and nothing of an agent's edit (only
/// the user edits what is not the manager's own).
pub(super) fn edited(state: &State, e: &Event) -> Option<(String, String)> {
    let Body::IntentUpdated { intent, title, goal, note, add_note, .. } = &e.body else { return None };
    if e.actor != "user" {
        return None;
    }
    let i = state.intents.iter().find(|i| &i.id == intent).filter(|i| i.state == workriver_core::IntentState::Active)?;
    let m = state.manager_of(intent)?;
    let n = i.edits.version;
    let mut what = vec![];
    if let Some(t) = title {
        what.push(users("title", t));
    }
    if let Some(g) = goal {
        what.push(users("goal", g));
    }
    if note.is_some() || add_note.is_some() {
        what.push(format!("its note changed (`workriver intent history {intent} --diff {n}`)"));
    }
    if what.is_empty() {
        return None;
    }
    Some((m.to_string(), format!("[workriver] The user changed {intent} (now v{n}): {}.", what.join("; "))))
}

/// What an intent came to, for the manager to tell the user.
const INTENT_CAME: &str = "what the intent came to";

/// The reminder to tell the user what came of finished work (`what`), in one sentence.
fn say(what: &str) -> String {
    format!("Tell the user with `workriver say --text \"...\"` {what} and what they should do now, if anything (restart a service, re-open the page, update other machines).")
}

/// What the output of `workriver intent done` says when the intent's manager ran it, in place
/// of the message: tell the user what the intent came to.
pub(super) fn said_done() -> String {
    say(INTENT_CAME)
}

/// What the manager of the workspace hears when the user starts a planned intent there from
/// the page, and to whom it goes (the intent's manager now, if it runs). The goal is quoted:
/// whoever added the intent wrote it.
pub(super) fn started(state: &State, live: Option<&[RuntimeAgent]>, intent: &str) -> Result<(String, String), (ReasonCode, String)> {
    let i = state.intents.iter().find(|i| i.id == intent).ok_or_else(|| (ReasonCode::NoManager, format!("no intent {intent}")))?;
    let to = target(state, live, intent, None)?;
    let goal = if i.goal.trim().is_empty() { String::new() } else { format!(" {}", quoted("Its goal", &i.goal)) };
    // Its project, so that a manager working on another one stops before splitting it.
    let folder = state.project_folder(i);
    let project = if folder.is_empty() {
        String::new()
    } else {
        format!(" of project «{}» (folder «{}»)", quote::name(&crate::server::views::project_title(state, &folder)), quote::name(&folder))
    };
    let check = if folder.is_empty() { "" } else { " If your workspace works on another project, do not split it: ask the user first (`workriver guide how ask-user`)." };
    Ok((
        to,
        format!(
            "[workriver] The user started {intent} «{}»{project} in your workspace; it is now where new work goes. Split it into units and tasks: `workriver guide how start-intent`, then `workriver board {intent}`.{check}{goal}",
            quote::name(&i.title)
        ),
    ))
}

#[cfg(test)]
mod tests;
