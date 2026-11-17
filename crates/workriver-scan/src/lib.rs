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

/// Where git runs: on this machine, or on another one over ssh (the same commands).
#[derive(Clone, PartialEq, Debug, Default)]
pub enum Runner {
    #[default]
    Local,
    Ssh {
        /// `ssh`, or a stand-in in tests.
        program: String,
        /// Options before the target.
        options: Vec<String>,
        /// Anything ssh takes as a destination: a Host of the user's config, `user@host`.
        target: String,
    },
}

/// One argument as a POSIX shell word: ssh hands the remote shell one string.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

impl Runner {
    /// git over ssh to `target`, never asking for anything (BatchMode), sharing one
    /// connection through a control socket in `control` (a folder only the user can read).
    /// Whatever the user's ssh config says, it forwards no agent, takes no unknown host key
    /// and sets up none of the config's port forwards.
    pub fn ssh(target: &str, control: &Path) -> Runner {
        let opts = ["BatchMode=yes", "ForwardAgent=no", "StrictHostKeyChecking=yes", "ClearAllForwardings=yes", "ConnectTimeout=5", "ServerAliveInterval=10", "ServerAliveCountMax=3", "ControlMaster=auto", "ControlPersist=120"];
        let mut options: Vec<String> = opts.iter().flat_map(|o| ["-o".to_string(), o.to_string()]).collect();
        options.extend(["-o".to_string(), format!("ControlPath={}", control.join("ssh-%C").display())]);
        Runner::Ssh { program: "ssh".into(), options, target: target.into() }
    }

    /// The process that runs `words` (a program and its arguments) there.
    pub fn command(&self, words: &[&str]) -> Command {
        match self {
            Runner::Local => {
                let mut c = Command::new(words[0]);
                c.args(&words[1..]);
                c
            }
            Runner::Ssh { program, options, target } => {
                let mut c = Command::new(program);
                c.args(options).arg("--").arg(target).arg(words.iter().map(|w| quote(w)).collect::<Vec<_>>().join(" "));
                c.stdin(std::process::Stdio::null());
                c
            }
        }
    }

    /// A file's bytes there; None if it cannot be read.
    fn read(&self, path: &Path) -> Option<Vec<u8>> {
        match self {
            Runner::Local => std::fs::read(path).ok(),
            Runner::Ssh { .. } => self.command(&["cat", "--", &path.to_string_lossy()]).output().ok().filter(|o| o.status.success()).map(|o| o.stdout),
        }
    }
}

/// How many git processes were run, here and over ssh: for the timing log.
pub static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Runs git in `dir`; exit codes in `ok` besides 0 count as success. Optional locks are off
/// so reading never rewrites the index, which the watcher would take for a change.
fn run(at: &Runner, dir: &Path, args: &[&str], ok: &[i32]) -> Result<Vec<u8>> {
    run_with(at, dir, args, ok, None)
}

/// The same, with `input` on git's stdin.
fn run_with(at: &Runner, dir: &Path, args: &[&str], ok: &[i32], input: Option<Vec<u8>>) -> Result<Vec<u8>> {
    let dir_s = dir.to_string_lossy();
    let mut words = vec!["git", "--no-optional-locks", "-C", &dir_s];
    words.extend_from_slice(args);
    let started = std::time::Instant::now();
    CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut cmd = at.command(&words);
    let out = match input {
        None => cmd.output()?,
        Some(bytes) => {
            let mut child = cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn()?;
            let mut stdin = child.stdin.take().expect("piped");
            let writer = std::thread::spawn(move || std::io::Write::write_all(&mut stdin, &bytes));
            let out = child.wait_with_output()?;
            let _ = writer.join();
            out
        }
    };
    let code = out.status.code().unwrap_or(-1);
    // What git was asked, without where (a path names the user's folders) or which files.
    let said: Vec<&str> = args.iter().map(|a| if a.contains('/') { "…" } else { *a }).take(8).collect();
    tracing::debug!(target: "perf", kind = "git", via = if matches!(at, Runner::Local) { "local" } else { "ssh" }, args = %said.join(" "), ms = started.elapsed().as_millis() as u64, code, bytes = out.stdout.len());
    if code != 0 && !ok.contains(&code) {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(Error::Git(format!("git {} in {}: {}", args.join(" "), dir.display(), err.trim())));
    }
    Ok(out.stdout)
}

fn git(at: &Runner, dir: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8_lossy(&run(at, dir, args, &[])?).into_owned())
}

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

/// `base..branch` (first parent), oldest first, and not what the base's upstream has either
/// when it has one: a local `main` is often behind `origin/main`, and a branch made from (or
/// rebased onto) the newer one would otherwise take in every commit in between. Two git calls.
fn since(at: &Runner, repo: &Path, base: &str, branch: &str) -> Result<Vec<Named>> {
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

/// The full hash `rev` names in `repo`; an error if it names no commit.
pub fn resolve(at: &Runner, repo: &Path, rev: &str) -> Result<String> {
    Ok(git(at, repo, &["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")])?.trim().to_string())
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

/// Every branch, tag and remote ref of a repository with the commit it points at: one call
/// tells which units' branches moved.
pub fn refs(at: &Runner, repo: &Path) -> Result<HashMap<String, String>> {
    let out = git(at, repo, &["for-each-ref", "--format=%(objectname) %(refname)"])?;
    Ok(out.lines().filter_map(|l| l.split_once(' ')).map(|(h, r)| (r.to_string(), h.to_string())).collect())
}

/// The commit `name` (a branch, a remote branch, a tag, or a full hash) points at in `refs`.
pub fn tip<'a>(refs: &'a HashMap<String, String>, name: &'a str) -> Option<&'a str> {
    ["refs/heads/", "refs/remotes/", "refs/tags/", ""].iter().find_map(|p| refs.get(&format!("{p}{name}")).map(String::as_str)).or_else(|| (name.len() == 40 && name.bytes().all(|b| b.is_ascii_hexdigit())).then_some(name))
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

/// The contents of many objects at once (`git cat-file --batch`): each spec (a blob hash, or
/// `rev:path`) gives its bytes, or None when there is no such object.
fn blobs(at: &Runner, dir: &Path, specs: &[String]) -> Result<Vec<Option<Vec<u8>>>> {
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
    const NULL: &str = "0000000000000000000000000000000000000000";
    struct Raw {
        head: Vec<String>,
        files: Vec<(String, String, String, String)>, // status, path, before blob, after blob
    }
    let mut raws = vec![];
    for chunk in out.split(|b| *b == 1).filter(|c| !c.is_empty()) {
        let text = String::from_utf8_lossy(chunk);
        let mut parts = text.splitn(7, '\u{2}');
        let head: Vec<String> = (0..6).filter_map(|_| parts.next().map(str::to_string)).collect();
        if head.len() < 6 {
            continue;
        }
        let mut files = vec![];
        let mut tokens = parts.next().unwrap_or_default().split('\0').map(|t| t.trim_start_matches('\n'));
        while let Some(t) = tokens.next() {
            let Some(meta) = t.strip_prefix(':') else { continue };
            let Some(path) = tokens.next() else { break };
            let m: Vec<&str> = meta.split(' ').collect();
            if m.len() >= 5 {
                let status = if m[4].starts_with('A') || m[4].starts_with('D') { m[4][..1].to_string() } else { "M".into() };
                files.push((status, path.to_string(), m[2].to_string(), m[3].to_string()));
            }
        }
        raws.push(Raw { head, files });
    }
    // Every text needed, in one call.
    let specs: Vec<String> = raws.iter().flat_map(|r| r.files.iter().flat_map(|f| [f.2.clone(), f.3.clone()])).filter(|b| b != NULL).collect();
    let texts: HashMap<String, Option<Vec<u8>>> = specs.iter().cloned().zip(blobs(at, repo, &specs)?).collect();
    let text = |b: &str| if b == NULL { None } else { texts.get(b).cloned().flatten() };
    Ok(raws
        .into_iter()
        .map(|r| {
            let [hash, author, date, parents, task, subject] = <[String; 6]>::try_from(r.head).expect("six fields");
            let id = hash.chars().take(9).collect();
            let task = task.split(',').next().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
            let files = r.files.into_iter().filter_map(|(status, path, before, after)| {
                let (b, a) = (if status == "A" { None } else { Some(text(&before).unwrap_or_default()) }, if status == "D" { None } else { Some(text(&after).unwrap_or_default()) });
                change(path, status, b, a)
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
    for (status, path) in name_status(&run(at, repo, &["diff-tree", "-r", "--no-renames", "-z", "--name-status", parent, hash], &[])?) {
        let before = if status == "A" { None } else { Some(blob(at, repo, parent, &path)?) };
        let after = if status == "D" { None } else { Some(blob(at, repo, hash, &path)?) };
        if let Some(c) = change(path, status, before, after) {
            files.push(c);
        }
    }
    Ok(Commit { id: id.into(), hash: hash.into(), subject: subject.into(), date: date.into(), author: author.into(), task, merge: parents.split(' ').count() > 1, files })
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

/// The repository's files at HEAD as a tree of folders, with line counts.
pub fn tree(at: &Runner, repo: &Path) -> Result<Module> {
    let top = toplevel(at, repo)?;
    let name = top.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut root = Module { name, children: Some(vec![]), ..Default::default() };
    if !has_rev(at, &top, "HEAD") {
        return Ok(root);
    }
    // `git grep -c ''` counts lines of every text file at HEAD in one go: "HEAD:path\0count".
    let mut loc = HashMap::new();
    for line in String::from_utf8_lossy(&run(at, &top, &["grep", "-I", "-z", "-c", "", "HEAD", "--"], &[1])?).lines() {
        if let Some((path, n)) = line.split_once('\0') {
            loc.insert(path.trim_start_matches("HEAD:").to_string(), n.parse().unwrap_or(0));
        }
    }
    for path in split0(&run(at, &top, &["ls-tree", "-r", "--name-only", "-z", "HEAD"], &[])?) {
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

fn blob(at: &Runner, dir: &Path, rev: &str, path: &str) -> Result<Vec<u8>> {
    run(at, dir, &["cat-file", "blob", &format!("{rev}:{path}")], &[])
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
    let (before, after) = (text(before)?, text(after)?);
    let (add, del) = counts(before.as_deref().unwrap_or_default(), after.as_deref().unwrap_or_default());
    Some(FileChange { add, del, before, after, path, status })
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
