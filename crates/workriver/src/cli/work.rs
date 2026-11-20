// The commands of the work: tasks (add, start, deliver, drop, show), findings, commits,
// gates, chores, attachments, and the review of a command the user is about to run.
use super::*;

/// The commands of tasks, findings, commits, gates, chores, attachments and a command's review: what each records (printed by `run` as before), or None when it
/// printed what it had to say itself.
pub(super) fn run(cmd: Cmd, cx: &Cx) -> anyhow::Result<Option<Value>> {
    let Cx { d, actor, here_intent, .. } = *cx;
    Ok(Some(match cmd {
        Cmd::Task(TaskCmd::Show { task, json }) => {
            let shown = task_show(d, task.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&shown)?);
            } else {
                print!("{}", display::text(&shown));
            }
            return Ok(None);
        }
        Cmd::Finding(FindingCmd::Show { task, finding }) => {
            let shown = task_show(d, Some(&task))?;
            let f = shown.output.iter().flat_map(|o| &o.findings).find(|f| f.id == finding).ok_or_else(|| anyhow!("{task} has no finding {finding}"))?;
            println!("{}  {}  {}", shown.reference, shown.kind, shown.title);
            print!("{}", display::finding_text(f, ""));
            return Ok(None);
        }
        Cmd::Task(TaskCmd::Add { unit, kind, title, text, after, cause, agent, reviewer, gate, step, plan, blind, blocks, option, require_note }) => {
            let inputs: Vec<Value> = after.iter().enumerate().map(|(k, n)| json!({ "from": n, "primary": k == 0 })).collect();
            // `N` or `u1#N`, in this unit.
            let blocks = blocks
                .map(|b| match b.split_once('#') {
                    Some((u, n)) if u == unit => n.parse::<u32>().map_err(|_| anyhow!("--blocks takes a task number: {b}")),
                    Some(_) => Err(anyhow!("--blocks names a task of {unit}, the unit the gate goes into: {b}")),
                    None => b.parse::<u32>().map_err(|_| anyhow!("--blocks takes a task number: {b}")),
                })
                .transpose()?;
            let causes = cause.iter().map(|c| parse_cause(c)).collect::<anyhow::Result<Vec<_>>>()?;
            let steps: Vec<Value> = step.iter().map(|s| parse_step(s)).collect();
            let choice = parse_choice(&option, &require_note)?;
            let asks = choice.is_none() && gate.as_deref() == Some("decide") && lists_options(&[&[title.as_str(), text.as_str()][..], &step.iter().map(String::as_str).collect::<Vec<_>>()].concat());
            let out = d.append(actor, "task.created", json!({
                "unit": unit, "kind": kind, "title": title, "text": text, "inputs": inputs, "causes": causes,
                "agent": agent, "reviewer": reviewer, "gate": gate, "steps": steps, "plans": plan, "blind": blind, "blocks": blocks, "choice": choice,
            }))?;
            if asks {
                println!("{}\n{OPTIONS_HINT}", made(&out));
                return Ok(None);
            }
            out
        }
        Cmd::Task(TaskCmd::Start { task, agent }) => {
            let (unit, n) = task_ref(&task)?;
            d.append(actor, "task.started", json!({ "unit": unit, "task": n, "agent": agent }))?
        }
        Cmd::Task(TaskCmd::Drop { task, reason }) => {
            let (unit, n) = task_ref(&task)?;
            d.append(actor, "task.dropped", json!({ "unit": unit, "task": n, "reason": reason }))?
        }
        Cmd::Task(TaskCmd::Deliver { task, note, evidence, findings: None, amend: true, add_findings: Some(file) }) => {
            // More findings on a delivered review: the others are not touched.
            if !note.is_empty() || !evidence.is_empty() {
                bail!("--add-findings adds findings only; correct the note or the evidence with a separate --amend");
            }
            let (unit, n) = task_ref(&task)?;
            let mut add = read_findings(Some(&file))?;
            if let Some(list) = add.as_array_mut() {
                upload_finding_files(d, actor, list)?;
            }
            d.append(actor, "task.amended", json!({ "unit": unit, "task": n, "add": add }))?
        }
        Cmd::Task(TaskCmd::Deliver { task, note, evidence, findings, amend, .. }) => deliver(d, actor, &task, note, &evidence, findings, amend)?,
        Cmd::Attach { files, task, finding, r#move, dir } => {
            if let Some(t) = dir {
                let (unit, n) = task_ref(&t)?;
                println!("{}", d.inbox(&unit, n)?.display());
                return Ok(None);
            }
            if files.is_empty() {
                bail!("name the files to attach, or use --dir <task> for the task's folder");
            }
            // `workriver attach fetch <link>…`: the other way, a stored file into this folder.
            if files[0] == "fetch" && files.len() > 1 {
                for link in &files[1..] {
                    let name = link.split(['?', '#']).next().unwrap_or_default().rsplit('/').next().unwrap_or_default();
                    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.') {
                        bail!("{link:?} is not a stored file: give it as workriver printed it (<id>.<ext>, or its link)");
                    }
                    let bytes = d.fetch(name)?;
                    std::fs::write(name, bytes).with_context(|| format!("cannot write {name} here"))?;
                    println!("{}", std::env::current_dir().unwrap_or_default().join(name).display());
                }
                return Ok(None);
            }
            let link = task.as_deref().map(task_ref).transpose()?;
            for f in &files {
                let file = PathBuf::from(f);
                let up = d.upload(&file, actor, link.as_ref().map(|(u, n)| (u.as_str(), *n, finding.as_deref())))?;
                if r#move {
                    std::fs::remove_file(&file).with_context(|| format!("stored, but cannot remove {}", file.display()))?;
                }
                println!("{}", up["path"].as_str().unwrap_or_default());
            }
            return Ok(None);
        }
        Cmd::Commit(CommitCmd::Link { hash, task }) => {
            let (unit, n) = task_ref(&task)?;
            d.append(actor, "commit.linked", json!({ "unit": unit, "hash": hash, "task": n }))?
        }
        Cmd::Commit(CommitCmd::Unlink { hash, unit }) => d.append(actor, "commit.unlinked", json!({ "unit": unit.unwrap_or_default(), "hash": hash }))?,
        Cmd::Finding(FindingCmd::Note { task, finding, text, attach }) => {
            let (unit, n) = task_ref(&task)?;
            let mut files = vec![];
            for f in &attach {
                files.push(d.upload(Path::new(f), actor, None)?["id"].clone());
            }
            d.append(actor, "finding.noted", json!({ "unit": unit, "task": n, "finding": finding, "text": text, "attachments": files }))?
        }
        Cmd::Finding(FindingCmd::Drop { target, finding, reason }) => {
            let (unit, n, finding) = match finding {
                Some(f) => {
                    let (u, n) = task_ref(&target)?;
                    (u, n, f)
                }
                None => (String::new(), 0, target),
            };
            d.append(actor, "finding.dropped", json!({ "unit": unit, "task": n, "finding": finding, "reason": reason }))?
        }
        Cmd::Finding(FindingCmd::Decide { task, finding, decision, note }) => {
            let (unit, n) = task_ref(&task)?;
            d.append(actor, "finding.decided", json!({ "unit": unit, "task": n, "finding": finding, "decision": decision, "note": note }))?
        }
        Cmd::Gate(GateCmd::Submit { task, result, note, findings, select }) => {
            let (unit, n) = task_ref(&task)?;
            let findings = read_findings(findings.as_deref())?;
            let mut findings = findings;
            if let Some(list) = findings.as_array_mut() {
                upload_finding_files(d, actor, list)?;
            }
            d.append(actor, "gate.submitted", json!({ "unit": unit, "task": n, "result": result, "note": note, "findings": findings, "selected": select.into_iter().collect::<Vec<_>>() }))?
        }
        Cmd::Chore(ChoreCmd::Add { intent, project, title, unit, step }) => {
            // An intent given or this workspace's; else the project given, or where this runs.
            let intent = if project.is_some() { None } else { here_intent(intent).ok() };
            let project = match (&intent, project) {
                (Some(_), _) => String::new(),
                (None, Some(p)) => project_arg(&p)?,
                (None, None) => cwd_folder().unwrap_or_default(),
            };
            let steps: Vec<Value> = step.iter().map(|s| parse_step(s)).collect();
            d.append(actor, "chore.created", json!({ "intent": intent.unwrap_or_default(), "project": project, "title": title, "unit": unit, "steps": steps }))?
        }
        Cmd::Chore(ChoreCmd::Done { chore }) => d.append(actor, "chore.done", json!({ "chore": chore }))?,
        Cmd::Run(RunCmd::Review { run, verdict, text }) => {
            if run.is_empty() || !run.chars().all(|c| c.is_ascii_alphanumeric()) {
                bail!("{run:?} is not a run id");
            }
            let mut review = d.me(actor);
            review["verdict"] = json!(verdict);
            review["text"] = json!(text);
            d.call(&format!("/api/run/{run}/review"), Some(review))?;
            println!("recorded");
            return Ok(None);
        }
        _ => unreachable!("not one of tasks, findings, commits, gates, chores, attachments and a command's review"),
    }))
}

#[derive(Subcommand)]
pub(crate) enum RunCmd {
    /// Record what a command the user is about to run will do: only for the agent workriver
    /// started to review it, as its prompt says.
    Review {
        /// The run (its id, from the prompt).
        run: String,
        #[arg(long, value_parser = ["ok", "caution", "danger"])]
        verdict: String,
        /// Short Markdown: what it does, then bullets.
        #[arg(long)]
        text: String,
    },
}

// Parsed once per run: `Add`'s many options cost nothing worth boxing.
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
pub(crate) enum TaskCmd {
    #[command(about = runtime::words::HELP_TASK_SHOW)]
    Show {
        /// The task (u1#3).
        task: Option<String>,
        /// The same as JSON (stable field names).
        #[arg(long)]
        json: bool,
    },
    /// Record a task in a unit; prints its ref (u1#3). Research and design for the intent as a
    /// whole go into a research unit (`workriver unit add --kind research`), not the intent.
    ///
    /// The first task of a unit starts from the unit itself. A later one says what it
    /// follows: --after N, or --after start to run beside the first; with --cause and no
    /// --after it follows the tasks whose findings it answers. A cause must name accepted,
    /// revised or scheduled findings.
    Add {
        /// The unit (u1…).
        unit: String,
        #[arg(long, value_parser = ["implement", "design", "research", "review", "tests", "gate"], default_value = "implement")]
        kind: String,
        /// What the work is (needed for implement, design, research and gate).
        #[arg(long, default_value = "")]
        title: String,
        /// The full ask, as handed to the agent; default: the title.
        #[arg(long, default_value = "")]
        text: String,
        /// Task number this one follows, or `start`; repeat for more. The first is the direct
        /// cause, the rest are merely involved. Needed once the unit has a task, unless
        /// --cause names one in it.
        #[arg(long, value_name = "N")]
        after: Vec<String>,
        /// Findings this task answers: `N:ID[,ID…]` in this unit (`4:P2-1,P3-1`),
        /// `u1#4:P2-1` in another, or backlog findings `f3[,f4…]`. Repeatable.
        #[arg(long, value_name = "N:IDS")]
        cause: Vec<String>,
        /// The agent it is handed to.
        #[arg(long)]
        agent: Option<String>,
        /// Review tasks: which kind of review.
        /// (`architecture-review`: how the code is organized and the solution built, against the
        /// project's own rules; `workriver guide how architecture-review`.)
        #[arg(long, value_parser = ["code-review", "security-review", "architecture-review"])]
        reviewer: Option<String>,
        /// Gate tasks: what the user is asked to do (`decide`: discuss and confirm a design, or
        /// answer a question).
        #[arg(long, value_parser = ["deploy", "test", "decide"])]
        gate: Option<String>,
        /// Decide gates: an option the user picks from, `ID=LABEL` (ID: 1 to 32 of a-z, 0-9,
        /// -); 2 to 8, in the order shown, as radio buttons. Not on a design's gate (there the
        /// choices go in --text). By the manager or the user. Repeatable.
        #[arg(long, value_name = "ID=LABEL")]
        option: Vec<String>,
        /// An option whose answer needs the user's note (why). Repeatable.
        #[arg(long, value_name = "ID")]
        require_note: Vec<String>,
        /// Gate tasks: one step for the user, `TEXT` or `TEXT => COMMAND`. Repeatable.
        #[arg(long)]
        step: Vec<String>,
        /// A path the work expects to change (file or folder). Repeatable.
        #[arg(long)]
        plan: Vec<String>,
        /// Gate tasks: put this gate before task N (or u1#N), a task still to do or running
        /// that is stuck on what only the user can do (a blocker). The task follows the gate
        /// from then on and its agent gets the user's result; the manager is told. Without
        /// --after it follows what that task follows. By the task's agent, the manager or
        /// the user.
        #[arg(long, value_name = "N")]
        blocks: Option<String>,
        /// For a review, tests, research or design task: its agent sees nothing of other
        /// work (notes, evidence, findings, decisions, questions) until it delivers, so its
        /// view is its own. Blindness guards against bias, not against an agent that sets
        /// out to look (commit messages, files on disk).
        #[arg(long)]
        blind: bool,
    },
    /// Mark a task as started (by the calling agent unless --agent is given).
    Start {
        /// The task (u1#3).
        task: String,
        #[arg(long)]
        agent: Option<String>,
    },
    /// Hand a task in: a note, the checks run, and (reviews, tests) the findings.
    ///
    /// Findings are JSON, an array of
    ///   {"severity": "P1|P2|P3", "problem": "…", "impact": "what happens if left",
    ///    "where": ["path/to/file"], "suggestion": {"decision": "accept|reject|defer", "reason": "…"}}
    /// (suggestion is optional; `"attachments": ["/abs/path.png"]` attaches files to a finding),
    /// read from a file or from stdin with `--findings -`.
    Deliver {
        /// The task (u1#3).
        task: String,
        #[arg(long, default_value = "")]
        note: String,
        /// A check that was run: `COMMAND => passed: SUMMARY` or `COMMAND => failed: SUMMARY`,
        /// e.g. `cargo test => passed: 12 passed`; add `=> FILE` to attach its screenshot or
        /// report. Repeatable. Files in the task's folder (`workriver attach --dir`) are
        /// attached too.
        #[arg(long)]
        evidence: Vec<String>,
        /// A JSON file of findings, or `-` for stdin.
        #[arg(long, value_name = "FILE")]
        findings: Option<String>,
        /// Correct a task already delivered: what is given (note, evidence, findings)
        /// replaces what was delivered. Findings only while none of them is decided.
        #[arg(long)]
        amend: bool,
        /// With --amend: more findings (JSON as --findings), numbered after those the task
        /// has, which stay as they are, decided or not.
        #[arg(long, value_name = "FILE", requires = "amend", conflicts_with = "findings")]
        add_findings: Option<String>,
    },
    /// Mark a task not done yet as no longer needed (a gate asked again, a plan changed). It
    /// is kept, never deleted; what follows it no longer waits on it, and its agent is told
    /// to stop. By the intent's manager or the user (or the agent that recorded it).
    Drop {
        /// The task (u1#15).
        task: String,
        /// Why, e.g. "superseded by u1#17".
        #[arg(long)]
        reason: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum FindingCmd {
    /// One finding in full: what it says, every decision on it, and everything added to it.
    Show {
        /// The task that found it (u1#4).
        task: String,
        /// The finding (P2-1).
        finding: String,
    },
    /// Accept, reject or defer an open finding. An accepted one is then answered by new
    /// tasks (`task add --cause`).
    Decide {
        /// The task that found it (u1#4).
        task: String,
        /// The finding (P2-1).
        finding: String,
        /// `revise`: it must be fixed, but the way the note says (a note is required).
        #[arg(value_parser = ["accept", "revise", "reject", "defer"])]
        decision: String,
        #[arg(long, default_value = "")]
        note: String,
    },
    /// Add to a finding after it was reported, decided or not: a requirement for its fix, a
    /// detail, a file. By the agent that reported it, the intent's manager, or the user. The
    /// finding itself is not rewritten; the agents of the work it led to are told.
    Note {
        /// The task that found it (u1#4).
        task: String,
        /// The finding (P2-1).
        finding: String,
        #[arg(long, default_value = "")]
        text: String,
        /// A file that goes with it. Repeatable.
        #[arg(long, value_name = "FILE")]
        attach: Vec<String>,
    },
    /// Drop a deferred or scheduled finding: it will not be done.
    Drop {
        /// The task that found it (u1#4), or a backlog finding (f3).
        target: String,
        /// The finding (P2-1); not for a backlog finding.
        finding: Option<String>,
        #[arg(long)]
        reason: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum CommitCmd {
    /// Link a commit of a unit's branch to a task, without rewriting it: for a commit made
    /// without the `Task: u1#3` trailer (or with a wrong one; the link wins). It follows the
    /// commit through a rebase. By the task's agent, the intent's manager, or the user.
    Link {
        /// The commit (a hash or its beginning, as in the unit's repository).
        hash: String,
        /// The task (u1#3).
        task: String,
    },
    /// Take a link back.
    Unlink {
        hash: String,
        /// The unit, when the same commit is linked in several.
        unit: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum GateCmd {
    /// Submit a waiting gate's result. A problem needs findings (JSON as in `task deliver`); a
    /// gate with options is answered with one of them (`answered --select ID`).
    Submit {
        /// The gate (u1#5).
        task: String,
        #[arg(value_parser = ["done", "problem", "answered"])]
        result: String,
        #[arg(long, default_value = "")]
        note: String,
        #[arg(long, value_name = "FILE")]
        findings: Option<String>,
        /// With answered: the option picked (its ID).
        #[arg(long, value_name = "ID")]
        select: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum ChoreCmd {
    /// Ask the user for something that blocks nobody; prints its id (c1…).
    ///
    /// It belongs to an intent; or, with --project (or outside any workspace with a current
    /// intent), to a project; or, where no project can be told, to nothing.
    Add {
        #[arg(help = runtime::words::HELP_CHORE_INTENT)]
        intent: Option<String>,
        /// A chore of no intent: the project it belongs to (p1, or a folder).
        #[arg(long, conflicts_with = "intent")]
        project: Option<String>,
        #[arg(long)]
        title: String,
        /// The unit it concerns, if one.
        #[arg(long)]
        unit: Option<String>,
        /// One step, `TEXT` or `TEXT => COMMAND`. Repeatable.
        #[arg(long)]
        step: Vec<String>,
    },
    /// Mark a chore done.
    Done { chore: String },
}

/// `task deliver`: stores the files it names and those in the task's folder (a name starting
/// with a finding id goes with that finding), records the delivery (or `--amend`), then
/// empties the folder.
pub(crate) fn deliver(d: &Daemon, actor: &str, task: &str, note: String, evidence: &[String], findings: Option<String>, amend: bool) -> anyhow::Result<Value> {
    let (unit, n) = task_ref(task)?;
    let mut checks = vec![];
    for e in evidence {
        let (mut v, file) = parse_evidence_file(e)?;
        if let Some(f) = file {
            v["attachment"] = d.upload(&f, actor, None)?["id"].clone();
        }
        checks.push(v);
    }
    let mut found = findings.map(|f| read_findings(Some(&f))).transpose()?;
    if let Some(list) = found.as_mut().and_then(Value::as_array_mut) {
        upload_finding_files(d, actor, list)?;
    }
    // The task's inbox: files named after a finding go with it, the rest with the task.
    let inbox = d.inbox(&unit, n)?;
    let staged = staged_files(&inbox)?;
    let new_ids = found.as_ref().and_then(Value::as_array).map(|l| finding_ids(l)).unwrap_or_default();
    let known: Vec<String> = if found.is_some() {
        new_ids.clone()
    } else {
        let state: core::State = serde_json::from_value(d.get("/api/state")?)?;
        state.task(&unit, n).map(|t| t.findings.iter().map(|f| f.found.id.clone()).collect()).unwrap_or_default()
    };
    let mut task_files = vec![];
    for file in &staged {
        let name = file.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
        match known.iter().find(|id| finding_file(&name, id)) {
            Some(id) if new_ids.contains(id) => {
                let up = d.upload(file, actor, None)?;
                let k = new_ids.iter().position(|x| x == id).unwrap_or(0);
                if let Some(f) = found.as_mut().and_then(|v| v.get_mut(k)) {
                    let list = f["attachments"].as_array().cloned().unwrap_or_default();
                    f["attachments"] = Value::Array(list.into_iter().chain([up["id"].clone()]).collect());
                }
            }
            // A finding already recorded: linked to it directly.
            Some(id) => {
                d.upload(file, actor, Some((&unit, n, Some(id))))?;
            }
            None => task_files.push(d.upload(file, actor, None)?["id"].clone()),
        }
    }
    let out = if amend {
        // Only what is given replaces what was delivered.
        let note = (!note.is_empty()).then_some(note);
        let checks = (!checks.is_empty()).then_some(checks);
        d.append(actor, "task.amended", json!({ "unit": unit, "task": n, "note": note, "evidence": checks, "findings": found, "attachments": task_files }))?
    } else {
        let found = found.unwrap_or_else(|| json!([]));
        d.append(actor, "task.delivered", json!({ "unit": unit, "task": n, "note": note, "evidence": checks, "findings": found, "attachments": task_files }))?
    };
    // Recorded: the staged files now live in the store.
    for file in &staged {
        let _ = std::fs::remove_file(file);
    }
    Ok(out)
}

/// Findings JSON may name files to attach (`"attachments": ["/abs/shot.png"]`): they are
/// stored, and their ids put in their place.
fn upload_finding_files(d: &Daemon, actor: &str, findings: &mut [Value]) -> anyhow::Result<()> {
    for f in findings {
        let Some(list) = f.get("attachments").and_then(Value::as_array).cloned() else { continue };
        let mut ids = vec![];
        for p in list {
            let p = p.as_str().ok_or_else(|| anyhow!("a finding's attachments are file paths"))?;
            ids.push(d.upload(Path::new(p), actor, None)?["id"].clone());
        }
        f["attachments"] = Value::Array(ids);
    }
    Ok(())
}

/// The ids findings get when recorded: per severity, in order (P1-1, P2-1, P2-2…).
fn finding_ids(findings: &[Value]) -> Vec<String> {
    let mut seen = std::collections::HashMap::new();
    findings
        .iter()
        .map(|f| {
            let sev = f["severity"].as_str().unwrap_or("P3").to_string();
            let k = seen.entry(sev.clone()).or_insert(0);
            *k += 1;
            format!("{sev}-{k}")
        })
        .collect()
}

/// A staged file belongs to a finding when its name starts with the finding's id and a
/// separator: `P2-1-login.png`, `P2-1.png`, `P2-1_x.txt`.
fn finding_file(name: &str, id: &str) -> bool {
    name.strip_prefix(id).is_some_and(|rest| rest.starts_with(['-', '_', '.', ' ']))
}

/// The files waiting in a task's inbox, oldest first.
fn staged_files(inbox: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(inbox)
        .with_context(|| format!("cannot read {}", inbox.display()))?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()) && !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| (e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), e.path()))
        .collect();
    files.sort();
    Ok(files.into_iter().map(|(_, p)| p).collect())
}

fn read_findings(from: Option<&str>) -> anyhow::Result<Value> {
    let text = match from {
        None => return Ok(json!([])),
        Some("-") => {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s)?;
            s
        }
        Some(path) => std::fs::read_to_string(path).with_context(|| format!("cannot read {path}"))?,
    };
    let v: Value = serde_json::from_str(&text).context("findings must be a JSON array")?;
    if !v.is_array() {
        bail!("findings must be a JSON array");
    }
    Ok(v)
}

/// One task as the service shows it to this caller; none: the task this agent is on.
fn task_show(d: &Daemon, task: Option<&str>) -> anyhow::Result<crate::common::wire::TaskShow> {
    let path = match task {
        Some(t) => {
            task_ref(t)?;
            format!("/api/task?task={}", t.replace('#', "%23"))
        }
        None => "/api/task".into(),
    };
    Ok(serde_json::from_value(d.get(&path)?)?)
}
