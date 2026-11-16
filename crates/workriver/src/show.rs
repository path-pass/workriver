//! `workriver task show` and `workriver finding show`: one task, or one finding, with
//! everything its reader may see, in full. An agent reads this on purpose, so it is data,
//! not a message: nothing is quoted in «…», but every text says whose it is.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use workriver_core::{self as core, FindingRef, State, Task, Who, name};

/// One task as `workriver task show --json` prints it. Field names are stable: agents and
/// scripts read them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskShow {
    #[serde(rename = "ref")]
    pub reference: String,
    pub kind: String,
    pub state: String,
    pub title: String,
    pub intent: Part,
    /// None for work of the intent itself (research, design across units).
    pub unit: Option<Part>,
    /// Where the work is, as it is on the machine it is on.
    pub place: Option<Place>,
    /// The agent it is assigned to, by name.
    pub agent: Option<String>,
    pub blind: bool,
    /// What it asks, in full; and who wrote it ("the user", or an agent's name).
    pub text: String,
    pub written_by: String,
    /// The tasks it comes after (`start`: it can begin at once).
    pub after: Vec<String>,
    pub causes: Vec<CauseShow>,
    pub plans: Vec<String>,
    pub steps: Vec<core::Step>,
    /// What it delivered, once it did.
    pub output: Option<OutputShow>,
    pub commits: Vec<CommitShow>,
    /// Its commits that were only on a branch the unit has left.
    pub lost_commits: Vec<String>,
    pub questions: Vec<QuestionShow>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Part {
    pub id: String,
    pub title: String,
    /// The intent's goal, or the unit's purpose.
    pub about: String,
    /// The intent's note (more about it; a merge appends the merged intents' text).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// A unit's: working, waiting, ready, merged or done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Place {
    /// None: the machine workriver runs on.
    pub machine: Option<String>,
    /// That machine is disconnected: since when (what is shown is as last read).
    #[serde(rename = "disconnectedSince", default, skip_serializing_if = "Option::is_none")]
    pub disconnected_since: Option<String>,
    pub repo: String,
    pub branch: String,
    pub base: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CauseShow {
    /// The task that found it (`u1#4`), or `backlog`.
    pub from: String,
    pub finding: FindingShow,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FindingShow {
    pub id: String,
    pub severity: String,
    pub problem: String,
    pub impact: String,
    #[serde(rename = "where")]
    pub places: Vec<String>,
    /// What the reporter suggested deciding, and why.
    pub suggestion: Option<(String, String)>,
    pub reported_by: String,
    pub state: String,
    pub decisions: Vec<DecisionShow>,
    pub notes: Vec<NoteShow>,
    pub files: Vec<String>,
    /// Dropped: why.
    pub reason: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DecisionShow {
    pub state: String,
    pub by: String,
    pub note: String,
    pub at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NoteShow {
    pub by: String,
    pub at: String,
    pub text: String,
    pub files: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutputShow {
    pub at: Option<String>,
    pub delivered_by: String,
    pub note: String,
    pub evidence: Vec<EvidenceShow>,
    pub findings: Vec<FindingShow>,
    pub files: Vec<String>,
    /// A gate's: done, or problem.
    pub result: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EvidenceShow {
    pub command: String,
    pub result: String,
    pub summary: String,
    pub file: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CommitShow {
    pub hash: String,
    pub subject: String,
    /// Linked to the task by hand (`workriver commit link`), not by its trailer.
    pub linked: bool,
    /// The same change as this commit of a branch the unit left: its old hash.
    #[serde(rename = "rebasedFrom", default, skip_serializing_if = "Option::is_none")]
    pub rebased_from: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuestionShow {
    pub id: String,
    pub question: String,
    pub asked_by: String,
    pub at: String,
    pub answer: Option<String>,
    pub answered_by: Option<String>,
}

/// Who someone is, as the reader knows them.
fn who(state: &State, s: &str) -> String {
    if s == "user" { "the user".into() } else { state.name_of(s).unwrap_or(s).to_string() }
}

/// `unit`'s task `n` as `state` (already scoped to the reader) shows it. `ws`: the snapshot
/// of the same state, for commits. `file`: what an attachment id is to the reader (a path
/// on this machine, a link on another).
pub fn task(state: &State, ws: &serde_json::Value, unit: &str, n: u32, file: &dyn Fn(&str) -> Option<String>) -> Result<TaskShow, core::Rejected> {
    let t = state.task(unit, n)?;
    let files = |ids: &[String]| ids.iter().filter_map(|id| file(id)).collect::<Vec<_>>();
    let finding = |f: &core::Finding, by: &str| FindingShow {
        id: f.found.id.clone(),
        severity: name(&f.found.severity),
        problem: f.found.problem.clone(),
        impact: f.found.impact.clone(),
        places: f.found.r#where.clone(),
        suggestion: f.found.suggestion.as_ref().map(|s| (name(&s.decision), s.reason.clone())),
        reported_by: by.to_string(),
        state: name(&f.state),
        decisions: f
            .decisions
            .iter()
            .map(|d| DecisionShow {
                state: name(&d.state),
                by: match (d.by, &d.agent) {
                    (Who::User, _) => "the user".into(),
                    (Who::Agent, Some(a)) => format!("the manager ({})", who(state, a)),
                    (Who::Agent, None) => "the manager".into(),
                },
                note: d.note.clone(),
                at: d.at.clone(),
            })
            .collect(),
        notes: f.notes.iter().map(|x| NoteShow { by: who(state, &x.by), at: x.at.clone(), text: x.text.clone(), files: files(&x.attachments) }).collect(),
        files: files(&f.found.attachments),
        reason: f.reason.clone(),
    };
    let reporter = |t: &Task| t.agent.as_deref().map(|a| who(state, a)).unwrap_or_else(|| "the user".into());
    let intent_id = state.intent_of(unit).unwrap_or(unit).to_string();
    let intent = state.intents.iter().find(|i| i.id == intent_id);
    let u = state.units.iter().find(|x| x.id == unit);
    let causes = t
        .causes
        .iter()
        .flat_map(|c| {
            let at = c.unit.clone().unwrap_or_else(|| unit.to_string());
            c.findings.iter().filter_map(move |id| {
                let r = FindingRef { unit: if c.task == 0 { String::new() } else { at.clone() }, task: c.task, finding: id.clone() };
                let f = state.finding(&r).ok()?;
                let (from, by) = if c.task == 0 {
                    ("backlog".to_string(), state.backlog.iter().find(|b| &b.finding.found.id == id).map(|b| who(state, &b.by)).unwrap_or_default())
                } else {
                    (format!("{at}#{}", c.task), state.task(&at, c.task).map(reporter).unwrap_or_default())
                };
                Some(CauseShow { from, finding: finding(f, &by) })
            })
        })
        .collect();
    // Commits come from git, through the snapshot the page reads too.
    let shown = ws["intents"].as_array().into_iter().flatten().flat_map(|i| i["units"].as_array().into_iter().flatten()).find(|x| x["id"] == unit);
    let mine = shown.and_then(|x| x["tasks"].as_array()?.iter().find(|y| y["taskId"].as_str() == Some(&n.to_string()) || y["id"].as_str() == Some(&n.to_string())));
    let ids = |v: Option<&serde_json::Value>| v.and_then(|v| v.as_array()).into_iter().flatten().filter_map(|c| c.as_str().map(str::to_string)).collect::<Vec<_>>();
    let commits = ids(mine.map(|m| &m["output"]["commits"]))
        .into_iter()
        .map(|id| {
            let c = shown.and_then(|x| x["commits"].as_array()?.iter().find(|c| c["id"] == id.as_str()));
            CommitShow {
                subject: c.and_then(|c| c["subject"].as_str()).unwrap_or_default().to_string(),
                linked: c.is_some_and(|c| c["linked"] == true),
                rebased_from: c.and_then(|c| c["rebasedFrom"].as_str()).map(str::to_string),
                hash: id,
            }
        })
        .collect();
    let done = t.state == core::TaskState::Done || t.done.is_some();
    Ok(TaskShow {
        reference: format!("{unit}#{n}"),
        kind: name(&t.kind),
        state: name(&t.state),
        title: t.title.clone(),
        intent: Part { id: intent_id.clone(), title: intent.map(|i| i.title.clone()).unwrap_or_default(), about: intent.map(|i| i.goal.clone()).unwrap_or_default(), note: intent.map(|i| i.note.trim().to_string()).unwrap_or_default(), progress: None },
        unit: u.map(|u| Part {
            id: u.id.clone(),
            title: u.title.clone(),
            about: u.purpose.clone(),
            note: String::new(),
            progress: Some(shown.and_then(|x| x["progress"].as_str()).map(str::to_string).unwrap_or_else(|| name(&state.progress(u)))),
        }),
        place: u.filter(|u| !u.branch.is_empty()).map(|u| Place {
            machine: u.machine.clone(),
            disconnected_since: u.machine.as_deref().and_then(|m| {
                let x = ws["machines"].as_array()?.iter().find(|x| x["label"] == m)?;
                (x["tunnel"] != true).then(|| x["downSince"].as_str().unwrap_or("now").to_string())
            }),
            repo: u.repo.clone(),
            branch: u.branch.clone(),
            base: u.base.clone(),
        }),
        agent: t.agent.as_deref().map(|a| who(state, a)),
        blind: t.blind,
        text: t.text.clone(),
        written_by: who(state, &t.by),
        after: t.inputs.iter().map(|i| if i.from == "start" { "start".into() } else { format!("{unit}#{}", i.from) }).collect(),
        causes,
        plans: t.plans.clone(),
        steps: t.steps.clone(),
        output: done.then(|| OutputShow {
            at: t.done.clone(),
            delivered_by: reporter(t),
            note: t.note.clone(),
            evidence: t.evidence.iter().map(|e| EvidenceShow { command: e.command.clone(), result: name(&e.result), summary: e.summary.clone(), file: e.attachment.as_deref().and_then(file) }).collect(),
            findings: t.findings.iter().map(|f| finding(f, &reporter(t))).collect(),
            files: files(&t.attachments),
            result: t.result.map(|r| name(&r)),
        }),
        commits,
        lost_commits: ids(mine.map(|m| &m["lostCommits"])),
        questions: state
            .questions
            .iter()
            .filter(|q| q.unit == unit && q.task == n)
            .map(|q| QuestionShow { id: q.id.clone(), question: q.text.clone(), asked_by: who(state, &q.by), at: q.at.clone(), answer: q.answer.clone(), answered_by: q.answered_by.as_deref().map(|a| who(state, a)) })
            .collect(),
    })
}

/// Text written by someone, kept as written: after its label when it is one line, else
/// each line indented under it.
fn block(out: &mut String, label: &str, text: &str) {
    if !text.contains('\n') {
        let _ = writeln!(out, "{label}: {text}");
        return;
    }
    let _ = writeln!(out, "{label}:");
    for line in text.lines() {
        let _ = writeln!(out, "    {line}");
    }
}

/// One finding in full, indented by `pad`.
pub fn finding_text(f: &FindingShow, pad: &str) -> String {
    let mut out = format!("{pad}{} ({}, {}), reported by {}\n", f.id, f.severity, f.state, f.reported_by);
    let mut part = |label: &str, text: &str| {
        if !text.is_empty() {
            let mut b = String::new();
            block(&mut b, label, text);
            out += &b.lines().map(|l| format!("{pad}  {l}\n")).collect::<String>();
        }
    };
    part("Problem", &f.problem);
    part("Impact", &f.impact);
    part("Where", &f.places.join("\n"));
    if let Some((decision, reason)) = &f.suggestion {
        part(&format!("Suggested by the reporter: {decision}"), reason);
    }
    part("Files", &f.files.join("\n"));
    for d in &f.decisions {
        part(&format!("{} {} by {}", d.at, d.state, d.by), if d.note.is_empty() { "(no note)" } else { &d.note });
    }
    if let Some(r) = &f.reason {
        part("Dropped by the user, because", r);
    }
    for x in &f.notes {
        let files = if x.files.is_empty() { String::new() } else { format!("\nFiles: {}", x.files.join(", ")) };
        part(&format!("{} added by {}", x.at, x.by), &format!("{}{files}", x.text));
    }
    out
}

/// `workriver task show`, as text.
pub fn text(s: &TaskShow) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}  {}  {}{}", s.reference, s.kind, s.state, if s.blind { "  [blind]" } else { "" });
    if !s.title.is_empty() {
        let _ = writeln!(out, "Title: {}", s.title);
    }
    let _ = writeln!(out, "Intent {} \"{}\": {}", s.intent.id, s.intent.title, s.intent.about);
    if !s.intent.note.is_empty() {
        block(&mut out, "Intent note", &s.intent.note);
    }
    if let Some(u) = &s.unit {
        let about = if u.about.is_empty() { String::new() } else { format!(": {}", u.about) };
        let _ = writeln!(out, "Unit {} \"{}\" ({}){about}", u.id, u.title, u.progress.as_deref().unwrap_or("?"));
    }
    if let Some(p) = &s.place {
        let on = p.machine.as_deref().map(|m| format!("on machine {m}")).unwrap_or_else(|| "on the machine workriver runs on".into());
        let _ = writeln!(out, "Where: {on}: {}, branch {} (from {})", p.repo, p.branch, p.base);
        if let Some(since) = &p.disconnected_since {
            let _ = writeln!(out, "  (that machine is disconnected since {since}: commits as last read)");
        }
    }
    let _ = writeln!(out, "Agent: {}", s.agent.as_deref().unwrap_or("(none yet)"));
    if !s.text.is_empty() && s.text != s.title {
        block(&mut out, &format!("What it asks (written by {})", s.written_by), &s.text);
    }
    let _ = writeln!(out, "After: {}", s.after.join(", "));
    if !s.causes.is_empty() {
        let _ = writeln!(out, "Causes (it must answer these findings):");
        for c in &s.causes {
            let _ = writeln!(out, "  from {}:", c.from);
            out += &finding_text(&c.finding, "    ");
        }
    }
    if !s.plans.is_empty() {
        let _ = writeln!(out, "Plans to change: {}", s.plans.join(", "));
    }
    if !s.steps.is_empty() {
        let _ = writeln!(out, "Steps:");
        for (i, st) in s.steps.iter().enumerate() {
            let _ = writeln!(out, "  {}. {}{}", i + 1, st.text, st.command.as_deref().map(|c| format!("\n     $ {c}")).unwrap_or_default());
        }
    }
    match &s.output {
        None => {
            let _ = writeln!(out, "Output: not delivered yet");
        }
        Some(o) => {
            let _ = writeln!(out, "Output (delivered {} by {}){}:", o.at.as_deref().unwrap_or("?"), o.delivered_by, o.result.as_deref().map(|r| format!(", result: {r}")).unwrap_or_default());
            if !o.note.is_empty() {
                let mut b = String::new();
                block(&mut b, &format!("Note (written by {})", o.delivered_by), &o.note);
                out += &b.lines().map(|l| format!("  {l}\n")).collect::<String>();
            }
            for e in &o.evidence {
                let _ = writeln!(out, "  Evidence: {} => {}: {}{}", e.command, e.result, e.summary, e.file.as_deref().map(|f| format!(" => {f}")).unwrap_or_default());
            }
            for f in &o.findings {
                out += &finding_text(f, "  ");
            }
            if !o.files.is_empty() {
                let _ = writeln!(out, "  Files: {}", o.files.join(", "));
            }
        }
    }
    if !s.commits.is_empty() {
        let _ = writeln!(out, "Commits:");
        for c in &s.commits {
            let rebased = c.rebased_from.as_deref().map(|o| format!("  (rebased from {o})")).unwrap_or_default();
            let _ = writeln!(out, "  {} {}{}{rebased}", c.hash, c.subject, if c.linked { "  (linked by hand)" } else { "" });
        }
    }
    if !s.lost_commits.is_empty() {
        let _ = writeln!(out, "Commits not on the current branch (on a branch the unit left): {}", s.lost_commits.join(", "));
    }
    if !s.questions.is_empty() {
        let _ = writeln!(out, "Questions:");
        for q in &s.questions {
            let mut b = String::new();
            block(&mut b, &format!("{} asked by {} ({})", q.id, q.asked_by, q.at), &q.question);
            match (&q.answer, &q.answered_by) {
                (Some(a), by) => block(&mut b, &format!("answered by {}", by.as_deref().unwrap_or("?")), a),
                (None, _) => b += "    (not answered yet)\n",
            }
            out += &b.lines().map(|l| format!("  {l}\n")).collect::<String>();
        }
    }
    out
}
