// `new write` and `new check` on what `old apply` wrote for the fixtures of `old`
// (`__fixtures__/rows`, `__fixtures__/replay`; the tests of `old` keep them in step).
use std::path::{Path, PathBuf};

use migrate_virtual_workspaces_new::{check::check, write::write};
use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/__fixtures__").join(name)
}

/// A copy of a fixture's folder, its events changed by `edit` (the lines as JSON).
fn edited(name: &str, edit: impl Fn(&mut Vec<Value>)) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for f in ["seats.jsonl", "old-state.json"] {
        std::fs::copy(fixture(name).join(f), tmp.path().join(f)).unwrap();
    }
    let mut ev: Vec<Value> = std::fs::read_to_string(fixture(name).join("events.jsonl")).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    edit(&mut ev);
    std::fs::write(tmp.path().join("events.jsonl"), ev.iter().map(|e| format!("{e}\n")).collect::<String>()).unwrap();
    tmp
}

fn find(ev: &[Value], kind: &str, field: &str, value: &str) -> usize {
    ev.iter().position(|e| e["kind"] == kind && e["body"][field] == value).unwrap()
}

#[test]
fn the_fixtures_migrate_and_pass_the_check() {
    for name in ["rows", "replay"] {
        let out = tempfile::tempdir().unwrap();
        let db = out.path().join("workriver.db");
        let said = write(&fixture(name), &db).unwrap();
        assert!(said.contains("it replays"), "{said}");
        let report = check(&fixture(name), &db).unwrap();
        assert!(report.contains("the check passes"), "{report}");
        // The new store is marked, and the product opens it.
        let conn = rusqlite::Connection::open(&db).unwrap();
        let mark: String = conn.query_row("SELECT events FROM format", [], |r| r.get(0)).unwrap();
        assert_eq!(mark, "virtual-workspaces");
        // Every output is new.
        assert!(write(&fixture(name), &db).unwrap_err().contains("exists"));
    }
}

#[test]
fn a_written_event_the_new_rules_refuse_fails_the_run() {
    // The manager's start, pointed at its record's other workspace, of another project.
    let tmp = edited("rows", |ev| {
        let k = find(ev, "intent.state_changed", "intent", "i6");
        ev[k]["body"]["workspace"] = json!("ws2");
    });
    let db = tmp.path().join("out.db");
    let e = write(tmp.path(), &db).unwrap_err();
    assert!(e.contains("refused: i6 belongs to project App; workspace ws2"), "{e}");
    assert!(!db.exists(), "nothing is written");
}

#[test]
fn a_body_that_comes_out_different_fails_the_run() {
    let tmp = edited("rows", |ev| {
        let k = find(ev, "workspace.created", "id", "ws3");
        ev[k]["body"]["id"] = json!("ws9");
    });
    let e = write(tmp.path(), &tmp.path().join("out.db")).unwrap_err();
    assert!(e.contains("comes out different"), "{e}");
    assert!(e.contains(r#"written:  {"id":"ws9""#) && e.contains(r#"prepared: {"id":"ws3""#), "{e}");
}

#[test]
fn an_old_event_kept_as_it_was_is_not_put_through_todays_rules() {
    // A unit's branch with no prefix: accepted when it was recorded, refused today. Kept
    // byte for byte, it passes; put through today's rules, it would not.
    let out = tempfile::tempdir().unwrap();
    write(&fixture("rows"), &out.path().join("a.db")).unwrap();
    let tmp = edited("rows", |ev| {
        let k = ev.iter().position(|e| e["kind"] == "unit.created").unwrap();
        assert_eq!(ev[k]["check"], Value::Null);
        ev[k]["check"] = json!(true);
    });
    let e = write(tmp.path(), &tmp.path().join("b.db")).unwrap_err();
    assert!(e.contains("unit.created") && e.contains("refused"), "{e}");
}

#[test]
fn an_unexplained_difference_fails_the_check() {
    let out = tempfile::tempdir().unwrap();
    let db = out.path().join("workriver.db");
    write(&fixture("rows"), &db).unwrap();
    let tmp = edited("rows", |_| {});
    let path = tmp.path().join("old-state.json");
    let mut old: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    old["units"][0]["title"] = json!("Another title");
    old["expected"]["messages"]["m2"] = json!("ws2");
    std::fs::write(&path, old.to_string()).unwrap();
    let e = check(tmp.path(), &db).unwrap_err();
    assert!(e.contains("DIFFERENT: units differs"), "{e}");
    assert!(e.contains(r#"DIFFERENT: message m2 is in "ws1" after, the mapping says "ws2""#), "{e}");
    assert!(e.contains("the check fails: 2 differences"), "{e}");
}
