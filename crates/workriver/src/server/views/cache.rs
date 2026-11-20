// What is kept between snapshots: commits, trees, what was read of other machines, and the
// part kept across restarts.
use super::*;

/// Commits never change and a tree only with its HEAD, so both are kept between snapshots.
#[derive(Default)]
pub(in crate::server) struct Cache {
    /// Commits with their files' texts, by hash: they never change.
    // ponytail: unbounded; a commit's texts are kept for the life of the service.
    pub(super) commits: HashMap<String, scan::Commit>,
    pub(super) trees: HashMap<String, (String, scan::Module)>,
    /// The machines followed, for repositories that are on one of them.
    pub(super) machines: Option<std::sync::Arc<crate::runtime::follow::Machines>>,
    /// What is kept across restarts (in `file`): where branches fork, which commits each
    /// unit had for which tips, what commits say, patch ids.
    pub(super) stored: Stored,
    file: Option<std::path::PathBuf>,
    pub(super) dirty: bool,
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
    /// Units found merged in git and not yet recorded so: (unit, its commits).
    pub(in crate::server) merged_now: Vec<(String, Vec<String>)>,
    /// How long a unit may be ready before it is overdue (the page shows it); none: never.
    pub(super) ready_after: Option<std::time::Duration>,
    /// This build: units whose commits were worked out again, and those reused.
    pub(in crate::server) recomputed: usize,
    pub(in crate::server) reused: usize,
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
}

/// A unit on another machine as last read: its commits, its uncommitted work, the patch ids
/// of its commits and of those of branches it left.
type RemoteUnit = (Vec<scan::Commit>, Vec<scan::FileChange>, HashMap<String, String>);

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
