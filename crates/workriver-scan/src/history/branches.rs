// A unit's branch: the commits that are its own, whatever was done with it since (merged,
// squashed, gone), and patch ids to find a commit again after a rebase.
use std::collections::HashMap;
use std::path::Path;

use crate::*;

/// `base..branch` (first parent), oldest first, and not what the base's upstream has either
/// when it has one: a local `main` is often behind `origin/main`, and a branch made from (or
/// rebased onto) the newer one would otherwise take in every commit in between. Two git calls.
pub(super) fn since(at: &Runner, repo: &Path, base: &str, branch: &str) -> Result<Vec<Named>> {
    let upstream = format!("^{base}@{{upstream}}");
    let not_base = format!("^{base}");
    let mut args = vec!["--reverse", branch, &not_base];
    if has_rev(at, repo, &upstream[1..]) {
        args.push(&upstream);
    }
    log(at, repo, &args)
}

/// `git log --first-parent` with `args`, by name only.
fn log(at: &Runner, repo: &Path, args: &[&str]) -> Result<Vec<Named>> {
    let fmt = "--format=%H%x00%aI%x00%an%x00%(trailers:key=Task,valueonly,separator=%x2C)%x00%s";
    let out = git(at, repo, &[&["log", "--first-parent", fmt][..], args, &["--"]].concat())?;
    Ok(out
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.splitn(5, '\0').collect();
            let [hash, date, author, task, subject] = f[..] else { return None };
            let task = task.split(',').next().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
            Some(Named { hash: hash.into(), subject: subject.into(), author: author.into(), date: date.into(), task })
        })
        .collect())
}

/// What a unit's branch holds, whatever was done with it since.
#[derive(Clone, PartialEq, Debug)]
pub enum Found {
    /// It has commits its base has not: those (oldest first), and where they start
    /// (`fork`, to be kept: it finds them again once they are merged).
    Ahead { commits: Vec<Named>, fork: String },
    /// All of it is in its base now (a fast-forward, or a merge commit): merged. Its
    /// commits, oldest first, as far as they can be told (see `branch`).
    Merged(Vec<Named>),
    /// It is there, but nothing on it is the unit's yet (just made from its base).
    Fresh,
    /// It is not there (deleted after a squash merge, say): what was seen last stands.
    Gone,
}

/// How many commits back from a merged branch's tip the unit's oldest one is looked for,
/// when where they started is not known.
const WALK: &str = "300";

/// A unit's commits on `branch`. The rules, in one place:
///
/// - The branch has commits `base` has not (`base..branch`, first parent, nor the base's
///   upstream: see `since`): they are the unit's, and `fork` (the parent of the oldest) is where they start. A squash merge
///   leaves the branch this way while it exists: its commits are still listed.
/// - The branch is all in `base` or its upstream (merged by fast-forward or a merge commit;
///   `base..branch` is empty): merged, and the unit's commits are `fork..branch` (first parent) when the
///   fork is known; else, walking back from the tip, every commit down to the oldest one
///   `ours` recognises (its `Task:` trailer, a link). None recognised, or the tip is the
///   fork itself: nothing of the unit's yet, `Fresh`.
/// - The branch is not there: `Gone`; the caller keeps the list it saw last.
pub fn branch(at: &Runner, repo: &Path, base: &str, branch: &str, fork: Option<&str>, ours: &dyn Fn(&Named) -> bool) -> Result<Found> {
    if !has_rev(at, repo, branch) {
        return Ok(Found::Gone);
    }
    let ahead = since(at, repo, base, branch)?;
    if let Some(oldest) = ahead.first() {
        let fork = format!("{}^", oldest.hash);
        return Ok(Found::Ahead { commits: ahead, fork });
    }
    if let Some(fork) = fork.filter(|f| has_rev(at, repo, f)) {
        let since = log(at, repo, &["--reverse", &format!("{fork}..{branch}")])?;
        return Ok(if since.is_empty() { Found::Fresh } else { Found::Merged(since) });
    }
    let back = log(at, repo, &["-n", WALK, branch])?;
    Ok(match back.iter().rposition(ours) {
        Some(k) => Found::Merged(back[..=k].iter().rev().cloned().collect()),
        None => Found::Fresh,
    })
}

/// How many of a branch's changed files name the commit that brought them into the base.
const NAMED_PATHS: usize = 50;

/// Whether a branch git does not show merged looks squash-merged: every file `fork..branch`
/// changes is the same at the tip of `base` (else of its upstream) as at the branch's. Then
/// where (`base`, or the upstream as `origin/main`) and the newest commit there since the
/// fork that touched those files: the squash, usually.
///
/// By content, not patch id: a squash made onto a base that had moved has other context
/// lines, so another patch id, and finding it would mean reading the patch of every commit
/// on the base since the fork. A base that moved on since in other files still shows it; one
/// that changed one of the same files again does not (the reminder after a long ready period
/// covers that). At most five git calls, all by name only.
pub fn squashed(at: &Runner, repo: &Path, base: &str, branch: &str, fork: &str) -> Result<Option<(String, String)>> {
    let changed = |a: &str, b: &str| -> Result<std::collections::HashSet<String>> { Ok(split0(&run(at, repo, &["diff", "--name-only", "--no-renames", "-z", a, b, "--"], &[])?).collect()) };
    let mine = changed(fork, branch)?;
    if mine.is_empty() {
        return Ok(None);
    }
    let upstream = String::from_utf8_lossy(&run(at, repo, &["rev-parse", "--abbrev-ref", &format!("{base}@{{upstream}}")], &[128])?).trim().to_string();
    for into in [base, upstream.as_str()].into_iter().filter(|i| !i.is_empty()) {
        if !changed(into, branch)?.is_disjoint(&mine) {
            continue;
        }
        let mut paths: Vec<String> = mine.iter().map(|p| format!(":(literal){p}")).collect();
        paths.sort();
        paths.truncate(NAMED_PATHS);
        let range = format!("{fork}..{into}");
        let args: Vec<&str> = ["log", "-1", "--format=%H", &range, "--"].into_iter().chain(paths.iter().map(String::as_str)).collect();
        let hash = git(at, repo, &args)?.trim().to_string();
        if !hash.is_empty() {
            return Ok(Some((into.to_string(), hash)));
        }
    }
    Ok(None)
}

/// A commit's patch id (`git patch-id --stable`): the same for the same change after a
/// rebase, when its hash is another. Empty for a commit that changes nothing.
pub fn patch_id(at: &Runner, repo: &Path, hash: &str) -> Result<String> {
    // Two gits and a pipe: through a shell, here as over ssh. The arguments are quoted.
    let script = format!("git --no-optional-locks -C {} show --format= {} -- | git patch-id --stable", quote(&repo.to_string_lossy()), quote(hash));
    let out = at.command(&["sh", "-c", &script]).output()?;
    if !out.status.success() {
        return Err(Error::Git(format!("git patch-id of {hash} in {}: {}", repo.display(), String::from_utf8_lossy(&out.stderr).trim())));
    }
    Ok(String::from_utf8_lossy(&out.stdout).split_whitespace().next().unwrap_or_default().to_string())
}

/// Patch ids of many commits at once (`git patch-id --stable` on their patches), by hash;
/// a commit that changes nothing has none.
pub fn patch_ids(at: &Runner, repo: &Path, hashes: &[String]) -> Result<HashMap<String, String>> {
    if hashes.is_empty() {
        return Ok(HashMap::new());
    }
    let list: Vec<String> = hashes.iter().map(|h| quote(h)).collect();
    let script = format!("git --no-optional-locks -C {} log --no-walk=unsorted -p --no-color --format='commit %H' {} -- | git patch-id --stable", quote(&repo.to_string_lossy()), list.join(" "));
    CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = at.command(&["sh", "-c", &script]).output()?;
    if !out.status.success() {
        return Err(Error::Git(format!("git patch-id in {}: {}", repo.display(), String::from_utf8_lossy(&out.stderr).trim())));
    }
    Ok(String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.split_once(' ')).map(|(p, h)| (h.to_string(), p.to_string())).collect())
}
