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
use crate::{backlog, herdr};

mod client;
mod display;
mod guide;
mod parse;

pub(crate) use client::*;
pub(crate) use display::*;
pub(crate) use guide::*;
pub(crate) use parse::*;

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


#[derive(Parser)]
#[command(name = "workriver", version = BUILD, about = "Keep track of many coding agents working in parallel.", long_about = ABOUT)]
pub(crate) struct Cli {
    /// Port of the daemon on 127.0.0.1. Default: on a machine workriver follows over ssh,
    /// the port of its current connection (in ~/.config/workriver/port); else 47820.
    #[arg(long, global = true, env = "WORKRIVER_PORT")]
    pub(crate) port: Option<u16>,
    /// For tests and development: record as this agent instead of the one in this Herdr
    /// pane. Refused unless the service was started with WORKRIVER_ALLOW_AS=1; never the user.
    #[arg(long = "as", global = true, env = "WORKRIVER_AS", value_name = "AGENT", hide = true)]
    actor: Option<String>,
    #[command(subcommand)]
    pub(crate) cmd: Cmd,
}

#[derive(Subcommand)]
pub(crate) enum Cmd {
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
pub(crate) enum MachineCmd {
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
pub(crate) enum TaskCmd {
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
pub(crate) enum ChoreCmd {
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

/// The CLI's commands: everything but `serve`, which `main` starts.
pub(crate) fn run(cli: Cli) -> anyhow::Result<()> {
    match &cli.cmd {
        Cmd::Serve { .. } => unreachable!("main starts the service"),
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
                print!("{}", display::text(&shown));
            }
            return Ok(());
        }
        Cmd::Finding(FindingCmd::Show { task, finding }) => {
            let shown = task_show(&d, Some(&task))?;
            let f = shown.output.iter().flat_map(|o| &o.findings).find(|f| f.id == finding).ok_or_else(|| anyhow!("{task} has no finding {finding}"))?;
            println!("{}  {}  {}", shown.reference, shown.kind, shown.title);
            print!("{}", display::finding_text(f, ""));
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

/// Polls until question `id` is answered (its text) or fails, or `timeout` passes.
pub(crate) fn wait_answer(d: &Daemon, id: &str, timeout: Duration) -> anyhow::Result<String> {
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

