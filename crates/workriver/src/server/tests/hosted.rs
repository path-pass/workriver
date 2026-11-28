// A hosted agent as the service's rules see it, over the fake runtime that stands for both
// sides (its place `acp:a1/1`): who drives it, read anew at every request; a session recorded
// as hosted, or reported by a host whose start is not recorded yet, is nobody at another place.
use serde_json::json;
use workriver_core::runtime::AgentState::Idle;
use workriver_core::{Body, Principal};

use super::support::*;
use crate::cli::Daemon;

/// ws1 "chat" and ws2 "docs" of one project, and the hosted agent a1 of ws1, started as
/// session `s-h` (recorded before the service starts: only the start action reserves one).
fn recorded(db: &std::path::Path) {
    use workriver_core::ports::WorkStore;
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
    add(&Principal::Service, json!({"kind": "agent.started", "body": {"agent": "a1", "session": "s-h", "host": {"pid": 9, "start": 1}, "generation": 1}}));
}

fn service(fake: std::sync::Arc<Fake>, db: &std::path::Path, hosts: std::sync::Arc<crate::runtime::Hosts>) -> Daemon {
    service_with(fake, db, hosts, Default::default())
}

fn service_with(fake: std::sync::Arc<Fake>, db: &std::path::Path, hosts: std::sync::Arc<crate::runtime::Hosts>, waits: crate::server::Waits) -> Daemon {
    let store = opened(db);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let d = kernel.client(listener.local_addr().unwrap().port(), db.parent().unwrap());
    let options = crate::server::Options { allow_as: true, hosts: Some(hosts), waits, ..kernel.options() };
    std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, fake, options)));
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

/// An instance directory with a key, and a stand-in for its host's socket: `hello` answers as
/// a host of protocol `protocol`; a `close` is written down in `close-asked`.
fn stand_in(dir: &std::path::Path, protocol: u64) {
    use std::io::{BufRead, Write};
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("host.key"), [7u8; 32]).unwrap();
    let listener = std::os::unix::net::UnixListener::bind(dir.join("host.sock")).unwrap();
    let dir = dir.to_path_buf();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let dir = dir.clone();
            std::thread::spawn(move || {
                let mut lines = std::io::BufReader::new(&stream);
                let mut line = String::new();
                while lines.read_line(&mut line).unwrap_or(0) > 0 {
                    let req: serde_json::Value = serde_json::from_str(&line).unwrap();
                    line.clear();
                    let answer = match req["method"].as_str() {
                        Some("hello") if protocol != 1 => json!({"jsonrpc": "2.0", "id": req["id"], "error": {"code": -32000, "message": "incompatible"}}),
                        Some("hello") => json!({"jsonrpc": "2.0", "id": req["id"], "result": {"protocolVersion": 1, "state": "idle"}}),
                        Some("close") => {
                            std::fs::write(dir.join("close-asked"), "").unwrap();
                            json!({"jsonrpc": "2.0", "id": req["id"], "result": {"accepted": true}})
                        }
                        _ => json!({"jsonrpc": "2.0", "id": req["id"], "error": {"code": -32601, "message": "no"}}),
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
    use workriver_core::ports::WorkStore;
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
    journal(&agents.join("a1"), &[decided(5, "allow_once"), json!({"kind": "host", "event": "closed", "outcome": "ended"})]);
    journal(&agents.join("a4"), &[decided(7, "reject_once"), json!({"kind": "host", "event": "exited", "outcome": "ended"})]);
    stand_in(&agents.join("a3"), 2);
    stand_in(&agents.join("a9"), 1);
    std::fs::write(agents.join("a9/instance.json"), json!({"id": "a9", "host": {"pid": me.pid, "start": me.start}}).to_string()).unwrap();
    // A starting host that answers nothing and does not go.
    std::fs::create_dir_all(agents.join("a5")).unwrap();
    std::fs::write(agents.join("a5/instance.json"), json!({"id": "a5", "host": {"pid": me.pid, "start": me.start}}).to_string()).unwrap();

    let waits = crate::server::Waits { hosted_close: std::time::Duration::from_secs(1), hosted_young: std::time::Duration::ZERO, ..Default::default() };
    let user = service_with(std::sync::Arc::new(Fake::default()), &db, crate::runtime::Hosts::new(tmp.path()), waits);
    wait_until("the close is recorded", || bodies(&db, "agent.closed").len() == 1);
    assert_eq!(bodies(&db, "agent.closed")[0], json!({"agent": "a1", "outcome": "ended"}));
    wait_until("the lost answers are recorded", || bodies(&db, "agent.answered").len() == 2);
    let answered = bodies(&db, "agent.answered");
    assert!(answered.contains(&json!({"agent": "a1", "request": 5, "decision": "allow"})) && answered.contains(&json!({"agent": "a4", "request": 7, "decision": "reject"})), "{answered:?}");
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
