// A manager's seat (docs/security.md): set by the user only, never over a seat the user was
// not shown; read at the seating on the session's own machine; kept while its terminal is
// gone, and back by itself only where it was; what was sent to or asked under one
// appointment never reaches another.
use serde_json::{Value, json};
use workriver_core::runtime::{AgentState::*, RuntimeAgent};

use super::support::*;
use crate::cli::Daemon;

/// A live agent started in `folder`.
fn started_in(session: &str, name: &str, folder: &str) -> RuntimeAgent {
    RuntimeAgent { cwd: folder.into(), ..live(session, name, "w1", Idle) }
}

fn workspace(d: &Daemon, id: &str) -> Value {
    d.get("/api/workspace").unwrap()["workspaces"].as_array().unwrap().iter().find(|w| w["id"] == id).unwrap().clone()
}

fn message(d: &Daemon, text: &str) -> Value {
    d.get("/api/workspace").unwrap()["messages"].as_array().unwrap().iter().find(|m| m["text"] == text).unwrap_or_else(|| panic!("no message {text:?}")).clone()
}

/// The lines workriver put in a conversation saying a manager has its seat back.
fn back(d: &Daemon) -> Vec<Value> {
    d.get("/api/workspace").unwrap()["messages"].as_array().into_iter().flatten().filter(|m| m["notice"] == "manager-back").cloned().collect()
}

fn told(fake: &Fake, to: &str, what: &str) -> usize {
    fake.sent.lock().unwrap().iter().filter(|(t, m)| t == to && m.contains(what)).count()
}

fn here(d: &Daemon, place: &str) -> anyhow::Result<Value> {
    at_place(d, place).call("/api/here", Some(json!({})))
}

/// The user seats a manager, by its session or by a name only one running agent has; the
/// session is told, with where its role is written. A change against another seat than
/// the one shown is refused (409) and changes nothing; the user seats over a manager that
/// runs, which is told it no longer manages; a session holds one seat; an empty seat is
/// emptied no further.
#[test]
fn the_user_seats_a_manager_never_over_a_seat_not_shown() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), started_in("s-boss", "boss", "/w/boss")), ("p2".into(), started_in("s-rev", "rev", "/w/rev"))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    let ws = workspace_named(&d, "chat");
    assert!(seat_of(&d, &ws).is_null());

    let seated = seat(&d, &ws, Some("boss")).unwrap();
    assert_eq!(seated["body"], json!({ "workspace": "ws1", "manager": "s-boss", "folder": "/w/boss", "replaces": null }), "by its name; where the runtime shows it");
    let first = seat_of(&d, &ws);
    assert_eq!(first["session"], "s-boss");
    wait_until("the manager is told", || told(&fake, "s-boss", "[workriver] The user made you the manager of workspace ws1 «chat» of project «chat»") == 1);
    assert_eq!(told(&fake, "s-boss", "`workriver guide manager`"), 1);
    assert_eq!((here(&d, "p1").unwrap()["manager"].as_bool(), here(&d, "p2").unwrap()["manager"].as_bool()), (Some(true), Some(false)));

    // The page was shown an empty seat, and the seat is boss's by now: refused, 409.
    let stale = json!({ "action": "manager", "workspace": ws, "manager": "s-rev", "replaces": null });
    let (code, text) = as_user(&d, "POST", "/api/workspaces", Some(&stale));
    assert!(code == 409 && text.starts_with("the seat changed") && text.contains("ws1's manager is boss now, not nobody"), "{code} {text}");
    let wrong = json!({ "action": "manager", "workspace": ws, "manager": "s-rev", "replaces": { "session": "s-boss", "since": first["since"].as_u64().unwrap() + 7 } });
    assert_eq!(as_user(&d, "POST", "/api/workspaces", Some(&wrong)).0, 409, "another appointment of the same session is another seat");
    assert_eq!(seat_of(&d, &ws), first, "nothing changed");
    assert_eq!(told(&fake, "s-rev", ""), 0);

    // Over a manager that runs, naming the seat as it is.
    seat(&d, &ws, Some("s-rev")).unwrap();
    let second = seat_of(&d, &ws);
    assert!(second["session"] == "s-rev" && second["since"].as_u64() > first["since"].as_u64(), "a new appointment: {second}");
    wait_until("the one replaced is told", || told(&fake, "s-boss", "[workriver] You are no longer the manager of workspace ws1 «chat»") == 1);
    wait_until("the one seated is told", || told(&fake, "s-rev", "The user made you the manager of workspace ws1") == 1);
    assert!(d.append("s-boss", "message.said", json!({ "text": "Still me?" })).unwrap_err().to_string().contains("only a workspace's manager says something to the user"));

    // One session, one seat: seated elsewhere, its old seat is empty.
    let side = workspace_named(&d, "side");
    seat(&d, &side, Some("s-rev")).unwrap();
    assert!(seat_of(&d, &ws).is_null() && seat_of(&d, &side)["session"] == "s-rev");
    // Emptied by the user; an empty seat is emptied no further.
    seat(&d, &side, None).unwrap();
    assert!(seat_of(&d, &side).is_null());
    assert!(seat(&d, &side, None).unwrap_err().to_string().contains("ws2 has no manager"));
    assert_eq!(here(&d, "p2").unwrap()["manager"], false);
}

/// The session's root is read at the seating, where its runtime shows it: one that does
/// not run, a name several running agents have, a session shown in two terminals at once,
/// one shown in another terminal than the one it was first seen in while that one's shell
/// runs, and a runtime that cannot say are all refused, and seat nobody.
#[test]
fn a_session_is_seated_only_where_the_runtime_shows_it_once() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    let set = |agents: Vec<(&str, RuntimeAgent)>| *fake.agents.lock().unwrap() = agents.into_iter().map(|(p, a)| (p.to_string(), a)).collect();
    set(vec![("p1", started_in("s-one", "rev", "/w/one")), ("p2", started_in("s-two", "rev", "/w/two"))]);
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    let ws = workspace_named(&d, "chat");
    let refused = |agent: &str| seat(&d, &ws, Some(agent)).unwrap_err().to_string();

    assert!(refused("s-ghost").contains("no running agent s-ghost"));
    let e = refused("rev");
    assert!(e.contains("several running agents are rev") && e.contains("s-one (rev)") && e.contains("s-two (rev)"), "{e}");
    // Shown in two terminals at once: neither's.
    set(vec![("p1", started_in("s-one", "rev", "/w/one")), ("p3", started_in("s-one", "rev", "/w/one"))]);
    let e = refused("s-one");
    assert!(e.contains("several running agents are s-one") || e.contains("shown in several terminals at once (p1, p3)"), "{e}");
    // First seen in p1, whose shell still runs; the runtime now shows the session in p4.
    set(vec![("p1", started_in("s-one", "rev", "/w/one"))]);
    here(&d, "p1").unwrap();
    set(vec![("p4", started_in("s-one", "rev", "/w/one"))]);
    let e = refused("s-one");
    assert!(e.contains("is shown in terminal p4, but its session was first seen in terminal p1, whose shell still runs"), "{e}");
    // The runtime cannot say where the agents are: 503, and nothing is seated.
    set(vec![("p1", started_in("s-one", "rev", "/w/one"))]);
    fake.agent_roots_down.store(true, std::sync::atomic::Ordering::Relaxed);
    let (code, text) = as_user(&d, "POST", "/api/workspaces", Some(&json!({ "action": "manager", "workspace": ws, "manager": "s-one", "replaces": null })));
    assert!(code == 503 && text.contains("cannot read the agents now"), "{code} {text}");
    assert!(seat_of(&d, &ws).is_null());
    fake.agent_roots_down.store(false, std::sync::atomic::Ordering::Relaxed);
    seat(&d, &ws, Some("s-one")).unwrap();
    assert_eq!(seat_of(&d, &ws)["session"], "s-one");
}

/// A seated session keeps its root. While that root runs, another terminal that reports the
/// session is refused (rule 5), and neither is the manager meanwhile. Once it is gone, the same session showing again on the same
/// machine in the same folder has its seat back by itself, at its own call or at the
/// regular read: one line of the workspace's conversation says so (not the manager's
/// message, not unread), and the appointment is as it was (its question stays open and is
/// answered). In another folder it is refused until the user confirms: nothing is typed
/// into it meanwhile, the page proposes it, and confirming is a new appointment, which
/// closes what the earlier one asked.
#[test]
fn a_seat_comes_back_by_itself_only_where_it_was() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    let boss = || started_in("s-boss", "boss", "/w/boss");
    let set = |place: &str, a: RuntimeAgent| *fake.agents.lock().unwrap() = vec![(place.to_string(), a), ("p9".into(), started_in("s-w", "w", "/w/w"))];
    set("p1", boss());
    let (d, kernel) = daemon_and(fake.clone(), &tmp.path().join("w.db"), Default::default());
    manage(&d, "p1", "s-boss");
    let since = seat_of(&d, "ws1");
    let choice = crate::cli::parse_choice(&["merge=Merge".into(), "hold=Not now".into()], &[]).unwrap();
    d.append("s-boss", "message.said", json!({ "text": "Merge now?", "choice": choice })).unwrap();
    assert_eq!(message(&d, "Merge now?")["open"], true);

    // Its root runs: the same session from another terminal is refused, seat or not.
    fake.agents.lock().unwrap().push(("p2".into(), boss()));
    let e = here(&d, "p2").unwrap_err().to_string();
    assert!(e.contains("session s-boss was first seen in") && e.contains(" p1, whose shell still runs"), "{e}");
    // Shown at two terminals, neither is the manager: the regular read shows it not running.
    wait_until("the page proposes it for the seat", || workspace(&d, "ws1")["proposal"] == "s-boss");
    assert!(back(&d).is_empty() && workspace(&d, "ws1")["manager"]["state"] == "gone");

    // Its terminal is gone; the session shows again in another one, in the same folder.
    kernel.procs.kill(root("p1"));
    set("p2", boss());
    let again = here(&d, "p2").unwrap();
    assert_eq!((again["workspace"]["id"].as_str(), again["manager"].as_bool()), (Some("ws1"), Some(true)), "{again}");
    wait_until("the conversation says so", || back(&d).len() == 1);
    let line = back(&d)[0].clone();
    assert_eq!((line["workspace"].as_str(), line["manager"].as_str(), line["text"].as_str(), line.get("unread"), line.get("byManager")), (Some("ws1"), Some("boss"), Some(""), None, None), "{line}");
    here(&d, "p2").unwrap();
    assert_eq!(back(&d).len(), 1, "said once");
    assert_eq!(seat_of(&d, "ws1"), since, "the same appointment");
    // What it asked before is still its to be answered.
    assert_eq!(message(&d, "Merge now?")["open"], true);
    d.call("/api/message", Some(json!({ "workspace": "ws1", "text": "", "answer": { "message": message(&d, "Merge now?")["id"], "selected": ["merge"] } }))).unwrap();
    wait_until("the answer reaches it", || told(&fake, "s-boss", "The user answered your question") == 1);

    // Gone again; nobody calls: the regular read gives it back, in the same folder.
    d.append("s-boss", "message.said", json!({ "text": "And deploy?", "choice": choice })).unwrap();
    kernel.procs.kill(root("p2"));
    set("p3", boss());
    wait_until("back at the regular read", || back(&d).len() == 2);
    assert_eq!(seat_of(&d, "ws1"), since);
    assert!(workspace(&d, "ws1")["manager"]["state"] == "idle" && workspace(&d, "ws1").get("proposal").is_none());
    here(&d, "p3").unwrap();
    assert_eq!(back(&d).len(), 2);

    // Gone once more; the session shows in another folder: not the manager by itself.
    kernel.procs.kill(root("p3"));
    set("p4", started_in("s-boss", "boss", "/w/elsewhere"));
    wait_until("the page proposes it for the seat", || workspace(&d, "ws1")["proposal"] == "s-boss");
    assert_eq!(workspace(&d, "ws1")["manager"]["state"], "gone", "its manager does not run, whatever reports its session");
    let e = here(&d, "p4").unwrap_err().to_string();
    assert!(e.contains("session s-boss holds a manager's seat, given to it in /w/boss") && e.contains("shows it in /w/elsewhere") && e.contains("only once the user confirms it"), "{e}");
    assert!(d.append("s-boss", "message.said", json!({ "text": "x" })).is_ok(), "(`--as` in tests names the session; the terminal itself is refused above)");
    assert_eq!(back(&d).len(), 2, "no line: it is not back");
    // Nothing is typed into it: the user's message is not delivered.
    d.call("/api/message", Some(json!({ "workspace": "ws1", "text": "Are you there?" }))).unwrap();
    wait_until("not delivered", || message(&d, "Are you there?")["step"] == "not-delivered");
    assert_eq!(message(&d, "Are you there?")["reasonCode"], "manager-gone");
    assert_eq!(told(&fake, "s-boss", "Are you there?"), 0);

    // The user confirms it: seated where it shows now, as a new appointment.
    let confirmed = seat(&d, "ws1", Some("s-boss")).unwrap();
    assert_eq!(confirmed["body"]["folder"], "/w/elsewhere");
    let now = seat_of(&d, "ws1");
    assert!(now["session"] == "s-boss" && now["since"].as_u64() > since["since"].as_u64(), "{now}");
    assert!(here(&d, "p4").unwrap()["manager"] == true);
    wait_until("no longer proposed", || workspace(&d, "ws1").get("proposal").is_none() && workspace(&d, "ws1")["manager"]["state"] == "idle");
    // What the earlier appointment asked is closed, and takes no answer.
    let asked = message(&d, "And deploy?");
    assert!(asked.get("open").is_none() && asked.get("answered").is_none(), "{asked}");
    let e = d.call("/api/message", Some(json!({ "workspace": "ws1", "text": "", "answer": { "message": asked["id"], "selected": ["merge"] } }))).unwrap_err().to_string();
    assert!(e.contains("was asked by an earlier manager of this workspace"), "{e}");
    assert_eq!(back(&d).len(), 2);
}

/// What was queued for an appointment never reaches the next: the user's message waits
/// while its manager waits for a person in its terminal; the user seats another manager
/// meanwhile; the message is recorded not delivered (the seat changed) and is typed into
/// neither terminal.
#[test]
fn a_queued_message_is_not_given_to_the_next_appointment() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), started_in("s-boss", "boss", "/w/boss")), ("p2".into(), started_in("s-rev", "rev", "/w/rev"))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    manage(&d, "p1", "s-boss");
    fake.agents.lock().unwrap()[0].1.state = Blocked;
    d.call("/api/message", Some(json!({ "workspace": "ws1", "text": "Queued for boss" }))).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500)); // held over a poll
    assert!(message(&d, "Queued for boss")["step"] != "delivered" && told(&fake, "s-boss", "Queued for boss") == 0);
    seat(&d, "ws1", Some("s-rev")).unwrap();
    fake.agents.lock().unwrap()[0].1.state = Idle;
    wait_until("not delivered", || message(&d, "Queued for boss")["step"] == "not-delivered");
    let m = message(&d, "Queued for boss");
    assert_eq!(m["reasonCode"], "seat-changed", "{m}");
    std::thread::sleep(std::time::Duration::from_millis(2500));
    assert_eq!((told(&fake, "s-boss", "Queued for boss"), told(&fake, "s-rev", "Queued for boss")), (0, 0), "never re-routed");
    // The same for the same session seated anew elsewhere and back: another appointment.
    d.call("/api/message", Some(json!({ "workspace": "ws1", "text": "For rev" }))).unwrap();
    wait_until("rev has it", || told(&fake, "s-rev", "For rev") == 1);
    let side = workspace_named(&d, "side");
    seat(&d, &side, Some("s-rev")).unwrap();
    seat(&d, "ws1", Some("s-rev")).unwrap();
    let e = d.append("s-rev", "message.replied", json!({ "message": message(&d, "For rev")["id"], "text": "Late" })).unwrap_err().to_string();
    assert!(e.contains("went to an earlier manager of workspace ws1"), "{e}");
}

/// The agent now in the terminal the manager was seated in is no manager: the workspace
/// shows its manager gone and proposes that agent, and only the user's seating makes it one.
#[test]
fn the_agent_now_in_the_managers_terminal_is_only_proposed() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), started_in("s-boss", "boss", "/w/boss"))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    manage(&d, "p1", "s-boss");
    assert!(workspace(&d, "ws1").get("proposal").is_none());
    // Its session ended; a new one runs in the same terminal, in the same folder.
    *fake.agents.lock().unwrap() = vec![("p1".into(), started_in("s-next", "next", "/w/boss"))];
    wait_until("proposed", || workspace(&d, "ws1")["proposal"] == "s-next");
    let w = workspace(&d, "ws1");
    assert_eq!((w["manager"]["session"].as_str(), w["manager"]["state"].as_str()), (Some("s-boss"), Some("gone")), "{w}");
    let next = here(&d, "p1").unwrap();
    assert_eq!((next["actor"].as_str(), next["manager"].as_bool(), next["workspace"].is_null()), (Some("s-next"), Some(false), true), "{next}");
    let e = from_place(&d, "p1", "POST", "/api/append", Some(&json!({ "kind": "message.said", "body": { "text": "I am the manager now" } }))).1;
    assert!(e.contains("only a workspace's manager says something to the user"), "{e}");
    let e = from_place(&d, "p1", "POST", "/api/append", Some(&json!({ "kind": "intent.state_changed", "body": { "intent": "i1", "state": "done" } }))).1;
    assert!(e.contains("only the user or the manager of intent i1"), "{e}");
    seat(&d, "ws1", Some("s-next")).unwrap();
    wait_until("seated, no longer proposed", || workspace(&d, "ws1").get("proposal").is_none() && workspace(&d, "ws1")["manager"]["state"] == "idle");
    assert_eq!(here(&d, "p1").unwrap()["manager"], true);
}

/// On another machine the seat's root is the one that machine's runtime shows for the
/// session when the user seats it, not whichever root called first with it: a session
/// first called with from one root and shown at another is refused; one shown at one root
/// and never seen is pinned there, and a caller from another root is then refused; one
/// shown at two roots there is refused. No request on that machine's listener seats anyone.
#[test]
fn a_manager_on_another_machine_is_seated_at_the_root_its_runtime_shows() {
    let tmp = tempfile::tempdir().unwrap();
    let local = std::sync::Arc::new(Fake::default());
    let far = std::sync::Arc::new(Fake::default());
    let set = |agents: Vec<(&str, RuntimeAgent)>| *far.agents.lock().unwrap() = agents.into_iter().map(|(p, a)| (p.to_string(), a)).collect();
    set(vec![("p1", started_in("s-far", "far", "/srv/a"))]);
    let all = crate::runtime::follow::Machines::with(local, vec![("box", far.clone())]);
    let (port, kernel) = daemon_over(all.clone(), Some(all.clone()), &tmp.path().join("w.db"));
    let user = kernel.client(port, tmp.path());
    wait_until("the machine's listener", || all.local_port("box").is_some());
    let there = kernel.daemon(all.local_port("box").unwrap(), None, None);
    let ws = workspace_named(&user, "chat");
    // Called with from p1 first: that is where it is pinned.
    assert_eq!(here(&there, "p1").unwrap()["actor"], "s-far");
    // That machine's runtime now shows the session at p2, while p1's shell still runs.
    set(vec![("p2", started_in("s-far", "far", "/srv/a"))]);
    all.poll_all();
    let e = seat(&user, &ws, Some("s-far")).unwrap_err().to_string();
    assert!(e.contains("is shown in terminal p2 on machine box, but its session was first seen in terminal p1 on machine box, whose shell still runs"), "{e}");
    assert!(seat_of(&user, &ws).is_null());
    // Shown at two roots there: refused.
    set(vec![("p5", started_in("s-twin", "twin", "/srv/t")), ("p6", started_in("s-twin", "twin", "/srv/t"))]);
    all.poll_all();
    let e = seat(&user, &ws, Some("s-twin")).unwrap_err().to_string();
    assert!(e.contains("several") && e.contains("s-twin"), "{e}");
    // Shown at one root, never called with: pinned there by the seating.
    set(vec![("p3", started_in("s-new", "new", "/srv/n"))]);
    all.poll_all();
    let seated = seat(&user, &ws, Some("s-new")).unwrap();
    assert_eq!((seated["body"]["machine"].as_str(), seated["body"]["folder"].as_str()), (Some("box"), Some("/srv/n")), "{seated}");
    assert_eq!(here(&there, "p3").unwrap()["manager"], true);
    // Another root that reports the session, while p3's shell runs: refused.
    set(vec![("p4", started_in("s-new", "new", "/srv/n"))]);
    all.poll_all();
    let e = here(&there, "p4").unwrap_err().to_string();
    assert!(e.contains("session s-new was first seen in") && e.contains(" p3 on machine box, whose shell still runs"), "{e}");
    // That machine's listener serves no seating, and no agent's event sets a seat.
    let over_there = at_place(&there, "p4");
    assert!(over_there.call("/api/workspaces", Some(json!({ "action": "manager", "workspace": ws, "manager": "s-new", "replaces": seat_of(&user, &ws) }))).is_err());
    set(vec![("p3", started_in("s-new", "new", "/srv/n"))]);
    all.poll_all();
    let e = at_place(&there, "p3").append("", "workspace.manager_set", json!({ "workspace": ws, "manager": null, "replaces": seat_of(&user, &ws) })).unwrap_err().to_string();
    assert!(e.contains("only the user does this (workspace.manager_set)"), "{e}");
    assert_eq!(seat_of(&user, &ws)["session"], "s-new");
}

/// A manager seated on another machine, at p3 there. Its terminal p3 still runs but holds
/// another agent now, and another terminal there (p4, another folder) reports the seated
/// session: that terminal is refused as the manager, the workspace shows its manager not
/// running and proposes the session where it shows now, and the user's message is typed
/// into nothing there (not delivered, as on this machine).
#[test]
fn a_remote_seat_whose_terminal_holds_another_agent_is_lost() {
    let tmp = tempfile::tempdir().unwrap();
    let local = std::sync::Arc::new(Fake::default());
    let far = std::sync::Arc::new(Fake::default());
    let set = |agents: Vec<(&str, RuntimeAgent)>| *far.agents.lock().unwrap() = agents.into_iter().map(|(p, a)| (p.to_string(), a)).collect();
    set(vec![("p3", started_in("s-new", "new", "/srv/n"))]);
    let all = crate::runtime::follow::Machines::with(local, vec![("box", far.clone())]);
    let (port, kernel) = daemon_over(all.clone(), Some(all.clone()), &tmp.path().join("w.db"));
    let user = kernel.client(port, tmp.path());
    wait_until("the machine's listener", || all.local_port("box").is_some());
    let there = kernel.daemon(all.local_port("box").unwrap(), None, None);
    let ws = workspace_named(&user, "chat");
    all.poll_all();
    seat(&user, &ws, Some("s-new")).unwrap();
    assert_eq!(here(&there, "p3").unwrap()["manager"], true);
    set(vec![("p3", started_in("s-other", "other", "/srv/n")), ("p4", started_in("s-new", "new", "/srv/x"))]);
    all.poll_all();
    let e = here(&there, "p4").unwrap_err().to_string();
    assert!(e.contains("session s-new was first seen in") && e.contains(" p3 on machine box, whose shell still runs"), "{e}");
    wait_until("the page proposes it for the seat", || workspace(&user, &ws)["proposal"] == "s-new");
    assert_eq!(workspace(&user, &ws)["manager"]["state"], "gone");
    user.call("/api/message", Some(json!({ "workspace": ws, "text": "For the manager only" }))).unwrap();
    wait_until("not delivered", || message(&user, "For the manager only")["step"] == "not-delivered");
    assert_eq!(message(&user, "For the manager only")["reasonCode"], "manager-gone");
    assert_eq!(far.sent.lock().unwrap().iter().filter(|(_, m)| m.contains("For the manager only")).count(), 0, "typed into nothing there");
}

/// The same session of a manager on another machine shows again there, in the same folder,
/// after its terminal is gone: it has its seat back by itself at the regular read, with no
/// call of its own: the conversation says so, the page shows it running, and the user's
/// message is given to it.
#[test]
fn a_remote_seat_comes_back_by_itself_where_it_was() {
    let tmp = tempfile::tempdir().unwrap();
    let local = std::sync::Arc::new(Fake::default());
    let far = std::sync::Arc::new(Fake::default());
    let set = |agents: Vec<(&str, RuntimeAgent)>| *far.agents.lock().unwrap() = agents.into_iter().map(|(p, a)| (p.to_string(), a)).collect();
    set(vec![("p3", started_in("s-new", "new", "/srv/n"))]);
    let all = crate::runtime::follow::Machines::with(local, vec![("box", far.clone())]);
    let (port, kernel) = daemon_over(all.clone(), Some(all.clone()), &tmp.path().join("w.db"));
    let user = kernel.client(port, tmp.path());
    wait_until("the machine's listener", || all.local_port("box").is_some());
    let ws = workspace_named(&user, "chat");
    all.poll_all();
    seat(&user, &ws, Some("s-new")).unwrap();
    let since = seat_of(&user, &ws);
    // Its terminal is gone; the same session shows in another one there, in the same folder.
    kernel.procs.kill(root("p3"));
    set(vec![("p5", started_in("s-new", "new", "/srv/n"))]);
    all.poll_all();
    wait_until("back at the regular read", || back(&user).len() == 1);
    assert_eq!(seat_of(&user, &ws), since, "the same appointment");
    wait_until("shown running", || workspace(&user, &ws)["manager"]["state"] == "idle" && workspace(&user, &ws).get("proposal").is_none());
    user.call("/api/message", Some(json!({ "workspace": ws, "text": "Welcome back" }))).unwrap();
    wait_until("delivered", || message(&user, "Welcome back")["step"] == "delivered");
    assert_eq!(told(&far, "s-new", "Welcome back"), 1);
}

/// A queued message's last check of its appointment and its typing are one step in order
/// with a seat change: the user seats another manager right before the message would be
/// typed in (after the service took the message up again); it is recorded not delivered,
/// and typed into neither terminal.
#[test]
fn a_seat_change_during_a_queued_send_stops_it() {
    use std::sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering::SeqCst}, mpsc};
    use std::time::Duration;
    let tmp = tempfile::tempdir().unwrap();
    let fake = Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), started_in("s-boss", "boss", "/w/boss")), ("p2".into(), started_in("s-rev", "rev", "/w/rev"))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    manage(&d, "p1", "s-boss");
    fake.agents.lock().unwrap()[0].1.state = Blocked;
    d.call("/api/message", Some(json!({ "workspace": "ws1", "text": "Queued for boss" }))).unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    assert!(message(&d, "Queued for boss")["step"] != "delivered");

    let (paused_tx, paused_rx) = mpsc::channel();
    let (continue_tx, continue_rx) = mpsc::channel::<()>();
    let continue_rx = Mutex::new(continue_rx);
    let calls = AtomicUsize::new(0);
    let stand_in = Arc::downgrade(&fake);
    *fake.on_agents.lock().unwrap() = Some(Arc::new(move || match calls.fetch_add(1, SeqCst) {
        // The poll's read: boss is free, the message is taken up again.
        0 => stand_in.upgrade().unwrap().agents.lock().unwrap()[0].1.state = Idle,
        // Inside `hand`, after its first check of the appointment, before typing.
        1 => {
            paused_tx.send(()).unwrap();
            let _ = continue_rx.lock().unwrap().recv_timeout(Duration::from_secs(10));
        }
        _ => {}
    }));
    paused_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    seat(&d, "ws1", Some("s-rev")).unwrap();
    continue_tx.send(()).unwrap();
    wait_until("not delivered", || message(&d, "Queued for boss")["step"] == "not-delivered");
    assert_eq!(message(&d, "Queued for boss")["reasonCode"], "seat-changed");
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!((told(&fake, "s-boss", "Queued for boss"), told(&fake, "s-rev", "Queued for boss")), (0, 0));
}

/// What the user wrote, typed into the place the fake runtime was asked to type it at, by
/// place.
fn typed_at(fake: &Fake, what: &str) -> Vec<String> {
    fake.typed_at.lock().unwrap().iter().filter(|(_, m)| m.contains(what)).map(|(p, _)| p.clone()).collect()
}

/// A message, then one to every manager: each recorded not delivered as to a manager that
/// does not run (its copy too), typed into nothing.
fn neither_delivered(user: &Daemon, ws: &str, fakes: &[&Fake]) {
    user.call("/api/message", Some(json!({ "workspace": ws, "text": "SECRET_TO_ONE" }))).unwrap();
    wait_until("not delivered", || message(user, "SECRET_TO_ONE")["step"] == "not-delivered");
    assert_eq!(message(user, "SECRET_TO_ONE")["reasonCode"], "manager-gone");
    user.call("/api/message", Some(json!({ "all": true, "text": "SECRET_TO_ALL" }))).unwrap();
    let copy = || message(user, "SECRET_TO_ALL")["to"][0].clone();
    wait_until("its copy not delivered", || copy()["step"] == "not-delivered");
    assert_eq!(copy()["reasonCode"], "manager-gone", "{}", copy());
    std::thread::sleep(std::time::Duration::from_millis(500));
    for fake in fakes {
        assert!(fake.sent.lock().unwrap().iter().all(|(_, m)| !m.contains("SECRET_TO")), "typed into nothing");
    }
}

/// A seated manager's message is typed into the terminal its session is pinned to, by
/// that place. Then a second terminal on this machine (another folder, listed first)
/// reports the seated session while its own still shows it: neither is the manager (rule 5
/// refuses the second), the workspace shows its manager not running and proposes the
/// session, and the user's message and a copy of one to every manager are typed into
/// neither terminal (not delivered).
#[test]
fn a_seat_shown_at_its_root_and_at_another_is_lost() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), started_in("s-boss", "boss", "/w/boss"))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    manage(&d, "p1", "s-boss");
    d.call("/api/message", Some(json!({ "workspace": "ws1", "text": "Hello boss" }))).unwrap();
    wait_until("delivered", || message(&d, "Hello boss")["step"] == "delivered");
    assert_eq!(typed_at(&fake, "Hello boss"), ["p1"]);
    *fake.agents.lock().unwrap() = vec![("p2".into(), started_in("s-boss", "boss", "/w/x")), ("p1".into(), started_in("s-boss", "boss", "/w/boss"))];
    let e = here(&d, "p2").unwrap_err().to_string();
    assert!(e.contains("session s-boss was first seen in") && e.contains("p1"), "{e}");
    neither_delivered(&d, "ws1", &[&fake]);
    wait_until("the page proposes it for the seat", || workspace(&d, "ws1")["proposal"] == "s-boss");
    assert_eq!(workspace(&d, "ws1")["manager"]["state"], "gone");
}

/// The same on another machine: the seated session shown at its pinned terminal there and
/// at another one there (listed first). Nothing is typed there.
#[test]
fn a_remote_seat_shown_at_its_root_and_at_another_is_lost() {
    let tmp = tempfile::tempdir().unwrap();
    let local = std::sync::Arc::new(Fake::default());
    let far = std::sync::Arc::new(Fake::default());
    let set = |agents: Vec<(&str, RuntimeAgent)>| *far.agents.lock().unwrap() = agents.into_iter().map(|(p, a)| (p.to_string(), a)).collect();
    set(vec![("p3", started_in("s-new", "new", "/srv/n"))]);
    let all = crate::runtime::follow::Machines::with(local.clone(), vec![("box", far.clone())]);
    let (port, kernel) = daemon_over(all.clone(), Some(all.clone()), &tmp.path().join("w.db"));
    let user = kernel.client(port, tmp.path());
    wait_until("the machine's listener", || all.local_port("box").is_some());
    let there = kernel.daemon(all.local_port("box").unwrap(), None, None);
    let ws = workspace_named(&user, "chat");
    all.poll_all();
    seat(&user, &ws, Some("s-new")).unwrap();
    assert_eq!(here(&there, "p3").unwrap()["manager"], true);
    user.call("/api/message", Some(json!({ "workspace": ws, "text": "Hello far" }))).unwrap();
    wait_until("delivered", || message(&user, "Hello far")["step"] == "delivered");
    assert_eq!(typed_at(&far, "Hello far"), ["p3"]);
    set(vec![("p4", started_in("s-new", "new", "/srv/x")), ("p3", started_in("s-new", "new", "/srv/n"))]);
    all.poll_all();
    let e = here(&there, "p4").unwrap_err().to_string();
    assert!(e.contains("session s-new was first seen in") && e.contains(" p3 on machine box, whose shell still runs"), "{e}");
    neither_delivered(&user, &ws, &[&local, &far]);
    wait_until("the page proposes it for the seat", || workspace(&user, &ws)["proposal"] == "s-new");
    assert_eq!(workspace(&user, &ws)["manager"]["state"], "gone");
}

/// A manager seated on another machine, whose terminal there still shows it, and a
/// terminal on this machine reports the same session: neither is the manager, and nothing
/// goes to either machine; not this one's, which the runtime lists first for the session.
#[test]
fn a_remote_seat_shown_on_this_machine_too_is_lost() {
    let tmp = tempfile::tempdir().unwrap();
    let local = std::sync::Arc::new(Fake::default());
    let far = std::sync::Arc::new(Fake::default());
    *far.agents.lock().unwrap() = vec![("p3".into(), started_in("s-new", "new", "/srv/n"))];
    let all = crate::runtime::follow::Machines::with(local.clone(), vec![("box", far.clone())]);
    let (port, kernel) = daemon_over(all.clone(), Some(all.clone()), &tmp.path().join("w.db"));
    let user = kernel.client(port, tmp.path());
    wait_until("the machine's listener", || all.local_port("box").is_some());
    let there = kernel.daemon(all.local_port("box").unwrap(), None, None);
    let ws = workspace_named(&user, "chat");
    all.poll_all();
    seat(&user, &ws, Some("s-new")).unwrap();
    assert_eq!(here(&there, "p3").unwrap()["manager"], true);
    *local.agents.lock().unwrap() = vec![("p4".into(), started_in("s-new", "new", "/srv/x"))];
    let e = here(&user, "p4").unwrap_err().to_string();
    assert!(e.contains("s-new"), "{e}");
    neither_delivered(&user, &ws, &[&local, &far]);
    wait_until("the page proposes it for the seat", || workspace(&user, &ws)["proposal"] == "s-new");
    assert_eq!(workspace(&user, &ws)["manager"]["state"], "gone");
}

/// A manager seated on this machine, whose terminal here still shows it, and a terminal on
/// another machine reports the same session: neither is the manager, nothing goes to either.
#[test]
fn a_seat_shown_on_another_machine_too_is_lost() {
    let tmp = tempfile::tempdir().unwrap();
    let local = std::sync::Arc::new(Fake::default());
    let far = std::sync::Arc::new(Fake::default());
    *local.agents.lock().unwrap() = vec![("p1".into(), started_in("s-boss", "boss", "/w/boss"))];
    let all = crate::runtime::follow::Machines::with(local.clone(), vec![("box", far.clone())]);
    let (port, kernel) = daemon_over(all.clone(), Some(all.clone()), &tmp.path().join("w.db"));
    let user = kernel.client(port, tmp.path());
    wait_until("the machine's listener", || all.local_port("box").is_some());
    manage(&user, "p1", "s-boss");
    *far.agents.lock().unwrap() = vec![("p4".into(), started_in("s-boss", "boss", "/srv/x"))];
    all.poll_all();
    wait_until("the page proposes it for the seat", || workspace(&user, "ws1")["proposal"] == "s-boss");
    assert_eq!(workspace(&user, "ws1")["manager"]["state"], "gone");
    neither_delivered(&user, "ws1", &[&local, &far]);
}

/// The user's message, then one to every manager, each once settled: (direct, copy).
fn both_settled(user: &Daemon, ws: &str, tag: &str) -> (Value, Value) {
    let settled = |v: &Value| v["step"] == "delivered" || v["step"] == "not-delivered";
    let (one, all) = (format!("{tag}_ONE"), format!("{tag}_ALL"));
    user.call("/api/message", Some(json!({ "workspace": ws, "text": one }))).unwrap();
    wait_until("the message settled", || settled(&message(user, &one)));
    user.call("/api/message", Some(json!({ "all": true, "text": all }))).unwrap();
    wait_until("its copy settled", || settled(&message(user, &all)["to"][0]));
    std::thread::sleep(std::time::Duration::from_millis(500));
    (message(user, &one), message(user, &all)["to"][0].clone())
}

/// What the fake runtime was asked to send containing `what`, by whom it was sent to.
fn sent_to(fake: &Fake, what: &str) -> Vec<String> {
    fake.sent.lock().unwrap().iter().filter(|(_, m)| m.contains(what)).map(|(t, _)| t.clone()).collect()
}

/// Another agent, with a session of its own, takes the seated manager's session id as its
/// display name and is listed first. The user's message and its copy of one to every
/// manager are for the seated session: typed at its pinned terminal, never into the agent
/// named after it.
#[test]
fn an_agent_named_after_a_seated_session_is_not_it() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), started_in("s-boss", "boss", "/w/boss"))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    manage(&d, "p1", "s-boss");
    *fake.agents.lock().unwrap() = vec![("p9".into(), started_in("s-evil", "s-boss", "/w/e")), ("p1".into(), started_in("s-boss", "boss", "/w/boss"))];
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a regular read of the runtime
    let (one, copy) = both_settled(&d, "ws1", "NAMED");
    assert_eq!((one["step"].as_str(), copy["step"].as_str()), (Some("delivered"), Some("delivered")), "{one} {copy}");
    assert_eq!(typed_at(&fake, "NAMED"), ["p1", "p1"]);
    assert!(sent_to(&fake, "NAMED").iter().all(|t| t != "s-evil"), "typed into the agent named after the session");
}

/// The same with the seat on another machine and the agent named after it on this one:
/// typed at the pinned terminal there, nothing here.
#[test]
fn an_agent_here_named_after_a_remote_seated_session_is_not_it() {
    let tmp = tempfile::tempdir().unwrap();
    let local = std::sync::Arc::new(Fake::default());
    let far = std::sync::Arc::new(Fake::default());
    *far.agents.lock().unwrap() = vec![("p3".into(), started_in("s-new", "new", "/srv/n"))];
    let all = crate::runtime::follow::Machines::with(local.clone(), vec![("box", far.clone())]);
    let (port, kernel) = daemon_over(all.clone(), Some(all.clone()), &tmp.path().join("w.db"));
    let user = kernel.client(port, tmp.path());
    wait_until("the machine's listener", || all.local_port("box").is_some());
    let there = kernel.daemon(all.local_port("box").unwrap(), None, None);
    let ws = workspace_named(&user, "chat");
    all.poll_all();
    seat(&user, &ws, Some("s-new")).unwrap();
    assert_eq!(here(&there, "p3").unwrap()["manager"], true);
    *local.agents.lock().unwrap() = vec![("p9".into(), started_in("s-evil", "s-new", "/w/e"))];
    all.poll_all();
    let (one, copy) = both_settled(&user, &ws, "NAMED");
    assert_eq!((one["step"].as_str(), copy["step"].as_str()), (Some("delivered"), Some("delivered")), "{one} {copy}");
    assert!(sent_to(&local, "NAMED").is_empty(), "typed into this machine's agent named after the session");
    assert_eq!(typed_at(&far, "NAMED"), ["p3", "p3"]);
}

/// What a seated manager must act on (here a worker's `manager tell`) goes by its session
/// too: typed at its pinned terminal, not by the runtime's lookup of an agent named after it.
#[test]
fn a_worker_reaches_its_seated_manager_not_one_named_after_it() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), started_in("s-boss", "boss", "/w/boss")), ("p2".into(), started_in("s-w", "ja-impl", "/w/w"))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    manage(&d, "p1", "s-boss");
    d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "M", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "push it", "agent": "ja-impl" })).unwrap();
    let worker = Daemon { place: Some("p2".into()), home: Some(tmp.path().to_path_buf()), ..d.clone() };
    worker.append("", "task.started", json!({ "unit": "u1", "task": 1 })).unwrap();
    let mut agents = fake.agents.lock().unwrap().clone();
    agents.insert(0, ("p9".into(), started_in("s-evil", "s-boss", "/w/e")));
    *fake.agents.lock().unwrap() = agents;
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a regular read of the runtime
    worker.append("", "manager.told", json!({ "unit": "u1", "task": 1, "text": "TOLD_BOSS" })).unwrap();
    wait_until("the manager hears it", || !sent_to(&fake, "TOLD_BOSS").is_empty());
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert_eq!(typed_at(&fake, "TOLD_BOSS"), ["p1"], "sent: {:?}", sent_to(&fake, "TOLD_BOSS"));
}

/// A message held while the seated manager waits for a person is given once that session is
/// free; an agent that names itself after the session and reports Working (listed first)
/// does not keep it held: readiness is read by session, as `hand` finds the recipient.
#[test]
fn a_held_message_waits_on_the_seated_session_not_one_named_after_it() {
    use workriver_core::runtime::AgentState::{Blocked, Working};
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), started_in("s-boss", "boss", "/w/boss"))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    manage(&d, "p1", "s-boss");
    *fake.agents.lock().unwrap() = vec![("p1".into(), RuntimeAgent { cwd: "/w/boss".into(), ..live("s-boss", "boss", "w1", Blocked) })];
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a regular read: the manager waits
    d.call("/api/message", Some(json!({ "workspace": "ws1", "text": "HELD" }))).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1500));
    *fake.agents.lock().unwrap() = vec![
        ("p9".into(), RuntimeAgent { cwd: "/w/e".into(), ..live("s-evil", "s-boss", "w1", Working) }),
        ("p1".into(), started_in("s-boss", "boss", "/w/boss")),
    ];
    wait_until("held message given", || message(&d, "HELD")["step"] == "delivered");
    assert_eq!(typed_at(&fake, "HELD"), ["p1"]);
}
