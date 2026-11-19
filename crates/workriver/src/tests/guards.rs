//! Guards that hold today's behaviour while the files move (docs/design/restructure.md,
//! phase 0): every route with its methods on both listeners, the guide's topics, the JSON
//! the page reads, and where the runtime's names may appear. A move keeps these green
//! unchanged; a test here changes only with a deliberate change of behaviour, and the
//! ratchet's list only shrinks.
use std::collections::BTreeSet;
use std::io::{Read, Write};

use serde_json::{Value, json};

use super::*;

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

const METHODS: [&str; 5] = ["GET", "POST", "PUT", "DELETE", "PATCH"];

/// The status line's code of one request with nothing that names anyone; never waits long
/// (the event stream would not end by itself).
fn status(port: u16, method: &str, path: &str) -> u16 {
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
    let body = if method == "GET" { String::new() } else { "{}".into() };
    write!(s, "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let mut head = [0u8; 12];
    s.read_exact(&mut head).unwrap();
    String::from_utf8_lossy(&head[9..12]).parse().unwrap()
}

/// A path for a route's template.
fn concrete(template: &str) -> String {
    template.replace("{id}", "x1")
}

/// Both listeners answer each declared path with exactly its methods: another method is
/// 405, whatever it is (a POST added to a GET path fails here although the count of routes
/// stays). On a machine's listener, every path of this machine's that is not on its
/// allow-list is refused by the fallback, whatever the method. And the sources declare no
/// route besides these: a new one is added to the tables in the same commit.
#[test]
fn every_route_answers_only_its_methods() {
    let tmp = tempfile::tempdir().unwrap();
    let local = std::sync::Arc::new(Fake::default());
    let far = std::sync::Arc::new(Fake::default());
    let all = crate::machines::Machines::with(local.clone(), vec![("box", far)]);
    let (port, _kernel) = daemon_over(local, Some(all.clone()), &tmp.path().join("w.db"));
    wait_until("the service", || http(port, "GET", "/api/status", &[], None).0 == 200);
    wait_until("the machine's listener", || all.local_port("box").is_some());
    let there = all.local_port("box").unwrap();

    for (port, table, name) in [(port, LOCAL, "local"), (there, MACHINE, "machine")] {
        for (route, allowed) in table {
            let path = concrete(route);
            for m in METHODS {
                let got = status(port, m, &path);
                if allowed.contains(&m) {
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

/// The recipes `workriver guide how <topic>` knows, by name. Moving a recipe keeps its name;
/// adding or removing one changes this list in the same commit.
const TOPICS: &[&str] = &[
    "architecture-review",
    "ask",
    "ask-user",
    "attachments",
    "blind-review",
    "blocked",
    "branch-moved",
    "closing-units",
    "closing-workers",
    "commit-link",
    "contract",
    "deciding-findings",
    "deferred-batches",
    "design-gate",
    "finding-additions",
    "gates-chores",
    "hand-off",
    "machines",
    "not-delivered",
    "push-pr-merge",
    "spike",
    "stalled",
    "start-intent",
    "units",
    "units-other-machine",
    "updating",
    "user-message",
];

/// Every topic answers with its own recipe (not the index an unknown topic gets), under a
/// head naming it; the embedded recipes are exactly these; and every guide there is still
/// answers.
#[test]
fn the_guide_keeps_its_topics() {
    let index = crate::guide("how", None).unwrap();
    for topic in TOPICS {
        let text = crate::guide("how", Some(topic)).unwrap();
        assert!(text.starts_with(&format!("workriver {} · guide how {topic}\n\n", crate::BUILD)) && text != index, "{topic}");
    }
    let embedded: BTreeSet<String> = crate::Recipes::iter().map(|f| f.trim_end_matches(".md").to_string()).collect();
    assert_eq!(embedded, TOPICS.iter().map(|t| t.to_string()).collect::<BTreeSet<_>>());
    for (what, topic) in [("worker", None), ("manager", None), ("import", None), ("how", None), ("changes", None), ("install-stub", Some("manager")), ("install-stub", Some("howto")), ("install-stub", Some("import"))] {
        assert!(crate::guide(what, topic).is_ok_and(|t| !t.is_empty()), "{what} {topic:?}");
    }
}

/// The source files that still name the runtime (`herdr`, any case) or its places (`pane`)
/// outside the runtime adapter, as of the start of the restructure. The adapter itself
/// (`ADAPTER`) may; every other file may only leave this list. When a file stops naming
/// them, its line is removed here (the test says so); a moved file's line follows it.
const ADAPTER: &str = "crates/workriver/src/herdr.rs";
const STILL_HERDR: &[&str] = &[
    "crates/workriver-core/src/lib.rs",
    "crates/workriver-core/src/model.rs",
    "crates/workriver-core/src/runtime.rs",
    "crates/workriver/src/ask.rs",
    "crates/workriver/src/caller.rs",
    "crates/workriver/src/deliver.rs",
    "crates/workriver/src/machines.rs",
    "crates/workriver/src/main.rs",
    "crates/workriver/src/metrics.rs",
    "crates/workriver/src/run.rs",
    "crates/workriver/src/serve.rs",
    "crates/workriver/src/snapshot.rs",
    "crates/workriver/src/stall.rs",
    "crates/workriver/src/tests.rs",
    "crates/workriver/tests/caller.rs",
    "crates/workriver/tests/cli.rs",
    "crates/workriver/tests/output.rs",
    "frontend/web/src/components/Run.tsx",
    "frontend/web/src/components/Settings.tsx",
    "frontend/web/src/components/Start.tsx",
    "frontend/web/src/i18n/en.ts",
    "frontend/web/src/i18n/ja.ts",
    "frontend/web/src/i18n/zh-CN.ts",
    "frontend/web/src/runtime.ts",
    "frontend/web/src/types.ts",
];
const STILL_PANE: &[&str] = &[
    "crates/workriver-core/src/model.rs",
    "crates/workriver/src/main.rs",
    "crates/workriver/src/run.rs",
    "crates/workriver/src/serve.rs",
    "crates/workriver/src/tests.rs",
    "crates/workriver/tests/caller.rs",
    "crates/workriver/tests/cli.rs",
    "crates/workriver/tests/output.rs",
    "frontend/web/src/components/CodeLayer.tsx",
    "frontend/web/src/i18n/en.ts",
    "frontend/web/src/runtime.ts",
];

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

fn names(text: &str, word: &str) -> bool {
    let lower = text.to_lowercase();
    if word == "herdr" {
        return lower.contains("herdr");
    }
    lower.match_indices(word).any(|(at, _)| {
        let before = lower[..at].chars().next_back().is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let rest = &lower[at + word.len()..];
        let rest = rest.strip_prefix('s').unwrap_or(rest);
        before && rest.chars().next().is_none_or(|c| !c.is_alphanumeric() && c != '_')
    })
}

#[test]
fn no_runtime_names_outside_the_adapter_but_those_still_listed() {
    let root = crate_dir().join("../..").canonicalize().unwrap();
    let me = crate_dir().join("src/tests/guards.rs");
    let mut files = sources(&root.join("crates"), &["rs"]);
    files.extend(sources(&root.join("frontend/web/src"), &["ts", "tsx"]));
    for (word, still) in [("herdr", STILL_HERDR), ("pane", STILL_PANE)] {
        let found: BTreeSet<String> = files
            .iter()
            .filter(|f| **f != me && names(&std::fs::read_to_string(f).unwrap(), word))
            .map(|f| f.strip_prefix(&root).unwrap().to_string_lossy().into_owned())
            .filter(|f| f != ADAPTER)
            .collect();
        let listed: BTreeSet<String> = still.iter().map(|s| s.to_string()).collect();
        let new: Vec<_> = found.difference(&listed).collect();
        let gone: Vec<_> = listed.difference(&found).collect();
        assert!(new.is_empty(), "{word:?} named outside the runtime adapter, in files not on the list: {new:?}");
        assert!(gone.is_empty(), "{word:?} no longer named in {gone:?}: take them off the list (it only shrinks)");
    }
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
    let all = crate::machines::Machines::with(fake.clone(), vec![("box", far)]);
    let store = workriver_core::Store::open(&tmp.join("w.db")).unwrap();
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
    let path = crate_dir().join("src/tests/page-json.txt");
    if std::env::var("WORKRIVER_BLESS").is_ok_and(|v| v == "1") {
        std::fs::write(&path, &got).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap();
    let (g, w): (BTreeSet<&str>, BTreeSet<&str>) = (got.lines().collect(), want.lines().collect());
    assert!(g == w, "the page's JSON changed; gone: {:?}; new: {:?}", w.difference(&g).collect::<Vec<_>>(), g.difference(&w).collect::<Vec<_>>());
}
