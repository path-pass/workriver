// The commands of the agents workriver holds itself (hosted): the user starts, answers and
// closes them; a workspace's manager prompts, reads and cancels its own workers.
use super::*;

pub(super) fn run(cmd: Cmd, cx: &Cx) -> anyhow::Result<Option<Value>> {
    let Cx { d, .. } = *cx;
    let Cmd::Agent(cmd) = cmd else { unreachable!("not one of the hosted agents' commands") };
    let at = |agent: &str, what: &str| format!("/api/hosted/{}/{what}", encode(agent));
    match cmd {
        AgentCmd::Start { workspace, name, cwd, model, harness, auth, wait } => {
            let cwd = std::path::absolute(&cwd)?.to_string_lossy().into_owned();
            let r = d.call("/api/hosted/start", Some(json!({"workspace": workspace, "name": name, "cwd": cwd, "model": model.unwrap_or_default(), "harness": harness, "auth": auth})))?;
            let id = r["agent"].as_str().unwrap_or("?").to_string();
            if !wait {
                println!("{id} starting; `workriver agent show {id}` tells how it goes");
                return Ok(None);
            }
            let shown = until_not(d, &id, "starting", Duration::from_secs(70))?;
            print_agent(&shown);
        }
        AgentCmd::List => {
            let r = d.get("/api/hosted")?;
            if r["on"] != true {
                println!("the hosted runtime is off (`workriver agent hosted on` turns it on)");
            }
            for a in r["agents"].as_array().into_iter().flatten() {
                println!("{:<5} {:<20} {:<12} {} {}", text(&a["id"]), text(&a["name"]), text(&a["state"]), text(&a["workspace"]), text(&a["cwd"]));
            }
        }
        AgentCmd::Show { agent } => print_agent(&d.get(&format!("/api/hosted/{}", encode(&agent)))?),
        AgentCmd::Prompt { agent, text } => {
            d.call(&at(&agent, "prompt"), Some(json!({"text": text})))?;
            println!("given to {agent}");
        }
        AgentCmd::Read { agent, after, limit } => {
            let page = d.get(&format!("{}?after={after}&limit={limit}", at(&agent, "read")))?;
            let entries = page["entries"].as_array().cloned().unwrap_or_default();
            for e in &entries {
                println!("{e}");
            }
            let last = entries.last().and_then(|e| e["seq"].as_u64()).unwrap_or(after);
            if page["head"].as_u64().is_some_and(|h| h > last) {
                println!("more: `workriver agent read {agent} --after {last}`");
            }
        }
        AgentCmd::Status { agent } => println!("{}", serde_json::to_string_pretty(&d.get(&at(&agent, "status"))?)?),
        AgentCmd::Cancel { agent } => {
            let r = d.call(&at(&agent, "cancel"), Some(json!({})))?;
            println!("{agent}: {}", text(&r["state"]));
        }
        AgentCmd::Answer { agent, request, decision } => {
            let r = d.call(&at(&agent, "answer"), Some(json!({"request": request, "decision": decision})))?;
            let told = if r["replied"] == true { "it was told" } else { "whether it was told is not confirmed" };
            println!("{agent}: request {request} answered {decision}; {told}");
        }
        AgentCmd::Close { agent, wait } => {
            d.call(&at(&agent, "close"), Some(json!({})))?;
            if !wait {
                println!("{agent} closing; `workriver agent show {agent}` tells how it ends");
                return Ok(None);
            }
            print_agent(&until_not(d, &agent, "closing", Duration::from_secs(45))?);
        }
        AgentCmd::Processes { close, start, marker, all } => {
            if close.is_some() || all {
                let r = d.call("/api/hosted/processes/close", Some(json!({"pid": close, "start": start, "marker": marker, "all": all})))?;
                for p in r["processes"].as_array().into_iter().flatten() {
                    println!("{:<7} {:<5} {}", p["pid"], text(&p["agent"]), text(&p["outcome"]));
                }
                return Ok(None);
            }
            let r = d.get("/api/hosted/processes")?;
            for (title, key) in [("running", "running"), ("left over", "leftover")] {
                let list = r[key].as_array().cloned().unwrap_or_default();
                println!("{title}: {}", if list.is_empty() { "none" } else { "" });
                for p in list {
                    println!("  {:<7} {:<5} gen {} {:<16} started {} {}", p["pid"], text(&p["agent"]), p["generation"], text(&p["command"]), text(&p["started"]), text(&p["why"]));
                    if key == "leftover" {
                        println!("    workriver agent processes --close {} --start {} --marker {}", p["pid"], p["start"], text(&p["marker"]));
                    }
                }
            }
        }
        AgentCmd::Hosted { on } => {
            let r = match on {
                Some(on) => d.call("/api/settings/hosted", Some(json!({"on": on == "on"})))?,
                None => d.get("/api/settings/hosted")?,
            };
            println!("the hosted runtime is {}", if r["on"] == true { "on" } else { "off" });
        }
    }
    Ok(None)
}

/// Shows the agent again, in short calls, until its state is not `state` or `wait` is over.
fn until_not(d: &Daemon, agent: &str, state: &str, wait: Duration) -> anyhow::Result<Value> {
    let until = std::time::Instant::now() + wait;
    loop {
        let shown = d.get(&format!("/api/hosted/{}", encode(agent)))?;
        if shown["state"] != state || std::time::Instant::now() >= until {
            return Ok(shown);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

fn print_agent(a: &Value) {
    println!("{} {} ({}): {}", text(&a["id"]), text(&a["name"]), text(&a["harness"]), text(&a["state"]));
    for (label, key) in [("workspace", "workspace"), ("folder", "cwd"), ("model", "model"), ("session", "session"), ("place", "place"), ("version", "version"), ("account", "account"), ("approvals", "approvals"), ("start ended", "ended")] {
        if let Some(v) = a[key].as_str().filter(|v| !v.is_empty()) {
            println!("  {label}: {v}");
        }
    }
    if let Some(pid) = a["host"].as_u64() {
        println!("  host process: {pid}");
    }
    if let Some(c) = a["closed"].as_object() {
        println!("  closed: {} {}", c["outcome"].as_str().unwrap_or("?"), c.get("pids").map(|p| p.to_string()).unwrap_or_default());
        if let Some(error) = c.get("markedError").and_then(Value::as_str) {
            println!("    {error}");
        }
        for p in c.get("processes").and_then(Value::as_array).into_iter().flatten() {
            println!("    process {}: {}", p["pid"], text(&p["outcome"]));
        }
    }
    if let Some(p) = a["pending"].as_object().filter(|p| !p.is_empty()) {
        println!("  waiting for your answer (`workriver agent answer {} <request> allow|reject`):", text(&a["id"]));
        for (key, r) in p {
            let call = &r["acp"]["params"]["toolCall"];
            let offered: Vec<&str> = r["acp"]["params"]["options"].as_array().into_iter().flatten().filter_map(|o| o["optionId"].as_str()).collect();
            println!("    {key}: {} [{}]", call["title"].as_str().unwrap_or("?"), offered.join(", "));
        }
    }
}

/// A path segment as the service reads it back.
fn encode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

#[derive(Subcommand)]
pub(crate) enum AgentCmd {
    /// Start an agent workriver holds itself (the user's alone; the hosted runtime must be
    /// on). Answers at once; `--wait` waits until it has started or its start has ended.
    Start {
        /// The workspace it works for.
        #[arg(long)]
        workspace: String,
        /// What it is called; one open hosted agent has a name at a time.
        #[arg(long)]
        name: String,
        /// The folder it works in.
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
        /// The model; none: the harness's own default.
        #[arg(long)]
        model: Option<String>,
        #[arg(long, default_value = "codex")]
        harness: String,
        /// How the harness signs in.
        #[arg(long, value_parser = ["chatgpt", "api_key"])]
        auth: String,
        #[arg(long)]
        wait: bool,
    },
    /// The hosted agents you may drive, with their state.
    List,
    /// One hosted agent: its state, where it runs, what waits for your answer.
    Show { agent: String },
    /// Give it a message, as the user or as its workspace's manager; refused while it is in a
    /// turn or waits for an answer.
    Prompt {
        agent: String,
        #[arg(long)]
        text: String,
    },
    /// A page of its conversation (JSON lines from its journal).
    Read {
        agent: String,
        /// Entries after this one.
        #[arg(long, default_value_t = 0)]
        after: u64,
        #[arg(long, default_value_t = 200)]
        limit: usize,
    },
    /// Its state and the approval requests waiting, whole.
    Status { agent: String },
    /// Stop its turn: what it waits for is settled first.
    Cancel { agent: String },
    /// Answer an approval request, once (the user's alone).
    Answer {
        agent: String,
        /// The request's key, as `agent show` lists it.
        request: u64,
        #[arg(value_parser = ["allow", "reject"])]
        decision: String,
    },
    /// Close it (the user's alone): its host ends its processes and says how that went.
    Close {
        agent: String,
        #[arg(long)]
        wait: bool,
    },
    /// The processes of the hosted agents (the user's): each running agent's, and those left
    /// over (its agent closed or gone, an older launch). Only `--close` or `--all` ends one:
    /// nothing ends them by itself. A process that cleared its environment, or one macOS
    /// keeps its environment from (its own binaries: sh, sleep), is not found.
    Processes {
        /// End this leftover process, with the start and marker from the list.
        #[arg(long, value_name = "PID", conflicts_with = "all", requires_all = ["start", "marker"])]
        close: Option<u32>,
        /// The process start value listed with --close.
        #[arg(long, requires = "close")]
        start: Option<u64>,
        /// The launch marker listed with --close.
        #[arg(long, requires = "close")]
        marker: Option<String>,
        /// End every leftover process.
        #[arg(long)]
        all: bool,
    },
    /// Whether the hosted runtime (an experiment) is on; the user turns it on or off. Off
    /// refuses new starts only: running hosted agents are still read, answered and closed.
    Hosted {
        #[arg(value_parser = ["on", "off"])]
        on: Option<String>,
    },
}
