// The commands of machines: the ones followed, and the load of each.
use super::*;

/// The commands of machines and their load: what each records (printed by `run` as before), or None when it
/// printed what it had to say itself.
pub(super) fn run(cmd: Cmd, cx: &Cx) -> anyhow::Result<Option<Value>> {
    let Cx { d, .. } = *cx;
    match cmd {
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
            Ok(None)
        }
        Cmd::Machine(cmd) => {
            let (label, action, extra) = match cmd {
                MachineCmd::Load { .. } => unreachable!("handled above"),
                MachineCmd::List => (String::new(), "", json!({})),
                MachineCmd::Add { label, ssh, socket } => {
                    let mut body = runtime::socket_request(socket);
                    body["ssh"] = json!(ssh);
                    (label, "add", body)
                }
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
            Ok(None)
        }
        _ => unreachable!("not one of machines and their load"),
    }
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
        #[arg(id = runtime::words::SOCKET_ARG, long = runtime::words::SOCKET_FLAG, value_name = "PATH", help = runtime::words::HELP_SOCKET)]
        socket: Option<String>,
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
