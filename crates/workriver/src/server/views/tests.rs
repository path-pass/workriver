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
