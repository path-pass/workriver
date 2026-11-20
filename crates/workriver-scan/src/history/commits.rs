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
    /// More than one parent. Its files are against its first parent (what it brings is the
    /// branch's), unless it is `from_base`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub merge: bool,
    /// A merge that brings the base in (a parent other than its first is in the base, or in
    /// the base's upstream): its files are against that parent, so they are the branch's whole
    /// change as of it, as `git diff base...merge` names it, and none of the base's own work.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub from_base: bool,
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

/// The parent of a merge, other than its first, that `base` or its upstream has: where the
/// merge brought the base in. None for a merge of anything else, and with no base. A few git
/// calls, for merges only.
// ponytail: git keeps no record of which side was the base, so a merge of another branch
// that has since reached the base too (the unit merged, then read again) counts as one.
fn base_side(at: &Runner, repo: &Path, parents: &[&str], base: Option<&str>) -> Result<Option<String>> {
    let Some(base) = base.filter(|_| parents.len() > 1) else { return Ok(None) };
    let upstream = format!("{base}@{{upstream}}");
    let tips: Vec<&str> = [base, upstream.as_str()].into_iter().filter(|t| has_rev(at, repo, t)).collect();
    for p in &parents[1..] {
        for t in &tips {
            // An ancestor of the tip is where the two meet (exit 1: they never do).
            if String::from_utf8_lossy(&run(at, repo, &["merge-base", p, t], &[1])?).trim() == *p {
                return Ok(Some(p.to_string()));
            }
        }
    }
    Ok(None)
}

/// What changed from `from` to commit `to`, as `--raw` names it.
fn raw_between(at: &Runner, repo: &Path, from: &str, to: &str) -> Result<Vec<Raw>> {
    Ok(raw(&String::from_utf8_lossy(&run(at, repo, &["diff-tree", "-r", "--no-renames", "-z", "--raw", "--no-abbrev", from, to], &[])?)))
}

/// Many commits with their files' texts before and after, in two git calls whatever their
/// number (`git log --raw` for what changed, `git cat-file --batch` for the texts); a merge
/// against its first parent, but one that brings in `base` against the base's side
/// (`Commit::from_base`; a few calls more for each merge). Falls back to one at a time where
/// git is too old for that.
pub fn commits(at: &Runner, repo: &Path, hashes: &[String], base: Option<&str>) -> Result<Vec<Commit>> {
    if hashes.is_empty() {
        return Ok(vec![]);
    }
    let fmt = "--format=%x01%H%x02%an%x02%aI%x02%P%x02%(trailers:key=Task,valueonly,separator=%x2C)%x02%s%x02";
    let args: Vec<&str> = ["log", "--no-walk=unsorted", "--diff-merges=first-parent", "--root", "--no-renames", "--raw", "-z", "--no-abbrev", fmt].into_iter().chain(hashes.iter().map(String::as_str)).chain(["--"]).collect();
    let out = match run(at, repo, &args, &[]) {
        Ok(o) => o,
        Err(_) => return hashes.iter().map(|h| commit(at, repo, h, base)).collect(),
    };
    let mut raws = vec![];
    for chunk in out.split(|b| *b == 1).filter(|c| !c.is_empty()) {
        let text = String::from_utf8_lossy(chunk);
        let mut parts = text.splitn(7, '\u{2}');
        let head: Vec<String> = (0..6).filter_map(|_| parts.next().map(str::to_string)).collect();
        if head.len() < 6 {
            continue;
        }
        let parents: Vec<&str> = head[3].split(' ').collect();
        let (files, from_base) = match base_side(at, repo, &parents, base)? {
            Some(side) => (raw_between(at, repo, &side, &head[0])?, true),
            None => (raw(parts.next().unwrap_or_default()), false),
        };
        raws.push((head, files, from_base));
    }
    // Every text needed, in one call.
    let specs: Vec<String> = raws.iter().flat_map(|(_, files, _)| files.iter().flat_map(|f| f.ids.clone())).filter(|b| b != NULL).collect();
    let texts: HashMap<String, Option<Vec<u8>>> = specs.iter().cloned().zip(blobs(at, repo, &specs)?).collect();
    let text = |b: &str| if b == NULL { None } else { texts.get(b).cloned().flatten() };
    Ok(raws
        .into_iter()
        .map(|(head, files, from_base)| {
            let [hash, author, date, parents, task, subject] = <[String; 6]>::try_from(head).expect("six fields");
            let id = hash.chars().take(9).collect();
            let task = task.split(',').next().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
            let files = files.into_iter().filter_map(|f| {
                let (b, a) = (if f.status == "A" { None } else { Some(text(&f.ids[0]).unwrap_or_default()) }, if f.status == "D" { None } else { Some(text(&f.ids[1]).unwrap_or_default()) });
                change(f, b, a)
            });
            Commit { id, hash, subject, date, author, task, merge: parents.split(' ').count() > 1, from_base, files: files.collect() }
        })
        .collect())
}

/// One commit with its files' text before and after, against its first parent or, for a
/// merge that brings in `base`, the base's side (as `commits`). Binary and very large files
/// are left out.
pub fn commit(at: &Runner, repo: &Path, hash: &str, base: Option<&str>) -> Result<Commit> {
    let fmt = "--format=%h%x00%an%x00%aI%x00%P%x00%(trailers:key=Task,valueonly,separator=%x2C)%x00%s";
    let out = git(at, repo, &["log", "-1", "--abbrev=9", fmt, hash, "--"])?;
    let f: Vec<&str> = out.trim_end_matches('\n').splitn(6, '\0').collect();
    let [id, author, date, parents, task, subject] = f[..] else {
        return Err(Error::Git(format!("unexpected git log output for {hash}")));
    };
    let all: Vec<&str> = parents.split(' ').collect();
    let side = base_side(at, repo, &all, base)?;
    let parent = side.as_deref().or(all.first().copied().filter(|p| !p.is_empty())).unwrap_or(EMPTY_TREE);
    let task = task.split(',').next().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
    let mut files = vec![];
    for f in raw_between(at, repo, parent, hash)? {
        // By object id, as `commits` reads them: a submodule's pointer has no text of its own.
        let text = |id: &String| blobs(at, repo, std::slice::from_ref(id)).map(|mut b| b.pop().flatten().unwrap_or_default());
        let before = if f.status == "A" { None } else { Some(text(&f.ids[0])?) };
        let after = if f.status == "D" { None } else { Some(text(&f.ids[1])?) };
        if let Some(c) = change(f, before, after) {
            files.push(c);
        }
    }
    Ok(Commit { id: id.into(), hash: hash.into(), subject: subject.into(), date: date.into(), author: author.into(), task, merge: all.len() > 1, from_base: side.is_some(), files })
}
