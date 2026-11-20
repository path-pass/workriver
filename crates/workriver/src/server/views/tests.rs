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
    let token = |f: scan::FileChange| code_of(&[], &[f]);
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

/// A unit's changed files are what its code tree shows with nothing picked (the page's
/// `fileDelta` over all its commits): git entries compared at the ends, uncommitted work's
/// side by the object id its text would get.
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
        before_mode: before.0.into(),
        before_id: before.1.into(),
        after_mode: after.0.into(),
        after_id: after.1.into(),
    };
    let commit = |files: Vec<scan::FileChange>| scan::Commit { id: "c".into(), hash: "c".into(), subject: String::new(), date: String::new(), author: String::new(), task: None, merge: false, files };
    let commits = [
        commit(vec![
            f("kept.txt", ("100644", one), ("100644", two), (None, None)),
            f("back.txt", ("100644", one), ("100644", two), (None, None)),
            f("gone.txt", (GONE, &zero), ("100644", one), (None, None)),
            f("mode.sh", ("100644", one), ("100755", two), (None, None)),
            f("undone.txt", ("100644", one), ("100644", two), (None, None)),
        ]),
        commit(vec![f("gone.txt", ("100644", one), (GONE, &zero), (None, None)), f("mode.sh", ("100755", two), ("100755", one), (None, None))]),
    ];
    let live = [
        // Changed back to the base's text in the work tree: not a change any more.
        f("back.txt", ("100644", two), ("100644", &zero), (Some("one\ntwo\n"), Some("one\n"))),
        // Changed in the work tree alone.
        f("new.txt", (GONE, &zero), ("100644", &zero), (None, Some("one\n"))),
        f("undone.txt", ("100644", two), ("100644", &zero), (Some("one\ntwo\n"), Some("one\ntwo\nthree\n"))),
    ];
    let got: Vec<String> = changed_paths(&commits, &live).into_iter().collect();
    assert_eq!(got, ["kept.txt", "mode.sh", "new.txt", "undone.txt"]);
}
