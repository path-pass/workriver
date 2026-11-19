// The commands of planning: intents, units, projects, workspaces and the backlog, and a
// manager registering itself. `workriver backlog list` and `export` read a project's roadmap
// and deferred findings from the daemon's snapshot.
use super::*;

/// One deferred (or scheduled) finding: where it is, and the finding.
struct Deferred<'a> {
    at: String,
    finding: &'a Value,
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or_default()
}

fn external(v: &Value) -> String {
    match v.get("external") {
        Some(e) if e.is_object() => format!(" [{}:{}]", s(&e["system"]), s(&e["key"])),
        _ => String::new(),
    }
}

/// A project as given (p1, or its folder) as its folder.
fn folder(w: &Value, given: &str) -> Option<String> {
    w["projects"].as_array().into_iter().flatten().find(|p| s(&p["id"]) == given || s(&p["key"]) == given).map(|p| s(&p["id"]).to_string())
}

fn roadmap<'a>(w: &'a Value, project: &str) -> Vec<&'a Value> {
    let mut list: Vec<&Value> = w["intents"].as_array().into_iter().flatten().filter(|i| s(&i["project"]) == project && s(&i["state"]) == "planned").collect();
    list.sort_by_key(|i| i["order"].as_u64().unwrap_or(u64::MAX));
    list
}

fn deferred<'a>(w: &'a Value, project: &str) -> Vec<Deferred<'a>> {
    let waiting = |f: &Value| matches!(s(&f["state"]), "deferred" | "scheduled");
    let mut out = vec![];
    for i in w["intents"].as_array().into_iter().flatten().filter(|i| s(&i["project"]) == project) {
        for u in i["units"].as_array().into_iter().flatten() {
            for t in u["tasks"].as_array().into_iter().flatten() {
                for f in t["output"]["findings"].as_array().into_iter().flatten().filter(|f| waiting(f)) {
                    out.push(Deferred { at: format!("{} {}#{} {}", s(&i["id"]), s(&u["id"]), s(&t["id"]), s(&f["id"])), finding: f });
                }
            }
        }
    }
    for b in w["backlog"].as_array().into_iter().flatten().filter(|b| s(&b["project"]) == project && waiting(&b["finding"])) {
        let from = if s(&b["source"]).is_empty() { String::new() } else { format!(" from {}", s(&b["source"])) };
        out.push(Deferred { at: format!("{}{from}", s(&b["finding"]["id"])), finding: &b["finding"] });
    }
    out
}

fn list(d: &Daemon, project: Option<&str>) -> anyhow::Result<()> {
    let w = d.get("/api/workspace?light=true")?;
    let wanted = project.map(|p| folder(&w, p).unwrap_or_default());
    let projects: Vec<&Value> = w["projects"].as_array().into_iter().flatten().filter(|p| wanted.as_deref().is_none_or(|x| s(&p["id"]) == x)).collect();
    if projects.is_empty() {
        bail!("no project {}; see `workriver project list`", project.unwrap_or("yet"));
    }
    for p in projects {
        let id = s(&p["id"]);
        println!("{}  {id}{}", s(&p["title"]), p["key"].as_str().map(|k| format!("  ({k})")).unwrap_or_default());
        for i in w["intents"].as_array().into_iter().flatten().filter(|i| s(&i["project"]) == id && s(&i["state"]) == "active") {
            let at = i["workspace"]["label"].as_str().map(|l| format!("  (in {l})")).unwrap_or_default();
            println!("  active  {} {}{at}", s(&i["id"]), s(&i["title"]));
        }
        let road = roadmap(&w, id);
        if !road.is_empty() {
            println!("  roadmap:");
        }
        for (k, i) in road.iter().enumerate() {
            println!("    {}. {} {} — {}{}", k + 1, s(&i["id"]), s(&i["title"]), s(&i["goal"]), external(i));
        }
        let open = deferred(&w, id);
        if !open.is_empty() {
            println!("  deferred findings:");
        }
        for f in open {
            let scheduled = if s(&f.finding["state"]) == "scheduled" { "  (scheduled)" } else { "" };
            println!("    {} {}: {}{}{scheduled}", s(&f.finding["severity"]), f.at, s(&f.finding["problem"]), external(f.finding));
        }
    }
    Ok(())
}

fn export(d: &Daemon, project: &str) -> anyhow::Result<()> {
    let w = d.get("/api/workspace?light=true")?;
    let Some(project) = folder(&w, project) else { bail!("no project {project}; see `workriver project list`") };
    let project = project.as_str();
    let p = w["projects"].as_array().into_iter().flatten().find(|p| s(&p["id"]) == project).expect("found above");
    println!("# {} backlog\n", s(&p["title"]));
    println!("Exported from workriver at {}. A read-only copy: change the backlog with `workriver backlog …` or in the web UI.\n", s(&w["builtAt"]));
    println!("## Roadmap\n");
    for (k, i) in roadmap(&w, project).iter().enumerate() {
        let link = match i.get("external") {
            Some(e) if e.is_object() => match e["url"].as_str() {
                Some(u) => format!(" ([{}:{}]({u}))", s(&e["system"]), s(&e["key"])),
                None => format!(" ({}:{})", s(&e["system"]), s(&e["key"])),
            },
            _ => String::new(),
        };
        println!("{}. **{}** ({}){link}", k + 1, s(&i["title"]), s(&i["id"]));
        println!("   {}", s(&i["goal"]));
        for line in s(&i["note"]).lines() {
            println!("   {line}");
        }
    }
    println!("\n## Deferred findings\n");
    for f in deferred(&w, project) {
        println!("- {} {}: {}{}", s(&f.finding["severity"]), f.at, s(&f.finding["problem"]), external(f.finding));
        if let Some(n) = f.finding["note"].as_str() {
            for line in n.lines() {
                println!("  {line}");
            }
        }
    }
    Ok(())
}

/// The commands of intents, units, projects, workspaces and the backlog: what each records (printed by `run` as before), or None when it
/// printed what it had to say itself.
pub(super) fn run(cmd: Cmd, cx: &Cx) -> anyhow::Result<Option<Value>> {
    let Cx { d, actor, here, group, here_intent } = *cx;
    Ok(Some(match cmd {
        Cmd::Manager(ManagerCmd::Here { intent, goal }) => {
            if group.is_none() && intent.is_none() {
                bail!("not inside a Herdr workspace: run `workriver manager here` in the manager's Herdr pane");
            }
            d.append(actor, "manager.registered", json!({ "intent": intent, "workspace": group }))?;
            println!("manager of workspace {}", group.as_ref().and_then(|g| g["label"].as_str()).or(intent.as_deref()).unwrap_or("?"));
            if let Some(goal) = goal {
                // ponytail: kept one release for old prompts; remove with --goal.
                eprintln!("--goal is deprecated: add intents with `workriver backlog add` and start them with `workriver intent start <intent>`.");
                let current = here.as_ref().and_then(|h| h["intent"].as_str().map(str::to_string));
                match current {
                    Some(i) => {
                        d.append(actor, "intent.updated", json!({ "intent": i, "goal": goal }))?;
                    }
                    None => {
                        let title = group.as_ref().and_then(|g| g["label"].as_str()).unwrap_or("intent").to_string();
                        let made = d.append(actor, "intent.created", json!({ "title": title, "goal": goal, "project": cwd_folder()?, "state": "planned" }))?;
                        let i = made["body"]["id"].as_str().unwrap_or_default().to_string();
                        d.append(actor, "intent.state_changed", json!({ "intent": i, "state": "active", "workspace": group }))?;
                        println!("started {i}");
                    }
                }
            }
            return Ok(None);
        }
        Cmd::Workspace(WorkspaceCmd::List) => {
            let w = d.get("/api/workspace?light=true")?;
            for g in w["workspaces"].as_array().into_iter().flatten() {
                let label = g["label"].as_str().unwrap_or("?");
                match (g["machine"].as_str(), g["linkedTo"].as_str()) {
                    (None, _) => match (g["projectTitle"].as_str(), g["projectFrom"].as_str()) {
                        (Some(p), Some(from)) => println!("{label}  works on {p} ({from})"),
                        _ => println!("{label}  project unknown"),
                    },
                    (Some(m), Some(to)) => println!("{m}:{label}  linked to {to}"),
                    (Some(m), None) => println!("{m}:{label}  not linked"),
                }
            }
            return Ok(None);
        }
        Cmd::Workspace(WorkspaceCmd::Link { remote, local }) => {
            let (machine, workspace) = remote.split_once(':').ok_or_else(|| anyhow!("write the remote workspace as <machine>:<workspace>, as `workriver workspace list` shows it"))?;
            d.append(actor, "workspace.linked", json!({ "machine": machine, "workspace": workspace, "local": local }))?
        }
        Cmd::Workspace(WorkspaceCmd::Unlink { remote }) => {
            let (machine, workspace) = remote.split_once(':').ok_or_else(|| anyhow!("write the remote workspace as <machine>:<workspace>"))?;
            d.append(actor, "workspace.unlinked", json!({ "machine": machine, "workspace": workspace }))?
        }
        Cmd::Workspace(WorkspaceCmd::Project { workspace, project, clear: _ }) => {
            let w = d.get("/api/workspace?light=true")?;
            let g = w["workspaces"].as_array().into_iter().flatten().find(|g| g["label"] == workspace.as_str() && g["machine"].is_null());
            let g = g.ok_or_else(|| anyhow!("no workspace {workspace} on this machine; see `workriver workspace list`"))?;
            d.append(actor, "workspace.project_set", json!({ "workspace": { "id": g["id"], "label": g["label"] }, "project": project.as_deref().map(project_arg).transpose()? }))?
        }
        Cmd::Project(ProjectCmd::Add { folder, title }) => {
            let folder = std::fs::canonicalize(&folder).with_context(|| format!("no folder {folder}"))?;
            d.append(actor, "project.added", json!({ "path": folder, "title": title }))?
        }
        Cmd::Project(ProjectCmd::Rename { project, title }) => d.append(actor, "project.renamed", json!({ "project": project_arg(&project)?, "title": title }))?,
        Cmd::Project(ProjectCmd::List) => {
            let w = d.get("/api/workspace?light=true")?;
            for p in w["projects"].as_array().into_iter().flatten() {
                let kind = if p["registered"] == true { "" } else { "  (a repository outside every project)" };
                let key = p["key"].as_str().map(|k| format!("{k} ")).unwrap_or_default();
                println!("{key}{}  {}{kind}", p["title"].as_str().unwrap_or("?"), p["id"].as_str().unwrap_or("?"));
                for r in p["repos"].as_array().into_iter().flatten() {
                    println!("  {}", r.as_str().unwrap_or("?"));
                }
            }
            return Ok(None);
        }
        Cmd::Backlog(BacklogCmd::Add { title, goal, project, note, external, url }) => d.append(
            actor,
            "intent.created",
            json!({ "title": title, "goal": goal, "project": project_arg(&project)?, "state": "planned", "note": note, "external": parse_external(external, url)? }),
        )?,
        Cmd::Backlog(BacklogCmd::AddFinding { project, severity, problem, impact, r#where, source, note, intent, external, url }) => d.append(
            actor,
            "finding.added",
            json!({
                "project": project_arg(&project)?, "intent": intent, "source": source, "note": note,
                "finding": { "severity": severity, "problem": problem, "impact": impact, "where": r#where, "external": parse_external(external, url)? },
            }),
        )?,
        Cmd::Backlog(BacklogCmd::List { project }) => return list(d, project.as_deref().map(project_arg).transpose()?.as_deref()).map(|()| None),
        Cmd::Backlog(BacklogCmd::Export { project }) => return export(d, &project_arg(&project)?).map(|()| None),
        Cmd::Backlog(BacklogCmd::Move { intent, before, after }) => d.append(actor, "intent.moved", json!({ "intent": intent, "before": before, "after": after }))?,
        Cmd::Backlog(BacklogCmd::Merge { into, from, title }) => d.call("/api/backlog/merge", Some(json!({ "into": into, "from": from, "title": title })))?,
        Cmd::Intent(IntentCmd::Start { intent }) => {
            let Some(g) = &group else { bail!("run `workriver intent start` in the manager's Herdr pane: the intent starts in that workspace") };
            let out = d.append(actor, "intent.state_changed", json!({ "intent": intent, "state": "active", "workspace": g }))?;
            if let Some(w) = out["warning"].as_str() {
                eprintln!("warning: {w}");
            }
            out
        }
        Cmd::Intent(IntentCmd::Done { intent }) => d.append(actor, "intent.state_changed", json!({ "intent": intent, "state": "done" }))?,
        Cmd::Intent(IntentCmd::Drop { intent, reason }) => d.append(actor, "intent.state_changed", json!({ "intent": intent, "state": "dropped", "reason": reason }))?,
        Cmd::Intent(IntentCmd::Add { title, goal, project }) => d.append(actor, "intent.created", json!({ "title": title, "goal": goal, "project": project_arg(&project)?, "state": "planned" }))?,
        Cmd::Intent(IntentCmd::Set { intent, title, goal, findings, add_note }) => d.append(actor, "intent.updated", json!({ "intent": intent, "title": title, "goal": goal, "findings": findings, "add_note": add_note }))?,
        Cmd::Unit(UnitCmd::Add { intent, kind, title, purpose, repo, branch, base, machine }) if kind == "research" => {
            if repo.is_some() || branch.is_some() || base.is_some() || machine.is_some() {
                bail!("a research unit has no repository, branch, base or machine: leave out --repo, --branch, --base and --machine");
            }
            let intent = here_intent(intent)?;
            d.append(actor, "unit.created", json!({ "intent": intent, "title": title, "purpose": purpose, "kind": "research" }))?
        }
        Cmd::Unit(UnitCmd::Add { intent, kind, title, purpose, repo, branch, base, machine }) => {
            let (repo, branch, base) = (repo.unwrap_or_else(|| ".".into()), branch.unwrap_or_default(), base.unwrap_or_else(|| "main".into()));
            let intent = here_intent(intent)?;
            // On another machine the path is that machine's: taken as written, checked there.
            let repo = match &machine {
                Some(m) if !Path::new(&repo).is_absolute() => bail!("--repo is a path on {m}: give it in full (it cannot be told from here)"),
                Some(_) => json!(repo),
                None => json!(std::fs::canonicalize(&repo).with_context(|| format!("no folder {repo}"))?),
            };
            d.append(actor, "unit.created", json!({ "intent": intent, "title": title, "purpose": purpose, "repo": repo, "branch": branch, "base": base, "machine": machine, "kind": kind }))?
        }
        Cmd::Unit(UnitCmd::Move { unit, machine, repo, branch, base }) => {
            if machine.is_none() && repo.is_none() && branch.is_none() && base.is_none() {
                bail!("nothing to move: give --machine, --repo, --branch or --base");
            }
            d.append(actor, "unit.relocated", json!({ "unit": unit, "machine": machine, "repo": repo.unwrap_or_default(), "branch": branch.unwrap_or_default(), "base": base.unwrap_or_default() }))?
        }
        Cmd::Unit(UnitCmd::Merged { unit }) => d.append(actor, "unit.merged", json!({ "unit": unit }))?,
        Cmd::Unit(UnitCmd::Done { unit, reason }) => d.append(actor, "unit.done", json!({ "unit": unit, "reason": reason }))?,
        _ => unreachable!("not one of intents, units, projects, workspaces and the backlog"),
    }))
}

#[derive(Subcommand)]
pub(crate) enum IntentCmd {
    /// Record a new planned intent (as `backlog add`); prints its id.
    Add {
        title: String,
        /// One sentence on what it is for.
        #[arg(long, default_value = "")]
        goal: String,
        /// The project (p1, or a folder); default: the current directory's.
        #[arg(long, default_value = ".")]
        project: String,
    },
    /// Start a planned intent in this Herdr workspace (run by its manager, in its pane): it
    /// becomes active and the workspace's current intent, where new units and tasks go.
    Start { intent: String },
    /// Mark an active intent done.
    Done { intent: String },
    /// Drop a planned or active intent.
    Drop {
        intent: String,
        #[arg(long)]
        reason: String,
    },
    /// Change an intent's title or goal, or add to its note (its manager or the user only), or
    /// who decides its findings (the user only).
    Set {
        /// The intent (i1…).
        intent: String,
        #[arg(long)]
        title: Option<String>,
        /// One sentence on what it is for.
        #[arg(long)]
        goal: Option<String>,
        /// Who decides its open findings: its manager (the default), or only you. You can
        /// always decide any finding yourself. Only the user sets this.
        #[arg(long, value_parser = ["manager", "user"])]
        findings: Option<String>,
        /// A paragraph to add at the end of its note (a blank line before it): what was
        /// learnt or decided about the intent as a whole, links welcome.
        #[arg(long, value_name = "TEXT")]
        add_note: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum WorkspaceCmd {
    /// Every workspace per machine, with its link.
    List,
    /// Make a workspace of another machine one with a workspace of this machine: one
    /// manager (wherever it runs), the same intents.
    Link {
        /// `<machine>:<workspace label>`.
        remote: String,
        /// The label of the workspace on this machine.
        local: String,
    },
    Unlink {
        /// `<machine>:<workspace label>`.
        remote: String,
    },
    /// Say which project a workspace of this machine works on, over what workriver works out
    /// (from where its manager runs, else its current intent). The user only.
    Project {
        /// The workspace's label.
        workspace: String,
        /// The project (p1, or a folder as `workriver project list` shows it).
        #[arg(required_unless_present = "clear")]
        project: Option<String>,
        /// Forget what was set: workriver works it out again.
        #[arg(long, conflicts_with = "project")]
        clear: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum ProjectCmd {
    /// Register a folder as a project: its repositories are its direct subfolders with a
    /// .git, or the folder itself. A repository outside every project is a project of its own.
    Add {
        folder: String,
        /// Default: the folder's name.
        #[arg(long, default_value = "")]
        title: String,
    },
    /// List the projects and their repositories.
    List,
    /// Change a project's title. The user only.
    Rename {
        /// Its id (p1…) or its folder.
        project: String,
        title: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum BacklogCmd {
    /// Add a planned intent at the end of a project's roadmap; prints its id.
    Add {
        title: String,
        /// One sentence on what it is for.
        #[arg(long)]
        goal: String,
        /// The project (p1, or a folder); default: the current directory's.
        #[arg(long, default_value = ".")]
        project: String,
        /// More about it; may span lines and hold links.
        #[arg(long, default_value = "")]
        note: String,
        /// Where it is tracked elsewhere: `SYSTEM:KEY`, e.g. `jira:ABC-12`.
        #[arg(long, value_name = "SYSTEM:KEY")]
        external: Option<String>,
        /// The link to it there.
        #[arg(long, requires = "external")]
        url: Option<String>,
    },
    /// Add a deferred finding that came from outside workriver (an earlier review, a notes
    /// file); prints its id (f1…).
    AddFinding {
        #[arg(long, default_value = ".")]
        project: String,
        #[arg(long, value_parser = ["P1", "P2", "P3"])]
        severity: String,
        #[arg(long)]
        problem: String,
        /// What happens if it is left.
        #[arg(long, default_value = "")]
        impact: String,
        /// A path that shows it. Repeatable.
        #[arg(long = "where", value_name = "PATH")]
        r#where: Vec<String>,
        /// Where it came from, in words: which review, branch, date.
        #[arg(long, default_value = "")]
        source: String,
        /// How to fix it, or anything else; may span lines.
        #[arg(long, default_value = "")]
        note: String,
        /// The intent it is meant for.
        #[arg(long)]
        intent: Option<String>,
        #[arg(long, value_name = "SYSTEM:KEY")]
        external: Option<String>,
        #[arg(long, requires = "external")]
        url: Option<String>,
    },
    /// Print a project's roadmap and deferred findings.
    List {
        /// Default: every project.
        #[arg(long)]
        project: Option<String>,
    },
    /// Move a planned intent before or after another in the roadmap.
    Move {
        intent: String,
        #[arg(long, conflicts_with = "after", required_unless_present = "after")]
        before: Option<String>,
        #[arg(long)]
        after: Option<String>,
    },
    /// Merge planned intents into one (a roadmap split too finely): the others are dropped and
    /// their title, goal, note and external id go into its note. The user's alone.
    Merge {
        /// The planned intent that stays.
        into: String,
        /// The planned intents merged into it, in the order their text is added.
        #[arg(required = true)]
        from: Vec<String>,
        /// A new title for the intent that stays (the merged whole may need one).
        #[arg(long)]
        title: Option<String>,
    },
    /// Print a project's backlog as Markdown (a read-only copy; the records stay in workriver).
    Export {
        #[arg(long, default_value = ".")]
        project: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum UnitCmd {
    /// Record a unit of an intent; prints its id (u1…).
    ///
    /// `--kind research`: research and design for the intent, with no repository, branch or
    /// machine (leave those out). It takes research, design, review and gate tasks, is never
    /// merged, and is closed with `workriver unit done` once its work is delivered and decided.
    ///
    /// `--kind spike`: an experiment (measure, compare, try an approach) whose code is kept on
    /// a branch of its own, spike/<name>, and shown like a code unit's, but never merged; it
    /// takes every task kind and is closed with `workriver unit done --reason "<what was learned>"`.
    Add {
        /// The intent it serves (i1…); default: the current intent of this Herdr workspace.
        intent: Option<String>,
        /// code: work on a branch of its own; research: research and design, no branch;
        /// spike: an experiment on a spike/ branch, never merged.
        #[arg(long, value_parser = ["code", "research", "spike"], default_value = "code")]
        kind: String,
        #[arg(long)]
        title: String,
        /// One sentence on what it is for.
        #[arg(long, default_value = "")]
        purpose: String,
        /// The repository (or a worktree of it); default: the current directory.
        #[arg(long)]
        repo: Option<String>,
        /// The unit's own branch, where the work happens; it may not exist yet. It starts
        /// with feat/, chore/ or fix/ (feat/mention), or spike/ for --kind spike, and is not
        /// the base. Needed for code and spikes.
        #[arg(long)]
        branch: Option<String>,
        /// The branch it starts from and merges into; default: main.
        #[arg(long)]
        base: Option<String>,
        /// The machine the work happens on, when it is not the one this runs on: a machine
        /// of `workriver machine list` (or `local`: the one workriver runs on). --repo and
        /// --branch are then the path and the branch as they are on that machine.
        #[arg(long, value_name = "LABEL")]
        machine: Option<String>,
    },
    /// Record that a unit's branch was merged. Not needed after a fast-forward or a merge
    /// commit: workriver sees those and records them itself; a squash merge it cannot see.
    Merged { unit: String },
    /// Close a unit finished without a merge: research or spike done, abandoned, superseded. By the
    /// manager of its intent, or the user.
    Done {
        unit: String,
        /// Why (finished, abandoned, superseded by u7…).
        #[arg(long, alias = "note", default_value = "")]
        reason: String,
    },
    /// Say where a unit's work really is: another machine, path, branch or base. What is not
    /// given stays. By the manager of its intent, or the user; checked on that machine.
    Move {
        unit: String,
        /// A machine of `workriver machine list`, or `local`: the one workriver runs on.
        #[arg(long, value_name = "LABEL")]
        machine: Option<String>,
        /// The repository's path on that machine.
        #[arg(long)]
        repo: Option<String>,
        /// The branch as it is named there. A new name starts with feat/, chore/ or fix/
        /// and is not the base.
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        base: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum ManagerCmd {
    /// Register the agent of this Herdr pane as the manager of its workspace. Decisions,
    /// gate results and batches of deferred findings for the intents active there are
    /// delivered to it.
    Here {
        /// Before 0.2: the intent to manage, when not run inside a Herdr workspace.
        #[arg(long, hide = true)]
        intent: Option<String>,
        /// Deprecated: creates and starts an intent with this goal. Use `backlog add` and
        /// `intent start`.
        #[arg(long)]
        goal: Option<String>,
    },
    /// Tell the manager of a task's intent something, from a worker: typed into the
    /// manager's terminal, quoted, while it runs. The way to reach your manager (never type
    /// into its terminal yourself). Stuck on what only the user can do: put a blocker
    /// instead (`workriver guide how blocked`); the manager is told of it.
    Tell {
        /// The task it is about (u1#3): yours, usually.
        task: String,
        #[arg(long)]
        text: String,
    },
}
