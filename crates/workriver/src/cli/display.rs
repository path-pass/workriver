// What the CLI prints that is more than a line: the board, a task or finding in full, an
// intent's history, and a machine's load and history.
use std::fmt::Write as _;

use crate::common::wire::{FindingShow, TaskShow};

use super::*;

/// A compact text view: per unit its tasks, open findings and waiting gates; open chores.
pub(super) fn board(d: &Daemon, target: Option<&str>) -> anyhow::Result<()> {
    print!("{}", board_text(d, target)?);
    println!("(one task in full, with its causes, notes and output: `workriver task show <unit>#<n>`)");
    Ok(())
}

/// The board as text, as the caller may see it.
pub(crate) fn board_text(d: &Daemon, target: Option<&str>) -> anyhow::Result<String> {
    use std::fmt::Write as _;
    let mut out = String::new();
    let read = d.get("/api/state")?;
    // Where each intent's text stands, which the state's own type does not carry.
    let version = |id: &str| read["intents"].as_array().and_then(|l| l.iter().find(|i| i["id"] == id)).and_then(|i| i["edits"]["version"].as_u64()).filter(|n| *n > 0);
    let state: core::State = serde_json::from_value(read.clone())?;
    let wanted = |intent: &str, unit: Option<&str>| match target {
        None => true,
        Some(t) => t == intent || unit == Some(t),
    };
    if let Some(t) = target
        && !state.intents.iter().any(|i| i.id == t)
        && !state.units.iter().any(|u| u.id == t)
    {
        bail!("no intent or unit {t}");
    }
    if state.reduced {
        let _ = writeln!(out, "blind review: other work is hidden until you deliver. Read the code from git (the diff, not the commit messages); what you need beyond your task, ask your manager for.");
    }
    let named = |s: &str| state.name_of(s).unwrap_or(s).to_string();
    // What git shows (commits, where each unit stands), and what the runtime shows (a worker
    // that stopped), as the page shows it.
    let ws = d.get("/api/workspace?light=true").unwrap_or_default();
    // Where the user opens each intent and unit: a line under it, for pointing the user there.
    let base = page(d).ok();
    let link = |out: &mut String, indent: &str, id: &str| {
        if let Some(b) = &base {
            let _ = writeln!(out, "{indent}link: {b}/{id}");
        }
    };
    let shown = |id: &str| ws["intents"].as_array().into_iter().flatten().flat_map(|i| i["units"].as_array().into_iter().flatten()).find(|x| x["id"] == id).cloned().unwrap_or_default();
    let line = |t: &core::Task| {
        let mut out = String::new();
        let what = match t.kind {
            TaskKind::Review => format!("review ({})", t.reviewer.map(|r| name(&r)).unwrap_or_default()),
            TaskKind::Gate => format!("gate ({})", t.gate.map(|g| name(&g)).unwrap_or_default()),
            k => name(&k),
        };
        let after: Vec<String> = t.inputs.iter().map(|i| if i.from == "start" { "start".into() } else { format!("#{}", i.from) }).collect();
        let agent = t.agent.as_ref().map(|a| format!(" @{}", named(a))).unwrap_or_default();
        let blind = if t.blind { " [blind]" } else { "" };
        let _ = writeln!(out, "    #{} {:<8} {what}{blind} {}{agent}  after {}", t.n, name(&t.state), t.title, after.join(","));
        // On a blind task the board is the hand-out: the whole ask.
        if state.reduced && !t.text.is_empty() && t.text != t.title {
            let _ = writeln!(out, "       {}", t.text);
        }
        for f in t.findings.iter().filter(|f| f.state == FindingState::Open) {
            let _ = writeln!(out, "       open {}: {}  ({})", f.found.id, f.found.problem, f.found.r#where.join(", "));
            for n in &f.notes {
                let _ = writeln!(out, "         + {} added: {}", named(&n.by), n.text);
            }
        }
        // Work still to do: what it answers, with what was added to those findings since.
        if !t.state.over() {
            for c in &t.causes {
                let at = c.unit.clone().unwrap_or_else(|| t_owner(&state, t));
                for f in state.task(&at, c.task).iter().flat_map(|x| x.findings.iter()).filter(|f| c.findings.contains(&f.found.id)) {
                    let _ = writeln!(out, "       answers {at}#{} {}: {}", c.task, f.found.id, f.found.problem);
                    for n in &f.notes {
                        let _ = writeln!(out, "         + {} added: {}", named(&n.by), n.text);
                    }
                }
            }
        }
        if t.state == TaskState::Waiting {
            let _ = writeln!(out, "       waiting for the user");
        }
        if let Some(d) = &t.dropped {
            let _ = writeln!(out, "       dropped ({})", d.reason);
        }
        let n = t.n.to_string();
        let mine = shown(&t_owner(&state, t))["tasks"].as_array().and_then(|ts| ts.iter().find(|x| x["id"] == n.as_str()).cloned());
        if let Some(s) = mine.and_then(|x| serde_json::from_value::<crate::common::wire::Stall>(x["stalled"].clone()).ok()) {
            let _ = writeln!(out, "       {}", stalled_line(&s));
        }
        out
    };
    // Dropped intents (merged ones among them) only when asked for by id.
    let hidden = state.intents.iter().filter(|i| target.is_none() && i.state == core::IntentState::Dropped).count();
    for i in &state.intents {
        if target.is_none() && i.state == core::IntentState::Dropped {
            continue;
        }
        let units: Vec<_> = state.units.iter().filter(|u| u.intent == i.id && wanted(&i.id, Some(&u.id))).collect();
        let chores: Vec<_> = state.chores.iter().filter(|c| c.intent == i.id && !c.done && wanted(&i.id, c.unit.as_deref())).collect();
        if units.is_empty() && chores.is_empty() && !wanted(&i.id, None) {
            continue;
        }
        let sort = planning::labels(&serde_json::to_value(i).unwrap_or_default());
        let _ = writeln!(out, "{} {}{sort}{}", i.id, i.title, state.manager_of(&i.id).map(|m| format!("  (manager {})", named(m))).unwrap_or_default());
        link(&mut out, "  ", &i.id);
        if i.state == core::IntentState::Dropped {
            let _ = writeln!(out, "  dropped: {}", i.reason.as_deref().unwrap_or(""));
        }
        // The version is the base an edit of its title, goal or note names.
        let v = version(&i.id).map(|n| format!("  (v{n})")).unwrap_or_default();
        match (i.goal.is_empty(), v.is_empty()) {
            (false, _) => {
                let _ = writeln!(out, "  goal: {}{v}", i.goal);
            }
            (true, false) => {
                let _ = writeln!(out, "{v}");
            }
            (true, true) => {}
        }
        // More about it, line by line: what the user wrote, and what merged intents brought in.
        if !i.note.trim().is_empty() {
            let _ = writeln!(out, "  note:");
            for l in i.note.trim().lines() {
                let _ = writeln!(out, "    {l}");
            }
        }
        if i.state == core::IntentState::Active && state.finished(&i.id) {
            let _ = writeln!(out, "  every unit is merged or done: close it with `workriver intent done {}`", i.id);
        }
        // The default is not said: the manager decides findings (the user may always).
        match i.findings {
            core::FindingsPolicy::Manager => {}
            core::FindingsPolicy::User => { let _ = writeln!(out, "  findings: the user decides them, not the manager"); }
        }
        // The intent's own work, from before research units: research and design that belong to no unit.
        if wanted(&i.id, None) {
            i.tasks.iter().for_each(|t| out += &line(t));
        }
        for u in units {
            let seen = shown(&u.id);
            let progress = seen["progress"].as_str().map(str::to_string).unwrap_or_else(|| name(&state.progress(u)));
            let why = u.done.as_ref().filter(|d| !d.reason.is_empty()).map(|d| format!(" ({})", d.reason)).unwrap_or_default();
            // Where the work is, as whoever does it sees it: the machine, the path and the branch there.
            let on = u.machine.as_ref().map(|m| format!("on {m}: ")).unwrap_or_default();
            let place = match u.kind {
                core::UnitKind::Code => format!("{on}{}, branch {} from {}", u.repo, u.branch, u.base),
                core::UnitKind::Spike => format!("{on}{}, branch {} from {}; spike, never merged: close it with `workriver unit done`", u.repo, u.branch, u.base),
                core::UnitKind::Research => "research, no branch: close it with `workriver unit done`".into(),
            };
            let _ = writeln!(out, "  {} {}  [{place}]  {progress}{why}", u.id, u.title);
            link(&mut out, "    ", &u.id);
            // Its machine is disconnected: what is shown is as last read, and nothing waited on it.
            if let Some(since) = u.machine.as_deref().and_then(|m| down_since(&ws, m)) {
                let _ = writeln!(out, "    {} is disconnected since {since}: as last read", u.machine.as_deref().unwrap_or_default());
            }
            // Where it was before (a rebase onto a new branch): its work is the same unit's.
            for e in &u.earlier {
                let _ = writeln!(out, "    previously on {} (from {}), {} commits seen there", e.branch, e.base, e.commits.len());
            }
            // What only the branch it left had (read from git by the service, so asked for
            // only where a unit moved): said, where the browser would show a task without commits.
            if !u.earlier.is_empty() && !state.reduced {
                for t in seen["tasks"].as_array().into_iter().flatten() {
                    let lost: Vec<&str> = t["lostCommits"].as_array().into_iter().flatten().filter_map(|c| c.as_str()).collect();
                    if !lost.is_empty() {
                        let _ = writeln!(out, "    #{}: commits not on the current branch: {}", t["taskId"].as_str().unwrap_or("?"), lost.join(", "));
                    }
                }
            }
            if state.reduced && u.has_code() {
                let _ = writeln!(out, "    to review: `git -C {} diff {}...{}` (commits: `git rev-list {}..{}`)", u.repo, u.base, u.branch, u.base, u.branch);
            }
            u.tasks.iter().for_each(|t| out += &line(t));
            let deferred: Vec<String> = u
                .tasks
                .iter()
                .flat_map(|t| t.findings.iter().filter(|f| f.state == FindingState::Deferred).map(move |f| format!("#{} {}: {}", t.n, f.found.id, f.found.problem)))
                .collect();
            if !deferred.is_empty() {
                let _ = writeln!(out, "    deferred:");
                for d in deferred {
                    let _ = writeln!(out, "      {d}");
                }
            }
        }
        for c in chores {
            let _ = writeln!(out, "  chore {} {} (by {})", c.id, c.title, state.name_of(&c.by).unwrap_or(&c.by));
        }
    }
    // Chores of no intent: a project's, or nobody's.
    if target.is_none() {
        for c in state.chores.iter().filter(|c| c.intent.is_empty() && !c.done) {
            let of = if c.project.is_empty() { String::new() } else { format!(" [project {}]", c.project) };
            let _ = writeln!(out, "chore {} {}{of} (by {})", c.id, c.title, state.name_of(&c.by).unwrap_or(&c.by));
        }
        if hidden > 0 {
            let _ = writeln!(out, "({hidden} dropped or merged intents not shown; `workriver board <intent>` shows one)");
        }
    }
    Ok(out)
}

/// The unit (or intent) a task is in: where its causes are, unless they say otherwise.
pub(super) fn t_owner(state: &core::State, t: &core::Task) -> String {
    let has = |tasks: &[core::Task]| tasks.iter().any(|x| std::ptr::eq(x, t));
    state.units.iter().find(|u| has(&u.tasks)).map(|u| u.id.clone()).or_else(|| state.intents.iter().find(|i| has(&i.tasks)).map(|i| i.id.clone())).unwrap_or_default()
}

/// Since when a machine is disconnected, as the snapshot `ws` says; none while connected.
pub(super) fn down_since(ws: &Value, machine: &str) -> Option<String> {
    let m = ws["machines"].as_array()?.iter().find(|m| m["label"] == machine)?;
    (m["tunnel"] != true && m["enabled"] == true).then(|| m["downSince"].as_str().unwrap_or("now").to_string())
}

/// One machine's history in a line: mean and highest CPU and memory share over the minutes.
pub(crate) fn history_text(m: &Value, points: &Value) -> String {
    let name = format!("{}{}", m["label"].as_str().unwrap_or("?"), if m["here"] == true { " (here)" } else { "" });
    let points = points.as_array().map(Vec::as_slice).unwrap_or_default();
    let stats = |v: Vec<f64>| if v.is_empty() { "-".to_string() } else { format!("mean {:.0}% max {:.0}%", v.iter().sum::<f64>() / v.len() as f64, v.iter().copied().fold(0.0, f64::max)) };
    let cpu = stats(points.iter().filter_map(|p| p["cpu"].as_f64()).collect());
    let mem = stats(points.iter().filter_map(|p| Some(p["memUsed"].as_f64()? * 100.0 / p["memTotal"].as_f64().filter(|t| *t > 0.0)?)).collect());
    if points.is_empty() { format!("{name}  no history yet") } else { format!("{name}  cpu {cpu}  memory {mem}  ({} min)", points.len()) }
}

/// `machine load` as text: one line per machine, its newest CPU and memory and alerts.
pub(crate) fn load_text(r: &Value) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let gb = |b: &Value| b.as_f64().unwrap_or(0.0) / 1_073_741_824.0;
    for m in r["machines"].as_array().into_iter().flatten() {
        let name = format!("{}{}", m["label"].as_str().unwrap_or("?"), if m["here"] == true { " (here)" } else { "" });
        let Some(s) = m["samples"].as_array().and_then(|s| s.last()).filter(|_| m["reachable"] == true) else {
            let why = m["error"].as_str().map(str::to_string).or_else(|| m["downSince"].as_str().map(|d| format!("disconnected since {d}"))).unwrap_or_else(|| "no reading yet".into());
            let _ = writeln!(out, "{name}  {why}");
            continue;
        };
        let cpu = s["cpu"].as_f64().map(|c| format!("{c:.0}%")).unwrap_or_else(|| "-".into());
        let total = gb(&s["memTotal"]);
        let mem = if total > 0.0 { format!("{:.0}% ({:.1}/{:.1} GB)", gb(&s["memUsed"]) * 100.0 / total, gb(&s["memUsed"]), total) } else { "-".into() };
        let alerts: Vec<String> = m["alerts"].as_object().into_iter().flatten().map(|(k, v)| format!("{k} {} since {}", v["level"].as_str().unwrap_or("?").to_uppercase(), v["since"].as_str().unwrap_or("?"))).collect();
        let alerts = if alerts.is_empty() { String::new() } else { format!("  ALERT: {}", alerts.join(", ")) };
        let _ = writeln!(out, "{name}  cpu {cpu}  memory {mem}{alerts}");
        // Below it, what this machine's system says beyond that; a missing piece is left out,
        // except Linux pressure, which says it is unknown (no PSI there).
        let mut more = vec![];
        if total > 0.0 {
            more.push(format!("available {:.1} GB", total - gb(&s["memUsed"])));
        }
        if let Some(p) = s["pressure"].as_str() {
            more.push(format!("pressure {p}"));
        } else if s["os"] == "linux" {
            more.push(match (s["pressureSome"].as_f64(), s["pressureFull"].as_f64()) {
                (Some(some), Some(full)) => format!("pressure some {some:.2}% full {full:.2}% (last 10 s)"),
                _ => "pressure unknown".into(),
            });
        }
        if !s["memCompressed"].is_null() {
            more.push(format!("compressed {:.1} GB", gb(&s["memCompressed"])));
        }
        if let Some(c) = s["compressor"].as_f64() {
            more.push(format!("compressor {c:.0}% of its limit"));
        }
        if let Some(f) = s["systemFree"].as_f64() {
            more.push(format!("system free {f:.0}%"));
        }
        if let Some(n) = s["oomKills"].as_u64() {
            more.push(format!("oom kills {n} ({} while watched)", s["oomKillsWatched"].as_u64().unwrap_or(0)));
        }
        if s["swapTotal"].as_u64().is_some_and(|t| t > 0) {
            more.push(format!("swap {:.1}/{:.1} GB", gb(&s["swapUsed"]), gb(&s["swapTotal"])));
        }
        for (k, d) in s["disks"].as_array().into_iter().flatten().enumerate() {
            let at = if k == 0 { String::new() } else { format!(" {}", d["mount"].as_str().unwrap_or("?")) };
            more.push(format!("disk{at} {:.1} GB free of {:.1} GB", gb(&d["free"]), gb(&d["total"])));
        }
        let _ = writeln!(out, "  {}", more.join("  "));
    }
    let t = &r["thresholds"];
    let on = |m: &Value, text: String| if m["on"] == true { text } else { "off".into() };
    let held = |m: &Value| m["minutes"].as_u64().map_or("at once".into(), |n| format!("for {n} min"));
    let lim = |m: &Value| on(m, format!("{}% {}", m["percent"], held(m)));
    // 60.0 as JSON says "60" here.
    let n = |v: &Value| v.as_f64().map_or("?".into(), |x| x.to_string());
    let lv = |m: &Value, dir: &str, unit: &str| on(m, format!("warn {dir}{}{unit} crit {dir}{}{unit} {}", n(&m["warn"]), n(&m["crit"]), held(m)));
    let (d, oom) = (&t["disk"], &t["oom"]);
    let disk = on(d, format!("warn >{}% or <{} GB free, crit >{}% or <{} GB free {}", n(&d["warn"]), n(&d["warnFree"]), n(&d["crit"]), n(&d["critFree"]), held(d)));
    let _ = writeln!(
        out,
        "alerts (the user sets them in Settings): cpu {}, memory {}; macOS: compressor {}, swap {}, pressure {}, system free {}; Linux: available {}, psi full {}, oom {}; every machine: disk {}",
        lim(&t["cpu"]),
        lim(&t["memory"]),
        lv(&t["compressor"], ">", "%"),
        lv(&t["swap"], ">", " GB"),
        on(&t["pressure"], format!("warning=warn critical=crit {}", held(&t["pressure"]))),
        lv(&t["systemFree"], "<", "%"),
        lv(&t["available"], "<", "%"),
        lv(&t["psi"], ">", "%"),
        on(oom, format!("crit at once, held {} min", oom["minutes"])),
        disk,
    );
    out
}

/// One line per machine: whether it is followed and can be reached, and why not.
pub(super) fn print_machines(list: &Value) {
    for m in list.as_array().into_iter().flatten() {
        let label = m["label"].as_str().unwrap_or("?");
        let state = if m["enabled"] != true {
            "disabled".to_string()
        } else if m["connecting"] == true {
            "connecting…".to_string()
        } else if m["reachable"] == true && m["tunnel"] == true {
            format!("connected, {} agents", m["agents"])
        } else if m["tunnel"] == true {
            crate::runtime::words::REMOTE_SILENT.to_string()
        } else {
            let since = m["downSince"].as_str().map(|s| format!(" since {s}")).unwrap_or_default();
            format!("disconnected{since}")
        };
        println!("machine {label} ({}): {state}", m["ssh"].as_str().unwrap_or("?"));
        for why in [&m["tunnelError"], &m["error"]].into_iter().filter_map(|e| e.as_str()) {
            println!("  last error: {why}");
        }
        if let Some(n) = m["retryIn"].as_u64().filter(|_| m["tunnel"] != true && m["enabled"] == true) {
            println!("  next attempt in {n} s");
        }
        if let Some(p) = m["port"].as_u64() {
            println!("  its port there: {p} (in ~/{} there, which the workriver CLI there reads)", crate::common::home::PORT_FILE);
        }
    }
}

/// The page's address where the user opens it, as the service says (also through a tunnel).
pub(crate) fn page(d: &Daemon) -> anyhow::Result<String> {
    d.get("/api/status")?["url"].as_str().map(str::to_string).ok_or_else(|| anyhow!("the service does not say its address: it is an older build; restart `workriver serve`"))
}

/// Text written by someone, kept as written: after its label when it is one line, else
/// each line indented under it.
pub(super) fn block(out: &mut String, label: &str, text: &str) {
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
pub(crate) fn finding_text(f: &FindingShow, pad: &str) -> String {
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

/// A stopped worker, as the board and `task show` say it.
pub(crate) fn stalled_line(s: &crate::common::wire::Stall) -> String {
    let reminded = s.reminded.as_ref().map(|at| format!("; reminded at {at}")).unwrap_or_default();
    match (s.escalated, s.gone) {
        (_, true) => format!("stalled since {}: its agent is gone{reminded}", s.since),
        (true, false) => format!("stalled since {}: its agent stopped without delivering{reminded}", s.since),
        (false, false) => format!("idle since {} without delivering{reminded}", s.since),
    }
}

/// `workriver task show`, as text.
pub(crate) fn text(s: &TaskShow) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}  {}  {}{}", s.reference, s.kind, s.state, if s.blind { "  [blind]" } else { "" });
    if !s.link.is_empty() {
        let _ = writeln!(out, "Link: {}", s.link);
    }
    if !s.title.is_empty() {
        let _ = writeln!(out, "Title: {}", s.title);
    }
    if let Some(d) = &s.dropped {
        block(&mut out, &format!("Dropped as no longer needed by {} at {}, because", d.by, d.at), &d.reason);
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
    if let Some(t) = &s.trace {
        let _ = writeln!(out, "  harness {}, model {}, session {}", t.harness.as_deref().unwrap_or("(not known)"), t.model.as_deref().unwrap_or("(not given)"), t.session);
        let _ = writeln!(out, "  session file: {}", session_file(t));
    }
    if let Some(st) = &s.stalled {
        let _ = writeln!(out, "{}", stalled_line(st));
    }
    if !s.text.is_empty() && s.text != s.title {
        block(&mut out, &format!("What it asks (written by {})", s.written_by), &s.text);
    }
    // Added since: part of what it asks, each with whose it is and when.
    for x in &s.notes {
        let files = if x.files.is_empty() { String::new() } else { format!("\nFiles: {}", x.files.join(", ")) };
        block(&mut out, &format!("Added to what it asks by {} at {}", x.by, x.at), &format!("{}{files}", x.text));
    }
    let _ = writeln!(out, "After: {}", s.after.join(", "));
    if !s.causes.is_empty() {
        let _ = writeln!(out, "Causes (it must answer these findings):");
        for c in &s.causes {
            let _ = writeln!(out, "  from {}:", c.from);
            out += &finding_text(&c.finding, "    ");
        }
    }
    if !s.earlier.is_empty() {
        let _ = writeln!(out, "Designed and found before this task (each design says whether the user confirmed it; if one seems wrong, see `workriver guide how contract`):");
        for d in &s.earlier {
            let title = if d.title.is_empty() { String::new() } else { format!(" \"{}\"", d.title) };
            let review = if d.under_review {
                ", the design under review"
            } else if d.revises {
                ", the design you revise"
            } else {
                ""
            };
            let review = review.to_string() + &d.in_place_of.as_ref().map(|o| format!(", revises {o}")).unwrap_or_default();
            let by = if d.kind == "research" { "found" } else { "designed" };
            let gate = match (&d.gate, d.confirmed) {
                (Some(g), Some(true)) => format!(", confirmed by the user in {g}"),
                (Some(g), Some(false)) => format!(", not confirmed by the user in {g}"),
                (Some(g), None) => format!(", not confirmed yet (gate {g})"),
                (None, _) => String::new(),
            };
            let gate = gate + &d.revised_in.as_ref().map(|r| format!(", being revised in {r}")).unwrap_or_default();
            let mut b = String::new();
            block(&mut b, &format!("from {} ({}{title}{review}), {by} by {}{gate}", d.from, d.kind, d.delivered_by), if d.note.is_empty() { "(no note)" } else { &d.note });
            if !d.files.is_empty() {
                let _ = writeln!(b, "  Files: {}", d.files.join(", "));
            }
            out += &b.lines().map(|l| format!("  {l}\n")).collect::<String>();
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
    if let Some(c) = &s.choice {
        let _ = writeln!(out, "Choices (the user picks one):");
        let options = c.options();
        let wide = options.iter().map(|o| o.value.chars().count()).max().unwrap_or(0);
        for o in &options {
            let _ = writeln!(out, "  {:wide$}  {}{}", o.value, o.label, if c.needs_note(&o.value) { " (a note required)" } else { "" });
        }
    }
    match &s.output {
        None => {
            let _ = writeln!(out, "Output: not delivered yet");
        }
        Some(o) => {
            let _ = writeln!(out, "Output (delivered {} by {}){}:", o.at.as_deref().unwrap_or("?"), o.delivered_by, o.result.as_deref().map(|r| format!(", result: {r}")).unwrap_or_default());
            for v in &o.selected {
                let label = s.choice.as_ref().and_then(|c| c.label(v)).unwrap_or_default();
                let _ = writeln!(out, "  Chose: {v} \"{label}\"");
            }
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

/// An intent's history as text: its versions (who, when, what each changed), or with
/// `version` one whole, or with `diff` what it changed against the one before it.
pub(crate) fn intent_history_text(d: &Daemon, intent: &str, version: Option<u32>, diff: Option<u32>) -> anyhow::Result<String> {
    use std::fmt::Write as _;
    let mut out = String::new();
    let field = |v: &Value, k: &str| v[k].as_str().unwrap_or_default().to_string();
    // Kind, size and tags as words: "fix", "S", "security, cli"; empty when not set.
    let sort = |v: &Value, k: &str| match k {
        "size" => field(v, k).to_uppercase(),
        "tags" => v[k].as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>().join(", "),
        _ => field(v, k),
    };
    if let Some(n) = version.or(diff) {
        let got = d.get(&format!("/api/intent/{intent}/history?version={n}"))?;
        let (now, before) = (&got["text"], &got["before"]);
        if version.is_some() || before.is_null() {
            let _ = writeln!(out, "{intent} v{n}\ntitle: {}\ngoal: {}", field(now, "title"), field(now, "goal"));
            for k in ["kind", "size", "tags"].into_iter().filter(|k| !sort(now, k).is_empty()) {
                let _ = writeln!(out, "{k}: {}", sort(now, k));
            }
            let _ = writeln!(out, "note:\n{}", field(now, "note"));
            return Ok(out);
        }
        let _ = writeln!(out, "{intent} v{n} against v{}", n - 1);
        for k in ["title", "goal"] {
            if field(before, k) != field(now, k) {
                let _ = writeln!(out, "{k}: {} -> {}", field(before, k), field(now, k));
            }
        }
        let shown = |v: &Value, k: &str| Some(sort(v, k)).filter(|x| !x.is_empty()).unwrap_or_else(|| "(not set)".into());
        for k in ["kind", "size", "tags"] {
            if sort(before, k) != sort(now, k) {
                let _ = writeln!(out, "{k}: {} -> {}", shown(before, k), shown(now, k));
            }
        }
        let (a, b) = (field(before, "note") + "\n", field(now, "note") + "\n");
        if a != b {
            let _ = writeln!(out, "note:\n{}", similar::TextDiff::from_lines(&a, &b).unified_diff().header(&format!("v{}", n - 1), &format!("v{n}")));
        }
        return Ok(out);
    }
    let got = d.get(&format!("/api/intent/{intent}/history"))?;
    let versions = got["versions"].as_array().cloned().unwrap_or_default();
    for v in &versions {
        let who = match v["name"].as_str() {
            Some(name) => name.to_string(),
            None if v["by"] == "user" => "the user".to_string(),
            None => field(v, "by"),
        };
        // A key present is a field the version set (a kind or size cleared is null).
        let mut changed: Vec<&str> = ["title", "goal", "note", "kind", "size", "tags"].into_iter().filter(|k| v.get(*k).is_some()).collect();
        let what = match v["how"].as_str() {
            Some("created") => "created".to_string(),
            Some("merged") => {
                let from: Vec<String> = v["merged"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)).collect();
                let title = if changed.contains(&"title") { ", with a new title" } else { "" };
                // The user's merge widens the tags (a version of them); an agent's never does.
                let tags = if changed.contains(&"tags") { "; changed the tags" } else { "" };
                format!("merged {} into it{title}{tags}", from.join(", "))
            }
            _ => {
                let added = !v["added"].is_null();
                changed.retain(|k| *k != "note" || !added);
                match (changed.is_empty(), added) {
                    (true, true) => "added to the note".to_string(),
                    (false, true) => format!("changed the {}; added to the note", changed.join(", ")),
                    _ => format!("changed the {}", changed.join(", ")),
                }
            }
        };
        let _ = writeln!(out, "v{}  {}  {who}  {what}", v["n"], field(v, "at"));
    }
    let n = versions.last().map(|v| v["n"].to_string()).unwrap_or_default();
    let _ = writeln!(out, "(now v{n}: give it as --base to change the title, goal, note, kind, size or tags; one whole: `workriver intent history {intent} --version N`; what it changed: `--diff N`)");
    Ok(out)
}

/// Where a task's session file is, on which machine, or why that is not known.
pub(crate) fn session_file(t: &crate::common::wire::Trace) -> String {
    use crate::common::wire::NoFile;
    let on = match (t.machine.as_deref(), t.unconfirmed) {
        (Some(m), _) => format!(" (on machine {m}, not looked at there)"),
        (None, true) => " (worked out, not seen there)".into(),
        (None, false) => String::new(),
    };
    match (&t.file, t.unknown) {
        (Some(f), _) => format!("{f}{on}"),
        (None, Some(NoFile::Id)) => "not known: the harness gave no session id of its own".into(),
        (None, Some(NoFile::Cwd)) => "not known: the agent's working directory was not recorded (it was last seen before workriver kept it)".into(),
        (None, Some(NoFile::Missing)) => "not found by its id on the machine workriver runs on".into(),
        (None, _) => format!("not known: workriver does not know where {} keeps its sessions", t.harness.as_deref().unwrap_or("this harness")),
    }
}
