// Who decides an intent's project, where it runs and its manager, through the service: only
// the user, or a start the rules checked. Agents are found from their process (no `--as`).
// A workspace is workriver's own record: nothing the runtime says of its groups makes,
// moves or names one.
use serde_json::{Value, json};
use workriver_core::runtime::{AgentState::Idle, RuntimeAgent};

use super::support::*;
use crate::cli::Daemon;

fn labelled(session: &str, name: &str, id: &str, label: &str) -> RuntimeAgent {
    let mut a = live(session, name, id, Idle);
    a.group.as_mut().unwrap().label = label.into();
    a
}

/// An intent's state and note.
fn intent(d: &Daemon, id: &str) -> (String, String) {
    let ws = d.get("/api/workspace").unwrap();
    let i = ws["intents"].as_array().unwrap().iter().find(|i| i["id"] == id).unwrap().clone();
    (i["state"].as_str().unwrap().to_string(), i["note"].as_str().unwrap_or("").to_string())
}

/// A planned intent of the user's, with the user's words as its note.
fn planned(d: &Daemon, title: &str, project: Option<&str>) -> String {
    d.append("user", "intent.created", json!({ "title": title, "project": project.unwrap_or(""), "note": "The user's own words", "state": "planned" })).unwrap()["body"]["id"].as_str().unwrap().to_string()
}

/// Each workspace record as `id:name:manager:project:current`, closed ones marked.
fn records(d: &Daemon) -> Vec<String> {
    let s = d.get("/api/state").unwrap();
    let text = |v: &Value| v.as_str().unwrap_or("-").to_string();
    s["workspaces"].as_array().unwrap().iter().map(|w| format!("{}:{}:{}:{}:{}{}", text(&w["id"]), text(&w["name"]), text(&w["manager"]["session"]), text(&w["project"]), text(&w["current"]), if w["closed"] == true { ":closed" } else { "" })).collect()
}

fn repo(tmp: &std::path::Path, name: &str) -> String {
    let p = tmp.join(name);
    std::fs::create_dir(&p).unwrap();
    git(&p, &["init", "-q", "-b", "main"]);
    git(&p, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "init"]);
    p.canonicalize().unwrap().to_string_lossy().into_owned()
}

fn start(id: &str, workspace: Option<&str>) -> Value {
    json!({ "intent": id, "state": "active", "workspace": workspace })
}

/// The status and words an event sent from `place` is answered with.
fn answered(d: &Daemon, place: &str, kind: &str, body: Value) -> (u16, String) {
    http_at(d, d.port, place, "POST", "/api/append", &[], Some(&json!({ "kind": kind, "body": body }).to_string()))
}

/// A service that follows machine "box", as it runs for real (no `--as`): the runtimes read
/// through, the user's client, and a client of box's own listener.
fn following(tmp: &std::path::Path, local: std::sync::Arc<Fake>, far: std::sync::Arc<Fake>) -> (std::sync::Arc<crate::runtime::follow::Machines>, Daemon, Daemon) {
    let all = crate::runtime::follow::Machines::with(local, vec![("box", far)]);
    let (port, kernel) = daemon_over(all.clone(), Some(all.clone()), &tmp.join("w.db"));
    let user = kernel.client(port, tmp);
    wait_until("the machine's listener", || all.local_port("box").is_some());
    let there = kernel.daemon(all.local_port("box").unwrap(), None, None);
    (all, user, there)
}


/// boss (p1) manages workspace "chat" (ws1) with i1, other (p2, in a group labelled
/// "other") manages "other" (ws2) with i2, both of the test's project; a worker at p3.
fn two_managers(tmp: &std::path::Path) -> (std::sync::Arc<Fake>, Daemon, Daemon, Daemon, Daemon) {
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), labelled("s-other", "other", "w2", "other")), ("p3".into(), live("s-w", "w", "w1", Idle))];
    let d = daemon_with(fake.clone(), &tmp.join("w.db"), Default::default());
    manage(&d, "p1", "s-boss");
    manage(&d, "p2", "s-other");
    let (boss, other, worker) = (at_place(&d, "p1"), at_place(&d, "p2"), at_place(&d, "p3"));
    (fake, d, boss, other, worker)
}

/// The test's project folder, as the service records it.
fn project(tmp: &std::path::Path) -> String {
    tmp.join("proj").canonicalize().unwrap().to_string_lossy().into_owned()
}

/// A unit gives its intent no project, and a manager starts only its workspace's project's
/// intents. boss manages a workspace of the test's project and adds a unit in a repository
/// of project Q to its own intent: Q's planned intent stays the user's to start, and its
/// note the user's. A planned intent of no project takes no unit and no start from an agent.
#[test]
fn a_unit_gives_no_project() {
    let tmp = tempfile::tempdir().unwrap();
    let q = repo(tmp.path(), "q");
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    let own = manage(&d, "p1", "s-boss");
    d.append("user", "project.added", json!({ "path": q, "title": "Q" })).unwrap();
    let theirs = planned(&d, "Theirs", Some(&q));
    let old = planned(&d, "Old goal", None);
    let boss = at_place(&d, "p1");
    let other_project = |e: String| assert!(e.contains(&format!("{theirs} belongs to project Q; workspace ws1 (chat) is of project chat: an intent runs in a workspace of its project")), "{e}");
    other_project(boss.append("", "intent.state_changed", start(&theirs, None)).unwrap_err().to_string());
    let unit = |i: &str| json!({ "intent": i, "title": "Decoy", "repo": q, "branch": "feat/decoy", "base": "main" });
    boss.append("", "unit.created", unit(&own)).expect("its manager adds its units");
    other_project(boss.append("", "intent.state_changed", start(&theirs, None)).unwrap_err().to_string());
    assert!(boss.append("", "intent.updated", json!({ "intent": theirs, "add_note": "x" })).is_err());
    let e = boss.append("", "unit.created", unit(&old)).unwrap_err().to_string();
    assert!(e.contains(&format!("{old} has no manager: units are added by the manager of the workspace it runs in, once it is started, or by the user")), "{e}");
    let e = boss.append("", "intent.state_changed", start(&old, None)).unwrap_err().to_string();
    assert!(e.contains("has no project: ask the user to start it"), "{e}");
    for id in [&theirs, &old] {
        assert_eq!(intent(&d, id), ("planned".to_string(), "The user's own words".to_string()));
    }
    // The user starts the one of no project where they say: it takes that workspace's project.
    d.append("user", "intent.state_changed", start(&old, Some("ws1"))).unwrap();
    let state = d.get("/api/state").unwrap();
    assert_eq!(state["intents"].as_array().unwrap().iter().find(|i| i["id"] == old.as_str()).unwrap()["project"], project(tmp.path()));
}

/// A manager closes, shelves or drops its own intent naming another workspace, which would
/// move it there: refused. Only a start names a workspace.
#[test]
fn a_close_names_no_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    let (_fake, d, boss, _other, _worker) = two_managers(tmp.path());
    let before = records(&d);
    for to in ["done", "planned", "dropped"] {
        let e = boss.append("", "intent.state_changed", json!({ "intent": "i1", "state": to, "workspace": "ws2", "reason": "r" })).unwrap_err().to_string();
        assert!(e.contains("a state change names a workspace only when it starts an intent"), "{to}: {e}");
    }
    assert_eq!(intent(&d, "i1").0, "active");
    assert_eq!(records(&d), before);
}

/// No agent sets a seat, a workspace or a project: not a manager, not a worker, through
/// `/api/append` (each of the kinds) or through the user's own route. That a manager has
/// its seat back is workriver's to record, nobody else's. Nothing is recorded of any of it.
#[test]
fn no_agent_sets_a_seat_a_workspace_or_a_project() {
    let tmp = tempfile::tempdir().unwrap();
    let (fake, d, ..) = two_managers(tmp.path());
    let solo = repo(tmp.path(), "solo");
    let before = (records(&d), d.get("/api/state").unwrap()["projects"].clone(), fake.sent.lock().unwrap().len());
    let seat_now = seat_of(&d, "ws1");
    for place in ["p1", "p2", "p3"] {
        let events = [
            ("workspace.created", json!({ "name": "mine", "project": "p1" })),
            ("workspace.renamed", json!({ "workspace": "ws1", "name": "mine" })),
            ("workspace.closed", json!({ "workspace": "ws1" })),
            ("workspace.opened", json!({ "workspace": "ws1" })),
            ("workspace.manager_set", json!({ "workspace": "ws1", "manager": "s-w", "replaces": seat_now })),
            ("workspace.manager_set", json!({ "workspace": "ws1", "manager": null, "replaces": seat_now })),
            ("intent.workspace_set", json!({ "intent": "i1", "workspace": "ws2" })),
            ("project.added", json!({ "path": solo, "title": "Solo" })),
            ("project.renamed", json!({ "project": "p1", "title": "Mine" })),
        ];
        for (kind, body) in events {
            let (code, text) = answered(&d, place, kind, body);
            assert!(code == 422 && text.contains(&format!("only the user does this ({kind})")), "{place} {kind}: {code} {text}");
        }
        let (code, text) = answered(&d, place, "workspace.manager_back", json!({ "workspace": "ws1", "session": "s-boss" }));
        assert!(code == 422 && text.contains("only workriver records this (workspace.manager_back)"), "{place}: {code} {text}");
        let acts = [
            json!({ "action": "add", "project": "p1", "name": "mine" }),
            json!({ "action": "rename", "workspace": "ws1", "name": "mine" }),
            json!({ "action": "close", "workspace": "ws2" }),
            json!({ "action": "open", "workspace": "ws2" }),
            json!({ "action": "manager", "workspace": "ws1", "manager": "s-w", "replaces": seat_now }),
            json!({ "action": "manager", "workspace": "ws1", "manager": null, "replaces": seat_now }),
            json!({ "action": "move", "intent": "i1", "workspace": "ws2" }),
        ];
        for act in acts {
            let (code, text) = from_place(&d, place, "POST", "/api/workspaces", Some(&act));
            assert!(code == 403 && text.contains("only the user does this"), "{place} {act}: {code} {text}");
        }
    }
    // Not the user's to say either: it says what the service saw of a terminal.
    let e = d.append("user", "workspace.manager_back", json!({ "workspace": "ws1", "session": "s-boss" })).unwrap_err().to_string();
    assert!(e.contains("only workriver records this (workspace.manager_back)"), "{e}");
    assert_eq!((records(&d), d.get("/api/state").unwrap()["projects"].clone(), fake.sent.lock().unwrap().len()), before, "nothing recorded, nobody told");
}

/// Nothing the runtime does to its groups changes a record: a group given a new id and
/// another name, a new group under the old id and name with an agent of its own, every
/// agent gone. The workspace, its seat, its intent and its conversation stay as recorded;
/// the agent in the look-alike group is nobody to it.
#[test]
fn a_runtime_group_made_renamed_or_closed_changes_no_record() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    manage(&d, "p1", "s-boss");
    d.append("s-boss", "message.said", json!({ "text": "Started." })).unwrap();
    let before = (records(&d), d.get("/api/state").unwrap()["messages"].clone());
    assert_eq!(before.0, ["ws1:chat:s-boss:p1:i1"]);
    // boss's group gets a new id and another name; a new group takes the old id and name.
    *fake.agents.lock().unwrap() = vec![("p1".into(), labelled("s-boss", "boss", "w9", "renamed")), ("p2".into(), live("s-new", "new", "w1", Idle))];
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a poll reads the runtime
    let (boss, new) = (at_place(&d, "p1"), at_place(&d, "p2"));
    let mine = boss.call("/api/here", Some(json!({}))).unwrap();
    assert_eq!((mine["workspace"]["id"].as_str(), mine["manager"].as_bool(), mine["intent"].as_str()), (Some("ws1"), Some(true), Some("i1")), "{mine}");
    let theirs = new.call("/api/here", Some(json!({}))).unwrap();
    assert_eq!((theirs["workspace"].is_null(), theirs["manager"].as_bool(), theirs["intent"].is_null()), (true, Some(false), true), "{theirs}");
    let e = new.append("", "message.said", json!({ "text": "Mine now" })).unwrap_err().to_string();
    assert!(e.contains("only a workspace's manager says something to the user"), "{e}");
    let next = planned(&d, "Next", Some(&project(tmp.path())));
    let e = new.append("", "intent.state_changed", start(&next, None)).unwrap_err().to_string();
    assert!(e.contains("only a workspace's manager, or the user, starts an intent"), "{e}");
    assert!(new.append("", "intent.state_changed", json!({ "intent": "i1", "state": "done" })).is_err());
    assert!(!new.get("/api/state").unwrap().to_string().contains("Started."), "nor does it read the conversation");
    boss.append("", "intent.state_changed", start(&next, None)).expect("the manager still is");
    assert_eq!(records(&d), [format!("ws1:chat:s-boss:p1:{next}")]);
    // Every group closed: the records stand, the page shows the manager not running.
    fake.agents.lock().unwrap().clear();
    std::thread::sleep(std::time::Duration::from_millis(2500));
    assert_eq!(records(&d), [format!("ws1:chat:s-boss:p1:{next}")]);
    assert_eq!(d.get("/api/state").unwrap()["messages"], before.1);
    assert_eq!(d.get("/api/workspace").unwrap()["workspaces"][0]["manager"]["state"], "gone");
}

/// A repository is part of a registered project or of none: an intent or a deferred finding
/// naming a folder no registered project holds is refused, whoever records it, here and on
/// another machine, saying what to do. Nothing registers a folder but the user.
#[test]
fn a_folder_no_registered_project_holds_is_no_project() {
    let tmp = tempfile::tempdir().unwrap();
    let solo = repo(tmp.path(), "solo");
    let local = std::sync::Arc::new(Fake::default());
    *local.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle))];
    let far = std::sync::Arc::new(Fake::default());
    *far.agents.lock().unwrap() = vec![("p1".into(), live("s-far", "far", "w1", Idle))];
    let (_all, user, there) = following(tmp.path(), local, far);
    let (boss, far) = (at_place(&user, "p1"), at_place(&there, "p1"));
    let finding = |project: &str| json!({ "project": project, "finding": { "severity": "P3", "problem": "x", "impact": "y", "where": ["a"] } });
    let goal = |project: &str| json!({ "title": "Decoy", "project": project, "state": "planned" });
    let no = "is in no registered project: name a project by its id (`workriver project list`), or ask the user to add it first";
    for (who, d) in [("the user", &user), ("an agent", &boss), ("an agent on another machine", &far)] {
        for folder in [solo.as_str(), "/Users/someone/.ssh"] {
            for (kind, body) in [("intent.created", goal(folder)), ("finding.added", finding(folder))] {
                let e = d.append("", kind, body).unwrap_err().to_string();
                assert!(e.contains(no) || (folder.contains("someone") && e.contains("no such id or folder")), "{who}, {kind} {folder}: {e}");
            }
        }
    }
    for d in [&boss, &far] {
        let e = d.append("", "project.added", json!({ "path": solo, "title": "Solo" })).unwrap_err().to_string();
        assert!(e.contains("only the user does this (project.added)") || e.contains("register project folders on the machine workriver runs on"), "{e}");
    }
    let state = user.get("/api/state").unwrap();
    assert!(state["projects"].as_array().unwrap().is_empty() && state["intents"].as_array().unwrap().is_empty() && state["backlog"].as_array().is_none_or(|b| b.is_empty()), "{state}");
    // Of no project, anyone may plan; once the user registers the folder, it is a project:
    // here by its folder or id, on another machine by its id.
    boss.append("", "intent.created", json!({ "title": "Of none", "state": "planned" })).unwrap();
    user.append("", "project.added", json!({ "path": solo, "title": "Solo" })).unwrap();
    boss.append("", "intent.created", goal(&solo)).unwrap();
    far.append("", "intent.created", goal("p1")).unwrap();
    far.append("", "finding.added", finding("p1")).unwrap();
    let state = user.get("/api/state").unwrap();
    let projects: Vec<&str> = state["intents"].as_array().unwrap().iter().map(|i| i["project"].as_str().unwrap()).collect();
    assert_eq!(projects, ["", solo.as_str(), solo.as_str()]);
    assert_eq!(state["workspaces"].as_array().unwrap().len(), 1, "the project came with its first workspace");
}

/// A start: a manager starts only in the workspace it holds the seat of, which its start
/// need not name (the rules fill it in) and may not name otherwise; an agent that holds no
/// seat starts nothing. The user says where, or it is the one open workspace of the
/// intent's project; never a workspace of another project.
#[test]
fn a_start_is_in_the_seated_workspace_or_where_the_user_says() {
    let tmp = tempfile::tempdir().unwrap();
    let (_fake, d, _boss, other, worker) = two_managers(tmp.path());
    let proj = project(tmp.path());
    let next = planned(&d, "Next", Some(&proj));
    let e = other.append("", "intent.state_changed", start(&next, Some("ws1"))).unwrap_err().to_string();
    assert!(e.contains("you manage workspace ws2 (other): a manager starts intents there, and nowhere else"), "{e}");
    let e = worker.append("", "intent.state_changed", start(&next, Some("ws1"))).unwrap_err().to_string();
    assert!(e.contains("only a workspace's manager, or the user, starts an intent"), "{e}");
    let started = other.append("", "intent.state_changed", start(&next, None)).unwrap();
    assert_eq!(started["body"]["workspace"], "ws2", "filled in from its seat");
    // The user, with two open workspaces of the project, says where.
    let more = planned(&d, "More", Some(&proj));
    let e = d.append("user", "intent.state_changed", start(&more, None)).unwrap_err().to_string();
    assert!(e.contains(&format!("say where {more} starts")), "{e}");
    assert!(d.append("user", "intent.state_changed", start(&more, Some("ws9"))).unwrap_err().to_string().contains("no workspace ws9"));
    // Another project has one open workspace: the user's start takes it; and no intent
    // runs in a workspace of another project, the user's start included.
    let q = repo(tmp.path(), "q");
    d.append("user", "project.added", json!({ "path": q, "title": "Q" })).unwrap();
    let theirs = planned(&d, "Theirs", Some(&q));
    let e = d.append("user", "intent.state_changed", start(&theirs, Some("ws1"))).unwrap_err().to_string();
    assert!(e.contains("an intent runs in a workspace of its project"), "{e}");
    let started = d.append("user", "intent.state_changed", start(&theirs, None)).unwrap();
    assert_eq!(started["body"]["workspace"], "ws3");
    let e = d.call("/api/workspaces", Some(json!({ "action": "move", "intent": theirs, "workspace": "ws1" }))).unwrap_err().to_string();
    assert!(e.contains("an intent runs in a workspace of its project"), "{e}");
    // The user moves an active intent to another workspace of its project: the manager there has it.
    d.call("/api/workspaces", Some(json!({ "action": "move", "intent": "i1", "workspace": "ws2" }))).unwrap();
    other.append("", "intent.state_changed", json!({ "intent": "i1", "state": "done" })).expect("its manager now");
}

/// A workspace with an active intent is not closed. Closed, it has no manager, takes no
/// start and no message; its intents and conversation stay; the user opens it again.
#[test]
fn a_workspace_is_closed_only_without_an_active_intent() {
    let tmp = tempfile::tempdir().unwrap();
    let (_fake, d, boss, ..) = two_managers(tmp.path());
    boss.append("", "message.said", json!({ "text": "Kept." })).unwrap();
    let close = json!({ "action": "close", "workspace": "ws1" });
    let (code, text) = as_user(&d, "POST", "/api/workspaces", Some(&close));
    assert!(code == 422 && text.contains("ws1 has an active intent (i1)"), "{code} {text}");
    d.append("user", "intent.state_changed", json!({ "intent": "i1", "state": "done" })).unwrap();
    d.call("/api/workspaces", Some(close)).unwrap();
    assert_eq!(records(&d)[0], "ws1:chat:-:p1:-:closed", "its seat is emptied");
    assert_eq!(boss.call("/api/here", Some(json!({}))).unwrap()["manager"], false);
    let next = planned(&d, "Next", Some(&project(tmp.path())));
    assert!(d.append("user", "intent.state_changed", start(&next, Some("ws1"))).unwrap_err().to_string().contains("workspace ws1 is closed"));
    assert!(d.call("/api/message", Some(json!({ "workspace": "ws1", "text": "Anyone?" }))).unwrap_err().to_string().contains("workspace ws1 has no manager"));
    assert!(seat(&d, "ws1", Some("s-boss")).unwrap_err().to_string().contains("ws1 is closed: open it first"));
    let state = d.get("/api/state").unwrap();
    assert!(state["messages"].to_string().contains("Kept.") && state["intents"][0]["workspace"] == "ws1", "its conversation and intents stay");
    d.call("/api/workspaces", Some(json!({ "action": "open", "workspace": "ws1" }))).unwrap();
    seat(&d, "ws1", Some("s-boss")).unwrap();
    assert_eq!(records(&d)[0], "ws1:chat:s-boss:p1:-");
}

/// An agent's workspace, wherever a command leaves the intent out, is the records': the
/// seat it holds; else the workspace of the tasks it has started, when that is one. A
/// worker with tasks running in two workspaces is in neither: it is asked for the intent.
#[test]
fn an_agents_workspace_is_the_one_its_tasks_are_in_when_that_is_one() {
    let tmp = tempfile::tempdir().unwrap();
    let (_fake, d, boss, other, worker) = two_managers(tmp.path());
    for (manager, intent) in [(&boss, "i1"), (&other, "i2")] {
        let u = manager.append("", "unit.created", json!({ "intent": intent, "title": "Survey", "kind": "research" })).unwrap()["body"]["id"].as_str().unwrap().to_string();
        manager.append("", "task.created", json!({ "unit": u, "kind": "research", "title": "Read", "agent": "s-w" })).unwrap();
    }
    let here = || worker.call("/api/here", Some(json!({}))).unwrap();
    let at = |h: &Value| (h["workspace"]["id"].as_str().map(str::to_string), h["intent"].as_str().map(str::to_string), h["manager"].as_bool());
    assert_eq!(at(&here()), (None, None, Some(false)), "given, not started: in none");
    worker.append("", "task.started", json!({ "unit": "u1", "task": 1 })).unwrap();
    assert_eq!(at(&here()), (Some("ws1".into()), Some("i1".into()), Some(false)));
    worker.append("", "task.started", json!({ "unit": "u2", "task": 1 })).unwrap();
    assert_eq!(at(&here()), (None, None, Some(false)), "in two: nothing picks one");
    worker.append("", "task.delivered", json!({ "unit": "u1", "task": 1, "note": "Read." })).unwrap();
    assert_eq!(at(&here()), (Some("ws2".into()), Some("i2".into()), Some(false)));
    // The page shows each agent's the same way.
    let ws = d.get("/api/workspace").unwrap();
    let session = |name: &str| ws["sessions"].as_array().unwrap().iter().find(|s| s["agent"] == name).unwrap().clone();
    assert_eq!((session("w")["workspace"].as_str(), session("boss")["workspace"].as_str(), session("other")["workspace"].as_str()), (Some("ws2"), Some("ws1"), Some("ws2")));
}
