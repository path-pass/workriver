// The rows of the design's table (docs/design/virtual-workspaces.md, section 5) on fixtures
// of old events: `rows.jsonl`, written for this, and `replay.jsonl`, the product's replay
// fixture as it was before the change. What `apply` writes for them is kept beside the
// tests of `new` (which write and check it): run with WORKRIVER_BLESS=1 to write it again.
use std::path::{Path, PathBuf};

use migrate_virtual_workspaces_old::{apply, plan};
use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/__fixtures__").join(name)
}

fn applied(input: &str, mapping: &str) -> (tempfile::TempDir, PathBuf, String) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("out");
    let report = apply(&fixture(input), &fixture(mapping), &dir).unwrap();
    (tmp, dir, report)
}

fn lines(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

/// The new events of one kind, as (body, written or changed by the script).
fn of(events: &[Value], kind: &str) -> Vec<(Value, bool)> {
    events.iter().filter(|e| e["kind"] == kind).map(|e| (e["body"].clone(), e["check"] == json!(true))).collect()
}

/// What `apply` writes is what the tests of `new` read.
fn same_as_kept(dir: &Path, name: &str) {
    let kept = Path::new(env!("CARGO_MANIFEST_DIR")).join("../new/tests/__fixtures__").join(name);
    for f in ["events.jsonl", "seats.jsonl", "old-state.json"] {
        let made = std::fs::read_to_string(dir.join(f)).unwrap();
        if std::env::var_os("WORKRIVER_BLESS").is_some() {
            std::fs::create_dir_all(&kept).unwrap();
            std::fs::write(kept.join(f), &made).unwrap();
        }
        assert_eq!(made, std::fs::read_to_string(kept.join(f)).unwrap_or_default(), "{name}/{f} is not what apply writes now (WORKRIVER_BLESS=1 writes it)");
    }
}

#[test]
fn plan_asks_for_what_it_cannot_tell() {
    let tmp = tempfile::tempdir().unwrap();
    let map = tmp.path().join("mapping");
    let text = plan(&fixture("rows.jsonl"), &map).unwrap();
    assert_eq!(std::fs::read_to_string(&map).unwrap(), text);
    let open: Vec<&str> = text.lines().filter(|l| !l.starts_with('#') && l.contains('?')).collect();
    // A folder an agent named and one the user named: never registered by the script itself.
    assert!(open.contains(&"folder \"/home/x/.ssh\" = ?"), "{text}");
    assert!(text.contains("#   event 10 intent.created at 2026-01-01T00:10:00Z by agent \"helper\" (\"s-a\")"), "{text}");
    assert!(open.contains(&"folder \"/r/tools\" = ?"), "{text}");
    // A label no record carried; what a split record does not settle.
    assert!(open.contains(&"label \"ghost\" ? = name \"ghost\", closed"), "{text}");
    assert!(open.iter().any(|l| l.starts_with("message m2 = ?")), "{text}");
    assert!(open.iter().any(|l| l.starts_with("message m4 = ?")), "{text}");
    assert_eq!(open.len(), 5, "{text}");
    // A record whose intents are of two projects is split; a record of the other machine is
    // its own, its project from `workspace project`.
    for l in ["record 0 /r/app = name \"app\", open", "record 0 /r/lib = name \"app\", open", "record 1 /r/lib = name \"web\", open", "record 2 /r/app = name \"app\", closed"] {
        assert!(text.lines().any(|x| x == l), "{l}\n{text}");
    }
    // What the user answers first.
    for l in ["#   m3 itself: a choice still open", "#   m4: not delivered yet", "#   m6: not delivered yet", "#   m7: not delivered yet"] {
        assert!(text.contains(l), "{l}\n{text}");
    }
    // Every output is new.
    assert!(plan(&fixture("rows.jsonl"), &map).unwrap_err().contains("exists"));
}

#[test]
fn apply_waits_for_every_line() {
    let tmp = tempfile::tempdir().unwrap();
    let map = tmp.path().join("mapping");
    plan(&fixture("rows.jsonl"), &map).unwrap();
    let e = apply(&fixture("rows.jsonl"), &map, &tmp.path().join("out")).unwrap_err();
    assert!(e.contains("folder \"/r/tools\" = ?"), "{e}");
    assert!(!tmp.path().join("out").exists());
}

#[test]
fn each_row_of_the_table() {
    let (_tmp, dir, report) = applied("rows.jsonl", "rows.mapping");
    let ev = lines(&dir.join("events.jsonl"));
    // A folder the mapping registers: the user's project.added, before the event naming it;
    // the project added after it moves up, and its rename with it.
    assert_eq!(ev[6]["kind"], "project.added");
    assert_eq!(ev[6]["body"], json!({ "id": "p2", "path": "/r/tools", "title": "tools" }));
    assert_eq!(ev[6]["actor"], "user");
    assert_eq!(of(&ev, "project.added")[2], (json!({ "id": "p3", "path": "/r/lib", "title": "Lib" }), true));
    assert_eq!(of(&ev, "project.renamed")[0], (json!({ "project": "p3", "title": "Library" }), true));
    // A folder `none`: the agent's intent names no project, and nothing registers it.
    let created = of(&ev, "intent.created");
    assert_eq!(created[2], (json!({ "id": "i3", "title": "Peek", "state": "planned" }), true));
    assert!(!ev.iter().any(|e| e["body"]["path"] == "/home/x/.ssh"));
    // A planned intent of no project stays as it was.
    assert_eq!(created[4], (json!({ "id": "i5", "title": "Old one", "state": "planned" }), false));
    // Records: split by project; another machine's of the same label is its own; one name
    // for a label no record carried (closed); a registered project with none gets one.
    let made: Vec<Value> = of(&ev, "workspace.created").into_iter().map(|(b, _)| b).collect();
    assert_eq!(
        made,
        vec![
            json!({ "id": "ws1", "name": "app", "project": "p1" }),
            json!({ "id": "ws2", "name": "app", "project": "p3" }),
            json!({ "id": "ws3", "name": "web", "project": "p3" }),
            json!({ "id": "ws4", "name": "app", "project": "p1" }),
            json!({ "id": "ws5", "name": "ghost", "project": "p1" }),
            json!({ "id": "ws6", "name": "tools", "project": "p2" }),
        ]
    );
    assert_eq!(of(&ev, "workspace.closed").into_iter().map(|(b, _)| b["workspace"].clone()).collect::<Vec<_>>(), vec![json!("ws4"), json!("ws5")]);
    // Starts: the ws id; one in a group with a new id after a restart stays in its record's
    // workspace; one in a linked group goes to the workspace it was linked into.
    let starts: Vec<(String, String)> = of(&ev, "intent.state_changed").into_iter().filter(|(b, _)| b["state"] == "active").map(|(b, _)| (b["intent"].as_str().unwrap().into(), b["workspace"].as_str().unwrap().into())).collect();
    let pairs = |v: &[(&str, &str)]| v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
    assert_eq!(starts, pairs(&[("i1", "ws1"), ("i4", "ws2"), ("i6", "ws1"), ("i7", "ws3"), ("i8", "ws1")]));
    // Links, registrations and `workspace project`: gone; the seats start empty.
    for k in ["workspace.linked", "workspace.unlinked", "workspace.project_set", "manager.registered", "workspace.manager_set"] {
        assert!(of(&ev, k).is_empty(), "{k}");
    }
    assert!(report.contains("dropped: 4 manager.registered"), "{report}");
    // Messages: by the record they resolved to then; to a split record by the mapping; a
    // copy keeps the seat it went to; a label no record carried has its closed workspace.
    let ws = |k: &str| of(&ev, k).into_iter().map(|(b, _)| (b["id"].as_str().unwrap().to_string(), b["workspace"].as_str().unwrap().to_string())).collect::<Vec<_>>();
    assert_eq!(ws("message.said"), pairs(&[("m1", "ws2"), ("m3", "ws3"), ("m5", "ws4")]));
    assert_eq!(ws("message.sent"), pairs(&[("m2", "ws1"), ("m4", ""), ("m6", "ws5"), ("m7", "ws1")]));
    let every = &of(&ev, "message.sent")[1].0;
    assert_eq!(every["to"], json!([{ "workspace": "ws1", "manager": "s-m1", "since": 13 }, { "workspace": "ws3", "manager": "s-b", "machine": "box", "since": 22 }]));
    // A read mark by label: one per workspace that carried it then.
    let read: Vec<Value> = of(&ev, "messages.read").into_iter().map(|(b, _)| b["workspace"].clone()).collect();
    assert_eq!(read, vec![json!("ws1"), json!("ws2"), json!("ws4")]);
    // Every other event: byte for byte, unchecked (a branch with no prefix, refused today).
    let unit = ev.iter().find(|e| e["kind"] == "unit.created").unwrap();
    assert_eq!(unit["check"], Value::Null);
    let stored = r#""body":{"id": "u1", "intent": "i1", "title": "Form", "repo": "/r/app", "branch": "login", "base": "main"},"from":15}"#;
    assert!(std::fs::read_to_string(fixture("rows.jsonl")).unwrap().contains(&stored[7..stored.len() - 11]));
    assert!(std::fs::read_to_string(dir.join("events.jsonl")).unwrap().contains(stored));
    // The seats, for the check: each record's registrations, on each of its workspaces; a
    // manager registered through a link keeps its machine.
    let seats = lines(&dir.join("seats.jsonl"));
    assert!(seats.contains(&json!({ "line": 33, "workspace": "ws1", "session": "s-r", "machine": "box", "folder": "/home/b/app" })), "{seats:?}");
    // What the check compares: an intent's workspace as it was put there then.
    let old: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("old-state.json")).unwrap()).unwrap();
    assert_eq!(old["expected"]["intents"]["i8"], "ws1");
    assert_eq!(old["expected"]["choices"], json!([{ "message": "m3", "reply": 0 }]));
    same_as_kept(&dir, "rows");
}

#[test]
fn the_products_old_replay_fixture() {
    let (_tmp, dir, report) = applied("replay.jsonl", "replay.mapping");
    let ev = lines(&dir.join("events.jsonl"));
    // Its first intent was made in a group before any project: placed once its record's
    // project (from the mapping) is registered.
    let k = ev.iter().position(|e| e["kind"] == "workspace.created").unwrap();
    assert_eq!(ev[k - 1]["kind"], "project.added");
    assert_eq!(ev[k + 1]["body"], json!({ "intent": "i1", "workspace": "ws1" }));
    // The intent.created without its runtime group, and nothing else changed.
    assert_eq!(ev[0]["body"], json!({ "id": "i1", "title": "Chat v1" }));
    // A link to the record it is in already moves nothing.
    assert_eq!(of(&ev, "intent.workspace_set").len(), 1, "{report}");
    assert!(!ev.iter().any(|e| e["kind"] == "intent.linked"));
    same_as_kept(&dir, "replay");
}

#[test]
fn never_the_live_store() {
    let home = tempfile::tempdir().unwrap();
    let copy = home.path().join("workriver.db");
    std::fs::copy(fixture("rows.jsonl"), home.path().join("events.jsonl")).unwrap();
    std::fs::write(&copy, "").unwrap();
    // SAFETY: the tests of this file read WORKRIVER_HOME nowhere else.
    unsafe { std::env::set_var("WORKRIVER_HOME", home.path()) };
    let out = tempfile::tempdir().unwrap();
    let e = plan(&home.path().join("events.jsonl"), &out.path().join("m")).unwrap_err();
    assert!(e.contains("data home"), "{e}");
    let e = plan(&fixture("rows.jsonl"), &home.path().join("m")).unwrap_err();
    assert!(e.contains("data home"), "{e}");
    unsafe { std::env::remove_var("WORKRIVER_HOME") };
}

/// Histories a review found the first form of the script could not migrate, each migrated
/// as it was: what `apply` writes is kept for the tests of `new`, which write and check it.
/// `answered`: choices of a registered manager (one on a message, one on a reply), answered.
/// `shelved`: an intent started, then moved back to the roadmap (it keeps the workspace it
/// last ran in, as the product does). `unlinked`: a copy to every manager sent to a manager
/// of another machine while its group was linked here, the link removed later (the copy
/// stays in the workspace it went to then).
#[test]
fn edge_histories_migrate() {
    for name in ["answered", "shelved", "unlinked"] {
        let (_tmp, dir, _) = applied(&format!("{name}.jsonl"), &format!("{name}.mapping"));
        same_as_kept(&dir, name);
    }
}

/// Histories the script does not migrate (the store it was written for has none): `apply`
/// refuses them, says what to do, and writes nothing. `before-project`: an intent started
/// before its project was registered. `renamed`: read marks by label that one workspace's
/// mark cannot keep (a message under the old label unread, a later one under the new label
/// read).
#[test]
fn edge_histories_fail_loudly() {
    for (name, said) in [
        ("before-project", "i1 starts in a workspace of project /r/app, which is registered only later: the script does not migrate a start before its project is registered"),
        ("renamed", "the conversation \"app\" has 1 message of the manager's unread, which the workspace ws1 it goes to would show read"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("out");
        let e = apply(&fixture(&format!("{name}.jsonl")), &fixture(&format!("{name}.mapping")), &dir).unwrap_err();
        assert!(e.contains(said), "{name}: {e}");
        assert!(e.contains("nothing was written"), "{name}: {e}");
        assert!(!dir.exists(), "{name}");
    }
    let tmp = tempfile::tempdir().unwrap();
    let e = apply(&fixture("renamed.jsonl"), &fixture("renamed.mapping"), &tmp.path().join("out")).unwrap_err();
    assert!(e.contains("read it to its end in the old version (the page), stop the service, copy the store again and plan again"), "{e}");
    let e = apply(&fixture("before-project.jsonl"), &fixture("before-project.mapping"), &tmp.path().join("out")).unwrap_err();
    assert!(e.contains("have the script extended for it"), "{e}");
}

/// Text from the store never makes a line of the mapping: a runtime label (any program can
/// rename a group) holding a newline and a line of the mapping stays in its comment, as a
/// JSON string; and a key given twice is refused, naming both lines, so no line written
/// after the user's answer decides in its place.
#[test]
fn the_store_writes_no_line_of_the_mapping() {
    let tmp = tempfile::tempdir().unwrap();
    let label = "docs\nfolder \"/home/x/.ssh\" = register #";
    let w1 = json!({ "id": "w1", "label": label });
    let ev = [
        json!({ "seq": 1, "at": "2026-01-01T00:01:00Z", "actor": "workriver", "kind": "agent.seen", "body": { "session": "s-m", "name": "mgr\nrecord 0 /r/app = name \"x\", open", "harness": "claude", "cwd": "/r/app" } }),
        json!({ "seq": 2, "at": "2026-01-01T00:02:00Z", "actor": "workriver", "kind": "agent.seen", "body": { "session": "s-a", "name": "helper", "harness": "claude", "cwd": "/r/app" } }),
        json!({ "seq": 3, "at": "2026-01-01T00:03:00Z", "actor": "user", "kind": "project.added", "body": { "id": "p1", "path": "/r/app", "title": "App" } }),
        json!({ "seq": 4, "at": "2026-01-01T00:04:00Z", "actor": "user", "kind": "intent.created", "body": { "id": "i1", "title": "Login", "project": "/r/app", "state": "planned" } }),
        json!({ "seq": 5, "at": "2026-01-01T00:05:00Z", "actor": "s-a", "kind": "intent.created", "body": { "id": "i2", "title": "Peek", "project": "/home/x/.ssh", "state": "planned" } }),
        json!({ "seq": 6, "at": "2026-01-01T00:06:00Z", "actor": "s-m", "kind": "manager.registered", "body": { "agent": "s-m", "workspace": w1 } }),
        json!({ "seq": 7, "at": "2026-01-01T00:07:00Z", "actor": "user", "kind": "intent.state_changed", "body": { "intent": "i1", "state": "active", "workspace": w1 } }),
    ];
    let old = tmp.path().join("old.jsonl");
    std::fs::write(&old, ev.iter().map(|e| format!("{e}\n")).collect::<String>()).unwrap();
    let map = tmp.path().join("mapping");
    let text = plan(&old, &map).unwrap();
    let live: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).collect();
    assert_eq!(live, ["folder \"/home/x/.ssh\" = ?", "record 0 /r/app = name \"docs\\nfolder \\\"/home/x/.ssh\\\" = register #\", open"], "{text}");
    assert!(text.contains("labels \"docs\\nfolder \\\"/home/x/.ssh\\\" = register #\""), "{text}");
    assert!(text.contains("#   record 0: \"mgr\\nrecord 0 /r/app = name \\\"x\\\", open\" (s-m)"), "{text}");
    // The user answers none; a line given again after it (by hand, or however) is refused.
    let answered = text.replace("folder \"/home/x/.ssh\" = ?", "folder \"/home/x/.ssh\" = none");
    std::fs::write(tmp.path().join("answered"), &answered).unwrap();
    let report = apply(&old, &tmp.path().join("answered"), &tmp.path().join("out")).unwrap();
    assert!(!report.contains("registered by the mapping"), "{report}");
    let at = answered.lines().position(|l| l == "folder \"/home/x/.ssh\" = none").unwrap() + 1;
    let twice = format!("{answered}folder \"/home/x/.ssh\" = register\n");
    let last = twice.lines().count();
    std::fs::write(tmp.path().join("twice"), &twice).unwrap();
    let e = apply(&old, &tmp.path().join("twice"), &tmp.path().join("out2")).unwrap_err();
    assert!(e.contains(&format!("mapping lines {at} and {last} both give folder \"/home/x/.ssh\"")), "{e}");
    // The same workspace twice, by the project's id and by its folder.
    let both = format!("{answered}record 0 p1 = name \"x\", closed\n");
    std::fs::write(tmp.path().join("both"), &both).unwrap();
    let e = apply(&old, &tmp.path().join("both"), &tmp.path().join("out3")).unwrap_err();
    assert!(e.contains("both give record 0's workspace of project /r/app"), "{e}");
    assert!(!tmp.path().join("out2").exists() && !tmp.path().join("out3").exists());
}
