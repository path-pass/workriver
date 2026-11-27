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
    /// object id), a binary side (its bytes are never sent) and one that could not be read.
    /// A text side keeps its text next to a binary or pointer side: a change over several
    /// commits is from its ends, which may both be text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// Each side's git entry, as `--raw` names it: its mode ("100644"; "000000" where the side
    /// does not exist) and object id (zeros where it does not exist, and for the work tree's
    /// side of uncommitted work, which git has not hashed: 40 zeros, or 64 in a SHA-256
    /// repository, `is_null`). Whether a file changed over several commits is whether these
    /// differ at the ends, not the texts.
    pub before_mode: String,
    pub before_id: String,
    pub after_mode: String,
    pub after_id: String,
    /// Each side is binary, as git takes it by default (a NUL in its first 8000 bytes): where
    /// either is, git says "Binary files differ", so that side's text is not there and both
    /// counts are 0.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub before_binary: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub after_binary: bool,
    /// Its texts were not read, so its lines were not counted (`add` and `del` are 0): uncommitted
    /// work read for its names only (`live_with`), or uncommitted work over the size limit on
    /// either side.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub uncounted: bool,
    /// The work tree's file as last seen (`FileStat`), where it is on this machine: what
    /// stands for its bytes, read or not (`Known`, the code token). None for a committed side,
    /// a deleted file, a submodule's pointer and a file over ssh.
    #[serde(skip)]
    pub stamp: Option<FileStat>,
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
    live_with(at, dir, &mut Known::default(), &Want::Texts)
}

/// What a work tree's uncommitted files were last read as (`live_with`), by path.
#[derive(Default, Clone, Debug)]
pub struct Known {
    /// Each file's git entry and stat as last seen, whether its texts are held, and what was
    /// found (with its texts where held).
    files: HashMap<String, (Stamp, bool, FileChange)>,
    /// Work-tree files read so far (the timing log's, and what tests count).
    pub reads: u64,
}

impl Known {
    /// Paths a change was seen at (a file, or a folder and all in it): read again at the next
    /// read, whatever their stat says.
    pub fn forget<'a>(&mut self, paths: impl IntoIterator<Item = &'a str>) {
        for p in paths {
            let inside = format!("{p}/");
            self.files.retain(|k, _| k != p && !k.starts_with(&inside));
        }
    }

    /// Lets go of the texts held; names, entries and counts stay.
    pub fn drop_texts(&mut self) {
        for (_, held, c) in self.files.values_mut() {
            *held = false;
            (c.before, c.after) = (None, None);
        }
    }

    /// Whether any texts are held.
    pub fn has_texts(&self) -> bool {
        self.files.values().any(|f| f.1)
    }

    /// `files`, as a read gave them, with their texts: None when one of them is held without.
    pub fn with_texts(&self, files: &[FileChange]) -> Option<Vec<FileChange>> {
        files
            .iter()
            .map(|f| match self.files.get(&f.path) {
                Some((_, true, c)) => Some(c.clone()),
                Some(_) => None,
                // Never kept (a submodule's pointer): as given, it has no text.
                None => Some(f.clone()),
            })
            .collect()
    }
}

/// A file as `diff --raw` (or `ls-files --others`) names it and as the file system has it.
#[derive(Clone, PartialEq, Debug)]
struct Stamp {
    status: String,
    modes: [String; 2],
    ids: [String; 2],
    file: Option<FileStat>,
}

/// How much of a work tree's changed files `live_with` reads.
#[derive(Clone, Debug)]
pub enum Want {
    /// Their texts and line counts.
    Texts,
    /// Their names and git entries only; for these paths also the id `git add` would give the
    /// file (`git hash-object`, its text not read here; a symlink's from where it points, as
    /// git stores one): what tells whether it is back as it was.
    Names(std::collections::HashSet<String>),
}

/// Uncommitted changes in the work tree `dir`, against its last commit, reading only what
/// `known` does not hold: a file whose git entry and stat (size, times, inode) are as they
/// were is taken from there, not read, cat-filed or hashed again (a file over ssh has no
/// stat, so it is read every time). `want` says how much of a file that changed is read; for
/// names only, counts stay where `known` has them for that same file (`FileChange::uncounted`
/// where not), and no texts are given. A file over the size limit on either side (the work
/// tree's by its stat, the committed one by its size in git) is never read, cat-filed or
/// hashed: its name and entries only, uncounted, in both kinds of read. `known` keeps the texts
/// it holds until `Known::drop_texts`.
pub fn live_with(at: &Runner, dir: &Path, known: &mut Known, want: &Want) -> Result<Vec<FileChange>> {
    let dir = dir.to_path_buf();
    let texts = matches!(want, Want::Texts);
    let id_wanted = |path: &str| matches!(want, Want::Names(ids) if ids.contains(path));
    let out = run(at, &dir, &["diff", "HEAD", "--no-renames", "-z", "--raw", "--no-abbrev"], &[])?;
    let mut listed = raw(&String::from_utf8_lossy(&out));
    // git names no mode for an untracked file: as `git add` would take it, a symlink (120000,
    // its text where it points), else a regular file (over ssh, where there is no stat, as the
    // read finds).
    // ponytail: an executable one is taken as 100644 too.
    let untracked = run(at, &dir, &["ls-files", "--others", "--exclude-standard", "-z"], &[])?;
    let tracked = listed.len();
    for path in split0(&untracked) {
        let link = at.stat(&dir.join(&path)).is_some_and(|s| s.link);
        listed.push(Raw { status: "A".into(), path, modes: [GONE.into(), if link { LINK } else { "100644" }.into()], ids: [NULL.into(), NULL.into()] });
    }
    // Each file: taken as known, named only, or to be read (in order, by its place).
    let mut slots: Vec<Option<FileChange>> = vec![None; listed.len()];
    let mut stamps = Vec::with_capacity(listed.len());
    let mut fresh: HashMap<String, (Stamp, bool, FileChange)> = HashMap::new();
    let (mut to_read, mut to_hash) = (vec![], vec![]);
    for (k, r) in listed.iter().enumerate() {
        // A submodule's pointer is worked out every time: it names no file to stat.
        let pointer = r.modes.iter().any(|m| m == GITLINK);
        let stat = if r.modes[1] == GONE || pointer { None } else { at.stat(&dir.join(&r.path)) };
        let stamp = Stamp { status: r.status.clone(), modes: r.modes.clone(), ids: r.ids.clone(), file: stat };
        // Known as it is: its side there is the same file, or there is none (deleted).
        let kept = known.files.remove(&r.path).filter(|(s, _, _)| *s == stamp && !pointer && (stamp.file.is_some() || r.modes[1] == GONE));
        match kept {
            Some((s, held, c)) if held || !texts => {
                slots[k] = Some(if texts { c.clone() } else { c.bare() });
                fresh.insert(r.path.clone(), (s, held, c));
            }
            _ if pointer || texts => to_read.push(k),
            _ => {
                let c = named(r, stat);
                slots[k] = Some(c.clone());
                // Kept (a deleted file too), so a read with texts knows its texts are not held.
                if stat.is_some() || r.modes[1] == GONE {
                    fresh.insert(r.path.clone(), (stamp.clone(), false, c));
                }
            }
        }
        // Named only, and its id wanted: hashed, unless it is over the size limit.
        let unhashed = slots[k].as_ref().is_some_and(|c| !texts && c.after_mode != GONE && is_null(&c.after_id));
        if unhashed && !pointer && id_wanted(&r.path) && stat.is_some_and(|s| s.len <= MAX_TEXT as u64) {
            to_hash.push(k);
        }
        stamps.push(stamp);
    }
    // Not one over the size limit on its committed side either (sizes from git, in one call).
    let olds: Vec<String> = to_hash.iter().map(|&k| &listed[k]).filter(|r| r.texted(0)).map(|r| r.ids[0].clone()).collect();
    let big: std::collections::HashSet<String> = olds.iter().cloned().zip(blob_sizes(at, &dir, &olds)?).filter(|(_, n)| n.is_some_and(|n| n > MAX_TEXT as u64)).map(|(id, _)| id).collect();
    to_hash.retain(|&k| !(listed[k].texted(0) && big.contains(&listed[k].ids[0])));
    // The ids wanted: a symlink's from where it points (its own text), any other in one call.
    let (links, plain): (Vec<usize>, Vec<usize>) = to_hash.into_iter().partition(|&k| listed[k].modes[1] == LINK);
    let paths: Vec<String> = plain.iter().map(|&k| listed[k].path.clone()).collect();
    let mut ids: Vec<(usize, Option<String>)> = plain.iter().copied().zip(hashes(at, &dir, &paths)?).collect();
    for k in links {
        known.reads += 1;
        let id = match at.read_link(&dir.join(&listed[k].path)) {
            Some(text) => hash_text(at, &dir, text)?,
            None => None,
        };
        ids.push((k, id));
    }
    for (k, id) in ids {
        if let (Some(id), Some(c)) = (id, slots[k].as_mut()) {
            c.after_id = id;
            // Kept with it, but on a file held with its texts, which a read with texts gives
            // as from nothing (unhashed: its text stands for it).
            if let Some(f) = fresh.get_mut(&c.path).filter(|f| !f.1) {
                f.2.after_id = c.after_id.clone();
            }
        }
    }
    // Over the size limit on either side: named only, as read as it will be. The committed
    // side's size from git, for all of them in one call.
    let olds: Vec<String> = to_read.iter().map(|&k| &listed[k]).filter(|r| r.texted(0)).map(|r| r.ids[0].clone()).collect();
    let sizes: HashMap<String, u64> = olds.iter().cloned().zip(blob_sizes(at, &dir, &olds)?).filter_map(|(id, n)| Some((id, n?))).collect();
    to_read.retain(|&k| {
        let r = &listed[k];
        let big = stamps[k].file.is_some_and(|s| s.len > MAX_TEXT as u64) || (r.texted(0) && sizes.get(&r.ids[0]).is_some_and(|n| *n > MAX_TEXT as u64));
        if big {
            let c = named(r, stamps[k].file);
            if stamps[k].file.is_some() || r.modes[1] == GONE {
                fresh.insert(r.path.clone(), (stamps[k].clone(), true, c.clone()));
            }
            slots[k] = Some(c);
        }
        !big
    });
    // What was committed, for every file read in one call, by object id (a name may carry
    // a line break, which ends a line of `cat-file --batch`); what is there now, from disk.
    let specs: Vec<String> = to_read.iter().map(|&k| &listed[k]).filter(|r| r.texted(0)).map(|r| r.ids[0].clone()).collect();
    let mut before = blobs(at, &dir, &specs)?.into_iter();
    let mut read = vec![];
    for &k in &to_read {
        let mut r = Raw { status: listed[k].status.clone(), path: listed[k].path.clone(), modes: listed[k].modes.clone(), ids: listed[k].ids.clone() };
        let path = dir.join(&r.path);
        let before = if r.texted(0) { Some(before.next().flatten().unwrap_or_default()) } else { None };
        let after = if k >= tracked {
            // Untracked: whether it is a symlink, from the read itself.
            known.reads += 1;
            let got = at.read_new(&path);
            r.modes[1] = if got.as_ref().is_some_and(|g| g.0) { LINK } else { "100644" }.into();
            got.map(|g| g.1)
        } else if !r.texted(1) {
            None
        } else {
            known.reads += 1;
            // A symlink's text is where it points (as git stores it), not the file there.
            if r.modes[1] == LINK { at.read_link(&path) } else { at.read(&path) }
        };
        // Grown over the limit since its stat, or read over ssh (no stat): named only.
        if after.as_ref().is_some_and(|a| a.len() > MAX_TEXT) {
            slots[k] = Some(named(&r, stamps[k].file));
            continue;
        }
        // A submodule's pointer, which `--raw` leaves unnamed here: the commit its checkout is
        // at (`git diff` shows that one), when it is checked out (the top of its own work tree).
        // One git cannot read there (gone since git listed it) stays unknown, as an unhashed
        // side does; the others keep theirs.
        if r.modes[1] == GITLINK && is_null(&r.ids[1]) {
            let out = match git(at, &dir.join(&r.path), &["rev-parse", "--show-prefix", "HEAD"]) {
                Err(Error::Git(_)) => String::new(),
                out => out?,
            };
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
        if let Some(mut c) = change(r, before, after) {
            c.stamp = stamps[k].file;
            read.push((k, c));
        }
    }
    // A binary side git has not hashed has no text to stand for it: the id `git add` would
    // give it.
    let unhashed: Vec<usize> = (0..read.len()).filter(|&k| read[k].1.after_binary && is_null(&read[k].1.after_id)).collect();
    let paths: Vec<String> = unhashed.iter().map(|&k| read[k].1.path.clone()).collect();
    for (&k, id) in unhashed.iter().zip(hashes(at, &dir, &paths)?) {
        if let Some(id) = id {
            read[k].1.after_id = id;
        }
    }
    for (k, c) in read {
        // Kept for next time where there is a stat to tell it is the same file (not over ssh).
        if !listed[k].modes.iter().any(|m| m == GITLINK) && (stamps[k].file.is_some() || listed[k].modes[1] == GONE) {
            fresh.insert(c.path.clone(), (stamps[k].clone(), true, c.clone()));
        }
        slots[k] = Some(c);
    }
    // Files no longer changed are forgotten.
    known.files = fresh;
    Ok(slots.into_iter().flatten().collect())
}

/// A file by its name and git entries only: its texts not read, its lines not counted.
fn named(r: &Raw, stat: Option<FileStat>) -> FileChange {
    FileChange { path: r.path.clone(), status: r.status.clone(), add: 0, del: 0, before: None, after: None, before_mode: r.modes[0].clone(), before_id: r.ids[0].clone(), after_mode: r.modes[1].clone(), after_id: r.ids[1].clone(), before_binary: false, after_binary: false, uncounted: true, stamp: stat }
}

/// The id git gives `bytes` as an object's content (`hash-object --stdin`); None where git
/// cannot tell.
fn hash_text(at: &Runner, dir: &Path, bytes: Vec<u8>) -> Result<Option<String>> {
    match run_with(at, dir, &["hash-object", "--stdin"], &[], Some(bytes)) {
        Ok(out) => Ok(Some(String::from_utf8_lossy(&out).trim().to_string()).filter(|s| !s.is_empty())),
        Err(Error::Git(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Which of `paths` (in the work tree `dir`) git ignores, as `.gitignore` and the like say:
/// one call for all. A tracked file is never ignored.
pub fn ignored(at: &Runner, dir: &Path, paths: &[PathBuf]) -> Result<std::collections::HashSet<PathBuf>> {
    let names: Vec<(&PathBuf, String)> = paths.iter().filter_map(|p| Some((p, p.strip_prefix(dir).ok()?.to_string_lossy().into_owned()))).filter(|(_, n)| !n.is_empty()).collect();
    if names.is_empty() {
        return Ok(Default::default());
    }
    let input: Vec<u8> = names.iter().flat_map(|(_, n)| n.bytes().chain([0])).collect();
    // 1: none of them is ignored.
    let out = run_with(at, dir, &["check-ignore", "--stdin", "-z"], &[1], Some(input))?;
    let hit: std::collections::HashSet<String> = split0(&out).collect();
    Ok(names.into_iter().filter(|(_, n)| hit.contains(n)).map(|(p, _)| p.clone()).collect())
}

/// The ids `git add` would give the files at `paths`, all in one call; None for one git
/// cannot read (not allowed, gone since), which fails the whole call: then each alone, so the
/// others keep theirs and that one stays unhashed (git lists it as changed all the same).
fn hashes(at: &Runner, dir: &Path, paths: &[String]) -> Result<Vec<Option<String>>> {
    if paths.is_empty() {
        return Ok(vec![]);
    }
    let one = |paths: &[String]| -> Result<String> {
        let args: Vec<&str> = ["hash-object", "--"].into_iter().chain(paths.iter().map(String::as_str)).collect();
        git(at, dir, &args)
    };
    match one(paths) {
        Ok(out) => Ok(out.lines().map(|id| Some(id.to_string())).collect()),
        Err(Error::Git(_)) => paths
            .iter()
            .map(|p| match one(std::slice::from_ref(p)) {
                Ok(id) => Ok(Some(id.trim().to_string())),
                Err(Error::Git(_)) => Ok(None),
                Err(e) => Err(e),
            })
            .collect(),
        Err(e) => Err(e),
    }
}

/// The work tree (main or linked) that has `branch` checked out.
pub fn worktree_of(at: &Runner, repo: &Path, branch: &str) -> Result<Option<PathBuf>> {
    let want = format!("branch refs/heads/{branch}");
    let list = git(at, repo, &["worktree", "list", "--porcelain"])?;
    Ok(list.split("\n\n").find(|b| b.lines().any(|l| l == want)).and_then(|b| b.lines().next()?.strip_prefix("worktree ").map(PathBuf::from)))
}

/// A file change with its sides' bytes (None where a side has no text: it does not exist, or
/// is a submodule's pointer). A binary side (git's default: a NUL in its first 8000 bytes)
/// has no text; the other keeps its own. Line counts only where git counts lines: both sides
/// text, neither a submodule's pointer (a file made one, or one made a file, counts none, as
/// every pointer's change). None for a very large text file in a change with lines.
// ponytail: very large text files are dropped from the diff, not shown as such.
pub(crate) fn change(r: Raw, before: Option<Vec<u8>>, after: Option<Vec<u8>>) -> Option<FileChange> {
    let binary = |b: &Option<Vec<u8>>| b.as_ref().is_some_and(|b| b[..b.len().min(FIRST_FEW)].contains(&0));
    let (before_binary, after_binary) = (binary(&before), binary(&after));
    let lined = !before_binary && !after_binary && !r.modes.iter().any(|m| m == GITLINK);
    let text = |b: Option<Vec<u8>>, binary: bool| -> Option<Option<String>> {
        match b {
            _ if binary => Some(None),
            None => Some(None),
            Some(b) if b.len() > MAX_TEXT => (!lined).then_some(None),
            Some(b) => Some(Some(String::from_utf8_lossy(&b).into_owned())),
        }
    };
    let (before, after) = (text(before, before_binary)?, text(after, after_binary)?);
    let (add, del) = if lined { counts(before.as_deref().unwrap_or_default(), after.as_deref().unwrap_or_default()) } else { (0, 0) };
    let Raw { status, path, modes: [before_mode, after_mode], ids: [before_id, after_id] } = r;
    Some(FileChange { add, del, before, after, path, status, before_mode, before_id, after_mode, after_id, before_binary, after_binary, uncounted: false, stamp: None })
}

pub(crate) fn split0(out: &[u8]) -> impl Iterator<Item = String> + '_ {
    out.split(|b| *b == 0).filter(|s| !s.is_empty()).map(|s| String::from_utf8_lossy(s).into_owned())
}

/// The mode of a side that does not exist, and its object id (git's, in `--raw`).
pub(crate) const GONE: &str = "000000";
pub(crate) const NULL: &str = "0000000000000000000000000000000000000000";

/// Whether an object id is git's placeholder (a side that does not exist, or one git has not
/// hashed): all zeros, 40 or, in a SHA-256 repository, 64 of them.
pub(crate) fn is_null(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b == b'0')
}
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
/// break before the first, in `git log`'s). Only the line break before a `:mode` is
/// formatting: a path is taken as it is, line breaks and all.
pub(crate) fn raw(out: &str) -> Vec<Raw> {
    let mut files = vec![];
    let mut tokens = out.split('\0');
    while let Some(t) = tokens.next() {
        let Some(meta) = t.trim_start_matches('\n').strip_prefix(':') else { continue };
        let Some(path) = tokens.next() else { break };
        if let [before_mode, after_mode, before_id, after_id, status, ..] = meta.split(' ').collect::<Vec<_>>()[..] {
            let status = if status.starts_with('A') || status.starts_with('D') { status[..1].to_string() } else { "M".into() };
            files.push(Raw { status, path: path.into(), modes: [before_mode.into(), after_mode.into()], ids: [before_id.into(), after_id.into()] });
        }
    }
    files
}
