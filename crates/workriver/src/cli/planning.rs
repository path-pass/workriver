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

/// An intent's kind, size and tags as printed after its title: ` [fix S] security, cli`
/// (`[-]` when it has tags only); empty when it has none.
pub(super) fn labels(i: &Value) -> String {
    let head = [i["kind"].as_str().map(str::to_string), i["size"].as_str().map(str::to_uppercase)].into_iter().flatten().collect::<Vec<_>>().join(" ");
    let tags = i["tags"].as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>().join(", ");
    match (head.is_empty(), tags.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!(" [{head}]"),
        (true, false) => format!(" [-] {tags}"),
        (false, false) => format!(" [{head}] {tags}"),
    }
}

/// What `backlog list` narrows to: within each of kind, size and tag any one given, all three
/// together; nothing given, every intent.
#[derive(Default)]
struct Filter {
    kinds: Vec<String>,
    sizes: Vec<String>,
    tags: Vec<String>,
}

impl Filter {
    fn keeps(&self, i: &Value) -> bool {
        let has = |list: &[String], v: Option<&str>| list.is_empty() || v.is_some_and(|v| list.iter().any(|x| x == v));
        let tags: Vec<&str> = i["tags"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        has(&self.kinds, i["kind"].as_str()) && has(&self.sizes, i["size"].as_str()) && (self.tags.is_empty() || self.tags.iter().any(|t| tags.contains(&t.as_str())))
    }
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

fn list(d: &Daemon, project: Option<&str>, only: &Filter) -> anyhow::Result<()> {
    let w = d.get("/api/workspace?light=true")?;
    let wanted = project.map(|p| folder(&w, p).unwrap_or_default());
    let projects: Vec<&Value> = w["projects"].as_array().into_iter().flatten().filter(|p| wanted.as_deref().is_none_or(|x| s(&p["id"]) == x)).collect();
    if projects.is_empty() {
        bail!("no project {}; see `workriver project list`", project.unwrap_or("yet"));
    }
    for p in projects {
        let id = s(&p["id"]);
        println!("{}  {id}{}", s(&p["title"]), p["key"].as_str().map(|k| format!("  ({k})")).unwrap_or_default());
        for i in w["intents"].as_array().into_iter().flatten().filter(|i| s(&i["project"]) == id && s(&i["state"]) == "active" && only.keeps(i)) {
            let at = i["workspace"].as_str().map(|l| format!("  (in {l})")).unwrap_or_default();
            println!("  active  {} {}{}{at}", s(&i["id"]), s(&i["title"]), labels(i));
        }
        // Numbered by their place in the whole roadmap, also when some are filtered out.
        let road: Vec<(usize, &Value)> = roadmap(&w, id).into_iter().enumerate().filter(|(_, i)| only.keeps(i)).collect();
        if !road.is_empty() {
            println!("  roadmap:");
        }
        for (k, i) in &road {
            println!("    {}. {} {}{} — {}{}", k + 1, s(&i["id"]), s(&i["title"]), labels(i), s(&i["goal"]), external(i));
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
        println!("{}. **{}** ({}){}{link}", k + 1, s(&i["title"]), s(&i["id"]), labels(i));
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
    let Cx { d, actor, here_intent } = *cx;
    Ok(Some(match cmd {
        // Gone: nobody becomes a manager by saying so.
        Cmd::Manager(ManagerCmd::Here { .. }) => bail!("`workriver manager here` is gone: an agent does not make itself a manager. {}. Tell the user which workspace you are meant for; once seated you are told in this terminal", workriver_core::SEATING),
        Cmd::Workspace(WorkspaceCmd::List) => {
            let w = d.get("/api/workspace?light=true")?;
            let title = |folder: &Value| w["projects"].as_array().into_iter().flatten().find(|p| &p["id"] == folder).map(|p| format!("{} ({})", s(&p["title"]), s(&p["key"]))).unwrap_or_else(|| "?".into());
            for g in w["workspaces"].as_array().into_iter().flatten() {
                let manager = match &g["manager"] {
                    m @ Value::Object(_) => format!("manager {} ({}{})", s(&m["name"]), s(&m["state"]), m["machine"].as_str().map(|x| format!(", on {x}")).unwrap_or_default()),
                    _ => "no manager".into(),
                };
                let current = g["current"].as_str().map(|i| format!("  current {i}")).unwrap_or_default();
                println!("{}  {}  project {}  {manager}{current}{}", s(&g["id"]), s(&g["name"]), title(&g["project"]), if g["closed"] == true { "  (closed)" } else { "" });
            }
            return Ok(None);
        }
        Cmd::Workspace(WorkspaceCmd::Add { project, name }) => d.call("/api/workspaces", Some(json!({ "action": "add", "project": project_arg(&project)?, "name": name })))?,
        Cmd::Workspace(WorkspaceCmd::Rename { workspace, name }) => d.call("/api/workspaces", Some(json!({ "action": "rename", "workspace": workspace, "name": name })))?,
        Cmd::Workspace(WorkspaceCmd::Close { workspace }) => d.call("/api/workspaces", Some(json!({ "action": "close", "workspace": workspace })))?,
        Cmd::Workspace(WorkspaceCmd::Open { workspace }) => d.call("/api/workspaces", Some(json!({ "action": "open", "workspace": workspace })))?,
        Cmd::Workspace(WorkspaceCmd::Manager { workspace, agent, none: _ }) => {
            // The seat as it is read now is the one this replaces: changed meanwhile, refused.
            let w = d.get("/api/workspace?light=true")?;
            let Some(g) = w["workspaces"].as_array().into_iter().flatten().find(|g| g["id"] == workspace.as_str()) else { bail!("no workspace {workspace}; see `workriver workspace list`") };
            let replaces = g["manager"].as_object().map(|m| json!({ "session": m["session"], "since": m["since"] }));
            d.call("/api/workspaces", Some(json!({ "action": "manager", "workspace": workspace, "manager": agent, "replaces": replaces })))?
        }
        Cmd::Project(ProjectCmd::Add { folder, title }) => {
            let folder = std::fs::canonicalize(&folder).with_context(|| format!("no folder {folder}"))?;
            d.append(actor, "project.added", json!({ "path": folder, "title": title }))?
        }
        Cmd::Project(ProjectCmd::Rename { project, title }) => d.append(actor, "project.renamed", json!({ "project": project_arg(&project)?, "title": title }))?,
        Cmd::Project(ProjectCmd::List) => {
            let w = d.get("/api/workspace?light=true")?;
            for p in w["projects"].as_array().into_iter().flatten() {
                let key = p["key"].as_str().map(|k| format!("{k} ")).unwrap_or_default();
                println!("{key}{}  {}", p["title"].as_str().unwrap_or("?"), p["id"].as_str().unwrap_or("?"));
                for r in p["repos"].as_array().into_iter().flatten() {
                    println!("  {}", r.as_str().unwrap_or("?"));
                }
            }
            return Ok(None);
        }
        Cmd::Backlog(BacklogCmd::Add { title, goal, project, note, external, url, sort }) => d.append(
            actor,
            "intent.created",
            json!({ "title": title, "goal": goal, "project": project_arg(&project)?, "state": "planned", "note": note, "external": parse_external(external, url)?, "kind": sort.kind, "size": sort.size.map(|z| z.to_lowercase()), "tags": sort.tags }),
        )?,
        Cmd::Backlog(BacklogCmd::AddFinding { project, severity, problem, impact, r#where, source, note, intent, external, url }) => d.append(
            actor,
            "finding.added",
            json!({
                "project": project_arg(&project)?, "intent": intent, "source": source, "note": note,
                "finding": { "severity": severity, "problem": problem, "impact": impact, "where": r#where, "external": parse_external(external, url)? },
            }),
        )?,
        Cmd::Backlog(BacklogCmd::List { project, kind, size, tag }) => {
            let only = Filter { kinds: kind, sizes: size.iter().map(|z| z.to_lowercase()).collect(), tags: tag.iter().map(|t| t.trim().to_lowercase()).collect() };
            return list(d, project.as_deref().map(project_arg).transpose()?.as_deref(), &only).map(|()| None);
        }
        Cmd::Backlog(BacklogCmd::Export { project }) => return export(d, &project_arg(&project)?).map(|()| None),
        Cmd::Backlog(BacklogCmd::Move { intent, before, after }) => d.append(actor, "intent.moved", json!({ "intent": intent, "before": before, "after": after }))?,
        Cmd::Backlog(BacklogCmd::Merge { into, from, title }) => d.call("/api/backlog/merge", Some(json!({ "into": into, "from": from, "title": title })))?,
        // A manager's start names nothing: it starts in the workspace it holds the seat of,
        // which the rules fill in. The user says where (or it is the one open workspace of
        // the intent's project).
        Cmd::Intent(IntentCmd::Start { intent, workspace }) => d.append(actor, "intent.state_changed", json!({ "intent": intent, "state": "active", "workspace": workspace }))?,
        Cmd::Intent(IntentCmd::Move { intent, workspace }) => d.call("/api/workspaces", Some(json!({ "action": "move", "intent": intent, "workspace": workspace })))?,
        Cmd::Intent(IntentCmd::Done { intent }) => d.append(actor, "intent.state_changed", json!({ "intent": intent, "state": "done" }))?,
        Cmd::Intent(IntentCmd::Drop { intent, reason }) => d.append(actor, "intent.state_changed", json!({ "intent": intent, "state": "dropped", "reason": reason }))?,
        Cmd::Intent(IntentCmd::Add { title, goal, project, sort }) => d.append(
            actor,
            "intent.created",
            json!({ "title": title, "goal": goal, "project": project_arg(&project)?, "state": "planned", "kind": sort.kind, "size": sort.size.map(|z| z.to_lowercase()), "tags": sort.tags }),
        )?,
        Cmd::Intent(IntentCmd::Set { intent, title, goal, findings, add_note, note, base, kind, size, tags }) => {
            if base.is_none() && (title.is_some() || goal.is_some() || note.is_some() || kind.is_some() || size.is_some() || tags.is_some()) {
                bail!("give --base N, the version you read (`workriver intent history {intent}` shows it): --title, --goal, --note, --kind, --size and --tags replace what is there, and are refused when it changed since");
            }
            let mut body = json!({ "intent": intent, "title": title, "goal": goal, "findings": findings, "add_note": add_note, "note": note, "base": base });
            // Only what was given: a null kind or size clears it, an empty list the tags.
            let none = |v: &str| v.eq_ignore_ascii_case("none");
            if let Some(k) = kind {
                body["kind"] = if none(&k) { Value::Null } else { json!(k) };
            }
            if let Some(z) = size {
                body["size"] = if none(&z) { Value::Null } else { json!(z.to_lowercase()) };
            }
            if let Some(t) = tags {
                // None is the one clear: a list that names no tag (empty, only commas) is a
                // mistake, not a wish to drop them all.
                let given: Vec<&str> = if none(&t) { vec![] } else { t.split(',').map(str::trim).filter(|t| !t.is_empty()).collect() };
                if given.is_empty() && !none(&t) {
                    bail!("--tags names no tag ({t:?}): give them comma-separated (--tags security,cli), or none to clear them");
                }
                body["tags"] = json!(given);
            }
            d.append(actor, "intent.updated", body)?
        }
        Cmd::Intent(IntentCmd::History { intent, version, diff }) => {
            print!("{}", intent_history_text(d, &intent, version, diff)?);
            return Ok(None);
        }
        Cmd::Unit(UnitCmd::Add { intent, kind, title, purpose, repo, branch, base, machine, template }) if kind == "research" => {
            if repo.is_some() || branch.is_some() || base.is_some() || machine.is_some() {
                bail!("a research unit has no repository, branch, base or machine: leave out --repo, --branch, --base and --machine");
            }
            let intent = here_intent(intent)?;
            d.append(actor, "unit.created", json!({ "intent": intent, "title": title, "purpose": purpose, "kind": "research", "template": template.unwrap_or_default() }))?
        }
        Cmd::Unit(UnitCmd::Add { intent, kind, title, purpose, repo, branch, base, machine, template }) => {
            let (repo, branch, base) = (repo.unwrap_or_else(|| ".".into()), branch.unwrap_or_default(), base.unwrap_or_else(|| "main".into()));
            let intent = here_intent(intent)?;
            // On another machine the path is that machine's: taken as written, checked there.
            let repo = match &machine {
                Some(m) if !Path::new(&repo).is_absolute() => bail!("--repo is a path on {m}: give it in full (it cannot be told from here)"),
                Some(_) => json!(repo),
                None => json!(std::fs::canonicalize(&repo).with_context(|| format!("no folder {repo}"))?),
            };
            d.append(actor, "unit.created", json!({ "intent": intent, "title": title, "purpose": purpose, "repo": repo, "branch": branch, "base": base, "machine": machine, "kind": kind, "template": template.unwrap_or_default() }))?
        }
        Cmd::Unit(UnitCmd::Move { unit, machine, repo, branch, base }) => {
            if machine.is_none() && repo.is_none() && branch.is_none() && base.is_none() {
                bail!("nothing to move: give --machine, --repo, --branch or --base");
            }
            d.append(actor, "unit.relocated", json!({ "unit": unit, "machine": machine, "repo": repo.unwrap_or_default(), "branch": branch.unwrap_or_default(), "base": base.unwrap_or_default() }))?
        }
        Cmd::Unit(UnitCmd::Merged { unit }) => d.append(actor, "unit.merged", json!({ "unit": unit }))?,
        // The library, as the service gives it: the user's and the built-ins; no copy here.
        Cmd::Template(TemplateCmd::List { unit }) => {
            let list: Vec<core::LibraryTemplate> = serde_json::from_value(d.get("/api/state")?["templates"].clone())?;
            let unit = unit.map(|u| serde_json::from_value::<core::UnitKind>(json!(u))).transpose()?;
            print!("{}", templates_text(&list, unit));
            return Ok(None);
        }
        Cmd::Template(TemplateCmd::Show { name, yaml }) => {
            print!("{}", template_show_text(d, &name, yaml)?);
            return Ok(None);
        }
        Cmd::Template(TemplateCmd::Add { file }) => {
            let text = std::fs::read_to_string(&file).with_context(|| format!("cannot read {file}"))?;
            d.call("/api/template", Some(json!({ "text": text })))?
        }
        Cmd::Template(TemplateCmd::Remove { name }) => d.call("/api/template/remove", Some(json!({ "name": name })))?,
        Cmd::Unit(UnitCmd::Done { unit, reason }) => d.append(actor, "unit.done", json!({ "unit": unit, "reason": reason }))?,
        _ => unreachable!("not one of intents, units, projects, workspaces and the backlog"),
    }))
}

/// What `template show` prints: the template as /api/state gives it (as `template list`
/// reads the library, so a CLI on a followed machine shows the steps this service places), or
/// with `yaml` its text as saved, which is for editing and read only on this machine
/// (/api/template is not on a machine's listener).
pub(crate) fn template_show_text(d: &Daemon, name: &str, yaml: bool) -> anyhow::Result<String> {
    if yaml {
        let shown = d.get(&format!("/api/template?name={name}"))?;
        return Ok(shown["text"].as_str().unwrap_or_default().to_string());
    }
    let list: Vec<core::LibraryTemplate> = serde_json::from_value(d.get("/api/state")?["templates"].clone())?;
    match list.iter().find(|t| t.template.name == name) {
        Some(t) => Ok(template_text(t)),
        None => bail!("no template {name}: `workriver template list` lists them"),
    }
}

/// A new intent's kind, size and tags (`backlog add`, `intent add`).
#[derive(clap::Args)]
pub(crate) struct Sort {
    /// What kind of work it is: feature (new behaviour), fix (wrong behaviour: a bug, a
    /// hole, a flaky test), chore (no new behaviour: refactor, docs, build, release),
    /// research (the outcome is a decision or a report).
    #[arg(long, value_parser = ["feature", "fix", "chore", "research"])]
    kind: Option<String>,
    /// How big it is expected to be: s (one unit, hours), m (one or two units, a day or
    /// two), l (three units or more, or a design first; days).
    #[arg(long, value_parser = ["s", "m", "l"], ignore_case = true)]
    size: Option<String>,
    /// A word for its area or theme (security, cli); repeatable, at most 8. Kept lower-case;
    /// no space or comma.
    #[arg(long = "tag", value_name = "TAG")]
    tags: Vec<String>,
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
        #[command(flatten)]
        sort: Sort,
    },
    /// Start a planned intent: it becomes active and its workspace's current intent, where
    /// new units and tasks go. By the workspace's manager (it starts where it holds the seat,
    /// its project's intents only), or by the user.
    Start {
        intent: String,
        /// The user only: the workspace it starts in (ws1…); default: the one open workspace
        /// of the intent's project.
        #[arg(long)]
        workspace: Option<String>,
    },
    /// Move an active intent to another open workspace of its project. The user only.
    Move {
        intent: String,
        /// The workspace it runs in from now on (ws1…).
        workspace: String,
    },
    /// Mark an active intent done.
    Done { intent: String },
    /// Drop a planned or active intent.
    Drop {
        intent: String,
        #[arg(long)]
        reason: String,
    },
    /// Change an intent's title or goal (the user only), its note, kind, size or tags (its
    /// manager too while it is active; a manager of its project may add to a planned one's
    /// note), or who decides its findings (the user only). A replace needs --base: the
    /// version it was made from.
    Set {
        /// The intent (i1…).
        intent: String,
        #[arg(long)]
        title: Option<String>,
        /// One sentence on what it is for.
        #[arg(long)]
        goal: Option<String>,
        /// The whole note, in place of what it holds ("" empties it). Needs --base.
        #[arg(long, value_name = "TEXT", conflicts_with = "add_note")]
        note: Option<String>,
        /// The version you read (`workriver intent history <intent>`): a --title, --goal or
        /// --note is refused when what it replaces changed since.
        #[arg(long, value_name = "N")]
        base: Option<u32>,
        /// Who decides its open findings: its manager (the default), or only you. You can
        /// always decide any finding yourself. Only the user sets this.
        #[arg(long, value_parser = ["manager", "user"])]
        findings: Option<String>,
        /// A paragraph to add at the end of its note (a blank line before it): what was
        /// learnt or decided about the intent as a whole, links welcome.
        #[arg(long, value_name = "TEXT")]
        add_note: Option<String>,
        /// What kind of work it is (feature, fix, chore, research); none clears it. Needs --base.
        #[arg(long, value_parser = ["feature", "fix", "chore", "research", "none"])]
        kind: Option<String>,
        /// How big it is expected to be (s, m, l); none clears it. Needs --base.
        #[arg(long, value_parser = ["s", "m", "l", "none"], ignore_case = true)]
        size: Option<String>,
        /// Its tags, the whole set, comma-separated (security,cli); none clears them (a list
        /// that names no tag is refused). Needs --base.
        #[arg(long, value_name = "TAGS")]
        tags: Option<String>,
    },
    /// An intent's versions: who changed its title, goal or note and when; one whole, or
    /// what it changed.
    History {
        /// The intent (i1…).
        intent: String,
        /// Print version N whole.
        #[arg(long, value_name = "N", conflicts_with = "diff")]
        version: Option<u32>,
        /// Print what version N changed, against the one before it.
        #[arg(long, value_name = "N")]
        diff: Option<u32>,
    },
}

#[derive(Subcommand)]
pub(crate) enum WorkspaceCmd {
    /// Every workspace: its id, name, project, manager and current intent.
    List,
    /// Make another workspace in a project (each project gets its first with `project add`):
    /// for a big intent, or a conversation of its own. The user only.
    Add {
        /// The project (p1, or a folder as `workriver project list` shows it).
        project: String,
        /// Shown only; it may repeat.
        #[arg(long)]
        name: String,
    },
    /// The user only.
    Rename { workspace: String, name: String },
    /// Close a workspace with no active intent: it leaves the rail, its seat is emptied, its
    /// intents and conversation stay. The user only.
    Close { workspace: String },
    /// Open a closed workspace again. The user only.
    Open { workspace: String },
    /// Seat a workspace's manager: a running agent, which is told in its terminal. From the
    /// user's own terminal only; an agent is never made a manager by an agent.
    Manager {
        /// The workspace (ws1…).
        workspace: String,
        /// The agent: its session, or its name when only one running agent has it.
        #[arg(required_unless_present = "none")]
        agent: Option<String>,
        /// Empty the seat.
        #[arg(long, conflicts_with = "agent")]
        none: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum ProjectCmd {
    /// Register a folder as a project: its repositories are its direct subfolders with a
    /// .git, or the folder itself. A repository outside every project is in none until registered.
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
        #[command(flatten)]
        sort: Sort,
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
    /// Print a project's roadmap and deferred findings: each intent with its kind, size and
    /// tags (`[fix S] security, cli`).
    List {
        /// Default: every project.
        #[arg(long)]
        project: Option<String>,
        /// Only intents of this kind; repeatable (any of them).
        #[arg(long, value_parser = ["feature", "fix", "chore", "research"])]
        kind: Vec<String>,
        /// Only intents of this size; repeatable (any of them).
        #[arg(long, value_parser = ["s", "m", "l"], ignore_case = true)]
        size: Vec<String>,
        /// Only intents with this tag; repeatable (any of them).
        #[arg(long, value_name = "TAG")]
        tag: Vec<String>,
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
        /// A new title for the intent that stays (the merged whole may need one). The
        /// user's alone: an agent's merge keeps the title.
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
        #[arg(help = runtime::words::HELP_UNIT_INTENT)]
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
        /// The template whose steps the unit gets, placed as tasks to fill: one made for its
        /// kind of unit, chosen by its description (`workriver template list --unit <kind>`).
        /// Needed; it never changes afterwards.
        #[arg(long, value_name = "NAME")]
        template: Option<String>,
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
pub(crate) enum TemplateCmd {
    /// The templates a unit can be made with: the user's and the built-ins, each with the kind
    /// of unit it is for, its whole description (what it is for, when to choose it) and its
    /// steps.
    List {
        /// Only the templates for this kind of unit.
        #[arg(long, value_parser = ["code", "research", "spike"])]
        unit: Option<String>,
    },
    /// One template: its description, then each step with what it follows and what it is for.
    Show {
        name: String,
        /// The text as saved (a built-in's text when the user has none of that name).
        #[arg(long)]
        yaml: bool,
    },
    /// Save a template from a YAML file: the user's, from a terminal that is not an agent's.
    /// One of a built-in's name replaces it whole. A change touches only units made after it.
    ///
    /// The format: `name` (a-z, 0-9, -), `description`, `unit` (code, research or spike;
    /// default code), `steps`: each an `id`, a `kind` (implement, review, tests, gate, design,
    /// research), a `description`, and where it applies `reviewer` (a review: code-review,
    /// security-review, architecture-review), `gate` (a gate: decide, deploy, test) and `after`
    /// (step ids written before it; left out: the step before; `[]`: the start). Any other key
    /// is refused, with its line. `workriver template show development --yaml` is an example.
    Add {
        /// The YAML file.
        file: String,
    },
    /// Remove a template of the user's; one of a built-in's name brings the built-in back.
    /// Units made with it keep their steps.
    Remove { name: String },
}

#[derive(Subcommand)]
pub(crate) enum ManagerCmd {
    /// Gone: the user seats a manager (`workriver workspace manager`). Kept to say so.
    #[command(hide = true)]
    Here {
        #[arg(long, hide = true)]
        intent: Option<String>,
        #[arg(long, hide = true)]
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
