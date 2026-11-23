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
