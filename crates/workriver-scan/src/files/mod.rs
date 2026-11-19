// File changes with their texts and line counts, and the uncommitted work in a work tree.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::*;

mod tree;

pub use tree::*;

/// ponytail: a file version larger than this is left out of the diff; stream or page when needed.
const MAX_TEXT: usize = 512 * 1024;

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct FileChange {
    pub path: String,
    /// "A", "M" or "D".
    pub status: String,
    /// Lines added and removed, worked out once from the texts: what lists show without them.
    pub add: u32,
    pub del: u32,
    /// The texts; left out where only the change's size is wanted (`FileChange::bare`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

impl FileChange {
    /// Without its texts.
    pub fn bare(&self) -> FileChange {
        FileChange { path: self.path.clone(), status: self.status.clone(), add: self.add, del: self.del, before: None, after: None }
    }
}

/// Lines added and removed from `before` to `after`, as a shortest line diff counts them (the
/// web UI's `diff` package does too; similar's default Myers takes shortcuts that can count
/// more). A pathological pair gives up after a second, approximate.
pub fn counts(before: &str, after: &str) -> (u32, u32) {
    struct Count(u32, u32);
    impl similar::algorithms::DiffHook for Count {
        type Error = std::convert::Infallible;
        fn delete(&mut self, _: usize, n: usize, _: usize) -> std::result::Result<(), Self::Error> {
            self.1 += n as u32;
            Ok(())
        }
        fn insert(&mut self, _: usize, _: usize, n: usize) -> std::result::Result<(), Self::Error> {
            self.0 += n as u32;
            Ok(())
        }
    }
    let (old, new): (Vec<&str>, Vec<&str>) = (before.split_inclusive('\n').collect(), after.split_inclusive('\n').collect());
    let mut c = Count(0, 0);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    let _ = similar::algorithms::myers::diff_deadline_raw(&mut c, &old, 0..old.len(), &new, 0..new.len(), Some(deadline));
    (c.0, c.1)
}

/// Uncommitted changes in the work tree where `branch` is checked out, against its last
/// commit; untracked files count as added. Empty when the branch is checked out nowhere.
pub fn live(at: &Runner, repo: &Path, branch: &str) -> Result<Vec<FileChange>> {
    let Some(dir) = worktree_of(at, repo, branch)? else { return Ok(vec![]) };
    live_in(at, &dir)
}

/// Every work tree of a repository, by the branch checked out there: one call.
pub fn worktrees(at: &Runner, repo: &Path) -> Result<HashMap<String, PathBuf>> {
    let list = git(at, repo, &["worktree", "list", "--porcelain"])?;
    Ok(list
        .split("\n\n")
        .filter_map(|b| {
            let dir = b.lines().next()?.strip_prefix("worktree ")?;
            let branch = b.lines().find_map(|l| l.strip_prefix("branch refs/heads/"))?;
            Some((branch.to_string(), PathBuf::from(dir)))
        })
        .collect())
}

/// Uncommitted changes in the work tree `dir`, against its last commit.
pub fn live_in(at: &Runner, dir: &Path) -> Result<Vec<FileChange>> {
    let dir = dir.to_path_buf();
    let mut changed = name_status(&run(at, &dir, &["diff", "HEAD", "--no-renames", "-z", "--name-status"], &[])?);
    let untracked = run(at, &dir, &["ls-files", "--others", "--exclude-standard", "-z"], &[])?;
    changed.extend(split0(&untracked).map(|p| ("A".to_string(), p)));
    // What was committed, for every changed file in one call; what is there now, from disk.
    let specs: Vec<String> = changed.iter().filter(|(s, _)| s != "A").map(|(_, p)| format!("HEAD:{p}")).collect();
    let mut before = blobs(at, &dir, &specs)?.into_iter();
    let mut files = vec![];
    for (status, path) in changed {
        let before = if status == "A" { None } else { Some(before.next().flatten().unwrap_or_default()) };
        let after = if status == "D" { None } else { at.read(&dir.join(&path)) };
        if let Some(c) = change(path, status, before, after) {
            files.push(c);
        }
    }
    Ok(files)
}

/// The work tree (main or linked) that has `branch` checked out.
pub fn worktree_of(at: &Runner, repo: &Path, branch: &str) -> Result<Option<PathBuf>> {
    let want = format!("branch refs/heads/{branch}");
    let list = git(at, repo, &["worktree", "list", "--porcelain"])?;
    Ok(list.split("\n\n").find(|b| b.lines().any(|l| l == want)).and_then(|b| b.lines().next()?.strip_prefix("worktree ").map(PathBuf::from)))
}

pub(crate) fn blob(at: &Runner, dir: &Path, rev: &str, path: &str) -> Result<Vec<u8>> {
    run(at, dir, &["cat-file", "blob", &format!("{rev}:{path}")], &[])
}

/// A file change with its texts; None for binary or very large files.
// ponytail: binary and oversized files are dropped from the diff, not shown as such.
pub(crate) fn change(path: String, status: String, before: Option<Vec<u8>>, after: Option<Vec<u8>>) -> Option<FileChange> {
    let text = |b: Option<Vec<u8>>| -> Option<Option<String>> {
        match b {
            None => Some(None),
            Some(b) if b.len() > MAX_TEXT || b.contains(&0) => None,
            Some(b) => Some(Some(String::from_utf8_lossy(&b).into_owned())),
        }
    };
    let (before, after) = (text(before)?, text(after)?);
    let (add, del) = counts(before.as_deref().unwrap_or_default(), after.as_deref().unwrap_or_default());
    Some(FileChange { add, del, before, after, path, status })
}

pub(crate) fn split0(out: &[u8]) -> impl Iterator<Item = String> + '_ {
    out.split(|b| *b == 0).filter(|s| !s.is_empty()).map(|s| String::from_utf8_lossy(s).into_owned())
}

/// `-z --name-status` output: status and path, each ended by a NUL.
pub(crate) fn name_status(out: &[u8]) -> Vec<(String, String)> {
    let parts: Vec<String> = split0(out).collect();
    // A type change (T) and the like show as modified.
    let status = |s: &str| if s == "A" || s == "D" { s.to_string() } else { "M".to_string() };
    parts.chunks(2).filter(|c| c.len() == 2).map(|c| (status(&c[0]), c[1].clone())).collect()
}
