// File changes with their texts and line counts, and the uncommitted work in a work tree.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::*;

mod tree;

pub use tree::*;

/// ponytail: a text file version larger than this is left out of the diff; stream or page when needed.
const MAX_TEXT: usize = 512 * 1024;
/// How far git looks for a NUL to call a file binary (its `FIRST_FEW_BYTES`).
const FIRST_FEW: usize = 8000;

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    /// "A", "M" or "D".
    pub status: String,
    /// Lines added and removed, worked out once from the texts: what lists show without them.
    pub add: u32,
    pub del: u32,
    /// The texts; left out where only the change's size is wanted (`FileChange::bare`). None
    /// also for a side that does not exist, a submodule's pointer (no text: the pointer is its
    /// object id) and both sides of a binary change.
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
    /// Either side is binary, as git takes it by default (a NUL in its first 8000 bytes): git
    /// says "Binary files differ", so neither text is there and both counts are 0.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub binary: bool,
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
    let changed = raw(&String::from_utf8_lossy(&out));
    // What was committed, for every changed file in one call; what is there now, from disk.
    let specs: Vec<String> = changed.iter().filter(|r| r.texted(0)).map(|r| format!("HEAD:{}", r.path)).collect();
    let mut before = blobs(at, &dir, &specs)?.into_iter();
    let mut files = vec![];
    for mut r in changed {
        let before = if r.texted(0) { Some(before.next().flatten().unwrap_or_default()) } else { None };
        // A symlink's text is where it points (as git stores it), not the file there.
        let after = if !r.texted(1) { None } else if r.modes[1] == LINK { at.read_link(&dir.join(&r.path)) } else { at.read(&dir.join(&r.path)) };
        // A submodule's pointer, which `--raw` leaves unnamed here: the commit its checkout is
        // at (`git diff` shows that one), when it is checked out (the top of its own work tree).
        if r.modes[1] == GITLINK && r.ids[1] == NULL {
            let out = git(at, &dir.join(&r.path), &["rev-parse", "--show-prefix", "HEAD"])?;
            if let [prefix, head] = out.lines().collect::<Vec<_>>()[..]
                && prefix.is_empty()
            {
                r.ids[1] = head.to_string();
            }
        }
        // Only its checkout's own work changed (git says "-dirty"): the pointer did not.
        if r.modes == [GITLINK, GITLINK] && r.ids[0] == r.ids[1] {
            continue;
        }
        if let Some(c) = change(r, before, after) {
            files.push(c);
        }
    }
    // git names no mode for an untracked file: as `git add` would take it, a symlink (120000,
    // its text where it points), else a regular file.
    // ponytail: an executable one is taken as 100644 too.
    let untracked = run(at, &dir, &["ls-files", "--others", "--exclude-standard", "-z"], &[])?;
    for path in split0(&untracked) {
        let (link, after) = at.read_new(&dir.join(&path)).map_or((false, None), |(l, b)| (l, Some(b)));
        let r = Raw { status: "A".into(), path, modes: [GONE.into(), if link { LINK } else { "100644" }.into()], ids: [NULL.into(), NULL.into()] };
        if let Some(c) = change(r, None, after) {
            files.push(c);
        }
    }
    // A binary file's side git has not hashed has no text to stand for it: the id `git add`
    // would give it, all in one call.
    let unhashed: Vec<usize> = (0..files.len()).filter(|&k| files[k].binary && files[k].after_mode != GONE && files[k].after_mode != LINK && files[k].after_id == NULL).collect();
    if !unhashed.is_empty() {
        let paths: Vec<String> = unhashed.iter().map(|&k| files[k].path.clone()).collect();
        let args: Vec<&str> = ["hash-object", "--"].into_iter().chain(paths.iter().map(String::as_str)).collect();
        let out = git(at, &dir, &args)?;
        for (&k, id) in unhashed.iter().zip(out.lines()) {
            files[k].after_id = id.to_string();
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

/// A file change with its sides' bytes (None where a side has no text: it does not exist, or
/// is a submodule's pointer). Binary when either side is (git's default: a NUL in its first
/// 8000 bytes): there, without texts or line counts. None for a very large text file.
// ponytail: very large text files are dropped from the diff, not shown as such.
pub(crate) fn change(r: Raw, before: Option<Vec<u8>>, after: Option<Vec<u8>>) -> Option<FileChange> {
    let binary = [&before, &after].into_iter().flatten().any(|b| b[..b.len().min(FIRST_FEW)].contains(&0));
    let text = |b: Option<Vec<u8>>| -> Option<Option<String>> {
        match b {
            _ if binary => Some(None),
            None => Some(None),
            Some(b) if b.len() > MAX_TEXT => None,
            Some(b) => Some(Some(String::from_utf8_lossy(&b).into_owned())),
        }
    };
    let (before, after) = (text(before)?, text(after)?);
    let (add, del) = counts(before.as_deref().unwrap_or_default(), after.as_deref().unwrap_or_default());
    let Raw { status, path, modes: [before_mode, after_mode], ids: [before_id, after_id] } = r;
    Some(FileChange { add, del, before, after, path, status, before_mode, before_id, after_mode, after_id, binary })
}

pub(crate) fn split0(out: &[u8]) -> impl Iterator<Item = String> + '_ {
    out.split(|b| *b == 0).filter(|s| !s.is_empty()).map(|s| String::from_utf8_lossy(s).into_owned())
}

/// The mode of a side that does not exist, and its object id (git's, in `--raw`).
pub(crate) const GONE: &str = "000000";
pub(crate) const NULL: &str = "0000000000000000000000000000000000000000";
/// A symlink's mode, and a submodule pointer's (a gitlink: its id is a commit, not a text).
const LINK: &str = "120000";
pub(crate) const GITLINK: &str = "160000";

/// One file of `--raw` output: its status ("A", "M" or "D"; a type change and the like show
/// as modified), path, and each side's mode and object id.
pub(crate) struct Raw {
    pub status: String,
    pub path: String,
    pub modes: [String; 2],
    pub ids: [String; 2],
}

impl Raw {
    /// Whether side `k` (0 before, 1 after) has a text: it exists and is no submodule pointer.
    pub fn texted(&self, k: usize) -> bool {
        self.modes[k] != GONE && self.modes[k] != GITLINK
    }
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
