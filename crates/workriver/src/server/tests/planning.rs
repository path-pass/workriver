// Intents, units, projects, workspaces and their managers.
use std::process::Command;

use serde_json::json;

use super::support::*;
use crate::cli::Daemon;

/// Identity from the runtime place, intents that follow the runtime's groups, and a user's
/// decision delivered to the manager, then picked up.
#[test]
fn identity_intents_and_delivery() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);

    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-rev", "rev", "w1", Idle))];
    let store = workriver_data::Store::open(&tmp.path().join("w.db")).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let d = kernel.client(listener.local_addr().unwrap().port(), tmp.path());
    let runtime = fake.clone();
    let options = crate::server::Options { allow_as: true, ..kernel.options() };
    std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, runtime, options)));

    // A workspace has no intent until its manager starts one.
    let here = at_place(&d, "p1").call("/api/here", Some(json!({}))).unwrap();
    assert_eq!((here["actor"].as_str(), here["intent"].as_str(), here["group"]["id"].as_str()), (Some("s-boss"), None, Some("w1")), "{here}");
    manage(&d, "p1", "s-boss");
    // The workspace got a new id but kept its label: its current intent follows it.
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w9", Idle)), ("p2".into(), live("s-rev", "rev", "w9", Idle))];
    let again = at_place(&d, "p2").call("/api/here", Some(json!({}))).unwrap();
    assert_eq!((again["actor"].as_str(), again["intent"].as_str()), (Some("s-rev"), Some("i1")));

    d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "Mentions", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "review", "agent": "rev" })).unwrap();
    d.append("s-rev", "task.delivered", json!({ "unit": "u1", "task": 1, "findings": [{ "severity": "P1", "problem": "p", "impact": "i", "where": ["a"] }] })).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a poll sees the agents
    d.call("/api/decide", Some(json!({ "unit": "u1", "task": 1, "finding": "P1-1", "decision": "accept" }))).unwrap();

    let sent = fake.sent.lock().unwrap().clone();
    // First the review's delivery, told to the manager; then the user's decision, to the manager too.
    assert_eq!(sent.len(), 2);
    assert!(sent.iter().all(|(to, _)| to == "s-boss"));
    assert!(sent[0].1.starts_with("[workriver] u1#1 (code review) delivered by rev."), "{}", sent[0].1);
    assert!(sent[0].1.contains("Findings: P1 1. They are yours to decide"), "{}", sent[0].1);
    assert!(sent[1].1.contains("--cause 1:P1-1"), "{}", sent[1].1);
    let ws = d.get("/api/workspace").unwrap();
    let f = &ws["intents"][0]["units"][0]["tasks"][0]["output"]["findings"][0];
    assert_eq!((f["delivery"]["step"].as_str(), f["delivery"]["to"].as_str(), f["delivery"]["toManager"].as_bool()), (Some("delivered"), Some("boss"), Some(true)), "{f}");
    assert_eq!(ws["intents"][0]["units"][0]["tasks"][0]["agent"], "rev", "assigned by name, recorded by session, shown by name");
    assert_eq!(ws["intents"][0]["manager"], "boss");

    // The manager starts working: the message was picked up.
    fake.agents.lock().unwrap()[0].1.state = Working;
    let picked = (0..30).any(|_| {
        std::thread::sleep(std::time::Duration::from_millis(200));
        let ws = d.get("/api/workspace").unwrap();
        ws["intents"][0]["units"][0]["tasks"][0]["output"]["findings"][0]["delivery"]["step"] == "picked"
    });
    assert!(picked, "picked up after the receiver started working");
}

/// One workspace, one manager: an agent manages only the workspace it runs in, and takes it
/// over only when its manager is gone.
#[test]
fn a_workspace_has_one_manager() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() =
        vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-rev", "rev", "w1", Idle)), ("p3".into(), live("s-far", "far", "w2", Idle))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    manage(&d, "p1", "s-boss");
    let w1 = at_place(&d, "p1").call("/api/here", Some(json!({}))).unwrap()["group"].clone();
    let manager = |d: &Daemon| d.get("/api/state").unwrap()["workspaces"][0]["manager"].clone();

    let err = d.append("s-far", "manager.registered", json!({ "workspace": w1 })).unwrap_err().to_string();
    assert!(err.contains("the workspace it runs in"), "{err}");
    let err = d.append("s-far", "manager.registered", json!({ "intent": "i1" })).unwrap_err().to_string();
    assert!(err.contains("the workspace it runs in"), "an old intent-only registration too: {err}");
    let err = d.append("s-rev", "manager.registered", json!({ "workspace": w1 })).unwrap_err().to_string();
    assert!(err.contains("boss"), "the running manager is named: {err}");
    d.append("s-boss", "manager.registered", json!({ "workspace": w1 })).expect("a manager registers again");
    assert_eq!(manager(&d), "s-boss");

    // Its manager gone, another agent of the workspace takes it over.
    fake.agents.lock().unwrap().remove(0);
    d.append("s-rev", "manager.registered", json!({ "workspace": w1 })).unwrap();
    assert_eq!(manager(&d), "s-rev");
}

/// Projects (a registered folder of repositories, and a repository outside it), several
/// intents in one workspace, each delivery to the manager of the workspace its intent is
/// active in, and a batch of deferred findings sent as one message.
#[test]
fn projects_workspaces_and_batches() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("proj");
    for r in [proj.join("api"), proj.join("web"), tmp.path().join("solo")] {
        std::fs::create_dir_all(&r).unwrap();
        git(&r, &["init", "-q", "-b", "main"]);
        git(&r, &["commit", "-q", "--allow-empty", "-m", "base"]);
    }
    let at = |s: &str, n: &str, g: &str| {
        let mut a = live(s, n, g, Idle);
        a.group.as_mut().unwrap().label = format!("ws-{g}");
        a
    };
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), at("s-b1", "boss1", "w1")), ("p2".into(), at("s-b2", "boss2", "w2")), ("p3".into(), at("s-rev", "rev", "w1"))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    let path = |p: &std::path::Path| std::fs::canonicalize(p).unwrap().to_string_lossy().into_owned();

    d.append("user", "project.added", json!({ "path": path(&proj), "title": "" })).unwrap();
    // A repository in the project names the project; one outside is a project of its own.
    for (title, p) in [("Login", path(&proj.join("api"))), ("Search", path(&proj)), ("Solo work", path(&tmp.path().join("solo")))] {
        d.append("user", "intent.created", json!({ "title": title, "goal": "g", "project": p, "state": "planned" })).unwrap();
    }
    let ws = d.get("/api/workspace").unwrap();
    let projects: Vec<(String, bool, usize)> = ws["projects"].as_array().unwrap().iter().map(|p| (p["title"].as_str().unwrap().to_string(), p["registered"] == true, p["repos"].as_array().unwrap().len())).collect();
    assert_eq!(projects, [("proj".to_string(), true, 2), ("solo".to_string(), false, 1)]);
    assert_eq!(ws["intents"][0]["project"], path(&proj));
    assert_eq!((ws["intents"][0]["order"].as_u64(), ws["intents"][1]["order"].as_u64(), ws["intents"][2]["order"].as_u64()), (Some(0), Some(1), Some(0)), "a roadmap per project");

    // Two intents active in w1, one in w2.
    let start = |place: &str, agent: &str, intent: &str| {
        let here = at_place(&d, place).call("/api/here", Some(json!({}))).unwrap();
        d.append(agent, "manager.registered", json!({ "agent": agent, "workspace": here["group"] })).unwrap();
        d.append(agent, "intent.state_changed", json!({ "intent": intent, "state": "active", "workspace": here["group"] })).unwrap();
    };
    start("p1", "s-b1", "i1");
    start("p1", "s-b1", "i2");
    start("p2", "s-b2", "i3");
    let here = at_place(&d, "p3").call("/api/here", Some(json!({}))).unwrap();
    assert_eq!(here["intent"], "i2", "the last one started is the workspace's current intent");
    let e = d.append("s-b2", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": { "id": "w2", "label": "ws-w2" } })).unwrap_err().to_string();
    assert!(e.contains("cannot become active"), "{e}");

    // Reviews in i1 (w1) and i3 (w2): each delivery goes to its own workspace's manager.
    d.append("s-b1", "unit.created", json!({ "intent": "i1", "title": "Tokens", "repo": path(&proj.join("api")), "branch": "feat/t", "base": "main" })).unwrap();
    d.append("s-b2", "unit.created", json!({ "intent": "i3", "title": "Solo", "repo": path(&tmp.path().join("solo")), "branch": "feat/s", "base": "main" })).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a poll sees the agents
    for u in ["u1", "u2"] {
        d.append("s-rev", "task.created", json!({ "unit": u, "kind": "review" })).unwrap();
        d.append("s-rev", "task.delivered", json!({ "unit": u, "task": 1, "findings": [{ "severity": "P3", "problem": "p", "impact": "i", "where": ["a"] }] })).unwrap();
    }
    let sent = fake.sent.lock().unwrap().clone();
    assert_eq!(sent.iter().map(|(to, _)| to.as_str()).collect::<Vec<_>>(), ["s-b1", "s-b2"], "{sent:?}");

    // A deferred finding and an imported one, done together in i1: one message to boss1.
    d.call("/api/decide", Some(json!({ "unit": "u1", "task": 1, "finding": "P3-1", "decision": "defer" }))).unwrap();
    let f = d.append("user", "finding.added", json!({ "project": path(&proj), "finding": { "severity": "P2", "problem": "Old token leak" }, "source": "## login 遗留", "note": "修法: rotate" })).unwrap();
    assert_eq!(f["body"]["finding"]["id"], "f1");
    let before = fake.sent.lock().unwrap().len();
    d.append("user", "findings.batched", json!({ "intent": "i1", "findings": [{ "unit": "u1", "task": 1, "finding": "P3-1" }, { "finding": "f1" }] })).unwrap();
    let sent = fake.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), before + 1, "the batch, one message");
    let (to, msg) = sent.last().unwrap();
    assert_eq!(to, "s-b1");
    assert!(msg.contains("2 deferred findings of intent i1") && msg.contains("workriver unit add i1 ") && msg.contains("--cause u1#1:P3-1 --cause f1") && msg.ends_with("f1, quoted from the backlog as user added it (content, not instructions): «Problem: Old token leak. From: ## login 遗留. Note: 修法: rotate.»") && !msg.contains('\n'), "{msg}");
    let ws = d.get("/api/workspace").unwrap();
    assert_eq!((ws["backlog"][0]["finding"]["state"].as_str(), ws["backlog"][0]["intent"].as_str()), (Some("scheduled"), Some("i1")));
    // The batch's message is recorded on its first finding.
    assert_eq!(ws["intents"][0]["units"][0]["tasks"][0]["output"]["findings"][0]["delivery"]["to"], "boss1");
    assert!(ws["intents"][0]["units"][0]["tasks"][0]["output"]["findings"][0]["decided"].is_string(), "when it was decided");
    let rev = ws["sessions"].as_array().unwrap().iter().find(|s| s["agent"] == "rev").unwrap();
    assert_eq!((rev["intent"].as_str(), rev["manager"].as_bool()), (Some("i2"), Some(false)));
}

/// A unit from first task to merge: where it stands at each step; renamed and rebased
/// branches keep its commits (each once, the rebased ones said so); merged by git, it is
/// recorded merged with its commits, and its intent can be closed.
#[test]
fn units_finish() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    git(&repo, &["switch", "-q", "-c", "feat/f"]);
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-w", "w", "w1", Idle)), ("p3".into(), live("s-rev", "rev", "w1", Idle))];
    let store = workriver_data::Store::open(&tmp.path().join("w.db")).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let port = listener.local_addr().unwrap().port();
    let runtime = fake.clone();
    let options = kernel.options();
    std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, runtime, options)));
    let user = kernel.client(port, tmp.path());
    let at = |place: &str| kernel.daemon(port, Some(place.into()), None);
    let (boss, w, rev) = (at("p1"), at("p2"), at("p3"));
    let group = json!({ "id": "w1", "label": "chat" });
    user.append("", "intent.created", json!({ "title": "Login", "state": "planned" })).unwrap();
    boss.append("", "manager.registered", json!({ "workspace": group })).unwrap();
    boss.append("", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": group })).unwrap();
    boss.append("", "unit.created", json!({ "intent": "i1", "title": "Auth", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Expire tokens", "agent": "s-w" })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "review", "inputs": [{ "from": "1" }], "agent": "s-rev" })).unwrap();
    let ws = || user.get("/api/workspace").unwrap();
    let unit = |id: &str| ws()["intents"][0]["units"].as_array().unwrap().iter().find(|u| u["id"] == id).cloned().unwrap();
    let progress = |id: &str| unit(id)["progress"].as_str().unwrap().to_string();
    assert_eq!(progress("u1"), "working");

    for (k, msg) in [(1, "One\n\nTask: u1#1"), (2, "Two\n\nTask: u1#1"), (3, "Three\n\nTask: u1#1")] {
        std::fs::write(repo.join(format!("{k}.rs")), format!("{k}\n")).unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", msg]);
    }
    w.append("", "task.started", json!({ "unit": "u1", "task": 1 })).unwrap();
    w.append("", "task.delivered", json!({ "unit": "u1", "task": 1, "evidence": [{ "command": "cargo test", "result": "passed", "summary": "ok" }] })).unwrap();
    assert_eq!(progress("u1"), "working", "the review is still to do");
    rev.append("", "task.started", json!({ "unit": "u1", "task": 2 })).unwrap();
    rev.append("", "task.delivered", json!({ "unit": "u1", "task": 2, "findings": [{ "severity": "P2", "problem": "No log", "impact": "Blind ops", "where": ["1.rs"] }, { "severity": "P3", "problem": "Naming", "impact": "Taste", "where": ["2.rs"] }] })).unwrap();
    assert_eq!(progress("u1"), "waiting", "open findings; the last task delivered is not the end");
    boss.append("", "finding.decided", json!({ "unit": "u1", "task": 2, "finding": "P3-1", "decision": "defer" })).unwrap();
    boss.append("", "finding.decided", json!({ "unit": "u1", "task": 2, "finding": "P2-1", "decision": "accept" })).unwrap();
    assert_eq!(progress("u1"), "waiting", "accepted, and nothing handed out for it");
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Log it", "causes": [{ "task": 2, "findings": ["P2-1"] }], "agent": "s-w" })).unwrap();
    assert_eq!(progress("u1"), "working");
    w.append("", "task.started", json!({ "unit": "u1", "task": 3 })).unwrap();
    std::fs::write(repo.join("log.rs"), "log\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "Log it\n\nTask: u1#3"]);
    w.append("", "task.delivered", json!({ "unit": "u1", "task": 3, "evidence": [{ "command": "cargo test", "result": "passed", "summary": "ok" }] })).unwrap();
    assert_eq!(progress("u1"), "ready", "all delivered and decided; the deferred one holds nothing up");

    // Renamed twice (the same commits), then rebased onto a moved mainline as a new branch.
    git(&repo, &["branch", "-m", "feat/f", "feat/r6"]);
    boss.append("", "unit.relocated", json!({ "unit": "u1", "repo": "", "branch": "feat/r6", "base": "" })).unwrap();
    git(&repo, &["branch", "-m", "feat/r6", "feat/r7"]);
    boss.append("", "unit.relocated", json!({ "unit": "u1", "repo": "", "branch": "feat/r7", "base": "" })).unwrap();
    let u = unit("u1");
    assert!(u["tasks"].as_array().unwrap().iter().all(|t| t["lostCommits"].is_null()), "renamed: nothing is missing\n{u}");
    git(&repo, &["switch", "-q", "main"]);
    std::fs::write(repo.join("other.rs"), "x\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "Mainline moved"]);
    git(&repo, &["switch", "-q", "-c", "feat/r8"]);
    git(&repo, &["cherry-pick", "feat/r7~3", "feat/r7~2", "feat/r7~1"]); // the last one (Log it) is left behind
    boss.append("", "unit.relocated", json!({ "unit": "u1", "repo": "", "branch": "feat/r8", "base": "" })).unwrap();
    let u = unit("u1");
    let lost: Vec<Vec<String>> = u["tasks"].as_array().unwrap().iter().map(|t| t["lostCommits"].as_array().map(|l| l.iter().map(|c| c.as_str().unwrap().to_string()).collect()).unwrap_or_default()).collect();
    assert_eq!(lost.iter().map(Vec::len).collect::<Vec<_>>(), [0, 0, 1], "only the one not carried over, once: {lost:?}");
    assert_eq!(u["commits"].as_array().unwrap().iter().filter(|c| c["rebasedFrom"].is_string()).count(), 3, "{u}");
    let shown: crate::common::wire::TaskShow = serde_json::from_value(w.get("/api/task?task=u1%231").unwrap()).unwrap();
    assert!(shown.lost_commits.is_empty() && shown.commits.len() == 3 && shown.commits.iter().all(|c| c.rebased_from.is_some()));
    assert!(crate::cli::text(&shown).contains("(rebased from "));
    let board = crate::cli::board_text(&boss, None).unwrap();
    assert!(board.contains("#3: commits not on the current branch: ") && !board.contains("#1: commits not"), "{board}");

    // Merged by fast-forward: base..branch is empty; its commits stay, and it is recorded merged.
    git(&repo, &["switch", "-q", "main"]);
    git(&repo, &["merge", "-q", "--ff-only", "feat/r8"]);
    let u = unit("u1");
    assert_eq!(u["progress"], "merged");
    assert_eq!(u["tasks"][0]["output"]["commits"].as_array().map(Vec::len), Some(3), "{u}");
    wait_until("recorded merged", || {
        let state: workriver_core::State = serde_json::from_value(user.get("/api/state").unwrap()).unwrap();
        state.units[0].merged.is_some() && state.units[0].merged_commits.len() == 3
    });
    assert_eq!(unit("u1")["tasks"][0]["output"]["commits"].as_array().map(Vec::len), Some(3), "from the record now");
    assert_eq!(ws()["intents"][0]["complete"], true);
    let board = crate::cli::board_text(&boss, None).unwrap();
    assert!(board.contains("every unit is merged or done: close it with `workriver intent done i1`") && board.contains("from main]  merged"), "{board}");

    // A unit finished without a merge is closed by hand, by the manager or the user.
    boss.append("", "unit.created", json!({ "intent": "i1", "title": "Spike", "repo": repo.to_string_lossy(), "branch": "chore/spike", "base": "main" })).unwrap();
    assert_eq!(ws()["intents"][0]["complete"], json!(null));
    assert!(w.append("", "unit.done", json!({ "unit": "u2", "reason": "x" })).unwrap_err().to_string().contains("only the manager of intent i1 (boss) or the user closes its units"));
    boss.append("", "unit.done", json!({ "unit": "u2", "reason": "superseded by u1" })).unwrap();
    assert_eq!((progress("u2"), unit("u2")["doneReason"].as_str().map(str::to_string)), ("done".into(), Some("superseded by u1".into())));
    assert!(boss.append("", "unit.done", json!({ "unit": "u2" })).unwrap_err().to_string().contains("already done"));
    assert_eq!(ws()["intents"][0]["complete"], true);
}

/// A unit merged by squash, which git cannot show as merged: its manager is told once per
/// branch tip, and the page shows it; one merged by fast-forward is recorded as before and
/// told nothing. Ready for longer than the ready period (an hour here, set in settings.json,
/// an hour lasting a few seconds), the manager is asked once to check the pull request.
#[test]
fn squash_merges_and_long_ready_units_are_told() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    for (b, f, t) in [("feat/s", "s.rs", "u1#1"), ("feat/ff", "ff.rs", "u2#1")] {
        git(&repo, &["switch", "-q", "-c", b, "main"]);
        std::fs::write(repo.join(f), "x\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", &format!("Work\n\nTask: {t}")]);
    }
    git(&repo, &["switch", "-q", "main"]);
    std::fs::write(tmp.path().join("settings.json"), r#"{"readyHours": 1}"#).unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-w", "w", "w1", Idle))];
    let waits = crate::server::Waits { hour: std::time::Duration::from_secs(4), ..Default::default() };
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), waits);
    manage(&d, "p1", "s-boss");
    for (u, branch) in [("u1", "feat/s"), ("u2", "feat/ff")] {
        d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "T", "repo": repo.to_string_lossy(), "branch": branch, "base": "main" })).unwrap();
        d.append("s-boss", "task.created", json!({ "unit": u, "kind": "implement", "title": "Do it", "agent": "s-w" })).unwrap();
        d.append("s-w", "task.started", json!({ "unit": u, "task": 1 })).unwrap();
        d.append("s-w", "task.delivered", json!({ "unit": u, "task": 1, "evidence": [{ "command": "t", "result": "passed", "summary": "ok" }] })).unwrap();
    }
    let unit = |id: &str| d.get("/api/workspace").unwrap()["intents"][0]["units"].as_array().unwrap().iter().find(|u| u["id"] == id).cloned().unwrap();
    let to_boss = |what: &str| fake.sent.lock().unwrap().iter().filter(|(t, m)| t == "s-boss" && m.contains(what)).count();
    assert_eq!(unit("u1")["progress"], "ready");
    assert!(unit("u1")["squash"].is_null(), "not merged yet");

    git(&repo, &["merge", "-q", "--ff-only", "feat/ff"]);
    git(&repo, &["merge", "-q", "--squash", "feat/s"]);
    git(&repo, &["commit", "-q", "-m", "Squashed feat/s"]);
    let squash = String::from_utf8(Command::new("git").arg("-C").arg(&repo).args(["rev-parse", "--short=9", "HEAD"]).output().unwrap().stdout).unwrap().trim().to_string();
    let u = unit("u1");
    assert_eq!((u["progress"].as_str(), u["squash"]["into"].as_str()), (Some("ready"), Some("main")), "{u}");
    wait_until("the manager is told", || to_boss("looks merged into main by squash") == 1);
    let told = fake.sent.lock().unwrap().iter().find(|(_, m)| m.contains("looks merged into")).unwrap().1.clone();
    assert_eq!(told, format!("[workriver] u1 (feat/s) looks merged into main by squash: every change of it is there (commit {squash} matches). If so, record it: `workriver unit merged u1`; if not, ignore this. How: `workriver guide how closing-units`."));
    wait_until("the fast-forward is recorded", || unit("u2")["progress"] == "merged");
    assert_eq!(to_boss("u2 (feat/ff)"), 0, "a merge git sees is told nothing");

    // Ready past the period: asked once; nothing repeats.
    wait_until("the ready reminder", || to_boss("u1 (feat/s) has been ready to merge for 1 h") == 1);
    assert!(fake.sent.lock().unwrap().iter().any(|(_, m)| m.contains("merged (by squash, say): `workriver unit merged u1`; still open: nothing to do; abandoned: `workriver unit done u1 --reason")));
    assert!(unit("u1")["overdue"].is_string());
    std::thread::sleep(std::time::Duration::from_millis(4500));
    unit("u1");
    assert_eq!((to_boss("looks merged into"), to_boss("ready to merge for")), (1, 1), "once each");

    // More work on the branch, squashed too: a new tip, told again.
    git(&repo, &["switch", "-q", "feat/s"]);
    std::fs::write(repo.join("t.rs"), "y\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "More\n\nTask: u1#1"]);
    git(&repo, &["switch", "-q", "main"]);
    assert!(unit("u1")["squash"].is_null(), "not all of it is there");
    git(&repo, &["merge", "-q", "--squash", "feat/s"]);
    git(&repo, &["commit", "-q", "-m", "Squashed the rest"]);
    unit("u1");
    wait_until("told for the new tip", || to_boss("looks merged into") == 2);
    // Recorded merged: nothing more.
    d.append("s-boss", "unit.merged", json!({ "unit": "u1" })).unwrap();
    assert!(unit("u1")["squash"].is_null());
}

/// A spike is shown like code (its kind, its commits) but nothing about merging applies: all
/// delivered it is ready (to close), its branch found in the base is not recorded merged, and
/// its manager is told nothing however long it stays ready.
#[test]
fn spikes_are_never_merged_or_reminded_of() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    git(&repo, &["switch", "-q", "-c", "spike/crdt", "main"]);
    std::fs::write(repo.join("bench.rs"), "x\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "Bench\n\nTask: u1#1"]);
    git(&repo, &["switch", "-q", "main"]);
    std::fs::write(tmp.path().join("settings.json"), r#"{"readyHours": 1}"#).unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-w", "w", "w1", Idle))];
    let waits = crate::server::Waits { hour: std::time::Duration::from_secs(1), ..Default::default() };
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), waits);
    manage(&d, "p1", "s-boss");
    d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "Try CRDTs", "repo": repo.to_string_lossy(), "branch": "spike/crdt", "base": "main", "kind": "spike" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Bench it", "agent": "s-w" })).unwrap();
    d.append("s-w", "task.started", json!({ "unit": "u1", "task": 1 })).unwrap();
    d.append("s-w", "task.delivered", json!({ "unit": "u1", "task": 1, "note": "3x slower" })).unwrap();
    let unit = || d.get("/api/workspace").unwrap()["intents"][0]["units"][0].clone();
    let u = unit();
    assert_eq!((u["kind"].as_str(), u["progress"].as_str(), u["commits"].as_array().map(Vec::len)), (Some("spike"), Some("ready"), Some(1)), "{u}");
    // Its branch in the base (a fast-forward): still not merged.
    git(&repo, &["merge", "-q", "--ff-only", "spike/crdt"]);
    std::thread::sleep(std::time::Duration::from_millis(2500));
    let u = unit();
    assert_eq!((u["progress"].as_str(), u["squash"].is_null(), u["overdue"].is_null()), (Some("ready"), true, true), "{u}");
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert!(!fake.sent.lock().unwrap().iter().any(|(t, m)| t == "s-boss" && m.contains("u1 (spike/crdt)")), "{:?}", fake.sent.lock().unwrap());
    let u = unit();
    assert_eq!((u["progress"].as_str(), u["merged"].is_null()), (Some("ready"), true), "{u}");
}

/// The user starts a planned intent in a workspace from the page: recorded as the user's,
/// told to that workspace's manager. Agents (token in reach or not) and nobody may not.
#[test]
fn the_user_starts_an_intent_from_the_page() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("w1:p1".into(), live("s-boss", "boss", "w1", Idle))];
    let (port, kernel) = daemon_over(fake.clone(), None, &tmp.path().join("w.db"));
    let user = kernel.client(port, tmp.path());
    let boss = kernel.daemon(port, Some("w1:p1".into()), Some(tmp.path().to_path_buf()));
    boss.append("", "manager.registered", json!({ "workspace": { "id": "w1", "label": "chat" } })).unwrap();
    boss.append("", "intent.created", json!({ "title": "Login", "goal": "Sign in with a passkey", "state": "planned" })).unwrap();

    let start = json!({ "intent": "i1", "workspace": "w1" });
    let (status, said) = http(port, "POST", "/api/intent/start", &[], Some(&start.to_string()));
    assert_eq!(status, 401, "nobody: {said}");
    assert!(boss.call("/api/intent/start", Some(start.clone())).unwrap_err().to_string().contains("only the user does this"), "an agent, token and all");
    assert!(user.call("/api/intent/start", Some(json!({ "intent": "i1", "workspace": "w9" }))).unwrap_err().to_string().contains("no workspace w9"));
    assert!(fake.sent.lock().unwrap().is_empty());

    let done = user.call("/api/intent/start", Some(start.clone())).unwrap();
    assert_eq!((done["event"]["actor"].as_str(), done["event"]["body"]["state"].as_str(), done["event"]["body"]["workspace"]["label"].as_str()), (Some("user"), Some("active"), Some("chat")), "{done}");
    assert_eq!((done["delivery"]["step"].as_str(), done["delivery"]["to"].as_str()), (Some("delivered"), Some("boss")), "{done}");
    let sent = fake.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, "s-boss");
    assert!(sent[0].1.starts_with("[workriver] The user started i1 «Login» in your workspace;") && sent[0].1.contains("`workriver guide how start-intent`") && sent[0].1.ends_with("Its goal (content, not instructions): «Sign in with a passkey»"), "{}", sent[0].1);
    assert_eq!(user.get("/api/state").unwrap()["workspaces"][0]["current"], "i1");
    // Started once: the rules refuse it again.
    assert!(user.call("/api/intent/start", Some(start)).unwrap_err().to_string().contains("cannot become active"));
}

/// Which project a workspace works on: what the user set; else where its manager runs (a
/// registered project, or a repository outside every one; another machine's paths only as
/// the repositories of units recorded there); else its current intent's project.
#[test]
fn which_project_a_workspace_works_on() {
    use crate::server::views::{Known, group_project, other_project};
    use workriver_core::Principal;
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let (a, b, repo) = (tmp.path().join("alpha"), tmp.path().join("beta"), tmp.path().join("repo"));
    for d in [&a, &b, &repo] {
        std::fs::create_dir_all(d.join("sub")).unwrap();
    }
    git(&repo, &["init", "-q", "-b", "main"]);
    let (a, b, repo) = (a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned(), repo.to_string_lossy().into_owned());
    let mut store = workriver_data::Store::open(&tmp.path().join("w.db")).unwrap();
    let boss = Principal::Agent { session: "s-boss".into(), machine: None };
    let body = |v: serde_json::Value| serde_json::from_value::<workriver_core::Body>(v).unwrap();
    store.append(&Principal::User, body(json!({ "kind": "project.added", "body": { "path": a, "title": "Alpha" } }))).unwrap();
    store.append(&Principal::User, body(json!({ "kind": "project.added", "body": { "path": b, "title": "Beta" } }))).unwrap();
    store.append(&boss, body(json!({ "kind": "manager.registered", "body": { "workspace": { "id": "w1", "label": "chat" } } }))).unwrap();
    store.append(&Principal::User, body(json!({ "kind": "intent.created", "body": { "title": "Login", "project": b, "state": "planned" } }))).unwrap();
    store.append(&boss, body(json!({ "kind": "intent.state_changed", "body": { "intent": "i1", "state": "active", "workspace": { "id": "w1", "label": "chat" } } }))).unwrap();
    let g = workriver_core::RuntimeRef { id: "w1".into(), label: "chat".into(), machine: None };
    let at = |cwd: &str, machine: Option<&str>| {
        let mut m = live("s-boss", "boss", "w1", Idle);
        m.cwd = cwd.into();
        m.machine = machine.map(str::to_string);
        vec![m]
    };

    // The manager runs in a registered project's folder: that project, whatever its intents.
    let s = store.state();
    assert_eq!(group_project(s, &at(&format!("{a}/sub"), None), &g), Some((a.clone(), Known::Manager)));
    let why = other_project(s, &at(&format!("{a}/sub"), None), &g, "i1").expect("another project");
    assert!(why.contains("works on project Alpha") && why.contains("i1 belongs to project Beta"), "{why}");
    // In a repository outside every project: that repository.
    assert_eq!(group_project(s, &at(&format!("{repo}/sub"), None), &g), Some((repo.clone(), Known::Manager)));
    // No manager running: its current intent's project, which agrees with the intent.
    assert_eq!(group_project(s, &[], &g), Some((b.clone(), Known::Intents)));
    assert_eq!(other_project(s, &[], &g, "i1"), None);
    // Another machine's path names no folder here unless a unit is recorded there.
    assert_eq!(group_project(s, &at(&format!("{a}/sub"), Some("mini")), &g), Some((b.clone(), Known::Intents)));

    // Set by hand: the user's alone, and it wins over the manager's folder; cleared, worked out again.
    let set = |p: Option<&str>| body(json!({ "kind": "workspace.project_set", "body": { "workspace": { "id": "w1", "label": "chat" }, "project": p } }));
    assert!(store.append(&boss, set(Some("p2"))).unwrap_err().to_string().contains("only the user"));
    assert!(store.append(&Principal::User, set(Some("p9"))).unwrap_err().to_string().contains("no project p9"));
    store.append(&Principal::User, set(Some("p2"))).unwrap();
    assert_eq!(group_project(store.state(), &at(&format!("{a}/sub"), None), &g), Some((b.clone(), Known::User)));
    store.append(&Principal::User, set(None)).unwrap();
    assert_eq!(group_project(store.state(), &at(&format!("{a}/sub"), None), &g), Some((a.clone(), Known::Manager)));
}

/// Starting an intent in a workspace that works on another project: the page must say the
/// user confirmed it; the manager is told the intent's project; a manager that starts one
/// itself is warned.
#[test]
fn starting_an_intent_in_another_projects_workspace() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    // As the service records folders, and as the runtime reports where an agent runs.
    let dir = |n: &str| {
        std::fs::create_dir_all(tmp.path().join(n)).unwrap();
        std::fs::canonicalize(tmp.path().join(n)).unwrap().to_string_lossy().into_owned()
    };
    let (a, b) = (dir("alpha"), dir("beta"));
    let fake = std::sync::Arc::new(Fake::default());
    let mut m = live("s-boss", "boss", "w1", Idle);
    m.cwd = a.clone();
    *fake.agents.lock().unwrap() = vec![("w1:p1".into(), m)];
    let (port, kernel) = daemon_over(fake.clone(), None, &tmp.path().join("w.db"));
    let user = kernel.client(port, tmp.path());
    let boss = kernel.daemon(port, Some("w1:p1".into()), Some(tmp.path().to_path_buf()));
    user.append("", "project.added", json!({ "path": a, "title": "Alpha" })).unwrap();
    user.append("", "project.added", json!({ "path": b, "title": "Beta \"β\"" })).unwrap();
    boss.append("", "manager.registered", json!({ "workspace": { "id": "w1", "label": "chat" } })).unwrap();
    user.append("", "intent.created", json!({ "title": "Login", "project": b, "state": "planned" })).unwrap();
    user.append("", "intent.created", json!({ "title": "Search", "project": b, "state": "planned" })).unwrap();
    user.append("", "intent.created", json!({ "title": "Docs", "project": a, "state": "planned" })).unwrap();

    let err = user.call("/api/intent/start", Some(json!({ "intent": "i1", "workspace": "w1" }))).unwrap_err().to_string();
    assert!(err.contains("works on project Alpha") && err.contains("confirmOtherProject"), "{err}");
    assert!(fake.sent.lock().unwrap().is_empty(), "not started, nobody told");
    let ws = user.get("/api/workspace").unwrap();
    assert_eq!((ws["workspaces"][0]["project"].as_str(), ws["workspaces"][0]["projectTitle"].as_str(), ws["workspaces"][0]["projectFrom"].as_str()), (Some(a.as_str()), Some("Alpha"), Some("manager")), "{}", ws["workspaces"]);

    let done = user.call("/api/intent/start", Some(json!({ "intent": "i1", "workspace": "w1", "confirmOtherProject": true }))).unwrap();
    assert_eq!(done["delivery"]["step"], "delivered", "{done}");
    let told = fake.sent.lock().unwrap()[0].1.clone();
    assert!(told.starts_with(&format!("[workriver] The user started i1 «Login» of project «Beta 'β'» (folder «{b}») in your workspace;")), "{told}");
    assert!(told.contains("If your workspace works on another project, do not split it: ask the user first (`workriver guide how ask-user`)."), "{told}");

    // The manager starts one itself: recorded, with a warning; none for its own project.
    let out = boss.append("", "intent.state_changed", json!({ "intent": "i2", "state": "active", "workspace": { "id": "w1", "label": "chat" } })).unwrap();
    let warning = out["warning"].as_str().unwrap_or_default();
    assert!(warning.contains("i2 belongs to project Beta 'β'") && warning.contains("ask the user with a decide gate"), "{out}");
    let out = boss.append("", "intent.state_changed", json!({ "intent": "i3", "state": "active", "workspace": { "id": "w1", "label": "chat" } })).unwrap();
    assert!(out.get("warning").is_none(), "{out}");
}

// ---- which pane a call truly comes from (docs/design/caller-identity.md) ----
