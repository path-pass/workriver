// workriver: the daemon (`serve`) and the short-lived CLI that records work through it.
mod ask;
mod attachments;
mod auth;
mod backlog;
mod caller;
mod common;
mod deliver;
mod herdr;
mod machines;
mod metrics;
mod quote;
mod run;
mod serve;
mod show;
mod snapshot;
mod stall;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, anyhow, bail};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use workriver_core::{self as core, FindingState, TaskKind, TaskState, name, parse_task_ref};

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

Who records: the service finds it, from the process that runs this command (the kernel
tells the service which process connected; the pane the CLI names is only cross-checked).
A command run in an agent's Herdr pane is that agent's (by its session). One run in a pane
with no agent, or in a terminal window outside Herdr, on the machine workriver runs on, is
the user's (the CLI sends the user token, `<WORKRIVER_HOME>/user-token`). Anything else is
refused: a command that names a pane it does not run in, one with no terminal in no pane,
a request that is not this CLI's. Nobody is anyone by saying so. Decisions as the user, gates, chores done, projects, machines and workspace links are
the user's alone. Inside a Herdr workspace the intent is the workspace's current one, so
commands that take an intent may leave it out.

Commits are read from git: a commit belongs to a task when its message has the trailer
line `Task: u1#3`. A commit on the unit's branch without one shows as unregistered.

Typical flow for an agent:
  workriver task start u1#3
  …work, commit with `Task: u1#3`…
  workriver task deliver u1#3 --evidence 'cargo test => passed: 12 passed'";

/// The version and the commit this binary was built from.
const BUILD: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("WORKRIVER_COMMIT"), ")");

#[derive(Parser)]
#[command(name = "workriver", version = BUILD, about = "Keep track of many coding agents working in parallel.", long_about = ABOUT)]
struct Cli {
    /// Port of the daemon on 127.0.0.1. Default: on a machine workriver follows over ssh,
    /// the port of its current connection (in ~/.config/workriver/port); else 47820.
    #[arg(long, global = true, env = "WORKRIVER_PORT")]
    port: Option<u16>,
    /// For tests and development: record as this agent instead of the one in this Herdr
    /// pane. Refused unless the service was started with WORKRIVER_ALLOW_AS=1; never the user.
    #[arg(long = "as", global = true, env = "WORKRIVER_AS", value_name = "AGENT", hide = true)]
    actor: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the daemon: records events, reads git, follows the agents in Herdr, serves the
    /// web UI. Data lives in $WORKRIVER_HOME (default ~/workriver-data).
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
    /// Open the web UI in the browser as the user: with a one-time link that gives the page
    /// its session. Not from an agent's pane.
    Open {
        /// Only print the link.
        #[arg(long)]
        print: bool,
    },
    /// Other machines whose agents (in their own Herdr) are followed over ssh.
    #[command(subcommand)]
    Machine(MachineCmd),
    /// Runtime workspaces (Herdr's) on this and other machines, and the links that make a
    /// workspace of another machine one with a workspace of this machine.
    #[command(subcommand)]
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
    /// The manager: the one agent per Herdr workspace that splits work and hands it out.
    #[command(subcommand)]
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
    /// Answer a question about a task (for the agent it was given to, or the task's own agent).
    Answer {
        /// The question (q3).
        question: String,
        #[arg(long)]
        text: String,
        /// The agent that did the work, if you asked it.
        #[arg(long)]
        asked: Option<String>,
        /// The text is the conclusion the user asked for.
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
    },
    /// As a workspace's manager, tell the user something without being asked: it shows in
    /// your workspace's conversation in workriver's page, and in the user's Needs you until
    /// read. For what the user should know (a milestone, a changed plan, a finished intent);
    /// a decision goes in a decide gate (`workriver guide how ask-user`).
    Say {
        #[arg(long)]
        text: String,
    },
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
        /// An intent (i3), unit (u1), task (u1#3, i3#1), or a finding of a task (u1#3:P2-1).
        target: String,
    },
}

#[derive(Subcommand)]
enum RunCmd {
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

#[derive(Subcommand)]
enum IntentCmd {
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
enum MachineCmd {
    /// Add a machine (not followed until enabled). workriver connects with
    /// `ssh -o BatchMode=yes <target>`: it must work without a password prompt.
    Add {
        /// workriver's name for it: letters, digits, '-', '_', '.'.
        label: String,
        /// What ssh takes: a Host of your ssh config, or user@host.
        #[arg(long)]
        ssh: String,
        /// Herdr's socket over there; default: asked of the machine
        /// ($HERDR_SOCKET_PATH, else ~/.config/herdr/herdr.sock).
        #[arg(long, value_name = "PATH")]
        herdr_socket: Option<String>,
    },
    /// The machines, whether each can be reached, and why not.
    List,
    /// CPU and memory of this machine and the followed ones, newest reading (with memory
    /// pressure, swap, OOM kills and free disk), and which are over their alert thresholds. Read-only: agents may run it too (the user sets the
    /// thresholds, in Settings).
    Load {
        /// Also the mean and highest CPU and memory of each machine over the last N hours
        /// (1 to 168), from the minute history workriver keeps.
        #[arg(long, value_name = "N")]
        hours: Option<u32>,
    },
    /// Follow a machine: its agents show up, and its `workriver` CLI reaches this service.
    Enable { label: String },
    /// Stop following a machine; nothing over there is stopped.
    Disable { label: String },
    Remove { label: String },
}

#[derive(Subcommand)]
enum WorkspaceCmd {
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
enum ProjectCmd {
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
enum BacklogCmd {
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
enum UnitCmd {
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
enum TaskCmd {
    /// Everything about one task that you may see, in full: what it asks, where the work
    /// is, the findings it must answer with all that was added to them and every decision,
    /// steps, its output, commits and questions. Read it before you start a task. Without a
    /// task, in an agent's pane: the task that agent is on.
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
        /// Gate tasks: what the user is asked to do (`decide`: discuss and confirm a design).
        #[arg(long, value_parser = ["deploy", "test", "decide"])]
        gate: Option<String>,
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
enum FindingCmd {
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
enum CommitCmd {
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
enum GateCmd {
    /// Submit a waiting gate's result. A problem needs findings (JSON as in `task deliver`).
    Submit {
        /// The gate (u1#5).
        task: String,
        #[arg(value_parser = ["done", "problem"])]
        result: String,
        #[arg(long, default_value = "")]
        note: String,
        #[arg(long, value_name = "FILE")]
        findings: Option<String>,
    },
}

#[derive(Subcommand)]
enum ChoreCmd {
    /// Ask the user for something that blocks nobody; prints its id (c1…).
    ///
    /// It belongs to an intent; or, with --project (or outside any workspace with a current
    /// intent), to a project; or, where no project can be told, to nothing.
    Add {
        /// The intent it belongs to (i1…); default: the intent of this Herdr workspace.
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

#[derive(Subcommand)]
enum ManagerCmd {
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

const GUIDE_WORKER: &str = include_str!("../../../skills/workriver-manager/references/worker.md");
const GUIDE_MANAGER: &str = include_str!("../../../skills/workriver-manager/SKILL.md");
const GUIDE_IMPORT: &str = include_str!("../../../skills/workriver-backlog-import/SKILL.md");
const GUIDE_HOW: &str = include_str!("../../../skills/workriver-howto/SKILL.md");
const GUIDE_CHANGES: &str = include_str!("../../../skills/workriver-howto/CHANGES.md");

/// The recipes of `workriver guide how <topic>`, one file each.
#[derive(rust_embed::Embed)]
#[folder = "../../skills/workriver-howto/references/"]
struct Recipes;

/// A skill file without its front matter (the part between the two `---` lines).
fn body(skill: &str) -> &str {
    skill.strip_prefix("---").and_then(|s| s.split_once("\n---\n")).map(|(_, b)| b.trim_start()).unwrap_or(skill)
}

/// What `workriver guide …` prints. Everything but the worker's paragraph starts with a line
/// naming the build, so a reader can tell which version it read; the worker's paragraph
/// stays one line (it goes into a terminal message) and names the build at its start.
fn guide(what: &str, topic: Option<&str>) -> anyhow::Result<String> {
    let head = |name: &str| format!("workriver {BUILD} · {name}\n\n");
    let recipe = |t: &str| Recipes::get(&format!("{t}.md")).map(|f| String::from_utf8_lossy(&f.data).into_owned());
    Ok(match (what, topic) {
        ("worker", _) => format!("(workriver {BUILD}) {}", GUIDE_WORKER),
        ("manager", _) => head("guide manager") + body(GUIDE_MANAGER),
        ("import", _) => head("guide import") + body(GUIDE_IMPORT),
        ("changes", _) => head("guide changes") + GUIDE_CHANGES,
        // An unknown topic gets the index, which lists the ones there are.
        ("how", Some(t)) if recipe(t).is_some() => head(&format!("guide how {t}")) + &recipe(t).unwrap_or_default(),
        ("how", _) => head("guide how") + body(GUIDE_HOW),
        ("install-stub", Some(skill @ ("manager" | "howto" | "import"))) => stub(skill),
        ("install-stub", _) => bail!("which skill? workriver guide install-stub manager|howto|import > <skills folder>/workriver-<name>/SKILL.md"),
        _ => unreachable!("clap checks the values"),
    })
}

/// A skill file to install once into a harness's skills folder: the real skill's name and
/// description (so it is found when needed), and a body that sends its reader to the
/// installed `workriver`, which is never older than itself.
fn stub(skill: &str) -> String {
    let (source, command, name) = match skill {
        "manager" => (GUIDE_MANAGER, "workriver guide manager", "workriver manager"),
        "howto" => (GUIDE_HOW, "workriver guide how", "workriver how-to"),
        _ => (GUIDE_IMPORT, "workriver guide import", "workriver backlog import"),
    };
    let front = source.strip_prefix("---").and_then(|s| s.split_once("\n---\n")).map(|(f, _)| f.trim()).unwrap_or_default();
    format!(
        "---\n{front}\n---\n\n# {name}\n\nRun `{command}` and follow what it prints: it is this skill, and it matches the installed\nworkriver (a copy kept here would go stale with every update).\n\nIf the command is not found, workriver is not installed on this machine: tell the user.\n"
    )
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match &cli.cmd {
        Cmd::Serve { terminal_app, perf_detail } => {
            common::diagnostics::init(&common::home::data_dir(), *perf_detail || std::env::var("WORKRIVER_PERF").is_ok_and(|v| v == "detail"));
            // A directory or file that cannot be made is the store's error, as when it made them.
            let path = common::home::database().map_err(workriver_data::Error::from).context("cannot open the workriver database")?;
            let store = workriver_data::Store::open(&path).context("cannot open the workriver database")?;
            let runtime = herdr::Herdr::new(herdr::socket_path(), terminal_app.clone());
            // Without Herdr the daemon still records and shows; it just sees no agents.
            let warning = runtime.check();
            let home = store.path().parent().map(Path::to_path_buf).unwrap_or_default();
            let all = machines::Machines::new(std::sync::Arc::new(runtime), home, terminal_app.clone());
            if let Some(w) = &warning {
                tracing::warn!("{w}");
            }
            let allow_as = std::env::var("WORKRIVER_ALLOW_AS").is_ok_and(|v| v == "1");
            let options = serve::Options { warning, waits: serve::Waits::default(), machines: Some(all.clone()), allow_as, sample: true, procs: std::sync::Arc::new(common::process::System), callers: std::sync::Arc::default() };
            let rt = tokio::runtime::Runtime::new()?;
            return rt.block_on(async {
                let listener = serve::bind(cli.port.unwrap_or(DEFAULT_PORT)).await?;
                serve::serve_with(listener, store, all, options).await
            });
        }
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
    let place = herdr::place_from_env();
    let d = Daemon { port: port_of(cli.port), place, home: Some(common::home::data_dir()), prove: std::sync::Arc::new(from_socket) };
    if cli.actor.as_deref() == Some("user") {
        bail!("--as user is gone: the user is whoever runs the CLI from a terminal that is not an agent's pane, on the machine workriver runs on");
    }
    // Who records, and in which intent, the service finds from the pane this process runs in.
    // Where it runs matters to these: the workspace's current intent, or the workspace itself.
    let needs_place = matches!(
        cli.cmd,
        Cmd::Unit(UnitCmd::Add { intent: None, .. })
            | Cmd::Chore(ChoreCmd::Add { intent: None, project: None, .. })
            | Cmd::Manager(ManagerCmd::Here { intent: None, .. })
            | Cmd::Intent(IntentCmd::Start { .. })
    );
    let records = !matches!(cli.cmd, Cmd::Status | Cmd::Board { .. } | Cmd::Link { .. } | Cmd::Task(TaskCmd::Show { .. }) | Cmd::Finding(FindingCmd::Show { .. }) | Cmd::Machine(_) | Cmd::Workspace(WorkspaceCmd::List) | Cmd::Project(ProjectCmd::List) | Cmd::Backlog(BacklogCmd::List { .. } | BacklogCmd::Export { .. }));
    // Who records is the daemon's to find, from this place; the CLI only asks where it is
    // for the commands that need the workspace or its current intent.
    let here = if records && needs_place { Some(d.call("/api/here", Some(json!({})))?) } else { None };
    let group = here.as_ref().map(|h| h["group"].clone()).filter(|g| !g.is_null());
    // `--as`, where the service allows it; else empty: whoever the service finds this to be.
    let actor = cli.actor.clone().unwrap_or_default();
    let actor = actor.as_str();
    let here_intent = |given: Option<String>| {
        given.or_else(|| here.as_ref().and_then(|h| h["intent"].as_str().map(str::to_string))).ok_or_else(|| match &group {
            Some(g) => anyhow!(
                "no intent given, and Herdr workspace {} has no current intent; its manager starts one: `workriver backlog list`, then `workriver intent start <intent>`",
                g["label"].as_str().unwrap_or("?")
            ),
            None => anyhow!("no intent given, and not inside a Herdr workspace; name one (see `workriver backlog list`)"),
        })
    };
    let out = match cli.cmd {
        Cmd::Serve { .. } | Cmd::Guide { .. } | Cmd::Perf { .. } => unreachable!(),
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
                "attachments: {} files, {:.1} MB in {} (kept for good, like the records)",
                at["files"],
                at["bytes"].as_f64().unwrap_or(0.0) / 1_048_576.0,
                at["dir"].as_str().unwrap_or("?")
            );
            return Ok(());
        }
        Cmd::Open { print } => {
            // An agent does not open the user's session for itself.
            if d.call("/api/here", Some(json!({}))).is_ok_and(|h| h["actor"].is_string()) {
                bail!("`workriver open` is the user's: run it from a terminal that is not an agent's pane");
            }
            let url = d.call("/api/session/code", Some(json!({ "printed": print })))?["url"].as_str().unwrap_or_default().to_string();
            if print {
                // A link on a screen is a link for everyone who reads that screen.
                eprintln!("Anyone who can read this terminal can open workriver as you with this link (agents read Herdr panes). It works once, for 60 seconds.");
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
        Cmd::Manager(ManagerCmd::Tell { task, text }) => {
            let (unit, n) = task_ref(&task)?;
            d.append(actor, "manager.told", json!({ "unit": unit, "task": n, "text": text }))?
        }
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
            return Ok(());
        }
        Cmd::Machine(MachineCmd::Load { hours }) => {
            let report = d.get("/api/metrics")?;
            print!("{}", load_text(&report));
            if let Some(hours) = hours {
                println!("over the last {hours} h (minute means):");
                for m in report["machines"].as_array().into_iter().flatten() {
                    let label = m["label"].as_str().unwrap_or_default();
                    let h = d.get(&format!("/api/metrics/history?machine={label}&hours={hours}"))?;
                    println!("{}", history_text(m, &h["points"]));
                }
            }
            return Ok(());
        }
        Cmd::Machine(cmd) => {
            let (label, action, extra) = match cmd {
                MachineCmd::Load { .. } => unreachable!("handled above"),
                MachineCmd::List => (String::new(), "", json!({})),
                MachineCmd::Add { label, ssh, herdr_socket } => (label, "add", json!({ "ssh": ssh, "herdrSocket": herdr_socket })),
                MachineCmd::Enable { label } => (label, "enable", json!({})),
                MachineCmd::Disable { label } => (label, "disable", json!({})),
                MachineCmd::Remove { label } => (label, "remove", json!({})),
            };
            let list = if action.is_empty() {
                d.get("/api/machines")?
            } else {
                let mut body = extra;
                body["label"] = json!(label);
                body["action"] = json!(action);
                d.call("/api/machines", Some(body))?
            };
            // Enabling connects in the background: wait for how the first attempt ends.
            let mut list = list;
            if action == "enable" {
                let until = std::time::Instant::now() + Duration::from_secs(10);
                let pending = |l: &Value| l.as_array().into_iter().flatten().any(|m| m["label"] == label.as_str() && m["connecting"] == true);
                while pending(&list) && std::time::Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(500));
                    list = d.get("/api/machines")?;
                }
            }
            print_machines(&list);
            if action == "enable" {
                println!("On {label}, `workriver status` must answer through this connection.");
            }
            return Ok(());
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
            return Ok(());
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
            return Ok(());
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
        Cmd::Backlog(BacklogCmd::List { project }) => return backlog::list(&d, project.as_deref().map(project_arg).transpose()?.as_deref()),
        Cmd::Backlog(BacklogCmd::Export { project }) => return backlog::export(&d, &project_arg(&project)?),
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
        Cmd::Board { target } => return board(&d, target.as_deref()),
        Cmd::Link { target } => {
            let path = link_path(&target)?;
            println!("{}{path}", page(&d)?);
            return Ok(());
        }
        Cmd::Task(TaskCmd::Show { task, json }) => {
            let shown = task_show(&d, task.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&shown)?);
            } else {
                print!("{}", show::text(&shown));
            }
            return Ok(());
        }
        Cmd::Finding(FindingCmd::Show { task, finding }) => {
            let shown = task_show(&d, Some(&task))?;
            let f = shown.output.iter().flat_map(|o| &o.findings).find(|f| f.id == finding).ok_or_else(|| anyhow!("{task} has no finding {finding}"))?;
            println!("{}  {}  {}", shown.reference, shown.kind, shown.title);
            print!("{}", show::finding_text(f, ""));
            return Ok(());
        }
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
        Cmd::Task(TaskCmd::Add { unit, kind, title, text, after, cause, agent, reviewer, gate, step, plan, blind, blocks }) => {
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
            d.append(actor, "task.created", json!({
                "unit": unit, "kind": kind, "title": title, "text": text, "inputs": inputs, "causes": causes,
                "agent": agent, "reviewer": reviewer, "gate": gate, "steps": steps, "plans": plan, "blind": blind, "blocks": blocks,
            }))?
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
                upload_finding_files(&d, actor, list)?;
            }
            d.append(actor, "task.amended", json!({ "unit": unit, "task": n, "add": add }))?
        }
        Cmd::Task(TaskCmd::Deliver { task, note, evidence, findings, amend, .. }) => deliver(&d, actor, &task, note, &evidence, findings, amend)?,
        Cmd::Attach { files, task, finding, r#move, dir } => {
            if let Some(t) = dir {
                let (unit, n) = task_ref(&t)?;
                println!("{}", d.inbox(&unit, n)?.display());
                return Ok(());
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
                return Ok(());
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
            return Ok(());
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
        Cmd::Gate(GateCmd::Submit { task, result, note, findings }) => {
            let (unit, n) = task_ref(&task)?;
            let findings = read_findings(findings.as_deref())?;
            let mut findings = findings;
            if let Some(list) = findings.as_array_mut() {
                upload_finding_files(&d, actor, list)?;
            }
            d.append(actor, "gate.submitted", json!({ "unit": unit, "task": n, "result": result, "note": note, "findings": findings }))?
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
        Cmd::Ask { task, question, file, lines, wait, timeout } => {
            let (unit, n) = task_ref(&task)?;
            let mut context = vec![];
            if let Some(path) = &file {
                context.push(match lines.as_deref().map(parse_lines).transpose()? {
                    Some((a, b)) => json!({ "kind": "lines", "label": format!("{path} L{a}-{b}"), "path": path, "from": a, "to": b }),
                    None => json!({ "kind": "file", "label": path, "path": path }),
                });
            }
            let mut ask = d.me(actor);
            ask["unit"] = json!(unit);
            ask["task"] = json!(n);
            ask["text"] = json!(question);
            ask["context"] = json!(context);
            let e = d.call("/api/ask", Some(ask))?;
            let id = e["body"]["id"].as_str().unwrap_or("?").to_string();
            if !wait {
                println!("{id}");
                return Ok(());
            }
            println!("{}", wait_answer(&d, &id, Duration::from_secs(timeout))?);
            return Ok(());
        }
        Cmd::Answer { question, text, asked, conclusion } => d.append(actor, "question.answered", json!({ "question": question, "text": text, "asked": asked, "conclusion": conclusion }))?,
        Cmd::Run(RunCmd::Review { run, verdict, text }) => {
            if run.is_empty() || !run.chars().all(|c| c.is_ascii_alphanumeric()) {
                bail!("{run:?} is not a run id");
            }
            let mut review = d.me(actor);
            review["verdict"] = json!(verdict);
            review["text"] = json!(text);
            d.call(&format!("/api/run/{run}/review"), Some(review))?;
            println!("recorded");
            return Ok(());
        }
        Cmd::Reply { message, text } => d.append(actor, "message.replied", json!({ "message": message, "text": text }))?,
        Cmd::Say { text } => d.append(actor, "message.said", json!({ "text": text }))?,
    };
    // What was recorded, by the id an agent uses next.
    let b = &out["body"];
    let made = match out["kind"].as_str() {
        Some("task.created") => format!("{}#{}", b["unit"].as_str().unwrap_or("?"), b["n"]),
        Some("intent.created" | "unit.created" | "chore.created") => b["id"].as_str().unwrap_or("?").to_string(),
        Some("finding.added") => b["finding"]["id"].as_str().unwrap_or("?").to_string(),
        Some("unit.relocated") => format!("{} is now {}{}, branch {} from {}", b["unit"].as_str().unwrap_or("?"), b["machine"].as_str().map(|m| format!("on {m}: ")).unwrap_or_default(), b["repo"].as_str().unwrap_or("?"), b["branch"].as_str().unwrap_or("?"), b["base"].as_str().unwrap_or("?")),
        Some("commit.linked") => format!("{} is now part of {}#{}", b["hash"].as_str().unwrap_or("?").chars().take(9).collect::<String>(), b["unit"].as_str().unwrap_or("?"), b["task"]),
        Some("commit.unlinked") => "unlinked".into(),
        Some("workspace.linked") => "linked".into(),
        Some("workspace.unlinked") => "unlinked".into(),
        Some("workspace.project_set") => b["project"].as_str().map(|p| format!("works on {p}")).unwrap_or_else(|| "worked out by workriver".into()),
        Some("project.added") => format!("{} {}", b["id"].as_str().unwrap_or("?"), b["path"].as_str().unwrap_or("?")),
        Some("intent.merged") => format!("merged {} into {}{}", b["from"].as_array().into_iter().flatten().filter_map(|f| f.as_str()).collect::<Vec<_>>().join(" "), b["intent"].as_str().unwrap_or("?"), b["title"].as_str().map(|t| format!(", now \"{t}\"")).unwrap_or_default()),
        Some("intent.state_changed") => format!("{} {}", b["intent"].as_str().unwrap_or("?"), b["state"].as_str().unwrap_or("?")),
        Some("task.amended") if b["add"].as_array().is_some_and(|a| !a.is_empty()) => {
            let ids: Vec<&str> = b["add"].as_array().into_iter().flatten().filter_map(|f| f["id"].as_str()).collect();
            format!("added findings {}", ids.join(" "))
        }
        Some("task.amended") => "amended".into(),
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
    };
    println!("{made}");
    Ok(())
}

/// How the CLI shows the service which process it is: the proof of its process (fetched
/// anew when `fresh`) and its process id. The CLI's own: `from_socket`.
type Prover = std::sync::Arc<dyn Fn(&Daemon, bool) -> (Option<String>, u32) + Send + Sync>;

#[derive(Clone)]
struct Daemon {
    port: u16,
    /// The pane the CLI finds in its environment: sent along (as a header and nowhere
    /// else), and only cross-checked. Who this is, the service takes from this process.
    place: Option<String>,
    /// Where the user token is, if this machine has it; read for each request.
    home: Option<PathBuf>,
    prove: Prover,
}

/// The proof of this process from the service's caller socket (the kernel tells the
/// service who connected), fetched once per command, and this process's id. Where there
/// is no such socket (a machine workriver follows: the service is elsewhere) only the id
/// goes along, and the service checks it over there.
fn from_socket(d: &Daemon, fresh: bool) -> (Option<String>, u32) {
    static PROOF: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
    let mut kept = PROOF.lock().unwrap();
    if fresh || kept.is_none() {
        *kept = d.home.as_deref().and_then(caller::fetch);
    }
    (kept.clone(), std::process::id())
}

const DEFAULT_PORT: u16 = 47820;

/// The computer the daemon runs on, when this CLI reaches it through a machine's ssh
/// tunnel (its port read from the file the daemon writes over there).
static THROUGH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// The daemon's port: as given; else, on a machine workriver follows, its connection's
/// port as the daemon wrote it there; else the default.
fn port_of(given: Option<u16>) -> u16 {
    if let Some(p) = given {
        return p;
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let read = |f: &str| home.as_ref().and_then(|h| std::fs::read_to_string(h.join(f)).ok()).map(|t| t.trim().to_string());
    match read(crate::common::home::PORT_FILE).and_then(|t| t.parse().ok()) {
        Some(p) => {
            let _ = THROUGH.set(read(crate::common::home::HOST_FILE).filter(|h| !h.is_empty()).unwrap_or_else(|| "the computer workriver runs on".into()));
            p
        }
        None => DEFAULT_PORT,
    }
}

/// How long the CLI waits to connect (a tunnel that is reconnecting refuses at once, or
/// not at all), and for an answer: it never hangs.
const CONNECT: Duration = Duration::from_secs(3);
const ANSWER: Duration = Duration::from_secs(30);

impl Daemon {
    /// Why the daemon could not be reached, as whoever runs the CLI can act on it.
    fn unreachable(&self, e: impl std::fmt::Display) -> anyhow::Error {
        match THROUGH.get() {
            Some(host) => anyhow!(
                "cannot reach workriver on {host} through the ssh tunnel (port {}, from ~/{}; it may be reconnecting); try again shortly ({e})",
                self.port,
                crate::common::home::PORT_FILE
            ),
            None => anyhow!("cannot reach workriver on 127.0.0.1:{} ({e}). Is `workriver serve` running?", self.port),
        }
    }

    /// Through a tunnel, first a quick look that the daemon answers at all: the forward of a
    /// connection that died accepts and then says nothing, and a request would wait out
    /// its whole time. Once per command.
    fn probe(&self) -> anyhow::Result<()> {
        static PROBED: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
        if THROUGH.get().is_none() {
            return Ok(());
        }
        let port = self.port;
        let seen = PROBED.get_or_init(|| {
            let agent: ureq::Agent = ureq::Agent::config_builder().timeout_connect(Some(CONNECT)).timeout_global(Some(CONNECT)).build().into();
            agent.get(format!("http://127.0.0.1:{port}/api/status")).call().map(|_| ()).map_err(|e| e.to_string())
        });
        seen.clone().map_err(|e| self.unreachable(e))
    }

    /// The proof of this process and its id, as `prove` gives them.
    fn process(&self, fresh: bool) -> (Option<String>, u32) {
        (self.prove)(self, fresh)
    }

    /// Sends a request (`make` adds the headers it is given) with everything that shows the
    /// service who sends it. The token goes along whenever this machine has it: it makes this
    /// the user only where the service finds this process in no agent's pane. A command that
    /// lasts longer than its proof gets a new one and repeats the request, once.
    fn send(&self, make: impl Fn(&[(&str, String)]) -> Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> anyhow::Result<ureq::http::Response<ureq::Body>> {
        let token = self.token().map(|t| format!("Bearer {t}")).unwrap_or_default();
        let mut fresh = false;
        loop {
            let (proof, pid) = self.process(fresh);
            let headers = [("authorization", token.clone()), ("x-workriver-place", self.place.clone().unwrap_or_default()), ("x-workriver-caller", proof.unwrap_or_default()), ("x-workriver-pid", pid.to_string())];
            let res = make(&headers).map_err(|e| self.unreachable(e))?;
            let expired = res.headers().get("x-workriver-caller").is_some_and(|v| v == crate::common::wire::EXPIRED);
            if !expired || fresh {
                return Ok(res);
            }
            fresh = true;
        }
    }

    fn call(&self, path: &str, body: Option<Value>) -> anyhow::Result<Value> {
        self.probe()?;
        let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).timeout_connect(Some(CONNECT)).timeout_global(Some(ANSWER)).build().into();
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let mut res = self.send(|headers| match &body {
            Some(b) => headers.iter().fold(agent.post(&url), |r, (k, v)| r.header(*k, v)).send_json(b),
            // Reading is as whoever this is, too.
            None => headers.iter().fold(agent.get(&url), |r, (k, v)| r.header(*k, v)).call(),
        })?;
        let ok = res.status().is_success();
        let text = read_answer(&mut res)?;
        if !ok {
            bail!("{text}");
        }
        serde_json::from_str(&text).with_context(|| format!("unexpected answer from 127.0.0.1:{}: {text}", self.port))
    }

    /// A file of the service's store by its name there (`<id>.<ext>`), as whoever this is.
    fn fetch(&self, name: &str) -> anyhow::Result<Vec<u8>> {
        self.probe()?;
        let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).timeout_connect(Some(CONNECT)).timeout_global(Some(Duration::from_secs(120))).build().into();
        let url = format!("http://127.0.0.1:{}/api/attachments/{name}", self.port);
        let mut res = self.send(|headers| headers.iter().fold(agent.get(&url), |r, (k, v)| r.header(*k, v)).call())?;
        if !res.status().is_success() {
            bail!("{}", read_answer(&mut res)?);
        }
        res.body_mut().with_config().limit(256 * 1024 * 1024).read_to_vec().map_err(|e| anyhow!("the service answered, but its answer could not be read: {e}"))
    }

    fn get(&self, path: &str) -> anyhow::Result<Value> {
        self.call(path, None)
    }

    fn token(&self) -> Option<String> {
        self.home.as_deref().and_then(common::home::read_token)
    }

    /// What a request says of who this is: nothing (the service finds it from this
    /// process); or, with `--as` (an agent's name, where the service allows it), that name.
    /// "user" in tests: nothing either.
    fn me(&self, actor: &str) -> Value {
        match actor {
            "" | "user" => json!({}),
            name => json!({ "as": name }),
        }
    }

    /// Puts a file into the attachment store, linked to a task (and a finding) if given.
    fn upload(&self, file: &Path, actor: &str, link: Option<(&str, u32, Option<&str>)>) -> anyhow::Result<Value> {
        let bytes = std::fs::read(file).with_context(|| format!("cannot read {}", file.display()))?;
        if bytes.len() > crate::common::wire::MAX_ATTACHMENT {
            bail!("{} is larger than {} MB, the most workriver stores", file.display(), crate::common::wire::MAX_ATTACHMENT / 1024 / 1024);
        }
        self.probe()?;
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
        let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).timeout_connect(Some(CONNECT)).timeout_global(Some(Duration::from_secs(120))).build().into();
        let me = self.me(actor);
        let mut res = self.send(|headers| {
            let mut req = headers.iter().fold(agent.post(format!("http://127.0.0.1:{}/api/attachments", self.port)), |r, (k, v)| r.header(*k, v)).query("name", &name).header("content-type", attachments::mime(&name, None));
            if let Some(v) = me["as"].as_str() {
                req = req.query("as", v);
            }
            if let Some((unit, task, finding)) = link {
                req = req.query("unit", unit).query("task", task.to_string());
                if let Some(f) = finding {
                    req = req.query("finding", f);
                }
            }
            req.send(&bytes[..])
        })?;
        let ok = res.status().is_success();
        let text = read_answer(&mut res)?;
        if !ok {
            bail!("{text}");
        }
        Ok(serde_json::from_str(&text)?)
    }

    /// A task's staging folder for screenshots, made if needed.
    fn inbox(&self, unit: &str, task: u32) -> anyhow::Result<PathBuf> {
        let v = self.call("/api/attachments/inbox", Some(json!({ "unit": unit, "task": task })))?;
        // The service is on another machine: the folder is on this one's disk, made here.
        if v["elsewhere"] == true {
            let d = crate::common::home::inbox(&common::home::data_dir(), unit, task);
            crate::common::home::mkdir(&d).with_context(|| format!("cannot make {}", d.display()))?;
            return Ok(d);
        }
        Ok(PathBuf::from(v["dir"].as_str().unwrap_or_default()))
    }

    fn append(&self, actor: &str, kind: &str, body: Value) -> anyhow::Result<Value> {
        let mut req = self.me(actor);
        req["kind"] = json!(kind);
        req["body"] = body;
        self.call("/api/append", Some(req))
    }
}

/// `A-B` (or one line `A`).
fn parse_lines(s: &str) -> anyhow::Result<(u32, u32)> {
    let err = || anyhow!("--lines {s:?}: write A-B, e.g. 10-24");
    let (a, b) = s.split_once('-').unwrap_or((s, s));
    Ok((a.trim().parse().map_err(|_| err())?, b.trim().parse().map_err(|_| err())?))
}

/// Polls until question `id` is answered (its text) or fails, or `timeout` passes.
fn wait_answer(d: &Daemon, id: &str, timeout: Duration) -> anyhow::Result<String> {
    let until = std::time::Instant::now() + timeout;
    loop {
        let state: core::State = serde_json::from_value(d.get("/api/state")?)?;
        let q = state.question(id).map_err(|e| anyhow!(e.0))?;
        if let Some(a) = &q.answer {
            return Ok(a.clone());
        }
        if let Some((code, why)) = &q.failed {
            bail!("question {id} got no answer ({}): {why}", name(code));
        }
        if std::time::Instant::now() >= until {
            bail!("no answer to {id} within {} s; it is still open, and the answer will come to your terminal", timeout.as_secs());
        }
        std::thread::sleep(Duration::from_millis(1000).min(until.saturating_duration_since(std::time::Instant::now()).max(Duration::from_millis(50))));
    }
}

/// The page's path for `i3`, `u1`, `u1#3` (`/u1/3`) or `u1#3:P2-1` (`/u1/3?finding=P2-1`).
fn link_path(s: &str) -> anyhow::Result<String> {
    let bad = || anyhow!("{s:?} is not an intent, unit or task; write it as i3, u1, u1#3 or u1#3:P2-1");
    let (at, finding) = s.split_once(':').map_or((s, None), |(a, f)| (a, Some(f)));
    let (id, n) = at.split_once('#').map_or((at, None), |(u, n)| (u, Some(n)));
    let digits = |x: &str| !x.is_empty() && x.bytes().all(|b| b.is_ascii_digit());
    if !(id.strip_prefix('u').or_else(|| id.strip_prefix('i')).is_some_and(digits) && n.is_none_or(digits)) {
        return Err(bad());
    }
    let mut path = format!("/{id}");
    if let Some(n) = n {
        path += &format!("/{n}");
    }
    match finding {
        None => {}
        Some(f) if n.is_some() && !f.is_empty() && f.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') => path += &format!("?finding={f}"),
        Some(_) => return Err(bad()),
    }
    Ok(path)
}

/// The page's address where the user opens it, as the service says (also through a tunnel).
fn page(d: &Daemon) -> anyhow::Result<String> {
    d.get("/api/status")?["url"].as_str().map(str::to_string).ok_or_else(|| anyhow!("the service does not say its address: it is an older build; restart `workriver serve`"))
}

fn task_ref(s: &str) -> anyhow::Result<(String, u32)> {
    parse_task_ref(s).map(|(u, n)| (u.to_string(), n)).ok_or_else(|| anyhow!("{s:?} is not a task; write it as <unit>#<n>, e.g. u1#3"))
}

/// `4:P2-1,P3-1` (this unit), `u1#4:P2-1` (another unit or intent), `f3,f4` (backlog findings).
fn parse_cause(s: &str) -> anyhow::Result<Value> {
    let ids = |x: &str| x.split(',').map(str::trim).filter(|i| !i.is_empty()).map(str::to_string).collect::<Vec<_>>();
    let Some((n, list)) = s.split_once(':') else {
        if s.split(',').all(|f| f.trim().strip_prefix('f').is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))) {
            return Ok(json!({ "task": 0, "findings": ids(s) }));
        }
        bail!("--cause {s:?}: write N:ID[,ID…] (4:P2-1), UNIT#N:ID (u1#4:P2-1), or backlog findings (f3,f4)");
    };
    if let Some((unit, n)) = parse_task_ref(n.trim()) {
        return Ok(json!({ "unit": unit, "task": n, "findings": ids(list) }));
    }
    let n: u32 = n.trim().trim_start_matches('#').parse().map_err(|_| anyhow!("--cause {s:?}: {n:?} is not a task number"))?;
    Ok(json!({ "task": n, "findings": ids(list) }))
}

/// One line per machine: whether it is followed and can be reached, and why not.
fn print_machines(list: &Value) {
    for m in list.as_array().into_iter().flatten() {
        let label = m["label"].as_str().unwrap_or("?");
        let state = if m["enabled"] != true {
            "disabled".to_string()
        } else if m["connecting"] == true {
            "connecting…".to_string()
        } else if m["reachable"] == true && m["tunnel"] == true {
            format!("connected, {} agents", m["agents"])
        } else if m["tunnel"] == true {
            "connected, but its Herdr does not answer".to_string()
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

/// `SYSTEM:KEY` and an optional link.
fn parse_external(external: Option<String>, url: Option<String>) -> anyhow::Result<Option<Value>> {
    let Some(e) = external else { return Ok(None) };
    let (system, key) = e.split_once(':').filter(|(s, k)| !s.trim().is_empty() && !k.trim().is_empty()).ok_or_else(|| anyhow!("--external {e:?}: write SYSTEM:KEY, e.g. jira:ABC-12"))?;
    Ok(Some(json!({ "system": system.trim(), "key": key.trim(), "url": url })))
}

/// A project as the daemon takes it: an id (p1) as is, a folder as its full path.
fn project_arg(p: &str) -> anyhow::Result<String> {
    if p.strip_prefix('p').is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit())) && !Path::new(p).exists() {
        return Ok(p.to_string());
    }
    Ok(std::fs::canonicalize(p).with_context(|| format!("project {p:?}: no such id or folder"))?.to_string_lossy().into_owned())
}

fn cwd_folder() -> anyhow::Result<String> {
    project_arg(".")
}

/// `TEXT` or `TEXT => COMMAND`
fn parse_step(s: &str) -> Value {
    match s.split_once("=>") {
        Some((text, cmd)) => json!({ "text": text.trim(), "command": cmd.trim() }),
        None => json!({ "text": s.trim() }),
    }
}

/// `COMMAND => passed: SUMMARY`; the command may itself hold `=>`, so the last one splits.
/// `COMMAND => passed: SUMMARY`, optionally `=> FILE` after it (a screenshot or report of
/// the check); the file comes back as the second value.
fn parse_evidence_file(s: &str) -> anyhow::Result<(Value, Option<PathBuf>)> {
    if let Some((head, last)) = s.rsplit_once("=>")
        && head.contains("=>")
        && parse_evidence(head).is_ok()
        && !last.trim().starts_with("passed")
        && !last.trim().starts_with("failed")
    {
        return Ok((parse_evidence(head)?, Some(PathBuf::from(last.trim()))));
    }
    Ok((parse_evidence(s)?, None))
}

fn parse_evidence(s: &str) -> anyhow::Result<Value> {
    let err = || anyhow!("--evidence {s:?}: write 'COMMAND => passed: SUMMARY' or 'COMMAND => failed: SUMMARY' (and `=> FILE` to attach one)");
    let (cmd, rest) = s.rsplit_once("=>").ok_or_else(err)?;
    let (result, summary) = rest.split_once(':').ok_or_else(err)?;
    let result = result.trim();
    if result != "passed" && result != "failed" {
        return Err(err());
    }
    Ok(json!({ "command": cmd.trim(), "result": result, "summary": summary.trim() }))
}

/// `task deliver`: stores the files it names and those in the task's folder (a name starting
/// with a finding id goes with that finding), records the delivery (or `--amend`), then
/// empties the folder.
fn deliver(d: &Daemon, actor: &str, task: &str, note: String, evidence: &[String], findings: Option<String>, amend: bool) -> anyhow::Result<Value> {
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

/// A compact text view: per unit its tasks, open findings and waiting gates; open chores.
fn board(d: &Daemon, target: Option<&str>) -> anyhow::Result<()> {
    print!("{}", board_text(d, target)?);
    println!("(one task in full, with its causes, notes and output: `workriver task show <unit>#<n>`)");
    Ok(())
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

/// The unit (or intent) a task is in: where its causes are, unless they say otherwise.
fn t_owner(state: &core::State, t: &core::Task) -> String {
    let has = |tasks: &[core::Task]| tasks.iter().any(|x| std::ptr::eq(x, t));
    state.units.iter().find(|u| has(&u.tasks)).map(|u| u.id.clone()).or_else(|| state.intents.iter().find(|i| has(&i.tasks)).map(|i| i.id.clone())).unwrap_or_default()
}

/// Since when a machine is disconnected, as the snapshot `ws` says; none while connected.
fn down_since(ws: &Value, machine: &str) -> Option<String> {
    let m = ws["machines"].as_array()?.iter().find(|m| m["label"] == machine)?;
    (m["tunnel"] != true && m["enabled"] == true).then(|| m["downSince"].as_str().unwrap_or("now").to_string())
}

/// The board as text, as the caller may see it.
fn board_text(d: &Daemon, target: Option<&str>) -> anyhow::Result<String> {
    use std::fmt::Write as _;
    let mut out = String::new();
    let state: core::State = serde_json::from_value(d.get("/api/state")?)?;
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
            let _ = writeln!(out, "       {}", show::stalled_line(&s));
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
        let _ = writeln!(out, "{} {}{}", i.id, i.title, state.manager_of(&i.id).map(|m| format!("  (manager {})", named(m))).unwrap_or_default());
        link(&mut out, "  ", &i.id);
        if i.state == core::IntentState::Dropped {
            let _ = writeln!(out, "  dropped: {}", i.reason.as_deref().unwrap_or(""));
        }
        if !i.goal.is_empty() {
            let _ = writeln!(out, "  goal: {}", i.goal);
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

#[cfg(test)]
mod tests;

/// The service's answer as text. A reply that cannot be read is said as that, not as the
/// service being unreachable: it answered.
// ponytail: whole answer in memory, capped at 256 MB; stream it if snapshots ever get there.
fn read_answer(res: &mut ureq::http::Response<ureq::Body>) -> anyhow::Result<String> {
    res.body_mut().with_config().limit(256 * 1024 * 1024).read_to_string().map_err(|e| anyhow!("the service answered, but its answer could not be read: {e}"))
}

/// One machine's history in a line: mean and highest CPU and memory share over the minutes.
fn history_text(m: &Value, points: &Value) -> String {
    let name = format!("{}{}", m["label"].as_str().unwrap_or("?"), if m["here"] == true { " (here)" } else { "" });
    let points = points.as_array().map(Vec::as_slice).unwrap_or_default();
    let stats = |v: Vec<f64>| if v.is_empty() { "-".to_string() } else { format!("mean {:.0}% max {:.0}%", v.iter().sum::<f64>() / v.len() as f64, v.iter().copied().fold(0.0, f64::max)) };
    let cpu = stats(points.iter().filter_map(|p| p["cpu"].as_f64()).collect());
    let mem = stats(points.iter().filter_map(|p| Some(p["memUsed"].as_f64()? * 100.0 / p["memTotal"].as_f64().filter(|t| *t > 0.0)?)).collect());
    if points.is_empty() { format!("{name}  no history yet") } else { format!("{name}  cpu {cpu}  memory {mem}  ({} min)", points.len()) }
}

/// `machine load` as text: one line per machine, its newest CPU and memory and alerts.
fn load_text(r: &Value) -> String {
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
