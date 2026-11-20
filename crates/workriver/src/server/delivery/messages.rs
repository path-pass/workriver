// The messages themselves: what a decision, a gate's result, a delivery, a drop or the
// user's own words read like to the agent that gets them. Authored text only through quote.
use super::*;

/// A message in two parts: workriver's own words, then what others wrote, quoted.
#[derive(Default)]
pub(super) struct Msg {
    pub(super) own: Vec<String>,
    pub(super) quotes: Vec<String>,
}

impl Msg {
    /// One paragraph (a terminal without bracketed paste submits at each newline): what
    /// happened and what to do, where the files are, then the quotes.
    pub(super) fn text(self, files: &str) -> String {
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

pub(super) fn batch_message(state: &State, intent: &str, refs: &[workriver_core::FindingRef]) -> Option<String> {
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
        "Next: one cleanup unit for all of them (`workriver unit add {intent} --title \"Cleanup: <what>\" --branch chore/<name>`), then a task with every finding as a cause: `workriver task add <unit> --title \"<outcome>\" {} --agent <agent>`.",
        causes.join(" ")
    ));
    Some(m.text(""))
}

/// " Attachments: <paths>." for a finding, or for a task (its own, its checks' and its
/// findings'), so the receiver can open them; empty when there are none.
pub(super) fn files(state: &State, home: &std::path::Path, unit: &str, n: u32, finding: Option<&str>) -> String {
    let Ok(t) = state.task(unit, n) else { return String::new() };
    let ids: Vec<&String> = match finding {
        Some(id) => t.findings.iter().filter(|f| f.found.id == id).flat_map(|f| &f.found.attachments).collect(),
        None => t.attachments.iter().chain(t.evidence.iter().filter_map(|e| e.attachment.as_ref())).chain(t.findings.iter().flat_map(|f| &f.found.attachments)).collect(),
    };
    let paths: Vec<String> = ids
        .into_iter()
        .filter_map(|id| state.attachments.iter().find(|a| &a.id == id))
        .filter_map(|a| crate::server::files::store::path(home, &a.id, &a.name))
        .map(|p| p.display().to_string())
        .collect();
    if paths.is_empty() { String::new() } else { format!(" Attachments: {}.", paths.join(", ")) }
}

/// The name an actor is shown by. A name is someone's choice: kept short and on one line.
pub(super) fn who(state: &State, actor: &str) -> String {
    quote::name(state.name_of(actor).unwrap_or(actor))
}

/// What the manager hears when someone delivers a task.
pub(super) fn delivered_message(state: &State, actor: &str, unit: &str, n: u32) -> Option<Msg> {
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
pub(super) fn added_message(state: &State, actor: &str, unit: &str, n: u32, added: usize) -> Option<Msg> {
    let t = state.task(unit, n).ok()?;
    let new: Vec<&Finding> = t.findings.iter().rev().take(added).rev().collect();
    let mut m = Msg::default();
    m.own.push(format!("[workriver] {} added {added} findings to {}: {}", who(state, actor), label(state, unit, t), whose(state, unit, n, &new)));
    m.own.push(format!("The earlier findings of it stay as they are. See them with `workriver board {unit}`."));
    m.own.push(details(unit, n));
    Some(m)
}

/// What a manager reads when the user writes to it from the page: workriver's words (what it
/// is, how to reply, that work that takes time goes to a worker, what the user picked and
/// where the code is), then the user's own, quoted as theirs (only the user records
/// `message.sent`, see `Body::user_only`). `dir`: where the
/// unit's branch is checked out (or its repository), found by the caller when the message
/// points at files or lines; named whenever given.
pub(in crate::server) fn user_message(state: &State, m: &workriver_core::Message, home: &Path, dir: Option<&str>) -> String {
    let about = state.intents.iter().find(|i| i.id == m.intent).map(|i| format!(", about intent {} \"{}\"", i.id, quote::name(&i.title))).unwrap_or_default();
    // The user's answer to a manager's choice: the question, the option, the user's note.
    if let Some(a) = &m.answer {
        let value = a.selected.first().cloned().unwrap_or_default();
        let asked = state.message(&a.message).ok();
        let (question, choice) = match a.reply {
            0 => (a.message.clone(), asked.and_then(|q| q.choice.as_ref())),
            k => (format!("in reply {k} to {}", a.message), asked.and_then(|q| q.replies.get(k as usize - 1)).and_then(|r| r.choice.as_ref())),
        };
        let mut said = format!(
            "[workriver] The user answered your question {question} in workriver's page (message {}{about}): option `{}`. Reply with `workriver reply {} --text \"...\"` if there is something to say; the answer starts nothing by itself: do what the option says, within the question.",
            m.id,
            quote::name(&value),
            m.id
        );
        if let Some(words) = choice.and_then(|c| c.label(&value)) {
            said += &format!(" {}", quoted("The option, as you wrote it", &words));
        }
        if !m.text.trim().is_empty() {
            said += &format!(" {}", users("The user says, in workriver", &m.text));
        }
        return said;
    }
    let unit = (!m.unit.is_empty()).then(|| state.unit_or_intent(&m.unit)).flatten();
    let mut at: Vec<String> = match (m.task, &unit) {
        (0, Some(u)) => vec![format!("{} {} \"{}\"", if state.unit(&u.id).is_ok() { "unit" } else { "intent" }, u.id, quote::name(&u.title))],
        (n, _) => state.task(&m.unit, n).ok().map(|t| task_named(state, &m.unit, t)).into_iter().collect(),
    };
    at.extend(pointed(state, &m.unit, &m.context, home));
    let at = if at.is_empty() { String::new() } else { format!("It points at: {}. ", at.join("; ")) };
    let code = match (&unit, dir.map(quote::path)) {
        (Some(u), Some(dir)) if u.branch.is_empty() => format!("The files are in {dir}. "),
        (Some(u), Some(dir)) => format!("The files are in {dir} on branch {} (from {}). ", quote::name(&u.branch), quote::name(&u.base)),
        _ => String::new(),
    };
    let details = match (m.task, &unit) {
        (0, Some(u)) => format!("Details: `workriver board {}`. ", u.id),
        (n, Some(u)) => format!("{} ", details(&u.id, n)),
        _ => String::new(),
    };
    format!(
        "[workriver] The user writes to you from workriver's page (message {}{about}). Reply with `workriver reply {} --text \"...\"`; the user reads replies there, not in this terminal. Do not repeat the reply here (one line at most, e.g. which message you answered). Work that takes time goes into a task for a worker (`workriver task add ...`) and your reply carries its link (`workriver link <task>`); answer at once only what needs no work. {at}{code}{details}{}",
        m.id,
        m.id,
        users("The user says, in workriver", &m.text)
    )
}

/// A task named from its record, with its kind and agent: `task u1#3 "Paging" (implement, by w)`.
fn task_named(state: &State, unit: &str, t: &Task) -> String {
    let by = t.agent.as_deref().map(|a| format!(", by {}", who(state, a))).unwrap_or_default();
    format!("task {} ({}{by})", label(state, unit, t), name(&t.kind))
}

/// What a question or a message points at besides its own task: other tasks of the unit,
/// findings, files, lines and attachments. Tasks are named from their records by number, and
/// an attachment by the store's record (the rules let only what may be pointed at through),
/// given as its path in the store, which `App::send` turns into a command for another
/// machine; never by the page's label. A task chip with no number (an old question's own
/// task) and a unit's chip add nothing.
pub(in crate::server) fn pointed(state: &State, unit: &str, context: &[AskContext], home: &Path) -> Vec<String> {
    let stored = |id: &Option<String>| id.as_ref().and_then(|id| state.attachments.iter().find(|a| &a.id == id));
    context
        .iter()
        .filter_map(|c| {
            Some(match (c.kind.as_str(), &c.path, c.from, c.to) {
                ("task", ..) => task_named(state, unit, state.task(unit, c.task?).ok()?),
                ("unit", ..) => return None,
                ("lines", Some(p), Some(a), Some(b)) => format!("lines {a}-{b} of {}", quote::name(p)),
                ("file", Some(p), _, _) => format!("the file {}", quote::name(p)),
                ("finding", _, _, _) => format!("finding {}", quote::name(&c.label)),
                ("attachment", _, _, _) => match stored(&c.id) {
                    Some(a) => match crate::server::files::store::path(home, &a.id, &a.name) {
                        // Raw: made of the configured home, a hex id and an alphanumeric
                        // extension, so `App::send` still finds the store's prefix in it and
                        // turns it into a fetch command for an agent on another machine.
                        Some(p) => format!("the attachment \"{}\" at {}", quote::name(&a.name), p.display()),
                        None => format!("the attachment \"{}\"", quote::name(&a.name)),
                    },
                    None => format!("the attachment \"{}\"", quote::name(&c.label)),
                },
                _ => quote::name(&c.label),
            })
        })
        .collect()
}

/// The last of workriver's own words in a message about a task: where all of it is. Before
/// any quote, so that nobody's quoted text can pass for it.
pub(super) fn details(owner: &str, n: u32) -> String {
    format!("Details: `workriver task show {owner}#{n}`.")
}

/// The task `t` follows directly.
/// Who waits on a gate: the agent of an unfinished task that comes after it; else the worker
/// that asked for it after its own task (a deploy it needs, say). Not whoever did the task
/// the gate follows: a reviewer done long ago waits on nothing.
pub(super) fn waiting_on(state: &State, unit: &str, g: &Task) -> Option<String> {
    let n = g.n.to_string();
    let behind = state.tasks_of(unit)?.iter().filter(|t| !t.state.over() && t.inputs.iter().any(|i| i.from == n)).find_map(|t| t.agent.clone());
    behind.or_else(|| followed(state, unit, g).filter(|f| f.agent.as_deref() == Some(g.by.as_str())).map(|_| g.by.clone()))
}

fn followed<'a>(state: &'a State, unit: &str, t: &Task) -> Option<&'a Task> {
    let from = t.inputs.iter().find(|i| i.primary)?.from.parse().ok()?;
    state.task(unit, from).ok()
}

/// How a task reads in a message: `u1#2 "title"`, or `u1#4 (code review of u1#2 "title")`.
pub(in crate::server) fn label(state: &State, unit: &str, t: &Task) -> String {
    let of = || followed(state, unit, t).map(|f| format!(" of {}", label(state, unit, f))).unwrap_or_default();
    match t.kind {
        TaskKind::Review => format!("{unit}#{} ({}{})", t.n, t.reviewer.map(|r| name(&r).replace('-', " ")).unwrap_or_default(), of()),
        TaskKind::Tests => format!("{unit}#{} (tests{})", t.n, of()),
        _ => format!("{unit}#{} \"{}\"", t.n, quote::name(&t.title)),
    }
}

/// `was`: the user changes a decision already made (the manager's, or their own).
pub(super) fn decision_message(state: &State, unit: &str, n: u32, finding: &str, decision: Decision, note: &str, was: Option<workriver_core::FindingState>) -> Option<Msg> {
    let u = state.unit_or_intent(unit)?;
    let t = state.task(unit, n).ok()?;
    let f = t.findings.iter().find(|f| f.found.id == finding)?;
    let from = format!("from {} in {} {unit} \"{}\"", label(state, unit, t), if state.unit(unit).is_err() { "intent" } else { "unit" }, quote::name(&u.title));
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
    // Only a gate about a design; a question to merge or deploy is answered with work.
    let design = t.gate == Some(GateKind::Decide) && !state.designs_asked(unit, n).is_empty();
    m.own.push(match decision {
        // From a design decision: the design is done again, and confirmed again.
        Decision::Accept | Decision::Revise if design => format!(
            "Next: revise the design: `workriver task add {unit} --kind design --title \"<revised design>\" --cause {n}:{finding} --agent <agent>`, then, if the design sets the shape of the work, its architecture review (`--kind review --reviewer architecture-review`), and a new `--kind gate --gate decide` after that; the implementation hangs after the new gate, not after {unit}#{n} (`workriver guide how design-gate`)."
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

pub(super) fn gate_message(state: &State, unit: &str, n: u32) -> Option<Msg> {
    let t = state.task(unit, n).ok()?;
    // A choice gate answered: the option picked, which is no go by itself.
    if t.result == Some(GateResult::Answered) {
        let value = t.selected.first().cloned().unwrap_or_default();
        let mut m = Msg::default();
        m.own.push(format!("[workriver] The user answered gate {}: option `{}`.", label(state, unit, t), quote::name(&value)));
        m.own.push(format!(
            "Next: do what that option says, within the question (push, pull request and merge stay one task each); add its task with `--after {n}`. Nothing follows from the answer by itself."
        ));
        m.own.push(details(unit, n));
        if let Some(words) = t.choice.as_ref().and_then(|c| c.label(&value)) {
            m.quotes.push(quoted(&format!("The option, as {} wrote it", who(state, &t.by)), &words));
        }
        if !t.note.trim().is_empty() {
            m.quotes.push(users("The user wrote", &t.note));
        }
        return Some(m);
    }
    let problem = t.result == Some(GateResult::Problem);
    let decide = t.gate == Some(GateKind::Decide);
    // A decide gate asks about a design only when it comes after one (through reviews);
    // any other question (merge, deploy, the goal) is confirmed or not as what it is.
    let design = decide && !state.designs_asked(unit, n).is_empty();
    let mut m = Msg::default();
    m.own.push(match (decide, design, problem) {
        (_, true, false) => format!("[workriver] The user confirmed the design in gate {}.", label(state, unit, t)),
        (_, true, true) => format!("[workriver] The user did not confirm the design in gate {}: revise the design.", label(state, unit, t)),
        (true, false, false) => format!("[workriver] The user confirmed gate {}.", label(state, unit, t)),
        (true, false, true) => format!("[workriver] The user did not confirm gate {}.", label(state, unit, t)),
        (false, _, p) => format!("[workriver] The user finished gate {}: {}.", label(state, unit, t), if p { "found a problem" } else { "done" }),
    });
    if design && !problem {
        m.own.push(format!(
            "Next: hand out the implementation accordingly, after this gate: `workriver task add {unit} --title \"<outcome>\" --after {n} --agent <agent>`, citing the user's decision in the task's text."
        ));
    } else if decide && !problem {
        m.own.push("Next: do what it asked about, as the user decided.".into());
    } else if problem {
        let ids: Vec<String> = t.findings.iter().map(|f| format!("{} ({})", f.found.id, name(&f.found.severity))).collect();
        m.own.push(format!("Findings: {}.", ids.join(", ")));
        m.own.push(if design {
            format!(
                "The user decides on each finding; for each accepted or revised one, a revised design task (`--kind design --cause {n}:<id>`), its architecture review if the design sets the shape of the work, and a new decide gate after that; the implementation hangs after the new gate, not after {unit}#{n}."
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
