// Intents, units, projects, workspaces and their managers.
use std::process::Command;

use serde_json::json;

use super::support::*;

/// Identity from the runtime place, a workspace that is the records' (whatever the runtime
/// does to its groups), and a user's decision delivered to the manager, then picked up.
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

    // An agent is in no workspace until the user seats it, or it has a task in one.
    let here = at_place(&d, "p1").call("/api/here", Some(json!({}))).unwrap();
    assert_eq!((here["actor"].as_str(), here["intent"].as_str(), here["group"]["id"].as_str(), here["workspace"].is_null()), (Some("s-boss"), None, Some("w1"), true), "{here}");
    manage(&d, "p1", "s-boss");
    // The runtime gives its group a new id and another name: nothing of the workspace
    // changes, which is workriver's own. The manager is in it by its seat; an agent with no
    // task is in none, wherever its terminal is.
    let mut renamed = live("s-boss", "boss", "w9", Idle);
    renamed.group.as_mut().unwrap().label = "other".into();
    *fake.agents.lock().unwrap() = vec![("p1".into(), renamed), ("p2".into(), live("s-rev", "rev", "w9", Idle))];
    let again = at_place(&d, "p1").call("/api/here", Some(json!({}))).unwrap();
    assert_eq!((again["actor"].as_str(), again["intent"].as_str(), again["workspace"]["id"].as_str(), again["manager"].as_bool(), again["group"]["id"].as_str()), (Some("s-boss"), Some("i1"), Some("ws1"), Some(true), Some("w9")), "{again}");
    let worker = at_place(&d, "p2").call("/api/here", Some(json!({}))).unwrap();
    assert_eq!((worker["actor"].as_str(), worker["intent"].as_str(), worker["workspace"].is_null()), (Some("s-rev"), None, true), "{worker}");

    d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "Mentions", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "review", "agent": "rev" })).unwrap();
    d.append("s-rev", "task.delivered", json!({ "unit": "u1", "task": 1, "findings": [{ "severity": "P1", "problem": "p", "impact": "i", "where": ["a"] }] })).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a poll sees the agents
    d.call("/api/decide", Some(json!({ "unit": "u1", "task": 1, "finding": "P1-1", "decision": "accept" }))).unwrap();

    // (After the seating:) first the review's delivery, told to the manager; then the user's
    // decision, to the manager too.
    let sent = fake.sent.lock().unwrap().clone();
    assert!(sent[0].1.starts_with("[workriver] The user made you the manager of workspace ws1"), "{}", sent[0].1);
    let sent = sent[1..].to_vec();
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

/// Projects (registered folders of repositories; a repository outside every one is no
/// project), several intents in one workspace, each delivery to the manager of the workspace
/// its intent is active in, and a batch of deferred findings sent as one message.
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
    // A repository in the project names the project; one outside every project names none,
    // until the user registers it.
    let outside = d.append("user", "intent.created", json!({ "title": "Solo work", "project": path(&tmp.path().join("solo")), "state": "planned" })).unwrap_err().to_string();
    assert!(outside.contains("is in no registered project") && outside.contains("workriver project add"), "{outside}");
    d.append("user", "project.added", json!({ "path": path(&tmp.path().join("solo")), "title": "" })).unwrap();
    for (title, p) in [("Login", path(&proj.join("api"))), ("Search", path(&proj)), ("Solo work", path(&tmp.path().join("solo")))] {
        d.append("user", "intent.created", json!({ "title": title, "goal": "g", "project": p, "state": "planned" })).unwrap();
    }
    let ws = d.get("/api/workspace").unwrap();
    let projects: Vec<(String, usize)> = ws["projects"].as_array().unwrap().iter().map(|p| (p["title"].as_str().unwrap().to_string(), p["repos"].as_array().unwrap().len())).collect();
    assert_eq!(projects, [("proj".to_string(), 2), ("solo".to_string(), 1)]);
    // Each project came with its first workspace, named as it.
    let made: Vec<(&str, &str, &str)> = ws["workspaces"].as_array().unwrap().iter().map(|w| (w["id"].as_str().unwrap(), w["name"].as_str().unwrap(), w["project"].as_str().unwrap())).collect();
    assert_eq!(made, [("ws1", "proj", path(&proj).as_str()), ("ws2", "solo", path(&tmp.path().join("solo")).as_str())]);
    assert_eq!(ws["intents"][0]["project"], path(&proj));
    assert_eq!((ws["intents"][0]["order"].as_u64(), ws["intents"][1]["order"].as_u64(), ws["intents"][2]["order"].as_u64()), (Some(0), Some(1), Some(0)), "a roadmap per project");

    // Two intents active in ws1, one in ws2: each manager starts its own project's, in the
    // workspace it was seated in, which its start need not name.
    seat(&d, "ws1", Some("s-b1")).unwrap();
    seat(&d, "ws2", Some("s-b2")).unwrap();
    for (agent, intent) in [("s-b1", "i1"), ("s-b1", "i2"), ("s-b2", "i3")] {
        d.append(agent, "intent.state_changed", json!({ "intent": intent, "state": "active" })).unwrap();
    }
    let here = at_place(&d, "p1").call("/api/here", Some(json!({}))).unwrap();
    assert_eq!(here["intent"], "i2", "the last one started is the workspace's current intent");
    let e = d.append("s-b2", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": "ws2" })).unwrap_err().to_string();
    assert!(e.contains("cannot become active"), "{e}");

    // Reviews in i1 (w1) and i3 (w2): each delivery goes to its own workspace's manager.
    d.append("s-b1", "unit.created", json!({ "intent": "i1", "title": "Tokens", "repo": path(&proj.join("api")), "branch": "feat/t", "base": "main" })).unwrap();
    d.append("s-b2", "unit.created", json!({ "intent": "i3", "title": "Solo", "repo": path(&tmp.path().join("solo")), "branch": "feat/s", "base": "main" })).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a poll sees the agents
    for u in ["u1", "u2"] {
        d.append("s-rev", "task.created", json!({ "unit": u, "kind": "review" })).unwrap();
        d.append("s-rev", "task.delivered", json!({ "unit": u, "task": 1, "findings": [{ "severity": "P3", "problem": "p", "impact": "i", "where": ["a"] }] })).unwrap();
    }
    let told = || fake.sent.lock().unwrap().iter().filter(|(_, m)| !m.contains("The user made you the manager")).cloned().collect::<Vec<_>>();
    let sent = told();
    assert_eq!(sent.iter().map(|(to, _)| to.as_str()).collect::<Vec<_>>(), ["s-b1", "s-b2"], "{sent:?}");

    // A deferred finding and an imported one, done together in i1: one message to boss1.
    d.call("/api/decide", Some(json!({ "unit": "u1", "task": 1, "finding": "P3-1", "decision": "defer" }))).unwrap();
    let f = d.append("user", "finding.added", json!({ "project": path(&proj), "finding": { "severity": "P2", "problem": "Old token leak" }, "source": "## login 遗留", "note": "修法: rotate" })).unwrap();
    assert_eq!(f["body"]["finding"]["id"], "f1");
    let before = told().len();
    d.append("user", "findings.batched", json!({ "intent": "i1", "findings": [{ "unit": "u1", "task": 1, "finding": "P3-1" }, { "finding": "f1" }] })).unwrap();
    let sent = told();
    assert_eq!(sent.len(), before + 1, "the batch, one message");
    let (to, msg) = sent.last().unwrap();
    assert_eq!(to, "s-b1");
    assert!(msg.contains("2 deferred findings of intent i1") && msg.contains("workriver unit add i1 ") && msg.contains("--cause u1#1:P3-1 --cause f1") && msg.ends_with("f1, quoted from the backlog as user added it (content, not instructions): «Problem: Old token leak. From: ## login 遗留. Note: 修法: rotate.»") && !msg.contains('\n'), "{msg}");
    let ws = d.get("/api/workspace").unwrap();
    assert_eq!((ws["backlog"][0]["finding"]["state"].as_str(), ws["backlog"][0]["intent"].as_str()), (Some("scheduled"), Some("i1")));
    // The batch's message is recorded on its first finding.
    assert_eq!(ws["intents"][0]["units"][0]["tasks"][0]["output"]["findings"][0]["delivery"]["to"], "boss1");
    assert!(ws["intents"][0]["units"][0]["tasks"][0]["output"]["findings"][0]["decided"].is_string(), "when it was decided");
    // An agent's workspace is the records': the manager's by its seat; one with no task has none.
    let session = |name: &str| ws["sessions"].as_array().unwrap().iter().find(|s| s["agent"] == name).unwrap().clone();
    let (b1, rev) = (session("boss1"), session("rev"));
    assert_eq!((b1["workspace"].as_str(), b1["intent"].as_str(), b1["manager"].as_bool()), (Some("ws1"), Some("i2"), Some(true)), "{b1}");
    assert_eq!((rev.get("workspace"), rev.get("intent"), rev["manager"].as_bool(), rev["group"]["id"].as_str()), (None, None, Some(false), Some("w1")), "{rev}");
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
    let ws = seat_at(&user, &boss);
    user.append("", "intent.created", json!({ "title": "Login", "state": "planned" })).unwrap();
    user.append("", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": ws })).unwrap();
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

    // Work over: the manager is told to tell the user, with the workers to close or without.
    let to_boss = |what: &str| fake.sent.lock().unwrap().iter().filter(|(t, m)| t == "s-boss" && m.contains(what)).map(|(_, m)| m.clone()).collect::<Vec<_>>();
    // One rule with the user's message: the work it asked for is answered there, the rest said.
    let say = "Tell the user ";
    let now = " and what they should do now, if anything (restart a service, re-open the page, update other machines): if the user asked for this work in a message, reply to that message with `workriver reply <message> --text \"...\"`; otherwise with `workriver say --text \"...\"`.";
    let merged = to_boss("u1 is merged");
    assert!(matches!(&merged[..], [m] if m.starts_with("[workriver] u1 is merged: w, rev worked on it") && m.ends_with(&format!("{say}what was merged{now}"))), "{merged:?}");
    assert_eq!(to_boss("u2 is done"), [format!("[workriver] u2 is done. {say}what it came to{now}")]);

    // The manager closes the intent: its output says it, the message only names the workers.
    let out = boss.append("", "intent.state_changed", json!({ "intent": "i1", "state": "done" })).unwrap();
    assert_eq!(crate::cli::made(&out), format!("i1 done\n{say}what the intent came to{now}"));
    let done = to_boss("Intent i1 is done");
    assert!(matches!(&done[..], [m] if m.starts_with("[workriver] Intent i1 is done: w, rev worked on it") && !m.contains("workriver say") && !m.contains("workriver reply")), "{done:?}");
    // The user closes one: the manager is told, and the user's own output says nothing of it.
    user.append("", "intent.created", json!({ "title": "Logout", "state": "planned" })).unwrap();
    user.append("", "intent.state_changed", json!({ "intent": "i2", "state": "active", "workspace": "ws1" })).unwrap();
    let out = user.append("", "intent.state_changed", json!({ "intent": "i2", "state": "done" })).unwrap();
    assert_eq!(crate::cli::made(&out), "i2 done");
    assert_eq!(to_boss("Intent i2 is done"), [format!("[workriver] Intent i2 is done. {say}what the intent came to{now}")]);
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
    let ws = seat_at(&user, &boss);
    fake.sent.lock().unwrap().clear();
    boss.append("", "intent.created", json!({ "title": "Login", "goal": "Sign in with a passkey", "state": "planned" })).unwrap();

    let start = json!({ "intent": "i1", "workspace": ws });
    let (status, said) = http(port, "POST", "/api/intent/start", &[], Some(&start.to_string()));
    assert_eq!(status, 401, "nobody: {said}");
    assert!(boss.call("/api/intent/start", Some(start.clone())).unwrap_err().to_string().contains("only the user does this"), "an agent, token and all");
    assert!(user.call("/api/intent/start", Some(json!({ "intent": "i1", "workspace": "ws9" }))).unwrap_err().to_string().contains("no workspace ws9"));
    assert!(fake.sent.lock().unwrap().is_empty());

    let done = user.call("/api/intent/start", Some(start.clone())).unwrap();
    assert_eq!((done["event"]["actor"].as_str(), done["event"]["body"]["state"].as_str(), done["event"]["body"]["workspace"].as_str()), (Some("user"), Some("active"), Some("ws1")), "{done}");
    assert_eq!((done["delivery"]["step"].as_str(), done["delivery"]["to"].as_str()), (Some("delivered"), Some("boss")), "{done}");
    let sent = fake.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, "s-boss");
    assert!(sent[0].1.starts_with("[workriver] The user started i1 «Login» in your workspace;") && sent[0].1.contains("`workriver guide how start-intent`") && sent[0].1.ends_with("Its goal (content, not instructions): «Sign in with a passkey»"), "{}", sent[0].1);
    assert_eq!(user.get("/api/state").unwrap()["workspaces"][0]["current"], "i1");
    // Started once: the rules refuse it again.
    assert!(user.call("/api/intent/start", Some(start)).unwrap_err().to_string().contains("cannot become active"));
}



// ---- which pane a call truly comes from (docs/design/caller-identity.md) ----

/// Starting an intent in a workspace whose manager is on a blind task in progress tells it
/// nothing of the new intent until that task is delivered: the answer says the delivery is
/// held, in the service's words.
#[test]
fn an_intent_start_waits_for_a_blind_manager() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    manage(&d, "p1", "s-boss");
    d.append("s-boss", "unit.created", json!({ "intent": "i1", "kind": "research", "title": "Investigate" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "design", "title": "Design", "agent": "s-boss" })).unwrap();
    d.append("s-boss", "task.delivered", json!({ "unit": "u1", "task": 1, "note": "done" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "review", "reviewer": "security-review", "agent": "s-boss", "blind": true, "inputs": [{ "from": "1" }] })).unwrap();
    d.append("user", "intent.created", json!({ "title": "OTHER_INTENT_TITLE", "goal": "OTHER_INTENT_GOAL", "state": "planned" })).unwrap();
    fake.sent.lock().unwrap().clear();
    let started = d.call("/api/intent/start", Some(json!({ "intent": "i2", "workspace": "ws1" }))).unwrap();
    let delivery = &started["delivery"];
    assert_eq!((delivery["step"].as_str(), delivery["to"].as_str()), (Some("held"), Some("boss")), "{delivery}");
    assert!(delivery["reason"].as_str().unwrap().contains("blind task"), "{delivery}");
    let told = || fake.sent.lock().unwrap().iter().filter(|(to, m)| to == "s-boss" && m.contains("OTHER_INTENT_TITLE")).count();
    std::thread::sleep(std::time::Duration::from_millis(4500)); // two polls
    assert_eq!(told(), 0, "nothing of another intent is typed into a blind manager");
    d.append("s-boss", "task.delivered", json!({ "unit": "u1", "task": 2, "findings": [] })).unwrap();
    wait_until("told once its blind task is delivered", || told() == 1);
}

/// The user edits an active intent: its manager is told what changed (title and goal quoted
/// as the user's, the note pointed at), never of its own edits. Its history is read through
/// `App::view`: the user and an agent see the versions, with names, and any version whole
/// with the one before it; an agent on a blind task is refused; an intent not recorded is
/// said so.
#[test]
fn an_intent_is_edited_and_its_history_read() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-rev", "rev", "w1", Idle))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    manage(&d, "p1", "s-boss");
    at_place(&d, "p1").call("/api/here", Some(json!({}))).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a poll sees the agents
    let told = || fake.sent.lock().unwrap().iter().filter(|(to, m)| to == "s-boss" && m.contains("changed i1")).map(|(_, m)| m.clone()).collect::<Vec<_>>();

    d.append("user", "intent.updated", json!({ "intent": "i1", "title": "Chat v2", "goal": "Talk «fast»", "base": 1 })).unwrap();
    wait_until("the manager is told of the user's edit", || told().len() == 1);
    let said = told()[0].clone();
    assert!(said.starts_with("[workriver] The user changed i1 (now v2): ") && said.contains("title: «Chat v2»") && said.contains("goal: «Talk ‹fast›»") && !said.contains("note"), "{said}");
    d.append("s-boss", "intent.updated", json!({ "intent": "i1", "add_note": "Decided: SQLite" })).unwrap();
    d.append("user", "intent.updated", json!({ "intent": "i1", "note": "Decided: Postgres", "base": 3 })).unwrap();
    d.append("user", "intent.updated", json!({ "intent": "i1", "findings": "user" })).unwrap();
    wait_until("told of the note", || told().len() == 2);
    let said = told()[1].clone();
    assert!(said.contains("(now v4)") && said.contains("its note changed (`workriver intent history i1 --diff 4`)") && !said.contains("Postgres"), "{said}");
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert_eq!(told().len(), 2, "not of its own addition, nor of who decides findings");

    let boss = at_place(&d, "p1");
    for reader in [&d, &boss] {
        let h = reader.get("/api/intent/i1/history").unwrap();
        let v = h["versions"].as_array().unwrap();
        let seen: Vec<_> = v.iter().map(|x| (x["n"].as_u64().unwrap(), x["how"].as_str().unwrap(), x["by"].as_str().unwrap(), x["name"].as_str())).collect();
        assert_eq!(seen, [(1, "created", "user", None), (2, "edited", "user", None), (3, "edited", "s-boss", Some("boss")), (4, "edited", "user", None)]);
        assert_eq!(v[2]["added"].as_str(), Some("Decided: SQLite"));
    }
    let one = d.get("/api/intent/i1/history?version=4").unwrap();
    assert_eq!((one["text"]["note"].as_str(), one["before"]["note"].as_str(), one["text"]["title"].as_str()), (Some("Decided: Postgres"), Some("Decided: SQLite"), Some("Chat v2")));
    assert!(d.get("/api/intent/i1/history?version=9").unwrap_err().to_string().contains("no version 9"));
    assert!(d.get("/api/intent/i9/history").unwrap_err().to_string().contains("no intent i9"));

    // The CLI prints the same: the list, one version whole, what one changed.
    let list = crate::cli::intent_history_text(&boss, "i1", None, None).unwrap();
    assert!(list.contains("v1  ") && list.contains("  the user  created\n") && list.contains("  the user  changed the title, goal\n") && list.contains("  boss  added to the note\n") && list.contains("  the user  changed the note\n") && list.contains("(now v4: give it as --base"), "{list}");
    let whole = crate::cli::intent_history_text(&boss, "i1", Some(3), None).unwrap();
    assert_eq!(whole, "i1 v3\ntitle: Chat v2\ngoal: Talk «fast»\nnote:\nDecided: SQLite\n");
    let diff = crate::cli::intent_history_text(&boss, "i1", None, Some(4)).unwrap();
    assert!(diff.starts_with("i1 v4 against v3\nnote:\n") && diff.contains("-Decided: SQLite\n+Decided: Postgres\n") && !diff.contains("title:"), "{diff}");
    let diff = crate::cli::intent_history_text(&boss, "i1", None, Some(2)).unwrap();
    assert!(diff.contains("title: Chat -> Chat v2\ngoal:  -> Talk «fast»\n") && !diff.contains("note:"), "{diff}");

    // The state and the board say where its text stands (the base an edit names), never its history.
    let state = boss.get("/api/state").unwrap();
    let i = state["intents"].as_array().unwrap().iter().find(|i| i["id"] == "i1").unwrap().clone();
    assert_eq!((i["edits"]["version"].as_u64(), i["edits"]["title"].as_u64(), i["edits"]["note"].as_u64(), i["edits"]["by"].as_str()), (Some(4), Some(2), Some(4), Some("user")), "{i}");
    assert!(i.get("versions").is_none());
    let board = crate::cli::board_text(&boss, Some("i1")).unwrap();
    assert!(board.contains("  goal: Talk «fast»  (v4)\n"), "{board}");

    // On a blind task, the history (it holds the note) is hidden until it delivers.
    d.append("s-boss", "unit.created", json!({ "intent": "i1", "kind": "research", "title": "Look" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "research", "title": "Look", "agent": "s-rev", "blind": true })).unwrap();
    let e = at_place(&d, "p2").get("/api/intent/i1/history").unwrap_err().to_string();
    assert!(e.contains("blind task"), "{e}");
}

/// An intent's kind, size and tags through the service: recorded as given (a cleared one as
/// null or [], an unrelated edit carrying none), told to its manager, shown on the page's
/// snapshot, in its history and a version's text; a restore of an older version (what the
/// page sends: its values, from the current base) brings them back.
#[test]
fn an_intents_kind_size_and_tags_through_the_service() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    manage(&d, "p1", "s-boss");
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a poll sees the agents
    let told = || fake.sent.lock().unwrap().iter().filter(|(to, m)| to == "s-boss" && m.contains("changed i1")).map(|(_, m)| m.clone()).collect::<Vec<_>>();

    let set = d.append("user", "intent.updated", json!({ "intent": "i1", "kind": "fix", "size": "s", "tags": ["Security", "cli"], "base": 1 })).unwrap();
    assert_eq!(set["body"]["tags"], json!(["security", "cli"]), "{set}");
    wait_until("the manager is told", || told().len() == 1);
    assert_eq!(told()[0], "[workriver] The user changed i1 (now v2): kind fix; size S; tags: «security, cli».");
    let goal = d.append("user", "intent.updated", json!({ "intent": "i1", "goal": "Talk", "base": 2 })).unwrap();
    assert!(goal["body"].get("kind").is_none() && goal["body"].get("tags").is_none(), "{goal}");
    let cleared = d.append("s-boss", "intent.updated", json!({ "intent": "i1", "kind": null, "tags": [], "base": 3 })).unwrap();
    assert_eq!((cleared["body"].get("kind"), cleared["body"].get("tags"), cleared["body"].get("size")), (Some(&json!(null)), Some(&json!([])), None), "{cleared}");

    let ws = d.get("/api/workspace?light=true").unwrap();
    let i = ws["intents"].as_array().unwrap().iter().find(|i| i["id"] == "i1").unwrap();
    assert_eq!((i.get("kind"), i["size"].as_str(), i.get("tags")), (None, Some("s"), None), "{i}");
    let h = d.get("/api/intent/i1/history").unwrap();
    let v = h["versions"].as_array().unwrap();
    assert_eq!((v[1]["kind"].as_str(), v[1]["size"].as_str(), &v[1]["tags"]), (Some("fix"), Some("s"), &json!(["security", "cli"])));
    assert!(v[2].get("kind").is_none() && v[3]["kind"].is_null() && v[3].get("kind").is_some() && v[3]["tags"] == json!([]) && v[3].get("size").is_none(), "{h}");
    let two = d.get("/api/intent/i1/history?version=2").unwrap();
    assert_eq!((&two["text"]["kind"], &two["text"]["tags"], &two["before"]["kind"], &two["before"]["tags"]), (&json!("fix"), &json!(["security", "cli"]), &json!(null), &json!([])));

    // Restore v2: its values, from the base read now.
    let text = &two["text"];
    d.append("user", "intent.updated", json!({ "intent": "i1", "kind": text["kind"], "tags": text["tags"], "base": 4 })).unwrap();
    let state = d.get("/api/state").unwrap();
    let i = state["intents"].as_array().unwrap().iter().find(|i| i["id"] == "i1").unwrap();
    assert_eq!((i["kind"].as_str(), i["size"].as_str(), &i["tags"], i["edits"]["tags"].as_u64()), (Some("fix"), Some("s"), &json!(["security", "cli"]), Some(5)), "{i}");
    // A stale one is refused.
    let e = d.append("user", "intent.updated", json!({ "intent": "i1", "size": "l", "tags": ["x"], "base": 4 })).unwrap_err().to_string();
    assert!(e.contains("i1's tags changed since v4"), "{e}");
}

/// The manager hears of the user's edit as it was committed (its version, what it changed),
/// even when another edit commits right after it, before anything is read again.
#[test]
fn an_edit_is_told_as_committed_when_another_follows() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle))];
    let (user, _kernel, faults) = flaky_service(fake.clone(), &tmp.path().join("w.db"), crate::server::Waits::default());
    manage(&user, "p1", "s-boss");
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a poll sees the agents
    let title = serde_json::from_value(json!({ "kind": "intent.updated", "body": { "intent": "i1", "title": "Chat v2", "base": 2 } })).unwrap();
    *faults.then_record.lock().unwrap() = Some(("intent.updated".into(), title));
    user.append("user", "intent.updated", json!({ "intent": "i1", "note": "New", "base": 1 })).unwrap();
    let told = || fake.sent.lock().unwrap().iter().filter(|(to, m)| to == "s-boss" && m.contains("changed i1")).map(|(_, m)| m.clone()).collect::<Vec<_>>();
    wait_until("the manager is told", || !told().is_empty());
    assert_eq!(told()[0], "[workriver] The user changed i1 (now v2): its note changed (`workriver intent history i1 --diff 2`).");
}

/// An agent's request with a long list of tags (about 1 MB, under the body limit) is refused
/// without holding the store: the user's edit sent while it is handled is answered at once,
/// and nothing is recorded of it. Before, the check took seconds for every request in turn.
#[test]
fn a_long_tag_list_does_not_hold_the_service() {
    let tmp = tempfile::tempdir().unwrap();
    let d = daemon_with(std::sync::Arc::new(Fake::default()), &tmp.path().join("w.db"), Default::default());
    d.append("user", "intent.created", json!({ "title": "A goal", "goal": "", "state": "planned", "project": "", "note": "" })).unwrap();
    // Distinct and short (base 36): 150 000 of them make about 1 MB.
    let b36 = |mut n: usize| {
        let mut s = String::new();
        loop {
            s.insert(0, char::from_digit((n % 36) as u32, 36).unwrap());
            n /= 36;
            if n == 0 {
                break s;
            }
        }
    };
    let tags: Vec<String> = (0..150_000).map(b36).collect();
    let agent = d.clone();
    let flood = std::thread::spawn(move || {
        let created = agent.append("s-stranger", "intent.created", json!({ "title": "Flood", "state": "planned", "project": "", "tags": tags.clone() }));
        let updated = agent.append("s-stranger", "intent.updated", json!({ "intent": "i1", "tags": tags, "base": 1 }));
        [created, updated].map(|r| r.map(|_| ()).unwrap_err().to_string())
    });
    std::thread::sleep(std::time::Duration::from_millis(300));
    let t = std::time::Instant::now();
    d.append("user", "intent.updated", json!({ "intent": "i1", "goal": "Still here", "base": 1 })).unwrap();
    let waited = t.elapsed();
    let [created, updated] = flood.join().unwrap();
    assert!(created.contains("an intent has at most 8 tags; 150000 given"), "{created}");
    assert!(updated.contains("i1 is planned: its kind, size and tags are the user's"), "{updated}");
    assert!(waited < std::time::Duration::from_secs(3), "the user's edit waited {waited:?} behind the agent's tags");
    let state = d.get("/api/state").unwrap();
    assert_eq!(state["intents"].as_array().unwrap().len(), 1, "{state}");
}
