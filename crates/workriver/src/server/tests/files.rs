// Attachments through the service.
use std::io::{Read, Write};

use serde_json::json;

use super::support::*;

/// Files go into the store and come back with their type: images inline, anything else as a
/// download, never sniffed, and only for this machine's Host. Delivering a task takes its
/// folder's files (one named after a finding goes with it), a finding's own files and a
/// check's file; the manager's message lists their paths.
#[test]
fn attachments_round_trip() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle))];
    let home = tmp.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let d = daemon_with(fake.clone(), &home.join("w.db"), Default::default());
    manage(&d, "p1", "s-boss");
    d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "M", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "tests", "agent": "s-t" })).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500));

    // Routes: an image and a text file.
    let files = tmp.path().join("files");
    std::fs::create_dir(&files).unwrap();
    std::fs::write(files.join("shot.png"), b"\x89PNG fake").unwrap();
    std::fs::write(files.join("notes.txt"), b"<script>alert(1)</script>").unwrap();
    let png = d.upload(&files.join("shot.png"), "s-t", None).unwrap();
    let txt = d.upload(&files.join("notes.txt"), "s-t", Some(("u1", 1, None))).unwrap();
    assert!(png["path"].as_str().unwrap().starts_with(home.join("attachments").to_str().unwrap()));
    let get = |id: &str, host: &str| {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", d.port)).unwrap();
        let token = crate::common::home::read_token(&home).unwrap();
        let proof = d.process(false).0.unwrap();
        write!(s, "GET /api/attachments/{id} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {token}\r\nX-Workriver-Caller: {proof}\r\nConnection: close\r\n\r\n").unwrap();
        let mut answer = vec![];
        s.read_to_end(&mut answer).unwrap();
        String::from_utf8_lossy(&answer).to_lowercase()
    };
    let ours = format!("127.0.0.1:{}", d.port);
    let image = get(png["id"].as_str().unwrap(), &ours);
    assert!(image.contains("content-type: image/png") && image.contains("content-disposition: inline") && image.contains("x-content-type-options: nosniff"), "{image}");
    let text = get(txt["id"].as_str().unwrap(), &ours);
    assert!(text.contains("content-type: application/octet-stream") && text.contains("content-disposition: attachment; filename=\"notes.txt\""), "{text}");
    assert!(get(png["id"].as_str().unwrap(), "evil.example").starts_with("http/1.1 403"));

    // Delivering: the folder's files, a finding's own file and a check's file.
    let inbox = d.inbox("u1", 1).unwrap();
    std::fs::write(inbox.join("P2-1-login.png"), b"login shot").unwrap();
    std::fs::write(inbox.join("overview.png"), b"overview shot").unwrap();
    std::fs::write(files.join("trace.txt"), b"trace").unwrap();
    std::fs::write(files.join("report.html"), b"<p>report</p>").unwrap();
    let findings = files.join("f.json");
    let trace = files.join("trace.txt");
    std::fs::write(&findings, json!([{ "severity": "P2", "problem": "Login fails", "impact": "No entry", "where": ["login.ts"], "attachments": [trace] }]).to_string()).unwrap();
    let evidence = format!("npx playwright test => failed: 1 failed => {}", files.join("report.html").display());
    crate::cli::deliver(&d, "s-t", "u1#1", "See shots".into(), &[evidence], Some(findings.to_string_lossy().into_owned()), false).unwrap();
    assert_eq!(std::fs::read_dir(&inbox).unwrap().count(), 0, "the folder is emptied");
    let state: workriver_core::State = serde_json::from_value(d.get("/api/state").unwrap()).unwrap();
    let t = state.task("u1", 1).unwrap();
    let named = |ids: &[String]| ids.iter().map(|id| state.attachments.iter().find(|a| &a.id == id).unwrap().name.clone()).collect::<Vec<_>>();
    assert_eq!(named(&t.attachments), ["notes.txt", "overview.png"]);
    assert_eq!(named(&t.findings[0].found.attachments), ["trace.txt", "P2-1-login.png"]);
    assert_eq!(named(std::slice::from_ref(t.evidence[0].attachment.as_ref().unwrap())), ["report.html"]);
    // The manager is told where the files are.
    wait_until("the manager hears of the delivery", || fake.sent.lock().unwrap().iter().any(|(to, m)| to == "s-boss" && m.contains("Attachments:")));
    let told = fake.sent.lock().unwrap().iter().find(|(to, m)| to == "s-boss" && m.contains("Attachments:")).unwrap().1.clone();
    assert!(told.contains(&home.join("attachments").display().to_string()) && told.contains(".png"), "{told}");
    // The page gets previews and links.
    let ws = d.get("/api/workspace").unwrap();
    let out = &ws["intents"][0]["units"][0]["tasks"][0]["output"];
    assert_eq!(out["attachments"][1]["type"], "image/png");
    assert!(out["attachments"][1]["url"].as_str().unwrap().starts_with("/api/attachments/"));
    assert_eq!(out["findings"][0]["attachments"][1]["name"], "P2-1-login.png");
    assert_eq!(out["evidence"][0]["attachment"]["name"], "report.html");
}

/// A task's inbox folder (`workriver attach --dir`) is made and named only for someone: the
/// user, or an agent from its own place (here, or on another machine, where the CLI makes it
/// itself). A process that is nobody learns nothing, not even whether the task exists.
#[test]
fn the_inbox_asks_who_calls() {
    use workriver_core::runtime::AgentState::*;
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

    let ask = json!({ "unit": "u1", "task": 1 }).to_string();
    let folder = crate::common::home::inbox(tmp.path(), "u1", 1);
    for (port, name) in [(port, "this machine"), (all.local_port("box").unwrap(), "another machine")] {
        let (code, text) = http(port, "POST", "/api/attachments/inbox", &[], Some(&ask));
        assert!(code == 401 && !text.contains("inbox"), "{name}: nobody got {code} {text}");
        // Not even whether a task exists.
        let (other, _) = http(port, "POST", "/api/attachments/inbox", &[], Some(&json!({ "unit": "u9", "task": 9 }).to_string()));
        assert_eq!(other, code, "{name}");
    }
    assert!(!folder.exists(), "made for nobody");
    assert_eq!(boss.call("/api/attachments/inbox", Some(json!({ "unit": "u1", "task": 1 }))).unwrap()["dir"], json!(folder));
    assert!(folder.is_dir());
    std::fs::remove_dir(&folder).unwrap();
    assert_eq!(user.call("/api/attachments/inbox", Some(json!({ "unit": "u1", "task": 1 }))).unwrap()["dir"], json!(folder));
    assert!(folder.is_dir());
    // Over there the CLI makes the folder on its own disk.
    assert_eq!(far.call("/api/attachments/inbox", Some(json!({ "unit": "u1", "task": 1 }))).unwrap(), json!({ "elsewhere": true }));
    assert!(boss.call("/api/attachments/inbox", Some(json!({ "unit": "u9", "task": 1 }))).unwrap_err().to_string().contains("u9"));
}

/// Two uploads of the same content at once (the same file attached twice to a question, sent
/// together): both are taken, and the store holds it once.
#[test]
fn identical_uploads_at_once_both_succeed() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    let d = daemon_with(fake, &tmp.path().join("w.db"), Default::default());
    let file = tmp.path().join("big.bin");
    for round in 0..5u8 {
        std::fs::write(&file, vec![round; 8 * 1024 * 1024]).unwrap();
        let start = std::sync::Barrier::new(2);
        let got: Vec<_> = std::thread::scope(|s| {
            let hs: Vec<_> = (0..2).map(|_| s.spawn(|| { start.wait(); d.upload(&file, "user", None) })).collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let ids: Vec<_> = got.iter().map(|r| r.as_ref().map(|v| v["id"].clone()).map_err(|e| e.to_string())).collect();
        assert!(ids[0].is_ok() && ids[0] == ids[1], "round {round}: {ids:?}");
    }
    assert_eq!(d.get("/api/status").unwrap()["attachments"]["files"], 5);
}
