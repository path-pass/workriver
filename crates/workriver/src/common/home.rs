// The data directory and the name of every file in it, in one place: whoever reads or writes
// one of them, the CLI or the service, finds it here.
use std::path::{Path, PathBuf};

/// Where workriver keeps its data: `$WORKRIVER_HOME`, or `~/workriver-data`.
pub fn data_dir() -> PathBuf {
    match std::env::var_os("WORKRIVER_HOME") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => std::env::home_dir().unwrap_or_default().join("workriver-data"),
    }
}

// ---- the files of the data directory ----

/// The database: the one SQLite file of events (workriver-data's store opens it).
pub const DATABASE: &str = "workriver.db";
/// The user token: whoever reads it, from a terminal no agent runs in, is the user.
pub const TOKEN: &str = "user-token";
/// The sessions the page was given: a hash and the second it ends, one per line.
pub const SESSIONS: &str = "user-sessions";
/// The socket's file in the data directory: the kernel names who connects to it.
pub const SOCKET: &str = "caller.sock";
/// The pins' file in the data directory: the root process each session was first seen at.
pub const PINS: &str = "session-roots.json";
/// The user's settings: machines followed, alert thresholds, how long a worker may stop.
pub const SETTINGS: &str = "settings.json";
/// The attachment store, and in it the folder where agents stage a task's files.
pub const ATTACHMENTS: &str = "attachments";
pub const INBOX: &str = "inbox";
/// The timing log's folder and its current file.
pub const LOGS: &str = "logs";
pub const PERF_LOG: &str = "perf.jsonl";
/// Each machine's load, a file per machine (`<label>.jsonl`).
pub const METRICS: &str = "metrics";
/// What the git scan keeps between starts.
pub const SCAN_CACHE: &str = "scan-cache";
/// Which managers were told of a guide's update.
pub const GUIDE_TOLD: &str = "guide-told.json";

// ---- on a followed machine, under its `$HOME` ----

/// Where the CLI on a followed machine finds the port that leads here.
pub const PORT_FILE: &str = ".config/workriver/port";
/// And the name of the computer it leads to, for its error messages.
pub const HOST_FILE: &str = ".config/workriver/daemon";

/// The database's path in the data directory, the directory and the file created first if
/// needed (0700 / 0600), so they start private.
pub fn database() -> std::io::Result<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let dir = data_dir();
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
    let path = dir.join(DATABASE);
    // Created here so it starts private; SQLite gives its -wal and -shm files the same mode.
    std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(&path)?;
    Ok(path)
}

pub fn token_path(home: &Path) -> PathBuf {
    home.join(TOKEN)
}

/// The token as the CLI reads it; None when there is none or it cannot be read.
pub fn read_token(home: &Path) -> Option<String> {
    std::fs::read_to_string(token_path(home)).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

pub fn attachments_dir(home: &Path) -> PathBuf {
    home.join(ATTACHMENTS)
}

/// Where agents put a task's screenshots before delivering it.
pub fn inbox(home: &Path, unit: &str, task: u32) -> PathBuf {
    attachments_dir(home).join(INBOX).join(format!("{unit}-{task}"))
}

/// A folder of the data directory, made private (0700) with its parents.
pub fn mkdir(d: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(d)
}
