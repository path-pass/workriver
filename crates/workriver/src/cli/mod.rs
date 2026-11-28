// The `workriver` command line: the commands, how the CLI reaches the service, and what it
// prints. It runs in the CLI's process and imports nothing of the service.
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, anyhow, bail};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use workriver_core::{self as core, FindingState, TaskKind, TaskState, name, parse_task_ref};

use crate::common::{self, BUILD};
use crate::runtime;

mod client;
mod conversations;
mod display;
mod guide;
mod hosted;
mod machines;
mod parse;
mod planning;
mod work;

pub(crate) use client::*;
pub(crate) use conversations::MessageCmd;
#[cfg(test)]
pub(crate) use conversations::wait_answer;
pub(crate) use display::*;
pub(crate) use guide::*;
pub(crate) use hosted::AgentCmd;
pub(crate) use machines::*;
pub(crate) use parse::*;
pub(crate) use planning::*;
pub(crate) use work::*;

/// What every command has to hand: the client and who records (`--as`, else whoever the service
/// finds this to be).
#[derive(Clone, Copy)]
pub(super) struct Cx<'a> {
    pub(super) d: &'a Daemon,
    pub(super) actor: &'a str,
}

const ABOUT: &str = "Keep track of many coding agents working in parallel.

workriver records work as it happens (intents, units, tasks, findings, evidence) and shows
it in a browser at http://127.0.0.1:<port>. `workriver serve` runs the daemon; every other
command is a short call to it.

Terms:
  intent   a goal; it groups the units that serve it (ids i1, i2…)
  unit     a piece of an intent built on its own branch (ids u1, u2…)
  task     one piece of work in a unit, numbered per unit: u1#1, u1#2…
           kinds: implement, review, tests, gate (a step only the user can do)
  finding  a problem a review, tests task or gate found: P1-1, P2-1… within its task
  chore    something asked of the user that blocks nobody (ids c1, c2…)

";

/// After who records (the runtime's paragraph, between the two).
const ABOUT_END: &str = "

Commits are read from git: a commit belongs to a task when its message has the trailer
line `Task: u1#3` in its last paragraph (with any other trailers, no blank line between
them). A commit on the unit's branch without one shows as unregistered.

Typical flow for an agent:
  workriver task start u1#3
  …work, commit with `Task: u1#3`…
  workriver task deliver u1#3 --evidence 'cargo test => passed: 12 passed'";


#[derive(Parser)]
#[command(name = "workriver", version = BUILD, about = "Keep track of many coding agents working in parallel.", long_about = format!("{ABOUT}{}{ABOUT_END}", runtime::words::WHO_RECORDS))]
pub(crate) struct Cli {
    /// Port of the daemon on 127.0.0.1. Default: on a machine workriver follows over ssh,
    /// the port of its current connection (in ~/.config/workriver/port); else 47820.
    #[arg(long, global = true, env = "WORKRIVER_PORT")]
    pub(crate) port: Option<u16>,
    #[arg(long = "as", global = true, env = "WORKRIVER_AS", value_name = "AGENT", hide = true, help = runtime::words::HELP_AS)]
    actor: Option<String>,
    #[command(subcommand)]
    pub(crate) cmd: Cmd,
}

#[derive(Subcommand)]
pub(crate) enum Cmd {
    #[command(hide = true)]
    Host { dir: std::path::PathBuf },
    #[command(about = runtime::words::HELP_SERVE)]
    Serve {
        /// The terminal app brought to the front when an agent's terminal is opened (macOS);
        /// empty for none.
        #[arg(long, env = "WORKRIVER_TERMINAL_APP", default_value = "Ghostty")]
        terminal_app: String,
        /// The timing log (`<home>/logs/perf.jsonl`) also takes each git and ssh call (also:
        /// WORKRIVER_PERF=detail).
        #[arg(long)]
        perf_detail: bool,
    },
    /// Where time went, from the service's timing log: slowest requests, snapshots, units,
    /// git and ssh calls, time other machines were disconnected. `--bundle` writes it with
    /// the newest raw lines into one file you can send (no content, no secrets, no paths).
    Perf {
        /// Only this recent: 30m, 1h, 2d.
        #[arg(long)]
        since: Option<String>,
        /// Rows per table.
        #[arg(long, default_value_t = 20)]
        top: usize,
        #[arg(long)]
        bundle: bool,
    },
    /// Check that the daemon is running and show what it holds.
    Status,
    #[command(about = runtime::words::HELP_OPEN)]
    Open {
        /// Only print the link.
        #[arg(long)]
        print: bool,
    },
    #[command(subcommand, about = runtime::words::HELP_MACHINE)]
    Machine(MachineCmd),
    /// Agents workriver holds itself (hosted, an experiment): the user starts, answers and
    /// closes them; a workspace's manager prompts, reads and cancels its own workers.
    #[command(subcommand)]
    Agent(AgentCmd),
    #[command(subcommand, about = runtime::words::HELP_WORKSPACE)]
    Workspace(WorkspaceCmd),
    /// Projects: folders holding one or more repositories.
    #[command(subcommand)]
    Project(ProjectCmd),
    /// The backlog: planned intents in order (the roadmap) and deferred findings, per project.
    #[command(subcommand)]
    Backlog(BacklogCmd),
    /// Intents: goals in a project, planned → active → done (or dropped).
    #[command(subcommand)]
    Intent(IntentCmd),
    /// Units: pieces of an intent, each on its own branch.
    #[command(subcommand)]
    Unit(UnitCmd),
    /// Templates: the steps a unit is made with, placed as tasks to fill. The user keeps the
    /// library; whoever makes a unit picks one by its description.
    #[command(subcommand)]
    Template(TemplateCmd),
    /// Tasks: work in a unit, handed to an agent or (a gate) to the user.
    #[command(subcommand)]
    Task(TaskCmd),
    /// Findings: decide on a problem a review, tests task or gate found.
    #[command(subcommand)]
    Finding(FindingCmd),
    /// Gates: steps only the user can do (a manual deploy or try-out).
    #[command(subcommand)]
    Gate(GateCmd),
    /// Commits: say which task one belongs to, when its message has no `Task:` trailer.
    #[command(subcommand)]
    Commit(CommitCmd),
    /// Chores: things asked of the user that block nobody.
    #[command(subcommand)]
    Chore(ChoreCmd),
    #[command(subcommand, about = runtime::words::HELP_MANAGER)]
    Manager(ManagerCmd),
    /// Ask about another task, for an agent: the task's own agent answers if it is free, else
    /// a temporary agent that reads the records and the code. The question is recorded on
    /// that task; the answer comes back to your terminal as `[workriver] Answer to your
    /// question qN …`. Returns at once with the question id, unless --wait.
    Ask {
        /// The task the question is about (u1#3).
        task: String,
        question: String,
        /// A file it is about.
        #[arg(long)]
        file: Option<String>,
        /// Lines of that file, `A-B`.
        #[arg(long, value_name = "A-B", requires = "file")]
        lines: Option<String>,
        /// Wait for the answer and print it.
        #[arg(long)]
        wait: bool,
        /// With --wait: give up after this many seconds.
        #[arg(long, default_value_t = 600)]
        timeout: u64,
    },
    /// Put files (screenshots, reports, logs) into workriver's store and print where they are.
    /// The other way, on a machine the store is not on: `workriver attach fetch <link>` saves
    /// a stored file into the current folder (a file of yours named `fetch`: `./fetch`).
    /// Better: save them in the task's folder from the start (`workriver attach --dir u1#3`);
    /// `task deliver` attaches everything there (a file whose name starts with a finding id,
    /// e.g. `P2-1-login.png`, goes with that finding).
    Attach {
        files: Vec<String>,
        /// The task they belong to (u1#3).
        #[arg(long)]
        task: Option<String>,
        /// The finding of that task they show (P2-1).
        #[arg(long, requires = "task")]
        finding: Option<String>,
        /// Remove the originals once stored.
        #[arg(long = "move")]
        r#move: bool,
        /// Print (and make) the task's folder for screenshots instead.
        #[arg(long, value_name = "TASK", conflicts_with_all = ["files", "task"])]
        dir: Option<String>,
    },
    /// Answer a question about a task (for the agent it was given to, or the task's own agent;
    /// a question of the user's discussion of a delivered result, the task's own agent alone).
    Answer {
        /// The question (q3).
        question: String,
        #[arg(long)]
        text: String,
        /// The agent that did the work, if you asked it.
        #[arg(long)]
        asked: Option<String>,
        /// The text is the conclusion the user asked for (in a discussion: amend your delivery
        /// first if the result changed).
        #[arg(long)]
        conclusion: bool,
    },
    /// Commands the user runs from workriver's page.
    #[command(subcommand)]
    Run(RunCmd),
    /// Reply to the user's message (m3) that workriver typed into your terminal, for the
    /// manager it was sent to; the user reads it in workriver's page.
    Reply {
        /// The message (m3).
        message: String,
        #[arg(long)]
        text: String,
        /// An option the user picks from, `ID=LABEL` (ID: 1 to 32 of a-z, 0-9, -); 2 to 8,
        /// shown as radio buttons under the reply. For a question nothing waits on; a decision
        /// work waits on is a decide gate. Repeatable.
        #[arg(long, value_name = "ID=LABEL")]
        option: Vec<String>,
        /// An option whose answer needs the user's note (why). Repeatable.
        #[arg(long, value_name = "ID")]
        require_note: Vec<String>,
    },
    /// As a workspace's manager, tell the user something without being asked: it shows in
    /// your workspace's conversation in workriver's page, and in the user's Needs you until
    /// read. For what the user should know (a milestone, a changed plan, a finished intent);
    /// a decision goes in a decide gate (`workriver guide how ask-user`).
    Say {
        #[arg(long)]
        text: String,
        /// An option the user picks from, `ID=LABEL` (ID: 1 to 32 of a-z, 0-9, -); 2 to 8,
        /// shown as radio buttons under the message. For a question nothing waits on.
        /// Repeatable.
        #[arg(long, value_name = "ID=LABEL")]
        option: Vec<String>,
        /// An option whose answer needs the user's note (why). Repeatable.
        #[arg(long, value_name = "ID")]
        require_note: Vec<String>,
    },
    /// A message of a conversation with the user, as you may read it.
    #[command(subcommand)]
    Message(MessageCmd),
    /// Print the instructions for an agent working with workriver.
    ///
    /// What it prints matches this binary, which a copied skill file may not: `manager` (the
    /// manager's role), `worker` (one paragraph, goes into every task message), `import`
    /// (importing a backlog file), `how [TOPIC]` (the recipes: the index, or one),
    /// `changes` (what changed lately), `install-stub manager|howto|import` (a small skill
    /// file to install once, which sends its reader here).
    Guide {
        #[arg(value_parser = ["worker", "manager", "import", "how", "changes", "install-stub"])]
        what: String,
        /// `how`: the recipe; `install-stub`: which skill.
        topic: Option<String>,
    },
    /// Print an intent or unit as text: its tasks, open findings, waiting gates and open
    /// chores. Without a target, everything.
    Board {
        /// An intent (i1) or unit (u1).
        target: Option<String>,
    },
    /// Print the link that opens an intent, unit or task in the user's browser, on the
    /// machine workriver runs on: to point the user at it.
    Link {
        /// An intent (i3), unit (u1), task (u1#3), or a finding of a task (u1#3:P2-1).
        target: String,
    },
}

/// The short-lived commands.
pub(crate) fn run(cli: Cli) -> anyhow::Result<()> {
    match &cli.cmd {
        Cmd::Serve { .. } | Cmd::Host { .. } => unreachable!("main starts the service and the host"),
        Cmd::Perf { since, top, bundle } => {
            let home = common::home::data_dir();
            if *bundle {
                let path = common::diagnostics::bundle(&home, 2 * 1024 * 1024)?;
                println!("{}", path.display());
                eprintln!("It holds times, counts, route templates, unit ids and machine names; no commit messages, file texts, task texts, findings, notes, prompts, tokens, cookies or paths. Look before you send it.");
            } else {
                print!("{}", common::diagnostics::summary(&home, since.as_deref().map(common::diagnostics::parse_since).transpose()?, *top));
            }
            return Ok(());
        }
        Cmd::Guide { what, topic } => {
            print!("{}", guide(what, topic.as_deref())?);
            return Ok(());
        }
        _ => {}
    }
    let place = runtime::caller_place();
    let d = Daemon { port: port_of(cli.port), place, home: Some(common::home::data_dir()), prove: std::sync::Arc::new(from_socket) };
    if cli.actor.as_deref() == Some("user") {
        bail!(runtime::words::AS_USER_GONE);
    }
    // `--as`, where the service allows it; else empty: whoever the service finds this to be.
    let actor = cli.actor.as_deref().unwrap_or_default();
    let cx = Cx { d: &d, actor };
    let out = match cli.cmd {
        Cmd::Serve { .. } | Cmd::Host { .. } | Cmd::Guide { .. } | Cmd::Perf { .. } => unreachable!(),
        Cmd::Status => {
            let s = d.get("/api/status")?;
            let theirs = format!("{} ({})", s["version"].as_str().unwrap_or("?"), s["commit"].as_str().unwrap_or("unknown"));
            println!("workriver {theirs} is running");
            if theirs != BUILD {
                println!("  this `workriver` command is {BUILD}: another build than the service. Rebuild the one that is behind (on every machine: `git pull`, build, restart `workriver serve`).");
            }
            println!("web UI: http://127.0.0.1:{}  (open it as you with `workriver open`)", s["port"]);
            println!("database: {}", s["db"].as_str().unwrap_or("?"));
            println!("{} events, {} intents, {} units", s["events"], s["intents"], s["units"]);
            let rt = &s["runtime"];
            println!("agent runtime: {}", if rt["connected"] == true { "connected" } else { "not connected" });
            if let Some(w) = rt["warning"].as_str() {
                println!("  at start: {w}");
            }
            print_machines(&s["machines"]);
            let at = &s["attachments"];
            println!(
                "attachments: {} files, {:.1} MB in {} (what the attachment rule in Settings keeps; kept for good unless set)",
                at["files"],
                at["bytes"].as_f64().unwrap_or(0.0) / 1_048_576.0,
                at["dir"].as_str().unwrap_or("?")
            );
            print_jobs(&s["jobs"]);
            return Ok(());
        }
        Cmd::Open { print } => {
            // An agent does not open the user's session for itself.
            if d.call("/api/here", Some(json!({}))).is_ok_and(|h| h["actor"].is_string()) {
                bail!(runtime::words::OPEN_IS_THE_USERS);
            }
            let url = d.call("/api/session/code", Some(json!({ "printed": print })))?["url"].as_str().unwrap_or_default().to_string();
            if print {
                // A link on a screen is a link for everyone who reads that screen.
                eprintln!("{}", runtime::words::LINK_ON_A_SCREEN);
                println!("{url}");
                return Ok(());
            }
            // Handed to the browser, never shown: it signs in whoever has it.
            let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
            let opened = std::process::Command::new(opener).arg(&url).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().is_ok_and(|s| s.success());
            if !opened {
                bail!("cannot open a browser here (`{opener}` failed). On a machine without one: `workriver open --print` prints the link, in a terminal no agent reads");
            }
            println!("Opened workriver in your browser.");
            return Ok(());
        }
        Cmd::Ask { .. } | Cmd::Answer { .. } | Cmd::Reply { .. } | Cmd::Say { .. } | Cmd::Message(_) | Cmd::Manager(ManagerCmd::Tell { .. }) => match conversations::run(cli.cmd, &cx)? {
            Some(out) => out,
            None => return Ok(()),
        },
        Cmd::Workspace(_) | Cmd::Project(_) | Cmd::Backlog(_) | Cmd::Intent(_) | Cmd::Unit(_) | Cmd::Template(_) | Cmd::Manager(ManagerCmd::Here { .. }) => match planning::run(cli.cmd, &cx)? {
            Some(out) => out,
            None => return Ok(()),
        },
        Cmd::Agent(_) => match hosted::run(cli.cmd, &cx)? {
            Some(out) => out,
            None => return Ok(()),
        },
        Cmd::Machine(_) => match machines::run(cli.cmd, &cx)? {
            Some(out) => out,
            None => return Ok(()),
        },
        Cmd::Board { target } => return board(&d, target.as_deref()),
        Cmd::Link { target } => {
            let path = link_path(&target)?;
            println!("{}{path}", page(&d)?);
            return Ok(());
        }
        Cmd::Task(_) | Cmd::Finding(_) | Cmd::Commit(_) | Cmd::Gate(_) | Cmd::Chore(_) | Cmd::Attach { .. } | Cmd::Run(_) => match work::run(cli.cmd, &cx)? {
            Some(out) => out,
            None => return Ok(()),
        },
    };
    println!("{}", made(&out));
    Ok(())
}

/// What a command prints of what it recorded: the id an agent uses next, and what the
/// service asks it to do next.
pub(crate) fn made(out: &serde_json::Value) -> String {
    let b = &out["body"];
    match out["kind"].as_str() {
        // A gate added ahead of steps not done: the warning on the next line.
        Some("task.created") => format!("{}#{}{}", b["unit"].as_str().unwrap_or("?"), b["n"], warned(out)),
        Some("task.filled") => format!("filled {}#{}{}", b["unit"].as_str().unwrap_or("?"), b["task"], warned(out)),
        Some("unit.done") => format!("{} done{}", b["unit"].as_str().unwrap_or("?"), warned(out)),
        Some("unit.created") => placed_text(b),
        Some("template.set") => format!("saved template {} ({} steps); a change touches only units made after it", b["name"].as_str().unwrap_or("?"), b["steps"].as_array().map_or(0, Vec::len)),
        Some("template.removed") => format!("removed template {}", b["name"].as_str().unwrap_or("?")),
        Some("intent.created" | "chore.created") => b["id"].as_str().unwrap_or("?").to_string(),
        Some("finding.added") => b["finding"]["id"].as_str().unwrap_or("?").to_string(),
        Some("unit.relocated") => format!("{} is now {}{}, branch {} from {}", b["unit"].as_str().unwrap_or("?"), b["machine"].as_str().map(|m| format!("on {m}: ")).unwrap_or_default(), b["repo"].as_str().unwrap_or("?"), b["branch"].as_str().unwrap_or("?"), b["base"].as_str().unwrap_or("?")),
        Some("commit.linked") => format!("{} is now part of {}#{}", b["hash"].as_str().unwrap_or("?").chars().take(9).collect::<String>(), b["unit"].as_str().unwrap_or("?"), b["task"]),
        Some("commit.unlinked") => "unlinked".into(),
        Some("workspace.created") => format!("{} {}", b["id"].as_str().unwrap_or("?"), b["name"].as_str().unwrap_or("?")),
        Some("workspace.renamed") => format!("{} is now {}", b["workspace"].as_str().unwrap_or("?"), b["name"].as_str().unwrap_or("?")),
        Some("workspace.closed") => format!("{} closed", b["workspace"].as_str().unwrap_or("?")),
        Some("workspace.opened") => format!("{} open", b["workspace"].as_str().unwrap_or("?")),
        Some("workspace.manager_set") => match b["manager"].as_str() {
            Some(m) => format!("{m} is the manager of {}; it was told in its terminal", b["workspace"].as_str().unwrap_or("?")),
            None => format!("{} has no manager now", b["workspace"].as_str().unwrap_or("?")),
        },
        Some("intent.workspace_set") => format!("{} runs in {}", b["intent"].as_str().unwrap_or("?"), b["workspace"].as_str().unwrap_or("?")),
        Some("project.added") => format!("{} {}", b["id"].as_str().unwrap_or("?"), b["path"].as_str().unwrap_or("?")),
        Some("intent.merged") => format!("merged {} into {}{}", b["from"].as_array().into_iter().flatten().filter_map(|f| f.as_str()).collect::<Vec<_>>().join(" "), b["intent"].as_str().unwrap_or("?"), b["title"].as_str().map(|t| format!(", now \"{t}\"")).unwrap_or_default()),
        // What the service asks of the one who ran it next (a manager closing its intent).
        Some("intent.state_changed") => format!("{} {}{}", b["intent"].as_str().unwrap_or("?"), b["state"].as_str().unwrap_or("?"), out["next"].as_str().map(|n| format!("\n{n}")).unwrap_or_default()),
        Some("task.amended") if b["add"].as_array().is_some_and(|a| !a.is_empty()) => {
            let ids: Vec<&str> = b["add"].as_array().into_iter().flatten().filter_map(|f| f["id"].as_str()).collect();
            format!("added findings {}", ids.join(" "))
        }
        Some("task.amended") => "amended".into(),
        // Recorded either way; whether its agent was told is said apart.
        Some("task.noted") => format!("added to {}#{}; {}", b["unit"].as_str().unwrap_or("?"), b["task"], out["told"].as_str().unwrap_or("its agent was not told")),
        Some("task.dropped") => format!("{}#{} dropped", b["unit"].as_str().unwrap_or("?"), b["task"]),
        Some("question.answered") => "answered".into(),
        Some("message.replied") => "replied".into(),
        Some("message.said") => format!("said in workspace {} ({})", b["workspace"].as_str().unwrap_or("?"), b["id"].as_str().unwrap_or("?")),
        Some("manager.told") => "told the manager (if it runs)".into(),
        Some("task.delivered") => {
            let ids: Vec<&str> = b["findings"].as_array().into_iter().flatten().filter_map(|f| f["id"].as_str()).collect();
            if ids.is_empty() { "recorded".into() } else { format!("recorded findings {}", ids.join(" ")) }
        }
        _ => "recorded".into(),
    }
}

/// The warning a command's answer carries (steps of a template not done), on a line of its
/// own; empty when there is none.
fn warned(out: &serde_json::Value) -> String {
    out["warning"].as_str().map(|w| format!("\n{w}")).unwrap_or_default()
}

#[cfg(test)]
mod guards;
#[cfg(test)]
mod tests;
