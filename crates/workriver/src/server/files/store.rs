// The attachment store: `<data dir>/attachments/<id>.<ext>`, one file per content (the id is
// a short hash of it), never deleted. Agents stage files per task in `attachments/inbox/`.
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// A stored file's path, from its id and its original name (for the extension); none for an
/// id that is not the store's own form (16 lowercase hex), which could name a file elsewhere
/// (the rules record no other, but a path is never built from one).
pub(in crate::server) fn path(home: &Path, id: &str, name: &str) -> Option<PathBuf> {
    Some(crate::common::home::attachments_dir(home).join(file_name(id, name)?))
}

/// A stored file's name, in the store and in a move's target: `<id>.<ext>`, the extension
/// from its recorded name when it is alphanumeric; none for an id not of the store's form.
/// Never the recorded name itself, so nothing is written outside the folder.
pub(in crate::server) fn file_name(id: &str, name: &str) -> Option<String> {
    if !workriver_core::attachment_id(id) {
        return None;
    }
    let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).filter(|e| e.chars().all(|c| c.is_ascii_alphanumeric()));
    Some(match ext {
        Some(e) => format!("{id}.{e}"),
        None => id.to_string(),
    })
}

/// Stores `bytes` (once per content) and returns its id and path. Safe for the same content
/// written by several at once (two uploads of one file): each write has a temporary file of
/// its own (tempfile: private, a random name), renamed into place whole; whichever lands
/// last replaces the same bytes.
pub(super) fn put(home: &Path, bytes: &[u8], name: &str) -> std::io::Result<(String, PathBuf)> {
    let id: String = Sha256::digest(bytes).iter().take(8).map(|b| format!("{b:02x}")).collect();
    let target = path(home, &id, name).expect("an id made here is of the store's form");
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
pub(in crate::server) fn usage(home: &Path) -> (u64, u64) {
    let files = std::fs::read_dir(crate::common::home::attachments_dir(home)).into_iter().flatten().flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file());
    files.fold((0, 0), |(n, b), m| (n + 1, b + m.len()))
}
