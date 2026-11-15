// workriver: the daemon (`serve`) and the short-lived CLI that records work through it.
mod ask;
mod deliver;
mod herdr;
mod serve;
mod snapshot;

use std::io::Read;
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

Who records: inside a Herdr pane, the agent running there (by its session); outside, the
user. `--as NAME` overrides. Inside a Herdr workspace the intent is the one that follows
that workspace (created on first use), so commands that take an intent may leave it out.

Commits are read from git: a commit belongs to a task when its message has the trailer
line `Task: u1#3`. A commit on the unit's branch without one shows as unregistered.

Typical flow for an agent:
  workriver task start u1#3 --as agent-a
  …work, commit with `Task: u1#3`…
  workriver task deliver u1#3 --as agent-a --evidence 'cargo test => passed: 12 passed'";

#[derive(Parser)]
#[command(name = "workriver", version, about = "Keep track of many coding agents working in parallel.", long_about = ABOUT)]
struct Cli {
    /// Port of the daemon on 127.0.0.1.
    #[arg(long, global = true, env = "WORKRIVER_PORT", default_value_t = 47820)]
    port: u16,
    /// Who records this. Default: the agent of the Herdr pane this runs in, else "user".
    #[arg(long = "as", global = true, env = "WORKRIVER_AS", value_name = "NAME")]
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
    },
    /// Check that the daemon is running and show what it holds.
    Status,
    /// Intents: goals that group units.
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
    /// Chores: things asked of the user that block nobody.
    #[command(subcommand)]
    Chore(ChoreCmd),
    /// The manager: the one agent per intent that splits work and hands it out.
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
    /// Print the instructions for an agent working with workriver.
    Guide {
        /// `worker` (short; goes into every task message) or `manager` (the manager's skill).
        #[arg(value_parser = ["worker", "manager"])]
        role: String,
    },
    /// Print an intent or unit as text: its tasks, open findings, waiting gates and open
    /// chores. Without a target, everything.
    Board {
        /// An intent (i1) or unit (u1).
        target: Option<String>,
    },
}

#[derive(Subcommand)]
enum IntentCmd {
    /// Record a new intent; prints its id.
    Add {
        title: String,
        /// One sentence on what it is for.
        #[arg(long, default_value = "")]
        goal: String,
    },
    /// Change an intent's title or goal (its manager or the user only).
    Set {
        /// The intent (i1…).
        intent: String,
        #[arg(long)]
        title: Option<String>,
        /// One sentence on what it is for.
        #[arg(long)]
        goal: Option<String>,
    },
}

#[derive(Subcommand)]
enum UnitCmd {
    /// Record a unit of an intent; prints its id (u1…).
    Add {
        /// The intent it serves (i1…); default: the intent of this Herdr workspace.
        intent: Option<String>,
        #[arg(long)]
        title: String,
        /// One sentence on what it is for.
        #[arg(long, default_value = "")]
        purpose: String,
        /// The repository (or a worktree of it); default: the current directory.
        #[arg(long, default_value = ".")]
        repo: String,
        /// The branch the work happens on; it may not exist yet.
        #[arg(long)]
        branch: String,
        /// The branch it starts from and merges into.
        #[arg(long, default_value = "main")]
        base: String,
    },
    /// Record that a unit's branch was merged.
    Merged { unit: String },
}

#[derive(Subcommand)]
enum TaskCmd {
    /// Record a task in a unit; prints its ref (u1#3).
    ///
    /// Without --after or --cause the task starts from the unit itself. With --cause and
    /// no --after it follows the tasks whose findings it answers. A cause must name
    /// accepted findings.
    Add {
        /// The unit (u1…).
        unit: String,
        #[arg(long, value_parser = ["implement", "design", "review", "tests", "gate"], default_value = "implement")]
        kind: String,
        /// What the work is (needed for implement and gate).
        #[arg(long, default_value = "")]
        title: String,
        /// The full ask, as handed to the agent; default: the title.
        #[arg(long, default_value = "")]
        text: String,
        /// Task number this one follows; repeat for more. The first is the direct cause, the
        /// rest are merely involved.
        #[arg(long, value_name = "N")]
        after: Vec<String>,
        /// Findings this task answers: `N:ID[,ID…]`, e.g. `4:P2-1,P3-1`. Repeatable.
        #[arg(long, value_name = "N:IDS")]
        cause: Vec<String>,
        /// The agent it is handed to.
        #[arg(long)]
        agent: Option<String>,
        /// Review tasks: which kind of review.
        #[arg(long, value_parser = ["code-review", "security-review"])]
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
    },
    /// Mark a task as started (by the --as agent unless --agent is given).
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
    /// (suggestion is optional), read from a file or from stdin with `--findings -`.
    Deliver {
        /// The task (u1#3).
        task: String,
        #[arg(long, default_value = "")]
        note: String,
        /// A check that was run: `COMMAND => passed: SUMMARY` or `COMMAND => failed: SUMMARY`,
        /// e.g. `cargo test => passed: 12 passed`. Repeatable.
        #[arg(long)]
        evidence: Vec<String>,
        /// A JSON file of findings, or `-` for stdin.
        #[arg(long, value_name = "FILE")]
        findings: Option<String>,
        /// Correct a task already delivered: what is given (note, evidence, findings)
        /// replaces what was delivered. Findings only while none of them is decided.
        #[arg(long)]
        amend: bool,
    },
}

#[derive(Subcommand)]
enum FindingCmd {
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
    Add {
        /// The intent it belongs to (i1…); default: the intent of this Herdr workspace.
        intent: Option<String>,
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
    /// Register the agent of this Herdr pane as the manager of its workspace's intent.
    /// Accepted findings and gate results without a waiting agent are delivered to it.
    Here {
        /// The intent, when not run inside a Herdr workspace.
        #[arg(long)]
        intent: Option<String>,
        /// One sentence on what the intent is for (also `workriver intent set --goal`).
        #[arg(long)]
        goal: Option<String>,
    },
}

const GUIDE_WORKER: &str = include_str!("../../../skills/workriver-manager/references/worker.md");
const GUIDE_MANAGER: &str = include_str!("../../../skills/workriver-manager/SKILL.md");

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match &cli.cmd {
        Cmd::Serve { terminal_app } => {
            tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).init();
            let store = core::Store::open_default().context("cannot open the workriver database")?;
            let runtime = herdr::Herdr::new(herdr::socket_path(), terminal_app.clone());
            // Without Herdr the daemon still records and shows; it just sees no agents.
            let warning = runtime.check();
            if let Some(w) = &warning {
                tracing::warn!("{w}");
            }
            let rt = tokio::runtime::Runtime::new()?;
            return rt.block_on(async {
                let listener = serve::bind(cli.port).await?;
                serve::serve(listener, store, std::sync::Arc::new(runtime), warning, serve::Waits::default()).await
            });
        }
        Cmd::Guide { role } => {
            print!("{}", if role == "worker" { GUIDE_WORKER } else { GUIDE_MANAGER });
            return Ok(());
        }
        _ => {}
    }
    let d = Daemon { port: cli.port };
    // Who records, and in which intent, from the runtime place the CLI runs in.
    let needs_intent = matches!(cli.cmd, Cmd::Unit(UnitCmd::Add { intent: None, .. }) | Cmd::Chore(ChoreCmd::Add { intent: None, .. }) | Cmd::Manager(ManagerCmd::Here { intent: None, .. }));
    let records = !matches!(cli.cmd, Cmd::Status | Cmd::Board { .. });
    let place = herdr::place_from_env();
    let here = match &place {
        Some(place) if records && (cli.actor.is_none() || needs_intent) => Some(
            d.call("/api/here", Some(json!({ "place": place, "intent": needs_intent }))).map_err(|e| anyhow!(herdr::cannot_ask(place, &e.to_string())))?,
        ),
        _ => None,
    };
    let actor = if records { actor_of(cli.actor.clone(), place.as_deref(), here.as_ref())? } else { "user".into() };
    let actor = actor.as_str();
    let here_intent = |given: Option<String>| {
        given
            .or_else(|| here.as_ref().and_then(|h| h["intent"].as_str().map(str::to_string)))
            .ok_or_else(|| anyhow!("no intent given, and not inside a Herdr workspace; name one (see `workriver intent add`)"))
    };
    let by = if actor == "user" { "user" } else { "agent" };
    let out = match cli.cmd {
        Cmd::Serve { .. } | Cmd::Guide { .. } => unreachable!(),
        Cmd::Status => {
            let s = d.get("/api/status")?;
            println!("workriver {} is running", s["version"].as_str().unwrap_or("?"));
            println!("web UI: http://127.0.0.1:{}", s["port"]);
            println!("database: {}", s["db"].as_str().unwrap_or("?"));
            println!("{} events, {} intents, {} units", s["events"], s["intents"], s["units"]);
            let rt = &s["runtime"];
            println!("agent runtime: {}", if rt["connected"] == true { "connected" } else { "not connected" });
            if let Some(w) = rt["warning"].as_str() {
                println!("  at start: {w}");
            }
            return Ok(());
        }
        Cmd::Manager(ManagerCmd::Here { intent, goal }) => {
            if actor == "user" {
                bail!("run this from the manager agent's Herdr pane (or pass --as <agent>)");
            }
            let intent = here_intent(intent)?;
            let out = d.append(actor, "manager.registered", json!({ "intent": intent, "agent": actor }))?;
            if let Some(goal) = goal {
                d.append(actor, "intent.updated", json!({ "intent": intent, "goal": goal }))?;
            }
            out
        }
        Cmd::Board { target } => return board(&d, target.as_deref()),
        Cmd::Intent(IntentCmd::Add { title, goal }) => d.append(actor, "intent.created", json!({ "title": title, "goal": goal }))?,
        Cmd::Intent(IntentCmd::Set { intent, title, goal }) => d.append(actor, "intent.updated", json!({ "intent": intent, "title": title, "goal": goal }))?,
        Cmd::Unit(UnitCmd::Add { intent, title, purpose, repo, branch, base }) => {
            let intent = here_intent(intent)?;
            let repo = std::fs::canonicalize(&repo).with_context(|| format!("no folder {repo}"))?;
            d.append(actor, "unit.created", json!({ "intent": intent, "title": title, "purpose": purpose, "repo": repo, "branch": branch, "base": base }))?
        }
        Cmd::Unit(UnitCmd::Merged { unit }) => d.append(actor, "unit.merged", json!({ "unit": unit }))?,
        Cmd::Task(TaskCmd::Add { unit, kind, title, text, after, cause, agent, reviewer, gate, step, plan }) => {
            let inputs: Vec<Value> = after.iter().enumerate().map(|(k, n)| json!({ "from": n, "primary": k == 0 })).collect();
            let causes = cause.iter().map(|c| parse_cause(c)).collect::<anyhow::Result<Vec<_>>>()?;
            let steps: Vec<Value> = step.iter().map(|s| parse_step(s)).collect();
            d.append(actor, "task.created", json!({
                "unit": unit, "kind": kind, "title": title, "text": text, "inputs": inputs, "causes": causes,
                "agent": agent, "reviewer": reviewer, "gate": gate, "steps": steps, "plans": plan,
            }))?
        }
        Cmd::Task(TaskCmd::Start { task, agent }) => {
            let (unit, n) = task_ref(&task)?;
            d.append(actor, "task.started", json!({ "unit": unit, "task": n, "agent": agent }))?
        }
        Cmd::Task(TaskCmd::Deliver { task, note, evidence, findings, amend }) => {
            let (unit, n) = task_ref(&task)?;
            let evidence = evidence.iter().map(|e| parse_evidence(e)).collect::<anyhow::Result<Vec<_>>>()?;
            if amend {
                // Only what is given replaces what was delivered.
                let note = (!note.is_empty()).then_some(note);
                let evidence = (!evidence.is_empty()).then_some(evidence);
                let findings = findings.map(|f| read_findings(Some(&f))).transpose()?;
                d.append(actor, "task.amended", json!({ "unit": unit, "task": n, "note": note, "evidence": evidence, "findings": findings }))?
            } else {
                let findings = read_findings(findings.as_deref())?;
                d.append(actor, "task.delivered", json!({ "unit": unit, "task": n, "note": note, "evidence": evidence, "findings": findings }))?
            }
        }
        Cmd::Finding(FindingCmd::Decide { task, finding, decision, note }) => {
            let (unit, n) = task_ref(&task)?;
            d.append(actor, "finding.decided", json!({ "unit": unit, "task": n, "finding": finding, "decision": decision, "note": note, "by": by }))?
        }
        Cmd::Gate(GateCmd::Submit { task, result, note, findings }) => {
            let (unit, n) = task_ref(&task)?;
            let findings = read_findings(findings.as_deref())?;
            d.append(actor, "gate.submitted", json!({ "unit": unit, "task": n, "result": result, "note": note, "findings": findings }))?
        }
        Cmd::Chore(ChoreCmd::Add { intent, title, unit, step }) => {
            let intent = here_intent(intent)?;
            let steps: Vec<Value> = step.iter().map(|s| parse_step(s)).collect();
            d.append(actor, "chore.created", json!({ "intent": intent, "title": title, "unit": unit, "steps": steps }))?
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
            let e = d.call("/api/ask", Some(json!({ "by": actor, "unit": unit, "task": n, "text": question, "context": context })))?;
            let id = e["body"]["id"].as_str().unwrap_or("?").to_string();
            if !wait {
                println!("{id}");
                return Ok(());
            }
            println!("{}", wait_answer(&d, &id, Duration::from_secs(timeout))?);
            return Ok(());
        }
        Cmd::Answer { question, text, asked, conclusion } => d.append(actor, "question.answered", json!({ "question": question, "text": text, "asked": asked, "conclusion": conclusion }))?,
    };
    // What was recorded, by the id an agent uses next.
    let b = &out["body"];
    let made = match out["kind"].as_str() {
        Some("task.created") => format!("{}#{}", b["unit"].as_str().unwrap_or("?"), b["n"]),
        Some("intent.created" | "unit.created" | "chore.created") => b["id"].as_str().unwrap_or("?").to_string(),
        Some("manager.registered") => format!("manager of {}", b["intent"].as_str().unwrap_or("?")),
        Some("task.amended") => "amended".into(),
        Some("question.answered") => "answered".into(),
        Some("task.delivered") => {
            let ids: Vec<&str> = b["findings"].as_array().into_iter().flatten().filter_map(|f| f["id"].as_str()).collect();
            if ids.is_empty() { "recorded".into() } else { format!("recorded findings {}", ids.join(" ")) }
        }
        _ => "recorded".into(),
    };
    println!("{made}");
    Ok(())
}

struct Daemon {
    port: u16,
}

impl Daemon {
    fn call(&self, path: &str, body: Option<Value>) -> anyhow::Result<Value> {
        let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(Duration::from_secs(60))).build().into();
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let res = match body {
            Some(b) => agent.post(&url).send_json(b),
            None => agent.get(&url).call(),
        };
        let mut res = res.map_err(|e| anyhow!("cannot reach workriver on 127.0.0.1:{} ({e}). Is `workriver serve` running?", self.port))?;
        let ok = res.status().is_success();
        let text = res.body_mut().read_to_string()?;
        if !ok {
            bail!("{text}");
        }
        serde_json::from_str(&text).with_context(|| format!("unexpected answer from 127.0.0.1:{}: {text}", self.port))
    }

    fn get(&self, path: &str) -> anyhow::Result<Value> {
        self.call(path, None)
    }

    fn append(&self, actor: &str, kind: &str, body: Value) -> anyhow::Result<Value> {
        self.call("/api/append", Some(json!({ "actor": actor, "kind": kind, "body": body })))
    }
}

/// Who records: `--as`, else the agent at the runtime place the CLI runs in (an error if
/// there is none), else, outside any runtime, the user.
fn actor_of(explicit: Option<String>, place: Option<&str>, here: Option<&Value>) -> anyhow::Result<String> {
    if let Some(a) = explicit {
        return Ok(a);
    }
    match (place, here) {
        (Some(p), Some(h)) => h["actor"].as_str().map(str::to_string).ok_or_else(|| anyhow!(herdr::no_agent_at(p))),
        _ => Ok("user".into()),
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

fn task_ref(s: &str) -> anyhow::Result<(String, u32)> {
    parse_task_ref(s).map(|(u, n)| (u.to_string(), n)).ok_or_else(|| anyhow!("{s:?} is not a task; write it as <unit>#<n>, e.g. u1#3"))
}

/// `4:P2-1,P3-1`
fn parse_cause(s: &str) -> anyhow::Result<Value> {
    let (n, ids) = s.split_once(':').ok_or_else(|| anyhow!("--cause {s:?}: write N:ID[,ID…], e.g. 4:P2-1"))?;
    let n: u32 = n.trim().trim_start_matches('#').parse().map_err(|_| anyhow!("--cause {s:?}: {n:?} is not a task number"))?;
    Ok(json!({ "task": n, "findings": ids.split(',').map(str::trim).filter(|i| !i.is_empty()).collect::<Vec<_>>() }))
}

/// `TEXT` or `TEXT => COMMAND`
fn parse_step(s: &str) -> Value {
    match s.split_once("=>") {
        Some((text, cmd)) => json!({ "text": text.trim(), "command": cmd.trim() }),
        None => json!({ "text": s.trim() }),
    }
}

/// `COMMAND => passed: SUMMARY`; the command may itself hold `=>`, so the last one splits.
fn parse_evidence(s: &str) -> anyhow::Result<Value> {
    let err = || anyhow!("--evidence {s:?}: write 'COMMAND => passed: SUMMARY' or 'COMMAND => failed: SUMMARY'");
    let (cmd, rest) = s.rsplit_once("=>").ok_or_else(err)?;
    let (result, summary) = rest.split_once(':').ok_or_else(err)?;
    let result = result.trim();
    if result != "passed" && result != "failed" {
        return Err(err());
    }
    Ok(json!({ "command": cmd.trim(), "result": result, "summary": summary.trim() }))
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
    for i in &state.intents {
        let units: Vec<_> = state.units.iter().filter(|u| u.intent == i.id && wanted(&i.id, Some(&u.id))).collect();
        let chores: Vec<_> = state.chores.iter().filter(|c| c.intent == i.id && !c.done && wanted(&i.id, c.unit.as_deref())).collect();
        if units.is_empty() && chores.is_empty() && !wanted(&i.id, None) {
            continue;
        }
        let named = |s: &str| state.name_of(s).unwrap_or(s).to_string();
        println!("{} {}{}", i.id, i.title, i.manager.as_ref().map(|m| format!("  (manager {})", named(m))).unwrap_or_default());
        if !i.goal.is_empty() {
            println!("  goal: {}", i.goal);
        }
        for u in units {
            let merged = if u.merged.is_some() { "  merged" } else { "" };
            println!("  {} {}  [branch {} from {}, in {}]{merged}", u.id, u.title, u.branch, u.base, u.repo);
            for t in &u.tasks {
                let what = match t.kind {
                    TaskKind::Review => format!("review ({})", t.reviewer.map(|r| name(&r)).unwrap_or_default()),
                    TaskKind::Gate => format!("gate ({})", t.gate.map(|g| name(&g)).unwrap_or_default()),
                    k => name(&k),
                };
                let after: Vec<String> = t.inputs.iter().map(|i| if i.from == "start" { "start".into() } else { format!("#{}", i.from) }).collect();
                let agent = t.agent.as_ref().map(|a| format!(" @{}", named(a))).unwrap_or_default();
                println!("    #{} {:<8} {what} {}{agent}  after {}", t.n, name(&t.state), t.title, after.join(","));
                for f in t.findings.iter().filter(|f| f.state == FindingState::Open) {
                    println!("       open {}: {}  ({})", f.found.id, f.found.problem, f.found.r#where.join(", "));
                }
                if t.state == TaskState::Waiting {
                    println!("       waiting for the user");
                }
            }
            let deferred: Vec<String> = u
                .tasks
                .iter()
                .flat_map(|t| t.findings.iter().filter(|f| f.state == FindingState::Deferred).map(move |f| format!("#{} {}: {}", t.n, f.found.id, f.found.problem)))
                .collect();
            if !deferred.is_empty() {
                println!("    deferred:");
                for d in deferred {
                    println!("      {d}");
                }
            }
        }
        for c in chores {
            println!("  chore {} {} (by {})", c.id, c.title, state.name_of(&c.by).unwrap_or(&c.by));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
