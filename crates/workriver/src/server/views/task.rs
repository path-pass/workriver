//! `workriver task show` and `workriver finding show`: one task, or one finding, with
//! everything its reader may see, in full. An agent reads this on purpose, so it is data,
//! not a message: nothing is quoted in «…», but every text says whose it is.

use workriver_core::{self as core, FindingRef, State, Task, Who, name};

use crate::common::wire::*;

/// Who someone is, as the reader knows them.
fn who(state: &State, s: &str) -> String {
    if s == "user" { "the user".into() } else { state.name_of(s).unwrap_or(s).to_string() }
}

/// `unit`'s task `n` as `state` (already scoped to the reader) shows it. `ws`: the snapshot
/// of the same state, for commits. `file`: what an attachment id is to the reader (a path
/// on this machine, a link on another).
pub(in crate::server) fn task(state: &State, ws: &serde_json::Value, unit: &str, n: u32, file: &dyn Fn(&str) -> Option<String>) -> Result<TaskShow, core::Rejected> {
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
    // What was designed and found before it. Which design is current and which gate asks
    // about it are the model's rules (`State::revised_by`, `State::design_gate`); here only
    // what is shown. A blind reader's state holds no other task or unit, so it gets none.
    let said = |owner: &str, b: &Task| -> Option<EarlierShow> {
        if b.done.is_none() || b.dropped.is_some() {
            return None;
        }
        let design = b.kind == core::TaskKind::Design;
        let revisions = if design { state.revised_by(owner, b.n) } else { vec![] };
        let revises = revisions.iter().any(|(o, r)| o == unit && r.n == n);
        // A delivered revision takes its place (the newest; see `State::design_in_place`),
        // except for the task that revises it.
        let (owner, b, replaced) = match state.design_in_place(owner, b.n).filter(|_| design && !revises) {
            Some((o, k)) => (o.clone(), state.task(&o, k).ok().filter(|r| !(o == unit && r.n == n))?, Some(format!("{owner}#{}", b.n))),
            None => (owner.to_string(), b, None),
        };
        let owner = owner.as_str();
        let revisions = if replaced.is_some() { state.revised_by(owner, b.n) } else { revisions };
        let gate = design.then(|| state.design_gate(owner, b.n)).flatten();
        Some(EarlierShow {
            from: format!("{owner}#{}", b.n),
            kind: name(&b.kind),
            title: b.title.clone(),
            delivered_by: reporter(b),
            note: b.note.clone(),
            files: files(&b.attachments),
            gate: gate.map(|g| format!("{owner}#{}", g.n)),
            confirmed: gate.and_then(|g| g.result).map(|r| r == core::GateResult::Done),
            under_review: design && t.kind == core::TaskKind::Review && owner == unit && t.inputs.iter().any(|i| i.from == b.n.to_string()),
            revises,
            revised_in: revisions.iter().find(|(o, r)| !(o == unit && r.n == n)).filter(|_| !revises).map(|(o, r)| format!("{o}#{}", r.n)),
            in_place_of: replaced,
        })
    };
    // First what was designed for the intent as a whole, in its research units (no input
    // reaches those from another unit), then the design and research tasks it comes after.
    let wide = state.units.iter().filter(|x| x.kind == core::UnitKind::Research && x.id != unit && x.intent == intent_id);
    let earlier = wide
        .flat_map(|x| x.tasks.iter().filter(|b| b.kind == core::TaskKind::Design).filter_map(|b| said(&x.id, b)))
        .chain(state.before(unit, n).iter().filter_map(|k| state.task(unit, *k).ok()).filter(|b| matches!(b.kind, core::TaskKind::Design | core::TaskKind::Research)).filter_map(|b| said(unit, b)))
        // A revision stands once, also where the reader comes after it and after what it revises.
        .fold(Vec::<EarlierShow>::new(), |mut all, e| {
            match all.iter_mut().find(|x| x.from == e.from) {
                Some(x) => x.in_place_of = x.in_place_of.take().or(e.in_place_of),
                None => all.push(e),
            }
            all
        });
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
        link: String::new(),
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
        notes: t.notes.iter().map(|x| NoteShow { by: who(state, &x.by), at: x.at.clone(), text: x.text.clone(), files: files(&x.attachments) }).collect(),
        no_longer_stored: vec![],
        after: t.inputs.iter().map(|i| if i.from == "start" { "start".into() } else { format!("{unit}#{}", i.from) }).collect(),
        causes,
        earlier,
        plans: t.plans.clone(),
        steps: t.steps.clone(),
        choice: t.choice.clone(),
        output: done.then(|| OutputShow {
            at: t.done.clone(),
            delivered_by: reporter(t),
            note: t.note.clone(),
            evidence: t.evidence.iter().map(|e| EvidenceShow { command: e.command.clone(), result: name(&e.result), summary: e.summary.clone(), file: e.attachment.as_deref().and_then(file) }).collect(),
            findings: t.findings.iter().map(|f| finding(f, &reporter(t))).collect(),
            files: files(&t.attachments),
            result: t.result.map(|r| name(&r)),
            selected: t.selected.clone(),
        }),
        commits,
        lost_commits: ids(mine.map(|m| &m["lostCommits"])),
        questions: state
            .questions
            .iter()
            .filter(|q| q.unit == unit && q.task == n)
            .map(|q| QuestionShow { id: q.id.clone(), question: q.text.clone(), asked_by: who(state, &q.by), at: q.at.clone(), answer: q.answer.clone(), answered_by: q.answered_by.as_deref().map(|a| who(state, a)) })
            .collect(),
        dropped: t.dropped.clone().map(|d| core::Closed { by: who(state, &d.by), ..d }),
        stalled: mine.and_then(|m| serde_json::from_value(m["stalled"].clone()).ok()),
        trace: None,
    })
}
