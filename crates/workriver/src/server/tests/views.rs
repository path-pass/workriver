// What the page and the CLI read of a unit and a task.
use std::process::Command;

use serde_json::json;

use super::support::*;

#[test]
fn page_load_timing_requires_a_page_session_and_bounded_numbers() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", workriver_core::runtime::AgentState::Idle))];
    let (port, kernel) = real_service(fake, tmp.path());
    let user = kernel.client(port, tmp.path());
    let origin = format!("http://127.0.0.1:{port}");
    let path = "/api/perf/page-load";
    let valid = json!({
        "ms": 120, "js_ms": 35, "css_ms": 28, "request_ms": 48,
        "response_ms": 85, "bytes": 1234, "ready_ms": 98, "parse_ms": 13, "app_ms": 14,
        "render_ms": 8, "git_pending": 1,
        "api": [{ "path": "/api/task", "count": 2, "ms": 25, "max_ms": 15 }]
    });
    let bearer = format!("Bearer {}", user.token().unwrap());
    let (status, answer) = http_at(&user, port, "p1", "POST", path, &[("Authorization", &bearer)], Some(&valid.to_string()));
    assert_eq!(status, 401, "{answer}");
    assert_eq!(as_user(&user, "POST", path, Some(&valid)).0, 401);

    let link = user.call("/api/session/code", Some(json!({}))).unwrap();
    let code = link["url"].as_str().unwrap().split_once("#k=").unwrap().1;
    let (status, answer) = http(port, "POST", "/api/session", &[("Origin", &origin)], Some(&json!({ "code": code }).to_string()));
    assert_eq!(status, 200, "{answer}");
    let cookie = answer.lines().find(|l| l.starts_with("set-cookie:")).unwrap().split_once(':').unwrap().1.split(';').next().unwrap().trim();
    let post = |body: &serde_json::Value| http(port, "POST", path, &[("Origin", &origin), ("Cookie", cookie)], Some(&body.to_string()));
    assert_eq!(http(port, "POST", path, &[("Cookie", cookie)], Some(&valid.to_string())).0, 401);
    assert_eq!(post(&valid).0, 204);
    let independently_measured = json!({
        "ms": 100, "css_ms": null, "ready_ms": 90, "app_ms": 5, "render_ms": 17, "git_pending": 0,
        "api": [
            { "path": "/api/task", "count": 1, "ms": 500, "max_ms": 400 },
            { "path": "/api/task", "count": 1, "ms": 20, "max_ms": 20 }
        ]
    });
    assert_eq!(post(&independently_measured).0, 204);
    for (field, bad) in [
        ("ms", json!(u64::MAX)), ("ready_ms", json!(-1)), ("response_ms", json!("85")),
        ("css_ms", json!(u64::MAX)), ("bytes", json!(u64::MAX)), ("git_pending", json!(2)),
        ("build", json!(42)), ("url", json!("/unexpected")),
        ("app_ms", json!(u64::MAX)), ("render_ms", json!(u64::MAX)), ("parse_ms", json!(u64::MAX)),
        ("api", json!([{ "path": "/api/task?id=private", "count": 1, "ms": 8, "max_ms": 8 }])),
        ("api", json!([{ "path": "/api/unit/private/code", "count": 1, "ms": 8, "max_ms": 8 }])),
        ("api", json!([{ "path": "/api/task", "count": u64::MAX, "ms": 8, "max_ms": 8 }])),
        ("api", json!([{ "path": "/api/task", "count": 1, "ms": u64::MAX, "max_ms": 8 }])),
        ("api", json!([{ "path": "/api/task", "count": 1, "ms": 8, "max_ms": u64::MAX }])),
    ] {
        let mut data = valid.clone();
        data[field] = bad;
        let (status, answer) = post(&data);
        assert_eq!(status, 422, "{field}: {answer}");
    }
    let oversized = format!("{}{}", valid, " ".repeat(8192));
    assert_eq!(http(port, "POST", path, &[("Origin", &origin), ("Cookie", cookie)], Some(&oversized)).0, 413);
}

#[test]
fn browser_workspace_shows_records_before_git_and_notifies_when_ready() {
    use std::io::{BufRead, BufReader, Write};
    use std::time::Duration;

    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "Base"]);
    git(&repo, &["switch", "-q", "-c", "feat/work"]);
    std::fs::write(repo.join("hello.txt"), "hello\n").unwrap();
    git(&repo, &["add", "hello.txt"]);
    git(&repo, &["commit", "-q", "-m", "Hello\n\nTask: u1#1"]);

    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", workriver_core::runtime::AgentState::Idle))];
    let (user, kernel) = daemon_and(fake, &tmp.path().join("w.db"), Default::default());
    let boss = kernel.daemon(user.port, Some("p1".into()), None);
    let ws = seat_at(&user, &boss);
    user.append("", "intent.created", json!({ "title": "First load", "state": "planned" })).unwrap();
    user.append("", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": ws })).unwrap();
    boss.append("", "unit.created", json!({ "template": "bare", "intent": "i1", "title": "Hello", "repo": repo, "branch": "feat/work", "base": "main" })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Write hello", "agent": "s-boss", "inputs": [{ "from": "start" }] })).unwrap();

    let port = user.port;
    let origin = format!("http://127.0.0.1:{port}");
    let link = user.call("/api/session/code", Some(json!({}))).unwrap();
    let code = link["url"].as_str().unwrap().split_once("#k=").unwrap().1;
    let (status, answer) = http(port, "POST", "/api/session", &[("Origin", &origin)], Some(&json!({ "code": code }).to_string()));
    assert_eq!(status, 200, "{answer}");
    let cookie = answer.lines().find(|l| l.starts_with("set-cookie:")).unwrap().split_once(':').unwrap().1.split(';').next().unwrap().trim();
    let browser = |path, headers: &[(&str, &str)]| {
        let headers = [("Cookie", cookie)].into_iter().chain(headers.iter().copied()).collect::<Vec<_>>();
        let (status, answer) = http(port, "GET", path, &headers, None);
        assert_eq!(status, 200, "{answer}");
        serde_json::from_str::<serde_json::Value>(answer.split_once("\r\n\r\n").unwrap().1).unwrap()
    };
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(stream, "GET /api/events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nCookie: {cookie}\r\n\r\n").unwrap();
    let mut events = BufReader::new(stream);
    let mut line = String::new();
    assert!(events.read_line(&mut line).unwrap() > 0 && line.starts_with("HTTP/1.1 200"), "{line}");
    while line != "\r\n" {
        line.clear();
        assert!(events.read_line(&mut line).unwrap() > 0);
    }

    let first = browser("/api/workspace", &[]);
    assert_eq!(first["gitPending"], true, "the first browser snapshot precedes Git");
    let unit = &first["intents"][0]["units"][0];
    assert_eq!(unit["tasks"][0]["title"], "Write hello");
    assert_eq!(unit["commits"], json!([]));
    assert_eq!(first["repos"][0]["modules"]["children"], json!([]));

    let full = loop {
        line.clear();
        assert!(events.read_line(&mut line).expect("the browser hears when Git is ready") > 0);
        if line.starts_with("event: changed") {
            let ws = browser("/api/workspace", &[]);
            if ws["gitPending"] != true {
                break ws;
            }
        }
    };
    let commit = &full["intents"][0]["units"][0]["commits"][0];
    assert_eq!(commit["subject"], "Hello");
    assert_eq!(full["intents"][0]["units"][0]["tasks"][0]["output"]["commits"][0], commit["id"]);
    assert_eq!(full["repos"][0]["modules"]["children"][0]["name"], "hello.txt");

    let proof = user.process(false).0.unwrap();
    let named = browser("/api/workspace?as=boss", &[("X-Workriver-Caller", &proof)]);
    assert_eq!(named["intents"][0]["units"][0]["commits"][0]["subject"], "Hello");
    let light = browser("/api/workspace?light=true", &[]);
    assert_eq!(light["intents"][0]["units"][0]["commits"][0]["subject"], "Hello");
    assert!(light["repos"][0]["modules"]["children"].as_array().unwrap().is_empty());
}

/// The unit open on the page (its code read) is followed: a file changed in its work tree
/// tells the page, and the next snapshot has the new file with its line counts and a new code
/// token, which the code read follows. Another open unit's uncommitted work is on the board
/// by name, without counts; a file git ignores in its work tree tells nobody.
#[test]
fn the_open_unit_is_followed_and_the_others_named() {
    use std::io::{BufRead, BufReader, Write};
    use std::time::{Duration, Instant};

    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let repo = root.join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join(".gitignore"), "build/\n").unwrap();
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "Base"]);
    for b in ["one", "two"] {
        git(&repo, &["worktree", "add", "-q", "-b", &format!("feat/{b}"), &root.join(b).to_string_lossy()]);
        std::fs::write(root.join(b).join("a.txt"), format!("a\n{b}\n")).unwrap();
    }

    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", workriver_core::runtime::AgentState::Idle))];
    let (user, kernel) = daemon_and(fake, &root.join("w.db"), Default::default());
    let boss = kernel.daemon(user.port, Some("p1".into()), None);
    let ws = seat_at(&user, &boss);
    user.append("", "intent.created", json!({ "title": "Follow", "state": "planned" })).unwrap();
    user.append("", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": ws })).unwrap();
    for b in ["one", "two"] {
        boss.append("", "unit.created", json!({ "template": "bare", "intent": "i1", "title": b, "repo": repo, "branch": format!("feat/{b}"), "base": "main" })).unwrap();
    }

    let port = user.port;
    let origin = format!("http://127.0.0.1:{port}");
    let link = user.call("/api/session/code", Some(json!({}))).unwrap();
    let code = link["url"].as_str().unwrap().split_once("#k=").unwrap().1;
    let (status, answer) = http(port, "POST", "/api/session", &[("Origin", &origin)], Some(&json!({ "code": code }).to_string()));
    assert_eq!(status, 200, "{answer}");
    let cookie = answer.lines().find(|l| l.starts_with("set-cookie:")).unwrap().split_once(':').unwrap().1.split(';').next().unwrap().trim().to_string();
    let browser = |path: &str| {
        let (status, answer) = http(port, "GET", path, &[("Cookie", &cookie)], None);
        assert_eq!(status, 200, "{answer}");
        serde_json::from_str::<serde_json::Value>(answer.split_once("\r\n\r\n").unwrap().1).unwrap()
    };
    let unit = |ws: &serde_json::Value, id: &str| ws["intents"][0]["units"].as_array().unwrap().iter().find(|u| u["id"] == id).unwrap().clone();
    // The board once Git is read (the first browser read is the records alone).
    let deadline = Instant::now() + Duration::from_secs(20);
    let board = loop {
        let ws = browser("/api/workspace");
        if ws["gitPending"] != true {
            break ws;
        }
        assert!(Instant::now() < deadline, "git is read");
        std::thread::sleep(Duration::from_millis(100));
    };
    for id in ["u1", "u2"] {
        let file = &unit(&board, id)["commits"][0]["files"][0];
        assert_eq!((file["path"].as_str(), file["uncounted"].as_bool()), (Some("a.txt"), Some(true)), "{id}: named, not read");
    }

    // u1 opened: its code, with the texts; the next snapshot has its counts, the same token.
    let read = browser("/api/unit/u1/code");
    assert_eq!(read["commits"][0]["files"][0]["after"], "a\none\n");
    let opened = unit(&browser("/api/workspace"), "u1");
    assert_eq!(opened["commits"][0]["files"][0]["add"], 1);
    assert!(opened["commits"][0]["files"][0].get("uncounted").is_none());
    assert_eq!(opened["code"], unit(&board, "u1")["code"], "the same files: no code read again");

    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(stream, "GET /api/events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nCookie: {cookie}\r\n\r\n").unwrap();
    let mut events = BufReader::new(stream);
    let mut line = String::new();
    assert!(events.read_line(&mut line).unwrap() > 0 && line.starts_with("HTTP/1.1 200"), "{line}");
    while line != "\r\n" {
        line.clear();
        assert!(events.read_line(&mut line).unwrap() > 0);
    }
    std::fs::write(root.join("one/a.txt"), "a\none\nmore\n").unwrap();
    let changed = loop {
        line.clear();
        assert!(events.read_line(&mut line).expect("the page hears of the open unit's change") > 0);
        if line.starts_with("event: changed") {
            let u = unit(&browser("/api/workspace"), "u1");
            if u["code"] != opened["code"] {
                break u;
            }
        }
    };
    assert_eq!(changed["commits"][0]["files"][0]["add"], 2);
    assert_eq!(browser("/api/unit/u1/code")["commits"][0]["files"][0]["after"], "a\none\nmore\n");
    assert_eq!(unit(&browser("/api/workspace"), "u2")["code"], unit(&board, "u2")["code"], "u2 as it was");
}

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
    let store = opened(&tmp.path().join("w.db"));
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
    let ws = seat_at(&user, &boss);
    user.append("", "intent.created", json!({ "title": "Login", "state": "planned" })).unwrap();
    user.append("", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": ws })).unwrap();
    boss.append("", "unit.created", json!({ "template": "bare", "intent": "i1", "title": "Auth", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
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
    let store = opened(&tmp.path().join("w.db"));
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
    let ws = seat_at(&user, &boss);
    user.append("", "intent.created", json!({ "title": "Login", "goal": "Safe login.", "state": "planned" })).unwrap();
    user.append("", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": ws })).unwrap();
    boss.append("", "unit.created", json!({ "template": "bare", "intent": "i1", "title": "Auth", "purpose": "Tokens that expire", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
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
    assert_eq!(keys(&shown), "after,agent,blind,causes,commits,intent,kind,link,lostCommits,notes,output,place,plans,questions,ref,state,steps,text,title,trace,unit,writtenBy");
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
    boss.append("", "unit.created", json!({ "template": "bare-research", "intent": "i1", "title": "Sessions", "kind": "research" })).unwrap();
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

/// A file attached to a gate is in what the page reads while the gate waits, before the user
/// answers it, and stays after; a blind agent's view still has none of it.
#[test]
fn a_waiting_gate_carries_its_attachments() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    git(&repo, &["switch", "-q", "-c", "feat/f"]);
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-imp", "imp", "w1", Idle)), ("p3".into(), live("s-sec", "sec", "w1", Idle))];
    let store = opened(&tmp.path().join("w.db"));
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let port = listener.local_addr().unwrap().port();
    let runtime = fake.clone();
    let options = kernel.options();
    std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, runtime, options)));
    let user = kernel.client(port, tmp.path());
    let at = |place: &str| kernel.daemon(port, Some(place.into()), None);
    let (boss, imp, sec) = (at("p1"), at("p2"), at("p3"));
    let ws = seat_at(&user, &boss);
    user.append("", "intent.created", json!({ "title": "Login", "goal": "Safe login", "state": "planned" })).unwrap();
    user.append("", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": ws })).unwrap();
    boss.append("", "unit.created", json!({ "template": "bare", "intent": "i1", "title": "Auth", "purpose": "Check tokens", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Check tokens", "agent": "s-imp" })).unwrap();
    imp.append("", "task.started", json!({ "unit": "u1", "task": 1 })).unwrap();
    imp.append("", "task.delivered", json!({ "unit": "u1", "task": 1, "note": "Done" })).unwrap();
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "gate", "gate": "test", "title": "Read the plan", "inputs": [{ "from": "1" }] })).unwrap();
    let plan = tmp.path().join("plan.md");
    std::fs::write(&plan, b"# Plan B\n").unwrap();
    let id = imp.upload(&plan, "", Some(("u1", 2, None))).unwrap()["id"].as_str().unwrap().to_string();
    let gate = |ws: &serde_json::Value| ws["intents"][0]["units"][0]["tasks"].as_array().unwrap().iter().find(|t| t["id"] == "2").cloned().unwrap();
    let names = |g: &serde_json::Value| g["output"]["attachments"].as_array().unwrap().iter().map(|a| (a["name"].as_str().unwrap().to_string(), a["url"].as_str().unwrap().to_string())).collect::<Vec<_>>();
    let waiting = gate(&user.get("/api/workspace").unwrap());
    assert_eq!(waiting["state"], "waiting");
    assert_eq!(names(&waiting), [("plan.md".to_string(), format!("/api/attachments/{id}"))]);

    // A blind review running: the gate and its file are not in what it reads.
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "review", "reviewer": "security-review", "inputs": [{ "from": "1" }], "agent": "s-sec", "blind": true })).unwrap();
    sec.append("", "task.started", json!({ "unit": "u1", "task": 3 })).unwrap();
    let blind = sec.get("/api/workspace").unwrap().to_string();
    assert!(!blind.contains(&id) && !blind.contains("Read the plan"), "{blind}");

    // Submitted with a picture of the user's: the agent's file stays, the picture after it.
    let shot = tmp.path().join("shot.png");
    std::fs::write(&shot, b"\x89PNG user").unwrap();
    let mine = user.upload(&shot, "", None).unwrap()["id"].as_str().unwrap().to_string();
    user.call("/api/gate", Some(json!({ "unit": "u1", "task": 2, "result": "done", "note": "Read", "findings": [], "attachments": [mine] }))).unwrap();
    let done = gate(&user.get("/api/workspace").unwrap());
    assert_eq!(done["state"], "done");
    assert_eq!(names(&done).into_iter().map(|(n, _)| n).collect::<Vec<_>>(), ["plan.md", "shot.png"]);
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

    let store = opened(&tmp.path().join("w.db"));
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let port = listener.local_addr().unwrap().port();
    let options = crate::server::Options { allow_as: true, ..kernel.options() };
    std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, std::sync::Arc::new(workriver_core::runtime::NoRuntime), options)));
    let d = kernel.client(port, tmp.path());
    let (one, other) = (one.to_string_lossy().to_string(), other.to_string_lossy().to_string());
    for (path, title) in [(&one, "One"), (&other, "Other")] {
        d.append("user", "project.added", json!({ "path": path, "title": title })).unwrap();
    }
    for (title, project) in [("Search", &one), ("Login", &one), ("Elsewhere", &other)] {
        d.append("user", "intent.created", json!({ "title": title, "project": project })).unwrap();
    }
    for (intent, r, b) in [("i1", &one, "feat/one"), ("i2", &one, "feat/two"), ("i1", &one, "feat/three"), ("i2", &one, "feat/four"), ("i2", &one, "feat/five"), ("i3", &other, "feat/six")] {
        d.append("user", "unit.created", json!({ "template": "bare", "intent": intent, "title": b, "repo": r, "branch": b, "base": "main" })).unwrap();
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

    // Its intent dropped, u2 is no work to coordinate with any more; its units unchanged.
    d.append("user", "intent.state_changed", json!({ "intent": "i2", "state": "dropped", "reason": "later" })).unwrap();
    let ws = d.get("/api/workspace").unwrap();
    for id in ["u1", "u2"] {
        let u = ws["intents"].as_array().unwrap().iter().flat_map(|i| i["units"].as_array().unwrap()).find(|u| u["id"] == id).unwrap();
        assert_eq!(u.get("overlaps").cloned().unwrap_or(json!([])), json!([]), "{id}");
    }
}

/// Two units compare their net changes, as their code trees show them: a file the base
/// brought into both (each merged it in) is neither's; a file one changed and then put back
/// in its work tree, byte for byte (not UTF-8 either), is not that unit's; and so it stays
/// when the base moves on with the branches where they were.
#[test]
fn overlaps_are_of_the_net_change() {
    let tmp = tempfile::tempdir().unwrap();
    let r = tmp.path().join("repo");
    std::fs::create_dir(&r).unwrap();
    git(&r, &["init", "-q", "-b", "main"]);
    let latin = |s: &str| [b"caf\xe9 ".as_slice(), s.as_bytes(), b"\n"].concat();
    std::fs::write(r.join("a.txt"), "a\n").unwrap();
    std::fs::write(r.join("r.txt"), "r\n").unwrap();
    std::fs::write(r.join("e9.txt"), latin("base")).unwrap();
    git(&r, &["add", "."]);
    git(&r, &["commit", "-q", "-m", "base"]);
    for name in ["one", "two"] {
        git(&r, &["switch", "-q", "-c", &format!("feat/{name}"), "main"]);
        std::fs::write(r.join("a.txt"), format!("a {name}\n")).unwrap();
        std::fs::write(r.join("r.txt"), format!("r {name}\n")).unwrap();
        std::fs::write(r.join("e9.txt"), latin(name)).unwrap();
        std::fs::write(r.join(format!("{name}.txt")), "mine\n").unwrap();
        git(&r, &["add", "."]);
        git(&r, &["commit", "-q", "-m", name]);
    }
    // The base moves on, and both branches take it in.
    git(&r, &["switch", "-q", "main"]);
    std::fs::write(r.join("main-only.txt"), "base\n").unwrap();
    git(&r, &["add", "."]);
    git(&r, &["commit", "-q", "-m", "base moves"]);
    for name in ["one", "two"] {
        git(&r, &["switch", "-q", &format!("feat/{name}")]);
        git(&r, &["merge", "-q", "--no-ff", "-m", "take main", "main"]);
    }
    git(&r, &["switch", "-q", "main"]);
    // one, in a work tree of its own, puts r.txt and e9.txt back as they were.
    let work = tmp.path().join("one-work");
    git(&r, &["worktree", "add", "-q", &work.to_string_lossy(), "feat/one"]);
    std::fs::write(work.join("r.txt"), "r\n").unwrap();
    std::fs::write(work.join("e9.txt"), latin("base")).unwrap();

    let store = opened(&tmp.path().join("w.db"));
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let port = listener.local_addr().unwrap().port();
    let options = crate::server::Options { allow_as: true, ..kernel.options() };
    std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, std::sync::Arc::new(workriver_core::runtime::NoRuntime), options)));
    let d = kernel.client(port, tmp.path());
    let repo = r.to_string_lossy().to_string();
    d.append("user", "project.added", json!({ "path": repo, "title": "Repo" })).unwrap();
    for title in ["Search", "Login"] {
        d.append("user", "intent.created", json!({ "title": title, "project": repo })).unwrap();
    }
    for (intent, b) in [("i1", "feat/one"), ("i2", "feat/two")] {
        d.append("user", "unit.created", json!({ "template": "bare", "intent": intent, "title": b, "repo": repo, "branch": b, "base": "main" })).unwrap();
    }
    let overlaps = || {
        let ws = d.get("/api/workspace").unwrap();
        ["u1", "u2"].map(|id| ws["intents"].as_array().unwrap().iter().flat_map(|i| i["units"].as_array().unwrap()).find(|u| u["id"] == id).unwrap().get("overlaps").cloned().unwrap_or(json!([])))
    };
    let want = [json!([{ "path": "a.txt", "units": ["u2"] }]), json!([{ "path": "a.txt", "units": ["u1"] }])];
    assert_eq!(overlaps(), want);
    // The base moves on again; the branches stay where they are.
    std::fs::write(r.join("later.txt"), "later\n").unwrap();
    git(&r, &["add", "."]);
    git(&r, &["commit", "-q", "-m", "base moves again"]);
    assert_eq!(overlaps(), want);
}

/// A task says which harness and model did it and where the harness keeps the session: on
/// the page and in `task show`, as the agent was first seen working (its directory is kept),
/// with the model it started the task with. Only workriver says what an agent is seen as,
/// and a blind agent reads where its own session is, nobody else's.
#[test]
fn a_task_shows_who_did_it_and_where_its_session_is() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let home = tmp.path().join("home");
    let day = home.join(".codex/sessions/2026/11/22");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::write(day.join("rollout-2026-11-22T10-00-00-0c0d-ex.jsonl"), "").unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    let at_dir = |session: &str, name: &str, harness: &str, cwd: &str| {
        let mut a = live(session, name, "w1", Idle);
        (a.harness, a.cwd) = (harness.into(), cwd.into());
        a
    };
    *fake.agents.lock().unwrap() = vec![
        ("p1".into(), live("s-boss", "boss", "w1", Idle)),
        ("p2".into(), at_dir("s-fix", "fix", "claude", "")),
        ("p3".into(), at_dir("0c0d-ex", "cx", "codex", "/w/cx")),
        ("p4".into(), at_dir("s-sec", "sec", "claude", "/w/sec")),
        ("p5".into(), { let mut a = at_dir("s-far", "far", "claude", "/far/repo"); a.machine = Some("box".into()); a }),
    ];
    let store = opened(&tmp.path().join("w.db"));
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let port = listener.local_addr().unwrap().port();
    let runtime = fake.clone();
    let options = crate::server::Options { user_home: Some(home.clone()), ..kernel.options() };
    std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, runtime, options)));
    let user = kernel.client(port, tmp.path());
    let at = |place: &str| kernel.daemon(port, Some(place.into()), None);
    let (boss, fix, cx, sec) = (at("p1"), at("p2"), at("p3"), at("p4"));
    let ws = seat_at(&user, &boss);
    user.append("", "intent.created", json!({ "title": "Login", "state": "planned" })).unwrap();
    user.append("", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": ws })).unwrap();
    boss.append("", "unit.created", json!({ "template": "bare", "intent": "i1", "title": "Auth", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    for (title, agent) in [("Check tokens", "s-fix"), ("Expire tokens", "0c0d-ex")] {
        boss.append("", "task.created", json!({ "unit": "u1", "kind": "implement", "title": title, "agent": agent, "inputs": [{ "from": "start" }] })).unwrap();
    }

    // Assigned and not started: no trace (its agent's session is still other work's).
    let shown = |who: &crate::cli::Daemon, n: u32| serde_json::from_value::<crate::common::wire::TaskShow>(who.get(&format!("/api/task?task=u1%23{n}")).unwrap()).unwrap();
    assert_eq!(shown(&user, 1).trace, None);
    assert!(user.get("/api/workspace").unwrap()["intents"][0]["units"][0]["tasks"][0].get("trace").is_none());

    // Seen first with no directory: its file is not known until one is seen, and the first
    // one seen is kept (a harness keeps the session by the directory it began in).
    fix.append("", "task.started", json!({ "unit": "u1", "task": 1, "model": "claude-opus-5-5" })).unwrap();
    let trace = |who: &crate::cli::Daemon, n: u32| shown(who, n).trace.unwrap();
    assert_eq!(trace(&user, 1).unknown, Some(crate::common::wire::NoFile::Cwd));
    for cwd in ["/w/fix_1.x", "/w/elsewhere"] {
        fake.agents.lock().unwrap()[1].1.cwd = cwd.into();
        fix.get("/api/task?task=u1%231").unwrap();
    }
    let t = trace(&user, 1);
    let file = home.join(".claude/projects/-w-fix-1-x/s-fix.jsonl").display().to_string();
    assert_eq!((t.harness.as_deref(), t.model.as_deref(), t.session.as_str(), t.file.as_deref(), t.machine), (Some("claude"), Some("claude-opus-5-5"), "s-fix", Some(file.as_str()), None));
    let state: workriver_core::State = serde_json::from_value(user.get("/api/state").unwrap()).unwrap();
    assert_eq!(state.agents.iter().filter(|a| a.session == "s-fix").map(|a| a.cwd.as_str()).collect::<Vec<_>>(), ["/w/fix_1.x"]);
    let text = crate::cli::text(&serde_json::from_value(user.get("/api/task?task=u1%231").unwrap()).unwrap());
    assert!(t.unconfirmed, "no such file in this home: worked out only");
    // The text says the harness and the model only; the session and its file are kept for
    // `--json` and `task session`.
    assert!(text.contains("Agent: fix\n  harness claude, model claude-opus-5-5\n"), "{text}");
    assert!(!text.contains("s-fix") && !text.contains(".jsonl"), "{text}");

    // Codex's file is found by its id; a start that says no model has none.
    // Nobody says another agent's model for it: a start recorded for it carries none.
    let e = user.append("", "task.started", json!({ "unit": "u1", "task": 2, "agent": "cx", "model": "gpt-x" })).unwrap_err().to_string();
    assert!(e.contains("--model"), "{e}");
    cx.append("", "task.started", json!({ "unit": "u1", "task": 2 })).unwrap();
    let t = trace(&user, 2);
    assert_eq!((t.harness.as_deref(), t.model, t.file), (Some("codex"), None, Some(day.join("rollout-2026-11-22T10-00-00-0c0d-ex.jsonl").display().to_string())));
    // The page reads the harness and the model only: no session, no file.
    let ws = user.get("/api/workspace").unwrap();
    let tasks = &ws["intents"][0]["units"][0]["tasks"];
    assert_eq!((&tasks[0]["trace"], &tasks[1]["trace"]), (&json!({ "harness": "claude", "model": "claude-opus-5-5" }), &json!({ "harness": "codex" })));

    // Only workriver says what an agent is seen as: not an agent, not the user.
    for who in [&fix, &user] {
        let e = who.append("", "agent.seen", json!({ "session": "s-fix", "name": "fix", "harness": "claude", "cwd": "/w/other" })).unwrap_err().to_string();
        assert!(e.contains("only workriver records this (agent.seen)"), "{e}");
    }
    assert!(fix.append("", "task.started", json!({ "unit": "u1", "task": 1, "model": "  " })).is_err());

    // A blind agent: where its own session is, nothing of anyone else's.
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "review", "reviewer": "security-review", "inputs": [{ "from": "1" }], "agent": "s-sec", "blind": true })).unwrap();
    sec.append("", "task.started", json!({ "unit": "u1", "task": 3, "model": "claude-sonnet-5-5" })).unwrap();
    assert_eq!(trace(&sec, 3).file, Some(home.join(".claude/projects/-w-sec/s-sec.jsonl").display().to_string()));
    assert!(sec.get("/api/task?task=u1%231").is_err());
    let seen = sec.get("/api/workspace").unwrap().to_string() + &sec.get("/api/state").unwrap().to_string();
    for hidden in ["/w/fix", "/w/cx", "-w-fix", "0c0d-ex.jsonl", "/far/repo", "\"box\""] {
        assert!(!seen.contains(hidden), "{hidden} is hidden from a blind agent");
    }
    // Others by name only: the far agent is listed, not where it runs.
    let far = sec.get("/api/workspace").unwrap()["sessions"].as_array().unwrap().iter().find(|x| x["session"] == "s-far").cloned().unwrap();
    assert_eq!((far["agent"].as_str(), far.get("machine")), (Some("far"), None));
    assert_eq!(user.get("/api/workspace").unwrap()["sessions"].as_array().unwrap().iter().find(|x| x["session"] == "s-far").unwrap()["machine"], "box");
}
