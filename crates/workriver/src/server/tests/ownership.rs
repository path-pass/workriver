// Who decides an intent's project, where it runs and its manager, through the service: only
// the user, or a start the rules checked. Agents are found from their process (no `--as`);
// each test is a way one once got around that, or a way two workspaces of one name were
// taken for one.
use serde_json::{Value, json};
use workriver_core::runtime::{AgentState::Idle, RuntimeAgent};

use super::support::*;
use crate::cli::Daemon;

/// An agent in a workspace labelled "docs".
fn docs(session: &str, name: &str, id: &str) -> RuntimeAgent {
    labelled(session, name, id, "docs")
}

fn labelled(session: &str, name: &str, id: &str, label: &str) -> RuntimeAgent {
    let mut a = live(session, name, id, Idle);
    a.group.as_mut().unwrap().label = label.into();
    a
}

fn group(d: &Daemon) -> Value {
    d.call("/api/here", Some(json!({}))).unwrap()["group"].clone()
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

/// Each workspace record as `id:label:manager:project`.
fn records(d: &Daemon) -> Vec<String> {
    let s = d.get("/api/state").unwrap();
    let text = |v: &Value| v.as_str().unwrap_or("-").to_string();
    s["workspaces"].as_array().unwrap().iter().map(|w| format!("{}:{}:{}:{}", text(&w["group"]["id"]), text(&w["group"]["label"]), text(&w["manager"]), text(&w["project"]))).collect()
}

fn repo(tmp: &std::path::Path, name: &str) -> String {
    let p = tmp.join(name);
    std::fs::create_dir(&p).unwrap();
    git(&p, &["init", "-q", "-b", "main"]);
    git(&p, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "init"]);
    p.canonicalize().unwrap().to_string_lossy().into_owned()
}

fn start(id: &str, workspace: &Value) -> Value {
    json!({ "intent": id, "state": "active", "workspace": workspace })
}

/// The status and words an event sent from `place` is answered with.
fn answered(d: &Daemon, place: &str, kind: &str, body: Value) -> (u16, String) {
    http_at(d, d.port, place, "POST", "/api/append", &[], Some(&json!({ "kind": kind, "body": body }).to_string()))
}

const SAME_NAME: &str = "is already recorded for another open workspace (w1); two open workspaces have one name, and workriver tells workspaces apart by name: ask the user to rename one of them";

/// boss manages w1 "docs"; w2 is another open workspace labelled "docs", where other runs.
/// Gives the fake runtime, the user's client, boss's, other's, and both groups.
fn two_open(tmp: &std::path::Path) -> (std::sync::Arc<Fake>, Daemon, Daemon, Daemon, Value, Value) {
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), docs("s-boss", "boss", "w1")), ("p2".into(), docs("s-other", "other", "w2"))];
    let d = daemon_with(fake.clone(), &tmp.join("w.db"), Default::default());
    let (boss, other) = (at_place(&d, "p1"), at_place(&d, "p2"));
    let (g1, g2) = (group(&boss), group(&other));
    boss.append("", "manager.registered", json!({ "workspace": g1 })).unwrap();
    (fake, d, boss, other, g1, g2)
}

/// A unit gives neither its intent nor its workspace a project. The user starts a
/// project-less intent in boss's workspace; boss, its manager, adds a unit in a repository of
/// project Q. Its workspace's project stays not known, so Q's planned intent stays the
/// user's to start. And a planned intent of no project takes no unit from an agent at all.
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
    let g1 = group(&boss);
    assert!(boss.append("", "intent.state_changed", start(&theirs, &g1)).is_err(), "not known: refused");
    let unit = |i: &str| json!({ "intent": i, "title": "Decoy", "repo": q, "branch": "feat/decoy", "base": "main" });
    boss.append("", "unit.created", unit(&own)).expect("its manager adds its units");
    let e = boss.append("", "intent.state_changed", start(&theirs, &g1)).unwrap_err().to_string();
    assert!(e.contains("your workspace's project is not known yet"), "{e}");
    assert!(boss.append("", "intent.updated", json!({ "intent": theirs, "add_note": "x" })).is_err());
    let e = boss.append("", "unit.created", unit(&old)).unwrap_err().to_string();
    assert!(e.contains(&format!("{old} has no manager: units are added by the manager of the workspace it runs in, once it is started, or by the user")), "{e}");
    let e = boss.append("", "intent.state_changed", start(&old, &g1)).unwrap_err().to_string();
    assert!(e.contains("has no project: ask the user to start it"), "{e}");
    for id in [&theirs, &old] {
        assert_eq!(intent(&d, id), ("planned".to_string(), "The user's own words".to_string()));
    }
}

/// A manager closes its own intent naming another workspace, which would make that
/// workspace's project its intent's: refused, and that workspace's manager still starts
/// nothing of the project.
#[test]
fn a_close_names_no_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), labelled("s-other", "other", "w2", "other"))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    let mine = repo(tmp.path(), "mine");
    d.append("user", "project.added", json!({ "path": mine, "title": "Mine" })).unwrap();
    let (boss, other) = (at_place(&d, "p1"), at_place(&d, "p2"));
    let (g1, g2) = (group(&boss), group(&other));
    boss.append("", "manager.registered", json!({ "workspace": g1 })).unwrap();
    other.append("", "manager.registered", json!({ "workspace": g2 })).unwrap();
    let own = planned(&d, "Own", Some(&mine));
    let theirs = planned(&d, "The user's", Some(&mine));
    d.append("user", "intent.state_changed", start(&own, &g1)).unwrap();
    assert!(other.append("", "intent.state_changed", start(&theirs, &g2)).is_err(), "w2's project is not known");
    for to in ["done", "planned", "dropped"] {
        let e = boss.append("", "intent.state_changed", json!({ "intent": own, "state": to, "workspace": g2, "reason": "r" })).unwrap_err().to_string();
        assert!(e.contains("a state change names a workspace only when it starts an intent"), "{to}: {e}");
    }
    assert!(other.append("", "intent.state_changed", start(&theirs, &g2)).is_err());
    assert!(other.append("", "intent.updated", json!({ "intent": theirs, "note": "x", "base": 1 })).is_err());
    assert_eq!(intent(&d, &own).0, "active");
    assert_eq!(intent(&d, &theirs), ("planned".to_string(), "The user's own words".to_string()));
}

/// An agent starts intents in the workspace the runtime gives it, and the start is
/// recorded as the runtime names that workspace. boss ("chat") starts its own project's
/// intent naming its own id under the label "docs", which a workspace of another machine is
/// linked to: recorded as "chat", so its record is not the link's, and the linked
/// workspace's intent stays its own manager's. A start naming another workspace, or from an
/// agent in none, is refused.
#[test]
fn an_agents_start_is_in_the_workspace_the_runtime_gives_it() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    let mut loose = live("s-loose", "loose", "w0", Idle);
    loose.group = None;
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), docs("s-other", "other", "w2")), ("p3".into(), loose)];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    let mine = repo(tmp.path(), "mine");
    d.append("user", "project.added", json!({ "path": mine, "title": "Mine" })).unwrap();
    let (boss, other) = (at_place(&d, "p1"), at_place(&d, "p2"));
    let (g1, g2) = (group(&boss), group(&other));
    boss.append("", "manager.registered", json!({ "workspace": g1 })).unwrap();
    other.append("", "manager.registered", json!({ "workspace": g2 })).unwrap();
    d.append("user", "workspace.project_set", json!({ "workspace": g1, "project": mine })).unwrap();
    d.append("user", "workspace.linked", json!({ "machine": "box", "workspace": "x", "local": "docs" })).unwrap();
    let (own, next) = (planned(&d, "Own", Some(&mine)), planned(&d, "Next", Some(&mine)));
    let far = planned(&d, "Over there", None);
    d.append("user", "intent.state_changed", start(&far, &json!({ "id": "x@box", "label": "x", "machine": "box" }))).unwrap();
    let mut forged = g1.clone();
    forged["label"] = json!("docs");
    let started = boss.append("", "intent.state_changed", start(&own, &forged)).unwrap();
    assert_eq!(started["body"]["workspace"], g1, "as the runtime names it");
    assert_eq!(records(&d), [format!("w1:chat:s-boss:{mine}"), "w2:docs:s-other:-".to_string()]);
    assert!(boss.append("", "intent.updated", json!({ "intent": far, "note": "x", "base": 1 })).is_err());
    assert!(boss.append("", "intent.state_changed", json!({ "intent": far, "state": "planned" })).is_err());
    assert_eq!(intent(&d, &far), ("active".to_string(), "The user's own words".to_string()));

    let refusal = "an agent starts intents in the workspace it runs in: run `workriver intent start <i>` from your own terminal";
    for (place, named) in [("p1", &g2), ("p3", &g1)] {
        let (status, said) = answered(&d, place, "intent.state_changed", start(&next, named));
        assert!(status == 403 && said.contains(refusal), "{place}: {status} {said}");
    }
    assert_eq!(intent(&d, &next).0, "planned");
    boss.append("", "intent.state_changed", start(&next, &g1)).expect("in its own");
}

/// The same for a registration: under its own id and the label a link names, it is
/// recorded as the runtime names the workspace, and the linked workspace's intent stays
/// with the manager of the workspace it is linked to.
#[test]
fn a_registration_is_in_the_workspace_the_runtime_gives_it() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), docs("s-other", "other", "w2"))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    let (boss, other) = (at_place(&d, "p1"), at_place(&d, "p2"));
    let (g1, g2) = (group(&boss), group(&other));
    // boss first: a label is matched to the first record that has it.
    boss.append("", "manager.registered", json!({ "workspace": g1 })).unwrap();
    other.append("", "manager.registered", json!({ "workspace": g2 })).unwrap();
    d.append("user", "workspace.linked", json!({ "machine": "box", "workspace": "x", "local": "docs" })).unwrap();
    let far = planned(&d, "Over there", None);
    d.append("user", "intent.state_changed", start(&far, &json!({ "id": "x@box", "label": "x", "machine": "box" }))).unwrap();
    let mut forged = g1.clone();
    forged["label"] = json!("docs");
    let made = boss.append("", "manager.registered", json!({ "workspace": forged })).unwrap();
    assert_eq!(made["body"]["workspace"], g1);
    assert!(boss.append("", "intent.updated", json!({ "intent": far, "note": "x", "base": 1 })).is_err());
    other.append("", "intent.updated", json!({ "intent": far, "add_note": "its manager's" })).unwrap();
    assert_eq!(records(&d), ["w1:chat:s-boss:-", "w2:docs:s-other:-"]);
}

/// Two open workspaces labelled alike are two workspaces: an agent in the second is never
/// matched to the first one's record. Not while its manager runs, and not once it left (the
/// workspace still open): the second one's agent would become the manager of every intent
/// that runs or ran in the first.
#[test]
fn a_second_open_workspace_of_one_name_is_not_the_first() {
    let tmp = tempfile::tempdir().unwrap();
    let (fake, d, _boss, other, g1, g2) = two_open(tmp.path());
    fake.agents.lock().unwrap().push(("p3".into(), docs("s-work", "worker", "w1")));
    let id = planned(&d, "In w1", None);
    d.append("user", "intent.state_changed", start(&id, &g1)).unwrap();
    let refused = |when: &str| {
        let (status, said) = answered(&d, "p2", "manager.registered", json!({ "workspace": g2 }));
        assert!(status == 409 && said.contains(&format!("workspace docs {SAME_NAME}")), "{when}: {status} {said}");
    };
    refused("while boss runs");
    // boss leaves; its workspace is still open (a worker runs there).
    fake.agents.lock().unwrap().retain(|(p, _)| p != "p1");
    refused("once boss left");
    let e = other.append("", "manager.registered", json!({ "intent": id })).unwrap_err().to_string();
    assert!(e.contains("the workspace it runs in"), "naming the intent: {e}");
    assert!(other.append("", "intent.updated", json!({ "intent": id, "note": "x", "base": 1 })).is_err());
    assert!(other.append("", "intent.state_changed", json!({ "intent": id, "state": "planned" })).is_err());
    let (status, said) = answered(&d, "p2", "intent.state_changed", start(&planned(&d, "Next", None), &g2));
    assert!(status == 409 && said.contains(SAME_NAME), "its start: {status} {said}");
    assert_eq!(records(&d), ["w1:docs:s-boss:-"]);
    assert_eq!(intent(&d, &id), ("active".to_string(), "The user's own words".to_string()));
    // The first one's own agents go on: a worker there takes it over from the manager gone.
    at_place(&d, "p3").append("", "manager.registered", json!({ "workspace": g1 })).unwrap();
    assert_eq!(records(&d), ["w1:docs:s-work:-"]);
}

/// The check is of the group, not of who asks: the user's own ways of naming a runtime
/// group (the page's start, `intent start` from a terminal there, the workspace's project
/// from Settings or the CLI, an intent made in or linked to it) are refused for the second
/// of two open workspaces labelled alike. Any of them would rename the first one's record to
/// the second's id, and hand it the first one's manager and intents.
#[test]
fn the_user_names_no_second_open_workspace_of_one_name_either() {
    let tmp = tempfile::tempdir().unwrap();
    let (_fake, d, boss, other, g1, g2) = two_open(tmp.path());
    let mine = repo(tmp.path(), "mine");
    d.append("user", "project.added", json!({ "path": mine, "title": "Mine" })).unwrap();
    let id = planned(&d, "In w2", None);
    let ways = [
        ("the page's start", d.call("/api/intent/start", Some(json!({ "intent": id, "workspace": "w2" })))),
        ("intent start", d.append("user", "intent.state_changed", start(&id, &g2))),
        ("workspace project", d.append("user", "workspace.project_set", json!({ "workspace": { "id": g2["id"], "label": g2["label"] }, "project": mine }))),
        ("an intent made there", d.append("user", "intent.created", json!({ "title": "There", "runtime_ref": g2 }))),
        ("an intent linked there", d.append("user", "intent.linked", json!({ "intent": id, "runtime_ref": g2 }))),
    ];
    for (way, r) in ways {
        let e = r.expect_err(way).to_string();
        assert!(e.contains(&format!("workspace docs {SAME_NAME}")), "{way}: {e}");
    }
    assert_eq!(records(&d), ["w1:docs:s-boss:-"]);
    assert_eq!(intent(&d, &id).0, "planned");
    // Nothing of the first is touched: its manager registers again, the second's cannot.
    boss.append("", "manager.registered", json!({ "workspace": g1 })).unwrap();
    assert!(other.append("", "manager.registered", json!({ "workspace": g2 })).is_err());
    // The first one is named as ever.
    d.append("user", "workspace.project_set", json!({ "workspace": g1, "project": mine })).unwrap();
    d.call("/api/intent/start", Some(json!({ "intent": id, "workspace": "w1" }))).unwrap();
    assert_eq!(records(&d), [format!("w1:docs:s-boss:{mine}")]);
}

/// What the label is for: the runtime restarted and the same workspace has a new id. Its
/// own id is no longer listed, so the group with its label is that workspace: its manager
/// registers again under the new id and keeps its intent, and the user starts another there.
#[test]
fn a_restarted_workspace_keeps_its_record() {
    let tmp = tempfile::tempdir().unwrap();
    let (fake, d, _boss, _other, g1, _g2) = two_open(tmp.path());
    let id = planned(&d, "In w1", None);
    d.append("user", "intent.state_changed", start(&id, &g1)).unwrap();
    *fake.agents.lock().unwrap() = vec![("p9".into(), docs("s-boss2", "boss", "w9"))];
    let boss = at_place(&d, "p9");
    let made = boss.append("", "manager.registered", json!({ "workspace": group(&boss) })).unwrap();
    assert_eq!(made["body"]["workspace"], json!({ "id": "w9", "label": "docs" }));
    boss.append("", "intent.updated", json!({ "intent": id, "note": "after the restart", "base": 1 })).unwrap();
    assert_eq!(records(&d), ["w9:docs:s-boss2:-"]);
    let next = planned(&d, "After", None);
    d.call("/api/intent/start", Some(json!({ "intent": next, "workspace": "w9" }))).unwrap();
    assert_eq!(intent(&d, &next).0, "active");
}

/// Where this stops, as `docs/design/caller-identity.md` writes it: once a workspace is
/// closed, the next group with its label is its record, whoever opened it. An agent can
/// close a workspace, or rename its own, through the runtime's socket; no request to
/// workriver does it, and workriver cannot tell it from a restart.
#[test]
fn a_closed_workspaces_name_is_the_next_groups_record() {
    let tmp = tempfile::tempdir().unwrap();
    let (fake, d, _boss, _other, g1, _g2) = two_open(tmp.path());
    let id = planned(&d, "In w1", None);
    d.append("user", "intent.state_changed", start(&id, &g1)).unwrap();
    *fake.agents.lock().unwrap() = vec![("p5".into(), docs("s-x", "x", "w5"))];
    let x = at_place(&d, "p5");
    x.append("", "manager.registered", json!({ "workspace": group(&x) })).unwrap();
    x.append("", "intent.updated", json!({ "intent": id, "note": "x", "base": 1 })).unwrap();
    assert_eq!(records(&d), ["w5:docs:s-x:-"]);
}

/// Whether a record's own workspace is still open is the runtime's to say: one that cannot
/// be read refuses the request, and never matches by label for want of an answer.
#[test]
fn an_unreadable_runtime_matches_no_workspace_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    let (fake, d, _boss, _other, _g1, g2) = two_open(tmp.path());
    fake.down.store(true, std::sync::atomic::Ordering::Relaxed);
    let (status, said) = answered(&d, "p2", "manager.registered", json!({ "workspace": g2 }));
    assert_eq!(status, 503, "{said}");
    assert!(d.append("user", "workspace.project_set", json!({ "workspace": g2, "project": null })).is_err());
    assert!(d.append("user", "intent.created", json!({ "title": "There", "runtime_ref": g2 })).is_err());
    fake.down.store(false, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(records(&d), ["w1:docs:s-boss:-"]);
}

/// An intent runs in its workspace's record, not under the name it was started with. boss
/// manages w1 "docs" and the user starts an intent there. The runtime restarts (w1 is w9)
/// and the user renames the workspace "notes"; boss registers after each, and stays its
/// intent's manager. A new workspace opened under the old name "docs" is one of its own:
/// its agent registers there and is nobody to that intent.
#[test]
fn an_intent_follows_its_workspaces_record() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), docs("s-boss", "boss", "w1"))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    let boss = at_place(&d, "p1");
    boss.append("", "manager.registered", json!({ "workspace": group(&boss) })).unwrap();
    let id = planned(&d, "Mine", None);
    d.append("user", "intent.state_changed", start(&id, &group(&boss))).unwrap();
    for (n, label) in [(1, "docs"), (2, "notes")] {
        fake.agents.lock().unwrap()[0].1 = labelled("s-boss", "boss", "w9", label);
        boss.append("", "manager.registered", json!({ "workspace": group(&boss) })).unwrap();
        boss.append("", "intent.updated", json!({ "intent": id, "add_note": format!("its manager's {n}") })).expect("still its manager");
    }
    fake.agents.lock().unwrap().push(("p2".into(), docs("s-new", "newcomer", "w5")));
    let newcomer = at_place(&d, "p2");
    newcomer.append("", "manager.registered", json!({ "workspace": group(&newcomer) })).expect("in its own workspace");
    assert_eq!(records(&d), ["w9:notes:s-boss:-", "w5:docs:s-new:-"]);
    let refusal = format!("only the user or the manager of intent {id}");
    for (what, body) in [("intent.updated", json!({ "intent": id, "note": "replaced", "base": 3 })), ("intent.state_changed", json!({ "intent": id, "state": "planned" })), ("intent.state_changed", json!({ "intent": id, "state": "done" }))] {
        let e = newcomer.append("", what, body).unwrap_err().to_string();
        assert!(e.contains(&refusal), "{what}: {e}");
    }
    assert_eq!(intent(&d, &id), ("active".to_string(), "The user's own words\n\nits manager's 1\n\nits manager's 2".to_string()));
    boss.append("", "intent.state_changed", json!({ "intent": id, "state": "done" })).expect("its manager closes it");
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

/// Whether a record's own workspace is still open is asked of its machine's runtime, now.
/// While another machine is followed, this machine's runtime not answering is not "no
/// workspace is open here": the second of two open workspaces of one name is refused (503),
/// from an agent and from the user, and the first one's record stays its own.
#[test]
fn a_group_list_that_cannot_be_read_is_not_an_empty_one() {
    let tmp = tempfile::tempdir().unwrap();
    let local = std::sync::Arc::new(Fake::default());
    *local.agents.lock().unwrap() = vec![("p1".into(), docs("s-work", "worker", "w1")), ("p2".into(), docs("s-other", "other", "w2"))];
    let (_all, user, _there) = following(tmp.path(), local.clone(), std::sync::Arc::new(Fake::default()));
    let (g1, g2) = (group(&at_place(&user, "p1")), group(&at_place(&user, "p2")));
    let id = planned(&user, "In w1", None);
    user.append("user", "intent.state_changed", start(&id, &g1)).unwrap();
    assert_eq!(records(&user), ["w1:docs:-:-"]);
    local.groups_down.store(true, std::sync::atomic::Ordering::Relaxed);
    let (status, said) = answered(&user, "p2", "manager.registered", json!({ "workspace": g2 }));
    assert!(status == 503 && said.contains("cannot say which groups are open"), "an agent: {status} {said}");
    let next = planned(&user, "Next", None);
    let ways = [
        ("the page's start", user.call("/api/intent/start", Some(json!({ "intent": next, "workspace": "w2" })))),
        ("intent start", user.append("user", "intent.state_changed", start(&next, &g2))),
        ("workspace project", user.append("user", "workspace.project_set", json!({ "workspace": g2, "project": null }))),
    ];
    for (way, r) in ways {
        let e = r.expect_err(way).to_string();
        assert!(e.contains("cannot say which groups are open"), "{way}: {e}");
    }
    assert_eq!(records(&user), ["w1:docs:-:-"]);
    // What needs no answer is not held up: the recorded workspace itself is named as ever.
    user.append("user", "workspace.project_set", json!({ "workspace": g1, "project": null })).unwrap();
    local.groups_down.store(false, std::sync::atomic::Ordering::Relaxed);
    let e = user.append("user", "intent.state_changed", start(&next, &g2)).unwrap_err().to_string();
    assert!(e.contains(SAME_NAME), "{e}");
}

/// The same on a followed machine: its workspaces are asked of its runtime at the request,
/// never taken from the last regular read. A workspace that restarted there since (its old
/// id is gone) keeps its record, though the last read still lists the old id; and while
/// that machine's runtime cannot say which workspaces are open, a second one of the name is
/// refused (503), not matched.
#[test]
fn a_followed_machines_workspaces_are_read_at_the_request() {
    let tmp = tempfile::tempdir().unwrap();
    let far = std::sync::Arc::new(Fake::default());
    *far.agents.lock().unwrap() = vec![("p1".into(), docs("s-far", "far", "w1"))];
    let (_all, user, there) = following(tmp.path(), std::sync::Arc::new(Fake::default()), far.clone());
    let g1 = group(&at_place(&there, "p1"));
    assert_eq!(g1, json!({ "id": "w1@box", "label": "docs", "machine": "box" }));
    let id = planned(&user, "Over there", None);
    user.append("user", "intent.state_changed", start(&id, &g1)).unwrap();
    // The runtime over there restarted: the same workspace under a new id. Not read again here.
    *far.agents.lock().unwrap() = vec![("p9".into(), docs("s-far2", "far", "w9"))];
    let g9 = group(&at_place(&there, "p9"));
    user.append("user", "workspace.project_set", json!({ "workspace": g9, "project": null })).expect("its own record, under its new id");
    assert_eq!(records(&user), ["w9@box:docs:-:-"]);
    // Another workspace of that name opens there, and its runtime stops saying which are open.
    far.agents.lock().unwrap().push(("p5".into(), docs("s-five", "five", "w5")));
    let g5 = group(&at_place(&there, "p5"));
    far.groups_down.store(true, std::sync::atomic::Ordering::Relaxed);
    let (status, said) = http_at(&there, there.port, "p5", "POST", "/api/append", &[], Some(&json!({ "kind": "manager.registered", "body": { "workspace": g5 } }).to_string()));
    assert!(status == 503 && said.contains("cannot say which groups are open"), "an agent there: {status} {said}");
    let e = user.append("user", "workspace.project_set", json!({ "workspace": g5, "project": null })).unwrap_err().to_string();
    assert!(e.contains("cannot say which groups are open"), "{e}");
    far.groups_down.store(false, std::sync::atomic::Ordering::Relaxed);
    let e = user.append("user", "workspace.project_set", json!({ "workspace": g5, "project": null })).unwrap_err().to_string();
    assert!(e.contains("is already recorded for another open workspace (w9@box)"), "{e}");
    assert_eq!(records(&user), ["w9@box:docs:-:-"]);
}

/// An agent's workspace is the one its place was found in when the request came, on a
/// followed machine too, never the one the last regular read of that machine had. The user
/// renamed boss's workspace over there since that read: boss's start is recorded under the
/// new name. And an agent that started there since registers in its workspace, though the
/// last read does not know it.
#[test]
fn an_agents_workspace_is_read_with_its_place() {
    let tmp = tempfile::tempdir().unwrap();
    let mine = repo(tmp.path(), "mine");
    let far = std::sync::Arc::new(Fake::default());
    *far.agents.lock().unwrap() = vec![("p1".into(), docs("s-boss", "boss", "w1"))];
    let (_all, user, there) = following(tmp.path(), std::sync::Arc::new(Fake::default()), far.clone());
    let boss = at_place(&there, "p1");
    user.append("user", "project.added", json!({ "path": mine, "title": "Mine" })).unwrap();
    boss.append("", "manager.registered", json!({ "workspace": group(&boss) })).unwrap();
    user.append("user", "workspace.project_set", json!({ "workspace": group(&boss), "project": mine })).unwrap();
    let id = planned(&user, "Own", Some(&mine));
    far.agents.lock().unwrap()[0].1 = labelled("s-boss", "boss", "w1", "notes");
    let named = group(&boss);
    assert_eq!(named["label"], "notes");
    let started = boss.append("", "intent.state_changed", start(&id, &named)).unwrap();
    assert_eq!(started["body"]["workspace"], named, "as the runtime names it now");
    boss.append("", "manager.registered", json!({ "workspace": named })).unwrap();
    assert_eq!(records(&user), [format!("w1@box:notes:s-boss:{mine}")]);
    // Started since the last read of that machine.
    far.agents.lock().unwrap().push(("p2".into(), labelled("s-late", "late", "w2", "other")));
    let late = at_place(&there, "p2");
    late.append("", "manager.registered", json!({ "workspace": group(&late) })).expect("in its own workspace");
    assert_eq!(records(&user), [format!("w1@box:notes:s-boss:{mine}"), "w2@box:other:s-late:-".to_string()]);
    // In another's, or in none: refused, as on this machine.
    let (status, said) = http_at(&there, there.port, "p2", "POST", "/api/append", &[], Some(&json!({ "kind": "manager.registered", "body": { "workspace": named } }).to_string()));
    assert!(status == 403 && said.contains("the workspace it runs in"), "{status} {said}");
}

/// Runs `then` once, from inside the service, as it next lists the agents for a request:
/// after a registration was let in, before it is recorded.
fn as_the_agents_are_listed(fake: &Fake, then: impl Fn() + Send + Sync + 'static) {
    let done = std::sync::atomic::AtomicBool::new(false);
    *fake.on_agents.lock().unwrap() = Some(std::sync::Arc::new(move || {
        if !done.swap(true, std::sync::atomic::Ordering::SeqCst) {
            then();
        }
    }));
}

/// What a group is checked against is what it is recorded on: the check and the record are
/// one step. Two workspaces of one name are open and neither is recorded yet. While other's
/// registration is under way, boss's is recorded: other's is checked against that, and
/// refused, where it once passed against no record and then took boss's record by its name.
#[test]
fn the_check_and_the_record_are_one_step() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), docs("s-boss", "boss", "w1")), ("p2".into(), docs("s-other", "other", "w2"))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    let (boss, other) = (at_place(&d, "p1"), at_place(&d, "p2"));
    let (g1, g2) = (group(&boss), group(&other));
    as_the_agents_are_listed(&fake, move || {
        boss.append("", "manager.registered", json!({ "workspace": g1 })).unwrap();
    });
    let (status, said) = answered(&d, "p2", "manager.registered", json!({ "workspace": g2 }));
    assert!(status == 409 && said.contains(&format!("workspace docs {SAME_NAME}")), "{status} {said}");
    assert_eq!(records(&d), ["w1:docs:s-boss:-"]);
}

/// The same for a registration that names only an intent: the workspace it sets the manager
/// of is the one the intent is in as it is recorded. The user moves the intent to another
/// workspace while an agent of the first registers for it: refused, where it once passed
/// for the first workspace and was then recorded as the manager of the second.
#[test]
fn a_registration_for_an_intent_is_for_where_it_is_recorded() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), labelled("s-one", "one", "w1", "one")), ("p2".into(), labelled("s-two", "two", "w2", "two"))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    let (g1, g2) = (group(&at_place(&d, "p1")), group(&at_place(&d, "p2")));
    let id = planned(&d, "Moves", None);
    d.append("user", "intent.state_changed", start(&id, &g1)).unwrap();
    let (user, moved) = (d.clone(), id.clone());
    as_the_agents_are_listed(&fake, move || {
        user.append("user", "intent.linked", json!({ "intent": moved, "runtime_ref": g2 })).unwrap();
    });
    let (status, said) = answered(&d, "p1", "manager.registered", json!({ "intent": id }));
    assert!(status == 403 && said.contains("the workspace it runs in"), "{status} {said}");
    assert_eq!(records(&d), ["w1:one:-:-", "w2:two:-:-"]);
    // Where the intent still is, it registers as before.
    let back = planned(&d, "Stays", None);
    d.append("user", "intent.state_changed", start(&back, &g1)).unwrap();
    at_place(&d, "p1").append("", "manager.registered", json!({ "intent": back })).unwrap();
    assert_eq!(records(&d), ["w1:one:s-one:-", "w2:two:-:-"]);
}
