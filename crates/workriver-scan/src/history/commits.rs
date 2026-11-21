// Commits with their files' text before and after, many at once.
use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::*;

/// git's empty tree, the "before" of a commit without a parent.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

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
    /// More than one parent. Its files are against its first parent, as every commit's are:
    /// what it brought into the branch, whatever the base is.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub merge: bool,
    /// Its parents (none for a root commit): whether a merge brought the base in depends on
    /// the base, so it is asked of these (`brings_base`), never kept with the commit.
    #[serde(skip)]
    pub parents: Vec<String>,
    /// Whether it brought the unit's base into the branch: worked out by the service against
    /// where the branch meets its base now (`brings_base`); scan leaves it false.
    #[serde(skip)]
    pub base_merge: bool,
    pub files: Vec<FileChange>,
}

/// The contents of many objects at once (`git cat-file --batch`): each spec (a blob hash, or
/// `rev:path`) gives its bytes, or None when there is no such object.
pub(crate) fn blobs(at: &Runner, dir: &Path, specs: &[String]) -> Result<Vec<Option<Vec<u8>>>> {
    if specs.is_empty() {
        return Ok(vec![]);
    }
    let input = specs.iter().map(|s| format!("{s}\n")).collect::<String>().into_bytes();
    let out = run_with(at, dir, &["cat-file", "--batch"], &[], Some(input))?;
    let mut found = Vec::with_capacity(specs.len());
    let mut rest = &out[..];
    for _ in specs {
        let Some(nl) = rest.iter().position(|b| *b == b'\n') else { break };
        let head = String::from_utf8_lossy(&rest[..nl]).into_owned();
        rest = &rest[nl + 1..];
        match head.rsplit(' ').next().and_then(|n| n.parse::<usize>().ok()).filter(|_| !head.ends_with(" missing")) {
            Some(size) if rest.len() > size => {
                found.push(Some(rest[..size].to_vec()));
                rest = &rest[size + 1..];
            }
            _ => found.push(None),
        }
    }
    found.resize(specs.len(), None);
    Ok(found)
}

/// Many commits with their files' texts before and after, in two git calls whatever their
/// number (`git log --raw` for what changed, `git cat-file --batch` for the texts); a merge
/// against its first parent. Falls back to one at a time where git is too old for that.
pub fn commits(at: &Runner, repo: &Path, hashes: &[String]) -> Result<Vec<Commit>> {
    if hashes.is_empty() {
        return Ok(vec![]);
    }
    let fmt = "--format=%x01%H%x02%an%x02%aI%x02%P%x02%(trailers:key=Task,valueonly,separator=%x2C)%x02%s%x02";
    let args: Vec<&str> = ["log", "--no-walk=unsorted", "--diff-merges=first-parent", "--root", "--no-renames", "--raw", "-z", "--no-abbrev", fmt].into_iter().chain(hashes.iter().map(String::as_str)).chain(["--"]).collect();
    let out = match run(at, repo, &args, &[]) {
        Ok(o) => o,
        Err(_) => return hashes.iter().map(|h| commit(at, repo, h)).collect(),
    };
    let mut raws = vec![];
    for chunk in out.split(|b| *b == 1).filter(|c| !c.is_empty()) {
        let text = String::from_utf8_lossy(chunk);
        let mut parts = text.splitn(7, '\u{2}');
        let head: Vec<String> = (0..6).filter_map(|_| parts.next().map(str::to_string)).collect();
        if head.len() < 6 {
            continue;
        }
        raws.push((head, raw(parts.next().unwrap_or_default())));
    }
    // Every text needed, in one call.
    let specs: Vec<String> = raws.iter().flat_map(|(_, files)| files.iter().flat_map(texted)).collect();
    let texts: HashMap<String, Option<Vec<u8>>> = specs.iter().cloned().zip(blobs(at, repo, &specs)?).collect();
    let text = |b: &str| if b == NULL { None } else { texts.get(b).cloned().flatten() };
    Ok(raws
        .into_iter()
        .map(|(head, files)| {
            let [hash, author, date, parents, task, subject] = <[String; 6]>::try_from(head).expect("six fields");
            let id = hash.chars().take(9).collect();
            let task = task.split(',').next().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
            let files = files.into_iter().filter_map(|f| {
                let (b, a) = (f.texted(0).then(|| text(&f.ids[0]).unwrap_or_default()), f.texted(1).then(|| text(&f.ids[1]).unwrap_or_default()));
                change(f, b, a)
            });
            let parents: Vec<String> = parents.split(' ').filter(|p| !p.is_empty()).map(str::to_string).collect();
            Commit { id, hash, subject, date, author, task, merge: parents.len() > 1, parents, base_merge: false, files: files.collect() }
        })
        .collect())
}

/// One commit with its files' text before and after. Very large text files are left out; a
/// binary file is there without texts.
pub fn commit(at: &Runner, repo: &Path, hash: &str) -> Result<Commit> {
    let fmt = "--format=%h%x00%an%x00%aI%x00%P%x00%(trailers:key=Task,valueonly,separator=%x2C)%x00%s";
    let out = git(at, repo, &["log", "-1", "--abbrev=9", fmt, hash, "--"])?;
    let f: Vec<&str> = out.trim_end_matches('\n').splitn(6, '\0').collect();
    let [id, author, date, parents, task, subject] = f[..] else {
        return Err(Error::Git(format!("unexpected git log output for {hash}")));
    };
    let parent = parents.split(' ').next().filter(|p| !p.is_empty()).unwrap_or(EMPTY_TREE);
    let task = task.split(',').next().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
    let mut files = vec![];
    for f in raw(&String::from_utf8_lossy(&run(at, repo, &["diff-tree", "-r", "--no-renames", "-z", "--raw", "--no-abbrev", parent, hash], &[])?)) {
        // By object id, as `commits` reads them; a submodule's pointer has no text.
        let text = |id: &String| blobs(at, repo, std::slice::from_ref(id)).map(|mut b| b.pop().flatten().unwrap_or_default());
        let before = if f.texted(0) { Some(text(&f.ids[0])?) } else { None };
        let after = if f.texted(1) { Some(text(&f.ids[1])?) } else { None };
        if let Some(c) = change(f, before, after) {
            files.push(c);
        }
    }
    let parents: Vec<String> = parents.split(' ').filter(|p| !p.is_empty()).map(str::to_string).collect();
    Ok(Commit { id: id.into(), hash: hash.into(), subject: subject.into(), date: date.into(), author: author.into(), task, merge: parents.len() > 1, parents, base_merge: false, files })
}

/// What changed from commit `from` to commit `to`, as `git diff --no-renames from to` names
/// it, with each file's texts (very large text files left out, a binary file there without
/// texts). Two git calls.
pub fn diff(at: &Runner, repo: &Path, from: &str, to: &str) -> Result<Vec<FileChange>> {
    let raws = raw(&String::from_utf8_lossy(&run(at, repo, &["diff-tree", "-r", "--no-renames", "-z", "--raw", "--no-abbrev", from, to], &[])?));
    let specs: Vec<String> = raws.iter().flat_map(texted).collect();
    let texts: HashMap<String, Option<Vec<u8>>> = specs.iter().cloned().zip(blobs(at, repo, &specs)?).collect();
    let text = |b: &str| if b == NULL { None } else { texts.get(b).cloned().flatten() };
    Ok(raws
        .into_iter()
        .filter_map(|f| {
            let (b, a) = (f.texted(0).then(|| text(&f.ids[0]).unwrap_or_default()), f.texted(1).then(|| text(&f.ids[1]).unwrap_or_default()));
            change(f, b, a)
        })
        .collect())
}

/// The object ids of a file change's sides that have a text (not a submodule's pointer).
fn texted(f: &Raw) -> Vec<String> {
    (0..2).filter(|&k| f.texted(k)).map(|k| f.ids[k].clone()).collect()
}

/// Where two commits meet: their merge base, None when they never do.
fn meet(at: &Runner, repo: &Path, revs: &[&str]) -> Result<Option<String>> {
    let out = run(at, repo, &[&["merge-base"][..], revs].concat(), &[1])?;
    Ok(Some(String::from_utf8_lossy(&out).trim().to_string()).filter(|h| !h.is_empty()))
}

/// Where `tip` and its base meet, as `git diff base...tip` takes it: the newest commit of
/// `tip`'s history that the base has, or its upstream when it has one (a branch made from a
/// newer `origin/main` than the local `main`, as `since` counts it). None when they never do.
pub fn merge_base(at: &Runner, repo: &Path, base: &str, tip: &str) -> Result<Option<String>> {
    let upstream = format!("{base}@{{upstream}}");
    let mut revs = vec![tip, base];
    if has_rev(at, repo, &upstream) {
        revs.push(&upstream);
    }
    // With three, git's merge base of the first and a merge of the other two.
    meet(at, repo, &revs)
}

/// Where a branch merged into its base met it just before: the merge base of `tip` and the
/// base as it stood before the merge that took `tip` in (the first parent of the oldest commit
/// on its first-parent line that has `tip`), on the base or, when that has not got it (or by
/// a fast-forward), on its upstream: the one that took it. None when neither has `tip`, or
/// got it by a fast-forward: their line is the branch's then, and where they stood before is
/// not in the history.
pub fn base_before(at: &Runner, repo: &Path, base: &str, tip: &str) -> Result<Option<String>> {
    let upstream = format!("{base}@{{upstream}}");
    for into in [base, upstream.as_str()] {
        if has_rev(at, repo, into)
            && let Some(met) = before_on(at, repo, into, tip)?
        {
            return Ok(Some(met));
        }
    }
    Ok(None)
}

/// `base_before` on one ref.
fn before_on(at: &Runner, repo: &Path, into: &str, tip: &str) -> Result<Option<String>> {
    let range = format!("{tip}..{into}");
    let line = git(at, repo, &["rev-list", "--first-parent", "--ancestry-path", "--reverse", &range, "--"])?;
    let Some(took) = line.lines().next() else { return Ok(None) };
    let before = format!("{took}^1");
    if !has_rev(at, repo, &before) || is_ancestor(at, repo, tip, &before)? {
        return Ok(None);
    }
    meet(at, repo, &[tip, &before])
}

/// Whether commit `a` is in `b`'s history (or is `b`).
pub fn is_ancestor(at: &Runner, repo: &Path, a: &str, b: &str) -> Result<bool> {
    let full = resolve(at, repo, a)?;
    Ok(meet(at, repo, &[a, b])?.is_some_and(|m| m == full))
}

/// Whether merge `c` brought the base in: a parent other than its first is in the history of
/// `base_at`, where the branch and its base meet. A merge of any other branch did not, even
/// one made from a newer base: what it brought is the branch's.
pub fn brings_base(at: &Runner, repo: &Path, c: &Commit, base_at: &str) -> Result<bool> {
    for p in c.parents.iter().skip(1) {
        if is_ancestor(at, repo, p, base_at)? {
            return Ok(true);
        }
    }
    Ok(false)
}
