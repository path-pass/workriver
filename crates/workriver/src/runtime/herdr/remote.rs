// Another machine's Herdr: the same client behind a socket forwarded from it; where that
// socket is over there (as the machine's setting says, else asked of the machine) and here;
// and what a machine's setting keeps of it.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use workriver_core::runtime::Runtime;

use super::Herdr;
use crate::common::config::Setting;
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

/// How long a Unix socket's path may be, its closing NUL included (`sun_path`).
#[cfg(target_os = "linux")]
pub const SOCKET_PATH_MAX: usize = 108;
#[cfg(not(target_os = "linux"))]
pub const SOCKET_PATH_MAX: usize = 104;

/// The sockets of a machine's Herdr brought here, each as (here, there): its API socket, and
/// next to it the client socket its terminal stream is served on. That second one only when
/// its path here can be bound: Herdr's terminal stream finds it only next to the API socket,
/// and its name is longer. Left out, the machine's agents are followed and answered as before,
/// and only its terminal view says it cannot open; ssh would otherwise end the whole
/// connection on a forward it cannot set up.
pub fn forwards(home: &Path, label: &str, there: &str) -> Vec<(String, String)> {
    let here = socket_here(home, label);
    let client = |p: &Path| super::screens::client_socket(p).to_string_lossy().into_owned();
    let mut all = vec![(here.to_string_lossy().into_owned(), there.to_string())];
    let near = client(&here);
    if near.len() < SOCKET_PATH_MAX {
        all.push((near, client(Path::new(there))));
    }
    all
}

/// What a machine's setting keeps for Herdr, in `settings.json` and in what the service is
/// asked to add: its socket over there, a path or nothing (anything else is refused, and makes
/// a settings file unreadable); none is not written. Other keys of a machine's entry are not kept.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Options {
    #[serde(default, rename = "herdrSocket", skip_serializing_if = "Option::is_none")]
    pub socket: Option<String>,
}

/// Where Herdr's socket is over there: as given, else asked of the machine.
pub fn socket_there(program: &str, setting: &Setting<Options>) -> Result<String, String> {
    setting.options.socket.clone().map(Ok).unwrap_or_else(|| ask_socket(program, &setting.ssh))
}

/// What `machine add` sends of these options: the socket under its key, null when not given.
pub fn request(socket: Option<String>) -> Value {
    json!({ "herdrSocket": socket })
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
