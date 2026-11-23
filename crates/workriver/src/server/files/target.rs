// Where the attachment rule moves files: a target is a value of the settings, `{ "kind":
// …, … }`, recorded as it was with every move. These three functions are the whole of what a
// target is; each is a match on the kind, and a kind not known here (a record of a later
// version, a file edited by hand) is refused in words, never guessed. The first and only
// kind is a local folder (one a sync client such as OneDrive keeps is a local folder too:
// its credentials are the client's). Nothing of a folder's checks is anywhere else.
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use workriver_core::{MAX_ATTACHMENT, Target};

use crate::server::files::store;

fn unknown(t: &Target) -> String {
    format!("no target of kind {:?}", t.kind)
}

/// A target as words show it: a folder's path, else its kind.
pub(in crate::server) fn shown(t: &Target) -> String {
    match t.kind.as_str() {
        "folder" => path_of(t).map_or_else(|e| e, |p| p.display().to_string()),
        kind => format!("a target of kind {kind}"),
    }
}

/// A folder target's path, as given.
fn path_of(t: &Target) -> Result<PathBuf, String> {
    t.rest.get("path").and_then(|p| p.as_str()).filter(|p| !p.is_empty()).map(PathBuf::from).ok_or_else(|| "a folder target needs its path".to_string())
}

/// Whether `bytes` are the content of attachment `id`: the store's own id of them.
fn matches(id: &str, bytes: &[u8]) -> bool {
    store::id(bytes) == id
}

/// Whether folder `dir` (canonical) is workriver's own, or inside it: the data directory,
/// the store's folder and its inbox, each as it is once its links are followed (either may
/// be a link to another disk). Compared by what the folders are (device and inode), not by
/// how their paths are spelled: one folder can have several names that `canonicalize`
/// keeps apart (on macOS the data volume's firmlink, `/System/Volumes/Data/…`; a bind mount
/// elsewhere). A store folder or inbox not made yet is nowhere to be inside.
fn inside(dir: &Path, home: &Path) -> Result<bool, String> {
    use std::os::unix::fs::MetadataExt;
    let store = crate::common::home::attachments_dir(home);
    let mut own = vec![];
    for (folder, must) in [(home.to_path_buf(), true), (store.join(crate::common::home::INBOX), false), (store, false)] {
        match std::fs::metadata(&folder) {
            Ok(m) => own.push((m.dev(), m.ino())),
            Err(e) if !must && e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{} cannot be read: {e}", folder.display())),
        }
    }
    for d in dir.ancestors() {
        let m = std::fs::metadata(d).map_err(|e| format!("the folder {} cannot be read: {e}", d.display()))?;
        if own.contains(&(m.dev(), m.ino())) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Everything that makes `t` a usable target, and the value as it is saved and recorded.
/// A folder: an absolute path to a folder that exists (never made here: a folder that is
/// missing is a drive that is not mounted), not the data directory nor inside it by what the
/// folders are, whatever the path's spelling (`inside`), and writable (a probe file of a random name is made and removed); the canonical
/// path is given back. Saving the rule calls it, and every run, which goes on only if it
/// gives back the saved value.
pub(in crate::server) fn check(t: &Target, home: &Path) -> Result<Target, String> {
    match t.kind.as_str() {
        "folder" => {
            let path = path_of(t)?;
            if !path.is_absolute() {
                return Err(format!("the folder {} is not an absolute path", path.display()));
            }
            let meta = std::fs::metadata(&path).map_err(|_| format!("the folder {} is not there", path.display()))?;
            if !meta.is_dir() {
                return Err(format!("{} is not a folder", path.display()));
            }
            let canonical = path.canonicalize().map_err(|e| format!("the folder {} cannot be read: {e}", path.display()))?;
            if inside(&canonical, home)? {
                return Err(format!("the folder {} is inside workriver's data directory; pick one outside it", canonical.display()));
            }
            let mut probe = tempfile::NamedTempFile::new_in(&canonical).map_err(|e| format!("the folder {} cannot be written: {e}", canonical.display()))?;
            probe.write_all(b"workriver").map_err(|e| format!("the folder {} cannot be written: {e}", canonical.display()))?;
            drop(probe);
            let mut out = t.clone();
            out.rest.insert("path".into(), canonical.to_string_lossy().into_owned().into());
            Ok(out)
        }
        _ => Err(unknown(t)),
    }
}

/// Copies attachment `id` (`bytes`, its recorded `name` giving the extension) into the
/// target, then reads it back and compares it with the id. A folder: through a temporary
/// file of a random name made there with `O_EXCL` (never an existing file or a link put
/// there), synced, renamed over `<id>.<ext>` (which replaces a link found there and never
/// follows it); a copy that does not read back as the id is removed.
pub(in crate::server) fn put(t: &Target, id: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
    match t.kind.as_str() {
        "folder" => {
            let dir = path_of(t)?;
            let file = store::file_name(id, name).ok_or_else(|| format!("{id:?} is not an attachment id"))?;
            let at = dir.join(&file);
            let write = || -> std::io::Result<()> {
                let mut tmp = tempfile::NamedTempFile::new_in(&dir)?;
                tmp.write_all(bytes)?;
                tmp.as_file().sync_all()?;
                tmp.persist(&at).map_err(|e| e.error)?;
                Ok(())
            };
            write().map_err(|e| format!("cannot copy {file} to {}: {e}", dir.display()))?;
            if let Err(e) = get(t, id, name, bytes.len() as u64) {
                let _ = std::fs::remove_file(&at);
                return Err(e);
            }
            Ok(())
        }
        _ => Err(unknown(t)),
    }
}

/// Attachment `id` as the target holds it, only if it is what was recorded: its `size` (at
/// most the store's limit, checked first, so the read is bounded whatever a record says)
/// and its content (the id). A folder: `<id>.<ext>` opened without following a link and
/// without waiting on a FIFO, then on that one handle: a regular file of exactly `size`
/// bytes, read whole, hashed. Anything else is refused with nothing read.
pub(in crate::server) fn get(t: &Target, id: &str, name: &str, size: u64) -> Result<Vec<u8>, String> {
    match t.kind.as_str() {
        "folder" => {
            if size > MAX_ATTACHMENT as u64 {
                return Err(format!("{size} bytes is larger than the store takes"));
            }
            let dir = path_of(t)?;
            let file = store::file_name(id, name).ok_or_else(|| format!("{id:?} is not an attachment id"))?;
            let at = dir.join(&file);
            let mut f: File = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(&at).map_err(|e| format!("cannot open {}: {e}", at.display()))?;
            let meta = f.metadata().map_err(|e| format!("cannot read {}: {e}", at.display()))?;
            if !meta.is_file() {
                return Err(format!("{} is not a regular file", at.display()));
            }
            if meta.len() != size {
                return Err(format!("{} is not the recorded size ({} bytes, not {size})", at.display(), meta.len()));
            }
            let mut bytes = Vec::with_capacity(size as usize);
            (&mut f).take(size).read_to_end(&mut bytes).map_err(|e| format!("cannot read {}: {e}", at.display()))?;
            if bytes.len() as u64 != size || !matches(id, &bytes) {
                return Err(format!("{} does not match the attachment it should be", at.display()));
            }
            Ok(bytes)
        }
        _ => Err(unknown(t)),
    }
}
