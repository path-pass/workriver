// What the page and the CLI read of a unit and a task.
use std::process::Command;

use serde_json::json;

use super::support::*;

/// A commit made without its trailer is linked to its task by hand; and a unit whose work
/// is rebased onto a new branch stays one unit: its commits are read from the new branch,
/// the link follows its commit to the new hash, and what only the old branch had is told.
#[test]
fn linked_commits_and_rebased_branches() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    git(&repo, &["switch", "-q", "-c", "feat/f"]);
    let commit = |file: &str, message: &str| {
        std::fs::write(repo.join(file), format!("{file}\n")).unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", message]);
        let out = Command::new("git").arg("-C").arg(&repo).args(["rev-parse", "HEAD"]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    let c1 = commit("one.rs", "One\n\nTask: u1#1");
    let c2 = commit("two.rs", "Two, made without its trailer");
    let c3 = commit("three.rs", "Three\n\nTask: u1#2");
    let base = { let o = Command::new("git").arg("-C").arg(&repo).args(["rev-parse", "main"]).output().unwrap(); String::from_utf8_lossy(&o.stdout).trim().to_string() };

    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-w", "w", "w1", Idle)), ("p3".into(), live("s-x", "x", "w1", Idle))];
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
    let (boss, w, x) = (at("p1"), at("p2"), at("p3"));
    let group = json!({ "id": "w1", "label": "chat" });
    user.append("", "intent.created", json!({ "title": "Login", "state": "planned" })).unwrap();
    boss.append("", "manager.registered", json!({ "workspace": group })).unwrap();
    boss.append("", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": group })).unwrap();
    boss.append("", "unit.created", json!({ "intent": "i1", "title": "Auth", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    for (title, after) in [("First", "start"), ("Second", "1")] {
        boss.append("", "task.created", json!({ "unit": "u1", "kind": "implement", "title": title, "agent": "s-w", "inputs": [{ "from": after }] })).unwrap();
    }
    let unit = || user.get("/api/workspace").unwrap()["intents"][0]["units"][0].clone();
    let tasks = |u: &serde_json::Value| u["tasks"].as_array().unwrap().iter().map(|t| (t["state"].as_str().unwrap().to_string(), t["output"]["commits"].as_array().map(|c| c.len()).unwrap_or(0))).collect::<Vec<_>>();
    assert_eq!(tasks(&unit()), [("todo".into(), 1), ("todo".into(), 1), ("unregistered".into(), 1)], "the commit without a trailer is a gap");

    // Linked by the task's agent (the manager and the user may too), not by another agent;
    // only a commit of the unit's branch.
    assert!(x.append("", "commit.linked", json!({ "unit": "u1", "hash": &c2[..9], "task": 1 })).unwrap_err().to_string().contains("by the agent of that task, the manager of its intent, or the user"));
    assert!(w.append("", "commit.linked", json!({ "unit": "u1", "hash": base, "task": 1 })).unwrap_err().to_string().contains("is not one of unit u1's"));
    assert!(w.append("", "commit.linked", json!({ "unit": "u1", "hash": "0000000", "task": 1 })).unwrap_err().to_string().contains("no commit 0000000"));
    let linked = w.append("", "commit.linked", json!({ "unit": "u1", "hash": &c2[..9], "task": 1 })).unwrap();
    assert_eq!((linked["body"]["hash"].as_str(), linked["body"]["subject"].as_str(), linked["body"]["patch_id"].as_str().map(str::len)), (Some(c2.as_str()), Some("Two, made without its trailer"), Some(40)));
    let u = unit();
    assert_eq!(tasks(&u), [("todo".into(), 2), ("todo".into(), 1)], "the gap is closed");
    assert_eq!(u["commits"].as_array().unwrap().iter().map(|c| c["linked"].as_bool().unwrap_or(false)).collect::<Vec<_>>(), [false, true, false]);

    // A link wins over what the commit's own message says.
    boss.append("", "commit.linked", json!({ "unit": "u1", "hash": c3, "task": 1 })).unwrap();
    assert_eq!(tasks(&unit()), [("todo".into(), 3), ("todo".into(), 0)]);
    boss.append("", "commit.unlinked", json!({ "unit": "u1", "hash": c3 })).unwrap();
    assert_eq!(tasks(&unit()), [("todo".into(), 2), ("todo".into(), 1)]);

    // The branch cannot be force-pushed: the work is rebased onto the moved mainline as a new
    // branch, and one commit is left behind.
    git(&repo, &["switch", "-q", "main"]);
    commit("mainline.rs", "Mainline moved");
    git(&repo, &["switch", "-q", "-c", "feat/f2"]);
    git(&repo, &["cherry-pick", &c1, &c2]);
    assert!(w.append("", "unit.relocated", json!({ "unit": "u1", "repo": "", "branch": "feat/f2", "base": "" })).unwrap_err().to_string().contains("only the manager of intent i1 (boss) or the user moves its units"));
    let moved = boss.append("", "unit.relocated", json!({ "unit": "u1", "repo": "", "branch": "feat/f2", "base": "" })).unwrap();
    assert_eq!(moved["body"]["previous"].as_array().unwrap().iter().map(|c| (c["hash"].as_str().unwrap().to_string(), c["task"].as_u64())).collect::<Vec<_>>(), [(c1.clone(), Some(1)), (c2.clone(), Some(1)), (c3.clone(), Some(2))]);
    let u = unit();
    assert_eq!((u["branch"].as_str(), u["previously"][0].as_str(), u["commits"].as_array().unwrap().len()), (Some("feat/f2"), Some("feat/f"), 2), "read from the new branch");
    assert!(u["commits"].as_array().unwrap().iter().all(|c| c["id"].as_str() != Some(&c1[..9]) && c["id"].as_str() != Some(&c2[..9])), "new hashes");
    assert_eq!(tasks(&u), [("todo".into(), 2), ("todo".into(), 0)], "the trailer and the link both follow the rebase; nothing became a gap");
    assert_eq!(u["commits"][1]["linked"], true, "found again by its patch id");
    assert_eq!(u["tasks"][1]["lostCommits"], json!([&c3[..9]]), "its commit was on the old branch only: said, not shown as none");
    let board = crate::cli::board_text(&boss, None).unwrap();
    assert!(board.contains("previously on feat/f (from main), 3 commits seen there") && board.contains(&format!("#2: commits not on the current branch: {}", &c3[..9])), "{board}");

    // Changed on the way (a conflict resolved in the rebase): its patch id is another, and it
    // is found by its subject and author date.
    std::fs::write(repo.join("two.rs"), "resolved\n").unwrap();
    git(&repo, &["commit", "-q", "-a", "--amend", "--no-edit"]);
    let u = unit();
    assert_eq!((tasks(&u), u["commits"][1]["linked"].as_bool()), (vec![("todo".into(), 2), ("todo".into(), 0)], Some(true)));

    // Taken back: the commit is a gap again.
    user.append("", "commit.unlinked", json!({ "hash": &c2[..7] })).unwrap();
    assert_eq!(tasks(&unit()), [("todo".into(), 1), ("todo".into(), 0), ("unregistered".into(), 1)]);
    let state: workriver_core::State = serde_json::from_value(user.get("/api/state").unwrap()).unwrap();
    assert_eq!((state.units[0].tasks.len(), state.units[0].earlier.len(), state.units[0].links.len()), (2, 1, 0));
}

/// `workriver task show`: one task with everything its reader may see, in full; the task an
/// agent is on when it names none; blind scoping; a JSON shape that stays.
#[test]
fn a_task_shown_in_full() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    git(&repo, &["switch", "-q", "-c", "feat/f"]);
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-rev", "rev", "w1", Idle)), ("p3".into(), live("s-fix", "fix", "w1", Idle)), ("p4".into(), live("s-sec", "sec", "w1", Idle))];
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
    let (boss, rev, fix, sec) = (at("p1"), at("p2"), at("p3"), at("p4"));
    let group = json!({ "id": "w1", "label": "chat" });
    user.append("", "intent.created", json!({ "title": "Login", "goal": "Safe login.", "state": "planned" })).unwrap();
    boss.append("", "manager.registered", json!({ "workspace": group })).unwrap();
    boss.append("", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": group })).unwrap();
    boss.append("", "unit.created", json!({ "intent": "i1", "title": "Auth", "purpose": "Tokens that expire", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Check tokens", "agent": "s-fix" })).unwrap();
    rev.append("", "task.created", json!({ "unit": "u1", "kind": "review", "inputs": [{ "from": "1" }] })).unwrap();
    rev.append("", "task.delivered", json!({ "unit": "u1", "task": 2, "findings": [{ "severity": "P1", "problem": "Token never expires", "impact": "Stolen tokens work forever", "where": ["auth.rs"], "suggestion": { "decision": "accept", "reason": "Easy to fix" } }] })).unwrap();
    boss.append("", "finding.decided", json!({ "unit": "u1", "task": 2, "finding": "P1-1", "decision": "accept", "note": "Real risk" })).unwrap();
    user.append("", "finding.decided", json!({ "unit": "u1", "task": 2, "finding": "P1-1", "decision": "revise", "note": "Expire after 15 minutes" })).unwrap();
    rev.append("", "finding.noted", json!({ "unit": "u1", "task": 2, "finding": "P1-1", "text": "The fix needs a test with an expired token" })).unwrap();
    // Five requirements and more than any message would carry: all of it comes back.
    let long: String = (1..=5).map(|k| format!("{k}) Requirement {k}: {}\n", "x".repeat(600))).collect();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Expire tokens", "text": long, "causes": [{ "task": 2, "findings": ["P1-1"] }], "agent": "s-fix", "plans": ["src/auth"] })).unwrap();
    std::fs::write(repo.join("auth.rs"), "fn expire() {}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "Expire tokens\n\nTask: u1#3"]);
    boss.call("/api/ask", Some(json!({ "place": "p1", "unit": "u1", "task": 1, "text": "Which clock?", "context": [] }))).unwrap();

    // No task named: the one this agent is on, or which ones when several.
    assert!(fix.get("/api/task").unwrap_err().to_string().contains("you have no task in progress"));
    fix.append("", "task.started", json!({ "unit": "u1", "task": 3 })).unwrap();
    let mine: crate::common::wire::TaskShow = serde_json::from_value(fix.get("/api/task").unwrap()).unwrap();
    assert_eq!(mine.reference, "u1#3");
    fix.append("", "task.started", json!({ "unit": "u1", "task": 1 })).unwrap();
    let several = fix.get("/api/task").unwrap_err().to_string();
    assert!(several.contains("you are on several tasks: u1#1 \"Check tokens\", u1#3 \"Expire tokens\""), "{several}");
    assert!(user.get("/api/task").unwrap_err().to_string().contains("name the task"));
    let first: crate::common::wire::TaskShow = serde_json::from_value(user.get("/api/task?task=u1%231").unwrap()).unwrap();
    assert_eq!(first.questions.iter().map(|q| (q.question.as_str(), q.asked_by.as_str())).collect::<Vec<_>>(), [("Which clock?", "boss")]);
    assert!(crate::cli::text(&first).contains("Questions:\n  q1 asked by boss ("), "{}", crate::cli::text(&first));

    let shown = fix.get("/api/task?task=u1%233").unwrap();
    let s: crate::common::wire::TaskShow = serde_json::from_value(shown.clone()).unwrap();
    assert_eq!(s.text, long, "untruncated");
    assert_eq!((s.kind.as_str(), s.state.as_str(), s.written_by.as_str(), s.agent.as_deref(), s.intent.about.as_str()), ("implement", "running", "boss", Some("fix"), "Safe login."));
    assert_eq!(s.place.as_ref().map(|p| (p.machine.clone(), p.branch.as_str(), p.base.as_str())), Some((None, "feat/f", "main")));
    let cause = &s.causes[0];
    assert_eq!((cause.from.as_str(), cause.finding.reported_by.as_str(), cause.finding.state.as_str()), ("u1#2", "rev", "revised"));
    assert_eq!(cause.finding.decisions.iter().map(|d| (d.state.as_str(), d.by.as_str(), d.note.as_str())).collect::<Vec<_>>(), [("accepted", "the manager (boss)", "Real risk"), ("revised", "the user", "Expire after 15 minutes")]);
    assert_eq!((cause.finding.notes[0].by.as_str(), cause.finding.notes[0].text.as_str()), ("rev", "The fix needs a test with an expired token"));
    assert_eq!(s.commits.iter().map(|c| (c.subject.as_str(), c.linked)).collect::<Vec<_>>(), [("Expire tokens", false)]);
    // The JSON's fields, as agents and scripts read them.
    let keys = |v: &serde_json::Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>().join(",");
    assert_eq!(keys(&shown), "after,agent,blind,causes,commits,intent,kind,link,lostCommits,output,place,plans,questions,ref,state,steps,text,title,unit,writtenBy");
    assert_eq!(keys(&shown["causes"][0]["finding"]), "decisions,files,id,impact,notes,problem,reason,reportedBy,severity,state,suggestion,where");
    let text = crate::cli::text(&s);
    for part in ["u1#3  implement  running", "Intent i1 \"Login\": Safe login.", "Unit u1 \"Auth\" (working): Tokens that expire", "What it asks (written by boss):\n    1) Requirement 1:", "from u1#2:\n    P1-1 (P1, revised), reported by rev", "accepted by the manager (boss): Real risk", "revised by the user: Expire after 15 minutes", "added by rev: The fix needs a test with an expired token", "Suggested by the reporter: accept: Easy to fix", "      Problem: Token never expires", "Plans to change: src/auth", "Output: not delivered yet", "Commits:\n  "] {
        assert!(text.contains(part), "{part:?} in\n{text}");
    }
    assert!(text.contains(&format!("    5) Requirement 5: {}", "x".repeat(600))));

    // What a task delivered, and a gate's steps.
    let review = crate::cli::text(&serde_json::from_value(user.get("/api/task?task=u1%232").unwrap()).unwrap());
    assert!(review.contains("Output (delivered ") && review.contains("  P1-1 (P1, revised), reported by rev\n    Problem: Token never expires"), "{review}");
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "gate", "gate": "deploy", "title": "Deploy", "inputs": [{ "from": "3" }], "steps": [{ "text": "Deploy to staging", "command": "npm run deploy" }] })).unwrap();
    let gate = crate::cli::text(&serde_json::from_value(fix.get("/api/task?task=u1%234").unwrap()).unwrap());
    assert!(gate.contains("After: u1#3\nSteps:\n  1. Deploy to staging\n     $ npm run deploy\n"), "{gate}");

    // A blind agent: its own task in full; nothing else.
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "review", "reviewer": "security-review", "inputs": [{ "from": "1" }], "agent": "s-sec", "blind": true, "text": "Find a way around the check" })).unwrap();
    let own: crate::common::wire::TaskShow = serde_json::from_value(sec.get("/api/task?task=u1%235").unwrap()).unwrap();
    assert_eq!((own.text.as_str(), own.blind, own.causes.len(), own.commits.len()), ("Find a way around the check", true, 0, 0));
    assert!(sec.get("/api/task?task=u1%233").unwrap_err().to_string().contains("no task"), "another task is hidden");
    assert!(sec.get("/api/task?task=u1%232").is_err());

    // What a design before a task delivered goes with the task, however many tasks between:
    // its note and its files. A task with no design before it says nothing of it.
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "design", "title": "Where tokens live", "inputs": [{ "from": "start" }], "agent": "s-rev" })).unwrap();
    rev.append("", "task.delivered", json!({ "unit": "u1", "task": 6, "note": "Tokens live in the store.\nThe page never holds one." })).unwrap();
    rev.append("", "attachment.added", json!({ "id": "00000000000000d6", "name": "tokens.md", "type": "text/markdown", "size": 3, "unit": "u1", "task": 6 })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Move tokens", "inputs": [{ "from": "6" }], "agent": "s-fix" })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "review", "reviewer": "architecture-review", "inputs": [{ "from": "7" }], "agent": "s-rev" })).unwrap();
    for n in [7, 8] {
        let shown: crate::common::wire::TaskShow = serde_json::from_value(fix.get(&format!("/api/task?task=u1%23{n}")).unwrap()).unwrap();
        assert_eq!(shown.earlier.iter().map(|d| (d.from.as_str(), d.kind.as_str(), d.delivered_by.as_str(), d.note.as_str())).collect::<Vec<_>>(), [("u1#6", "design", "rev", "Tokens live in the store.\nThe page never holds one.")], "u1#{n}");
        assert!(shown.earlier[0].files[0].ends_with("00000000000000d6.md"), "{:?}", shown.earlier[0].files);
        let text = crate::cli::text(&shown);
        assert!(text.contains("Designed and found before this task (each design says whether the user confirmed it; if one seems wrong, see `workriver guide how contract`):\n  from u1#6 (design \"Where tokens live\"), designed by rev:\n      Tokens live in the store.\n      The page never holds one.\n    Files: "), "{text}");
    }
    assert!(crate::cli::text(&serde_json::from_value(fix.get("/api/task?task=u1%238").unwrap()).unwrap()).starts_with("u1#8  review  todo"));
    assert!(s.earlier.is_empty() && !text.contains("before this task"));
    // A design for the whole intent, in a research unit, reaches the code unit's tasks once
    // delivered, before the unit's own; its research does not, nor a design not delivered.
    boss.append("", "unit.created", json!({ "intent": "i1", "title": "Sessions", "kind": "research" })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u2", "kind": "research", "title": "Compare stores", "agent": "s-rev" })).unwrap();
    rev.append("", "task.delivered", json!({ "unit": "u2", "task": 1, "note": "Three stores compared." })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u2", "kind": "design", "title": "One session store", "inputs": [{ "from": "1" }], "agent": "s-rev" })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u2", "kind": "design", "title": "Not yet", "inputs": [{ "from": "2" }], "agent": "s-rev" })).unwrap();
    let from = |r: &str| serde_json::from_value::<crate::common::wire::TaskShow>(fix.get(&format!("/api/task?task={}", r.replace('#', "%23"))).unwrap()).unwrap().earlier.iter().map(|d| d.from.clone()).collect::<Vec<_>>();
    assert_eq!(from("u1#7"), ["u1#6"]);
    rev.append("", "task.delivered", json!({ "unit": "u2", "task": 2, "note": "Sessions go in SQLite." })).unwrap();
    assert_eq!((from("u1#7"), from("u1#3"), from("u2#3"), from("u2#2")), (vec!["u2#2".to_string(), "u1#6".into()], vec!["u2#2".to_string()], vec!["u2#1".to_string(), "u2#2".into()], vec!["u2#1".to_string()]));
    let wide = crate::cli::text(&serde_json::from_value(fix.get("/api/task?task=u1%233").unwrap()).unwrap());
    assert!(wide.contains("  from u2#2 (design \"One session store\"), designed by rev: Sessions go in SQLite.\n"), "{wide}");
    // Research reads as found; a design says where its gate stands.
    let show = |r: &str| crate::cli::text(&serde_json::from_value(fix.get(&format!("/api/task?task={}", r.replace('#', "%23"))).unwrap()).unwrap());
    assert!(show("u2#3").contains("  from u2#1 (research \"Compare stores\"), found by rev: Three stores compared.\n"));
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "gate", "gate": "decide", "title": "Confirm", "inputs": [{ "from": "6" }] })).unwrap();
    assert!(show("u1#7").contains("  from u1#6 (design \"Where tokens live\"), designed by rev, not confirmed yet (gate u1#9):\n"), "{}", show("u1#7"));
    // The user does not confirm: said so; the design that answers the gate's finding takes
    // the old one's place for every task after either, once delivered, and alone.
    user.append("", "gate.submitted", json!({ "unit": "u1", "task": 9, "result": "problem", "findings": [{ "severity": "P2", "problem": "Tokens belong in the keychain" }] })).unwrap();
    assert!(show("u1#7").contains("designed by rev, not confirmed by the user in u1#9:\n"));
    user.append("", "finding.decided", json!({ "unit": "u1", "task": 9, "finding": "P2-1", "decision": "accept" })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "design", "title": "Tokens in the keychain", "causes": [{ "task": 9, "findings": ["P2-1"] }], "agent": "s-rev" })).unwrap();
    // Until the revision is delivered the old design stays, said to be under revision; the
    // task that revises it is shown it as such, also afterwards.
    assert!(show("u1#7").contains("designed by rev, not confirmed by the user in u1#9, being revised in u1#10:\n"), "{}", show("u1#7"));
    assert!(show("u1#10").contains("  from u1#6 (design \"Where tokens live\", the design you revise), designed by rev, not confirmed by the user in u1#9:\n"), "{}", show("u1#10"));
    rev.append("", "task.delivered", json!({ "unit": "u1", "task": 10, "note": "Tokens go in the keychain." })).unwrap();
    // Delivered, the revision stands in the old design's place for every task after the old
    // one, also those that do not come after the revision; the revision still reads the old.
    assert_eq!((from("u1#7"), from("u1#10")), (vec!["u2#2".to_string(), "u1#10".into()], vec!["u2#2".to_string(), "u1#6".into()]));
    assert!(show("u1#7").contains("  from u1#10 (design \"Tokens in the keychain\", revises u1#6), designed by rev: Tokens go in the keychain.\n"), "{}", show("u1#7"));
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "gate", "gate": "decide", "title": "Confirm again", "inputs": [{ "from": "10" }] })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Use the keychain", "inputs": [{ "from": "11" }], "agent": "s-fix" })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "review", "reviewer": "architecture-review", "inputs": [{ "from": "10" }], "agent": "s-rev" })).unwrap();
    assert_eq!((from("u1#12"), from("u1#7")), (vec!["u2#2".to_string(), "u1#10".into()], vec!["u2#2".to_string(), "u1#10".into()]), "once, also after both");
    assert!(show("u1#13").contains("  from u1#10 (design \"Tokens in the keychain\", the design under review, revises u1#6), designed by rev, not confirmed yet (gate u1#11): Tokens go in the keychain.\n"), "{}", show("u1#13"));
    user.append("", "gate.submitted", json!({ "unit": "u1", "task": 11, "result": "done" })).unwrap();
    assert!(show("u1#12").contains("  from u1#10 (design \"Tokens in the keychain\", revises u1#6), designed by rev, confirmed by the user in u1#11: Tokens go in the keychain.\n"), "{}", show("u1#12"));
    // A later question to merge is not the design's gate: open or refused, the design stays confirmed.
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "gate", "gate": "decide", "title": "Merge u1 into main?", "inputs": [{ "from": "12" }] })).unwrap();
    assert!(show("u1#12").contains("designed by rev, confirmed by the user in u1#11: "), "{}", show("u1#12"));
    fix.append("", "task.delivered", json!({ "unit": "u1", "task": 12, "note": "In the keychain" })).unwrap();
    user.append("", "gate.submitted", json!({ "unit": "u1", "task": 14, "result": "problem", "findings": [{ "severity": "P2", "problem": "Not yet" }] })).unwrap();
    assert!(show("u1#12").contains("designed by rev, confirmed by the user in u1#11: "), "{}", show("u1#12"));
    // A blind task after the same designs is told nothing of them.
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "review", "inputs": [{ "from": "12" }], "agent": "s-sec", "blind": true, "text": "Review the move" })).unwrap();
    let own = sec.get("/api/task?task=u1%2315").unwrap();
    assert!(own.get("earlier").is_none() && !crate::cli::text(&serde_json::from_value(own).unwrap()).contains("Tokens live"));
    // Nobody reads as nobody.
    assert_eq!(http(port, "GET", "/api/task?task=u1%233", &[], None).0, 401);
}

/// A file another open unit of the same project and repository also changes is named on
/// each unit: by its net change, uncommitted work included; a merged or closed unit, and a
/// unit of another project, are not.
#[test]
fn files_two_open_units_change() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = |name: &str| {
        let r = tmp.path().join(name);
        std::fs::create_dir(&r).unwrap();
        git(&r, &["init", "-q", "-b", "main"]);
        for f in ["a.txt", "b.txt", "c.txt", "d.txt"] {
            std::fs::write(r.join(f), format!("{f}\n")).unwrap();
        }
        git(&r, &["add", "."]);
        git(&r, &["commit", "-q", "-m", "base"]);
        r
    };
    let branch = |r: &std::path::Path, name: &str, files: &[&str]| {
        git(r, &["switch", "-q", "-c", name, "main"]);
        for f in files {
            std::fs::write(r.join(f), format!("{f} on {name}\n")).unwrap();
        }
        git(r, &["commit", "-q", "-am", name]);
        git(r, &["switch", "-q", "main"]);
    };
    let (one, other) = (repo("one"), repo("other"));
    branch(&one, "feat/one", &["a.txt", "b.txt"]);
    branch(&one, "feat/two", &["a.txt", "d.txt"]);
    branch(&one, "feat/three", &["c.txt"]);
    branch(&one, "feat/four", &["a.txt"]);
    branch(&one, "feat/five", &["b.txt"]);
    branch(&other, "feat/six", &["a.txt"]);
    // feat/one in a work tree of its own, writing d.txt.
    let work = tmp.path().join("one-work");
    git(&one, &["worktree", "add", "-q", &work.to_string_lossy(), "feat/one"]);
    std::fs::write(work.join("d.txt"), "d.txt, being written\n").unwrap();

    let store = workriver_data::Store::open(&tmp.path().join("w.db")).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let port = listener.local_addr().unwrap().port();
    let options = crate::server::Options { allow_as: true, ..kernel.options() };
    std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, std::sync::Arc::new(workriver_core::runtime::NoRuntime), options)));
    let d = kernel.client(port, tmp.path());
    let (one, other) = (one.to_string_lossy().to_string(), other.to_string_lossy().to_string());
    for (title, project) in [("Search", &one), ("Login", &one), ("Elsewhere", &other)] {
        d.append("user", "intent.created", json!({ "title": title, "project": project })).unwrap();
    }
    for (intent, r, b) in [("i1", &one, "feat/one"), ("i2", &one, "feat/two"), ("i1", &one, "feat/three"), ("i2", &one, "feat/four"), ("i2", &one, "feat/five"), ("i3", &other, "feat/six")] {
        d.append("user", "unit.created", json!({ "intent": intent, "title": b, "repo": r, "branch": b, "base": "main" })).unwrap();
    }
    d.append("user", "unit.merged", json!({ "unit": "u4" })).unwrap();
    d.append("user", "unit.done", json!({ "unit": "u5", "reason": "superseded" })).unwrap();

    let ws = d.get("/api/workspace").unwrap();
    let overlaps = |id: &str| {
        let u = ws["intents"].as_array().unwrap().iter().flat_map(|i| i["units"].as_array().unwrap()).find(|u| u["id"] == id).unwrap();
        u.get("overlaps").cloned().unwrap_or(json!([]))
    };
    assert_eq!(overlaps("u1"), json!([{ "path": "a.txt", "units": ["u2"] }, { "path": "d.txt", "units": ["u2"] }]), "b.txt: u5 is closed, u4 merged");
    assert_eq!(overlaps("u2"), json!([{ "path": "a.txt", "units": ["u1"] }, { "path": "d.txt", "units": ["u1"] }]));
    for id in ["u3", "u4", "u5", "u6"] {
        assert_eq!(overlaps(id), json!([]), "{id}");
    }
}
