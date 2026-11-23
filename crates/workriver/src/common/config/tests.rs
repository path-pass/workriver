// The settings file as the user may have edited it by hand.
use serde_json::json;

use super::attachments::*;
use super::*;

type S = Settings<serde_json::Value>;

fn home_with(settings: &serde_json::Value) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join(crate::common::home::SETTINGS), serde_json::to_vec(settings).unwrap()).unwrap();
    home
}

fn rule_of(attachments: serde_json::Value) -> Result<Rule, String> {
    S::attachment_rule(home_with(&json!({ "attachments": attachments })).path())
}

#[test]
fn the_attachment_rule_is_keep_until_set() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(S::attachment_rule(home.path()), Ok(Rule::default()));
    assert_eq!(Rule::default().after, After::Keep);
    assert_eq!(Rule::default().days, 30);
    // A file that does not parse is the defaults, as every setting is.
    std::fs::write(home.path().join(crate::common::home::SETTINGS), "{ not json").unwrap();
    assert_eq!(S::attachment_rule(home.path()), Ok(Rule::default()));
    // A file without the key: keep; the keys left out take their defaults.
    assert_eq!(rule_of(json!({ "after": "delete" })), Ok(Rule { after: After::Delete, ..Rule::default() }));
}

#[test]
fn a_rule_out_of_range_is_an_error_naming_its_key_never_brought_into_range() {
    for (bad, key) in [
        (json!({ "after": "delete", "days": 0 }), "days"),
        (json!({ "after": "delete", "days": 4000 }), "days"),
        (json!({ "after": "delete", "days": "ten" }), "days"),
        (json!({ "after": "delete", "minMb": 50001 }), "minMb"),
        (json!({ "after": "move" }), "target"),
        (json!({ "after": "move", "target": { "path": "/x" } }), "target"),
        (json!({ "after": "shred" }), "after"),
        (json!({ "after": "delete", "only": "big" }), "only"),
        (json!("delete"), "attachments"),
    ] {
        let e = rule_of(bad.clone()).expect_err(&bad.to_string());
        assert!(e.starts_with(key), "{bad}: {e}");
        assert!(e.contains("Settings → Attachments"), "{e}");
    }
    assert_eq!(rule_of(json!({ "after": "delete", "days": 0 })).unwrap_err(), "days is 0, not 1 to 3650; set it in Settings → Attachments");
    // A target is read only when moving.
    assert_eq!(rule_of(json!({ "after": "delete", "target": "nonsense" })).map(|r| r.after), Ok(After::Delete));
}

#[test]
fn a_rule_is_saved_checked_and_the_rest_of_the_file_stays() {
    let home = home_with(&json!({ "stallMinutes": 7 }));
    let target = workriver_core::Target { kind: "folder".into(), rest: serde_json::from_value(json!({ "path": "/x" })).unwrap() };
    let rule = Rule { after: After::Move, days: 10, only: workriver_core::Only::Pictures, min_mb: 1, target: Some(target) };
    S::set_attachment_rule(home.path(), &rule).unwrap();
    assert_eq!(S::attachment_rule(home.path()), Ok(rule.clone()));
    assert_eq!(S::stall_minutes(home.path()), 7);
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(home.path().join(crate::common::home::SETTINGS)).unwrap()).unwrap();
    assert_eq!(saved["attachments"], json!({ "after": "move", "days": 10, "only": "pictures", "minMb": 1, "target": { "kind": "folder", "path": "/x" } }));
    assert_eq!(rule.conditions(), workriver_core::Conditions { days: 10, only: workriver_core::Only::Pictures, min_mb: 1 });
    // Out of range: refused, the saved one stays.
    let e = S::set_attachment_rule(home.path(), &Rule { days: 0, ..rule.clone() }).unwrap_err();
    assert!(e.starts_with("days is 0"), "{e}");
    assert_eq!(S::attachment_rule(home.path()), Ok(rule));
    // Back to the default: the key is no longer written.
    S::set_attachment_rule(home.path(), &Rule::default()).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(home.path().join(crate::common::home::SETTINGS)).unwrap()).unwrap();
    assert!(saved.get("attachments").is_none(), "{saved}");
}
