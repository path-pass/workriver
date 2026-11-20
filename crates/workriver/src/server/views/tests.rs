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
