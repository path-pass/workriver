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

/// A unit `u1` working on `feature` from `main` in `repo`.
fn unit_in(repo: &std::path::Path) -> core::Unit {
    core::Unit {
        id: "u1".into(), intent: "i1".into(), title: String::new(), purpose: String::new(),
        repo: repo.to_str().unwrap().into(), branch: "feature".into(), base: "main".into(),
        at: String::new(), merged: None, machine: None, merged_commits: vec![], tasks: vec![],
        earlier: vec![], links: vec![], done: None, reopened: None, kind: core::UnitKind::Code,
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
