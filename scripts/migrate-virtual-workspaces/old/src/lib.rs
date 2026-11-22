//! The first half of the one-off migration to virtual workspaces (see ../../README.md):
//!
//!   old plan  <copy.db> <mapping>        the mapping the user reads and fills in
//!   old apply <copy.db> <mapping> <dir>  events.jsonl, seats.jsonl, old-state.json
//!
//! The copy is opened read-only; every output is a new file or folder.
use std::path::Path;

use workriver_core::{IntentState, State};

pub mod analyse;
pub mod emit;
pub mod mapping;
pub mod read;

use analyse::{Analysis, Place, analyse};
use emit::{Resolver, describe};
use mapping::{Key, Mapping, quoted, word};

pub fn plan(copy: &Path, map: &Path) -> Result<String, String> {
    read::new_path(map)?;
    let a = analyse(read::events(copy)?);
    let text = proposal(&a, copy)?;
    std::fs::write(map, &text).map_err(|e| format!("{}: {e}", map.display()))?;
    Ok(text)
}

pub fn apply(copy: &Path, map: &Path, dir: &Path) -> Result<String, String> {
    read::new_path(dir)?;
    let a = analyse(read::events(copy)?);
    let m = mapping::parse(&std::fs::read_to_string(map).map_err(|e| format!("{}: {e}", map.display()))?)?;
    if !m.open.is_empty() {
        return Err(format!("the mapping still has lines to fill in (replace each ?):\n{}", m.open.join("\n")));
    }
    let r = Resolver::new(&a, &m)?;
    let out = emit::emit(&r).map_err(|e| format!("the mapping does not settle everything; nothing was written:\n{e}"))?;
    let mut old = serde_json::to_value(&a.state).unwrap();
    old.as_object_mut().unwrap().insert("expected".into(), out.expected);
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let write = |name: &str, text: String| std::fs::write(dir.join(name), text).map_err(|e| format!("{name}: {e}"));
    let lines = |v: &[String]| v.iter().map(|l| format!("{l}\n")).collect::<String>();
    write("events.jsonl", lines(&out.events))?;
    write("seats.jsonl", lines(&out.seats))?;
    write("old-state.json", serde_json::to_string_pretty(&old).unwrap() + "\n")?;
    Ok(out.report.iter().map(|l| format!("{l}\n")).collect())
}

/// Who wrote an event, as the plan shows it.
fn who(s: &State, actor: &str) -> String {
    match actor {
        "user" => "the user".into(),
        "workriver" => "workriver".into(),
        a => format!("agent {} ({})", quoted(s.name_of(a).unwrap_or("?")), quoted(a)),
    }
}

/// The mapping `plan` proposes: every decision, the ones it cannot make as `?`. Every text
/// from the store in it is a JSON string (`quoted`): a label or a name any program may have
/// given never makes a line of its own. That a line is one line is checked at the end.
fn proposal(a: &Analysis, copy: &Path) -> Result<String, String> {
    let s = &a.state;
    let r = Resolver::new(a, &Mapping::default()).unwrap();
    let mut out = vec![
        format!("# The mapping of {} to virtual workspaces, proposed by `old plan`.", quoted(&copy.display().to_string())),
        "# Fill in each ?; change a name or open/closed where you want it otherwise. Then".into(),
        "# `old apply` with this file (it says what is still missing, if anything).".into(),
        "# A project is named by its id below or by its folder. Lines: see old/src/mapping.rs.".into(),
        "#".into(),
        "# Registered projects:".into(),
    ];
    out.extend(s.projects.iter().map(|p| format!("#   {} {} {}", word(&p.id), word(&p.path), quoted(&p.title))));
    if !a.folders.is_empty() {
        out.push(String::new());
        out.push("# Folders that no registered project holds, as intents or `workspace project` named them.".into());
        out.push("# register: you add it as a project now (the script records it as yours, before the".into());
        out.push("# first event that names it); into <project>: its intents are that project's; none:".into());
        out.push("# its intents are of no project (a planned one goes to the roadmap of \"No project\").".into());
        for (f, names) in &a.folders {
            for n in names {
                let o = &a.events[n.event].event;
                out.push(format!("#   event {} {} at {} by {}", o.seq, a.events[n.event].kind, o.at, who(s, &o.actor)));
            }
            out.push(format!("folder {} = ?", quoted(f)));
        }
    }
    out.push(String::new());
    out.push("# Workspaces: one per old workspace record and project (a record whose intents are of".into());
    out.push("# several projects is split). Open when an intent is active in it, else closed.".into());
    let mut nothing = vec![];
    let mut settle = vec![];
    for (n, rec) in a.records.iter().enumerate() {
        if rec.intents.is_empty() && !rec.talked {
            nothing.push(n);
            continue;
        }
        out.push(format!("# record {n}: {}; labels {}; intents {}; messages {}", describe(s, n), rec.labels.iter().map(|l| quoted(l)).collect::<Vec<_>>().join(", "), list(&rec.intents), list(&a.messages.iter().filter(|(_, p)| **p == Place::Record(n)).map(|(m, _)| m.clone()).collect::<Vec<_>>())));
        let name = quoted(&r.name(&Key::Record(n)));
        match r.projects(&Key::Record(n)) {
            Ok(ps) if ps.is_empty() => {
                let units: Vec<String> = rec.intents.iter().flat_map(|i| s.units.iter().filter(move |u| &u.intent == i).map(|u| quoted(&u.repo))).collect();
                out.push(format!("#   nothing tells its project (its units' repositories: {})", list(&units)));
                out.push(format!("record {n} ? = name {name}, {}", if active(s, a, n) { "open" } else { "closed" }));
            }
            Ok(ps) => {
                for p in &ps {
                    let key = (Key::Record(n), p.clone());
                    let open = s.intents.iter().any(|i| i.state == IntentState::Active && rec.intents.contains(&i.id) && r.ws_for(&Place::Record(n), &i.id, "").ok().flatten() == Some(key.clone()) && record_now(s, &i.id) == Some(n));
                    out.push(format!("record {n} {} = name {name}, {}", word(p), if open { "open" } else { "closed" }));
                }
                if ps.len() > 1 {
                    for (m, p) in &a.messages {
                        let intent = s.messages.iter().find(|x| &x.id == m).map(|x| x.intent.clone()).unwrap_or_default();
                        if *p == Place::Record(n) && r.ws_for(p, &intent, "").is_err() {
                            settle.push(format!("message {m} = ?    # record {n}, names no intent"));
                        }
                    }
                    for (step, o) in a.steps.iter().zip(&a.events) {
                        if let analyse::Step::Sent { copies, .. } = step
                            && copies.iter().any(|(_, p)| *p == Place::Record(n))
                        {
                            let id = serde_json::from_str::<serde_json::Value>(&o.body).unwrap()["id"].as_str().unwrap_or_default().to_string();
                            settle.push(format!("message {id} = ?    # its copy to record {n}, to every manager"));
                        }
                    }
                    for i in &rec.intents {
                        if r.intent_project(i).is_ok_and(|p| p.is_empty()) {
                            settle.push(format!("intent {i} = ?    # record {n}, of no project"));
                        }
                    }
                }
            }
            Err(x) => out.push(format!("#   its project waits on a folder line: {x}")),
        }
    }
    for (l, ms) in &a.orphans {
        out.push(format!("# label {}: no record carried it; messages {}. A closed workspace for them.", quoted(l), list(ms)));
        match r.projects(&Key::Label(l.clone())) {
            Ok(ps) if !ps.is_empty() => out.extend(ps.iter().map(|p| format!("label {} {} = name {}, closed", quoted(l), word(p), quoted(l)))),
            _ => out.push(format!("label {} ? = name {}, closed", quoted(l), quoted(l))),
        }
    }
    if !settle.is_empty() {
        out.push(String::new());
        out.push("# What a split record does not settle: the project of the workspace it goes to.".into());
        out.extend(settle);
    }
    if !nothing.is_empty() {
        out.push(String::new());
        out.push("# Records that hold nothing (made by a registration or `workspace project` only): no workspace.".into());
        out.extend(nothing.iter().map(|n| format!("#   record {n}: {}", describe(s, *n))));
    }
    let seats: Vec<String> = a.records.iter().enumerate().filter_map(|(n, _)| s.workspaces[n].manager.as_ref().map(|m| format!("#   record {n}: {} ({})", quoted(s.name_of(m).unwrap_or("?")), word(m)))).collect();
    if !seats.is_empty() {
        out.push(String::new());
        out.push("# The managers registered last: the seats start empty; you seat each manager again".into());
        out.push("# after the migration (the workspace's line, Manager).".into());
        out.extend(seats);
    }
    let mut pending = vec![];
    for m in &s.messages {
        for (k, c) in std::iter::once(m.choice.as_ref()).chain(m.replies.iter().map(|x| x.choice.as_ref())).enumerate() {
            if c.is_some() && s.choice_open(&m.id, k as u32) {
                pending.push(format!("#   {} {}: a choice still open; it is closed after the migration", m.id, if k == 0 { "itself".into() } else { format!("reply {k}") }));
            }
        }
        let waiting = |d: &Option<workriver_core::Delivery>| d.as_ref().is_none_or(|d| d.step == workriver_core::DeliveryStep::Held);
        if (!m.by_manager && m.to.is_empty() && waiting(&m.delivery)) || m.to.iter().any(|x| waiting(&x.delivery)) {
            pending.push(format!("#   {}: not delivered yet; it is not delivered after the migration", m.id));
        }
    }
    if !pending.is_empty() {
        out.push(String::new());
        out.push("# Answer these before migrating:".into());
        out.extend(pending);
    }
    if let Some(l) = out.iter().find(|l| l.contains(['\n', '\r'])) {
        return Err(format!("plan: a line of the mapping would break in two, nothing was written: {}", quoted(l)));
    }
    Ok(out.iter().map(|l| format!("{l}\n")).collect())
}

/// Ids from the store, each as a word of the mapping.
fn list(v: &[String]) -> String {
    if v.is_empty() { "none".into() } else { v.iter().map(|x| word(x)).collect::<Vec<_>>().join(" ") }
}

/// The record an intent runs or ran in at the end.
fn record_now(s: &State, intent: &str) -> Option<usize> {
    let w = s.intent_workspace(intent)?;
    s.workspaces.iter().position(|x| std::ptr::eq(x, w))
}

fn active(s: &State, a: &Analysis, n: usize) -> bool {
    a.records[n].intents.iter().any(|i| record_now(s, i) == Some(n) && s.intents.iter().any(|x| &x.id == i && x.state == IntentState::Active))
}
