// A project added and a manager started from the page (the user's click): what is opened and
// started is built from the records and workriver's own table, never from the request; what
// the start brings up is seated once, or not, with why; a restart ends what was pending.
use serde_json::{Value, json};
use workriver_core::runtime::{AgentState::*, HarnessState};

use super::support::*;
use crate::cli::Daemon;

/// Short waits: a start waits 3 s for a session, a try does not wait to read its screen.
fn waits() -> crate::server::Waits {
    crate::server::Waits { start: std::time::Duration::from_secs(3), settle: std::time::Duration::ZERO, ..Default::default() }
}

fn start(d: &Daemon, ws: &str, model: &str) -> anyhow::Result<Value> {
    d.call("/api/workspaces", Some(json!({ "action": "start", "workspace": ws, "harness": "claude", "model": model, "replaces": seat_of(d, ws) })))
}

/// The workspace's line as the page reads it.
fn line(d: &Daemon, id: &str) -> Value {
    d.get("/api/workspace").unwrap()["workspaces"].as_array().unwrap().iter().find(|w| w["id"] == id).unwrap().clone()
}

fn told(fake: &Fake, to: &str, what: &str) -> usize {
    fake.sent.lock().unwrap().iter().filter(|(t, m)| t == to && m.contains(what)).count()
}

fn project_folder(d: &Daemon) -> String {
    d.get("/api/state").unwrap()["projects"][0]["path"].as_str().unwrap().to_string()
}

/// The click opens a place in the project's folder, in the runtime's group named as the
/// workspace, records the start with it, starts the table's harness there with the table's
/// arguments, and seats what it brings up once, told once. Nothing else in the request is
/// read: a folder, arguments or a place it names are not.
#[test]
fn a_start_opens_a_place_in_the_projects_folder_and_seats_what_it_brings_up() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    let db = tmp.path().join("w.db");
    let d = daemon_with(fake.clone(), &db, waits());
    let ws = workspace_named(&d, "chat");
    let folder = project_folder(&d);
    let asked = json!({ "action": "start", "workspace": ws, "harness": "claude", "model": "opus", "replaces": null, "folder": "/etc", "args": ["--dangerously-skip-permissions"], "place": "w9:p9", "name": "boss" });
    let e = d.call("/api/workspaces", Some(asked)).unwrap();
    assert_eq!((e["kind"].as_str(), e["actor"].as_str()), (Some("workspace.manager_started"), Some("user")));
    let b = &e["body"];
    assert_eq!((b["folder"].as_str(), b["name"].as_str(), b["place"].as_str(), b["harness"].as_str(), b["model"].as_str()), (Some(folder.as_str()), Some("ws1-manager"), Some("opened-1"), Some("claude"), Some("opus")));
    assert_eq!(fake.opened.lock().unwrap().clone(), [("chat".to_string(), folder.clone(), "opened-1".to_string())]);
    wait_until("seated", || seat_of(&d, &ws)["session"] == "s-ws1-manager");
    assert_eq!(fake.started_in.lock().unwrap().clone(), [("opened-1".to_string(), "claude".to_string(), "ws1-manager".to_string(), vec!["--model".to_string(), "opus".to_string()])]);
    wait_until("told once", || told(&fake, "s-ws1-manager", "The user made you the manager of workspace ws1") == 1);
    let seated = bodies(&db, "workspace.manager_start_seated");
    assert_eq!(seated.len(), 1);
    assert_eq!((seated[0]["folder"].as_str(), seated[0]["place"].as_str(), seated[0]["start"].as_u64()), (Some(folder.as_str()), Some("opened-1"), e["seq"].as_u64()));
    // Seen before it was seated, by the service.
    let seen = stored(&db).iter().position(|e| matches!(&e.body, workriver_core::Body::AgentSeen { session, .. } if session == "s-ws1-manager")).unwrap();
    let at = stored(&db).iter().position(|e| matches!(e.body, workriver_core::Body::ManagerStartSeated { .. })).unwrap();
    assert!(seen < at);
    assert!(line(&d, &ws).get("start").is_none(), "the seating ended the start");
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert_eq!(told(&fake, "s-ws1-manager", "The user made you the manager"), 1, "told once");
    assert!(fake.closed_places.lock().unwrap().is_empty());
}

/// Only a harness of the table that is offered here, with a model of the forms a start takes;
/// anything else is refused before anything is opened or recorded.
#[test]
fn a_start_runs_only_what_the_table_offers() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    let db = tmp.path().join("w.db");
    let d = daemon_with(fake.clone(), &db, waits());
    let ws = workspace_named(&d, "chat");
    let ask = |harness: &str, model: &str| as_user(&d, "POST", "/api/workspaces", Some(&json!({ "action": "start", "workspace": ws, "harness": harness, "model": model, "replaces": null })));
    let (code, text) = ask("bash", "");
    assert!(code == 422 && text.contains("no harness \"bash\""), "{code} {text}");
    for model in ["--dangerously-skip-permissions", "opus sonnet", "$(id)"] {
        let (code, text) = ask("claude", model);
        assert!(code == 422 && text.contains("is not a model"), "{model}: {code} {text}");
    }
    *fake.harness_states.lock().unwrap() = Some(vec![HarnessState { kind: "claude".into(), installed: true, integrated: true }, HarnessState { kind: "codex".into(), installed: true, integrated: false }]);
    let (code, text) = ask("codex", "");
    assert!(code == 422 && text.contains("integration"), "{code} {text}");
    assert!(fake.opened.lock().unwrap().is_empty() && fake.started_in.lock().unwrap().is_empty());
    assert!(bodies(&db, "workspace.manager_started").is_empty());
    // Through the CLI's own entrance it is refused too: only the start action opens a place.
    let e = d.append("", "workspace.manager_started", json!({ "workspace": ws, "harness": "claude", "place": "w9:p9", "shell": { "pid": 1, "start": 1 } })).unwrap_err().to_string();
    assert!(e.contains("a manager is started only by the start action"), "{e}");
    assert!(line(&d, &ws).get("start").is_none() && bodies(&db, "workspace.manager_started").is_empty());
}

/// One pending start per workspace: a second click is refused (409); two at once leave one
/// place open and one start recorded, the refused one's place closed.
#[test]
fn one_start_at_a_time() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.bring.lock().unwrap() = Bring::Pending;
    let db = tmp.path().join("w.db");
    let d = daemon_with(fake.clone(), &db, crate::server::Waits { start: std::time::Duration::from_secs(600), ..waits() });
    let ws = workspace_named(&d, "chat");
    start(&d, &ws, "").unwrap();
    let (code, text) = as_user(&d, "POST", "/api/workspaces", Some(&json!({ "action": "start", "workspace": ws, "harness": "claude", "replaces": null })));
    assert!(code == 409 && text == "a manager is being started for ws1: wait for it, or seat one yourself", "{code} {text}");
    let side = workspace_named(&d, "side");
    let both: Vec<_> = (0..2)
        .map(|_| {
            let (d, side) = (d.clone(), side.clone());
            std::thread::spawn(move || as_user(&d, "POST", "/api/workspaces", Some(&json!({ "action": "start", "workspace": side, "harness": "claude", "replaces": null }))).0)
        })
        .collect();
    let mut codes: Vec<u16> = both.into_iter().map(|t| t.join().unwrap()).collect();
    codes.sort();
    assert_eq!(codes, [200, 409]);
    let theirs: Vec<String> = fake.opened.lock().unwrap().iter().filter(|(g, ..)| g == "side").map(|(.., p)| p.clone()).collect();
    let closed = fake.closed_places.lock().unwrap().clone();
    assert_eq!(theirs.iter().filter(|p| !closed.contains(p)).count(), 1, "one place left: {theirs:?} closed {closed:?}");
    assert_eq!(bodies(&db, "workspace.manager_started").iter().filter(|b| b["workspace"] == side).count(), 1);
}

/// A harness that waits for a person (its first-run question) before it has a session: the
/// place is kept and the wait goes on; once it has a session it is seated, once.
#[test]
fn a_start_waits_through_its_question_and_seats_once() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.bring.lock().unwrap() = Bring::Pending;
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), crate::server::Waits { start: std::time::Duration::from_secs(600), ..waits() });
    let ws = workspace_named(&d, "chat");
    start(&d, &ws, "").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500));
    let w = line(&d, &ws);
    assert!(seat_of(&d, &ws).is_null() && w["start"]["reason"].is_null() && w["start"]["name"] == "ws1-manager", "pending: {w}");
    assert!(fake.closed_places.lock().unwrap().is_empty() && fake.closed.lock().unwrap().is_empty(), "the place is kept");
    // Found on the page by its name while it has no session: blocked, Needs you.
    let sessions = d.get("/api/workspace").unwrap()["sessions"].clone();
    assert!(sessions.as_array().unwrap().iter().any(|s| s["agent"] == "ws1-manager" && s["blockedSince"].is_string()), "{sessions}");
    fake.give_session("opened-1", "s-real");
    wait_until("seated", || seat_of(&d, &ws)["session"] == "s-real");
    wait_until("told", || told(&fake, "s-real", "The user made you the manager of workspace ws1") == 1);
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert_eq!(told(&fake, "s-real", "The user made you the manager"), 1);
}

/// Nothing shows up in the place, or the runtime refuses the start: ended with the reason,
/// the place closed (it is still an empty shell), nobody seated; an ended start does not
/// block the next one.
#[test]
fn a_start_that_brings_nothing_up_ends_and_closes_its_place() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.bring.lock().unwrap() = Bring::Nothing;
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), waits());
    let ws = workspace_named(&d, "chat");
    start(&d, &ws, "").unwrap();
    wait_until("ended", || line(&d, &ws)["start"]["reason"] == "no_agent");
    assert_eq!(fake.closed_places.lock().unwrap().clone(), ["opened-1"]);
    *fake.bring.lock().unwrap() = Bring::Refuse("the runtime: no such agent kind".into());
    start(&d, &ws, "").unwrap();
    wait_until("ended", || line(&d, &ws)["start"]["reason"] == "runtime");
    let w = line(&d, &ws);
    assert!(w["start"]["words"].as_str().unwrap().contains("no such agent kind"), "{w}");
    assert_eq!(fake.closed_places.lock().unwrap().clone(), ["opened-1", "opened-2"]);
    assert!(seat_of(&d, &ws).is_null());
}

/// The user seats someone while a start is pending: the start is over, and what it brings up
/// later is not seated (it is left running). One with no session in time ends `no_session`,
/// its place left for the user.
#[test]
fn a_start_seats_nobody_after_the_seat_changed_or_its_time() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle))];
    *fake.bring.lock().unwrap() = Bring::Pending;
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), waits());
    let ws = workspace_named(&d, "chat");
    start(&d, &ws, "").unwrap();
    seat(&d, &ws, Some("s-boss")).unwrap();
    assert!(line(&d, &ws).get("start").is_none());
    fake.give_session("opened-1", "s-late");
    std::thread::sleep(std::time::Duration::from_millis(2000));
    assert_eq!(seat_of(&d, &ws)["session"], "s-boss");
    assert_eq!(told(&fake, "s-late", ""), 0);
    assert!(fake.closed.lock().unwrap().is_empty() && fake.closed_places.lock().unwrap().is_empty(), "left running");
    // Never a session within its time.
    let side = workspace_named(&d, "side");
    start(&d, &side, "").unwrap();
    wait_until("ended", || line(&d, &side)["start"]["reason"] == "no_session");
    assert!(seat_of(&d, &side).is_null());
    assert!(fake.closed_places.lock().unwrap().is_empty(), "its place is left: the user may be in its dialog");
}

/// A start over a manager that runs: once seated, the one replaced is told it no longer
/// manages, and a message held for it (it waited for a person) is never typed into either.
#[test]
fn a_start_over_a_running_manager_ends_its_appointment() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle))];
    *fake.bring.lock().unwrap() = Bring::Pending;
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), crate::server::Waits { start: std::time::Duration::from_secs(600), ..waits() });
    manage(&d, "p1", "s-boss");
    fake.agents.lock().unwrap()[0].1.state = Blocked;
    d.call("/api/message", Some(json!({ "workspace": "ws1", "text": "Queued for boss" }))).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500));
    start(&d, "ws1", "").unwrap();
    fake.give_session("opened-1", "s-new");
    wait_until("seated", || seat_of(&d, "ws1")["session"] == "s-new");
    fake.agents.lock().unwrap()[0].1.state = Idle;
    let message = || d.get("/api/workspace").unwrap()["messages"].as_array().unwrap().iter().find(|m| m["text"] == "Queued for boss").unwrap().clone();
    wait_until("not delivered", || message()["step"] == "not-delivered");
    assert_eq!(message()["reasonCode"], "seat-changed");
    std::thread::sleep(std::time::Duration::from_millis(2500));
    assert_eq!((told(&fake, "s-boss", "Queued for boss"), told(&fake, "s-new", "Queued for boss")), (0, 0));
    // Over a manager that runs and is free: it is told it no longer manages, as for a seating.
    wait_until("s-new told", || told(&fake, "s-new", "The user made you the manager of workspace ws1") == 1);
    start(&d, "ws1", "").unwrap();
    fake.give_session("opened-2", "s-third");
    wait_until("seated", || seat_of(&d, "ws1")["session"] == "s-third");
    wait_until("the one replaced is told", || told(&fake, "s-new", "You are no longer the manager of workspace ws1") == 1);
}

/// A start still pending when the service stops has lost its wait: the next start-up ends it
/// before it serves, and seats nobody after; a new start may be made.
#[test]
fn a_restart_ends_a_pending_start() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("w.db");
    let fake = std::sync::Arc::new(Fake::default());
    *fake.bring.lock().unwrap() = Bring::Pending;
    let d = daemon_with(fake.clone(), &db, crate::server::Waits { start: std::time::Duration::from_secs(600), ..waits() });
    let ws = workspace_named(&d, "chat");
    start(&d, &ws, "").unwrap();
    drop(d);
    let next = std::sync::Arc::new(Fake::default());
    *next.bring.lock().unwrap() = Bring::Pending;
    *next.agents.lock().unwrap() = fake.agents.lock().unwrap().clone();
    let d = daemon_with(next.clone(), &db, waits());
    assert_eq!(line(&d, &ws)["start"]["reason"], "restarted", "ended before it serves");
    next.give_session("opened-1", "s-late");
    std::thread::sleep(std::time::Duration::from_millis(2000));
    assert!(seat_of(&d, &ws).is_null());
    next.agents.lock().unwrap().clear();
    start(&d, &ws, "").unwrap();
    assert!(line(&d, &ws)["start"]["reason"].is_null());
}

/// A project's folder is a whole path to a folder: from / or ~/ (the service's home), never
/// read against where the service was started; a file, and a folder inside a project or
/// holding one, are refused, and nothing is registered.
#[test]
fn a_project_is_a_whole_path_to_a_folder_of_its_own() {
    let tmp = tempfile::tempdir().unwrap();
    let d = daemon_with(std::sync::Arc::new(Fake::default()), &tmp.path().join("w.db"), waits());
    let add = |path: &str| d.append("", "project.added", json!({ "path": path, "title": "" })).map_err(|e| e.to_string());
    for path in [".", "foo", "../x", "~", "~name", ""] {
        let e = add(path).unwrap_err();
        assert!(e.contains("give the folder's whole path, from / or ~/"), "{path:?}: {e}");
    }
    let file = tmp.path().join("notes.txt");
    std::fs::write(&file, "x").unwrap();
    assert!(add(&file.to_string_lossy()).unwrap_err().contains("is a file: a project is a folder"));
    // ~/ is the service's home: nothing is made there, its refusal names the folder looked for.
    let home = std::env::home_dir().unwrap();
    let e = add("~/workriver-no-such-folder-probe").unwrap_err();
    assert!(e.contains(&home.join("workriver-no-such-folder-probe").to_string_lossy().into_owned()), "{e}");
    assert!(d.get("/api/state").unwrap()["projects"].as_array().is_none_or(|p| p.is_empty()));
    let outer = tmp.path().join("outer");
    std::fs::create_dir_all(outer.join("inner")).unwrap();
    let made = add(&outer.to_string_lossy()).unwrap();
    assert_eq!(made["body"]["title"], "outer", "the folder's name when no title is given");
    assert!(add(&outer.join("inner").to_string_lossy()).unwrap_err().contains("is inside project p1"));
    assert_eq!(d.get("/api/state").unwrap()["workspaces"].as_array().unwrap().len(), 1, "its first workspace with it");
}

/// The default harness and model: the user's, kept in the settings, offered harnesses only
/// and models of a start's forms. Try it starts one once in a temporary folder of its own,
/// seats nobody, records nothing, and closes and removes what it opened.
#[test]
fn the_default_harness_is_set_and_tried_by_the_user() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    let db = tmp.path().join("w.db");
    let d = daemon_with(fake.clone(), &db, waits());
    let got = d.get("/api/settings/start").unwrap();
    assert_eq!((got["harness"].as_str(), got["model"].as_str()), (Some("claude"), Some("")));
    assert_eq!(got["harnesses"], json!([{ "kind": "claude", "aliases": ["opus", "sonnet", "fable"] }, { "kind": "codex", "aliases": [] }]));
    let set = d.call("/api/settings/start", Some(json!({ "harness": "codex", "model": "gpt-6.2-sol" }))).unwrap();
    assert_eq!((set["harness"].as_str(), set["model"].as_str()), (Some("codex"), Some("gpt-6.2-sol")));
    let (code, text) = as_user(&d, "POST", "/api/settings/start", Some(&json!({ "harness": "claude", "model": "a b" })));
    assert!(code == 422 && text.contains("is not a model"), "{code} {text}");
    *fake.harness_states.lock().unwrap() = Some(vec![HarnessState { kind: "claude".into(), installed: false, integrated: false }]);
    assert_eq!(d.get("/api/settings/start").unwrap()["harnesses"][0]["why"], "not_installed");
    assert_eq!(d.get("/api/settings/start").unwrap()["harnesses"][1]["why"], "not_installed");
    assert_eq!(as_user(&d, "POST", "/api/settings/start", Some(&json!({ "harness": "claude", "model": "" }))).0, 422);
    assert_eq!(d.get("/api/settings/start").unwrap()["harness"], "codex", "unchanged");
    *fake.harness_states.lock().unwrap() = None;

    let tried = d.call("/api/settings/start/try", Some(json!({ "harness": "claude", "model": "opus" }))).unwrap();
    assert_eq!(tried["ok"], true, "{tried}");
    let (group, dir, place) = fake.opened.lock().unwrap()[0].clone();
    assert!(group == "workriver-try" && !std::path::Path::new(&dir).exists(), "{group} {dir}");
    let (_, kind, name, args) = fake.started_in.lock().unwrap()[0].clone();
    assert_eq!((kind.as_str(), args), ("claude", vec!["--model".to_string(), "opus".to_string()]));
    assert!(fake.closed.lock().unwrap().contains(&format!("s-{name}")), "its agent closed");
    *fake.bring.lock().unwrap() = Bring::Refuse("the runtime: claude exited".into());
    let tried = d.call("/api/settings/start/try", Some(json!({ "harness": "claude", "model": "nope" }))).unwrap();
    assert_eq!(tried["ok"], false);
    assert!(tried["words"].as_str().unwrap().contains("claude exited"), "{tried}");
    assert!(fake.closed_places.lock().unwrap().contains(&fake.opened.lock().unwrap()[1].2), "its empty place closed");
    assert_ne!(place, fake.opened.lock().unwrap()[1].2, "a place of its own each time");
    assert!(bodies(&db, "workspace.manager_started").is_empty() && bodies(&db, "workspace.manager_start_seated").is_empty());
}
