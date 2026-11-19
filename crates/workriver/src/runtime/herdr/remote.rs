// Another machine's Herdr: the same client behind a socket forwarded from it; where that
// socket is over there (as the machine's setting says, else asked of the machine) and here;
// and the option a machine's setting keeps it under.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use workriver_core::runtime::Runtime;

use super::Herdr;
use crate::common::config::{Options, Setting};
use crate::runtime::ssh::QUIET;

impl Herdr {
    /// The Herdr of another machine, behind a socket forwarded from it.
    pub fn on(socket: PathBuf, machine: &str, terminal_app: String) -> Herdr {
        Herdr { socket, terminal_app, ids: AtomicU64::new(1), panes: Mutex::default(), machine: Some(machine.into()) }
    }
}

/// The Herdr of machine `label`, behind its socket forwarded here.
pub fn connect(socket: PathBuf, label: &str, terminal_app: String) -> Arc<dyn Runtime> {
    Arc::new(Herdr::on(socket, label, terminal_app))
}

/// Where a machine's Herdr socket appears on this machine, in the data directory.
pub fn socket_here(home: &Path, label: &str) -> PathBuf {
    home.join(format!("herdr-{label}.sock"))
}

/// Where Herdr's socket is over there: as given, else asked of the machine.
pub fn socket_there(program: &str, setting: &Setting) -> Result<String, String> {
    socket_of(&setting.options).map(Ok).unwrap_or_else(|| ask_socket(program, &setting.ssh))
}

/// The key a machine's setting keeps Herdr's socket under, in `settings.json` and in what the
/// CLI sends: as it was when the setting had a field of that name.
const SOCKET: &str = "herdrSocket";

fn socket_of(options: &Options) -> Option<String> {
    options.get(SOCKET)?.as_str().map(str::to_string)
}

/// A machine's options with Herdr's socket over there, or none (then asked of the machine).
pub fn options(socket: Option<String>) -> Options {
    socket.map(|s| (SOCKET.to_string(), Value::String(s))).into_iter().collect()
}

/// Of what a request gave, the options Herdr knows; anything else is not kept.
pub fn known(given: &Options) -> Options {
    options(socket_of(given))
}

/// Prints Herdr's socket path on the machine, as Herdr itself works it out (default session).
const ASK_SOCKET: &str = r#"printf %s "${HERDR_SOCKET_PATH:-${XDG_CONFIG_HOME:-$HOME/.config}/herdr/herdr.sock}""#;

fn ask_socket(program: &str, ssh: &str) -> Result<String, String> {
    let mut cmd = Command::new(program);
    for o in QUIET {
        cmd.args(["-o", o]);
    }
    // This one sets up no forward, so the config's are cleared.
    cmd.args(["-o", "ClearAllForwardings=yes", "--", ssh, ASK_SOCKET]);
    let started = Instant::now();
    let out = crate::common::process::output_within(cmd, Duration::from_secs(20)).map_err(|e| format!("cannot run ssh: {e}"))?;
    tracing::debug!(target: "perf", kind = "ssh", purpose = "herdr_socket", ms = started.elapsed().as_millis() as u64, code = out.status.code().unwrap_or(-1));
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && path.starts_with('/') {
        Ok(path)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("ssh gave no socket path").trim().to_string())
    }
}
