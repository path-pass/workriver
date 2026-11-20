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
    /// More than one parent (the base merged into the branch, say); its files are against
    /// its first parent, so they are not work of its own.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub merge: bool,
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
    let specs: Vec<String> = raws.iter().flat_map(|(_, files)| files.iter().flat_map(|f| f.ids.clone())).filter(|b| b != NULL).collect();
    let texts: HashMap<String, Option<Vec<u8>>> = specs.iter().cloned().zip(blobs(at, repo, &specs)?).collect();
    let text = |b: &str| if b == NULL { None } else { texts.get(b).cloned().flatten() };
    Ok(raws
        .into_iter()
        .map(|(head, files)| {
            let [hash, author, date, parents, task, subject] = <[String; 6]>::try_from(head).expect("six fields");
            let id = hash.chars().take(9).collect();
            let task = task.split(',').next().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
            let files = files.into_iter().filter_map(|f| {
                let (b, a) = (if f.status == "A" { None } else { Some(text(&f.ids[0]).unwrap_or_default()) }, if f.status == "D" { None } else { Some(text(&f.ids[1]).unwrap_or_default()) });
                change(f, b, a)
            });
            Commit { id, hash, subject, date, author, task, merge: parents.split(' ').count() > 1, files: files.collect() }
        })
        .collect())
}

/// One commit with its files' text before and after. Binary and very large files are left out.
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
        // By object id, as `commits` reads them: a submodule's pointer has no text of its own.
        let text = |id: &String| blobs(at, repo, std::slice::from_ref(id)).map(|mut b| b.pop().flatten().unwrap_or_default());
        let before = if f.status == "A" { None } else { Some(text(&f.ids[0])?) };
        let after = if f.status == "D" { None } else { Some(text(&f.ids[1])?) };
        if let Some(c) = change(f, before, after) {
            files.push(c);
        }
    }
    Ok(Commit { id: id.into(), hash: hash.into(), subject: subject.into(), date: date.into(), author: author.into(), task, merge: parents.split(' ').count() > 1, files })
}
