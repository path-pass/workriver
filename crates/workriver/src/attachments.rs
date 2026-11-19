// The attachment store: `<data dir>/attachments/<id>.<ext>`, one file per content (the id is
// a short hash of it), never deleted. Agents stage files per task in `attachments/inbox/`.
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// A stored file's path, from its id and its original name (for the extension).
pub fn path(home: &Path, id: &str, name: &str) -> PathBuf {
    let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).filter(|e| e.chars().all(|c| c.is_ascii_alphanumeric()));
    crate::common::home::attachments_dir(home).join(match ext {
        Some(e) => format!("{id}.{e}"),
        None => id.to_string(),
    })
}

/// Stores `bytes` (once per content) and returns its id and path. Safe for the same content
/// written by several at once (two uploads of one file): each write has a temporary file of
/// its own (tempfile: private, a random name), renamed into place whole; whichever lands
/// last replaces the same bytes.
pub fn put(home: &Path, bytes: &[u8], name: &str) -> std::io::Result<(String, PathBuf)> {
    let id: String = Sha256::digest(bytes).iter().take(8).map(|b| format!("{b:02x}")).collect();
    let target = path(home, &id, name);
    if !target.exists() {
        crate::common::home::mkdir(&crate::common::home::attachments_dir(home))?;
        let mut f = tempfile::NamedTempFile::new_in(crate::common::home::attachments_dir(home))?;
        f.write_all(bytes)?;
        f.as_file().sync_all()?;
        f.persist(&target).map_err(|e| e.error)?;
    }
    Ok((id, target))
}

/// How many files the store holds and their size, for `workriver status`.
pub fn usage(home: &Path) -> (u64, u64) {
    let files = std::fs::read_dir(crate::common::home::attachments_dir(home)).into_iter().flatten().flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file());
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
        assert_eq!(std::fs::metadata(crate::common::home::attachments_dir(home.path())).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(usage(home.path()), (1, 9));
        assert_eq!(crate::common::files::mime("a.png", None), "image/png");
        assert_eq!(crate::common::files::mime("report", Some("text/plain")), "text/plain");
    }

    /// The same content written by several at once (two uploads of one file): every write
    /// succeeds, one file is kept, whole, and no temporary file is left.
    #[test]
    fn the_same_content_at_once_is_stored_once() {
        let home = tempfile::tempdir().unwrap();
        let bytes = vec![7u8; 4 * 1024 * 1024];
        for round in 0..5 {
            let bytes: Vec<u8> = bytes.iter().map(|b| b ^ round).collect();
            let start = std::sync::Barrier::new(8);
            let got: Vec<_> = std::thread::scope(|s| {
                let hs: Vec<_> = (0..8).map(|_| s.spawn(|| { start.wait(); put(home.path(), &bytes, "big.bin") })).collect();
                hs.into_iter().map(|h| h.join().unwrap()).collect()
            });
            let first = got[0].as_ref().unwrap().clone();
            assert!(got.iter().all(|r| r.as_ref().ok() == Some(&first)), "{got:?}");
            assert_eq!(std::fs::read(&first.1).unwrap(), bytes);
            assert_eq!(std::fs::metadata(&first.1).unwrap().permissions().mode() & 0o777, 0o600);
        }
        assert_eq!(usage(home.path()), (5, 5 * 4 * 1024 * 1024), "nothing but the stored files");
    }
}
