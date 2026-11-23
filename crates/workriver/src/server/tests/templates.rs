// Templates through the service: the user's library (saved from its text, read back, removed),
// the built-ins every reader is given, a unit made from one, and what the manager is told
// and warned of about the steps it placed.
use serde_json::json;

use super::support::*;

const MINE: &str = "# A small fix, my way.\nname: quick\nunit: code\ndescription: A small fix.\nsteps:\n  - id: fix\n    kind: implement\n    description: The fix.\n  - id: look\n    kind: review\n    description: A look at it.\n";

/// Only the user saves or removes a template, from its text (never through `/api/append`);
/// anyone reads one with its text, and every reader is given the built-ins as values.
#[test]
fn the_library_is_the_users() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle))];
    let user = daemon_with(fake, &tmp.path().join("w.db"), crate::server::Waits::default());
    manage(&user, "p1", "s-boss");
    let boss = at_place(&user, "p1");

    let e = boss.call("/api/template", Some(json!({ "text": MINE }))).unwrap_err().to_string();
    assert!(e.contains("only the user"), "{e}");
    let e = user.call("/api/template", Some(json!({ "text": "name: quick\ndescription: d\nsteps:\n  - id: fix\n    kind: build\n    description: d\n" }))).unwrap_err().to_string();
    assert!(e.contains("line 5: kind \"build\" is not one of"), "{e}");
    let e = user.append("", "template.set", json!({ "name": "quick", "description": "d", "steps": [], "source": MINE })).unwrap_err().to_string();
    assert!(e.contains("workriver template add"), "{e}");
    let saved = user.call("/api/template", Some(json!({ "text": MINE }))).unwrap();
    assert_eq!((saved["kind"].as_str(), saved["body"]["steps"].as_array().map(Vec::len)), (Some("template.set"), Some(2)));

    // Read back: the user's text as saved, comments and all; a built-in's text.
    let shown = boss.get("/api/template?name=quick").unwrap();
    assert_eq!((shown["text"].as_str(), shown["template"]["revision"].as_u64()), (Some(MINE), saved["seq"].as_u64()));
    let built = boss.get("/api/template?name=development").unwrap();
    assert!(built["text"].as_str().unwrap().contains("- id: go") && built["template"]["builtin"] == true, "{built}");
    assert!(boss.get("/api/template?name=nope").unwrap_err().to_string().contains("no template nope"));
    // Every reader gets the library: the user's and the built-ins, as values.
    for reads in [boss.get("/api/state").unwrap()["templates"].clone(), boss.get("/api/workspace").unwrap()["templates"].clone()] {
        let names: Vec<(String, bool)> = reads.as_array().unwrap().iter().map(|t| (t["name"].as_str().unwrap().to_string(), t["builtin"] == true)).collect();
        for want in [("quick", false), ("research", true), ("development", true), ("spike", true)] {
            assert!(names.contains(&(want.0.into(), want.1)), "{names:?}");
        }
    }
    // The user's of a built-in's name replaces it; removing it brings the built-in back.
    user.call("/api/template", Some(json!({ "text": "name: development\ndescription: Mine.\nsteps:\n  - id: do\n    kind: implement\n    description: Do it.\n" }))).unwrap();
    assert_eq!(boss.get("/api/template?name=development").unwrap()["template"]["steps"].as_array().map(Vec::len), Some(1));
    assert!(boss.call("/api/template/remove", Some(json!({ "name": "development" }))).unwrap_err().to_string().contains("only the user"));
    user.call("/api/template/remove", Some(json!({ "name": "development" }))).unwrap();
    assert_eq!(boss.get("/api/template?name=development").unwrap()["template"]["builtin"], true);
}

/// A unit's steps are placed from its template; the manager fills them (an agent named as
/// for `task add`), is warned when a gate goes ahead of steps not done or the unit closes
/// over them, and every notice it gets about the unit ends with the steps still to fill.
#[test]
fn the_manager_fills_and_is_reminded() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-dev", "dev", "w1", Idle))];
    let user = daemon_with(fake.clone(), &tmp.path().join("w.db"), crate::server::Waits::default());
    manage(&user, "p1", "s-boss");
    let (boss, dev) = (at_place(&user, "p1"), at_place(&user, "p2"));
    let unit = json!({ "intent": "i1", "title": "Faster", "repo": repo.to_string_lossy(), "branch": "feat/sync", "base": "main" });
    let e = boss.append("", "unit.created", unit.clone()).unwrap_err().to_string();
    assert!(e.contains("--template") && e.contains("development:"), "{e}");
    let mut with = unit.clone();
    with["template"] = json!("development");
    let made = boss.append("", "unit.created", with).unwrap();
    assert_eq!(made["body"]["placed"].as_array().map(Vec::len), Some(8));
    // Filled for an agent by its name, recorded by its session.
    let filled = boss.append("", "task.filled", json!({ "unit": "u1", "task": 1, "title": "Shape it", "agent": "dev" })).unwrap();
    assert_eq!(filled["body"]["agent"], "s-dev");
    assert!(filled.get("warning").is_none(), "no gate: {filled}");
    // A gate filled while what it lists is not done: said, not refused.
    let early = boss.append("", "task.filled", json!({ "unit": "u1", "task": 8, "title": "Merge feat/sync?" })).unwrap();
    let w = early["warning"].as_str().unwrap();
    assert!(w.contains("design (#1)") && w.contains("architecture-review (#7)") && w.contains("gate u1#8 lists them"), "{w}");
    assert!(!w.contains("confirm (#3)"), "gates aside: {w}");
    // A notice about the unit ends with its steps still to fill.
    dev.append("", "task.started", json!({ "unit": "u1", "task": 1 })).unwrap();
    dev.append("", "task.delivered", json!({ "unit": "u1", "task": 1, "note": "Batch the writes." })).unwrap();
    wait_until("the manager hears of the delivery", || fake.sent.lock().unwrap().iter().any(|(to, m)| to == "s-boss" && m.contains("delivered by dev")));
    let told = fake.sent.lock().unwrap().iter().rev().find(|(to, m)| to == "s-boss" && m.contains("delivered by dev")).unwrap().1.clone();
    assert!(told.ends_with("Steps of u1 still to fill: #2 design-review, #3 confirm, #4 implement, #5 code-review, #6 security-review, #7 architecture-review (`workriver task fill u1#<n>`, or `workriver task drop u1#<n> --reason \"...\"`)."), "{told}");
    // Closed over them: said too.
    let closed = boss.append("", "unit.done", json!({ "unit": "u1", "reason": "superseded" })).unwrap();
    assert!(closed["warning"].as_str().unwrap().contains("u1 is closed without them"), "{closed}");
    // What task show says of a step, and what a gate lists.
    let step: crate::common::wire::TaskShow = serde_json::from_value(boss.get("/api/task?task=u1%232").unwrap()).unwrap();
    let s = step.step.unwrap();
    assert_eq!((s.id.as_str(), s.template.as_str(), s.users, s.to_fill), ("design-review", "development", false, true));
    let gate: crate::common::wire::TaskShow = serde_json::from_value(boss.get("/api/task?task=u1%238").unwrap()).unwrap();
    let listed = gate.template_steps.unwrap();
    assert_eq!(listed.done.iter().map(|m| (m.id.as_str(), m.by.as_deref())).collect::<Vec<_>>(), [("design", Some("dev"))]);
    let page = user.get("/api/workspace").unwrap();
    let shown = &page["intents"][0]["units"][0];
    assert_eq!((shown["template"]["name"].as_str(), shown["template"]["builtin"].as_bool()), (Some("development"), Some(true)));
    assert_eq!(shown["tasks"][7]["gateSteps"]["done"][0]["by"], "dev");
    assert_eq!(shown["tasks"][1]["step"]["id"], "design-review");
}
