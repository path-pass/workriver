//! The service's guards for the restructure (docs/design/restructure.md, section 5): every
//! route with its methods on both listeners, who may use each route, and the JSON the page
//! reads. A move keeps these green unchanged; a test or blessed file here changes only with
//! a deliberate change of behaviour. (The CLI's guards are in `cli/guards.rs`; the runtime
//! names' ratchet, which reads the whole repository, is `tests/boundaries.rs`.)
use std::collections::BTreeSet;
use std::io::{Read, Write};

use serde_json::{Value, json};

use super::support::*;
use crate::cli::Daemon;

/// Every route of this machine's listener, as `router()` declares it, with the methods it
/// answers. The page, the CLI and the user's session use these.
const LOCAL: &[(&str, &[&str])] = &[
    ("/api/session", &["GET", "POST"]),
    ("/api/session/code", &["POST"]),
    ("/api/machines", &["GET", "POST"]),
    ("/api/metrics", &["GET"]),
    ("/api/metrics/history", &["GET"]),
    ("/api/metrics/thresholds", &["POST"]),
    ("/api/settings/stall", &["GET", "POST"]),
    ("/api/workspace", &["GET"]),
    ("/api/unit/{id}/code", &["GET"]),
    ("/api/state", &["GET"]),
    ("/api/task", &["GET"]),
    ("/api/status", &["GET"]),
    ("/api/events", &["GET"]),
    ("/api/append", &["POST"]),
    ("/api/decide", &["POST"]),
    ("/api/task/drop", &["POST"]),
    ("/api/gate", &["POST"]),
    ("/api/chore-done", &["POST"]),
    ("/api/backlog/merge", &["POST"]),
    ("/api/focus", &["POST"]),
    ("/api/intent/start", &["POST"]),
    ("/api/run/prepare", &["POST"]),
    ("/api/run", &["POST"]),
    ("/api/run/{id}", &["GET"]),
    ("/api/run/{id}/focus", &["POST"]),
    ("/api/run/{id}/review", &["POST"]),
    ("/api/here", &["POST"]),
    ("/api/ask", &["POST"]),
    ("/api/thread/end", &["POST"]),
    ("/api/message", &["POST"]),
    ("/api/messages/read", &["POST"]),
    ("/api/attachments", &["POST"]),
    ("/api/attachments/inbox", &["POST"]),
    ("/api/attachments/{id}", &["GET"]),
];

/// The allow-list of a followed machine's listener (`machine_router()`): only what an
/// agent's CLI over there needs. Anything else there is refused by its fallback.
const MACHINE: &[(&str, &[&str])] = &[
    ("/api/here", &["POST"]),
    ("/api/append", &["POST"]),
    ("/api/ask", &["POST"]),
    ("/api/attachments", &["POST"]),
    ("/api/attachments/inbox", &["POST"]),
    ("/api/attachments/{id}", &["GET"]),
    ("/api/workspace", &["GET"]),
    ("/api/state", &["GET"]),
    ("/api/task", &["GET"]),
    ("/api/status", &["GET"]),
    ("/api/metrics", &["GET"]),
    ("/api/metrics/history", &["GET"]),
    ("/api/run/{id}/review", &["POST"]),
];


/// Every standard method. A GET route answers HEAD too (axum adds it); nothing else is
/// implied by a route.
const METHODS: [&str; 9] = ["GET", "HEAD", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "TRACE", "CONNECT"];

/// The status code of one request, with these headers and this body; never waits long (the
/// event stream would not end by itself).
fn ask(port: u16, method: &str, path: &str, headers: &[(String, String)], body: &[u8], kind: &str) -> u16 {
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n");
    for (k, v) in headers {
        head += &format!("{k}: {v}\r\n");
    }
    if !body.is_empty() {
        head += &format!("Content-Type: {kind}\r\nContent-Length: {}\r\n", body.len());
    }
    s.write_all(head.as_bytes()).unwrap();
    s.write_all(b"\r\n").unwrap();
    s.write_all(body).unwrap();
    let mut line = [0u8; 12];
    s.read_exact(&mut line).unwrap();
    String::from_utf8_lossy(&line[9..12]).parse().unwrap()
}

/// The status line's code of one request with nothing that names anyone.
fn status(port: u16, method: &str, path: &str) -> u16 {
    let body: &[u8] = if matches!(method, "GET" | "HEAD") { b"" } else { b"{}" };
    ask(port, method, path, &[], body, "application/json")
}

/// Whether a path is one of a route's template (`{id}`: any one segment).
fn fits(template: &str, path: &str) -> bool {
    let (t, p): (Vec<&str>, Vec<&str>) = (template.split('/').collect(), path.split('/').collect());
    t.len() == p.len() && t.iter().zip(&p).all(|(a, b)| a == b || (a.starts_with('{') && !b.is_empty()))
}

/// A path for a route's template.
fn concrete(template: &str) -> String {
    template.replace("{id}", "x1")
}

/// Both listeners answer each declared path with exactly its methods, of all nine: HEAD
/// where GET is (and only there), any other undeclared method 405 (a POST or an OPTIONS
/// added to a GET path fails here although the count of routes stays). On a machine's
/// listener, every path of this machine's that is not on its allow-list is refused by the
/// fallback, whatever the method. And the sources declare no route besides these: a new one
/// is added to the tables in the same commit.
#[test]
fn every_route_answers_only_its_methods() {
    let tmp = tempfile::tempdir().unwrap();
    let local = std::sync::Arc::new(Fake::default());
    let far = std::sync::Arc::new(Fake::default());
    let all = crate::runtime::follow::Machines::with(local.clone(), vec![("box", far)]);
    let (port, _kernel) = daemon_over(local, Some(all.clone()), &tmp.path().join("w.db"));
    wait_until("the service", || http(port, "GET", "/api/status", &[], None).0 == 200);
    wait_until("the machine's listener", || all.local_port("box").is_some());
    let there = all.local_port("box").unwrap();

    for (port, table, name) in [(port, LOCAL, "local"), (there, MACHINE, "machine")] {
        for (route, allowed) in table {
            let path = concrete(route);
            for m in METHODS {
                let got = status(port, m, &path);
                let declared = allowed.contains(&m) || (m == "HEAD" && allowed.contains(&"GET"));
                if m == "HEAD" && declared {
                    // HEAD is GET without the body: it answers what GET answers, never more.
                    assert_eq!(got, status(port, "GET", &path), "{name}: HEAD {route} answers otherwise than GET");
                } else if declared {
                    assert!(got != 405 && got != 404, "{name}: {m} {route} is declared, yet answered {got}");
                } else {
                    assert_eq!(got, 405, "{name}: {m} {route} is not declared, yet answered {got}");
                }
            }
        }
    }
    for (route, _) in LOCAL.iter().filter(|(r, _)| !MACHINE.iter().any(|(m, _)| m == r)) {
        for m in METHODS {
            assert_eq!(status(there, m, &concrete(route)), 403, "machine: {m} {route} must not be routed there");
        }
    }

    // The declarations in the sources: as many as the two tables, and no other path.
    let mut declared = vec![];
    for file in sources(&crate_dir().join("src"), &["rs"]) {
        if file.components().any(|c| c.as_os_str() == "tests") || file.ends_with("tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        declared.extend(text.match_indices(".route(\"").map(|(at, m)| text[at + m.len()..].split('"').next().unwrap().to_string()));
    }
    let tables: BTreeSet<String> = LOCAL.iter().chain(MACHINE).map(|(r, _)| r.to_string()).collect();
    assert_eq!(declared.len(), LOCAL.len() + MACHINE.len(), "routes declared in the sources: {declared:?}");
    assert_eq!(declared.into_iter().collect::<BTreeSet<_>>(), tables);
}

/// One request per declared route and method, with a body and query its handler takes, so
/// that what answers is the handler's own check of who calls, not a refused body. Names in
/// them (u1#1, w1…) are those `who_may_use_each_route` sets up; the others name nothing.
const PROBES: &[(&str, &str, &str)] = &[
    ("GET", "/api/session", ""),
    ("POST", "/api/session", r#"{"code":"not-a-code"}"#),
    ("POST", "/api/session/code", r#"{"printed":true}"#),
    ("GET", "/api/machines", ""),
    ("POST", "/api/machines", r#"{"label":"nowhere","action":"disable"}"#),
    ("GET", "/api/metrics", ""),
    ("GET", "/api/metrics/history?machine=local&hours=1", ""),
    ("POST", "/api/metrics/thresholds", "THRESHOLDS"),
    ("GET", "/api/settings/stall", ""),
    ("POST", "/api/settings/stall", r#"{"minutes":30}"#),
    ("GET", "/api/workspace", ""),
    ("GET", "/api/unit/u1/code", ""),
    ("GET", "/api/state", ""),
    ("GET", "/api/task?task=u1%231", ""),
    ("GET", "/api/status", ""),
    ("GET", "/api/events", ""),
    ("POST", "/api/append", r#"{"kind":"intent.created","body":{"title":"Probe","state":"planned"}}"#),
    ("POST", "/api/decide", r#"{"unit":"u1","task":1,"finding":"P2-9","decision":"defer"}"#),
    ("POST", "/api/task/drop", r#"{"unit":"u1","task":9,"reason":"probe"}"#),
    ("POST", "/api/gate", r#"{"unit":"u1","task":9,"result":"done"}"#),
    ("POST", "/api/chore-done", r#"{"chore":"c9"}"#),
    ("POST", "/api/backlog/merge", r#"{"into":"i8","from":["i9"]}"#),
    ("POST", "/api/focus", r#"{"agent":"nobody-here"}"#),
    ("POST", "/api/intent/start", r#"{"intent":"i9","workspace":"chat"}"#),
    ("POST", "/api/run/prepare", r#"{"command":"true","unit":"u9"}"#),
    ("POST", "/api/run", r#"{"id":"r9","command":"true"}"#),
    ("GET", "/api/run/r9", ""),
    ("POST", "/api/run/r9/focus", ""),
    ("POST", "/api/run/r9/review", r#"{"verdict":"ok","text":"probe"}"#),
    ("POST", "/api/here", "{}"),
    ("POST", "/api/ask", r#"{"unit":"u1","task":9,"text":"why?"}"#),
    ("POST", "/api/thread/end", r#"{"thread":"q9"}"#),
    ("POST", "/api/message", r#"{"intent":"i9","text":"probe"}"#),
    ("POST", "/api/messages/read", r#"{"workspace":"chat","at":"2026-01-01T00:00:00Z"}"#),
    ("POST", "/api/attachments?name=probe.txt", "BYTES"),
    ("POST", "/api/attachments/inbox", r#"{"unit":"u1","task":1}"#),
    ("GET", "/api/attachments/x1", ""),
];

/// Who may use each route and method (HEAD too, wherever GET is), on both listeners: the
/// status every route answers to a caller
/// with nothing that names it, to an agent from its own place, and to the user (the user
/// token, from a process in no agent's place; on a machine's listener, where no token
/// counts, the same request). The service runs as it does for the user (no `--as`, a
/// terminal asked for). Pinned in `route-access.txt`; and, whatever the file says, nobody is
/// answered with success except where anyone may ask (`ANYONE`).
#[test]
fn who_may_use_each_route() {
    use workriver_core::runtime::AgentState::*;
    const ANYONE: &[(&str, &str)] = &[("GET", "/api/session"), ("HEAD", "/api/session"), ("GET", "/api/status"), ("HEAD", "/api/status"), ("POST", "/api/here")];
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let local = std::sync::Arc::new(Fake::default());
    *local.agents.lock().unwrap() = vec![("w1:p1".into(), live("s-boss", "boss", "w1", Idle))];
    let far = std::sync::Arc::new(Fake::default());
    *far.agents.lock().unwrap() = vec![("w1:p1".into(), live("s-far", "far", "w1", Idle))];
    let all = crate::runtime::follow::Machines::with(local, vec![("box", far)]);
    let (port, kernel) = daemon_over(all.clone(), Some(all.clone()), &tmp.path().join("w.db"));
    wait_until("the service", || http(port, "GET", "/api/status", &[], None).0 == 200);
    wait_until("the machine's listener", || all.local_port("box").is_some());
    let user = kernel.client(port, tmp.path());
    let boss = kernel.daemon(port, Some("w1:p1".into()), None);
    let far = kernel.daemon(all.local_port("box").unwrap(), Some("w1:p1".into()), None);
    let group = json!({ "id": "w1", "label": "chat" });
    user.append("", "intent.created", json!({ "title": "Chat", "state": "planned" })).unwrap();
    boss.append("", "manager.registered", json!({ "workspace": group })).unwrap();
    boss.append("", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": group })).unwrap();
    boss.append("", "unit.created", json!({ "intent": "i1", "title": "M", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Do it" })).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a poll sees the agents

    let token = format!("Bearer {}", crate::common::home::read_token(tmp.path()).unwrap());
    // What shows the service who calls: the place a process names, its proof (this machine)
    // and its process id (a machine's listener), and the token if it has it.
    let from = |d: &Daemon, token: Option<&str>| -> Vec<(String, String)> {
        let (proof, pid) = d.process(true);
        let mut h = vec![("x-workriver-caller".to_string(), proof.unwrap_or_default()), ("x-workriver-pid".to_string(), pid.to_string())];
        h.extend(d.place.clone().map(|p| ("x-workriver-place".to_string(), p)));
        h.extend(token.map(|t| ("authorization".to_string(), t.to_string())));
        h
    };
    // HEAD is asked wherever GET is (axum answers it there), as GET is asked: the same path.
    let probes: Vec<(&str, &str, &str)> = PROBES.iter().copied().chain(PROBES.iter().filter(|(m, ..)| *m == "GET").map(|(_, p, _)| ("HEAD", *p, ""))).collect();
    let answers = |ms: &[&str], m: &str| ms.contains(&m) || (m == "HEAD" && ms.contains(&"GET"));
    // Every declared route and method, HEAD included, has its probe.
    for (r, ms) in LOCAL.iter().chain(MACHINE) {
        for m in ms.iter().copied().chain(ms.contains(&"GET").then_some("HEAD")) {
            assert!(probes.iter().any(|(pm, pp, _)| *pm == m && fits(r, pp.split('?').next().unwrap())), "no probe for {m} {r}");
        }
    }
    let thresholds = serde_json::to_string(&crate::common::config::Thresholds::default()).unwrap();
    let mut got = String::new();
    for (listener, port, table) in [("local", port, LOCAL), ("machine", all.local_port("box").unwrap(), MACHINE)] {
        for (method, path, body) in &probes {
            let route = path.split('?').next().unwrap();
            if !table.iter().any(|(r, ms)| fits(r, route) && answers(ms, method)) {
                continue;
            }
            let (body, kind) = match *body {
                "THRESHOLDS" => (thresholds.as_bytes(), "application/json"),
                "BYTES" => (&b"probe"[..], "text/plain"),
                b => (b.as_bytes(), "application/json"),
            };
            let agent = if listener == "local" { &boss } else { &far };
            let callers: [(&str, Vec<(String, String)>); 3] = [("anonymous", vec![]), ("agent", from(agent, None)), ("user", from(&user, Some(&token)))];
            let mut line = format!("{listener} {method} {path}");
            for (who, headers) in callers {
                let code = ask(port, method, path, &headers, body, kind);
                line += &format!("  {who} {code}");
                if who == "anonymous" {
                    assert!(!(200..300).contains(&code) || ANYONE.contains(&(*method, route)), "{listener}: anonymous {method} {path} answered {code}");
                }
            }
            got += &line;
            got.push('\n');
        }
    }
    let path = crate_dir().join("src/server/tests/route-access.txt");
    if std::env::var("WORKRIVER_BLESS").is_ok_and(|v| v == "1") {
        std::fs::write(&path, &got).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap();
    let (g, w): (BTreeSet<&str>, BTreeSet<&str>) = (got.lines().collect(), want.lines().collect());
    assert!(g == w, "who may use a route changed; was: {:?}; now: {:?}", w.difference(&g).collect::<Vec<_>>(), g.difference(&w).collect::<Vec<_>>());
}

fn crate_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every file under `dir` with one of these extensions, skipping build output and packages.
fn sources(dir: &std::path::Path, ext: &[&str]) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy();
        if p.is_dir() {
            if !matches!(name.as_ref(), "target" | "node_modules" | "dist" | ".git") {
                out.extend(sources(&p, ext));
            }
        } else if p.extension().is_some_and(|e| ext.iter().any(|x| e == *x)) {
            out.push(p);
        }
    }
    out
}


/// Every key path of a JSON value with the kinds of value found there (arrays: one path for
/// all their elements, `[]`). What the page reads is these names and kinds.
fn shape(v: &Value, at: &str, out: &mut BTreeSet<String>) {
    let kind = match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(a) => {
            for x in a {
                shape(x, &format!("{at}[]"), out);
            }
            "array"
        }
        Value::Object(o) => {
            for (k, x) in o {
                shape(x, &format!("{at}.{k}"), out);
            }
            "object"
        }
    };
    out.insert(format!("{at}: {kind}"));
}

/// A workspace with most of what the page shows: projects and a roadmap, a backlog, an
/// intent with its manager, a code unit with committed, unregistered and uncommitted work,
/// a research unit, a design, its review with decided findings, a waiting gate, a chore, an
/// attachment, a question with its temporary agent, a conversation with the manager, and a
/// second machine.
fn a_full_workspace(tmp: &std::path::Path) -> (Daemon, std::sync::Arc<Fake>) {
    use workriver_core::runtime::AgentState::*;
    let repo = tmp.join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "base"]);
    git(&repo, &["switch", "-q", "-c", "feat/index"]);
    std::fs::write(repo.join("a.txt"), "one\ntwo\n").unwrap();
    git(&repo, &["commit", "-q", "-am", "Build\n\nTask: u1#2"]);
    std::fs::write(repo.join("b.txt"), "b\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "tweak"]);
    std::fs::write(repo.join("c.txt"), "c\n").unwrap();
    let repo = repo.to_string_lossy().to_string();

    let fake = std::sync::Arc::new(Fake::default());
    let mut rev = live("s-rev", "rev", "w1", Blocked);
    rev.topic = Some("reviewing".into());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-impl", "impl", "w1", Working)), ("p3".into(), rev)];
    let far = std::sync::Arc::new(Fake::default());
    *far.agents.lock().unwrap() = vec![("p1".into(), live("s-far", "far", "w1", Idle))];
    let all = crate::runtime::follow::Machines::with(fake.clone(), vec![("box", far)]);
    let store = workriver_data::Store::open(&tmp.join("w.db")).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::serve::bind(0)).unwrap();
    let kernel = Kernel::default();
    let d = kernel.client(listener.local_addr().unwrap().port(), tmp);
    let (runtime, waits) = (fake.clone(), crate::serve::Waits { answer: std::time::Duration::from_secs(60), ..Default::default() });
    let options = crate::serve::Options { machines: Some(all.clone()), waits, allow_as: true, ..kernel.options() };
    std::thread::spawn(move || rt.block_on(crate::serve::serve_with(listener, store, runtime, options)));
    wait_until("the machine's listener", || all.local_port("box").is_some());

    d.append("user", "project.added", json!({ "path": repo, "title": "Chat" })).unwrap();
    manage(&d, "p1", "s-boss");
    d.append("user", "intent.created", json!({ "title": "Search", "goal": "Find messages", "project": repo, "state": "planned", "note": "See the spec.", "external": { "system": "jira", "key": "CH-1", "url": "https://example.invalid/CH-1" } })).unwrap();
    d.append("user", "finding.added", json!({ "project": repo, "finding": { "severity": "P2", "problem": "Old dependency", "impact": "A known hole", "where": ["Cargo.toml"] }, "source": "audit" })).unwrap();
    d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "Index", "purpose": "Build the index", "repo": repo, "branch": "feat/index", "base": "main" })).unwrap();
    d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "Survey", "kind": "research" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "design", "title": "Index design", "agent": "impl", "plans": ["a.txt"] })).unwrap();
    d.append("s-impl", "task.delivered", json!({ "unit": "u1", "task": 1, "note": "One table", "evidence": [{ "command": "cargo test", "result": "passed", "summary": "3 passed" }] })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Build it", "agent": "impl", "inputs": [{ "from": "1" }], "steps": [{ "text": "Check", "command": "cargo test" }] })).unwrap();
    d.append("s-impl", "task.started", json!({ "unit": "u1", "task": 2 })).unwrap();
    d.append("s-impl", "task.delivered", json!({ "unit": "u1", "task": 2, "note": "Built", "evidence": [{ "command": "cargo test", "result": "passed", "summary": "5 passed" }] })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "review", "title": "Review it", "agent": "rev", "inputs": [{ "from": "2" }], "blind": true })).unwrap();
    d.append("s-rev", "task.delivered", json!({ "unit": "u1", "task": 3, "findings": [
        { "severity": "P1", "problem": "No limit", "impact": "Out of memory", "where": ["a.txt"], "suggestion": { "decision": "accept", "reason": "real" } },
        { "severity": "P3", "problem": "Naming", "impact": "Reads badly", "where": ["b.txt"] } ] })).unwrap();
    d.append("s-rev", "finding.noted", json!({ "unit": "u1", "task": 3, "finding": "P1-1", "text": "Cap at 10k" })).unwrap();
    d.call("/api/decide", Some(json!({ "unit": "u1", "task": 3, "finding": "P1-1", "decision": "accept", "note": "yes" }))).unwrap();
    d.call("/api/decide", Some(json!({ "unit": "u1", "task": 3, "finding": "P3-1", "decision": "defer" }))).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "gate", "gate": "decide", "title": "Merge?", "inputs": [{ "from": "3" }] })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u2", "kind": "research", "title": "Survey tools", "agent": "rev" })).unwrap();
    d.append("s-boss", "chore.created", json!({ "intent": "i1", "title": "Add a token", "steps": [{ "text": "Create it", "command": "gh auth token" }] })).unwrap();
    let shot = tmp.join("shot.png");
    std::fs::write(&shot, b"\x89PNG shape").unwrap();
    d.upload(&shot, "s-impl", Some(("u1", 2, None))).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a poll sees the agents
    d.call("/api/ask", Some(json!({ "unit": "u1", "task": 2, "text": "Why one table?", "context": [{ "kind": "file", "label": "a.txt", "path": "a.txt" }] }))).unwrap();
    wait_until("the question's agent is asked", || fake.sent.lock().unwrap().iter().any(|(to, _)| to == "why-q1"));
    d.call("/api/message", Some(json!({ "intent": "i1", "text": "How is it going?" }))).unwrap();
    d.append("s-boss", "message.said", json!({ "text": "Design confirmed." })).unwrap();
    (d, fake)
}

/// The JSON the page reads (`/api/workspace`, a unit's `/api/unit/{id}/code`) keeps its
/// names and kinds of value: the page's types (frontend/web/src/types.ts) are the contract,
/// and the code that builds it moves. Pinned on the workspace above: a key this workspace
/// leaves out is not pinned. `WORKRIVER_BLESS=1` writes the file again after a deliberate
/// change of what the page reads (with the page changed in the same commit).
#[test]
fn the_json_the_page_reads_keeps_its_shape() {
    let tmp = tempfile::tempdir().unwrap();
    let (d, _fake) = a_full_workspace(tmp.path());
    let mut got = BTreeSet::new();
    shape(&d.get("/api/workspace").unwrap(), "workspace", &mut got);
    shape(&d.get("/api/unit/u1/code").unwrap(), "code", &mut got);
    let got: String = got.into_iter().map(|l| l + "\n").collect();
    let path = crate_dir().join("src/server/tests/page-json.txt");
    if std::env::var("WORKRIVER_BLESS").is_ok_and(|v| v == "1") {
        std::fs::write(&path, &got).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap();
    let (g, w): (BTreeSet<&str>, BTreeSet<&str>) = (got.lines().collect(), want.lines().collect());
    assert!(g == w, "the page's JSON changed; gone: {:?}; new: {:?}", w.difference(&g).collect::<Vec<_>>(), g.difference(&w).collect::<Vec<_>>());
}
