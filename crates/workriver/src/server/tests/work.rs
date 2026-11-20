// Recording work through the service: tasks, findings, what is told to whom, and the
// replay of a copied database.
use std::io::{Read, Write};

use serde_json::json;

use super::support::*;
use crate::cli::Daemon;

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
    git(&repo, &["switch", "-q", "-c", "feat/feature"]);
    std::fs::write(repo.join("a.txt"), "one\ntwo\n").unwrap();
    git(&repo, &["commit", "-q", "-am", "two\n\nTask: u1#1"]);

    let store = workriver_data::Store::open(&tmp.path().join("w.db")).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let port = listener.local_addr().unwrap().port();
    let options = crate::server::Options { allow_as: true, ..kernel.options() };
    std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, std::sync::Arc::new(workriver_core::runtime::NoRuntime), options)));

    let d = kernel.client(port, tmp.path());
    d.append("user", "intent.created", json!({ "title": "Chat" })).unwrap();
    let repo_dir = repo.clone();
    let repo = repo.to_string_lossy();
    d.append("user", "unit.created", json!({ "intent": "i1", "title": "Mentions", "repo": repo, "branch": "feat/feature", "base": "main" })).unwrap();
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
    assert_eq!(unit["tasks"][1]["output"]["findings"][0]["state"], "deferred");
    // The page's snapshot: each file with how many lines changed, not its texts, and the
    // repository's tree; the texts come per unit, with the same `code` to tell when to read again.
    let f = &unit["commits"][0]["files"][0];
    assert_eq!((f["path"].as_str(), f["status"].as_str(), f["add"].as_u64(), f["del"].as_u64(), f.get("before"), f.get("after")), (Some("a.txt"), Some("M"), Some(1), Some(0), None, None), "{f}");
    assert_eq!(ws["repos"][0]["modules"]["children"][0]["path"], "a.txt");
    let code = d.get("/api/unit/u1/code").unwrap();
    assert_eq!((code["code"].as_str(), code["commits"][0]["id"].as_str()), (unit["code"].as_str(), unit["commits"][0]["id"].as_str()));
    assert!(unit["code"].as_str().is_some_and(|c| !c.is_empty()));
    let f = &code["commits"][0]["files"][0];
    assert_eq!((f["before"].as_str(), f["after"].as_str(), f["add"].as_u64()), (Some("one\n"), Some("one\ntwo\n"), Some(1)), "{f}");
    assert!(d.get("/api/unit/u9/code").unwrap_err().to_string().contains("no unit u9"));
    // Uncommitted work: in the snapshot by name and size, its texts with the code, and a new
    // `code` once it changes.
    std::fs::write(repo_dir.join("b.txt"), "new\n").unwrap();
    let again = d.get("/api/workspace").unwrap();
    let unit2 = &again["intents"][0]["units"][0];
    let live = unit2["commits"].as_array().unwrap().iter().find(|c| c["live"] == true).expect("uncommitted work");
    assert_eq!((live["files"][0]["path"].as_str(), live["files"][0]["add"].as_u64(), live["files"][0].get("after")), (Some("b.txt"), Some(1), None));
    assert_ne!(unit2["code"], unit["code"]);
    let code = d.get("/api/unit/u1/code").unwrap();
    assert_eq!(code["commits"][1]["files"][0]["after"], "new\n");
    // Commits the reader has are not sent again; uncommitted work always is.
    let have = code["commits"][0]["id"].as_str().unwrap();
    let code = d.get(&format!("/api/unit/u1/code?have={have}")).unwrap();
    assert_eq!((code["commits"].as_array().unwrap().len(), code["commits"][0]["id"].as_str()), (1, Some("uncommitted")), "{code}");
    // A light read (the CLI's lists) names the files but carries none of their texts, even
    // once a full read has cached them.
    let light = d.get("/api/workspace?light=true").unwrap();
    let f = &light["intents"][0]["units"][0]["commits"][0]["files"][0];
    assert_eq!((f["path"].as_str(), f["after"].is_null(), f["before"].is_null()), (Some("a.txt"), true, true), "{f}");

    // The built page (when there is one): its hashed files are kept for good, the page
    // itself, which names them, is asked for every time.
    let (status, page) = http(port, "GET", "/", &[], None);
    if status == 200 {
        assert!(page.to_lowercase().contains("cache-control: no-cache"), "{page}");
        let script = page.split("src=\"").nth(1).and_then(|s| s.split('"').next()).expect("the page names its script");
        let (status, js) = http(port, "GET", script, &[], None);
        assert!(status == 200 && js.to_lowercase().contains("cache-control: public, max-age=31536000, immutable"), "{script}");
        // A unit's, an intent's or a task's own link is the same page; it names its files
        // from the root, so they load from a nested path too.
        assert!(script.starts_with('/'), "{script}");
        for path in ["/u1", "/u1/3", "/i1", "/i1/1", "/u1/3?finding=P2-1"] {
            let (status, same) = http(port, "GET", path, &[], None);
            assert!(status == 200 && same.to_lowercase().contains("cache-control: no-cache") && same.contains(script), "{path}");
        }
    }
    // Unknown API paths stay a 404, not the page.
    let (status, text) = http(port, "GET", "/api/nope", &[], None);
    assert!(status == 404 && !text.contains("<html"), "{text}");
    // Links for the user: the service's own address, whoever asks.
    let page = format!("http://127.0.0.1:{port}");
    assert_eq!(d.get("/api/status").unwrap()["url"], page.as_str());
    let shown: crate::common::wire::TaskShow = serde_json::from_value(d.get("/api/task?task=u1%232").unwrap()).unwrap();
    assert_eq!(shown.link, format!("{page}/u1/2"));
    assert!(crate::cli::text(&shown).contains(&format!("\nLink: {page}/u1/2\n")));
    let board = crate::cli::board_text(&d, None).unwrap();
    assert!(board.contains(&format!("i1 Chat\n  link: {page}/i1\n")) && board.contains(&format!("\n    link: {page}/u1\n")), "{board}");
    // Another Host is turned away, whatever the path.
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(s, "GET /api/workspace HTTP/1.1\r\nHost: evil.example:{port}\r\nConnection: close\r\n\r\n").unwrap();
    let mut answer = String::new();
    s.read_to_string(&mut answer).unwrap();
    assert!(answer.starts_with("HTTP/1.1 403"), "{answer}");
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
        let store = workriver_data::Store::open(&db).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let listener = rt.block_on(crate::server::bind(0)).unwrap();
        let kernel = Kernel::default();
        let d = kernel.client(listener.local_addr().unwrap().port(), db.parent().unwrap());
        let options = crate::server::Options { allow_as: true, ..kernel.options() };
        std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, fake, options)));
        d
    };
    // The first daemon delivers to a manager that is busy all along: no pick-up seen.
    let busy = std::sync::Arc::new(Fake::default());
    *busy.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Working))];
    let a = start(busy.clone());
    a.append("user", "intent.created", json!({ "title": "Chat" })).unwrap();
    a.append("s-boss", "manager.registered", json!({ "intent": "i1", "agent": "s-boss" })).unwrap();
    a.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "M", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
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

/// What a previous run left without its wait is put right at the next start, in this order:
/// a question still open is failed as restarted, a message that was held for its manager is
/// recorded as not delivered, and the question's temporary agent is closed.
#[test]
fn what_a_previous_run_left_is_closed_at_the_next_start() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let db = tmp.path().join("w.db");
    let start = |fake: std::sync::Arc<Fake>| daemon_with(fake, &db, crate::server::Waits::default());
    // What is stored from event `from` on, as "kind by actor" with why it failed, if it says.
    let since = |from: usize| {
        let line = |e: &workriver_core::Event| {
            let v = serde_json::to_value(e).unwrap();
            let why = v["body"]["reason_code"].as_str().map(|c| format!(" ({c})")).unwrap_or_default();
            format!("{} by {}{why}", v["kind"].as_str().unwrap(), e.actor)
        };
        stored(&db)[from..].iter().map(line).collect::<Vec<_>>()
    };

    // The first run: a question whose new agent waits for a person, and a message held for
    // a manager that does too. Neither got anywhere.
    let first = std::sync::Arc::new(Fake::default());
    *first.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle))];
    let a = start(first.clone());
    manage(&a, "p1", "s-boss");
    a.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "M", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    a.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "API" })).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500));
    *first.start_as.lock().unwrap() = Some(Blocked);
    a.call("/api/ask", Some(json!({ "unit": "u1", "task": 1, "text": "Why paged?" }))).unwrap();
    wait_until("the question has its agent", || since(0).contains(&"question.started by workriver".to_string()));
    first.agents.lock().unwrap()[0].1.state = Blocked;
    a.call("/api/message", Some(json!({ "intent": "i1", "text": "Still there?" }))).unwrap();
    let left = since(0);
    assert!(!left.iter().any(|l| l.starts_with("question.failed") || l.starts_with("message.delivery")), "nothing came of either: {left:?}");
    let mark = left.len();

    // The next run, on the same records.
    let next = std::sync::Arc::new(Fake::default());
    *next.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle))];
    let _b = start(next.clone());
    wait_until("both are recorded", || since(mark).len() >= 2);
    assert_eq!(since(mark)[..2], ["question.failed by workriver (restarted)", "message.delivery by workriver (restarted)"]);
    wait_until("the question's agent is closed", || next.closed.lock().unwrap().iter().any(|n| n == "why-q1"));
}

/// The real store, whose reads fail while `down` is set: a storage that cannot be read.
struct Flaky {
    inner: workriver_data::Store,
    down: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl workriver_core::ports::WorkStore for Flaky {
    fn recorded(&self) -> Result<workriver_core::ports::Recorded, workriver_core::ports::DataError> {
        if self.down.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(workriver_core::ports::DataError::Unavailable("the disk is gone".into()));
        }
        self.inner.recorded()
    }
    fn record(&mut self, who: &workriver_core::Principal, request: workriver_core::Body) -> Result<workriver_core::ports::Committed, workriver_core::ports::DataError> {
        self.inner.record(who, request)
    }
    fn history(&self, after: workriver_core::ports::Revision, limit: usize) -> Result<workriver_core::ports::History, workriver_core::ports::DataError> {
        self.inner.history(after, limit)
    }
    fn status(&self) -> Result<workriver_core::ports::DataStatus, workriver_core::ports::DataError> {
        self.inner.status()
    }
}

/// A store that cannot be read is the service's error, not the caller's: a request is
/// answered 500 with the storage's words (never 422, never "no task"), the poll and what
/// follows a record skip their step, and everything works again once the store does.
#[test]
fn a_store_that_cannot_be_read_is_the_services_error() {
    use std::sync::atomic::Ordering::SeqCst;
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let db = tmp.path().join("w.db");
    let down = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle))];
    let store = Flaky { inner: workriver_data::Store::open(&db).unwrap(), down: down.clone() };
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let port = listener.local_addr().unwrap().port();
    let user = kernel.client(port, tmp.path());
    let (options, runtime, home) = (crate::server::Options { allow_as: true, ..kernel.options() }, fake.clone(), tmp.path().to_path_buf());
    std::thread::spawn(move || rt.block_on(crate::server::serve_over(listener, Box::new(store), home, runtime, options)));
    manage(&user, "p1", "s-boss");
    user.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "M", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    user.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "API" })).unwrap();
    // As the manager from its place, with the status of each answer.
    let ask = |method: &str, path: &str, body: Option<&str>| {
        let (status, answer) = http_at(&user, port, "p1", method, path, &[("content-type", "application/json")], body);
        (status, answer.split("\r\n\r\n").nth(1).unwrap_or_default().to_string())
    };
    assert_eq!(ask("POST", "/api/attachments/inbox", Some(r#"{"unit":"u1","task":9}"#)).0, 422, "a task that is not there is the caller's error");

    down.store(true, SeqCst);
    // The whole state, one record by id, and a record whose completion reads a unit.
    assert_eq!(ask("GET", "/api/state", None), (500, "the disk is gone".into()));
    assert_eq!(ask("POST", "/api/attachments/inbox", Some(r#"{"unit":"u1","task":1}"#)), (500, "the disk is gone".into()));
    assert_eq!(ask("POST", "/api/attachments/inbox", Some(r#"{"unit":"u1","task":9}"#)), (500, "the disk is gone".into()), "not: no such task");
    assert_eq!(ask("POST", "/api/append", Some(r#"{"kind":"unit.merged","body":{"unit":"u1"}}"#)), (500, "the disk is gone".into()));
    // A record still commits (writing works); what follows it and the poll skip their reads.
    let mark = stored(&db).len();
    user.append("user", "intent.created", json!({ "title": "Later", "state": "planned" })).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a poll, with nothing to read
    assert_eq!(stored(&db).len(), mark + 1);

    down.store(false, SeqCst);
    assert_eq!(ask("GET", "/api/state", None).0, 200);
    assert_eq!(ask("POST", "/api/attachments/inbox", Some(r#"{"unit":"u1","task":1}"#)).0, 200);
    user.append("s-boss", "task.started", json!({ "unit": "u1", "task": 1 })).unwrap();
}

/// A copy of a real database (WORKRIVER_REPLAY_DB) replays onto the current model. The
/// copy is made into a temp folder; the given file is only read.
#[test]
#[ignore = "needs WORKRIVER_REPLAY_DB"]
fn replays_a_copied_database() {
    use workriver_core::ports::WorkStore;
    let from = std::env::var("WORKRIVER_REPLAY_DB").expect("WORKRIVER_REPLAY_DB");
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("w.db");
    std::fs::copy(&from, &db).unwrap();
    let store = workriver_data::Store::open(&db).unwrap();
    let s = store.state();
    for i in &s.intents {
        println!("{} {:?} {:?} project={:?} workspace={:?} manager={:?}", i.id, i.title, i.state, i.project, i.workspace.as_ref().map(|g| &g.label), s.manager_of(&i.id));
    }
    for w in &s.workspaces {
        println!("workspace {} ({}) manager={:?} current={:?}", w.group.label, w.group.id, w.manager, w.current);
    }
    for u in &s.units {
        for t in &u.tasks {
            assert!(!t.intent.is_empty(), "{}#{} has its intent", u.id, t.n);
            let f: Vec<String> = t.findings.iter().map(|f| format!("{}={}", f.found.id, workriver_core::name(&f.state))).collect();
            println!("{}#{} {} {:?} intent={} findings={f:?} delivery={:?}", u.id, t.n, workriver_core::name(&t.kind), t.title, t.intent, t.delivery.as_ref().map(|d| workriver_core::name(&d.step)));
        }
    }
    let mut cache = crate::server::views::Cache::default();
    let snap = serde_json::to_value(crate::server::views::build(s, &mut cache, None, &Default::default(), &Default::default(), &Default::default(), &[])).unwrap();
    println!("projects: {}", snap["projects"]);
    println!("events: {}", store.status().unwrap().events);
    // The whole replayed state, for comparing a copy before and after a change byte for byte
    // (docs/design/restructure.md, "Replaying a copy of the real database").
    if let Ok(out) = std::env::var("WORKRIVER_REPLAY_OUT") {
        let runs: Vec<String> = s.runs.iter().map(|r| format!("{r:?}")).collect();
        let all = json!({ "events": store.status().unwrap().events, "state": s, "runs": runs });
        std::fs::write(&out, serde_json::to_string_pretty(&all).unwrap() + "\n").unwrap();
    }
}

/// Findings after the fact: the manager decides by default; the reviewer adds to a decided
/// finding and the agents of the work it led to hear of it; new findings join the review;
/// the user changes the manager's decision; with no manager running, findings fall to the user.
#[test]
fn findings_after_the_fact() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-rev", "rev", "w1", Idle)), ("p3".into(), live("s-fix", "fix", "w1", Idle))];
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
    let (boss, rev, fix) = (at("p1"), at("p2"), at("p3"));
    let group = json!({ "id": "w1", "label": "chat" });
    user.append("", "intent.created", json!({ "title": "Login", "state": "planned" })).unwrap();
    boss.append("", "manager.registered", json!({ "workspace": group })).unwrap();
    boss.append("", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": group })).unwrap();
    boss.append("", "unit.created", json!({ "intent": "i1", "title": "Auth", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Check tokens", "agent": "s-fix" })).unwrap();
    rev.append("", "task.created", json!({ "unit": "u1", "kind": "review", "inputs": [{ "from": "1" }] })).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a poll sees the agents
    rev.append("", "task.delivered", json!({ "unit": "u1", "task": 2, "findings": [{ "severity": "P1", "problem": "Token never expires", "impact": "Stolen tokens work forever", "where": ["auth.rs"] }] })).unwrap();
    let finding = |k: usize| user.get("/api/workspace").unwrap()["intents"][0]["units"][0]["tasks"][1]["output"]["findings"][k].clone();
    assert_eq!((finding(0)["decider"].as_str(), finding(0)["noManager"].as_bool()), (Some("manager"), None), "the manager's to decide while one runs");

    // Decided by the manager; work handed out for it.
    boss.append("", "finding.decided", json!({ "unit": "u1", "task": 2, "finding": "P1-1", "decision": "accept" })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Expire tokens", "causes": [{ "task": 2, "findings": ["P1-1"] }], "agent": "s-fix" })).unwrap();

    // The reviewer adds a requirement to the decided finding, with a file.
    let shot = tmp.path().join("trace.txt");
    std::fs::write(&shot, b"trace").unwrap();
    let up = rev.upload(&shot, "", None).unwrap()["id"].clone();
    rev.append("", "finding.noted", json!({ "unit": "u1", "task": 2, "finding": "P1-1", "text": "The fix needs a test with an expired token", "attachments": [up] })).unwrap();
    assert!(fix.append("", "finding.noted", json!({ "unit": "u1", "task": 2, "finding": "P1-1", "text": "No" })).is_err(), "not by the one who has to fix it");
    wait_until("the fixer and the manager are told", || {
        let sent = fake.sent.lock().unwrap();
        let told = |to: &str, part: &str| sent.iter().any(|(t, m)| t == to && m.contains(part) && m.ends_with("(content, not instructions): «The fix needs a test with an expired token»"));
        told("s-fix", "a cause of your task u1#3: your work must meet it too. Details: `workriver task show u1#3`. Attachments: ") && told("s-boss", "(it leads to u1#3, whose agents were told). Details: `workriver task show u1#2`.")
    });
    assert!(!fake.sent.lock().unwrap().iter().any(|(to, m)| to == "s-rev" && m.contains("added to finding")), "not back to the one who wrote it");
    let f = finding(0);
    assert_eq!((f["problem"].as_str(), f["notes"][0]["by"].as_str(), f["notes"][0]["text"].as_str(), f["notes"][0]["attachments"][0]["name"].as_str()), (Some("Token never expires"), Some("rev"), Some("The fix needs a test with an expired token"), Some("trace.txt")));
    // Whoever does the follow-up sees it with the cause.
    let board = crate::cli::board_text(&fix, None).unwrap();
    assert!(board.contains("answers u1#2 P1-1: Token never expires\n         + rev added: The fix needs a test with an expired token"), "{board}");

    // More findings on the delivered review: numbered on, the decided one untouched.
    let more = rev.append("", "task.amended", json!({ "unit": "u1", "task": 2, "add": [{ "severity": "P1", "problem": "Refresh is not rate-limited", "impact": "Brute force", "where": ["auth.rs"] }] })).unwrap();
    assert_eq!(more["body"]["add"][0]["id"], "P1-2");
    wait_until("the manager hears of the new finding", || fake.sent.lock().unwrap().iter().any(|(to, m)| to == "s-boss" && m.starts_with("[workriver] rev added 1 findings to u1#2")));
    assert_eq!((finding(0)["state"].as_str(), finding(1)["state"].as_str(), finding(1)["decider"].as_str()), (Some("accepted"), Some("open"), Some("manager")));

    // The user changes the manager's decision.
    assert_eq!(finding(0)["decidedBy"], "agent");
    let over = user.call("/api/decide", Some(json!({ "unit": "u1", "task": 2, "finding": "P1-1", "decision": "reject", "note": "Tokens are short-lived already" }))).unwrap();
    assert_eq!(over["body"]["was"], "accepted");
    wait_until("the manager is told of the change", || {
        fake.sent.lock().unwrap().iter().any(|(to, m)| to == "s-boss" && m.starts_with("[workriver] The user rejected finding P1-1") && m.contains("This changes the earlier decision (it was accepted)") && m.contains("Work already handed out for it: u1#3 (todo)"))
    });
    assert_eq!((finding(0)["state"].as_str(), finding(0)["decidedBy"].as_str()), (Some("rejected"), Some("user")));

    // The manager is gone: what was its to decide falls to the user.
    fake.agents.lock().unwrap().retain(|(_, a)| a.session != "s-boss");
    wait_until("the open finding falls to the user", || finding(1)["noManager"] == true && finding(1)["decider"].is_null());
}

/// What follows a record keeps its order, at each of the three places the service writes.
/// A request: the event is stored before anyone is told of it, the delivery is stored after
/// the message was taken, and the caller is answered after both. What a kind brings with it
/// (a message to a manager is handed over) comes after its own record. The poll: a pick-up
/// is stored once the receiver is seen working. A refused request is followed by nothing.
#[test]
fn what_follows_a_record_keeps_its_order() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let db = tmp.path().join("w.db");
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-rev", "rev", "w1", Idle))];
    let (user, kernel) = daemon_and(fake.clone(), &db, crate::server::Waits::default());
    let at = |place: &str| kernel.daemon(user.port, Some(place.into()), None);
    let (boss, rev) = (at("p1"), at("p2"));
    let group = json!({ "id": "w1", "label": "chat" });
    user.append("", "intent.created", json!({ "title": "Login", "state": "planned" })).unwrap();
    boss.append("", "manager.registered", json!({ "workspace": group })).unwrap();
    boss.append("", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": group })).unwrap();
    boss.append("", "unit.created", json!({ "intent": "i1", "title": "Auth", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Check tokens" })).unwrap();
    rev.append("", "task.created", json!({ "unit": "u1", "kind": "review", "inputs": [{ "from": "1" }] })).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a poll sees the agents
    rev.append("", "task.delivered", json!({ "unit": "u1", "task": 2, "findings": [{ "severity": "P1", "problem": "Token never expires", "impact": "Stolen tokens work forever", "where": ["auth.rs"] }] })).unwrap();

    // What is stored, as "kind by actor" (a delivery with its step), from event `from` on.
    let line = |e: &workriver_core::Event| {
        let v = serde_json::to_value(e).unwrap();
        let step = v["body"]["step"].as_str().map(|s| format!(" ({s})")).unwrap_or_default();
        format!("{} by {}{step}", v["kind"].as_str().unwrap(), e.actor)
    };
    let since = |from: usize| stored(&db)[from..].iter().map(line).collect::<Vec<_>>();
    // Each message as it is taken: who gets it, and the last event stored by then.
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let (log, path) = (seen.clone(), db.clone());
    *fake.on_send.lock().unwrap() = Some(Box::new(move |to| log.lock().unwrap().push(format!("{to} told after {}", line(stored(&path).last().unwrap())))));
    let told = || std::mem::take(&mut *seen.lock().unwrap());

    // A request with a message to deliver: stored, sent, the delivery stored, then answered.
    let mark = stored(&db).len();
    user.call("/api/decide", Some(json!({ "unit": "u1", "task": 2, "finding": "P1-1", "decision": "accept" }))).unwrap();
    assert_eq!(since(mark), ["finding.decided by user", "delivery.updated by workriver (delivered)"], "both are stored when the caller is answered");
    assert_eq!(told(), ["s-boss told after finding.decided by user"]);

    // The poll: the receiver is seen working, so what it was given (the review's delivery
    // before, the decision now) was picked up.
    let mark = stored(&db).len();
    fake.agents.lock().unwrap()[0].1.state = Working;
    wait_until("the pick-ups are stored", || since(mark).len() == 2);
    assert_eq!(since(mark), ["delivery.updated by workriver (picked)", "delivery.updated by workriver (picked)"]);
    assert_eq!(told(), [] as [&str; 0]);
    fake.agents.lock().unwrap()[0].1.state = Idle;

    // What a kind brings with it: the user's message is handed to the manager after its own
    // record, and where it got to is stored after that.
    let mark = stored(&db).len();
    user.call("/api/message", Some(json!({ "intent": "i1", "text": "How is it going?" }))).unwrap();
    assert_eq!(since(mark), ["message.sent by user", "message.delivery by workriver (delivered)"]);
    assert_eq!(told(), ["s-boss told after message.sent by user"]);

    // Refused: nothing stored, nobody told.
    let mark = stored(&db).len();
    assert!(rev.append("", "message.sent", json!({ "intent": "i1", "text": "x" })).is_err());
    assert_eq!((since(mark), told()), (vec![], vec![]));
}

/// A manager that was running before the service was updated is told once, at its next
/// command, to read what changed; one that registers now is not (it just read the guide).
#[test]
fn a_manager_is_told_of_an_update_once() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-w", "w", "w1", Idle))];
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
    let (boss, w) = (at("p1"), at("p2"));
    user.append("", "intent.created", json!({ "title": "A", "state": "planned" })).unwrap();
    boss.append("", "manager.registered", json!({ "workspace": { "id": "w1", "label": "chat" } })).unwrap();
    boss.get("/api/state").unwrap();
    w.get("/api/state").unwrap();
    let updates = || fake.sent.lock().unwrap().iter().filter(|(_, m)| m.contains("workriver was updated")).map(|(to, _)| to.clone()).collect::<Vec<_>>();
    assert!(updates().is_empty(), "a manager that registers now has just read this build's guide");
    // As if the service had been another build when the manager last heard of it.
    let told = tmp.path().join("guide-told.json");
    assert!(std::fs::read_to_string(&told).unwrap().contains(crate::common::BUILD));
    std::fs::write(&told, r#"{"s-boss":"0.0.1 (old)"}"#).unwrap();
    w.get("/api/state").unwrap();
    assert!(updates().is_empty(), "workers are not told: their instructions come with each task");
    boss.get("/api/state").unwrap();
    boss.append("", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": { "id": "w1", "label": "chat" } })).unwrap();
    boss.get("/api/workspace").unwrap();
    assert_eq!(updates(), ["s-boss"], "once per manager per build");
    let said = fake.sent.lock().unwrap().iter().find(|(_, m)| m.contains("workriver was updated")).unwrap().1.clone();
    assert!(said.starts_with("[workriver] workriver was updated") && said.contains(crate::common::BUILD) && said.contains("`workriver guide changes`"), "{said}");
}


/// The user drops a task from the page: the agent on it is told to stop and the manager is
/// told; an agent cannot use the user's route. The board, `task show` and the page show it
/// dropped with why; a dropped gate no longer waits for the user.
#[test]
fn a_dropped_task_is_told_and_shown() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p3".into(), live("s-fix", "fix", "w1", Idle))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), Default::default());
    let at = |place: &str| Daemon { place: Some(place.into()), home: None, ..d.clone() };
    let (boss, fix) = (at("p1"), at("p3"));
    let group = json!({ "id": "w1", "label": "chat" });
    d.append("", "intent.created", json!({ "title": "Login", "state": "planned" })).unwrap();
    boss.append("", "manager.registered", json!({ "workspace": group })).unwrap();
    boss.append("", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": group })).unwrap();
    boss.append("", "unit.created", json!({ "intent": "i1", "title": "Auth", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Check tokens", "agent": "s-fix" })).unwrap();
    fix.append("", "task.started", json!({ "unit": "u1", "task": 1 })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "gate", "gate": "decide", "title": "Merge u1 into main?", "inputs": [{ "from": "1" }] })).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a poll sees the agents

    let drop = |n: u32, reason: &str| json!({ "unit": "u1", "task": n, "reason": reason });
    assert!(fix.call("/api/task/drop", Some(drop(1, "mine"))).is_err(), "the page's route is the user's alone");
    d.call("/api/task/drop", Some(drop(1, "superseded by u1#3"))).unwrap();
    wait_until("the agent and the manager are told", || {
        let sent = fake.sent.lock().unwrap();
        sent.iter().any(|(to, m)| to == "s-fix" && m.starts_with("[workriver] The user dropped your task u1#1") && m.ends_with("The user wrote: «superseded by u1#3»"))
            && sent.iter().any(|(to, m)| to == "s-boss" && m.contains("The user dropped u1#1 \"Check tokens\" as no longer needed"))
    });
    // The manager drops the gate it asked before; it is not told its own act.
    boss.append("", "task.dropped", json!({ "unit": "u1", "task": 2, "reason": "the branch changed; asked again" })).unwrap();
    assert!(!fake.sent.lock().unwrap().iter().any(|(to, m)| to == "s-boss" && m.contains("u1#2")));

    let board = crate::cli::board_text(&boss, None).unwrap();
    assert!(board.contains("#1 dropped  implement Check tokens @fix") && board.contains("       dropped (superseded by u1#3)\n") && board.contains("       dropped (the branch changed; asked again)\n") && !board.contains("waiting for the user"), "{board}");
    let shown = crate::cli::text(&serde_json::from_value(boss.get("/api/task?task=u1%231").unwrap()).unwrap());
    assert!(shown.contains("u1#1  implement  dropped") && shown.contains("Dropped as no longer needed by the user at ") && shown.contains("because: superseded by u1#3"), "{shown}");
    let tasks = d.get("/api/workspace").unwrap()["intents"][0]["units"][0]["tasks"].clone();
    assert_eq!((tasks[1]["state"].as_str(), tasks[1]["dropped"]["by"].as_str(), tasks[1]["dropped"]["reason"].as_str()), (Some("dropped"), Some("boss"), Some("the branch changed; asked again")));
}

/// A worker that stops without delivering: reminded once at the threshold (set by the user
/// only); still idle after the second wait, it is escalated (the task shows it, the manager is
/// told once); working again after the reminder clears it; gone, it is escalated at once;
/// waiting on a gate it asked for, it is left alone.
#[test]
fn stopped_workers_are_reminded_then_escalated() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-w", "ja-impl", "w1", Working))];
    // One minute of the threshold lasts a second here.
    let waits = crate::server::Waits { minute: std::time::Duration::from_secs(1), ..Default::default() };
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), waits);
    manage(&d, "p1", "s-boss");

    // The threshold: ten minutes until the user sets it; only the user does, within bounds.
    assert_eq!(d.get("/api/settings/stall").unwrap()["minutes"], 10);
    let worker = Daemon { place: Some("p2".into()), home: Some(tmp.path().to_path_buf()), ..d.clone() };
    assert!(worker.call("/api/settings/stall", Some(json!({ "minutes": 1 }))).is_err(), "an agent does not set it");
    assert!(d.call("/api/settings/stall", Some(json!({ "minutes": 0 }))).is_err());
    d.call("/api/settings/stall", Some(json!({ "minutes": 2 }))).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(tmp.path().join("settings.json")).unwrap()).unwrap();
    assert_eq!(saved["stallMinutes"], 2);

    d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "M", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    for (title, after) in [("push it", "start"), ("again", "1"), ("deploy", "2")] {
        d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "implement", "title": title, "agent": "ja-impl", "inputs": [{ "from": after }] })).unwrap();
    }
    d.append("s-w", "task.started", json!({ "unit": "u1", "task": 1 })).unwrap();
    let task = |n: usize| d.get("/api/workspace").unwrap()["intents"][0]["units"][0]["tasks"][n].clone();
    let to = |who: &str, what: &str| fake.sent.lock().unwrap().iter().filter(|(t, m)| t == who && m.contains(what)).count();
    let set = |state| fake.agents.lock().unwrap()[1].1.state = state;

    // Idle past the threshold: the worker itself is reminded, once; nobody else hears yet.
    set(Idle);
    wait_until("the reminder", || to("s-w", "Your task u1#1 \"push it\" is still running") == 1);
    let stall = task(0)["stalled"].clone();
    assert!(stall["reminded"].is_string() && stall["escalated"].is_null(), "{stall}");
    assert!(to("s-w", "a hook forbids push") == 1 && to("s-boss", "") == 0);
    // Still idle after the second wait: escalated, the manager told once, the board says so.
    wait_until("escalated", || task(0)["stalled"]["escalated"] == true);
    wait_until("the manager is told", || to("s-boss", "ja-impl stopped") == 1);
    let told = fake.sent.lock().unwrap().iter().find(|(t, _)| t == "s-boss").unwrap().1.clone();
    assert!(told.starts_with("[workriver] ja-impl stopped") && told.contains("herdr agent read ja-impl") && told.contains("workriver task drop u1#1") && told.contains("It was reminded"), "{told}");
    std::thread::sleep(std::time::Duration::from_millis(4500));
    assert_eq!((to("s-w", "Your task"), to("s-boss", "")), (1, 1), "once per stall");
    assert!(crate::cli::board_text(&d, None).unwrap().contains("stalled since"), "the board says it");
    assert!(d.get("/api/task?task=u1%231").unwrap()["stalled"]["escalated"] == true, "task show too");
    // It works again: cleared.
    set(Working);
    wait_until("cleared", || task(0)["stalled"].is_null());

    // Reminded, it goes on working: cleared, never escalated.
    d.append("s-w", "task.delivered", json!({ "unit": "u1", "task": 1, "evidence": [{ "command": "t", "result": "passed", "summary": "ok" }] })).unwrap();
    d.append("s-w", "task.started", json!({ "unit": "u1", "task": 2 })).unwrap();
    set(Idle);
    wait_until("the second reminder", || to("s-w", "Your task u1#2") == 1);
    set(Working);
    wait_until("cleared after working on", || task(1)["stalled"].is_null());
    std::thread::sleep(std::time::Duration::from_millis(3000));
    assert_eq!(to("s-boss", "stopped"), 1, "no escalation");

    // Waiting on a gate it asked for after its task started: left alone.
    set(Idle);
    d.append("s-w", "task.created", json!({ "unit": "u1", "kind": "gate", "gate": "deploy", "title": "Deploy it", "inputs": [{ "from": "1" }] })).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5000));
    assert!(task(1)["stalled"].is_null() && to("s-w", "Your task u1#2") == 1, "{}", task(1));

    // Herdr not answering: unknown, nobody is gone.
    d.append("s-w", "task.delivered", json!({ "unit": "u1", "task": 2, "evidence": [{ "command": "t", "result": "passed", "summary": "ok" }] })).unwrap();
    d.append("s-w", "task.started", json!({ "unit": "u1", "task": 3 })).unwrap();
    fake.down.store(true, std::sync::atomic::Ordering::Relaxed);
    fake.agents.lock().unwrap().pop();
    std::thread::sleep(std::time::Duration::from_millis(4500));
    assert!(task(2)["stalled"].is_null() && to("s-boss", "is gone") == 0, "{}", task(2));
    // Read again, it is missing: gone after a minute of good reads; no one to remind.
    fake.down.store(false, std::sync::atomic::Ordering::Relaxed);
    wait_until("gone", || task(2)["stalled"]["gone"] == true);
    wait_until("the manager hears", || to("s-boss", "is gone") == 1);
    assert_eq!(to("s-w", "Your task u1#3"), 0);
}

/// A worker stuck on what only the user can do puts a blocker before its task: the manager is
/// told which task is stuck and what was asked, quoted; the user's result goes to the worker.
/// A worker reaches its manager with `manager tell`, quoted; the user writes from the page.
#[test]
fn a_stuck_worker_puts_a_blocker_and_tells_its_manager() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-w", "ja-impl", "w1", Working))];
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), crate::server::Waits::default());
    manage(&d, "p1", "s-boss");
    d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "M", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "push it", "agent": "ja-impl" })).unwrap();
    // The worker records from its own pane, as an agent does.
    let worker = Daemon { place: Some("p2".into()), home: Some(tmp.path().to_path_buf()), ..d.clone() };
    worker.append("", "task.started", json!({ "unit": "u1", "task": 1 })).unwrap();
    let to = |who: &str, what: &str| fake.sent.lock().unwrap().iter().filter(|(t, m)| t == who && m.contains(what)).map(|(_, m)| m.clone()).collect::<Vec<_>>();

    worker.append("", "task.created", json!({ "unit": "u1", "kind": "gate", "gate": "test", "title": "Allow git push", "text": "git push was refused: a hook forbids it", "blocks": 1 })).unwrap();
    wait_until("the manager is told", || to("s-boss", "is stuck on").len() == 1);
    let told = to("s-boss", "is stuck on")[0].clone();
    assert!(told.starts_with("[workriver] ja-impl is stuck on u1#1 \"push it\": it put gate u1#2 \"Allow git push\""), "{told}");
    assert!(told.contains("«Allow git push: git push was refused: a hook forbids it»"), "{told}");
    let tasks = d.get("/api/workspace").unwrap()["intents"][0]["units"][0]["tasks"].clone();
    assert_eq!((tasks[1]["blocks"].as_str(), tasks[1]["state"].as_str()), (Some("1"), Some("waiting")), "{}", tasks[1]);

    d.call("/api/gate", Some(json!({ "unit": "u1", "task": 2, "result": "done", "note": "Allowed for this repo" }))).unwrap();
    wait_until("the worker gets the result", || !to("s-w", "u1#2").is_empty());
    assert!(to("s-boss", "finished gate").is_empty(), "the result goes to the stuck worker, not the manager");

    worker.append("", "manager.told", json!({ "unit": "u1", "task": 1, "text": "The branch is now feat/f2" })).unwrap();
    wait_until("the manager hears it", || to("s-boss", "tells you about").len() == 1);
    let told = to("s-boss", "tells you about")[0].clone();
    assert!(told.starts_with("[workriver] ja-impl tells you about u1#1 \"push it\".") && told.ends_with("«The branch is now feat/f2»"), "{told}");
    let e = d.append("user", "manager.told", json!({ "unit": "u1", "task": 1, "text": "x" })).unwrap_err().to_string();
    assert!(e.contains("the user writes to a manager in workriver's page"), "{e}");
}

/// Workers left running with nothing to do: when a unit is merged its manager hears which of
/// its workers are still running with no other task (not the manager, not one with work
/// elsewhere); a worker idle past the threshold with no task is told of once per idle
/// period; one with a task, or never on one (the user's own session), is not.
#[test]
fn idle_workers_are_told_to_their_manager() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    for b in ["feat/a", "feat/b"] {
        git(&repo, &["branch", b]);
    }
    std::fs::write(tmp.path().join("settings.json"), r#"{"idleMinutes": 2}"#).unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = ["boss", "ja-impl", "ja-test", "ja-busy", "me"].iter().enumerate().map(|(k, n)| (format!("p{k}"), live(&format!("s-{n}"), n, "w1", Idle))).collect();
    // One minute lasts a second: the threshold is two seconds here.
    let waits = crate::server::Waits { minute: std::time::Duration::from_secs(1), ..Default::default() };
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), waits);
    manage(&d, "p0", "s-boss");
    for b in ["feat/a", "feat/b"] {
        d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "T", "repo": repo.to_string_lossy(), "branch": b, "base": "main" })).unwrap();
    }
    for (unit, agent, after) in [("u1", "boss", "start"), ("u1", "ja-impl", "1"), ("u1", "ja-test", "2"), ("u1", "ja-busy", "3"), ("u2", "ja-busy", "start")] {
        d.append("s-boss", "task.created", json!({ "unit": unit, "kind": "implement", "title": "Do it", "agent": agent, "inputs": [{ "from": after }] })).unwrap();
    }
    for (n, agent) in [(1, "s-boss"), (2, "s-ja-impl"), (3, "s-ja-test"), (4, "s-ja-busy")] {
        d.append(agent, "task.started", json!({ "unit": "u1", "task": n })).unwrap();
        d.append(agent, "task.delivered", json!({ "unit": "u1", "task": n, "evidence": [{ "command": "t", "result": "passed", "summary": "ok" }] })).unwrap();
    }
    let to_boss = |what: &str| fake.sent.lock().unwrap().iter().filter(|(t, m)| t == "s-boss" && m.contains(what)).count();

    // Merged: one message, the two with nothing else to do.
    d.append("s-boss", "unit.merged", json!({ "unit": "u1" })).unwrap();
    assert_eq!(to_boss("[workriver] u1 is merged: ja-impl, ja-test worked on it and are still running with no other task; close them if not needed"), 1, "{:?}", fake.sent.lock().unwrap());
    assert_eq!(to_boss("is merged"), 1);

    // Idle past the threshold with no task: told once each; not the busy one, the manager, or `me`.
    wait_until("ja-impl", || to_boss("ja-impl has been idle for") == 1);
    wait_until("ja-test", || to_boss("ja-test has been idle for") == 1);
    let told = fake.sent.lock().unwrap().iter().find(|(_, m)| m.contains("ja-impl has been idle")).unwrap().1.clone();
    assert!(told.contains("(its last: u1#2 \"Do it\")") && told.contains("`workriver guide how closing-workers`"), "{told}");
    std::thread::sleep(std::time::Duration::from_millis(4500));
    assert_eq!((to_boss("ja-impl has been idle"), to_boss("ja-test has been idle")), (1, 1), "once per idle period");
    assert_eq!(to_boss("ja-busy has been idle"), 0, "it has a task");
    assert_eq!(fake.sent.lock().unwrap().iter().filter(|(_, m)| m.contains("me has been") || m.contains("boss has been")).count(), 0);
    // Working, then idle again: a new idle period, told again.
    fake.agents.lock().unwrap()[1].1.state = Working;
    std::thread::sleep(std::time::Duration::from_millis(2500));
    fake.agents.lock().unwrap()[1].1.state = Idle;
    wait_until("told again", || to_boss("ja-impl has been idle for") == 2);
}
