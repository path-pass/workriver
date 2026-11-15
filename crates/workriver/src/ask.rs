// The one prompt a temporary agent gets to answer the user's question about a task: who it
// is, what it may do, the records of the task, and how to answer. Pure.
use workriver_core::{Question, State, TaskKind, name};

use crate::herdr;

/// How long a temporary agent has to answer.
pub const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);
/// ponytail: a thread's temporary agent is closed after this long without a new question;
/// a later question starts a new one with the thread so far. Tune when it bites.
pub const IDLE: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// What a temporary agent is asked to do with a question, and how to record it.
fn how_to_answer(q: &Question) -> String {
    if q.conclude {
        format!(
            "The user asks for the conclusion of this discussion ({}): in a few sentences, what was decided, what changes, and what stays open. Record it with `workriver answer {} --conclusion --text \"...\"`.",
            q.id, q.id
        )
    } else {
        format!(
            "Answer briefly, in the language of the question, naming file paths where they help: `workriver answer {} --text \"...\"`.",
            q.id
        )
    }
}

/// A follow-up in a thread, to the agent that answered the earlier questions.
pub fn follow_up(q: &Question) -> String {
    let ask = if q.conclude { String::new() } else { format!("Follow-up question {}: \"{}\" ", q.id, q.text) };
    let about: Vec<String> = pointed(q);
    let at = if about.is_empty() { String::new() } else { format!("The user points at: {}. ", about.join("; ")) };
    format!("[workriver] {ask}{at}{} Then wait; more may come.", how_to_answer(q))
}

/// The findings, files and lines a question points at.
fn pointed(q: &Question) -> Vec<String> {
    q.context
        .iter()
        .filter(|c| c.kind != "task")
        .map(|c| match (c.kind.as_str(), &c.path, c.from, c.to) {
            ("lines", Some(p), Some(a), Some(b)) => format!("lines {a}-{b} of {p}"),
            ("file", Some(p), _, _) => format!("the file {p}"),
            ("finding", _, _, _) => format!("finding {}", c.label),
            _ => c.label.clone(),
        })
        .collect()
}

/// The temporary agent's name for a question.
pub fn agent_name(q: &str) -> String {
    format!("why-{q}")
}

/// One paragraph: a newline would submit early in a terminal without bracketed paste.
/// `dir` is where the unit's branch is checked out (or the repository); `commits` are the
/// task's commits as (short id, subject).
pub fn prompt(state: &State, q: &Question, dir: &str, commits: &[(String, String)]) -> Option<String> {
    let u = state.unit(&q.unit).ok()?;
    let t = state.task(&q.unit, q.task).ok()?;
    let r = format!("{}#{}", q.unit, q.task);
    let named = |s: &str| state.name_of(s).unwrap_or(s).to_string();
    let mut m = vec![
        format!(
            "[workriver] You are a temporary agent started by workriver to answer questions about recorded work (from the user, or from another agent); more questions may follow in this terminal, and workriver closes you when the user is done. Read-only: do not edit files, do not commit, and run no workriver command except `workriver board {}` and `workriver answer`.",
            q.unit
        ),
        match (q.conclude, q.by.as_str()) {
            (true, _) => String::new(),
            (false, "user") => format!("Question {}: \"{}\"", q.id, q.text),
            (false, by) => format!("Question {} from the agent {}: \"{}\"", q.id, named(by), q.text),
        },
        format!(
            "It is about task {r} \"{}\" ({}{}, {}) in unit {} \"{}\"{}. The code is in {dir} on branch {} (from {}); read it there.",
            t.title,
            name(&t.kind),
            t.agent.as_deref().map(|a| format!(", by {}", named(a))).unwrap_or_default(),
            name(&t.state),
            q.unit,
            u.title,
            if u.purpose.is_empty() { String::new() } else { format!(" ({})", u.purpose) },
            u.branch,
            u.base,
        ),
    ];
    if !t.text.is_empty() && t.text != t.title {
        m.push(format!("The task as handed out: \"{}\".", t.text));
    }
    for c in &t.causes {
        for id in &c.findings {
            if let Some(f) = state.task(&q.unit, c.task).ok().and_then(|x| x.findings.iter().find(|f| &f.found.id == id)) {
                m.push(format!("It answers finding {id} of {}#{}: {} If left: {} Where: {}.", q.unit, c.task, f.found.problem, f.found.impact, f.found.r#where.join(", ")));
            }
        }
    }
    if !t.note.is_empty() {
        m.push(format!("Its delivery note: \"{}\".", t.note));
    }
    for e in &t.evidence {
        m.push(format!("Check `{}`: {} ({}).", e.command, name(&e.result), e.summary));
    }
    for f in &t.findings {
        m.push(format!("Finding {} ({}, {}): {} If left: {} Where: {}.", f.found.id, name(&f.found.severity), name(&f.state), f.found.problem, f.found.impact, f.found.r#where.join(", ")));
    }
    if !commits.is_empty() {
        let list: Vec<String> = commits.iter().map(|(id, s)| format!("{id} \"{s}\"")).collect();
        m.push(format!("Its commits (see them with `git show <id>` in {dir}): {}.", list.join(", ")));
    } else if t.kind == TaskKind::Implement {
        m.push("It has no commits naming it.".into());
    }
    let about = pointed(q);
    if !about.is_empty() {
        m.push(format!("The user points at: {}.", about.join("; ")));
    }
    // The thread so far, when an earlier agent of it is gone.
    for earlier in state.questions.iter().filter(|x| x.thread == q.thread && x.id != q.id && !x.conclude) {
        if let Some(a) = &earlier.answer {
            m.push(format!("Asked before in this thread: \"{}\" Answered: \"{}\"", earlier.text, a));
        }
    }
    if let Some(a) = &t.agent {
        m.push(herdr::ask_hint(&named(a)));
    }
    m.push(format!("{} Then wait; more may come.", how_to_answer(q)));
    Some(m.into_iter().filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" "))
}

/// An agent's question, straight to the agent of the task it is about (which is free).
pub fn direct(state: &State, q: &Question) -> Option<String> {
    let t = state.task(&q.unit, q.task).ok()?;
    let asker = state.name_of(&q.by).unwrap_or(&q.by);
    let about = pointed(q);
    let at = if about.is_empty() { String::new() } else { format!(" It points at: {}.", about.join("; ")) };
    Some(format!(
        "[workriver] The agent {asker} asks about your task {}#{} \"{}\" (question {}): \"{}\"{at} Answer briefly with `workriver answer {} --text \"...\"`, then go on with what you were doing. If the question means the agreed contract or plan must change, do not agree a change between you: say so in the answer; the manager decides.",
        q.unit, q.task, t.title, q.id, q.text, q.id
    ))
}

/// The answer, back to the agent that asked.
pub fn answer_back(state: &State, q: &Question) -> String {
    let who = q.answered_by.as_deref().map(|a| state.name_of(a).unwrap_or(a).to_string()).unwrap_or_default();
    format!(
        "[workriver] Answer to your question {} about {}#{} (from {who}): \"{}\"",
        q.id,
        q.unit,
        q.task,
        q.answer.as_deref().unwrap_or_default()
    )
}
