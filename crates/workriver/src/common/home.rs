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
/// What the git scan keeps between starts, in a folder: this machine's repositories, and
/// those of followed machines.
pub const SCAN_CACHE: &str = "scan-cache";
pub const SCAN_CACHE_LOCAL: &str = "local.json";
pub const SCAN_CACHE_REMOTE: &str = "remote.json";
/// Which managers were told of a guide's update.
pub const GUIDE_TOLD: &str = "guide-told.json";
// Not named here: the ssh control sockets of followed machines (`ssh-%C`, ssh expands the
// `%C`), which workriver-scan's `Runner::ssh` makes in the data directory it is given; the
// runner owns its ssh options, this file names only the folder (the data directory itself).
// Nor the socket of each followed machine's runtime, forwarded here: the runtime adapter
// names it (`socket_here`, in its remote.rs), since the name is the runtime's own.
// A file of the data directory written whole goes through a temporary beside it:
// `<name>.<hex>.tmp` (`temporary`); a machine's metrics history, `<label>.jsonl.tmp`.

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

/// The timing log's file `k`: the current one (0: `PERF_LOG`), else an older one moved up
/// (`perf.1.jsonl`, `perf.2.jsonl`…).
pub fn perf_log(k: usize) -> String {
    if k == 0 { PERF_LOG.to_string() } else { format!("{}.{k}.jsonl", PERF_LOG.trim_end_matches(".jsonl")) }
}

/// A timing bundle `workriver perf --bundle` writes beside the log, by its time:
/// `perf-bundle-<stamp>.txt`.
pub fn perf_bundle(stamp: &str) -> String {
    format!("perf-bundle-{stamp}.txt")
}

/// A temporary file to write `name` through, beside it: `<name>.<hex>.tmp`.
pub fn temporary(name: &str, hex: &str) -> String {
    format!("{name}.{hex}.tmp")
}
