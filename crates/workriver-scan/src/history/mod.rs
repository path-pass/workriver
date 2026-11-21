// A repository's history as workriver reads it: which repository and commit a path is in,
// its refs, and (branches.rs) what a unit's branch holds; (commits.rs) commits with their
// files' texts.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::*;

mod branches;
mod commits;

pub use branches::*;
pub use commits::*;

/// The top folder of the work tree `path` is in; an error if it is not in one.
pub fn toplevel(at: &Runner, path: &Path) -> Result<PathBuf> {
    Ok(PathBuf::from(git(at, path, &["rev-parse", "--show-toplevel"])?.trim()))
}

/// The commit HEAD points at.
pub fn head(at: &Runner, repo: &Path) -> Result<String> {
    Ok(git(at, repo, &["rev-parse", "HEAD"])?.trim().to_string())
}

/// The git folder shared by all work trees of the repository, where refs live.
pub fn common_dir(at: &Runner, repo: &Path) -> Result<PathBuf> {
    Ok(PathBuf::from(git(at, repo, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?.trim()))
}

/// Whether `rev` names a commit in `repo`.
pub fn has_rev(at: &Runner, repo: &Path, rev: &str) -> bool {
    run(at, repo, &["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")], &[]).is_ok()
}

/// A commit as it is named and what it says, without its content.
#[derive(Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
pub struct Named {
    pub hash: String,
    pub subject: String,
    #[serde(default)]
    pub author: String,
    /// ISO 8601 with offset: the author's date, which a rebase keeps.
    pub date: String,
    /// The value of a `Task:` trailer.
    pub task: Option<String>,
}

/// The commits on `branch` since `base`, oldest first, by name only.
pub fn names(at: &Runner, repo: &Path, base: &str, branch: &str) -> Result<Vec<Named>> {
    if !has_rev(at, repo, branch) {
        return Ok(vec![]);
    }
    since(at, repo, base, branch)
}

/// The full hash `rev` names in `repo`; an error if it names no commit.
pub fn resolve(at: &Runner, repo: &Path, rev: &str) -> Result<String> {
    Ok(git(at, repo, &["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")])?.trim().to_string())
}

/// Every branch, tag and remote ref of a repository with the commit it points at: one call
/// tells which units' branches moved. A branch with an upstream also has "<ref>@{upstream}"
/// naming that upstream's ref (`upstream`): which one is configured changes what a branch is
/// measured against, its commits the same.
pub fn refs(at: &Runner, repo: &Path) -> Result<HashMap<String, String>> {
    let out = git(at, repo, &["for-each-ref", "--format=%(objectname) %(refname) %(upstream)"])?;
    let mut refs = HashMap::new();
    for l in out.lines() {
        let mut f = l.split(' ');
        let (Some(h), Some(r)) = (f.next(), f.next()) else { continue };
        if let Some(up) = f.next().filter(|u| !u.is_empty()) {
            refs.insert(format!("{r}@{{upstream}}"), up.to_string());
        }
        refs.insert(r.to_string(), h.to_string());
    }
    Ok(refs)
}

/// The ref branch `name` has as its upstream in `refs`, if any.
pub fn upstream<'a>(refs: &'a HashMap<String, String>, name: &str) -> Option<&'a str> {
    refs.get(&format!("refs/heads/{name}@{{upstream}}")).map(String::as_str)
}

/// The commit `name` (a branch, a remote branch, a tag, or a full hash) points at in `refs`.
pub fn tip<'a>(refs: &'a HashMap<String, String>, name: &'a str) -> Option<&'a str> {
    ["refs/heads/", "refs/remotes/", "refs/tags/", ""].iter().find_map(|p| refs.get(&format!("{p}{name}")).map(String::as_str)).or_else(|| (name.len() == 40 && name.bytes().all(|b| b.is_ascii_hexdigit())).then_some(name))
}
