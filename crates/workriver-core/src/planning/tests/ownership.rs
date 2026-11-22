// Who decides an intent's project, where it runs and its manager: only the user, or a start
// the rules checked. Workspaces are workriver's own records, made and seated by the user;
// nothing an agent records sets one, its seat or a project.
use serde_json::{Value, json};

use crate::records::tests::*;
use crate::*;

fn set(i: &str, f: Value) -> Value {
    let mut b = json!({"intent": i});
    b.as_object_mut().unwrap().extend(f.as_object().unwrap().clone());
    json!({"kind": "intent.updated", "body": b})
}

fn state(i: &str, st: &str, w: Option<&str>) -> Value {
    match w {
        Some(w) => json!({"kind": "intent.state_changed", "body": {"intent": i, "state": st, "workspace": w}}),
        None => json!({"kind": "intent.state_changed", "body": {"intent": i, "state": st}}),
    }
}

fn unit(i: &str, repo: &str) -> Value {
    json!({"kind": "unit.created", "body": {"intent": i, "title": "Decoy", "repo": repo, "branch": "feat/decoy", "base": "main"}})
}

fn seat_of<'a>(s: &'a Records, ws: &str) -> Option<&'a Seat> {
    s.state().workspace(ws).unwrap().manager.as_ref()
}

/// The seating of `session` in `ws` over the seat `replaces`, as an event.
fn seating(ws: &str, session: Option<&str>, replaces: Option<SeatRef>) -> Value {
    json!({"kind": "workspace.manager_set", "body": {"workspace": ws, "manager": session, "folder": "/w", "replaces": replaces}})
}

/// /p (p1) and /q (p2) registered, ws1 "chat" of /p with boss seated, ws2 "docs" of /q with
/// nobody. i1 (/p, planned), i2 (/p, active in ws1), i3 (/q, planned, the user's words),
/// i4 (no project, planned, the user's words).
fn setup() -> Records {
    let mut s = Records::default();
    assert_eq!((workspace(&mut s, "chat", "/p"), workspace(&mut s, "docs", "/q")), ("ws1".to_string(), "ws2".to_string()));
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "One", "note": "N1", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Two", "note": "N2", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Three", "note": "The user's own words", "project": "/q", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Four", "note": "The user's old words", "state": "planned"}})).unwrap();
    seat(&mut s, "ws1", "boss");
    add(&mut s, "user", state("i2", "active", Some("ws1"))).unwrap();
    s
}

/// A unit is part of its intent's plan: the intent's manager adds it, or the user. Nobody
/// else, and nobody but the user while the intent has no manager.
#[test]
fn units_are_added_by_the_intents_manager_or_the_user() {
    let mut s = setup();
    let e = add(&mut s, "worker", unit("i2", "/p/r")).unwrap_err();
    assert_eq!(e, "only the user or the manager of intent i2 (boss) adds its units; tell the manager (`workriver manager tell <your task> --text \"…\"`)");
    // Planned and never run: no manager, so not boss's either, whose project it is.
    for who in ["worker", "boss"] {
        let e = add(&mut s, who, unit("i1", "/p/r")).unwrap_err();
        assert_eq!(e, "i1 has no manager: units are added by the manager of the workspace it runs in, once it is started, or by the user", "{who}");
    }
    // Running where nobody is seated: none either.
    let empty = workspace(&mut s, "more", "/p");
    add(&mut s, "user", state("i1", "active", Some(&empty))).unwrap();
    assert!(add(&mut s, "boss", unit("i1", "/p/r")).unwrap_err().contains("i1 has no manager"));
    add(&mut s, "boss", unit("i2", "/p/r")).unwrap();
    add(&mut s, "user", unit("i1", "/p/r")).unwrap();
    assert_eq!(s.state().units.len(), 2);
}

/// An intent recorded with no project has none for the rules, whatever its units are: the
/// first unit's repository is shown, never judged by. So a unit does not turn "no project:
/// the user starts it" into a start its adder may make.
#[test]
fn a_unit_never_makes_an_intents_project() {
    let mut s = setup();
    assert!(add(&mut s, "boss", state("i4", "active", Some("ws1"))).unwrap_err().contains("i4 has no project"));
    // Its own unit is refused (the intent is nobody's yet) ...
    for who in ["boss", "stranger"] {
        assert!(add(&mut s, who, unit("i4", "/p/r")).unwrap_err().contains("i4 has no manager"), "{who}");
    }
    assert_eq!(s.state().project_folder(s.state().intent("i4").unwrap()), "");
    // ... and one the user adds gives it a folder to show, not a project to start it by.
    add(&mut s, "user", unit("i4", "/p/r")).unwrap();
    let i = s.state().intent("i4").unwrap();
    assert_eq!((s.state().project_folder(i).as_str(), s.state().intent_project(i).as_str()), ("/p", ""));
    assert!(add(&mut s, "boss", state("i4", "active", None)).unwrap_err().contains("i4 has no project: ask the user to start it"));
    // Nor a note to add to: the intent has no project, whatever boss's workspace is of.
    let e = add(&mut s, "boss", set("i4", json!({"add_note": "x"}))).unwrap_err();
    assert_eq!(e, "i4 is planned and has no project: only the user adds to its note; ask the user to add yours");
    let i = s.state().intent("i4").unwrap();
    assert_eq!((i.state, i.note.as_str()), (IntentState::Planned, "The user's old words"));
    // A recorded project is the registered one that holds it.
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Deep", "project": "/q/sub", "state": "planned"}})).unwrap();
    assert_eq!(s.state().intent_project(s.state().intent("i5").unwrap()), "/q");
    // A unit in another project's repository changes neither the intent's project nor the
    // workspace's, which is fixed when it is made.
    add(&mut s, "boss", unit("i2", "/q/r")).unwrap();
    assert_eq!((s.state().intent_project(s.state().intent("i2").unwrap()).as_str(), s.state().workspace("ws1").unwrap().project.as_str()), ("/p", "p1"));
    assert!(add(&mut s, "boss", state("i3", "active", None)).is_err(), "Q's intents stay out of a workspace of P");
}

/// Where an intent runs is set by a start. Done, back to the roadmap and dropped name no
/// workspace, from anyone: one that did would move the intent there.
#[test]
fn a_close_names_no_workspace() {
    let mut s = setup();
    let other = workspace(&mut s, "more", "/p");
    seat(&mut s, &other, "other");
    for who in ["boss", "user"] {
        for to in ["done", "planned", "dropped"] {
            let mut b = state("i2", to, Some(&other));
            b["body"]["reason"] = json!("r");
            let e = add(&mut s, who, b).unwrap_err();
            assert_eq!(e, "a state change names a workspace only when it starts an intent: leave the workspace out (an intent stays recorded where it last ran)", "{who} {to}");
        }
    }
    let i = s.state().intent("i2").unwrap();
    assert_eq!((i.state, i.workspace.as_deref()), (IntentState::Active, Some("ws1")));
    assert_eq!(s.state().workspace(&other).unwrap().current, None);
    add(&mut s, "boss", state("i2", "done", None)).unwrap();
    assert_eq!(s.state().intent("i2").unwrap().workspace.as_deref(), Some("ws1"), "recorded where it last ran");
    assert_eq!((s.state().manager_of("i2"), s.state().workspace("ws1").unwrap().current.as_deref()), (Some("boss"), None));
}

/// No event of an agent's sets a seat, a workspace or a project: making, renaming, closing
/// and opening a workspace, seating (itself included) and moving an intent are the user's;
/// that a seated session is back is workriver's own to record.
#[test]
fn no_agent_event_sets_a_seat_a_workspace_or_a_project() {
    let mut s = setup();
    let before = s.state().clone();
    let shown = seat_of(&s, "ws1").map(Seat::shown);
    let theirs = [
        ("workspace.created", json!({"name": "mine", "project": "p2"})),
        ("workspace.renamed", json!({"workspace": "ws2", "name": "mine"})),
        ("workspace.closed", json!({"workspace": "ws2"})),
        ("workspace.opened", json!({"workspace": "ws2"})),
        ("workspace.manager_set", json!({"workspace": "ws2", "manager": "worker", "replaces": null})),
        ("workspace.manager_set", json!({"workspace": "ws1", "manager": "worker", "replaces": shown})),
        ("workspace.manager_set", json!({"workspace": "ws1", "manager": null, "replaces": shown})),
        ("intent.workspace_set", json!({"intent": "i2", "workspace": "ws2"})),
        ("project.added", json!({"path": "/mine", "title": "Mine"})),
        ("project.renamed", json!({"project": "p1", "title": "Mine"})),
    ];
    for who in ["worker", "boss", "workriver"] {
        for (kind, b) in &theirs {
            let e = add(&mut s, who, json!({"kind": kind, "body": b})).unwrap_err();
            assert_eq!(e, format!("only the user does this ({kind}): in the web UI, or with the CLI from a terminal that is not an agent's"), "{who}");
        }
    }
    for who in ["worker", "boss", "user"] {
        let e = add(&mut s, who, json!({"kind": "workspace.manager_back", "body": {"workspace": "ws1", "session": "boss"}})).unwrap_err();
        assert!(e.contains("only workriver records this (workspace.manager_back)"), "{who}: {e}");
    }
    assert_eq!(*s.state(), before, "nothing changed");
    // workriver's own: only of the session that holds the seat; a line of the conversation,
    // and the seat as it was.
    assert!(add(&mut s, "workriver", json!({"kind": "workspace.manager_back", "body": {"workspace": "ws1", "session": "worker"}})).unwrap_err().contains("worker does not hold the seat of ws1"));
    add(&mut s, "workriver", json!({"kind": "workspace.manager_back", "body": {"workspace": "ws1", "session": "boss"}})).unwrap();
    let m = s.state().messages.last().unwrap();
    assert_eq!((m.id.as_str(), m.workspace.as_str(), m.manager.as_str(), m.notice, m.by_manager, m.text.as_str()), ("m1", "ws1", "boss", Some(Notice::ManagerBack), false, ""));
    assert_eq!(seat_of(&s, "ws1"), before.workspaces[0].manager.as_ref());
    assert!(add(&mut s, "boss", json!({"kind": "message.replied", "body": {"message": "m1", "text": "ok"}})).unwrap_err().contains("nothing to reply to"));
    assert_eq!(replayed(&s), *s.state());
}

/// A manager starts intents only in the workspace it holds the seat of (filled in from the
/// seat; a body naming another is refused) and only that workspace's project's; the user
/// names any open workspace of the intent's project. Nobody else starts one.
#[test]
fn a_start_names_only_the_seated_workspace() {
    let mut s = setup();
    seat(&mut s, "ws2", "other");
    let e = add(&mut s, "boss", state("i1", "active", Some("ws2"))).unwrap_err();
    assert_eq!(e, "you manage workspace ws1 (chat): a manager starts intents there, and nowhere else");
    let e = add(&mut s, "boss", state("i1", "active", Some("ws9"))).unwrap_err();
    assert!(e.contains("you manage workspace ws1"), "{e}");
    let e = add(&mut s, "worker", state("i1", "active", Some("ws1"))).unwrap_err();
    assert!(e.contains("only a workspace's manager, or the user, starts an intent") && e.contains(SEATING), "{e}");
    // Another project's intent: not in its workspace, and not in that project's either.
    let e = add(&mut s, "boss", state("i3", "active", None)).unwrap_err();
    assert_eq!(e, "i3 belongs to project Q; workspace ws1 (chat) is of project P: an intent runs in a workspace of its project");
    assert!(add(&mut s, "boss", state("i3", "active", Some("ws2"))).unwrap_err().contains("you manage workspace ws1"));
    // Its own: with no workspace named, or its own; recorded with the seat's.
    let e = add(&mut s, "boss", state("i1", "active", None)).unwrap();
    assert_eq!(e.body, body(state("i1", "active", Some("ws1"))));
    add(&mut s, "boss", state("i1", "planned", None)).unwrap();
    add(&mut s, "boss", state("i1", "active", Some("ws1"))).unwrap();
    let st = s.state();
    assert_eq!((st.intent("i1").unwrap().workspace.as_deref(), st.workspace("ws1").unwrap().current.as_deref(), st.manager_of("i1")), (Some("ws1"), Some("i1"), Some("boss")));
    // The user: says where, in an open workspace of the intent's project.
    assert!(add(&mut s, "user", state("i3", "active", None)).unwrap_err().contains("say where i3 starts"));
    assert!(add(&mut s, "user", state("i3", "active", Some("ws9"))).unwrap_err().contains("no workspace ws9"));
    assert!(add(&mut s, "user", state("i3", "active", Some("ws1"))).unwrap_err().contains("i3 belongs to project Q; workspace ws1 (chat) is of project P"));
    add(&mut s, "user", state("i3", "active", Some("ws2"))).unwrap();
    assert_eq!(s.state().manager_of("i3"), Some("other"));
    assert_eq!(replayed(&s), *s.state());
}

/// An intent of no project is the user's to start, anywhere: it takes the workspace's
/// project, in the records, for good.
#[test]
fn a_projectless_intent_takes_the_project_of_the_workspace_it_starts_in() {
    let mut s = setup();
    seat(&mut s, "ws2", "other");
    for (who, w) in [("boss", None), ("other", Some("ws2"))] {
        assert!(add(&mut s, who, state("i4", "active", w)).unwrap_err().contains("i4 has no project: ask the user to start it"), "{who}");
    }
    add(&mut s, "user", state("i4", "active", Some("ws2"))).unwrap();
    let i = s.state().intent("i4").unwrap();
    assert_eq!((i.project.as_str(), s.state().intent_project(i).as_str(), i.workspace.as_deref()), ("/q", "/q", Some("ws2")));
    // Back in the roadmap it is Q's: Q's manager starts it, P's does not.
    add(&mut s, "other", state("i4", "planned", None)).unwrap();
    assert!(add(&mut s, "boss", state("i4", "active", None)).unwrap_err().contains("i4 belongs to project Q"));
    add(&mut s, "other", state("i4", "active", None)).unwrap();
    assert_eq!(replayed(&s), *s.state());
}

/// An intent names, as its project, a folder a registered project holds, or none: whoever
/// records it. (The review's probe: an agent's planned intent naming `/Users/someone/.ssh`.)
#[test]
fn an_intent_names_a_registered_project_or_none() {
    let mut s = setup();
    let plan = |project: &str| json!({"kind": "intent.created", "body": {"title": "x", "state": "planned", "project": project}});
    for who in ["boss", "worker", "user"] {
        for folder in ["/Users/someone/.ssh", "/pp", "/", "p9"] {
            let e = add(&mut s, who, plan(folder)).unwrap_err();
            assert!(e.contains(&format!("{folder} is in no registered project")) && e.contains("workriver project add"), "{who} {folder}: {e}");
        }
    }
    assert_eq!(s.state().intents.len(), 4);
    add(&mut s, "worker", plan("/p/deep/er")).unwrap();
    add(&mut s, "worker", json!({"kind": "intent.created", "body": {"title": "nowhere", "state": "planned"}})).unwrap();
    let st = s.state();
    assert_eq!((st.intent_project(st.intent("i5").unwrap()).as_str(), st.intent_project(st.intent("i6").unwrap()).as_str()), ("/p", ""));
    // One recorded before the rule replays, and is of no project for the rules.
    let old = recorded_before(s, "boss", json!({"kind": "intent.created", "body": {"id": "i7", "title": "Old", "state": "planned", "project": "/Users/someone/.ssh"}}));
    let i = old.state().intent("i7").unwrap();
    assert_eq!((i.project.as_str(), old.state().intent_project(i).as_str(), old.state().project_folder(i).as_str()), ("/Users/someone/.ssh", "", ""));
    assert!(!old.state().projects.iter().any(|p| p.path == i.project));
}

/// The seat: one per workspace, one per session; set over the seat the user was shown and
/// no other; the user seats over a manager that runs; emptied by the user.
#[test]
fn a_seat_is_set_over_the_one_shown_and_a_session_holds_one() {
    let mut s = setup();
    let boss = seat_of(&s, "ws1").unwrap().clone();
    assert_eq!((boss.session.as_str(), boss.machine.as_deref(), boss.folder.as_str()), ("boss", None, "/w"));
    assert_eq!(boss.since, s.events().iter().find(|e| matches!(e.body, Body::WorkspaceManagerSet { .. })).unwrap().seq as u64, "the appointment is the seating's revision");
    // Against a seat other than the one it is: refused, saying who holds it.
    for stale in [None, Some(SeatRef { session: "boss".into(), since: boss.since + 1 }), Some(SeatRef { session: "old".into(), since: boss.since })] {
        let e = add(&mut s, "user", seating("ws1", Some("new"), stale)).unwrap_err();
        assert!(e.starts_with(SEAT_CHANGED) && e.contains("ws1's manager is boss now"), "{e}");
    }
    assert!(add(&mut s, "user", seating("ws2", Some("new"), Some(boss.shown()))).unwrap_err().starts_with(SEAT_CHANGED));
    assert_eq!(seat_of(&s, "ws1"), Some(&boss));
    // Over a manager that still runs, with the seat as shown: the user may.
    add(&mut s, "user", seating("ws1", Some("new"), Some(boss.shown()))).unwrap();
    let new = seat_of(&s, "ws1").unwrap().clone();
    assert!(new.session == "new" && new.since > boss.since);
    assert_eq!((s.state().manager_of("i2"), s.state().seat_of("boss").map(|w| w.id.as_str())), (Some("new"), None));
    // A session holds one seat: seated elsewhere, its old one is empty.
    add(&mut s, "user", seating("ws2", Some("new"), None)).unwrap();
    assert_eq!((seat_of(&s, "ws1"), seat_of(&s, "ws2").map(|m| m.session.as_str()), s.state().seat_of("new").map(|w| w.id.as_str())), (None, Some("new"), Some("ws2")));
    assert_eq!(s.state().manager_of("i2"), None);
    // Emptied by the user; an empty seat is not emptied; nobody is "user" or nameless.
    assert!(add(&mut s, "user", seating("ws1", None, None)).unwrap_err().contains("ws1 has no manager"));
    for bad in ["", " ", "user", "workriver"] {
        assert!(add(&mut s, "user", seating("ws1", Some(bad), None)).unwrap_err().contains("a manager is an agent"), "{bad:?}");
    }
    let shown = seat_of(&s, "ws2").map(Seat::shown);
    let e = add(&mut s, "user", json!({"kind": "workspace.manager_set", "body": {"workspace": "ws2", "manager": null, "machine": "box", "folder": "/x", "replaces": shown}})).unwrap();
    assert_eq!(e.body, body(json!({"kind": "workspace.manager_set", "body": {"workspace": "ws2", "manager": null, "replaces": shown}})), "an empty seat has no machine or folder");
    assert_eq!(seat_of(&s, "ws2"), None);
    assert!(add(&mut s, "user", seating("ws9", Some("new"), None)).unwrap_err().contains("no workspace ws9"));
    assert_eq!(replayed(&s), *s.state());
}

/// The appointment is kept only where the session is seated again where it was: the same
/// session, machine and folder. Another folder, machine or session is a new appointment.
#[test]
fn an_appointment_is_kept_only_where_the_session_was() {
    let mut s = setup();
    let since = |s: &Records| seat_of(s, "ws1").unwrap().since;
    seat_at(&mut s, "ws1", Some("boss"), Some("box"), "/home/me/app").unwrap();
    let first = since(&s);
    seat_at(&mut s, "ws1", Some("boss"), Some("box"), "/home/me/app").unwrap();
    assert_eq!(since(&s), first, "the same session where it was");
    seat_at(&mut s, "ws1", Some("boss"), Some("box"), "/home/me/other").unwrap();
    let second = since(&s);
    assert!(second > first, "another folder");
    seat_at(&mut s, "ws1", Some("boss"), None, "/home/me/other").unwrap();
    let third = since(&s);
    assert!(third > second, "another machine");
    seat_at(&mut s, "ws1", Some("next"), None, "/home/me/other").unwrap();
    assert!(since(&s) > third, "another session");
    // Emptied and seated again where it was: a new one too (the seat was not its).
    let fourth = since(&s);
    seat_at(&mut s, "ws1", None, None, "").unwrap();
    seat_at(&mut s, "ws1", Some("next"), None, "/home/me/other").unwrap();
    assert!(since(&s) > fourth);
    assert_eq!(replayed(&s), *s.state());
}

/// Closing a workspace: refused while it has an active intent; it empties the seat and
/// keeps its intents; nothing starts or is seated there until the user opens it again.
/// A name is shown only: it may repeat, and renaming changes nothing else.
#[test]
fn a_workspace_is_closed_opened_and_renamed_by_the_user() {
    let mut s = setup();
    let e = add(&mut s, "user", json!({"kind": "workspace.closed", "body": {"workspace": "ws1"}})).unwrap_err();
    assert!(e.contains("ws1 has an active intent (i2)"), "{e}");
    assert!(add(&mut s, "user", json!({"kind": "workspace.opened", "body": {"workspace": "ws1"}})).unwrap_err().contains("ws1 is open"));
    add(&mut s, "boss", state("i2", "done", None)).unwrap();
    add(&mut s, "user", json!({"kind": "workspace.closed", "body": {"workspace": "ws1"}})).unwrap();
    let w = s.state().workspace("ws1").unwrap();
    assert!(w.closed && w.manager.is_none());
    assert_eq!((s.state().intent("i2").unwrap().workspace.as_deref(), s.state().manager_of("i2"), s.state().seat_of("boss")), (Some("ws1"), None, None));
    assert!(add(&mut s, "user", json!({"kind": "workspace.closed", "body": {"workspace": "ws1"}})).unwrap_err().contains("closed already"));
    assert!(add(&mut s, "user", state("i1", "active", Some("ws1"))).unwrap_err().contains("workspace ws1 is closed"));
    assert!(add(&mut s, "user", seating("ws1", Some("boss"), None)).unwrap_err().contains("ws1 is closed: open it first"));
    add(&mut s, "user", json!({"kind": "workspace.opened", "body": {"workspace": "ws1"}})).unwrap();
    assert!(seat_of(&s, "ws1").is_none(), "opened with the seat empty");
    add(&mut s, "user", state("i1", "active", Some("ws1"))).unwrap();
    // Names: shown only.
    assert!(add(&mut s, "user", json!({"kind": "workspace.renamed", "body": {"workspace": "ws1", "name": " "}})).is_err());
    add(&mut s, "user", json!({"kind": "workspace.renamed", "body": {"workspace": "ws1", "name": " docs "}})).unwrap();
    let twin = workspace(&mut s, "docs", "/p");
    let st = s.state();
    assert_eq!(st.workspaces.iter().map(|w| (w.id.as_str(), w.name.as_str(), w.project.as_str())).collect::<Vec<_>>(), [("ws1", "docs", "p1"), ("ws2", "docs", "p2"), ("ws3", "docs", "p1")]);
    assert_eq!((twin.as_str(), st.intent("i1").unwrap().workspace.as_deref(), st.workspace("ws3").unwrap().current.as_deref()), ("ws3", Some("ws1"), None));
    // Made in a registered project, by its id or folder, with a name.
    for (name, project, says) in [("x", "/nowhere", "no project /nowhere"), ("x", "p9", "no project p9"), (" ", "p1", "needs a name")] {
        let e = add(&mut s, "user", json!({"kind": "workspace.created", "body": {"name": name, "project": project}})).unwrap_err();
        assert!(e.contains(says), "{e}");
    }
    assert_eq!(replayed(&s), *s.state());
}

/// The user moves an active intent to another open workspace of its project: it is then
/// that workspace's current intent and its manager's; where it was, the intent started last
/// among those still active is current again.
#[test]
fn the_user_moves_an_active_intent_to_another_workspace_of_its_project() {
    let mut s = setup();
    add(&mut s, "boss", state("i1", "active", None)).unwrap();
    let more = workspace(&mut s, "more", "/p");
    seat(&mut s, &more, "other");
    let moved = |i: &str, w: &str| json!({"kind": "intent.workspace_set", "body": {"intent": i, "workspace": w}});
    for who in ["boss", "other"] {
        assert!(add(&mut s, who, moved("i1", &more)).unwrap_err().contains("only the user does this (intent.workspace_set)"), "{who}");
    }
    for (i, w, says) in [("i3", "ws2", "i3 is planned"), ("i1", "ws2", "i1 belongs to project P; workspace ws2 (docs) is of project Q"), ("i1", "ws1", "runs in ws1 already"), ("i1", "ws9", "no workspace ws9"), ("i9", "ws1", "no intent i9")] {
        let e = add(&mut s, "user", moved(i, w)).unwrap_err();
        assert!(e.contains(says), "{i} -> {w}: {e}");
    }
    assert_eq!(s.state().workspace("ws1").unwrap().current.as_deref(), Some("i1"));
    add(&mut s, "user", moved("i1", &more)).unwrap();
    let st = s.state();
    assert_eq!((st.intent("i1").unwrap().workspace.as_deref(), st.manager_of("i1"), st.manager_of("i2")), (Some(more.as_str()), Some("other"), Some("boss")));
    assert_eq!((st.workspace("ws1").unwrap().current.as_deref(), st.workspace(&more).unwrap().current.as_deref()), (Some("i2"), Some("i1")));
    add(&mut s, "user", state("i4", "active", Some(&more))).unwrap();
    assert!(add(&mut s, "user", moved("i4", "ws2")).unwrap_err().contains("i4 belongs to project P"), "it took P when it started");
    add(&mut s, "user", json!({"kind": "workspace.closed", "body": {"workspace": "ws2"}})).unwrap();
    assert!(add(&mut s, "user", moved("i4", "ws2")).unwrap_err().contains("workspace ws2 is closed"));
    assert_eq!(replayed(&s), *s.state());
}

/// An agent's workspace, where a command leaves it out, comes from the records alone: the
/// one whose seat it holds; else the one workspace of the intents of the tasks it has
/// (started, not over); none when those are in two, or it has none.
#[test]
fn an_agents_workspace_comes_from_the_records() {
    let mut s = setup();
    seat(&mut s, "ws2", "other");
    add(&mut s, "user", state("i3", "active", Some("ws2"))).unwrap();
    add(&mut s, "boss", json!({"kind": "unit.created", "body": {"intent": "i2", "title": "A", "repo": "/p/r", "branch": "feat/a", "base": "main"}})).unwrap();
    add(&mut s, "other", json!({"kind": "unit.created", "body": {"intent": "i3", "title": "B", "repo": "/q/r", "branch": "feat/b", "base": "main"}})).unwrap();
    for (who, u) in [("boss", "u1"), ("boss", "u1"), ("other", "u2")] {
        add(&mut s, who, json!({"kind": "task.created", "body": {"unit": u, "kind": "implement", "title": "T", "inputs": [{"from": "start"}]}})).unwrap();
    }
    let of = |s: &Records, who: &str| s.state().workspace_of_agent(who).map(|w| w.id.clone());
    assert_eq!((of(&s, "boss"), of(&s, "other"), of(&s, "worker")), (Some("ws1".into()), Some("ws2".into()), None));
    add(&mut s, "worker", json!({"kind": "task.started", "body": {"unit": "u1", "task": 1}})).unwrap();
    assert_eq!(of(&s, "worker"), Some("ws1".into()), "the workspace of its one task");
    add(&mut s, "worker", json!({"kind": "task.started", "body": {"unit": "u1", "task": 2}})).unwrap();
    assert_eq!(of(&s, "worker"), Some("ws1".into()), "two tasks, one workspace");
    add(&mut s, "worker", json!({"kind": "task.started", "body": {"unit": "u2", "task": 1}})).unwrap();
    assert_eq!(of(&s, "worker"), None, "tasks in two workspaces: asked, never guessed");
    add(&mut s, "worker", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1}})).unwrap();
    add(&mut s, "worker", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 2}})).unwrap();
    assert_eq!(of(&s, "worker"), Some("ws2".into()), "delivered tasks no longer count");
    // The seat goes before the tasks.
    add(&mut s, "boss", json!({"kind": "task.started", "body": {"unit": "u2", "task": 1, "agent": "boss"}})).ok();
    assert_eq!(of(&s, "boss"), Some("ws1".into()));
}

/// A deferred finding from outside the tasks goes into a registered project's backlog, as
/// an intent does: a folder no registered project holds is refused, whoever records it.
#[test]
fn a_backlog_finding_names_a_registered_project() {
    let mut s = Records::default();
    project(&mut s, "/p");
    let found = |project: &str| json!({"kind": "finding.added", "body": {"project": project, "finding": {"severity": "P2", "problem": "Leak", "impact": "Slow", "where": ["a"]}}});
    for who in ["boss", "user"] {
        let e = add(&mut s, who, found("/Users/someone/.ssh")).unwrap_err();
        assert!(e.contains("is in no registered project") && e.contains("workriver project add"), "{who}: {e}");
    }
    add(&mut s, "boss", found("/p/app")).unwrap();
    assert_eq!(s.state().backlog.len(), 1);
}
