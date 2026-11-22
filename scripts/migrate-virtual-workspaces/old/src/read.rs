// The old events, read from a copy of a store (opened read-only) or, for the tests, from a
// file of `{seq, at, actor, kind, body}` lines; each decoded by the old code.
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use workriver_core::Event;

/// One stored event: as the old code reads it, and its body as stored.
pub struct Old {
    pub event: Event,
    pub kind: String,
    pub body: String,
}

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

/// Refuses a path in a live data home: the script works on a copy only.
pub fn not_live(path: &Path) -> Result<(), String> {
    let at = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => match path.parent().map(|p| if p.as_os_str().is_empty() { Path::new(".") } else { p }).and_then(|p| p.canonicalize().ok()) {
            Some(p) => p.join(path.file_name().unwrap_or_default()),
            None => return Err(format!("{}: its folder does not exist", path.display())),
        },
    };
    match live_homes().into_iter().find(|h| at.starts_with(h)) {
        Some(h) => Err(format!("{} is in workriver's data home {}: work on a copy taken with the service stopped, never on the live store", path.display(), h.display())),
        None => Ok(()),
    }
}

/// A new file or folder: refused when it exists, or when it is in a live data home.
pub fn new_path(path: &Path) -> Result<(), String> {
    not_live(path)?;
    if path.exists() {
        return Err(format!("{} exists: every output is a new file or folder", path.display()));
    }
    Ok(())
}

pub fn events(path: &Path) -> Result<Vec<Old>, String> {
    not_live(path)?;
    let rows = if path.extension().is_some_and(|e| e == "jsonl") { from_lines(path)? } else { from_store(path)? };
    rows.into_iter()
        .map(|(seq, at, actor, kind, body)| {
            let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| format!("event {seq}: {e}"))?;
            let event = serde_json::from_value(serde_json::json!({ "seq": seq, "at": at, "actor": actor, "kind": kind, "body": v })).map_err(|e| format!("event {seq} ({kind}) does not decode in the old form: {e}"))?;
            Ok(Old { event, kind, body })
        })
        .collect()
}

type Row = (i64, String, String, String, String);

fn from_store(path: &Path) -> Result<Vec<Row>, String> {
    let e = |e: rusqlite::Error| format!("{}: {e}", path.display());
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(e)?;
    let marked: bool = conn.query_row("SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 'format')", [], |r| r.get(0)).map_err(e)?;
    if marked {
        return Err(format!("{} is in the new form already (it has the format mark): nothing to migrate", path.display()));
    }
    let mut stmt = conn.prepare("SELECT seq, at, actor, kind, body FROM events ORDER BY seq").map_err(e)?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))).map_err(e)?;
    rows.collect::<Result<_, _>>().map_err(e)
}

fn from_lines(path: &Path) -> Result<Vec<Row>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = vec![];
    // The body as written in the line, as a store keeps it.
    #[derive(serde::Deserialize)]
    struct Line<'a> {
        seq: Option<i64>,
        at: String,
        actor: String,
        kind: String,
        #[serde(borrow)]
        body: &'a serde_json::value::RawValue,
    }
    for (k, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let l: Line = serde_json::from_str(line).map_err(|e| format!("{} line {}: {e}", path.display(), k + 1))?;
        out.push((l.seq.unwrap_or(k as i64 + 1), l.at, l.actor, l.kind, l.body.get().to_string()));
    }
    Ok(out)
}
