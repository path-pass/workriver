// The data directory and the name of every file in it, in one place: whoever reads or writes
// one of them, the CLI or the service, finds it here.
use std::path::PathBuf;

/// Where workriver keeps its data: `$WORKRIVER_HOME`, or `~/workriver-data`.
pub fn data_dir() -> PathBuf {
    match std::env::var_os("WORKRIVER_HOME") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => std::env::home_dir().unwrap_or_default().join("workriver-data"),
    }
}

/// The database: the one SQLite file of events (workriver-data's store opens it).
pub const DATABASE: &str = "workriver.db";

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
