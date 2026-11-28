// A hosted agent as the service's rules see it, over the fake runtime that stands for both
// sides (its place `acp:a1/1`): who drives it, read anew at every request; a session recorded
// as hosted, or reported by a host whose start is not recorded yet, is nobody at another place.
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use workriver_core::ports::WorkStore;
use workriver_core::runtime::AgentState::Idle;
use workriver_core::{Body, Principal};

use super::support::*;
use crate::cli::Daemon;

/// ws1 "chat" and ws2 "docs" of one project, and the hosted agent a1 of ws1, started as
/// session `s-h` (recorded before the service starts: only the start action reserves one).
fn recorded(db: &std::path::Path) {
    recorded_with_root(db, None);
}

fn recorded_with_root(db: &std::path::Path, root: Option<workriver_core::Shell>) {
    let mut store = opened(db);
    let repo = db.parent().unwrap().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let mut add = |who: &Principal, v: serde_json::Value| {
        store.record(who, serde_json::from_value::<Body>(v).unwrap()).unwrap();
    };
    add(&Principal::User, json!({"kind": "project.added", "body": {"path": repo.to_string_lossy(), "title": "Repo"}}));
    add(&Principal::User, json!({"kind": "workspace.created", "body": {"name": "chat", "project": "p1"}}));
    add(&Principal::User, json!({"kind": "workspace.created", "body": {"name": "docs", "project": "p1"}}));
    add(&Principal::User, json!({"kind": "agent.start_reserved", "body": {"workspace": "ws1", "name": "w", "harness": "codex", "cwd": "/p", "auth": "chatgpt"}}));
    let host = root.unwrap_or(workriver_core::Shell { pid: 9, start: 1 });
    add(&Principal::Service, json!({"kind": "agent.started", "body": {"agent": "a1", "session": "s-h", "host": host, "generation": 1}}));
}

fn service(fake: std::sync::Arc<Fake>, db: &std::path::Path, hosts: std::sync::Arc<crate::runtime::Hosts>) -> Daemon {
    service_with(fake, db, hosts, Default::default())
}

fn service_with(fake: std::sync::Arc<Fake>, db: &std::path::Path, hosts: std::sync::Arc<crate::runtime::Hosts>, waits: crate::server::Waits) -> Daemon {
    service_over(fake, db, hosts, waits, Box::new(opened(db)))
}

fn service_over(fake: Arc<Fake>, db: &Path, hosts: Arc<crate::runtime::Hosts>, waits: crate::server::Waits, store: Box<dyn WorkStore>) -> Daemon {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let home = db.parent().unwrap().to_owned();
    let d = kernel.client(listener.local_addr().unwrap().port(), &home);
    let options = crate::server::Options { allow_as: true, hosts: Some(hosts), waits, ..kernel.options() };
    std::thread::spawn(move || rt.block_on(crate::server::serve_over(listener, store, home, fake, options)));
    wait_until("the service", || http(d.port, "GET", "/api/status", &[], None).0 == 200);
    d
}

fn setup() -> (tempfile::TempDir, std::sync::Arc<Fake>, std::sync::Arc<crate::runtime::Hosts>, Daemon) {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("w.db");
    recorded(&db);
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![
        ("acp:a1/1".into(), live("s-h", "w", "w0", Idle)),
        ("w1:p1".into(), live("s-boss", "boss", "w1", Idle)),
        ("w1:p2".into(), live("s-other", "other", "w1", Idle)),
    ];
    let hosts = crate::runtime::Hosts::new(tmp.path());
    let user = service(fake.clone(), &db, hosts.clone());
    seat(&user, "ws1", Some("s-boss")).unwrap();
    seat(&user, "ws2", Some("s-other")).unwrap();
    (tmp, fake, hosts, user)
}

/// 8.1: the user drives any hosted agent; a workspace's seated manager its own workers while
/// they hold no seat; nobody else; only the user answers. Every request reads the seats anew.
#[test]
fn a_manager_drives_its_own_workers_and_loses_one_seated_elsewhere() {
    let (_tmp, fake, _hosts, user) = setup();
    let prompt = json!({"text": "do u1#3"});
    let code = |place: &str, method: &str, path: &str, body: Option<&serde_json::Value>| from_place(&user, place, method, path, body);
    let (c, body) = code("w1:p1", "POST", "/api/hosted/w/prompt", Some(&prompt));
    assert_eq!(c, 200, "{body}");
    assert!(fake.typed_at.lock().unwrap().iter().any(|(p, t)| p == "acp:a1/1" && t == "From \"boss\" (your manager): do u1#3"), "{:?}", fake.typed_at.lock().unwrap());
    for (place, path) in [("w1:p2", "/api/hosted/w/prompt"), ("w1:p2", "/api/hosted/w/cancel"), ("acp:a1/1", "/api/hosted/w/prompt")] {
        assert_eq!(code(place, "POST", path, Some(&prompt)).0, 403, "{place} {path}");
    }
    for path in ["/api/hosted/w", "/api/hosted/w/read", "/api/hosted/w/status"] {
        assert_eq!(code("w1:p2", "GET", path, None).0, 403, "{path}");
        assert_ne!(code("w1:p1", "GET", path, None).0, 403, "{path}");
    }
    // A manager never answers or closes; the user does.
    assert_eq!(code("w1:p1", "POST", "/api/hosted/w/answer", Some(&json!({"request": 1, "decision": "allow"}))).0, 403);
    assert_eq!(code("w1:p1", "POST", "/api/hosted/w/close", Some(&json!({}))).0, 403);
    assert_eq!(as_user(&user, "POST", "/api/hosted/w/prompt", Some(&prompt)).0, 200);
    // The user seats the worker as ws2's manager: ws1's manager drives it no more.
    seat(&user, "ws2", Some("s-h")).unwrap();
    for (method, path) in [("POST", "/api/hosted/w/prompt"), ("GET", "/api/hosted/w/read"), ("POST", "/api/hosted/w/cancel")] {
        assert_eq!(code("w1:p1", method, path, Some(&prompt)).0, 403, "{path}");
    }
    assert_eq!(as_user(&user, "POST", "/api/hosted/w/prompt", Some(&prompt)).0, 200);
    // The seat given back: its workspace's manager may act again.
    seat(&user, "ws2", Some("s-other")).unwrap();
    assert_eq!(code("w1:p1", "POST", "/api/hosted/w/prompt", Some(&prompt)).0, 200);
    // The list: the user's whole; a manager's its own workers; another's none.
    let listed = |place: &str| from_place(&user, place, "GET", "/api/hosted", None).1;
    assert!(listed("w1:p1").contains("\"id\":\"a1\""));
    assert!(!listed("w1:p2").contains("\"id\":\"a1\""));
}

/// 3.6: a session recorded as hosted is that agent only at its own host's place; one a
/// starting host reported is nobody at another place before its start is recorded.
#[test]
fn a_hosted_session_is_nobody_at_another_place() {
    let (_tmp, fake, hosts, user) = setup();
    let here = |place: &str| from_place(&user, place, "POST", "/api/here", Some(&json!({})));
    let (c, body) = here("acp:a1/1");
    assert!(c == 200 && body.contains("\"actor\":\"s-h\""), "{body}");
    fake.agents.lock().unwrap().push(("w1:p9".into(), live("s-h", "w", "w1", Idle)));
    let (c, body) = here("w1:p9");
    assert!(c == 403 && body.contains("the hosted agent a1"), "{body}");
    // A start in the window between its host's answer and its record.
    let me = crate::common::process::one(std::process::id()).unwrap();
    hosts.track("a2", me.pid, me.start);
    hosts.pending("a2", "s-new");
    fake.agents.lock().unwrap().push(("w1:p3".into(), live("s-new", "another", "w1", Idle)));
    let (c, body) = here("w1:p3");
    assert!(c == 403 && body.contains("the hosted agent a2"), "{body}");
    // Nothing was recorded for it, so its start can still be.
    assert!(bodies(&_tmp.path().join("w.db"), "agent.seen").iter().all(|b| b["session"] != "s-new"));
}

#[test]
fn a_prompt_to_a_stopped_host_never_goes_to_a_terminal() {
    let (_tmp, fake, _hosts, user) = setup();
    let mut agents = fake.agents.lock().unwrap();
    agents.retain(|(place, _)| place != "acp:a1/1");
    agents.push(("w1:p9".into(), live("s-h", "w", "w1", Idle)));
    drop(agents);

    let (status, body) = from_place(&user, "w1:p1", "POST", "/api/hosted/w/prompt", Some(&json!({"text": "worker instructions"})));
    assert_eq!(status, 503, "{body}");
    assert!(body.contains("stopped"), "{body}");
    assert!(fake.typed_at.lock().unwrap().iter().all(|(place, _)| place != "w1:p9"));
}

#[test]
fn process_routes_wait_for_initial_host_reattachment() {
    use std::sync::{Condvar, Mutex, mpsc};

    let dir = tempfile::Builder::new().prefix("wr-reattach-").tempdir_in("/tmp").unwrap();
    let db = dir.path().join("w.db");
    recorded(&db);
    let me = crate::common::process::one(std::process::id()).unwrap();
    {
        let mut store = opened(&db);
        let mut add = |who: &Principal, v: Value| { store.record(who, serde_json::from_value::<Body>(v).unwrap()).unwrap(); };
        add(&Principal::User, json!({"kind":"agent.start_reserved", "body": {"workspace":"ws1", "name":"live", "harness":"codex", "cwd":"/p", "auth":"chatgpt"}}));
        add(&Principal::Service, json!({"kind":"agent.started", "body": {"agent":"a2", "session":"s-reattach", "host":{"pid":me.pid,"start":me.start}, "generation":1}}));
    }
    let (called, hello) = mpsc::channel();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let go = release.clone();
    scripted_host(&dir.path().join("agents/a2"), move |req| match req["method"].as_str() {
        Some("hello") => {
            let _ = called.send(());
            let (lock, notify) = &*go;
            let _result = notify.wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(8), |released| !*released).unwrap();
            Ok(json!({"protocolVersion":1, "generation":1, "state":"running", "session":"s-reattach"}))
        }
        _ => Err("no"),
    });
    let user = service(Arc::new(Fake::default()), &db, crate::runtime::Hosts::new(dir.path()));
    hello.recv_timeout(Duration::from_secs(4)).unwrap();
    let listed = as_user(&user, "GET", "/api/hosted/processes", None);
    let closed = as_user(&user, "POST", "/api/hosted/processes/close", Some(&json!({"all":true})));
    let (lock, notify) = &*release;
    *lock.lock().unwrap() = true;
    notify.notify_all();
    for (status, body) in [listed, closed] {
        assert!(status == 503 && body.contains("still re-attaching"), "{status} {body}");
    }
    wait_until("startup reconciliation completes", || as_user(&user, "GET", "/api/hosted/processes", None).0 == 200);
}

fn with_active_unit(user: &Daemon, root: &std::path::Path) {
    let repo = root.join("repo");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    user.append("", "intent.created", json!({"title": "Docs", "state": "planned"})).unwrap();
    user.append("", "intent.state_changed", json!({"intent": "i1", "state": "active", "workspace": "ws1"})).unwrap();
    user.append("", "unit.created", json!({"template": "bare", "intent": "i1", "title": "Docs", "repo": repo, "branch": "feat/docs", "base": "main"})).unwrap();
}

#[test]
fn a_manager_who_becomes_blind_cannot_read_the_next_hosted_page() {
    let (tmp, _fake, hosts, user) = setup();
    journal(&hosts.dir("a1"), &[
        json!({"kind": "host", "event": "first"}),
        json!({"kind": "host", "event": "second"}),
    ]);
    let read = |after| from_place(&user, "w1:p1", "GET", &format!("/api/hosted/a1/read?after={after}&limit=1"), None);
    let (status, first) = read(0);
    assert_eq!(status, 200, "{first}");
    assert!(first.contains("first") && !first.contains("second"), "{first}");
    with_active_unit(&user, tmp.path());
    user.append("", "task.created", json!({"unit": "u1", "kind": "review", "title": "Check docs", "agent": "s-boss", "blind": true})).unwrap();
    let (status, second) = read(1);
    assert_eq!(status, 403, "{second}");
    assert!(!second.contains("entries"), "{second}");
    assert_eq!(from_place(&user, "w1:p1", "GET", "/api/hosted/a1/status", None).0, 403);
    assert!(as_user(&user, "GET", "/api/hosted/a1/read?after=1&limit=1", None).1.contains("second"));
}

fn connected_host(handle: impl Fn(&Value) -> Option<Value> + Send + Sync + 'static) -> (tempfile::TempDir, Daemon) {
    use std::sync::Arc;
    let tmp = tempfile::Builder::new().prefix("wr-hc-").tempdir_in("/tmp").unwrap();
    let db = tmp.path().join("w.db");
    let me = crate::common::process::one(std::process::id()).unwrap();
    recorded_with_root(&db, Some(workriver_core::Shell { pid: me.pid, start: me.start }));
    let fake = Arc::new(Fake::default());
    fake.agents.lock().unwrap().push(("w1:p1".into(), live("s-boss", "boss", "w1", Idle)));
    let hosts = crate::runtime::Hosts::new(tmp.path());
    scripted_host(&hosts.dir("a1"), move |req| match req["method"].as_str() {
        Some("hello") => Ok(json!({"protocolVersion": 1, "id": "a1", "generation": 1, "state": "running", "session": "s-h"})),
        Some("status") => Ok(json!({"generation": 1, "state": "idle", "head": 0, "pending": {}})),
        _ => handle(req).ok_or("disconnect"),
    });
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let user = kernel.client(listener.local_addr().unwrap().port(), tmp.path());
    let runtime = Arc::new(crate::runtime::Composite::new(fake, hosts.clone()));
    let options = crate::server::Options { allow_as: true, hosts: Some(hosts), ..kernel.options() };
    let store = opened(&db);
    std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, runtime, options)));
    wait_until("the host is attached", || user.get("/api/hosted/a1").is_ok_and(|shown| shown["state"] == "idle"));
    seat(&user, "ws1", Some("s-boss")).unwrap();
    (tmp, user)
}

#[test]
fn reseating_does_not_wait_for_the_cancel_reply() {
    use std::sync::{Mutex, mpsc};
    let (entered, cancelling) = mpsc::channel();
    let (finish, release) = mpsc::channel();
    let release = Mutex::new(release);
    let (_tmp, user) = connected_host(move |req| {
        if req["method"] == "cancel" {
            entered.send(()).unwrap();
            release.lock().unwrap().recv_timeout(Duration::from_secs(5)).unwrap();
            Some(json!({"state": "cancelled"}))
        } else {
            Some(json!({}))
        }
    });
    let old = user.clone();
    let cancel = std::thread::spawn(move || from_place(&old, "w1:p1", "POST", "/api/hosted/a1/cancel", Some(&json!({}))));
    cancelling.recv_timeout(Duration::from_secs(5)).unwrap();

    let prompting = user.clone();
    let (prompted, prompt_result) = mpsc::channel();
    let prompt = std::thread::spawn(move || {
        let _ = prompted.send(from_place(&prompting, "w1:p1", "POST", "/api/hosted/a1/prompt", Some(&json!({"text":"next"}))));
    });
    let prompted_while_cancelling = prompt_result.recv_timeout(Duration::from_secs(2));

    let seating = user.clone();
    let (seated, done) = mpsc::channel();
    let reseat = std::thread::spawn(move || {
        let _ = seated.send(seating.call("/api/workspaces", Some(json!({
            "action": "manager", "workspace": "ws2", "manager": "s-h", "replaces": null,
        }))));
    });
    let seated_while_cancelling = done.recv_timeout(Duration::from_secs(2));
    finish.send(()).unwrap();
    let (code, out) = cancel.join().unwrap();
    assert_eq!(code, 200, "{out}");
    assert_eq!(serde_json::from_str::<Value>(&out).unwrap()["state"], "cancelled");
    prompt.join().unwrap();
    reseat.join().unwrap();
    let (code, out) = prompted_while_cancelling.expect("prompting the cancelling worker waited for the cancel reply");
    assert_eq!(code, 409, "{out}");
    seated_while_cancelling.expect("reseating waited for the cancel reply").unwrap();
    assert_eq!(from_place(&user, "w1:p1", "POST", "/api/hosted/a1/cancel", Some(&json!({}))).0, 403);
}

#[test]
fn a_held_delivery_reuses_its_key_after_the_host_loses_the_reply() {
    use std::sync::{Arc, Mutex};
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let received = prompts.clone();
    let (tmp, user) = connected_host(move |req| {
        if req["method"] != "prompt" || !req["params"]["text"].as_str().unwrap().contains("Include the evidence") {
            return Some(json!({"seq": 1}));
        }
        let mut received = received.lock().unwrap();
        received.push(req["params"].clone());
        (received.len() > 1).then(|| json!({"seq": 1, "repeat": true}))
    });
    with_active_unit(&user, tmp.path());
    user.append("", "task.created", json!({"unit": "u1", "kind": "implement", "title": "Docs", "agent": "s-h"})).unwrap();
    user.append("s-h", "task.started", json!({"unit": "u1", "task": 1})).unwrap();
    let note = user.append("", "task.noted", json!({"unit": "u1", "task": 1, "text": "Include the evidence"})).unwrap();
    assert!(note["told"].as_str().unwrap().contains("not told yet"), "{note}");
    wait_until("the held delivery is retried", || prompts.lock().unwrap().len() >= 2);
    let prompts = prompts.lock().unwrap();
    assert_eq!(prompts.len(), 2);
    assert_eq!(prompts[0], prompts[1]);
    assert!(prompts[0]["key"].as_str().is_some_and(|s| !s.is_empty()));
}

/// An instance directory with a key, and a stand-in for its host's socket: `hello` answers as
/// a host of protocol `protocol`; a `close` is written down in `close-asked`.
fn stand_in(dir: &std::path::Path, protocol: u64) {
    let path = dir.to_owned();
    scripted_host(dir, move |req| match req["method"].as_str() {
        Some("hello") if protocol != 1 => Err("incompatible"),
        Some("hello") => Ok(json!({"protocolVersion": 1, "generation": 1, "state": "running", "session": "s-test"})),
        Some("close") => {
            std::fs::write(path.join("close-asked"), "").unwrap();
            Ok(json!({"accepted": true}))
        }
        _ => Err("no"),
    });
}

fn scripted_host(dir: &Path, respond: impl Fn(&Value) -> Result<Value, &'static str> + Sync + Send + 'static) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("host.key"), [7u8; 32]).unwrap();
    script_host_socket(dir, respond);
}

fn script_host_socket(dir: &Path, respond: impl Fn(&Value) -> Result<Value, &'static str> + Sync + Send + 'static) {
    use std::io::{BufRead, Write};
    let listener = std::os::unix::net::UnixListener::bind(dir.join("host.sock")).unwrap();
    let respond = Arc::new(respond);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let respond = respond.clone();
            std::thread::spawn(move || {
                let mut lines = std::io::BufReader::new(&stream);
                let mut line = String::new();
                while lines.read_line(&mut line).unwrap_or(0) > 0 {
                    let req: serde_json::Value = serde_json::from_str(&line).unwrap();
                    line.clear();
                    let answer = match respond(&req) {
                        Ok(value) => json!({"jsonrpc": "2.0", "id": req["id"], "result": value}),
                        Err("disconnect") => break,
                        Err(why) => json!({"jsonrpc": "2.0", "id": req["id"], "error": {"code": -32000, "message": why}}),
                    };
                    let _ = (&stream).write_all(format!("{answer}\n").as_bytes());
                }
            });
        }
    });
}

fn journal(dir: &std::path::Path, lines: &[serde_json::Value]) {
    std::fs::create_dir_all(dir).unwrap();
    let text: String = lines.iter().enumerate().map(|(i, l)| {
        let mut l = l.clone();
        l["seq"] = json!(i + 1);
        l["gen"] = json!(1);
        l["at"] = json!("2026-01-01T00:00:00Z");
        format!("{l}\n")
    }).collect();
    std::fs::write(dir.join("journal.jsonl"), text).unwrap();
}

/// 3.8 and 6.3: what each record and host comes to, after one restart. A host gone with a
/// `closed` line: the close recorded, after the answer its receipt lost. Gone without one:
/// stopped, its answers recorded, kept open. A reservation a stopped service left, no host:
/// ended. One whose host lives on and answers nothing: kept starting, never ended or
/// published. A live host of another protocol: not attached, shown incompatible. A live host
/// no reservation is for: asked to close, never adopted.
#[test]
fn the_reconciliation_brings_records_and_hosts_together() {
    let tmp = tempfile::Builder::new().prefix("wr-rec-").tempdir_in("/tmp").unwrap();
    let db = tmp.path().join("w.db");
    let me = crate::common::process::one(std::process::id()).unwrap();
    {
        let mut store = opened(&db);
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let mut add = |who: &Principal, v: serde_json::Value| {
            store.record(who, serde_json::from_value::<Body>(v).unwrap()).unwrap();
        };
        add(&Principal::User, json!({"kind": "project.added", "body": {"path": repo.to_string_lossy(), "title": "Repo"}}));
        add(&Principal::User, json!({"kind": "workspace.created", "body": {"name": "chat", "project": "p1"}}));
        for name in ["closed", "left", "other", "stopped", "hung"] {
            add(&Principal::User, json!({"kind": "agent.start_reserved", "body": {"workspace": "ws1", "name": name, "harness": "codex", "cwd": "/p", "auth": "chatgpt"}}));
        }
        let gone = json!({"pid": 9, "start": 1});
        let live = json!({"pid": me.pid, "start": me.start});
        for (agent, session, host) in [("a1", "s-1", &gone), ("a3", "s-3", &live), ("a4", "s-4", &gone)] {
            add(&Principal::Service, json!({"kind": "agent.started", "body": {"agent": agent, "session": session, "host": host, "generation": 1}}));
        }
    }
    let agents = tmp.path().join("agents");
    let decided = |request: u64, kind: &str| json!({"kind": "host", "event": "decided", "request": request, "optionKind": kind, "by": "user"});
    let closed = json!({"outcome":"unverifiable", "markedError":"the process scan failed", "processes":[{"pid": 12, "outcome":"unverifiable"}], "pids":[12]});
    let mut close_row = closed.clone();
    close_row["kind"] = json!("host");
    close_row["event"] = json!("closed");
    journal(&agents.join("a1"), &[decided(5, "allow_once"), close_row]);
    journal(&agents.join("a4"), &[decided(7, "reject_once"), json!({"kind": "host", "event": "exited", "outcome": "ended"})]);
    stand_in(&agents.join("a3"), 2);
    stand_in(&agents.join("a9"), 1);
    std::fs::write(agents.join("a9/instance.json"), json!({"id": "a9", "host": {"pid": me.pid, "start": me.start}}).to_string()).unwrap();
    // A starting host that answers nothing and does not go.
    std::fs::create_dir_all(agents.join("a5")).unwrap();
    std::fs::write(agents.join("a5/instance.json"), json!({"id": "a5", "host": {"pid": me.pid, "start": me.start}}).to_string()).unwrap();

    let waits = crate::server::Waits { hosted_close: Duration::from_secs(1), hosted_start: Duration::ZERO, ..Default::default() };
    let user = service_with(std::sync::Arc::new(Fake::default()), &db, crate::runtime::Hosts::new(tmp.path()), waits);
    wait_until("the close is recorded", || bodies(&db, "agent.closed").len() == 1);
    let mut event = closed;
    event["agent"] = json!("a1");
    assert_eq!(bodies(&db, "agent.closed")[0], event);
    wait_until("the lost answers are recorded", || bodies(&db, "agent.answered").len() == 2);
    let answered = bodies(&db, "agent.answered");
    assert!(answered.contains(&json!({"agent": "a1", "request": 5, "decision": "allow", "by": "user"})) && answered.contains(&json!({"agent": "a4", "request": 7, "decision": "reject", "by": "user"})), "{answered:?}");
    wait_until("the left reservation ends", || bodies(&db, "agent.start_ended").len() == 1);
    assert_eq!(bodies(&db, "agent.start_ended")[0], json!({"agent": "a2", "why": "the service stopped before the start finished"}));
    let state = |agent: &str| serde_json::from_str::<serde_json::Value>(&as_user(&user, "GET", &format!("/api/hosted/{agent}"), None).1).unwrap()["state"].clone();
    wait_until("the other protocol's host is told apart", || state("a3") == "incompatible");
    wait_until("a host gone without a close is stopped", || state("a4") == "stopped");
    wait_until("the unreserved host is asked to close", || agents.join("a9/close-asked").exists());
    assert!(!agents.join("a3/close-asked").exists(), "an incompatible host is left to the user");
    // A start whose host does not go is not ended: its name stays taken, it is never published.
    assert_eq!(state("a5"), "starting");
    assert_eq!(as_user(&user, "POST", "/api/settings/hosted", Some(&json!({"on": true}))).0, 200);
    let (c, body) = as_user(&user, "POST", "/api/hosted/start", Some(&json!({"workspace": "ws1", "name": "hung", "cwd": "/", "auth": "chatgpt"})));
    assert!(c == 422 && body.contains("hung is a5 already"), "{c} {body}");
    // A second round changes nothing: nothing recorded twice.
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert_eq!((bodies(&db, "agent.closed").len(), bodies(&db, "agent.answered").len(), bodies(&db, "agent.start_ended").len()), (1, 2, 1));
}

#[test]
fn a_reserved_start_gets_its_receipt_if_the_later_read_fails() {
    let dir = tempfile::Builder::new().prefix("wr-start-").tempdir_in("/tmp").unwrap();
    let db = dir.path().join("w.db");
    recorded(&db);
    let faults = Arc::new(Faults::default());
    *faults.down_after.lock().unwrap() = Some("agent.start_reserved".into());
    let hosts = crate::runtime::Hosts::new(dir.path());
    let user = service_over(Arc::new(Fake::default()), &db, hosts, Default::default(), Box::new(Flaky::new(&db, faults)));
    assert_eq!(as_user(&user, "POST", "/api/settings/hosted", Some(&json!({"on": true}))).0, 200);

    let (status, receipt) = as_user(&user, "POST", "/api/hosted/start", Some(&json!({
        "workspace": "ws1", "name": "new", "cwd": dir.path(), "auth": "chatgpt",
    })));
    assert_eq!(status, 200, "{receipt}");
    assert_eq!(serde_json::from_str::<Value>(&receipt).unwrap(), json!({"agent":"a2","state":"starting"}));
    wait_until("the failed read ends the reservation", || !bodies(&db, "agent.start_ended").is_empty());
    assert_eq!(bodies(&db, "agent.start_ended")[0]["agent"], "a2");
}

#[test]
fn a_young_reservation_without_a_host_ends_on_restart() {
    let dir = tempfile::Builder::new().prefix("wr-lost-").tempdir_in("/tmp").unwrap();
    let db = dir.path().join("w.db");
    recorded(&db);
    let mut store = opened(&db);
    store.record(&Principal::User, serde_json::from_value(json!({
        "kind":"agent.start_reserved", "body":{"workspace":"ws1", "name":"new", "harness":"codex", "cwd":dir.path(), "auth":"chatgpt"},
    })).unwrap()).unwrap();
    drop(store);

    let _service = service(Arc::new(Fake::default()), &db, crate::runtime::Hosts::new(dir.path()));
    wait_until("the reservation ends without waiting for readiness", || !bodies(&db, "agent.start_ended").is_empty());
    assert_eq!(bodies(&db, "agent.start_ended")[0], json!({"agent":"a2", "why":"the service stopped before the start finished"}));
}

#[test]
fn a_transient_receipt_write_cannot_put_the_close_before_an_answer() {
    let dir = tempfile::Builder::new().prefix("wr-answer-").tempdir_in("/tmp").unwrap();
    let db = dir.path().join("w.db");
    recorded(&db);
    let mut store = opened(&db);
    store.record(&Principal::Service, serde_json::from_value(json!({
        "kind":"agent.answered", "body":{"agent":"a1","request":9,"decision":"reject","by":"user"},
    })).unwrap()).unwrap();
    drop(store);
    journal(&dir.path().join("agents/a1"), &[
        json!({"kind":"host","event":"decided","request":5,"optionKind":"allow_once","by":"user"}),
        json!({"kind":"host","event":"decided","request":9,"optionKind":"reject_once","by":"user"}),
        json!({"kind":"host","event":"closed","outcome":"ended"}),
    ]);
    let faults = Arc::new(Faults::default());
    *faults.fail_write.lock().unwrap() = Some("agent.answered");
    let waits = crate::server::Waits { tick: Duration::from_millis(80), ..Default::default() };
    let _service = service_over(Arc::new(Fake::default()), &db, crate::runtime::Hosts::new(dir.path()), waits, Box::new(Flaky::new(&db, faults)));

    wait_until("the close is recorded after retry", || !bodies(&db, "agent.closed").is_empty());
    let events: Vec<Value> = stored(&db).into_iter().map(|e| serde_json::to_value(e).unwrap())
        .filter(|v| matches!(v["kind"].as_str(), Some("agent.answered" | "agent.closed"))).collect();
    assert_eq!(events.len(), 3, "{events:?}");
    assert_eq!(events[0]["body"]["request"], 9);
    assert_eq!(events[1]["body"], json!({"agent":"a1", "request":5, "decision":"allow", "by":"user"}));
    assert_eq!(events[2]["kind"], "agent.closed");
}

#[test]
fn an_accepted_close_without_a_journal_receipt_becomes_stopped() {
    let dir = tempfile::Builder::new().prefix("wr-cut-").tempdir_in("/tmp").unwrap();
    let db = dir.path().join("w.db");
    recorded(&db);
    let mut host = HostLifetime::new();
    let process = host.root();
    let mut store = opened(&db);
    for (who, event) in [
        (Principal::User, json!({"kind":"agent.start_reserved","body":{"workspace":"ws1","name":"new","harness":"codex","cwd":dir.path(),"auth":"chatgpt"}})),
        (Principal::Service, json!({"kind":"agent.started","body":{"agent":"a2","session":"s-child","generation":1,"host":{"pid":process.pid,"start":process.start}}})),
    ] {
        store.record(&who, serde_json::from_value(event).unwrap()).unwrap();
    }
    drop(store);
    stand_in(&dir.path().join("agents/a2"), 1);
    let hosts = crate::runtime::Hosts::new(dir.path());
    let user = service_with(Arc::new(Fake::default()), &db, hosts.clone(), crate::server::Waits { tick: Duration::from_millis(80), ..Default::default() });
    wait_until("the host is known", || hosts.alive("a2"));
    let mut reply = None;
    wait_until("the close is accepted", || {
        let result = as_user(&user, "POST", "/api/hosted/a2/close", Some(&json!({})));
        if result.1.contains("try again in a moment") { return false }
        reply = Some(result);
        true
    });
    let (status, receipt) = reply.unwrap();
    host.end();
    assert_eq!(status, 200, "{receipt}");
    assert_eq!(serde_json::from_str::<Value>(&receipt).unwrap()["state"], "closing");
    wait_until("the absent host is stopped", || as_user(&user, "GET", "/api/hosted/a2", None).1.contains("\"state\":\"stopped\""));
    assert!(bodies(&db, "agent.closed").is_empty());
}

struct HostLifetime(std::process::Child);

impl HostLifetime {
    fn new() -> Self {
        Self(std::process::Command::new("/bin/sleep").arg("30").spawn().unwrap())
    }

    fn root(&self) -> workriver_core::Shell {
        let process = crate::common::process::one(self.0.id()).unwrap();
        workriver_core::Shell { pid: process.pid, start: process.start }
    }

    fn end(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for HostLifetime {
    fn drop(&mut self) {
        self.end();
    }
}

#[test]
fn a_live_host_reconciles_the_answer_whose_acknowledgement_was_lost() {
    use std::io::Write;
    use workriver_core::runtime::Runtime;

    let dir = tempfile::Builder::new().prefix("wr-reply-").tempdir_in("/tmp").unwrap();
    let db = dir.path().join("w.db");
    let host = HostLifetime::new();
    recorded_with_root(&db, Some(host.root()));
    let path = dir.path().join("agents/a1");
    let log = path.join("journal.jsonl");
    scripted_host(&path, move |req| match req["method"].as_str() {
        Some("hello") => Ok(json!({"protocolVersion":1, "generation":1, "state":"running", "session":"s-h"})),
        Some("answer") => {
            let request = req["params"]["request"].as_u64().unwrap();
            let (seq, option) = match request { 5 => (1, "allow_once"), 9 => (2, "reject_once"), _ => panic!("{req}") };
            let row = json!({"seq":seq, "gen":1, "at":"2026-01-01T00:00:00Z", "kind":"host", "event":"decided",
                "request":request, "optionKind":option, "by":"user"});
            let mut journal = std::fs::OpenOptions::new().create(true).append(true).open(&log).unwrap();
            writeln!(journal, "{row}").unwrap();
            if request == 5 { Err("disconnect") } else { Ok(json!({"replied":true})) }
        }
        _ => Err("no"),
    });
    let hosts = crate::runtime::Hosts::new(dir.path());
    let user = service(Arc::new(Fake::default()), &db, hosts.clone());
    wait_until("the agent has attached", || hosts.roots(None, None).unwrap().iter().any(|r| r.at.agent.is_some()));
    let answer = |request, decision| as_user(&user, "POST", "/api/hosted/a1/answer", Some(&json!({"request":request, "decision":decision})));
    assert_eq!(answer(5, "allow").0, 503);
    assert_eq!(answer(9, "reject").0, 200);
    wait_until("both answers are recorded after reconnect", || bodies(&db, "agent.answered").len() == 2);
    let answers = bodies(&db, "agent.answered");
    for (request, decision) in [(5, "allow"), (9, "reject")] {
        assert!(answers.contains(&json!({"agent":"a1","request":request,"decision":decision,"by":"user"})), "{answers:?}");
    }
    assert!(hosts.alive("a1"));
}

#[test]
fn a_new_start_requests_close_at_its_deadline_and_ends_after_the_host_exits() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::Instant;
    use workriver_core::runtime::Runtime;

    let dir = tempfile::Builder::new().prefix("wr-deadline-").tempdir_in("/tmp").unwrap();
    let db = dir.path().join("w.db");
    recorded(&db);
    let hold = dir.path().join("hold-host");
    std::fs::write(&hold, "").unwrap();
    let program = dir.path().join("fake-host");
    std::fs::write(&program, r#"#!/bin/sh
n=0
while [ "$n" -lt 200 ] && [ -f ../../hold-host ]; do
    n=$((n + 1))
    /bin/sleep 0.05
done
"#).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (tx, close) = std::sync::mpsc::channel();
    let hosts = crate::runtime::Hosts::with_host_program(dir.path(), move |path| {
        let tx = tx.clone();
        script_host_socket(path, move |req| match req["method"].as_str() {
            Some("hello") => Ok(json!({"protocolVersion":1,"generation":2,"state":"starting","session":null})),
            Some("close") => {
                let _ = tx.send((Instant::now(), req["params"]["generation"].clone()));
                Ok(json!({"accepted":true}))
            }
            _ => Err("no"),
        });
        program.clone()
    });
    let ready = Duration::from_secs(1);
    let tick = Duration::from_millis(50);
    let waits = crate::server::Waits { hosted_start: ready, tick, ..Default::default() };
    let user = service_with(Arc::new(Fake::default()), &db, hosts.clone(), waits);
    assert_eq!(as_user(&user, "POST", "/api/settings/hosted", Some(&json!({"on": true}))).0, 200);

    let requested = Instant::now();
    let (status, receipt) = as_user(&user, "POST", "/api/hosted/start", Some(&json!({
        "workspace":"ws1", "name":"new", "cwd":dir.path(), "auth":"chatgpt",
    })));
    assert_eq!(status, 200, "{receipt}");
    assert_eq!(serde_json::from_str::<Value>(&receipt).unwrap(), json!({"agent":"a2","state":"starting"}));
    wait_until("the new host is tracked", || hosts.alive("a2"));
    let pid = hosts.roots(None, Some("acp:a2/1")).unwrap()[0].shell.unwrap();
    let (asked, generation) = close.recv_timeout(Duration::from_secs(3)).expect("the start asks the host to close");
    assert!(asked.duration_since(requested) >= ready);
    // Only the in-flight start uses the generation reported by hello. Recovery uses 1.
    assert_eq!(generation, 2);
    std::thread::sleep(tick * 4);
    assert!(crate::common::process::one(pid).is_some());
    assert!(bodies(&db, "agent.start_ended").is_empty(), "the reservation ended while the host was running");

    std::fs::remove_file(hold).unwrap();
    wait_until("the reservation ends after the host exits", || !bodies(&db, "agent.start_ended").is_empty());
    assert!(crate::common::process::one(pid).is_none());
    assert_eq!(bodies(&db, "agent.start_ended"), [json!({"agent":"a2","why":"did not start in time"})]);
    assert_eq!(bodies(&db, "agent.started").len(), 1);
}

#[test]
fn restart_waits_for_a_young_reachable_start_and_closes_it_at_expiry() {
    use workriver_core::runtime::Runtime;

    let dir = tempfile::Builder::new().prefix("wr-wait-").tempdir_in("/tmp").unwrap();
    let db = dir.path().join("w.db");
    recorded(&db);
    let mut store = opened(&db);
    store.record(&Principal::User, serde_json::from_value(json!({
        "kind":"agent.start_reserved", "body":{"workspace":"ws1", "name":"new", "harness":"codex", "cwd":dir.path(), "auth":"chatgpt"},
    })).unwrap()).unwrap();
    drop(store);

    let mut host = HostLifetime::new();
    let path = dir.path().join("agents/a2");
    let (tx, events) = std::sync::mpsc::channel();
    scripted_host(&path, move |req| match req["method"].as_str() {
        Some("hello") => {
            let _ = tx.send("hello");
            Ok(json!({"protocolVersion":1,"generation":1,"state":"starting","session":null}))
        }
        Some("close") => {
            let _ = tx.send("close");
            Ok(json!({"accepted":true}))
        }
        _ => Err("no"),
    });
    std::fs::write(path.join("instance.json"), json!({"id":"a2", "host":host.root(), "generation":1}).to_string()).unwrap();
    let hosts = crate::runtime::Hosts::new(dir.path());
    let waits = crate::server::Waits { hosted_start: Duration::from_secs(3), tick: Duration::from_millis(100), ..Default::default() };
    let user = service_with(Arc::new(Fake::default()), &db, hosts.clone(), waits);
    assert_eq!(events.recv_timeout(Duration::from_secs(2)).unwrap(), "hello");
    let (status, body) = as_user(&user, "GET", "/api/hosted/a2", None);
    assert!(status == 200 && body.contains("\"state\":\"starting\""), "{status} {body}");
    assert!(bodies(&db, "agent.start_ended").is_empty());
    let until = std::time::Instant::now() + Duration::from_secs(5);
    while events.recv_timeout(until.saturating_duration_since(std::time::Instant::now())).unwrap() != "close" {}
    assert!(hosts.roots(None, None).unwrap().is_empty(), "instance.json must not publish a root");
    host.end();
    wait_until("the expired reservation ends after close", || !bodies(&db, "agent.start_ended").is_empty());
    assert_eq!(bodies(&db, "agent.start_ended")[0], json!({"agent":"a2", "why":"did not start in time"}));
    assert_eq!(bodies(&db, "agent.started").len(), 1);
}

#[test]
fn restart_closes_a_ready_but_unrecorded_host_without_adopting_its_root() {
    use workriver_core::runtime::Runtime;

    let dir = tempfile::Builder::new().prefix("wr-ready-").tempdir_in("/tmp").unwrap();
    let db = dir.path().join("w.db");
    recorded(&db);
    let mut store = opened(&db);
    store.record(&Principal::User, serde_json::from_value(json!({
        "kind":"agent.start_reserved", "body":{"workspace":"ws1", "name":"new", "harness":"codex", "cwd":dir.path(), "auth":"chatgpt"},
    })).unwrap()).unwrap();
    drop(store);

    let mut host = HostLifetime::new();
    let path = dir.path().join("agents/a2");
    stand_in(&path, 1);
    std::fs::write(path.join("instance.json"), json!({"id":"a2", "host":host.root(), "generation":1}).to_string()).unwrap();
    let hosts = crate::runtime::Hosts::new(dir.path());
    let _service = service(Arc::new(Fake::default()), &db, hosts.clone());
    wait_until("the ready host is asked to close before its start deadline", || path.join("close-asked").exists());
    assert!(hosts.roots(None, None).unwrap().is_empty(), "instance.json must not publish a root");
    host.end();
    wait_until("the reservation of the closed host ends", || !bodies(&db, "agent.start_ended").is_empty());
    assert_eq!(bodies(&db, "agent.start_ended")[0], json!({"agent":"a2", "why":"the service stopped before the start finished"}));
    assert_eq!(bodies(&db, "agent.started").len(), 1);
}
