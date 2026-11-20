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
#[serde(rename_all = "camelCase")]
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
    /// Each side's git entry, as `--raw` names it: its mode ("100644"; "000000" where the side
    /// does not exist) and object id (zeros where it does not exist, and for the work tree's
    /// side of uncommitted work, which git has not hashed). Whether a file changed over several
    /// commits is whether these differ at the ends, not the texts.
    pub before_mode: String,
    pub before_id: String,
    pub after_mode: String,
    pub after_id: String,
}

impl FileChange {
    /// Without its texts.
    pub fn bare(&self) -> FileChange {
        FileChange { before: None, after: None, ..self.clone() }
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
    let out = run(at, &dir, &["diff", "HEAD", "--no-renames", "-z", "--raw", "--no-abbrev"], &[])?;
    let mut changed = raw(&String::from_utf8_lossy(&out));
    let untracked = run(at, &dir, &["ls-files", "--others", "--exclude-standard", "-z"], &[])?;
    // ponytail: git names no mode for an untracked file; taken as a regular one (with its
    // text, it only decides whether the file is the same as in the base).
    changed.extend(split0(&untracked).map(|path| Raw { status: "A".into(), path, modes: [GONE.into(), "100644".into()], ids: [NULL.into(), NULL.into()] }));
    // What was committed, for every changed file in one call; what is there now, from disk.
    let specs: Vec<String> = changed.iter().filter(|r| r.status != "A").map(|r| format!("HEAD:{}", r.path)).collect();
    let mut before = blobs(at, &dir, &specs)?.into_iter();
    let mut files = vec![];
    for r in changed {
        let before = if r.status == "A" { None } else { Some(before.next().flatten().unwrap_or_default()) };
        // A symlink's text is where it points (as git stores it), not the file there.
        let after = if r.status == "D" { None } else if r.modes[1] == LINK { at.read_link(&dir.join(&r.path)) } else { at.read(&dir.join(&r.path)) };
        if let Some(c) = change(r, before, after) {
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

/// A file change with its texts; None for binary or very large files.
// ponytail: binary and oversized files are dropped from the diff, not shown as such.
pub(crate) fn change(r: Raw, before: Option<Vec<u8>>, after: Option<Vec<u8>>) -> Option<FileChange> {
    let text = |b: Option<Vec<u8>>| -> Option<Option<String>> {
        match b {
            None => Some(None),
            Some(b) if b.len() > MAX_TEXT || b.contains(&0) => None,
            Some(b) => Some(Some(String::from_utf8_lossy(&b).into_owned())),
        }
    };
    let (before, after) = (text(before)?, text(after)?);
    let (add, del) = counts(before.as_deref().unwrap_or_default(), after.as_deref().unwrap_or_default());
    let Raw { status, path, modes: [before_mode, after_mode], ids: [before_id, after_id] } = r;
    Some(FileChange { add, del, before, after, path, status, before_mode, before_id, after_mode, after_id })
}

pub(crate) fn split0(out: &[u8]) -> impl Iterator<Item = String> + '_ {
    out.split(|b| *b == 0).filter(|s| !s.is_empty()).map(|s| String::from_utf8_lossy(s).into_owned())
}

/// The mode of a side that does not exist, and its object id (git's, in `--raw`).
pub(crate) const GONE: &str = "000000";
pub(crate) const NULL: &str = "0000000000000000000000000000000000000000";
/// A symlink's mode.
const LINK: &str = "120000";

/// One file of `--raw` output: its status ("A", "M" or "D"; a type change and the like show
/// as modified), path, and each side's mode and object id.
pub(crate) struct Raw {
    pub status: String,
    pub path: String,
    pub modes: [String; 2],
    pub ids: [String; 2],
}

/// `-z --raw` output: `:mode mode id id status` and the path, each ended by a NUL (a line
/// break before the first, in `git log`'s).
pub(crate) fn raw(out: &str) -> Vec<Raw> {
    let mut files = vec![];
    let mut tokens = out.split('\0').map(|t| t.trim_start_matches('\n'));
    while let Some(t) = tokens.next() {
        let Some(meta) = t.strip_prefix(':') else { continue };
        let Some(path) = tokens.next() else { break };
        if let [before_mode, after_mode, before_id, after_id, status, ..] = meta.split(' ').collect::<Vec<_>>()[..] {
            let status = if status.starts_with('A') || status.starts_with('D') { status[..1].to_string() } else { "M".into() };
            files.push(Raw { status, path: path.into(), modes: [before_mode.into(), after_mode.into()], ids: [before_id.into(), after_id.into()] });
        }
    }
    files
}
