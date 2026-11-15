// The attachment store: `<data dir>/attachments/<id>.<ext>`, one file per content (the id is
// a short hash of it), never deleted. Agents stage files per task in `attachments/inbox/`.
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// ponytail: the largest file taken; raise it, or stream uploads, if real files need more.
pub const MAX: usize = 50 * 1024 * 1024;

pub fn dir(home: &Path) -> PathBuf {
    home.join("attachments")
}

/// Where agents put a task's screenshots before delivering it.
pub fn inbox(home: &Path, unit: &str, task: u32) -> PathBuf {
    dir(home).join("inbox").join(format!("{unit}-{task}"))
}

/// A stored file's path, from its id and its original name (for the extension).
pub fn path(home: &Path, id: &str, name: &str) -> PathBuf {
    let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).filter(|e| e.chars().all(|c| c.is_ascii_alphanumeric()));
    dir(home).join(match ext {
        Some(e) => format!("{id}.{e}"),
        None => id.to_string(),
    })
}

/// The type to serve a file as: the given one, else guessed from the name.
pub fn mime(name: &str, given: Option<&str>) -> String {
    match given.map(str::trim).filter(|t| !t.is_empty() && *t != "application/octet-stream") {
        Some(t) => t.to_string(),
        None => mime_guess::from_path(name).first_or_octet_stream().to_string(),
    }
}

pub fn mkdir(d: &Path) -> std::io::Result<()> {
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(d)
}

/// Stores `bytes` (once per content) and returns its id and path.
pub fn put(home: &Path, bytes: &[u8], name: &str) -> std::io::Result<(String, PathBuf)> {
    let id: String = Sha256::digest(bytes).iter().take(8).map(|b| format!("{b:02x}")).collect();
    let target = path(home, &id, name);
    if !target.exists() {
        mkdir(&dir(home))?;
        let tmp = target.with_extension("part");
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, &target)?;
    }
    Ok((id, target))
}

/// How many files the store holds and their size, for `workriver status`.
pub fn usage(home: &Path) -> (u64, u64) {
    let files = std::fs::read_dir(dir(home)).into_iter().flatten().flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file());
    files.fold((0, 0), |(n, b), m| (n + 1, b + m.len()))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn stores_once_per_content_privately() {
        let home = tempfile::tempdir().unwrap();
        let (a, pa) = put(home.path(), b"png bytes", "Shot.PNG").unwrap();
        let (b, pb) = put(home.path(), b"png bytes", "other.png").unwrap();
        assert_eq!((a.len(), &a, &pa), (16, &b, &pb), "same content, same file");
        assert_eq!(pa.file_name().unwrap().to_string_lossy(), format!("{a}.png"));
        assert_eq!(std::fs::metadata(&pa).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(dir(home.path())).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(usage(home.path()), (1, 9));
        assert_eq!(mime("a.png", None), "image/png");
        assert_eq!(mime("report", Some("text/plain")), "text/plain");
    }
}
