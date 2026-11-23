// A project added from the page, and a manager started from it: the user's start, the
// service's seating of what it brings up, and its end. The seating is the start's: checked
// here by sequence, never by the seat's value.
use serde_json::{Value, json};

use crate::records::tests::*;
use crate::*;

/// /p registered with ws1 "chat" (nobody seated) and ws2 "docs" (boss seated).
fn setup() -> Records {
    let mut s = Records::default();
    assert_eq!((workspace(&mut s, "chat", "/p"), workspace(&mut s, "docs", "/p")), ("ws1".to_string(), "ws2".to_string()));
    seat(&mut s, "ws2", "boss");
    s
}

fn shell(pid: u32) -> Value {
    json!({"pid": pid, "start": 7})
}

/// The user's start in `ws`, over the seat as it is, at `place` with shell 41.
fn started(ws: &str, place: &str) -> Value {
    started_over(ws, place, Value::Null)
}

fn started_over(ws: &str, place: &str, replaces: Value) -> Value {
    json!({"kind": "workspace.manager_started", "body": {"workspace": ws, "harness": "claude", "model": "opus", "name": "x", "folder": "/elsewhere", "place": place, "shell": shell(41), "replaces": replaces}})
}

/// The service's seating of the start recorded at `seq`.
fn seated(ws: &str, seq: u64, session: &str) -> Value {
    seated_at(ws, seq, session, "pl", 41, "/p", "claude")
}

fn seated_at(ws: &str, seq: u64, session: &str, place: &str, pid: u32, folder: &str, harness: &str) -> Value {
    json!({"kind": "workspace.manager_start_seated", "body": {"workspace": ws, "start": seq, "session": session, "harness": harness, "folder": folder, "place": place, "shell": shell(pid)}})
}

fn ended(ws: &str, seq: u64, reason: &str) -> Value {
    json!({"kind": "workspace.manager_start_ended", "body": {"workspace": ws, "start": seq, "reason": reason, "words": "the runtime says no"}})
}

/// What the runtime shows of a session, as the service records it.
fn seen(s: &mut Records, session: &str) {
    add(s, "workriver", json!({"kind": "agent.seen", "body": {"session": session, "name": session, "harness": "claude", "cwd": "/p"}})).unwrap();
}

/// The user starts a manager in ws1; the sequence of the start.
fn start(s: &mut Records) -> u64 {
    add(s, "user", started("ws1", "pl")).unwrap().seq as u64
}

fn seat_of<'a>(s: &'a Records, ws: &str) -> Option<&'a Seat> {
    s.state().workspace(ws).unwrap().manager.as_ref()
}

fn pending(s: &Records, ws: &str) -> bool {
    s.state().workspace(ws).unwrap().start.as_ref().is_some_and(|x| x.ended.is_none())
}

/// A project is not inside a registered one and holds none: one added inside another would
/// change the project of intents already recorded there.
#[test]
fn a_project_inside_another_or_holding_one_is_refused() {
    let mut s = Records::default();
    project(&mut s, "/p/q");
    let e = add(&mut s, "user", json!({"kind": "project.added", "body": {"path": "/p/q/r", "title": "R"}})).unwrap_err();
    assert_eq!(e, "/p/q/r is inside project p1 (/p/q): a project holds no other");
    let e = add(&mut s, "user", json!({"kind": "project.added", "body": {"path": "/p", "title": "P"}})).unwrap_err();
    assert_eq!(e, "/p holds project p1 (/p/q): a project holds no other");
    // A name that only starts the same is another folder.
    add(&mut s, "user", json!({"kind": "project.added", "body": {"path": "/p/qq", "title": "QQ"}})).unwrap();
    add(&mut s, "user", json!({"kind": "project.added", "body": {"path": "/p/q2", "title": "Q2"}})).unwrap();
    // Projects nested in an older store replay as they were.
    let s = recorded_before(s, "user", json!({"kind": "project.added", "body": {"id": "p4", "path": "/p/q/inner", "title": "Inner"}}));
    assert_eq!(s.state().projects.len(), 4);
}

/// The start is the user's alone; the seating and the end of one are workriver's alone.
#[test]
fn a_start_is_the_users_and_its_seating_and_end_are_workrivers() {
    let mut s = setup();
    for who in ["boss", "workriver"] {
        assert!(add(&mut s, who, started("ws1", "pl")).unwrap_err().contains("only the user"), "{who}");
    }
    let seq = start(&mut s);
    for who in ["user", "boss"] {
        assert!(add(&mut s, who, seated("ws1", seq, "s-new")).unwrap_err().contains("only workriver records this"), "{who}");
        assert!(add(&mut s, who, ended("ws1", seq, "no_agent")).unwrap_err().contains("only workriver records this"), "{who}");
    }
    assert!(pending(&s, "ws1"));
}

/// The rules fill in the folder of the workspace's project, whatever the body says, and the
/// start's agent goes by a name of its own, the workspace's and the start's sequence; a closed
/// workspace, a seat changed since the user looked and a start still pending are refused, the
/// last two as conflicts.
#[test]
fn a_start_takes_the_projects_folder_and_one_at_a_time() {
    let mut s = setup();
    let e = add(&mut s, "user", started("ws1", "pl")).unwrap();
    let Body::WorkspaceManagerStarted { folder, .. } = &e.body else { panic!() };
    assert_eq!(folder, "/p");
    let w = s.state().workspace("ws1").unwrap().start.clone().unwrap();
    assert_eq!((w.seq, w.harness.as_str(), w.model.as_str(), w.place.as_str(), w.shell.pid, w.folder.as_str()), (e.seq as u64, "claude", "opus", "pl", 41, "/p"));
    assert_eq!(w.name, format!("ws1-manager-{}", e.seq), "not the body's");
    assert_eq!(w.name, ManagerStart::name_of("ws1", e.seq as u64));
    let e = add(&mut s, "user", started("ws1", "pl2")).unwrap_err();
    assert!(e.starts_with(START_PENDING), "{e}");
    assert_eq!(e, "a manager is being started for ws1: wait for it, or seat one yourself");
    // Over boss, the user was shown nobody: the seat changed.
    let e = add(&mut s, "user", started("ws2", "pl")).unwrap_err();
    assert!(e.starts_with(SEAT_CHANGED), "{e}");
    let boss = seat_of(&s, "ws2").unwrap().shown();
    add(&mut s, "user", started_over("ws2", "pl2", json!(boss))).unwrap();
    let closed = workspace(&mut s, "old", "/p");
    add(&mut s, "user", json!({"kind": "workspace.closed", "body": {"workspace": closed}})).unwrap();
    assert!(add(&mut s, "user", started(&closed, "pl3")).unwrap_err().contains("is closed"));
}

/// What a start brings up is seated: a session the records never knew, or first seen after
/// the start, with a new appointment; the start is then over. A manager seen coming back in
/// between changes nothing.
#[test]
fn the_start_seats_a_session_new_to_the_records() {
    let mut s = setup();
    let seq = start(&mut s);
    add(&mut s, "workriver", seated("ws1", seq, "s-new")).unwrap();
    let m = seat_of(&s, "ws1").unwrap().clone();
    assert_eq!((m.session.as_str(), m.folder.as_str(), m.machine.as_deref()), ("s-new", "/p", None));
    assert_eq!(m.since, s.events().last().unwrap().seq as u64, "a new appointment: the seating's own");
    assert!(s.state().workspace("ws1").unwrap().start.is_none());
    // First seen after the start, and boss back in its terminal meanwhile.
    let mut s = setup();
    let seq = start(&mut s);
    add(&mut s, "workriver", json!({"kind": "workspace.manager_back", "body": {"workspace": "ws2", "session": "boss"}})).unwrap();
    seen(&mut s, "s-later");
    assert!(s.state().agents.iter().find(|a| a.session == "s-later").unwrap().first > seq);
    add(&mut s, "workriver", seated("ws1", seq, "s-later")).unwrap();
    assert_eq!(seat_of(&s, "ws1").unwrap().session, "s-later");
}

/// Anything that touched the seat since the click drops the start: the late seating is
/// refused, whatever the seat's value is by then.
#[test]
fn the_seating_is_refused_once_the_seat_was_touched() {
    // Filled and emptied again: the same value (nobody) as at the click.
    let mut s = setup();
    let seq = start(&mut s);
    seat(&mut s, "ws1", "other");
    seat_at(&mut s, "ws1", None, None, "").unwrap();
    assert!(s.state().workspace("ws1").unwrap().start.is_none());
    let e = add(&mut s, "workriver", seated("ws1", seq, "s-new")).unwrap_err();
    assert_eq!(e, format!("the start of a manager for ws1 ({seq}) is over: what it brought up is not seated"));
    assert!(seat_of(&s, "ws1").is_none());
    // Closed and opened again.
    let mut s = setup();
    let seq = start(&mut s);
    add(&mut s, "user", json!({"kind": "workspace.closed", "body": {"workspace": "ws1"}})).unwrap();
    add(&mut s, "user", json!({"kind": "workspace.opened", "body": {"workspace": "ws1"}})).unwrap();
    assert!(add(&mut s, "workriver", seated("ws1", seq, "s-new")).unwrap_err().contains("is over"));
    // Seated by the user meanwhile.
    let mut s = setup();
    let seq = start(&mut s);
    seat(&mut s, "ws1", "boss");
    assert!(add(&mut s, "workriver", seated("ws1", seq, "s-new")).unwrap_err().contains("is over"));
    assert_eq!(seat_of(&s, "ws1").unwrap().session, "boss");
    // A second seating of one start: the first dropped it.
    let mut s = setup();
    let seq = start(&mut s);
    add(&mut s, "workriver", seated("ws1", seq, "s-new")).unwrap();
    assert!(add(&mut s, "workriver", seated("ws1", seq, "s-other")).unwrap_err().contains("is over"));
    assert_eq!(seat_of(&s, "ws1").unwrap().session, "s-new");
}

/// A start whose seat is taken away by a seating elsewhere (the session it held is seated in
/// another workspace) is dropped too.
#[test]
fn a_seating_elsewhere_that_empties_the_seat_drops_the_start() {
    let mut s = setup();
    let boss = seat_of(&s, "ws2").unwrap().shown();
    let seq = add(&mut s, "user", started_over("ws2", "pl", json!(boss))).unwrap().seq as u64;
    seat(&mut s, "ws1", "boss");
    assert!(seat_of(&s, "ws2").is_none());
    assert!(s.state().workspace("ws2").unwrap().start.is_none());
    assert!(add(&mut s, "workriver", seated("ws2", seq, "s-new")).unwrap_err().contains("is over"));
}

/// An ended start seats nobody, and a start that is not the workspace's own (another
/// sequence) is refused; the end is accepted once, for the pending start only.
#[test]
fn an_ended_start_or_another_one_seats_nobody() {
    let mut s = setup();
    let seq = start(&mut s);
    assert!(add(&mut s, "workriver", seated("ws1", seq + 1, "s-new")).unwrap_err().contains(&format!("{} is not the start of a manager for ws1", seq + 1)));
    assert!(add(&mut s, "workriver", ended("ws1", seq + 1, "no_agent")).unwrap_err().contains("is not the start of a manager for ws1"));
    add(&mut s, "workriver", ended("ws1", seq, "no_session")).unwrap();
    let start = s.state().workspace("ws1").unwrap().start.clone().unwrap();
    let end = start.ended.unwrap();
    assert_eq!((end.reason, end.words.as_str()), (StartEnd::NoSession, "the runtime says no"));
    assert!(add(&mut s, "workriver", seated("ws1", seq, "s-new")).unwrap_err().contains("is over"));
    assert!(add(&mut s, "workriver", ended("ws1", seq, "restarted")).unwrap_err().contains("is over"));
    assert!(seat_of(&s, "ws1").is_none());
    // The reason stays until the next start, which a pending one no longer blocks.
    let next = add(&mut s, "user", started("ws1", "pl2")).unwrap().seq as u64;
    assert!(s.state().workspace("ws1").unwrap().start.as_ref().is_some_and(|x| x.seq == next && x.ended.is_none()));
    // Its agent goes by another name than the first start's, which may still run.
    assert_eq!(s.state().workspace("ws1").unwrap().start.as_ref().unwrap().name, format!("ws1-manager-{next}"));
    assert_ne!(ManagerStart::name_of("ws1", seq), ManagerStart::name_of("ws1", next));
    // What showed up reported a problem as it started: ended so, unseated.
    add(&mut s, "workriver", ended("ws1", next, "startup_error")).unwrap();
    assert_eq!(s.state().workspace("ws1").unwrap().start.as_ref().unwrap().ended.as_ref().unwrap().reason, StartEnd::StartupError);
    assert!(seat_of(&s, "ws1").is_none());
}

/// Only at the recorded place and shell, in the project's folder, with the start's harness.
#[test]
fn the_seating_is_where_the_start_was_made() {
    let cases = [
        (seated_at("ws1", 0, "s-new", "other", 41, "/p", "claude"), "at another place"),
        (seated_at("ws1", 0, "s-new", "pl", 42, "/p", "claude"), "another shell"),
        (seated_at("ws1", 0, "s-new", "pl", 41, "/q", "claude"), "in another folder"),
        (seated_at("ws1", 0, "s-new", "pl", 41, "/p", "codex"), "another harness"),
    ];
    for (mut v, why) in cases {
        let mut s = setup();
        let seq = start(&mut s);
        v["body"]["start"] = json!(seq);
        let e = add(&mut s, "workriver", v).unwrap_err();
        assert!(e.contains(why), "{why}: {e}");
        assert!(seat_of(&s, "ws1").is_none() && pending(&s, "ws1"), "{why}");
    }
}

/// A session the records knew before the click is never seated by a start: one seen before,
/// one that holds a seat (seated without ever running a command, so never seen), one that has
/// a task. The other seat stays.
#[test]
fn a_session_known_before_the_start_is_refused() {
    // Seen before the start.
    let mut s = setup();
    seen(&mut s, "s-old");
    let seq = start(&mut s);
    assert!(add(&mut s, "workriver", seated("ws1", seq, "s-old")).unwrap_err().contains("s-old was known before the start"));
    // Seated in ws2 before the start, never seen.
    let mut s = setup();
    let seq = start(&mut s);
    let e = add(&mut s, "workriver", seated("ws1", seq, "boss")).unwrap_err();
    assert!(e.contains("boss holds the seat of ws2"), "{e}");
    assert_eq!(seat_of(&s, "ws2").unwrap().session, "boss");
    // Seated in ws2 after the start (the user's pick while it was pending), never seen.
    let mut s = setup();
    let seq = start(&mut s);
    let boss = seat_of(&s, "ws2").unwrap().shown();
    add(&mut s, "user", json!({"kind": "workspace.manager_set", "body": {"workspace": "ws2", "manager": "s-new", "folder": "/p", "replaces": boss}})).unwrap();
    seen(&mut s, "s-new");
    let e = add(&mut s, "workriver", seated("ws1", seq, "s-new")).unwrap_err();
    assert!(e.contains("s-new holds the seat of ws2"), "{e}");
    assert_eq!(seat_of(&s, "ws2").unwrap().session, "s-new");
    // A task's agent.
    let mut s = setup();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "T", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": "ws2"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "U", "repo": "/p/r", "branch": "feat/u", "base": "main"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "T", "agent": "s-worker"}})).unwrap();
    let seq = start(&mut s);
    let e = add(&mut s, "workriver", seated("ws1", seq, "s-worker")).unwrap_err();
    assert!(e.contains("s-worker has tasks"), "{e}");
}

/// A store without starts, and agents seen before a first-seen sequence was kept, read as
/// before: such an agent counts as known.
#[test]
fn old_records_replay_and_old_agents_count_as_known() {
    let s = setup();
    let s = recorded_before(s, "workriver", json!({"kind": "agent.seen", "body": {"session": "s-old", "name": "old", "harness": "claude"}}));
    let state = replayed(&s);
    assert!(state.workspaces.iter().all(|w| w.start.is_none()));
    let a = state.agents.iter().find(|a| a.session == "s-old").unwrap();
    assert_eq!(a.first, s.events().len() as u64, "replayed, the sequence it was first seen at");
    // An agent stored before `first` existed reads as 0: known before anything.
    let old: Agent = serde_json::from_value(json!({"session": "s", "name": "n", "harness": "claude"})).unwrap();
    assert_eq!(old.first, 0);
}
