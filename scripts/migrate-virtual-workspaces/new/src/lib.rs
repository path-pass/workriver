//! The second half of the one-off migration to virtual workspaces (see ../../README.md):
//!
//!   new write <dir> <out.db>   the new store, from what `old apply` wrote in <dir>
//!   new check <dir> <out.db>   its state against the old state
use std::path::{Path, PathBuf};

pub mod check;
pub mod write;

/// The data homes the service uses: `$WORKRIVER_HOME` and `~/workriver-data`.
fn live_homes() -> Vec<PathBuf> {
    let mut out = vec![];
    if let Some(h) = std::env::var_os("WORKRIVER_HOME") {
        out.push(PathBuf::from(h));
    }
    if let Some(h) = std::env::home_dir() {
        out.push(h.join("workriver-data"));
    }
    out.into_iter().filter_map(|h| h.canonicalize().ok()).collect()
}

/// A new file: refused when it exists or is in a live data home (the live store's path).
pub fn new_path(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!("{} exists: the new store is a new file", path.display()));
    }
    let folder = path.parent().map(|p| if p.as_os_str().is_empty() { Path::new(".") } else { p }).and_then(|p| p.canonicalize().ok()).ok_or_else(|| format!("{}: its folder does not exist", path.display()))?;
    if let Some(h) = live_homes().into_iter().find(|h| folder.starts_with(h)) {
        return Err(format!("{} is in workriver's data home {}: write the new store elsewhere, and put it in place yourself with the service stopped", path.display(), h.display()));
    }
    Ok(())
}
