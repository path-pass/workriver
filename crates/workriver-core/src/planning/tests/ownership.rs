// Who decides an intent's project, where it runs and its manager: only the user, or a start
// the rules checked. Each test is a way an agent once got around that through another event.
use serde_json::{Value, json};

use crate::records::tests::*;
use crate::*;

fn set(i: &str, f: Value) -> Value {
    let mut b = json!({"intent": i});
    b.as_object_mut().unwrap().extend(f.as_object().unwrap().clone());
    json!({"kind": "intent.updated", "body": b})
}

fn state(i: &str, st: &str, w: Option<&Value>) -> Value {
    match w {
        Some(w) => json!({"kind": "intent.state_changed", "body": {"intent": i, "state": st, "workspace": w}}),
        None => json!({"kind": "intent.state_changed", "body": {"intent": i, "state": st}}),
    }
}

fn registered(w: &Value) -> Value {
    json!({"kind": "manager.registered", "body": {"workspace": w}})
}

fn unit(i: &str, repo: &str) -> Value {
    json!({"kind": "unit.created", "body": {"intent": i, "title": "Decoy", "repo": repo, "branch": "feat/decoy", "base": "main"}})
}

fn group(v: &Value) -> RuntimeRef {
    serde_json::from_value(v.clone()).unwrap()
}

/// The project the rules read for the workspace recorded under `id`.
fn project(s: &Records, id: &str) -> Option<String> {
    let st = s.state();
    st.workspaces.iter().find(|w| w.group.id == id).and_then(|w| st.workspace_project(w))
}

/// Each record as `id:label:manager`.
fn records(s: &Records) -> Vec<String> {
    s.state().workspaces.iter().map(|w| format!("{}:{}:{}", w.group.id, w.group.label, w.manager.as_deref().unwrap_or("-"))).collect()
}

/// /p and /q registered. i1 (/p, planned), i2 (/p, active in w1: boss's, set to /p by the
/// user), i3 (/q, planned, the user's words), i4 (no project, planned, the user's words).
fn setup() -> Records {
    let mut s = Records::default();
    let w1 = json!({"id": "w1", "label": "chat"});
    add(&mut s, "user", json!({"kind": "project.added", "body": {"path": "/p", "title": "P"}})).unwrap();
    add(&mut s, "user", json!({"kind": "project.added", "body": {"path": "/q", "title": "Q"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "One", "note": "N1", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Two", "note": "N2", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Three", "note": "The user's own words", "project": "/q", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Four", "note": "The user's old words", "state": "planned"}})).unwrap();
    add(&mut s, "boss", registered(&w1)).unwrap();
    add(&mut s, "user", json!({"kind": "workspace.project_set", "body": {"workspace": w1, "project": "/p"}})).unwrap();
    add(&mut s, "user", state("i2", "active", Some(&w1))).unwrap();
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
    // Running where nobody registered: none either.
    add(&mut s, "user", state("i1", "active", Some(&json!({"id": "w2", "label": "docs"})))).unwrap();
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
    let w1 = json!({"id": "w1", "label": "chat"});
    assert!(add(&mut s, "boss", state("i4", "active", Some(&w1))).unwrap_err().contains("i4 has no project"));
    // Its own unit is refused (the intent is nobody's yet) ...
    for who in ["boss", "stranger"] {
        assert!(add(&mut s, who, unit("i4", "/p/r")).unwrap_err().contains("i4 has no manager"), "{who}");
    }
    assert_eq!(s.state().project_folder(s.state().intent("i4").unwrap()), "");
    // ... and one the user adds gives it a folder to show, not a project to start it by.
    add(&mut s, "user", unit("i4", "/p/r")).unwrap();
    let i = s.state().intent("i4").unwrap();
    assert_eq!((s.state().project_folder(i).as_str(), s.state().intent_project(i).as_str()), ("/p", ""));
    assert!(add(&mut s, "boss", state("i4", "active", Some(&w1))).unwrap_err().contains("i4 has no project: ask the user to start it"));
    // Nor a note to add to: the intent has no project, whatever boss's workspace works on,
    // so nothing boss could have set would make it boss's.
    let e = add(&mut s, "boss", set("i4", json!({"add_note": "x"}))).unwrap_err();
    assert_eq!(e, "i4 is planned and has no project: only the user adds to its note; ask the user to add yours");
    let i = s.state().intent("i4").unwrap();
    assert_eq!((i.state, i.note.as_str()), (IntentState::Planned, "The user's old words"));
    // A recorded project is the registered one that holds it.
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Deep", "project": "/q/sub", "state": "planned"}})).unwrap();
    assert_eq!(s.state().intent_project(s.state().intent("i5").unwrap()), "/q");
}

/// Nor a workspace's project: the user starts a project-less intent in a workspace whose
/// project nothing recorded says, and its manager adds a unit in another project's
/// repository. The workspace's project stays not known, and that project's intents stay the
/// user's to start.
#[test]
fn a_unit_never_makes_a_workspaces_project() {
    let mut s = setup();
    let w2 = json!({"id": "w2", "label": "docs"});
    add(&mut s, "idle", registered(&w2)).unwrap();
    add(&mut s, "user", state("i4", "active", Some(&w2))).unwrap();
    assert!(add(&mut s, "idle", state("i3", "active", Some(&w2))).is_err(), "not known: refused");
    add(&mut s, "idle", unit("i4", "/q/r")).expect("its manager adds its units");
    assert_eq!(project(&s, "w2"), None);
    let e = add(&mut s, "idle", state("i3", "active", Some(&w2))).unwrap_err();
    assert!(e.contains("your workspace's project is not known yet"), "{e}");
    assert!(add(&mut s, "idle", set("i3", json!({"add_note": "x"}))).is_err());
    assert_eq!(s.state().intent("i3").unwrap().note, "The user's own words");
    // Once the intent is closed it "ran" there: still no project to go by.
    add(&mut s, "idle", state("i4", "done", None)).unwrap();
    assert_eq!(project(&s, "w2"), None);
}

/// Where an intent runs is set by a start. Done, back to the roadmap and dropped name no
/// workspace, from anyone: one that did would move the intent there, and with it that
/// workspace's project.
#[test]
fn a_close_names_no_workspace() {
    let mut s = Records::default();
    let w1 = json!({"id": "w1", "label": "chat"});
    let w2 = json!({"id": "w2", "label": "docs"});
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "P work", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "The user's", "project": "/p", "state": "planned", "note": "the user's text"}})).unwrap();
    add(&mut s, "boss", registered(&w1)).unwrap();
    add(&mut s, "other", registered(&w2)).unwrap();
    add(&mut s, "user", state("i1", "active", Some(&w1))).unwrap();
    assert!(add(&mut s, "other", state("i2", "active", Some(&w2))).is_err(), "w2's project is not known");
    for who in ["boss", "user"] {
        for to in ["done", "planned", "dropped"] {
            let mut b = state("i1", to, Some(&w2));
            b["body"]["reason"] = json!("r");
            let e = add(&mut s, who, b).unwrap_err();
            assert_eq!(e, "a state change names a workspace only when it starts an intent: leave the workspace out (an intent stays recorded where it last ran)", "{who} {to}");
        }
    }
    let i = s.state().intent("i1").unwrap();
    assert_eq!((i.state, i.workspace.as_ref().map(|g| g.id.as_str())), (IntentState::Active, Some("w1")));
    assert_eq!(project(&s, "w2"), None);
    assert!(add(&mut s, "other", state("i2", "active", Some(&w2))).is_err());
    add(&mut s, "boss", state("i1", "done", None)).unwrap();
    assert_eq!(s.state().intent("i1").unwrap().workspace.as_ref().map(|g| g.id.as_str()), Some("w1"), "recorded where it last ran");
}

/// The manager registered for an intent itself (before 0.2) counts only for an intent with
/// no workspace at all. It does not follow the intent into a workspace where nobody
/// registered: that intent has no manager, and the user acts for it.
#[test]
fn a_per_intent_manager_does_not_follow_the_intent() {
    let mut s = Records::default();
    let w1 = json!({"id": "w1", "label": "chat"});
    let w2 = json!({"id": "w2", "label": "docs"});
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "One", "note": "N1", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", state("i1", "active", Some(&w1))).unwrap();
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"intent": "i1", "workspace": w1}})).unwrap();
    assert_eq!(s.state().manager_of("i1"), Some("boss"));
    add(&mut s, "user", state("i1", "planned", None)).unwrap();
    add(&mut s, "user", state("i1", "active", Some(&w2))).unwrap();
    assert_eq!(s.state().intent("i1").unwrap().manager.as_deref(), Some("boss"), "still recorded on the intent");
    assert_eq!(s.state().manager_of("i1"), None);
    assert!(add(&mut s, "boss", state("i1", "done", None)).unwrap_err().contains("only the user or the manager of intent i1"));
    assert!(add(&mut s, "boss", set("i1", json!({"note": "x", "base": 1}))).is_err());
    // An old record with no workspace at all keeps the manager it was given.
    let old = recorded_before(Records::default(), "user", json!({"kind": "intent.created", "body": {"id": "i1", "title": "Old", "state": "active"}}));
    let old = recorded_before(old, "boss", json!({"kind": "manager.registered", "body": {"intent": "i1", "agent": "boss"}}));
    assert_eq!(old.state().manager_of("i1"), Some("boss"));
}

/// Only an event that names a workspace writes its record's id and label. Closing an
/// intent finds the record it ran in and leaves it as it is: it is not renamed to what the
/// intent was started with. Here the workspace was renamed since ("docs" to "api"), another
/// one is now "docs", and a workspace of another machine is linked to "docs": closing the
/// old intent must not give the first workspace that name back, and the link with it.
#[test]
fn closing_an_intent_never_renames_its_workspaces_record() {
    for to in ["done", "planned", "dropped"] {
        let mut s = Records::default();
        let (docs1, api1, docs2) = (json!({"id": "w1", "label": "docs"}), json!({"id": "w1", "label": "api"}), json!({"id": "w2", "label": "docs"}));
        let far = json!({"id": "y@box", "label": "y", "machine": "box"});
        add(&mut s, "boss", registered(&docs1)).unwrap();
        add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Old", "state": "planned"}})).unwrap();
        add(&mut s, "user", state("i1", "active", Some(&docs1))).unwrap();
        add(&mut s, "boss", registered(&api1)).unwrap();
        add(&mut s, "other", registered(&docs2)).unwrap();
        add(&mut s, "user", json!({"kind": "workspace.linked", "body": {"machine": "box", "workspace": "y", "local": "docs"}})).unwrap();
        add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Over there", "state": "planned"}})).unwrap();
        add(&mut s, "user", state("i2", "active", Some(&far))).unwrap();
        assert_eq!(s.state().manager_of("i2"), Some("other"));
        let mut close = state("i1", to, None);
        close["body"]["reason"] = json!("r");
        add(&mut s, "boss", close).unwrap();
        assert_eq!(records(&s), ["w1:api:boss", "w2:docs:other"], "{to}");
        assert_eq!(s.state().manager_of("i2"), Some("other"), "{to}");
        assert_eq!(s.state().workspace_of(&group(&api1)).unwrap().current, None, "{to}: no longer where new work goes");
        assert_eq!(replayed(&s).workspaces, s.state().workspaces, "{to}");
    }
}

/// The same for a registration that names only an intent (before 0.2): it sets the manager
/// of the record the intent's workspace is found at, and makes one when there is none.
#[test]
fn a_registration_naming_only_an_intent_never_renames_the_record() {
    let mut s = Records::default();
    let (docs1, api1) = (json!({"id": "w1", "label": "docs"}), json!({"id": "w1", "label": "api"}));
    add(&mut s, "old", registered(&docs1)).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Old", "state": "planned"}})).unwrap();
    add(&mut s, "user", state("i1", "active", Some(&docs1))).unwrap();
    add(&mut s, "old", registered(&api1)).unwrap();
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"intent": "i1"}})).unwrap();
    assert_eq!(records(&s), ["w1:api:boss"]);
    assert_eq!(s.state().manager_of("i1"), Some("boss"));
    // No record yet (an intent the user made in a group, planned): made, as the intent has it.
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "New", "state": "planned", "runtime_ref": {"id": "w7", "label": "seven"}}})).unwrap();
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"intent": "i2"}})).unwrap();
    assert_eq!(records(&s), ["w1:api:boss", "w7:seven:boss"]);
    // An intent the user made under another id of a recorded label, never started there, is
    // found at that record by the label: registering for it, and dropping it, find the
    // record and leave its id as it is.
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Elsewhere", "state": "planned", "runtime_ref": {"id": "w8", "label": "api"}}})).unwrap();
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"intent": "i3"}})).unwrap();
    assert_eq!(records(&s), ["w1:api:boss", "w7:seven:boss"]);
    add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i3", "state": "dropped", "reason": "r"}})).unwrap();
    assert_eq!(records(&s), ["w1:api:boss", "w7:seven:boss"]);
    assert_eq!(replayed(&s).workspaces, s.state().workspaces);
}

/// Where the rules stop, as `docs/design/caller-identity.md` writes it: they take a
/// workspace as an event names it, and a link finds the first local record with its label.
/// So a registration naming the caller's own id under the label a link names would take the
/// linked workspace's intents. The rules know no runtime and cannot refuse it; the service
/// records an agent's workspace as the runtime names it, never as the request does, and its
/// tests hold that (`server/tests/ownership.rs`). What is left is a rename in the runtime.
#[test]
fn the_rules_take_a_workspace_as_it_is_named() {
    let mut s = setup();
    let w2 = json!({"id": "w2", "label": "docs"});
    let far = json!({"id": "x@box", "label": "x", "machine": "box"});
    add(&mut s, "other", registered(&w2)).unwrap();
    add(&mut s, "user", json!({"kind": "workspace.linked", "body": {"machine": "box", "workspace": "x", "local": "docs"}})).unwrap();
    add(&mut s, "user", state("i3", "active", Some(&far))).unwrap();
    assert_eq!(s.state().manager_of("i3"), Some("other"));
    add(&mut s, "boss", registered(&json!({"id": "w1", "label": "docs"}))).unwrap();
    assert_eq!(records(&s), ["w1:docs:boss", "w2:docs:other"]);
    assert_eq!(s.state().manager_of("i3"), Some("boss"), "the limit: the first record with the link's label");
}

/// An intent runs in its workspace's record, not under the label it was started with. The
/// record follows its workspace to a new id (the runtime restarted) and to a new label (the
/// user renamed it), and the intents that run or ran there follow the record. So the next
/// workspace opened under the old label is a workspace of its own: its manager is not the
/// manager of an intent that never ran there.
#[test]
fn an_intent_follows_its_workspaces_record() {
    for order in [["restart", "rename"], ["rename", "restart"]] {
        let mut s = Records::default();
        add(&mut s, "user", json!({"kind": "project.added", "body": {"path": "/p", "title": "P"}})).unwrap();
        add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Mine", "note": "The user's words", "project": "/p", "state": "planned"}})).unwrap();
        add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Done there", "project": "/p", "state": "planned"}})).unwrap();
        let w1 = json!({"id": "w1", "label": "docs"});
        add(&mut s, "boss", registered(&w1)).unwrap();
        add(&mut s, "user", state("i2", "active", Some(&w1))).unwrap();
        add(&mut s, "boss", state("i2", "done", None)).unwrap();
        add(&mut s, "user", state("i1", "active", Some(&w1))).unwrap();
        // Each step is recorded by the next event that names the workspace.
        let steps = if order[0] == "restart" { [json!({"id": "w9", "label": "docs"}), json!({"id": "w9", "label": "notes"})] } else { [json!({"id": "w1", "label": "notes"}), json!({"id": "w9", "label": "notes"})] };
        for (step, g) in order.iter().zip(&steps) {
            add(&mut s, "boss", registered(g)).unwrap();
            assert_eq!(s.state().manager_of("i1"), Some("boss"), "{order:?}: after the {step}");
        }
        let now = group(&steps[1]);
        for i in ["i1", "i2"] {
            assert_eq!(s.state().intent(i).unwrap().workspace.as_ref(), Some(&now), "{order:?}: {i} is where its record is");
        }
        // Another workspace, open under the old name: its agent registers in its own.
        add(&mut s, "newcomer", registered(&json!({"id": "w5", "label": "docs"}))).unwrap();
        assert_eq!(records(&s), ["w9:notes:boss", "w5:docs:newcomer"], "{order:?}");
        assert_eq!(s.state().manager_of("i1"), Some("boss"), "{order:?}");
        assert!(add(&mut s, "newcomer", set("i1", json!({"note": "replaced", "base": 1}))).unwrap_err().contains("only the user or the manager of intent i1"));
        assert!(add(&mut s, "newcomer", unit("i1", "/p/r")).unwrap_err().contains("only the user or the manager of intent i1 (boss)"));
        assert!(add(&mut s, "newcomer", state("i1", "done", None)).unwrap_err().contains("only the user or the manager of intent i1"));
        assert_eq!(project(&s, "w5"), None, "{order:?}: nothing ran in the newcomer's");
        add(&mut s, "boss", set("i1", json!({"add_note": "its manager's"}))).unwrap();
        assert_eq!(replayed(&s).intents, s.state().intents, "{order:?}");
    }
}

/// A workspace of another machine linked to a local one keeps the group it runs in: only
/// the local record is renamed, and the link still names it by the label the user gave.
#[test]
fn a_linked_intent_keeps_its_own_group() {
    let mut s = Records::default();
    let far = json!({"id": "x@box", "label": "x", "machine": "box"});
    add(&mut s, "boss", registered(&json!({"id": "w1", "label": "docs"}))).unwrap();
    add(&mut s, "user", json!({"kind": "workspace.linked", "body": {"machine": "box", "workspace": "x", "local": "docs"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Over there", "state": "planned"}})).unwrap();
    add(&mut s, "user", state("i1", "active", Some(&far))).unwrap();
    add(&mut s, "boss", registered(&json!({"id": "w9", "label": "docs"}))).unwrap();
    assert_eq!(s.state().intent("i1").unwrap().workspace.as_ref(), Some(&group(&far)));
    assert_eq!(s.state().manager_of("i1"), Some("boss"));
}
