use super::*;

/// A cache kept under other rules (or before rules were versioned) is not reused.
#[test]
fn a_cache_of_other_rules_is_dropped() {
    use serde_json::json;
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("local.json");
    let memo = json!({ "u1": { "key": "k", "hashes": ["abc"], "merged": false } });
    for old in [json!({ "memo": memo }), json!({ "rules": RULES - 1, "memo": memo })] {
        std::fs::write(&file, old.to_string()).unwrap();
        assert!(Cache::default().remembering(file.clone()).stored.memo.is_empty(), "{old}");
    }
    std::fs::write(&file, json!({ "rules": RULES, "memo": memo }).to_string()).unwrap();
    let mut c = Cache::default().remembering(file.clone());
    assert_eq!(c.stored.memo.len(), 1, "kept under these rules");
    c.dirty = true;
    c.save();
    assert_eq!(Cache::default().remembering(file).stored.memo.len(), 1, "and saved under them");
}

/// The code token changes when only an uncommitted file's git entry does (its text the
/// same): a mode changed in the work tree reaches the page, not its cached entry.
#[test]
fn the_code_token_follows_uncommitted_entries() {
    let file = scan::FileChange {
        path: "run.sh".into(),
        status: "M".into(),
        add: 1,
        del: 1,
        before: Some("new\n".into()),
        after: Some("old\n".into()),
        before_mode: "100644".into(),
        before_id: "3e757656cf36eca53338e520d134963a44f793f8".into(),
        after_mode: "100644".into(),
        after_id: "0".repeat(40),
        before_binary: false,
        after_binary: false,
        uncounted: false,
        stamp: None,
    };
    let token = |f: scan::FileChange| code_of(&[], &[f], None);
    let was = token(file.clone());
    for changed in [
        scan::FileChange { after_mode: "100755".into(), ..file.clone() },
        scan::FileChange { before_mode: "100755".into(), ..file.clone() },
        scan::FileChange { after_id: "1".repeat(40), ..file.clone() },
        scan::FileChange { before_id: "1".repeat(40), ..file.clone() },
    ] {
        assert_ne!(token(changed.clone()), was, "{changed:?}");
    }
}

/// The files a unit changes are what its code tree shows with nothing picked (the page's
/// `wholeDelta`): its net change with uncommitted work on top, each file's entry at the start
/// against its entry at the end; uncommitted work's side, which git has not hashed, by its
/// text. Without a net change, its commits taken together (the page's `fileDelta`).
#[test]
fn a_units_changed_files_are_its_net_change() {
    const GONE: &str = "000000";
    let (one, two) = ("5626abf0f72e58d7a153368ba57db4c673c0e171", "814f4a422927b82f5f8a43f8fab6d3839e3983f2");
    let zero = "0".repeat(40);
    let f = |path: &str, before: (&str, &str), after: (&str, &str), texts: (Option<&str>, Option<&str>)| scan::FileChange {
        path: path.into(),
        status: "M".into(),
        add: 0,
        del: 0,
        before: texts.0.map(Into::into),
        after: texts.1.map(Into::into),
        before_binary: false,
        after_binary: false,
        before_mode: before.0.into(),
        before_id: before.1.into(),
        after_mode: after.0.into(),
        after_id: after.1.into(),
        uncounted: false,
        stamp: None,
    };
    let commit = |files: Vec<scan::FileChange>| scan::Commit { id: "c".into(), hash: "c".into(), subject: String::new(), date: String::new(), author: String::new(), task: None, merge: false, parents: vec![], base_merge: false, files };
    // A merge of the base brought base.txt in: in the commits, not in the net change.
    let commits = [
        commit(vec![f("kept.txt", ("100644", one), ("100644", two), (Some("one\n"), Some("one\ntwo\n"))), f("back.txt", ("100644", one), ("100644", two), (Some("caf\u{fffd}\n"), Some("two\n")))]),
        commit(vec![f("base.txt", (GONE, &zero), ("100644", one), (None, Some("one\n")))]),
    ];
    let net = [
        f("kept.txt", ("100644", one), ("100644", two), (Some("one\n"), Some("one\ntwo\n"))),
        f("back.txt", ("100644", one), ("100644", two), (Some("caf\u{fffd}\n"), Some("two\n"))),
        f("mode.sh", ("100644", one), ("100755", one), (Some("one\n"), Some("one\n"))),
    ];
    let live = [
        // Changed back to the base's text (not UTF-8: read lossily on both sides alike).
        f("back.txt", ("100644", two), ("100644", &zero), (Some("two\n"), Some("caf\u{fffd}\n"))),
        // Changed in the work tree alone; and changed there and back.
        f("new.txt", (GONE, &zero), ("100644", &zero), (None, Some("one\n"))),
        f("same.txt", ("100644", one), ("100644", &zero), (Some("one\n"), Some("one\n"))),
        // A file of the net change deleted in the work tree.
        f("mode.sh", ("100755", one), (GONE, &zero), (Some("one\n"), None)),
    ];
    let refs: Vec<&scan::Commit> = commits.iter().collect();
    let got = |live: &[scan::FileChange], net: Option<&[scan::FileChange]>| changed_paths(&refs, live, net).into_iter().collect::<Vec<_>>();
    assert_eq!(got(&live, Some(&net)), ["kept.txt", "mode.sh", "new.txt"]);
    assert_eq!(got(&[], Some(&net)), ["back.txt", "kept.txt", "mode.sh"]);
    assert!(got(&[], Some(&[])).is_empty(), "an empty net change is the unit's, whatever its commits hold");
    // No net change: the commits (and uncommitted work) taken together, base.txt with them.
    assert_eq!(got(&live, None), ["base.txt", "kept.txt", "mode.sh", "new.txt"]);
    // A file the work added and deleted again is not there at either end.
    let gone = [commit(vec![f("tmp.txt", (GONE, &zero), ("100644", one), (None, Some("one\n")))]), commit(vec![f("tmp.txt", ("100644", one), (GONE, &zero), (Some("one\n"), None))])];
    assert!(changed_paths(&gone.iter().collect::<Vec<_>>(), &[], None).is_empty());
}

/// A unit `u1` working on `feature` from `main` in `repo`.
fn unit_in(repo: &std::path::Path) -> core::Unit {
    core::Unit {
        id: "u1".into(), intent: "i1".into(), title: String::new(), purpose: String::new(),
        repo: repo.to_str().unwrap().into(), branch: "feature".into(), base: "main".into(),
        at: String::new(), merged: None, machine: None, merged_commits: vec![], tasks: vec![],
        earlier: vec![], links: vec![], done: None, kind: core::UnitKind::Code, template: String::new(), revision: None,
    }
}

/// git in `repo`, failing the test when it does.
fn git_in(repo: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git").arg("-C").arg(repo).args(["-c", "user.name=Test", "-c", "user.email=test@example.invalid"]).args(args).output().unwrap();
    assert!(output.status.success(), "{args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

/// feature (one commit) with a side branch from main merged into it.
fn side_merged(repo: &std::path::Path) {
    let add = |name: &str, trailer: &str| {
        std::fs::write(repo.join(name), format!("{name}\n")).unwrap();
        git_in(repo, &["add", name]);
        git_in(repo, &["commit", "-qm", name, "-m", trailer]);
    };
    git_in(repo, &["init", "-qb", "main"]);
    add("root.txt", "Base");
    git_in(repo, &["switch", "-qc", "feature"]);
    add("feature.txt", "Task: u1#1");
    git_in(repo, &["switch", "-qc", "side", "main"]);
    add("side.txt", "Side");
    git_in(repo, &["switch", "-q", "feature"]);
    git_in(repo, &["merge", "-q", "--no-ff", "-m", "Bring in side", "-m", "Task: u1#2", "side"]);
}

/// Each commit's hash, whether it brought the base in, and its files; the net change's files.
type Shown = (Vec<(String, bool, Vec<String>)>, Vec<String>);
fn seen(read: &(Vec<scan::Commit>, Vec<scan::FileChange>, Option<NetRead>)) -> Shown {
    let commits = read.0.iter().map(|c| (c.hash.clone(), c.base_merge, c.files.iter().map(|f| f.path.clone()).collect())).collect();
    let mut net: Vec<String> = read.2.iter().flat_map(|(_, files)| files.iter().map(|f| f.path.clone())).collect();
    net.sort();
    (commits, net)
}

/// The same refs read with a warm cache and a cold one give the same code, and the base
/// moving on (the branch's commits the same) changes the code's token: what a commit holds
/// is read against its first parent, which no base changes; whether a merge brought the
/// base in and the net change are worked out against where the branch meets its base now.
#[test]
fn the_code_follows_the_base_warm_or_cold() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();
    side_merged(repo);
    let u = unit_in(repo);
    let mut cache = Cache::default();
    let first = super::code::read_unit(&scan::Runner::Local, &u, &mut cache, false);
    assert_eq!(seen(&first).1, ["feature.txt", "side.txt"], "side's work is the branch's");
    assert!(!first.0.last().unwrap().base_merge);
    git_in(repo, &["switch", "-q", "main"]);
    git_in(repo, &["merge", "-q", "--ff-only", "side"]);
    assert_eq!(git_in(repo, &["diff", "--no-renames", "--name-only", "main...feature"]), "feature.txt\n");
    cache.refs.clear();
    cache.worktrees.clear();
    let warm = super::code::read_unit(&scan::Runner::Local, &u, &mut cache, false);
    let cold = super::code::read_unit(&scan::Runner::Local, &u, &mut Cache::default(), false);
    assert_eq!(seen(&warm), seen(&cold), "warm and cold reads of the same refs agree");
    assert_eq!(seen(&warm).1, ["feature.txt"], "the net change is git diff main...feature");
    assert!(warm.0.last().unwrap().base_merge, "side is the base now");
    assert_eq!(seen(&warm).0.last().unwrap().2, ["side.txt"], "the merge's own files, against its first parent, as before");
    let token = |r: &(Vec<scan::Commit>, Vec<scan::FileChange>, Option<NetRead>)| code_of(&r.0, &r.1, r.2.as_ref());
    assert_eq!(token(&warm), token(&cold));
    assert_ne!(token(&first), token(&warm), "the page reads the code again");
}

/// A merged unit, read again from nothing (a restart), keeps what its commits were while
/// it worked: a side branch it merged stays its own work, though the base has it now, found
/// merged in git or recorded so.
#[test]
fn a_merged_unit_keeps_its_history_read_cold() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();
    side_merged(repo);
    let mut u = unit_in(repo);
    let active = super::code::read_unit(&scan::Runner::Local, &u, &mut Cache::default(), false);
    assert!(!active.0.last().unwrap().base_merge);
    git_in(repo, &["switch", "-q", "main"]);
    git_in(repo, &["merge", "-q", "--no-ff", "-m", "Merge the unit", "feature"]);
    let found = super::code::read_unit(&scan::Runner::Local, &u, &mut Cache::default(), false);
    assert_eq!(seen(&found), seen(&active), "found merged in git");
    u.merged = Some("2026-11-21T00:00:00Z".into());
    u.merged_commits = active.0.iter().map(|c| c.hash.clone()).collect();
    let recorded = super::code::read_unit(&scan::Runner::Local, &u, &mut Cache::default(), false);
    assert_eq!(seen(&recorded), seen(&active), "recorded merged");
}

/// A file named `path` holding its name, committed with `trailer`.
fn add_file(repo: &std::path::Path, path: &str, trailer: &str) {
    std::fs::write(repo.join(path), format!("{path}\n")).unwrap();
    git_in(repo, &["add", path]);
    git_in(repo, &["commit", "-qm", path, "-m", trailer]);
}

/// `u` read again, as a new snapshot would (refs and work trees read anew).
fn read_again(repo: &std::path::Path, u: &core::Unit, cache: &mut Cache) -> (Vec<scan::Commit>, Vec<scan::FileChange>, Option<NetRead>) {
    cache.refs.clear();
    cache.worktrees.clear();
    assert_eq!(repo.to_str().unwrap(), &u.repo);
    super::code::read_unit(&scan::Runner::Local, u, cache, false)
}

/// The base merged in and the unit merged into it with no read in between: the merged
/// unit reads from where it met its base just before it merged, warm or cold, found merged
/// in git or recorded so; the last sync is a base merge, never the unit's work.
#[test]
fn a_final_sync_unseen_before_the_merge_is_not_the_units() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    side_merged(repo);
    let mut u = unit_in(repo);
    let mut cache = Cache::default();
    let old = read_again(repo, &u, &mut cache);
    assert_eq!(seen(&old).1, ["feature.txt", "side.txt"]);

    git_in(repo, &["switch", "-q", "main"]);
    add_file(repo, "base-only.txt", "Base");
    git_in(repo, &["switch", "-q", "feature"]);
    git_in(repo, &["merge", "-q", "--no-ff", "-m", "Final sync", "-m", "Task: u1#3", "main"]);
    assert_eq!(git_in(repo, &["diff", "--no-renames", "--name-only", "main...feature"]), "feature.txt\nside.txt\n");
    git_in(repo, &["switch", "-q", "main"]);
    git_in(repo, &["merge", "-q", "--no-ff", "-m", "Integrate final tip", "feature"]);

    let cold = read_again(repo, &u, &mut Cache::default());
    let warm = read_again(repo, &u, &mut cache);
    assert_eq!(seen(&cold).1, ["feature.txt", "side.txt"]);
    assert!(cold.0.last().unwrap().base_merge);
    assert_eq!(seen(&warm).0.last().unwrap().2, ["base-only.txt"]);
    u.merged = Some("2026-11-21T00:00:00Z".into());
    u.merged_commits = cold.0.iter().map(|c| c.hash.clone()).collect();
    let recorded = read_again(repo, &u, &mut cache);
    assert_eq!(seen(&recorded), seen(&cold));
}

/// The base's upstream changed (here: unset), no branch moved: a warm read follows, as a
/// cold one and git do.
#[test]
fn a_change_of_the_bases_upstream_reads_the_code_again() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    side_merged(repo);
    git_in(repo, &["branch", "-q", "--set-upstream-to=side", "main"]);
    let u = unit_in(repo);
    let mut cache = Cache::default();
    let initial = read_again(repo, &u, &mut cache);
    assert_eq!(seen(&initial).1, ["feature.txt"]);
    assert!(initial.0.last().unwrap().base_merge);

    git_in(repo, &["branch", "-q", "--unset-upstream", "main"]);
    assert_eq!(git_in(repo, &["diff", "--no-renames", "--name-only", "main...feature"]), "feature.txt\nside.txt\n");
    let warm = read_again(repo, &u, &mut cache);
    let cold = read_again(repo, &u, &mut Cache::default());
    assert_eq!(seen(&cold).1, ["feature.txt", "side.txt"]);
    assert!(!cold.0.last().unwrap().base_merge);
    assert_eq!(seen(&warm), seen(&cold));
}

/// A unit merged only into the base's upstream (the local base behind): it reads as before
/// the merge there, found merged in git or recorded so.
#[test]
fn a_unit_merged_into_the_upstream_only_reads_as_before() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    side_merged(repo);
    let mut u = unit_in(repo);
    git_in(repo, &["switch", "-qc", "up", "main"]);
    git_in(repo, &["branch", "-q", "--set-upstream-to=up", "main"]);
    add_file(repo, "base-only.txt", "Upstream work");
    git_in(repo, &["switch", "-q", "feature"]);
    git_in(repo, &["merge", "-q", "--no-ff", "-m", "Sync upstream", "-m", "Task: u1#3", "up"]);
    let expected = read_again(repo, &u, &mut Cache::default());
    assert_eq!(git_in(repo, &["diff", "--no-renames", "--name-only", "up...feature"]), "feature.txt\nside.txt\n");
    assert_eq!(seen(&expected).1, ["feature.txt", "side.txt"]);
    assert!(expected.0.last().unwrap().base_merge);
    git_in(repo, &["switch", "-q", "up"]);
    git_in(repo, &["merge", "-q", "--no-ff", "-m", "Integrate feature only in upstream", "feature"]);
    let mut cold = Cache::default();
    let found = read_again(repo, &u, &mut cold);
    assert_eq!(cold.merged_now.len(), 1);
    u.merged = Some("2026-11-21T00:00:00Z".into());
    u.merged_commits = expected.0.iter().map(|c| c.hash.clone()).collect();
    let recorded = read_again(repo, &u, &mut Cache::default());
    assert_eq!(seen(&found), seen(&expected), "found merged in git");
    assert_eq!(seen(&recorded), seen(&expected), "recorded merged");
}

/// A repository with two units' work trees (branches `a` and `b`, beside it), each with an
/// uncommitted file, a `.gitignore` naming `build/`; the units, and a cache whose watcher
/// follows what `App::watch` follows: the repository, its git folder and the work trees.
fn two_units(root: &std::path::Path) -> (Vec<core::Unit>, Cache) {
    let repo = root.join("repo");
    std::fs::create_dir(&repo).unwrap();
    git_in(&repo, &["init", "-qb", "main"]);
    std::fs::write(repo.join(".gitignore"), "build/\n").unwrap();
    for f in ["one.txt", "two.txt"] {
        std::fs::write(repo.join(f), "one\n").unwrap();
    }
    git_in(&repo, &["add", "."]);
    git_in(&repo, &["commit", "-qm", "base"]);
    let units = ["a", "b"]
        .iter()
        .enumerate()
        .map(|(k, b)| {
            git_in(&repo, &["worktree", "add", "-q", "-b", b, root.join(format!("wt-{b}")).to_str().unwrap()]);
            std::fs::write(root.join(format!("wt-{b}/one.txt")), "one\ntwo\n").unwrap();
            core::Unit { id: format!("u{}", k + 1), branch: b.to_string(), ..unit_in(&repo) }
        })
        .collect();
    let cache = Cache::default();
    {
        let mut m = cache.marks.lock().unwrap();
        m.on = true;
        m.watched.extend([root.join("repo"), root.join("repo/.git"), root.join("wt-a"), root.join("wt-b")]);
    }
    (units, cache)
}

/// Every unit read as a build reads them; what each found.
fn read_all(units: &[core::Unit], cache: &mut Cache) -> Vec<Vec<scan::FileChange>> {
    cache.refs.clear();
    cache.worktrees.clear();
    cache.read_live.clear();
    cache.take_marks();
    units.iter().map(|u| super::code::read_unit(&scan::Runner::Local, u, cache, false).1).collect()
}

/// The watcher saw `paths` change; whether the page is told.
fn saw(cache: &Cache, paths: &[std::path::PathBuf]) -> bool {
    crate::server::app::noticed(&cache.marks, paths.iter().map(|p| p.as_path()))
}

/// A unit's uncommitted work is read again only when the watcher saw its work tree (or its
/// repository's git folder) change, not for a path git ignores; then only its changed files
/// are read. A unit whose work tree is not followed, and every unit after a restart, is read.
#[test]
fn only_the_unit_whose_work_tree_changed_is_read_again() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (units, mut cache) = two_units(&root);
    let first = read_all(&units, &mut cache);
    assert_eq!(cache.read_live, ["u1", "u2"], "never read: read");
    let reads = |cache: &Cache, id: &str| cache.live[id].known.reads;
    assert_eq!((reads(&cache, "u1"), reads(&cache, "u2")), (1, 1));
    assert_eq!(read_all(&units, &mut cache), first);
    assert!(cache.read_live.is_empty(), "nothing changed: nothing read");

    // A file of u1's changes: u1 alone is read again, that file alone.
    std::fs::write(root.join("wt-a/two.txt"), "one\nthree\n").unwrap();
    assert!(saw(&cache, &[root.join("wt-a/two.txt")]));
    let now = read_all(&units, &mut cache);
    assert_eq!(cache.read_live, ["u1"]);
    assert_eq!((reads(&cache, "u1"), reads(&cache, "u2")), (2, 1));
    assert_eq!(now[0].iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["one.txt", "two.txt"]);
    assert_eq!(now[1], first[1]);

    // What git ignores marks nothing, and the page is not told.
    std::fs::create_dir_all(root.join("wt-b/build")).unwrap();
    std::fs::write(root.join("wt-b/build/out.o"), "x").unwrap();
    assert!(!saw(&cache, &[root.join("wt-b/build/out.o")]));
    assert!(cache.marks.lock().unwrap().paths.is_empty());
    read_all(&units, &mut cache);
    assert!(cache.read_live.is_empty());

    // A commit in b's work tree: its branch's ref (and its index) moved; u2 alone is read
    // again. git's logs are no change.
    git_in(&root.join("wt-b"), &["commit", "-qam", "b"]);
    assert!(saw(&cache, &[root.join("repo/.git/refs/heads/b"), root.join("repo/.git/worktrees/wt-b/index"), root.join("repo/.git/logs/refs/heads/b")]));
    let after = read_all(&units, &mut cache);
    assert_eq!(cache.read_live, ["u2"]);
    assert!(after[1].is_empty(), "b's work is committed");
    // A branch no unit is on: none. packed-refs (what moved cannot be told): every unit there.
    assert!(saw(&cache, &[root.join("repo/.git/refs/heads/main")]));
    read_all(&units, &mut cache);
    assert!(cache.read_live.is_empty());
    assert!(saw(&cache, &[root.join("repo/.git/packed-refs")]));
    read_all(&units, &mut cache);
    assert_eq!(cache.read_live, ["u1", "u2"]);

    // Not followed (the watcher does not run): read every time.
    cache.marks.lock().unwrap().on = false;
    read_all(&units, &mut cache);
    assert_eq!(cache.read_live, ["u1", "u2"]);
    // A restart: nothing known, every unit read.
    let mut fresh = Cache::default();
    fresh.marks = cache.marks.clone();
    fresh.marks.lock().unwrap().on = true;
    assert_eq!(read_all(&units, &mut fresh), after);
    assert_eq!(fresh.read_live, ["u1", "u2"]);
}

/// The board reads uncommitted work for its names only (no file read; counts left out where
/// not known); a unit's code read reads its texts, and again only the files changed since.
/// The code token is the same either way, so the page reads the code only when files changed.
#[test]
fn the_board_names_files_and_the_code_read_reads_them() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (units, mut cache) = two_units(&root);
    cache.bare = true;
    let token = |files: &[scan::FileChange]| code_of(&[], files, None);
    let board = read_all(&units, &mut cache);
    assert_eq!((cache.live["u1"].known.reads, cache.live["u2"].known.reads), (0, 0));
    assert!(board.iter().flatten().all(|f| f.uncounted && f.after.is_none()));
    // Overlaps need names only: a file changed in the work tree, its text unread, is changed.
    assert_eq!(changed_paths(&[], &board[0], None).into_iter().collect::<Vec<_>>(), ["one.txt"]);

    let code = code_read(&units[0], &mut cache);
    assert_eq!(cache.read_live, ["u1"]);
    assert_eq!((code[0].add, code[0].uncounted, code[0].after.as_deref()), (1, false, Some("one\ntwo\n")));
    assert_eq!(token(&code), token(&board[0]), "the same files: the page reads no code again");
    assert_eq!(code_read(&units[0], &mut cache), code, "held: not read again");
    assert!(cache.read_live.is_empty());
    let again = read_all(&units, &mut cache);
    assert!(cache.read_live.is_empty(), "the board reads nothing again");
    assert!(again.iter().flatten().all(|f| f.after.is_none()), "and gives no texts");

    std::fs::write(root.join("wt-a/one.txt"), "one\ntwo\nthree\n").unwrap();
    assert!(saw(&cache, &[root.join("wt-a/one.txt")]));
    let changed = read_all(&units, &mut cache);
    assert_eq!(cache.read_live, ["u1"]);
    assert_ne!(token(&changed[0]), token(&board[0]), "its change reaches the page");
    let reads = cache.live["u1"].known.reads;
    let code = code_read(&units[0], &mut cache);
    assert_eq!((code[0].add, cache.live["u1"].known.reads), (2, reads + 1), "the changed file alone is read");
}

/// Unit `u`'s uncommitted work as its code read reads it (with texts).
fn code_read(u: &core::Unit, cache: &mut Cache) -> Vec<scan::FileChange> {
    let bare = std::mem::replace(&mut cache.bare, false);
    cache.refs.clear();
    cache.worktrees.clear();
    cache.read_live.clear();
    cache.take_marks();
    let files = super::code::read_unit(&scan::Runner::Local, u, cache, false).1;
    cache.bare = bare;
    files
}

/// Texts are held for the last few units whose code was read: one falling out of them lets
/// go of its texts (its names and counts stay, the board reads nothing again); a merged
/// unit's work goes altogether.
#[test]
fn texts_are_held_for_the_last_code_reads_only() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (mut units, mut cache) = two_units(&root);
    cache.bare = true;
    read_all(&units, &mut cache);
    code_read(&units[0], &mut cache);
    assert!(cache.live["u1"].known.has_texts());
    for k in 0..crate::common::config::defaults::TEXT_UNITS {
        cache.hold_texts(&format!("x{k}"));
    }
    assert!(!cache.live["u1"].known.has_texts(), "let go of");
    let board = read_all(&units, &mut cache);
    assert!(cache.read_live.is_empty());
    assert_eq!((board[0][0].add, board[0][0].uncounted), (1, false), "its counts stay");
    let reads = cache.live["u1"].known.reads;
    code_read(&units[0], &mut cache);
    assert_eq!(cache.live["u1"].known.reads, reads + 1, "read again with its texts");
    units[0].merged = Some("2026-11-27T00:00:00Z".into());
    read_all(&units, &mut cache);
    assert!(!cache.live.contains_key("u1") && !cache.texted.contains(&"u1".to_string()));
}

/// What git does in a work tree that changes its uncommitted work without touching a file
/// there git does not ignore: a `git add -f` of an ignored file (the work tree's own index)
/// marks that unit; a lock file of the work tree's own (Cargo.lock) is a file like any.
#[test]
fn an_index_change_and_a_lock_file_mark_their_unit() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (units, mut cache) = two_units(&root);
    let a = root.join("wt-a");
    std::fs::create_dir(a.join("build")).unwrap();
    std::fs::write(a.join("build/new.txt"), "generated\n").unwrap();
    read_all(&units, &mut cache);
    git_in(&a, &["add", "-f", "build/new.txt"]);
    let index = git_in(&a, &["rev-parse", "--path-format=absolute", "--git-path", "index"]);
    assert!(saw(&cache, &[index.trim().into()]));
    let now = read_all(&units, &mut cache);
    assert_eq!(cache.read_live, ["u1"]);
    assert!(now[0].iter().any(|f| f.path == "build/new.txt"));
    std::fs::write(a.join("Cargo.lock"), "lock\n").unwrap();
    assert!(saw(&cache, &[a.join("Cargo.lock")]));
    assert!(read_all(&units, &mut cache)[0].iter().any(|f| f.path == "Cargo.lock"));
    // git's own lock files are not.
    assert!(!saw(&cache, &[root.join("repo/.git/worktrees/wt-a/index.lock"), root.join("repo/.git/refs/heads/a.lock")]));
}

/// A symlink's own change marks its own unit (never the unit, or nothing, where it points),
/// and its id is its own text in both kinds of read: one put back as the base had it is no
/// change on the board as in the code.
#[test]
fn a_symlink_is_its_own_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (mut units, mut cache) = two_units(&root);
    let a = root.join("wt-a");
    read_all(&units, &mut cache);
    std::fs::write(root.join("external"), "outside\n").unwrap();
    let link = a.join("linked");
    std::os::unix::fs::symlink(root.join("external"), &link).unwrap();
    assert!(saw(&cache, std::slice::from_ref(&link)));
    assert_eq!(cache.marks.lock().unwrap().paths.iter().collect::<Vec<_>>(), [&link], "the link, not where it points");
    assert!(read_all(&units, &mut cache)[0].iter().any(|f| f.path == "linked"));
    assert_eq!(cache.read_live, ["u1"]);

    // Committed pointing at `first`, moved to `second`, put back to `first` in the work tree.
    std::fs::remove_file(&link).unwrap();
    std::fs::write(a.join("first"), "file bytes\n").unwrap();
    std::fs::write(a.join("second"), "file bytes\n").unwrap();
    std::os::unix::fs::symlink("first", &link).unwrap();
    git_in(&a, &["add", "first", "second", "linked"]);
    git_in(&a, &["commit", "-qm", "link base"]);
    git_in(&a, &["branch", "link-base", "HEAD"]);
    units[0].base = "link-base".into();
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("second", &link).unwrap();
    git_in(&a, &["commit", "-qam", "move link"]);
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("first", &link).unwrap();
    let net = scan::diff(&scan::Runner::Local, &a, "link-base", "HEAD").unwrap();
    cache.bare = true;
    cache.marks.lock().unwrap().all = true;
    let board = read_all(&units, &mut cache);
    let code = code_read(&units[0], &mut cache);
    assert!(!changed_paths(&[], &code, Some(&net)).contains("linked"));
    assert_eq!(changed_paths(&[], &board[0], Some(&net)), changed_paths(&[], &code, Some(&net)));
}

/// A file written again with its size and modification time kept, or replaced by another
/// such (a sync that keeps times), is read again and changes the code token.
#[test]
fn a_rewrite_that_keeps_size_and_time_reaches_the_page() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (units, mut cache) = two_units(&root);
    let file = root.join("wt-a/one.txt");
    let first = read_all(&units, &mut cache);
    let kept = std::fs::metadata(&file).unwrap().modified().unwrap();
    std::fs::write(&file, "one\nsix\n").unwrap();
    std::fs::File::options().write(true).open(&file).unwrap().set_modified(kept).unwrap();
    assert!(saw(&cache, std::slice::from_ref(&file)));
    let now = read_all(&units, &mut cache);
    assert_eq!(now[0][0].after.as_deref(), Some("one\nsix\n"));
    assert_ne!(code_of(&[], &now[0], None), code_of(&[], &first[0], None));
    let next = root.join("wt-a/next");
    std::fs::write(&next, "one\nten\n").unwrap();
    std::fs::File::options().write(true).open(&next).unwrap().set_modified(kept).unwrap();
    std::fs::rename(&next, &file).unwrap();
    assert!(saw(&cache, &[file]));
    let replaced = read_all(&units, &mut cache);
    assert_eq!(replaced[0][0].after.as_deref(), Some("one\nten\n"));
    assert_ne!(code_of(&[], &replaced[0], None), code_of(&[], &now[0], None));
}

/// A work tree read before the watcher followed it is read again once it is followed: what
/// changed in between is not taken as read.
#[test]
fn a_work_tree_followed_late_is_read_again_first() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (units, mut cache) = two_units(&root);
    let a = root.join("wt-a");
    cache.marks.lock().unwrap().watched.remove(&a);
    read_all(&units, &mut cache);
    std::fs::write(a.join("new.txt"), "written before it was followed\n").unwrap();
    cache.marks.lock().unwrap().watched.insert(a.clone());
    assert!(read_all(&units, &mut cache)[0].iter().any(|f| f.path == "new.txt"));
    read_all(&units, &mut cache);
    assert!(cache.read_live.is_empty(), "followed since its last read: taken as read");
}

/// A work tree made after the watcher started (a sibling folder, outside what it follows) is
/// registered by the read that finds it: an edit there is seen and marks its unit.
#[test]
fn a_late_work_tree_is_registered_by_the_read_that_finds_it() {
    use std::time::Duration;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (units, mut cache) = two_units(&root);
    let a = root.join("wt-a");
    let marks = cache.marks.clone();
    marks.lock().unwrap().watched.clear();
    let (tx, rx) = std::sync::mpsc::channel();
    let d = notify_debouncer_mini::new_debouncer(Duration::from_millis(100), move |res: notify_debouncer_mini::DebounceEventResult| {
        if let Ok(events) = res
            && crate::server::app::noticed(&marks, events.iter().map(|e| e.path.as_path()))
        {
            let _ = tx.send(());
        }
    })
    .unwrap();
    *cache.watcher.lock().unwrap() = Some(d);
    read_all(&units, &mut cache);
    assert!(cache.marks.lock().unwrap().cover(&a).is_some(), "registered by the read");
    std::fs::write(a.join("late.txt"), "late\n").unwrap();
    rx.recv_timeout(Duration::from_secs(10)).expect("the edit is seen");
    let now = read_all(&units, &mut cache);
    assert_eq!(cache.read_live, ["u1"]);
    assert!(now[0].iter().any(|f| f.path == "late.txt"));
}

/// A change whose committed side is over the size limit reads the same on the board and in
/// the code: named, uncounted, the same token; the old side never read.
#[test]
fn a_large_committed_side_reads_the_same_either_way() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (units, mut cache) = two_units(&root);
    let a = root.join("wt-a");
    for name in ["large-deleted.txt", "large-shrunk.txt"] {
        std::fs::write(a.join(name), vec![b'a'; 512 * 1024 + 1]).unwrap();
    }
    git_in(&a, &["add", "."]);
    git_in(&a, &["commit", "-qm", "large files"]);
    std::fs::remove_file(a.join("large-deleted.txt")).unwrap();
    std::fs::write(a.join("large-shrunk.txt"), "small\n").unwrap();
    cache.bare = true;
    let board = read_all(&units, &mut cache);
    let code = code_read(&units[0], &mut cache);
    assert_eq!(code.iter().map(|f| (f.path.as_str(), f.uncounted)).collect::<Vec<_>>(), [("large-deleted.txt", true), ("large-shrunk.txt", true)]);
    assert_eq!(code_of(&[], &code, None), code_of(&[], &board[0], None));
}

// Round two review's probes (each a gap it found), kept as the tests of the fixes.

#[test]
fn a_deleted_file_read_by_name_first_has_its_diff_in_the_code() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (units, mut cache) = two_units(&root);
    let a = root.join("wt-a");
    std::fs::write(a.join("one.txt"), "one\n").unwrap();
    std::fs::remove_file(a.join("two.txt")).unwrap();
    cache.bare = true;
    let board = read_all(&units, &mut cache);
    assert_eq!(board[0].len(), 1);
    assert_eq!(board[0][0].status, "D");
    let code = code_read(&units[0], &mut cache);
    let forced = scan::live_in(&scan::Runner::Local, &a).unwrap();
    assert_eq!((forced[0].before.as_deref(), forced[0].del), (Some("one\n"), 1));
    assert_eq!(
        (code[0].before.as_deref(), code[0].del, code[0].uncounted),
        (Some("one\n"), 1, false),
        "only the deleted file is present; board had no text and code refreshed {:?}, token equal: {}",
        cache.read_live,
        code_of(&[], &board[0], None) == code_of(&[], &code, None),
    );
}

#[test]
fn a_deleted_files_token_is_the_same_either_way() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (units, mut cache) = two_units(&root);
    let a = root.join("wt-a");
    std::fs::remove_file(a.join("two.txt")).unwrap();
    cache.bare = true;
    let board = read_all(&units, &mut cache);
    let code = scan::live_in(&scan::Runner::Local, &a).unwrap();
    assert!(code.iter().any(|f| f.path == "two.txt" && f.before.as_deref() == Some("one\n")));
    assert_eq!(code_of(&[], &code, None), code_of(&[], &board[0], None), "only the board/code view mode changed");
}

#[test]
fn a_code_read_keeps_the_ids_the_board_found() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (units, mut cache) = two_units(&root);
    let a = root.join("wt-a");
    git_in(&a, &["commit", "-qam", "change file"]);
    std::fs::write(a.join("one.txt"), "one\n").unwrap();
    let net = scan::diff(&scan::Runner::Local, &a, "main", "HEAD").unwrap();
    let overlaps = |files: &[scan::FileChange]| changed_paths(&[], files, Some(&net)).contains("one.txt");
    cache.bare = true;
    let first = read_all(&units, &mut cache);
    assert!(!overlaps(&first[0]), "correct before opening");
    let code = code_read(&units[0], &mut cache);
    assert!(!overlaps(&code), "correct in full code");
    let board = read_all(&units, &mut cache);
    assert!(!overlaps(&board[0]), "false overlap on the next snapshot; prior ID {:?}, current ID {:?}; re-read {:?}",
        first[0][0].after_id, board[0][0].after_id, cache.read_live);
}

#[test]
fn a_moved_base_reads_the_ids_its_net_change_needs() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (units, mut cache) = two_units(&root);
    let repo = root.join("repo");
    let a = root.join("wt-a");
    std::fs::write(repo.join("one.txt"), "base advance\n").unwrap();
    git_in(&repo, &["commit", "-qam", "advance base"]);
    git_in(&a, &["reset", "--hard", "main"]);
    std::fs::write(a.join("two.txt"), "another change\n").unwrap();
    git_in(&a, &["commit", "-qam", "feature"]);
    std::fs::write(a.join("one.txt"), "one\n").unwrap();
    cache.bare = true;
    let board = read_all(&units, &mut cache);
    assert!(board[0][0].after_id.bytes().all(|b| b == b'0'), "one.txt is only a live change before base moves");
    git_in(&repo, &["update-ref", "refs/heads/main", "HEAD^"]);
    assert!(saw(&cache, &[repo.join(".git/refs/heads/main")]));
    cache.refs.clear();
    cache.worktrees.clear();
    cache.read_live.clear();
    cache.take_marks();
    let (_, board, net) = super::code::read_unit(&scan::Runner::Local, &units[0], &mut cache, false);
    let (key, _) = net.unwrap();
    let net = &cache.nets[&key];
    assert!(net.iter().any(|f| f.path == "one.txt" && f.before.as_deref() == Some("one\n")));
    let reference = scan::live_in(&scan::Runner::Local, &a).unwrap();
    assert!(!changed_paths(&[], &reference, Some(net)).contains("one.txt"));
    assert!(!changed_paths(&[], &board, Some(net)).contains("one.txt"), "board with new merge base used old id {:?}, re-read {:?}", board[0].after_id, cache.read_live);
}

#[test]
fn a_large_committed_side_is_not_hashed_for_names() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (units, mut cache) = two_units(&root);
    let a = root.join("wt-a");
    let file = a.join("shrank.txt");
    std::fs::write(&file, vec![0; 512 * 1024 + 1]).unwrap();
    git_in(&a, &["add", "shrank.txt"]);
    git_in(&a, &["commit", "-qm", "large file"]);
    std::fs::write(&file, "small\n").unwrap();
    cache.bare = true;
    let normal = read_all(&units, &mut cache);
    let file = normal[0].iter().find(|f| f.path == "shrank.txt").unwrap();
    assert!(file.uncounted);
    assert!(file.after_id.bytes().all(|b| b == b'0'), "over-limit before blob but hash-object read the after file and produced {:?}", file.after_id);
}

#[test]
fn a_work_tree_made_again_is_read_again() {
    use std::os::unix::fs::MetadataExt;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (units, mut cache) = two_units(&root);
    let a = root.join("wt-a");
    let old = root.join("old-worktree");
    {
        let mut marks = cache.marks.lock().unwrap();
        marks.watched.remove(&a);
    }
    // Hold a real watcher, with no callbacks passed into Marks during registration, as
    // when filesystem edits precede restart of that root's watch.
    let d = notify_debouncer_mini::new_debouncer(std::time::Duration::from_millis(300), |_: notify_debouncer_mini::DebounceEventResult| {}).unwrap();
    *cache.watcher.lock().unwrap() = Some(d);
    cache.bare = true;
    let first = read_all(&units, &mut cache);
    assert_eq!(first[0][0].path, "one.txt");
    assert!(cache.live["u1"].followed.is_some());
    let before = cache.marks.lock().unwrap().inodes[&a];
    std::fs::rename(&a, &old).unwrap();
    std::fs::create_dir(&a).unwrap();
    for name in [".git", ".gitignore", "two.txt"] {
        std::fs::copy(old.join(name), a.join(name)).unwrap();
    }
    std::fs::write(a.join("one.txt"), "one\n").unwrap();
    std::fs::write(a.join("replacement.txt"), "new root\n").unwrap();
    let inode = std::fs::metadata(&a).unwrap().ino();
    assert_ne!(before, inode);
    let board = read_all(&units, &mut cache);
    assert_eq!(cache.marks.lock().unwrap().inodes[&a], inode, "real watcher registration succeeded at the replacement root");
    assert!(board[0].iter().any(|f| f.path == "replacement.txt"), "root was registered but cached file list reused; re-read {:?}", cache.read_live);
}

#[test]
fn a_closed_unit_lets_go_of_its_texts() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (mut units, mut cache) = two_units(&root);
    cache.bare = true;
    read_all(&units, &mut cache);
    code_read(&units[0], &mut cache);
    assert!(cache.live["u1"].known.has_texts());
    let state = core::State::default();
    assert!(open_work(&state, &units[0]));
    units[0].done = Some(core::Closed { at: String::new(), by: "user".into(), reason: String::new() });
    unit(&state, &[], &units[0], &mut cache, &Names(HashMap::new()), None);
    assert!(!cache.live["u1"].known.has_texts());
    assert!(!cache.texted.contains(&"u1".to_string()));
}
