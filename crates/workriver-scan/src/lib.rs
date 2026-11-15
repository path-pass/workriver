//! What workriver reads from git: a unit's commits with each file's text before and after,
//! the uncommitted work on a branch, and a repository's file tree. Plain `git` processes.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Git(String),
    #[error("cannot run git: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// ponytail: a file version larger than this is left out of the diff; stream or page when needed.
const MAX_TEXT: usize = 512 * 1024;
/// git's empty tree, the "before" of a commit without a parent.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct FileChange {
    pub path: String,
    /// "A", "M" or "D".
    pub status: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Commit {
    /// Short hash.
    pub id: String,
    pub hash: String,
    pub subject: String,
    /// ISO 8601 with offset.
    pub date: String,
    pub author: String,
    /// The value of a `Task:` trailer, e.g. "u1#3".
    pub task: Option<String>,
    pub files: Vec<FileChange>,
}

/// A folder or file of the tree, as the UI's `ModuleNode`.
#[derive(Serialize, Clone, PartialEq, Debug, Default)]
pub struct Module {
    pub name: String,
    pub path: String,
    pub loc: u64,
    pub files: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Module>>,
}

/// Runs git in `dir`; exit codes in `ok` besides 0 count as success. Optional locks are off
/// so reading never rewrites the index, which the watcher would take for a change.
fn run(dir: &Path, args: &[&str], ok: &[i32]) -> Result<Vec<u8>> {
    let out = Command::new("git").arg("--no-optional-locks").arg("-C").arg(dir).args(args).output()?;
    let code = out.status.code().unwrap_or(-1);
    if code != 0 && !ok.contains(&code) {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(Error::Git(format!("git {} in {}: {}", args.join(" "), dir.display(), err.trim())));
    }
    Ok(out.stdout)
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8_lossy(&run(dir, args, &[])?).into_owned())
}

/// The top folder of the work tree `path` is in; an error if it is not in one.
pub fn toplevel(path: &Path) -> Result<PathBuf> {
    Ok(PathBuf::from(git(path, &["rev-parse", "--show-toplevel"])?.trim()))
}

/// The commit HEAD points at.
pub fn head(repo: &Path) -> Result<String> {
    Ok(git(repo, &["rev-parse", "HEAD"])?.trim().to_string())
}

/// The git folder shared by all work trees of the repository, where refs live.
pub fn common_dir(repo: &Path) -> Result<PathBuf> {
    Ok(PathBuf::from(git(repo, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?.trim()))
}

/// Whether `rev` names a commit in `repo`.
pub fn has_rev(repo: &Path, rev: &str) -> bool {
    run(repo, &["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")], &[]).is_ok()
}

/// Full hashes of the commits on `branch` since `base`, oldest first; none if the branch is not there yet.
pub fn commit_list(repo: &Path, base: &str, branch: &str) -> Result<Vec<String>> {
    if !has_rev(repo, branch) {
        return Ok(vec![]);
    }
    let range = format!("{base}..{branch}");
    Ok(git(repo, &["rev-list", "--reverse", "--first-parent", &range, "--"])?.lines().map(str::to_string).collect())
}

/// One commit with its files' text before and after. Binary and very large files are left out.
pub fn commit(repo: &Path, hash: &str) -> Result<Commit> {
    let fmt = "--format=%h%x00%an%x00%aI%x00%P%x00%(trailers:key=Task,valueonly,separator=%x2C)%x00%s";
    let out = git(repo, &["log", "-1", "--abbrev=9", fmt, hash, "--"])?;
    let f: Vec<&str> = out.trim_end_matches('\n').splitn(6, '\0').collect();
    let [id, author, date, parents, task, subject] = f[..] else {
        return Err(Error::Git(format!("unexpected git log output for {hash}")));
    };
    let parent = parents.split(' ').next().filter(|p| !p.is_empty()).unwrap_or(EMPTY_TREE);
    let task = task.split(',').next().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
    let mut files = vec![];
    for (status, path) in name_status(&run(repo, &["diff-tree", "-r", "--no-renames", "-z", "--name-status", parent, hash], &[])?) {
        let before = if status == "A" { None } else { Some(blob(repo, parent, &path)?) };
        let after = if status == "D" { None } else { Some(blob(repo, hash, &path)?) };
        if let Some(c) = change(path, status, before, after) {
            files.push(c);
        }
    }
    Ok(Commit { id: id.into(), hash: hash.into(), subject: subject.into(), date: date.into(), author: author.into(), task, files })
}

/// Uncommitted changes in the work tree where `branch` is checked out, against its last
/// commit; untracked files count as added. Empty when the branch is checked out nowhere.
pub fn live(repo: &Path, branch: &str) -> Result<Vec<FileChange>> {
    let Some(dir) = worktree_of(repo, branch)? else { return Ok(vec![]) };
    let mut changed = name_status(&run(&dir, &["diff", "HEAD", "--no-renames", "-z", "--name-status"], &[])?);
    let untracked = run(&dir, &["ls-files", "--others", "--exclude-standard", "-z"], &[])?;
    changed.extend(split0(&untracked).map(|p| ("A".to_string(), p)));
    let mut files = vec![];
    for (status, path) in changed {
        let before = if status == "A" { None } else { Some(blob(&dir, "HEAD", &path)?) };
        let after = if status == "D" { None } else { std::fs::read(dir.join(&path)).ok() };
        if let Some(c) = change(path, status, before, after) {
            files.push(c);
        }
    }
    Ok(files)
}

/// The work tree (main or linked) that has `branch` checked out.
pub fn worktree_of(repo: &Path, branch: &str) -> Result<Option<PathBuf>> {
    let want = format!("branch refs/heads/{branch}");
    let list = git(repo, &["worktree", "list", "--porcelain"])?;
    Ok(list.split("\n\n").find(|b| b.lines().any(|l| l == want)).and_then(|b| b.lines().next()?.strip_prefix("worktree ").map(PathBuf::from)))
}

/// The repository's files at HEAD as a tree of folders, with line counts.
pub fn tree(repo: &Path) -> Result<Module> {
    let top = toplevel(repo)?;
    let name = top.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut root = Module { name, children: Some(vec![]), ..Default::default() };
    if !has_rev(&top, "HEAD") {
        return Ok(root);
    }
    // `git grep -c ''` counts lines of every text file at HEAD in one go: "HEAD:path\0count".
    let mut loc = HashMap::new();
    for line in String::from_utf8_lossy(&run(&top, &["grep", "-I", "-z", "-c", "", "HEAD", "--"], &[1])?).lines() {
        if let Some((path, n)) = line.split_once('\0') {
            loc.insert(path.trim_start_matches("HEAD:").to_string(), n.parse().unwrap_or(0));
        }
    }
    for path in split0(&run(&top, &["ls-tree", "-r", "--name-only", "-z", "HEAD"], &[])?) {
        let n = loc.get(&path).copied().unwrap_or(0);
        insert(&mut root, &path, n);
    }
    Ok(root)
}

fn insert(root: &mut Module, path: &str, loc: u64) {
    let mut node = root;
    let mut at = 0;
    for part in path.split('/') {
        node.loc += loc;
        node.files += 1;
        at += part.len() + 1;
        let here = &path[..at - 1];
        let kids = node.children.get_or_insert_with(Vec::new);
        let k = match kids.iter().position(|c| c.name == part) {
            Some(k) => k,
            None => {
                let folder = here.len() < path.len();
                kids.push(Module { name: part.into(), path: here.into(), children: folder.then(Vec::new), ..Default::default() });
                kids.len() - 1
            }
        };
        node = &mut kids[k];
    }
    node.loc += loc;
    node.files += 1;
}

fn blob(dir: &Path, rev: &str, path: &str) -> Result<Vec<u8>> {
    run(dir, &["cat-file", "blob", &format!("{rev}:{path}")], &[])
}

/// A file change with its texts; None for binary or very large files.
// ponytail: binary and oversized files are dropped from the diff, not shown as such.
fn change(path: String, status: String, before: Option<Vec<u8>>, after: Option<Vec<u8>>) -> Option<FileChange> {
    let text = |b: Option<Vec<u8>>| -> Option<Option<String>> {
        match b {
            None => Some(None),
            Some(b) if b.len() > MAX_TEXT || b.contains(&0) => None,
            Some(b) => Some(Some(String::from_utf8_lossy(&b).into_owned())),
        }
    };
    Some(FileChange { before: text(before)?, after: text(after)?, path, status })
}

fn split0(out: &[u8]) -> impl Iterator<Item = String> + '_ {
    out.split(|b| *b == 0).filter(|s| !s.is_empty()).map(|s| String::from_utf8_lossy(s).into_owned())
}

/// `-z --name-status` output: status and path, each ended by a NUL.
fn name_status(out: &[u8]) -> Vec<(String, String)> {
    let parts: Vec<String> = split0(out).collect();
    // A type change (T) and the like show as modified.
    let status = |s: &str| if s == "A" || s == "D" { s.to_string() } else { "M".to_string() };
    parts.chunks(2).filter(|c| c.len() == 2).map(|c| (status(&c[0]), c[1].clone())).collect()
}

#[cfg(test)]
mod tests;
