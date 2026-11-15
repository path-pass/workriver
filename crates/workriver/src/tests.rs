use std::io::{Read, Write};
use std::process::Command;

use serde_json::json;

use crate::Daemon;

fn git(dir: &std::path::Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "agent-a")
        .env("GIT_AUTHOR_EMAIL", "a@example.invalid")
        .env("GIT_COMMITTER_NAME", "agent-a")
        .env("GIT_COMMITTER_EMAIL", "a@example.invalid")
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// Starts the daemon on a free port with a temporary database, records through it the way
/// the CLI does, and reads the workspace back.
#[test]
fn daemon_smoke() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "base"]);
    git(&repo, &["switch", "-q", "-c", "feature"]);
    std::fs::write(repo.join("a.txt"), "one\ntwo\n").unwrap();
    git(&repo, &["commit", "-q", "-am", "two\n\nTask: u1#1"]);

    let store = workriver_core::Store::open(&tmp.path().join("w.db")).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::serve::bind(0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || rt.block_on(crate::serve::serve(listener, store, std::sync::Arc::new(workriver_core::runtime::NoRuntime), None, Default::default())));

    let d = Daemon { port };
    d.append("user", "intent.created", json!({ "title": "Chat" })).unwrap();
    let repo = repo.to_string_lossy();
    d.append("user", "unit.created", json!({ "intent": "i1", "title": "Mentions", "repo": repo, "branch": "feature", "base": "main" })).unwrap();
    d.append("manager", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Find mentions" })).unwrap();
    d.append("agent-a", "task.created", json!({ "unit": "u1", "kind": "review", "inputs": [{ "from": "1" }] })).unwrap();
    d.append("agent-a", "task.delivered", json!({ "unit": "u1", "task": 2, "findings": [{ "severity": "P2", "problem": "p", "impact": "i", "where": ["a.txt"] }] })).unwrap();
    // The page's route for a decision.
    d.call("/api/decide", Some(json!({ "unit": "u1", "task": 2, "finding": "P2-1", "decision": "defer" }))).unwrap();
    let err = d.append("agent-a", "task.started", json!({ "unit": "u1", "task": 9 })).unwrap_err();
    assert!(err.to_string().contains("no task u1#9"), "{err}");

    let ws = d.get("/api/workspace").unwrap();
    let unit = &ws["intents"][0]["units"][0];
    assert_eq!(unit["repo"], ws["repos"][0]["path"]);
    assert_eq!(unit["tasks"][0]["output"]["commits"].as_array().unwrap().len(), 1, "the trailer links the commit");
    assert_eq!(unit["commits"][0]["files"][0]["after"], "one\ntwo\n");
    assert_eq!(unit["tasks"][1]["output"]["findings"][0]["state"], "deferred");

    // Another Host is turned away, whatever the path.
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(s, "GET /api/workspace HTTP/1.1\r\nHost: evil.example:{port}\r\nConnection: close\r\n\r\n").unwrap();
    let mut answer = String::new();
    s.read_to_string(&mut answer).unwrap();
    assert!(answer.starts_with("HTTP/1.1 403"), "{answer}");
}

/// A runtime of our own: agents by place, every send kept, states set by the test.
#[derive(Default)]
struct Fake {
    agents: std::sync::Mutex<Vec<(String, workriver_core::runtime::RuntimeAgent)>>,
    sent: std::sync::Mutex<Vec<(String, String)>>,
    /// Temporary agents started: (group, harness, name, cwd); and those closed.
    started: std::sync::Mutex<Vec<(String, String, String, String)>>,
    closed: std::sync::Mutex<Vec<String>>,
}

impl workriver_core::runtime::Runtime for Fake {
    fn agents(&self) -> Result<Vec<workriver_core::runtime::RuntimeAgent>, workriver_core::runtime::RuntimeError> {
        Ok(self.agents.lock().unwrap().iter().map(|(_, a)| a.clone()).collect())
    }
    fn locate(&self, place: &str) -> Result<workriver_core::runtime::Place, workriver_core::runtime::RuntimeError> {
        let agent = self.agents.lock().unwrap().iter().find(|(p, _)| p == place).map(|(_, a)| a.clone());
        Ok(workriver_core::runtime::Place { group: agent.as_ref().and_then(|a| a.group.clone()), agent })
    }
    fn focus(&self, _: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
        Ok(())
    }
    fn send(&self, session: &str, text: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
        self.sent.lock().unwrap().push((session.into(), text.into()));
        Ok(())
    }
    fn start_temporary(&self, group: &str, harness: &str, name: &str, cwd: &str) -> Result<workriver_core::runtime::RuntimeAgent, workriver_core::runtime::RuntimeError> {
        self.started.lock().unwrap().push((group.into(), harness.into(), name.into(), cwd.into()));
        let mut agent = live(&format!("s-{name}"), name, group, workriver_core::runtime::AgentState::Idle);
        agent.harness = harness.into();
        self.agents.lock().unwrap().push((format!("place-{name}"), agent.clone()));
        Ok(agent)
    }
    fn close(&self, agent: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
        self.closed.lock().unwrap().push(agent.into());
        self.agents.lock().unwrap().retain(|(_, a)| a.name != agent && a.session != agent);
        Ok(())
    }
}

fn live(session: &str, name: &str, group: &str, state: workriver_core::runtime::AgentState) -> workriver_core::runtime::RuntimeAgent {
    workriver_core::runtime::RuntimeAgent {
        session: session.into(),
        harness: "claude".into(),
        state,
        cwd: String::new(),
        group: Some(workriver_core::RuntimeRef { id: group.into(), label: "chat".into() }),
        name: name.into(),
        topic: None,
    }
}

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
    let store = workriver_core::Store::open(&tmp.path().join("w.db")).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::serve::bind(0)).unwrap();
    let d = Daemon { port: listener.local_addr().unwrap().port() };
    let runtime = fake.clone();
    std::thread::spawn(move || rt.block_on(crate::serve::serve(listener, store, runtime, None, Default::default())));

    let here = d.call("/api/here", Some(json!({ "place": "p1", "intent": true }))).unwrap();
    assert_eq!((here["actor"].as_str(), here["intent"].as_str()), (Some("s-boss"), Some("i1")), "{here}");
    // The workspace got a new id but kept its label: the same intent follows it.
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w9", Idle)), ("p2".into(), live("s-rev", "rev", "w9", Idle))];
    let again = d.call("/api/here", Some(json!({ "place": "p2", "intent": true }))).unwrap();
    assert_eq!((again["actor"].as_str(), again["intent"].as_str()), (Some("s-rev"), Some("i1")));

    d.append("s-boss", "manager.registered", json!({ "intent": "i1", "agent": "s-boss" })).unwrap();
    d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "Mentions", "repo": repo.to_string_lossy(), "branch": "f", "base": "main" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "review", "agent": "rev" })).unwrap();
    d.append("s-rev", "task.delivered", json!({ "unit": "u1", "task": 1, "findings": [{ "severity": "P1", "problem": "p", "impact": "i", "where": ["a"] }] })).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a poll sees the agents
    d.call("/api/decide", Some(json!({ "unit": "u1", "task": 1, "finding": "P1-1", "decision": "accept" }))).unwrap();

    let sent = fake.sent.lock().unwrap().clone();
    // First the review's delivery, told to the manager; then the user's decision, to the manager too.
    assert_eq!(sent.len(), 2);
    assert!(sent.iter().all(|(to, _)| to == "s-boss"));
    assert!(sent[0].1.starts_with("[workriver] u1#1 (code review) delivered by rev."), "{}", sent[0].1);
    assert!(sent[0].1.contains("Findings: P1 1 (waiting for the user's decision)"), "{}", sent[0].1);
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

/// A message delivered before a restart is still marked picked up afterwards.
#[test]
fn pick_up_survives_a_restart() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let db = tmp.path().join("w.db");
    let start = |fake: std::sync::Arc<Fake>| {
        let store = workriver_core::Store::open(&db).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let listener = rt.block_on(crate::serve::bind(0)).unwrap();
        let d = Daemon { port: listener.local_addr().unwrap().port() };
        std::thread::spawn(move || rt.block_on(crate::serve::serve(listener, store, fake, None, Default::default())));
        d
    };
    // The first daemon delivers to a manager that is busy all along: no pick-up seen.
    let busy = std::sync::Arc::new(Fake::default());
    *busy.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Working))];
    let a = start(busy.clone());
    a.append("user", "intent.created", json!({ "title": "Chat" })).unwrap();
    a.append("s-boss", "manager.registered", json!({ "intent": "i1", "agent": "s-boss" })).unwrap();
    a.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "M", "repo": repo.to_string_lossy(), "branch": "f", "base": "main" })).unwrap();
    a.append("s-rev", "task.created", json!({ "unit": "u1", "kind": "review" })).unwrap();
    a.append("s-rev", "task.delivered", json!({ "unit": "u1", "task": 1, "findings": [{ "severity": "P1", "problem": "p", "impact": "i", "where": ["a"] }] })).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500));
    a.call("/api/decide", Some(json!({ "unit": "u1", "task": 1, "finding": "P1-1", "decision": "accept" }))).unwrap();
    assert_eq!(busy.sent.lock().unwrap().len(), 2, "the review's delivery and the decision");

    // A second daemon on the same records sees the manager go idle, then start working.
    let fresh = std::sync::Arc::new(Fake::default());
    *fresh.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle))];
    let b = start(fresh.clone());
    std::thread::sleep(std::time::Duration::from_millis(2500));
    fresh.agents.lock().unwrap()[0].1.state = Working;
    let step = |d: &Daemon| d.get("/api/workspace").unwrap()["intents"][0]["units"][0]["tasks"][0]["output"]["findings"][0]["delivery"]["step"].clone();
    assert!((0..30).any(|_| {
        std::thread::sleep(std::time::Duration::from_millis(200));
        step(&b) == "picked"
    }));
}

/// Inside a runtime pane with no agent, the CLI says so instead of recording as the user.
#[test]
fn identity_is_explicit_about_a_pane_without_an_agent() {
    use crate::actor_of;
    let none = json!({ "actor": null, "intent": null });
    let err = actor_of(None, Some("w1:p3"), Some(&none)).unwrap_err().to_string();
    assert!(err.contains("Herdr lists no agent in pane w1:p3") && err.contains("--as <name>"), "{err}");
    assert_eq!(actor_of(Some("rev".into()), Some("w1:p3"), Some(&none)).unwrap(), "rev");
    assert_eq!(actor_of(None, Some("w1:p1"), Some(&json!({ "actor": "s-1" }))).unwrap(), "s-1");
    assert_eq!(actor_of(None, None, None).unwrap(), "user");
}

/// Starts a daemon on a free port over a fake runtime with short waits; returns its client.
fn daemon_with(fake: std::sync::Arc<Fake>, db: &std::path::Path, waits: crate::serve::Waits) -> Daemon {
    let store = workriver_core::Store::open(db).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::serve::bind(0)).unwrap();
    let d = Daemon { port: listener.local_addr().unwrap().port() };
    std::thread::spawn(move || rt.block_on(crate::serve::serve(listener, store, fake, None, waits)));
    d
}

fn wait_until(what: &str, mut ok: impl FnMut() -> bool) {
    assert!((0..60).any(|_| ok() || { std::thread::sleep(std::time::Duration::from_millis(200)); false }), "{what}");
}

/// Asking why: a temporary agent starts in the intent's workspace with the task agent's
/// harness, gets the question, answers; a follow-up reuses it; ending the thread closes it;
/// a conclusion starts the following gate's note; an idle thread's agent is closed; an
/// unanswered question times out.
#[test]
fn asking_why() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let fake = std::sync::Arc::new(Fake::default());
    let mut worker = live("s-w", "w", "w1", Idle);
    worker.harness = "codex".into();
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), worker)];
    let waits = crate::serve::Waits { answer: std::time::Duration::from_secs(3), idle: std::time::Duration::from_secs(1) };
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), waits);
    d.call("/api/here", Some(json!({ "place": "p1", "intent": true }))).unwrap();
    d.append("s-boss", "manager.registered", json!({ "intent": "i1", "agent": "s-boss" })).unwrap();
    d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "M", "repo": repo.to_string_lossy(), "branch": "f", "base": "main" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "design", "title": "Design the mentions", "agent": "s-w" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "gate", "gate": "decide", "title": "Confirm the design", "inputs": [{ "from": "1" }] })).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a poll sees the agents

    let asked = d.call("/api/ask", Some(json!({ "unit": "u1", "task": 1, "text": "Why one table?", "context": [{ "kind": "file", "label": "a.rs", "path": "src/a.rs" }] }))).unwrap();
    assert_eq!(asked["body"]["thread"], "q1");
    wait_until("the agent is started and asked", || !fake.sent.lock().unwrap().is_empty());
    let started = fake.started.lock().unwrap().clone();
    assert_eq!((started[0].0.as_str(), started[0].1.as_str(), started[0].2.as_str()), ("w1", "codex", "why-q1"), "the intent's workspace, the task agent's harness");
    let (to, prompt) = fake.sent.lock().unwrap()[0].clone();
    assert_eq!(to, "why-q1");
    assert!(prompt.contains("\"Why one table?\"") && prompt.contains("the file src/a.rs") && prompt.contains("workriver answer q1 --text"), "{prompt}");
    assert!(!prompt.contains('\n'));
    // Someone else may not answer; the temporary agent may.
    assert!(d.append("s-boss", "question.answered", json!({ "question": "q1", "text": "x" })).is_err());
    d.append("s-why-q1", "question.answered", json!({ "question": "q1", "text": "Fewer joins.", "asked": "s-w" })).unwrap();

    // A follow-up goes to the same agent.
    d.call("/api/ask", Some(json!({ "unit": "u1", "task": 1, "thread": "q1", "text": "And later?" }))).unwrap();
    wait_until("the follow-up is sent", || fake.sent.lock().unwrap().len() == 2);
    assert_eq!(fake.started.lock().unwrap().len(), 1, "no second agent");
    assert!(fake.sent.lock().unwrap()[1].1.contains("Follow-up question q2: \"And later?\""));
    d.append("s-why-q1", "question.answered", json!({ "question": "q2", "text": "Then split." })).unwrap();
    // The conclusion goes to the same agent and starts the decide gate's note.
    d.call("/api/ask", Some(json!({ "unit": "u1", "task": 1, "thread": "q1", "text": "Draft a conclusion of this discussion.", "conclude": true }))).unwrap();
    wait_until("the conclusion is asked for", || fake.sent.lock().unwrap().len() == 3);
    assert!(fake.sent.lock().unwrap()[2].1.contains("workriver answer q3 --conclusion"));
    assert!(d.append("s-why-q1", "question.answered", json!({ "question": "q3", "text": "No." })).unwrap_err().to_string().contains("--conclusion"));
    d.append("s-why-q1", "question.answered", json!({ "question": "q3", "text": "One table now; split when it grows.", "conclusion": true })).unwrap();
    let ws = d.get("/api/workspace").unwrap();
    let tasks = &ws["intents"][0]["units"][0]["tasks"];
    assert_eq!(tasks[1]["draft"], "One table now; split when it grows.");
    assert_eq!(tasks[0]["design"], true);
    let thread = &tasks[0]["threads"][0];
    assert_eq!((thread["turns"].as_array().unwrap().len(), thread["conclusion"].as_str()), (3, Some("One table now; split when it grows.")));
    assert_eq!(thread["turns"][0]["consulted"]["agent"], "w");

    // Idle too long: the agent is closed; the thread stays, and a new question starts a new agent with it.
    wait_until("the idle agent is closed", || fake.closed.lock().unwrap().contains(&"why-q1".to_string()));
    d.call("/api/ask", Some(json!({ "unit": "u1", "task": 1, "thread": "q1", "text": "One more?" }))).unwrap();
    wait_until("a new agent starts", || fake.started.lock().unwrap().len() == 2);
    wait_until("it gets the thread so far", || fake.sent.lock().unwrap().len() == 4);
    let prompt = fake.sent.lock().unwrap()[3].1.clone();
    assert!(prompt.contains("Asked before in this thread: \"Why one table?\" Answered: \"Fewer joins.\""), "{prompt}");
    // No answer in time: it fails and its agent is closed.
    wait_until("the question times out", || {
        d.get("/api/state").unwrap()["questions"][3]["failed"][0] == "timeout"
    });
    wait_until("the late agent is closed", || fake.closed.lock().unwrap().contains(&"why-q4".to_string()));
    // Ending a thread closes its agent.
    d.call("/api/ask", Some(json!({ "unit": "u1", "task": 1, "text": "New topic" }))).unwrap();
    wait_until("a third agent", || fake.started.lock().unwrap().len() == 3);
    d.call("/api/thread/end", Some(json!({ "thread": "q5" }))).unwrap();
    wait_until("ended thread's agent is closed", || fake.closed.lock().unwrap().contains(&"why-q5".to_string()));
    assert!(d.call("/api/ask", Some(json!({ "unit": "u1", "task": 1, "thread": "q5", "text": "?" }))).is_err(), "an ended thread takes no more");
}

/// With no runtime the question fails at once, saying why.
#[test]
fn asking_without_a_runtime() {
    let tmp = tempfile::tempdir().unwrap();
    let store = workriver_core::Store::open(&tmp.path().join("w.db")).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::serve::bind(0)).unwrap();
    let d = Daemon { port: listener.local_addr().unwrap().port() };
    std::thread::spawn(move || rt.block_on(crate::serve::serve(listener, store, std::sync::Arc::new(workriver_core::runtime::NoRuntime), None, Default::default())));
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    d.append("user", "intent.created", json!({ "title": "Chat" })).unwrap();
    d.append("user", "unit.created", json!({ "intent": "i1", "title": "M", "repo": repo.to_string_lossy(), "branch": "f", "base": "main" })).unwrap();
    d.append("user", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "x" })).unwrap();
    d.call("/api/ask", Some(json!({ "unit": "u1", "task": 1, "text": "Why?" }))).unwrap();
    wait_until("it fails as runtime-down", || {
        d.get("/api/workspace").unwrap()["intents"][0]["units"][0]["tasks"][0]["threads"][0]["turns"][0]["failed"]["code"] == "runtime-down"
    });
}

/// A blocked agent shows with when it was first seen blocked, kept across polls, and goes
/// when it is no longer blocked.
#[test]
fn blocked_agents_carry_their_wait() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-a", "a", "w1", Blocked)), ("p2".into(), live("s-b", "b", "w1", Idle))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    let since = |d: &Daemon, k: usize| d.get("/api/workspace").unwrap()["sessions"][k]["blockedSince"].clone();
    wait_until("a poll sees it blocked", || since(&d, 0).is_string());
    let first = since(&d, 0);
    assert!(since(&d, 1).is_null());
    std::thread::sleep(std::time::Duration::from_millis(2500));
    assert_eq!(since(&d, 0), first, "the wait starts when first seen");
    fake.agents.lock().unwrap()[0].1.state = Working;
    wait_until("no longer blocked", || since(&d, 0).is_null());
}

/// Agents ask each other through workriver: a free task agent is asked directly, a busy one
/// is not interrupted (a temporary agent answers); the answer goes back to the asker.
#[test]
fn agents_ask_each_other() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![
        ("p1".into(), live("s-boss", "boss", "w1", Idle)),
        ("p2".into(), live("s-back", "back", "w1", Idle)),
        ("p3".into(), live("s-front", "front", "w1", Working)),
    ];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    d.call("/api/here", Some(json!({ "place": "p1", "intent": true }))).unwrap();
    d.call("/api/here", Some(json!({ "place": "p3", "intent": false }))).unwrap();
    d.append("s-boss", "manager.registered", json!({ "intent": "i1", "agent": "s-boss" })).unwrap();
    d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "M", "repo": repo.to_string_lossy(), "branch": "f", "base": "main" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "API", "agent": "s-back" })).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500));

    // The back end's agent is free: asked directly, no temporary agent.
    let q = d.call("/api/ask", Some(json!({ "by": "s-front", "unit": "u1", "task": 1, "text": "Is the list paged?", "context": [{ "kind": "file", "label": "api.ts", "path": "src/api.ts" }] }))).unwrap();
    assert_eq!(q["actor"], "s-front");
    wait_until("asked directly", || fake.sent.lock().unwrap().iter().any(|(to, _)| to == "s-back"));
    assert!(fake.started.lock().unwrap().is_empty());
    let text = fake.sent.lock().unwrap().iter().find(|(to, _)| to == "s-back").unwrap().1.clone();
    assert!(text.contains("The agent front asks about your task u1#1 \"API\" (question q1)") && text.contains("workriver answer q1"), "{text}");
    // Another agent may not answer; the task's own agent may.
    assert!(d.append("s-boss", "question.answered", json!({ "question": "q1", "text": "x" })).is_err());
    d.append("s-back", "question.answered", json!({ "question": "q1", "text": "Yes, 50 per page." })).unwrap();
    wait_until("the answer goes back to the asker", || fake.sent.lock().unwrap().iter().any(|(to, t)| to == "s-front" && t.contains("Answer to your question q1") && t.contains("50 per page")));
    let ws = d.get("/api/workspace").unwrap();
    assert_eq!(ws["intents"][0]["units"][0]["tasks"][0]["threads"][0]["turns"][0]["by"], "front", "shown with the asker");
    assert!(fake.closed.lock().unwrap().is_empty(), "the task's own agent is never closed");

    // Busy now: a temporary agent answers instead, and the answer still goes back.
    fake.agents.lock().unwrap()[1].1.state = Working;
    std::thread::sleep(std::time::Duration::from_millis(2500));
    d.call("/api/ask", Some(json!({ "by": "s-front", "unit": "u1", "task": 1, "text": "And sorted how?" }))).unwrap();
    wait_until("a temporary agent starts", || fake.started.lock().unwrap().len() == 1);
    assert!(!fake.sent.lock().unwrap().iter().any(|(to, t)| to == "s-back" && t.contains("q2")), "the busy agent is not interrupted");
    d.append("s-why-q2", "question.answered", json!({ "question": "q2", "text": "By date." })).unwrap();
    wait_until("answer back", || fake.sent.lock().unwrap().iter().any(|(to, t)| to == "s-front" && t.contains("q2") && t.contains("By date.")));

    // --wait gives up with a clear message when nobody answers in time.
    d.call("/api/ask", Some(json!({ "by": "s-front", "unit": "u1", "task": 1, "text": "Unanswered?" }))).unwrap();
    let err = crate::wait_answer(&d, "q3", std::time::Duration::from_secs(1)).unwrap_err().to_string();
    assert!(err.contains("no answer to q3 within 1 s"), "{err}");
}
