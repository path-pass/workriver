// What is kept between snapshots: commits, trees, what was read of other machines, and the
// part kept across restarts.
use super::*;

/// Commits never change and a tree only with its HEAD, so both are kept between snapshots.
#[derive(Default)]
pub(in crate::server) struct Cache {
    /// Commits with their files' texts, by hash: they never change.
    // ponytail: unbounded; a commit's texts are kept for the life of the service.
    pub(super) commits: HashMap<String, scan::Commit>,
    /// Whether a merge brought the base in, by "merge|where the branch meets its base": it
    /// depends on the base, so never kept with the commit.
    pub(super) brings: HashMap<String, bool>,
    /// A unit's net change with its texts, by "from..tip" (`code::net`).
    // ponytail: unbounded, as `commits`.
    pub(super) nets: HashMap<String, Vec<scan::FileChange>>,
    /// Where a merged unit met its base before it merged, when nothing recorded it: by unit,
    /// with the tip it was worked out for.
    pub(super) before: HashMap<String, (String, Option<String>)>,
    pub(super) trees: HashMap<String, (String, scan::Module)>,
    /// The machines followed, for repositories that are on one of them.
    pub(super) machines: Option<std::sync::Arc<crate::runtime::follow::Machines>>,
    /// What is kept across restarts (in `file`): where branches fork, which commits each
    /// unit had for which tips, what commits say, patch ids.
    pub(super) stored: Stored,
    file: Option<std::path::PathBuf>,
    pub(super) dirty: bool,
    /// The board's initial build: Git data has not been read yet.
    pub(super) records_only: bool,
    /// This build: no file texts, uncommitted work or trees (board, `task show`).
    pub(super) light: bool,
    /// This build: files without their texts, only how many lines changed (the page's
    /// snapshot; `code` gives a unit's texts).
    pub(super) bare: bool,
    /// Whether this cache reads repositories on other machines (the background one does);
    /// one that does not takes what the background read last, from `shared`.
    pub(in crate::server) remote_reads: bool,
    pub(super) shared: std::sync::Arc<std::sync::Mutex<Shared>>,
    /// Each repository's refs and work trees, read once per build.
    pub(super) refs: HashMap<String, HashMap<String, String>>,
    pub(super) worktrees: HashMap<String, HashMap<String, std::path::PathBuf>>,
    /// This build: the files each open code unit changes (`changed_paths`, `open_work`), by unit.
    pub(super) changed: HashMap<String, BTreeSet<String>>,
    /// Units found merged in git and not yet recorded so: (unit, its commits).
    pub(in crate::server) merged_now: Vec<(String, Vec<String>)>,
    /// How long a unit may be ready before it is overdue (the page shows it); none: never.
    pub(super) ready_after: Option<std::time::Duration>,
    /// This build: units whose commits were worked out again, and those reused.
    pub(in crate::server) recomputed: usize,
    pub(in crate::server) reused: usize,
    /// Each local unit's uncommitted work as last read (`code::live_work`), by unit.
    pub(super) live: HashMap<String, Live>,
    /// What the watcher saw change since a build took it, shared with it.
    pub(in crate::server) marks: std::sync::Arc<std::sync::Mutex<Marks>>,
    /// The watcher, shared with the service: a work tree a read finds is registered there.
    pub(in crate::server) watcher: Watcher,
    /// The units whose uncommitted texts are held (those whose code was read last, the last
    /// first; `TEXT_UNITS`): memory only, the board never reads texts.
    pub(super) texted: Vec<String>,
    /// This build: the units whose uncommitted work was read again.
    pub(in crate::server) read_live: Vec<String>,
}

/// The watcher of this machine's repositories (none: it could not start).
pub(in crate::server) type Watcher = std::sync::Arc<std::sync::Mutex<Option<notify_debouncer_mini::Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>>>>;

/// A local unit's uncommitted work as last read: where, what was found (no texts: those are
/// in `known` alone, for the units in `Cache::texted`), the files as read (`scan::Known`),
/// whether the watcher followed its work tree when it was read, whether it saw a change there
/// since, and the paths it named.
#[derive(Default)]
pub(super) struct Live {
    pub(super) dir: std::path::PathBuf,
    /// `dir` as the file system names it (symlinks resolved), its own git folder (where its
    /// index and HEAD are) and its repository's: what the watcher's paths are matched against.
    pub(super) real: std::path::PathBuf,
    pub(super) git: std::path::PathBuf,
    pub(super) common: std::path::PathBuf,
    /// The branch checked out there: a move of its ref changes what the work is against.
    pub(super) branch: String,
    pub(super) files: Vec<scan::FileChange>,
    pub(super) known: scan::Known,
    pub(super) followed: bool,
    pub(super) stale: bool,
    /// Paths (in the work tree) the watcher named since: read again whatever their stat says.
    pub(super) touched: BTreeSet<String>,
}

/// What the watcher of this machine's repositories saw change, for the snapshots: the paths
/// it found changed (not ignored), and the folders it follows. A unit whose work tree it does
/// not follow (none yet, or it does not run) is read again in every build, as is one never
/// read, and one read before it was followed.
#[derive(Default)]
pub(in crate::server) struct Marks {
    pub(in crate::server) on: bool,
    pub(in crate::server) paths: std::collections::HashSet<std::path::PathBuf>,
    /// The watcher failed to say what changed: every unit's is read again.
    pub(in crate::server) all: bool,
    /// The folders registered (as the file system names them), and the inode each had then: a
    /// folder made again (a work tree removed and added back) is registered again.
    pub(in crate::server) watched: std::collections::HashSet<std::path::PathBuf>,
    pub(in crate::server) inodes: HashMap<std::path::PathBuf, u64>,
    /// When registering a folder last failed: tried again after `RETRY`, not on every read.
    pub(in crate::server) failed: HashMap<std::path::PathBuf, std::time::Instant>,
}

/// How long a folder the watcher could not register waits before it is tried again.
const RETRY: std::time::Duration = std::time::Duration::from_secs(60);

/// A folder as the file system names it: symlinks resolved, all of it.
pub(in crate::server) fn real_dir(p: &Path) -> std::path::PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn inode(p: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(p).ok().map(|m| m.ino())
}

impl Marks {
    /// Whether the watcher follows `real` (a folder as the file system names it): inside a
    /// folder registered, that folder still the one registered.
    pub(in crate::server) fn covers(&self, real: &Path) -> bool {
        self.on && self.watched.iter().any(|w| real.starts_with(w) && self.inodes.get(w).is_none_or(|i| inode(w) == Some(*i)))
    }
}

/// Registers `path` (a folder) with the watcher, unless it already follows it; whether it
/// does now. The one way a folder is followed: the service's own list (`App::watch`) and the
/// reads that find a work tree (`code::live_work`) both come here. A failure is tried again
/// after a while.
pub(in crate::server) fn register(watcher: &Watcher, marks: &std::sync::Mutex<Marks>, path: &Path) -> bool {
    use notify_debouncer_mini::notify::RecursiveMode;
    let real = real_dir(path);
    {
        let m = marks.lock().unwrap();
        if !m.on {
            return false;
        }
        if m.covers(&real) {
            return true;
        }
        if m.failed.get(&real).is_some_and(|at| at.elapsed() < RETRY) {
            return false;
        }
    }
    let mut w = watcher.lock().unwrap();
    let Some(w) = w.as_mut() else { return false };
    let done = w.watcher().watch(&real, RecursiveMode::Recursive);
    let mut m = marks.lock().unwrap();
    match done {
        Ok(()) => {
            m.failed.remove(&real);
            if let Some(i) = inode(&real) {
                m.inodes.insert(real.clone(), i);
            }
            m.watched.insert(real);
            true
        }
        Err(e) => {
            tracing::warn!("cannot watch {}: {e}", real.display());
            m.failed.insert(real, std::time::Instant::now());
            false
        }
    }
}

/// The rules by which a unit's commits are worked out (`scan::branch`). Bump it whenever the
/// scan rules change what a unit's commits are: a cache kept under other rules is dropped on
/// load (one rescan), else its old lists and forks would be served on, their keys unchanged.
/// 1: the base's upstream is left out too.
pub(super) const RULES: u32 = 1;

/// What a cache keeps across restarts.
#[derive(serde::Serialize, serde::Deserialize, Default)]
pub(super) struct Stored {
    /// `RULES` when it was written; 0 before there was one.
    #[serde(default)]
    rules: u32,
    #[serde(default)]
    pub(super) seen: HashMap<String, Seen>,
    #[serde(default)]
    pub(super) memo: HashMap<String, Memo>,
    #[serde(default)]
    pub(super) meta: HashMap<String, scan::Named>,
    #[serde(default)]
    pub(super) patch: HashMap<String, String>,
    /// Whether each ready unit looked squash-merged, under the key of its commits (`Memo`).
    #[serde(default)]
    pub(super) squash: HashMap<String, (String, Option<Squash>)>,
}

/// A unit's commits for one state of its branch and base (`key`).
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(super) struct Memo {
    pub(super) key: String,
    pub(super) hashes: Vec<String>,
    pub(super) merged: bool,
    /// Where the branch met its base (`Seen::base_at`) for this key.
    #[serde(default)]
    pub(super) base_at: Option<String>,
    /// `base_at` was worked out (a memo from before it was is worked out again).
    #[serde(default)]
    pub(super) based: bool,
}

/// A unit's net change, as `git diff from..tip` names it: "from..tip", and its files.
pub(super) type NetRead = (String, Vec<scan::FileChange>);

/// A unit on another machine as last read: its commits, its uncommitted work, the patch ids
/// of its commits and of those of branches it left, and its net change.
type RemoteUnit = (Vec<scan::Commit>, Vec<scan::FileChange>, HashMap<String, String>, Option<NetRead>);

/// What the background read of other machines found last, for requests to show.
#[derive(Default)]
pub(in crate::server) struct Shared {
    pub(super) units: HashMap<String, RemoteUnit>,
    pub(super) trees: HashMap<String, scan::Module>,
    /// Ready units, on any machine, that looked squash-merged when last read.
    pub(in crate::server) squash: HashMap<String, Squash>,
}

/// A unit's branch as last read.
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
pub(super) struct Seen {
    /// The repository, branch and base it was read at; another place starts over.
    pub(super) place: String,
    pub(super) fork: Option<String>,
    pub(super) commits: Vec<scan::Named>,
    /// Where the branch last met its base while it was not merged (`scan::merge_base`): kept,
    /// so a merged unit's history reads as it did then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) base_at: Option<String>,
}

/// How often a repository on another machine is read again.
// ponytail: polled, since files there cannot be watched; a watcher on the machine could tell.
pub(in crate::server) const REMOTE: std::time::Duration = std::time::Duration::from_secs(15);

impl Cache {
    pub(in crate::server) fn following(machines: Option<std::sync::Arc<crate::runtime::follow::Machines>>) -> Cache {
        Cache { machines, ..Default::default() }
    }

    /// What the background read of other machines found goes through `shared`.
    pub(in crate::server) fn sharing(mut self, shared: std::sync::Arc<std::sync::Mutex<Shared>>) -> Cache {
        self.shared = shared;
        self
    }

    /// Keeps what it learns in `file`, and starts from what it holds.
    pub(in crate::server) fn remembering(mut self, file: std::path::PathBuf) -> Cache {
        let stored: Option<Stored> = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok());
        self.stored = stored.filter(|s| s.rules == RULES).unwrap_or_default();
        self.stored.rules = RULES;
        self.file = Some(file);
        self
    }

    /// Writes what was learnt since the last time.
    pub(in crate::server) fn save(&mut self) {
        let Some(file) = self.file.as_ref().filter(|_| self.dirty) else { return };
        let written = file.parent().map(std::fs::create_dir_all).unwrap_or(Ok(())).and_then(|_| serde_json::to_vec(&self.stored).map_err(std::io::Error::other)).and_then(|b| {
            let tmp = file.with_extension("tmp");
            std::fs::write(&tmp, b).and_then(|_| std::fs::rename(&tmp, file))
        });
        match written {
            Ok(()) => self.dirty = false,
            Err(e) => tracing::warn!("cannot write {}: {e}", file.display()),
        }
    }

    /// Where git runs for a repository: None while its machine cannot be reached.
    pub(super) fn runner(&self, machine: Option<&str>) -> Option<scan::Runner> {
        match machine {
            None => Some(scan::Runner::Local),
            Some(m) => self.machines.as_ref().and_then(|x| x.runner(m)),
        }
    }

    /// Takes what the watcher saw change. A path in a unit's work tree (the deepest, where one
    /// is inside another) marks that unit, and the path to be read again; a folder holding work
    /// trees, as a coarse event names it, marks those. In a git folder: a work tree's own
    /// index or HEAD (a `git add`, a checkout) marks that work tree; a branch's ref, the units
    /// on that branch; another ref, none (no work tree has it checked out); anything else
    /// there (packed-refs, what cannot be told), every unit of that repository. A path in no
    /// unit's (the base's checkout, a unit not read yet) marks none.
    pub(super) fn take_marks(&mut self) {
        let (paths, all) = {
            let mut m = self.marks.lock().unwrap();
            (std::mem::take(&mut m.paths), std::mem::take(&mut m.all))
        };
        for p in paths {
            if let Some(l) = self.live.values_mut().find(|l| p == l.git.join("index") || p == l.git.join("HEAD")) {
                l.stale = true;
                continue;
            }
            if let Some(common) = self.live.values().map(|l| l.common.clone()).find(|c| p.starts_with(c)) {
                let rel = p.strip_prefix(&common).unwrap_or(&p).to_string_lossy().into_owned();
                let own = self.live.values().filter(|l| l.git != common && p.starts_with(&l.git)).map(|l| l.git.clone()).next();
                for l in self.live.values_mut().filter(|l| l.common == common) {
                    l.stale |= match (rel.strip_prefix("refs/heads/"), rel.starts_with("refs/"), &own) {
                        (Some(branch), ..) => l.branch == branch,
                        (None, true, _) => false,
                        (None, false, Some(git)) => l.git == *git,
                        (None, false, None) => true,
                    };
                }
                continue;
            }
            if let Some(l) = self.live.values_mut().filter(|l| p.starts_with(&l.real)).max_by_key(|l| l.real.as_os_str().len()) {
                l.stale = true;
                if let Ok(rel) = p.strip_prefix(&l.real) {
                    l.touched.insert(rel.to_string_lossy().into_owned());
                }
            }
            for l in self.live.values_mut().filter(|l| l.real.starts_with(&p) && l.real != p) {
                l.stale = true;
            }
        }
        if all {
            for l in self.live.values_mut() {
                l.stale = true;
            }
        }
    }

    /// Whether the watcher follows the work tree `real`, registering it there if not.
    pub(super) fn follow(&self, real: &Path) -> bool {
        register(&self.watcher, &self.marks, real)
    }

    /// Unit `id`'s texts were read now: held, the oldest beyond `TEXT_UNITS` let go of.
    pub(super) fn hold_texts(&mut self, id: &str) {
        self.texted.retain(|x| x != id);
        self.texted.insert(0, id.to_string());
        for old in self.texted.split_off(self.texted.len().min(crate::common::config::defaults::TEXT_UNITS)) {
            self.release(&old);
        }
    }

    /// Lets go of unit `id`'s uncommitted texts (names, entries and counts stay): it fell out
    /// of `texted`, or it is closed.
    pub(in crate::server) fn release(&mut self, id: &str) {
        self.texted.retain(|x| x != id);
        if let Some(l) = self.live.get_mut(id) {
            l.known.drop_texts();
        }
    }

    /// A repository's refs, read once per build.
    pub(super) fn refs(&mut self, at: &scan::Runner, u: &core::Unit) -> Option<HashMap<String, String>> {
        let key = format!("{}\0{}", u.machine.as_deref().unwrap_or_default(), u.repo);
        if !self.refs.contains_key(&key) {
            let r = scan::refs(at, Path::new(&u.repo)).ok()?;
            self.refs.insert(key.clone(), r);
        }
        self.refs.get(&key).cloned()
    }
}
