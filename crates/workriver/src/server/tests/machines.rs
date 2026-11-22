// Other machines: followed, their agents in workriver's workspaces, their connection.
use serde_json::json;

use super::support::*;

/// Two machines with the same pane, runtime group id and label: everything from the other
/// machine carries its name, messages go where the agent is, a machine that cannot be
/// reached leaves the rest working, and what the runtimes call their groups joins nothing:
/// an agent over there is in a workspace by the user's seating or by its tasks, with no
/// link. A caller on the other machine's listener is an agent of that machine, whatever it
/// says.
#[test]
fn machines_are_separate_and_need_no_link() {
    use workriver_core::runtime::{AgentState::*, Runtime};
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let local = std::sync::Arc::new(Fake::default());
    local.strict.store(true, std::sync::atomic::Ordering::Relaxed);
    *local.agents.lock().unwrap() = vec![("w1:p1".into(), live("s-boss", "boss", "w1", Idle))];
    let far = std::sync::Arc::new(Fake::default());
    *far.agents.lock().unwrap() = vec![("w1:p1".into(), live("s-far", "far", "w1", Idle))];
    let all = crate::runtime::follow::Machines::with(local.clone(), vec![("box", far.clone()), ("gone", std::sync::Arc::new(Broken))]);

    let agents = all.agents().unwrap();
    assert_eq!(agents.len(), 2, "the machine that cannot be reached does not take the others with it");
    let a = agents.iter().find(|a| a.session == "s-far").unwrap();
    let g = a.group.clone().unwrap();
    assert_eq!((a.machine.as_deref(), g.id.as_str(), g.machine.as_deref()), (Some("box"), "w1@box", Some("box")));
    let states = all.states();
    assert_eq!(states.iter().map(|m| (m.label.as_str(), m.reachable)).collect::<Vec<_>>(), [("box", true), ("gone", false)]);
    assert!(states[1].error.as_deref().unwrap().contains("timed out"));
    // The same pane id on each machine is a different agent.
    assert_eq!(all.locate(None, "w1:p1").unwrap().agent.unwrap().session, "s-boss");
    assert_eq!(all.locate(Some("box"), "w1:p1").unwrap().agent.unwrap().session, "s-far");
    // Messages and new agents go to the agent's own machine, with that machine's own ids.
    all.send("s-far", "hi").unwrap();
    all.send("s-boss", "ho").unwrap();
    assert_eq!((far.sent.lock().unwrap().len(), local.sent.lock().unwrap().len()), (1, 1));
    all.start_temporary("w1@box", "claude", "why-q9", "/srv/x").unwrap();
    assert_eq!(far.started.lock().unwrap()[0].0, "w1");
    all.send("why-q9", "by name, before the next read").unwrap();
    assert_eq!(far.sent.lock().unwrap().len(), 2);
    all.close("why-q9").unwrap();
    all.poll_all();

    let store = workriver_data::Store::open(&tmp.path().join("w.db")).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let d = kernel.client(listener.local_addr().unwrap().port(), tmp.path());
    let (runtime, machines) = (all.clone(), all.clone());
    let options = crate::server::Options { machines: Some(machines), allow_as: true, ..kernel.options() };
    std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, runtime, options)));
    wait_until("the machine's listener", || all.local_port("box").is_some());
    // A caller over there has no user token.
    let via_box = kernel.daemon(all.local_port("box").unwrap(), None, None);

    // Who a caller is depends on the listener it came in on, not on what it claims.
    manage(&d, "w1:p1", "s-boss");
    assert_eq!(at_place(&d, "w1:p1").call("/api/here", Some(json!({}))).unwrap()["actor"], "s-boss");
    let there = at_place(&via_box, "w1:p1").call("/api/here", Some(json!({}))).unwrap();
    assert_eq!((there["actor"].as_str(), there["group"]["id"].as_str(), there["intent"].as_str(), there["workspace"].is_null()), (Some("s-far"), Some("w1@box"), None, true), "the same group name over there is no workspace");
    assert!(via_box.call("/api/machines", Some(json!({ "label": "x", "action": "add", "ssh": "x" }))).is_err(), "machines are managed locally");

    // The user makes a second workspace and seats the agent over there in it: a manager on
    // another machine holds a seat like any other. It asks with options; the question is
    // open in the page, and the user's answer, sent as the page sends it (the question's
    // workspace by its id), goes to its asker.
    let far_ws = workspace_named(&d, "far");
    let seated = seat(&d, &far_ws, Some("s-far")).unwrap();
    assert_eq!((seated["body"]["manager"].as_str(), seated["body"]["machine"].as_str()), (Some("s-far"), Some("box")), "{seated}");
    wait_until("the far manager is told of its seat", || far.sent.lock().unwrap().iter().any(|(to, m)| to == "s-far" && m.contains(&format!("made you the manager of workspace {far_ws}"))));
    let choice = crate::cli::parse_choice(&["a=Here".into(), "b=There".into()], &[]).unwrap();
    let said = via_box.append("s-far", "message.said", json!({ "text": "Which?", "choice": choice })).unwrap();
    assert_eq!(said["body"]["workspace"], far_ws.as_str());
    let id = said["body"]["id"].as_str().unwrap().to_string();
    let asked = || d.get("/api/workspace").unwrap()["messages"].as_array().unwrap().iter().find(|m| m["id"] == id.as_str()).unwrap().clone();
    assert_eq!(asked()["open"], true, "{}", asked());
    let sent = d.call("/api/message", Some(json!({ "workspace": far_ws, "text": "", "answer": { "message": id, "selected": ["b"] } }))).unwrap();
    assert_eq!(sent["body"]["manager"], "s-far", "not the manager of the other workspace");
    wait_until("the answer reaches the far manager", || far.sent.lock().unwrap().iter().any(|(to, m)| to == "s-far" && m.contains(&format!("The user answered your question {id}"))));
    assert!(asked().get("open").is_none() && asked()["answered"]["selected"] == json!(["b"]));

    // A unit is its intent's manager's to add, not any agent's over there; one the manager
    // puts on the other machine has its repository there.
    let far_unit = json!({ "intent": "i1", "title": "Far", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" });
    let refused = via_box.append("s-far", "unit.created", far_unit.clone()).unwrap_err().to_string();
    assert!(refused.contains("only the user or the manager of intent i1 (boss) adds its units"), "{refused}");
    let unit = d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "Far", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main", "machine": "box" })).unwrap();
    assert_eq!(unit["body"]["machine"], "box");
    std::thread::sleep(std::time::Duration::from_millis(2500)); // a poll sees the agents
    // A review delivered there reaches the manager here; a file it attached is, for an agent
    // over there, a link through its tunnel, never a path on this machine.
    via_box.append("s-far", "task.created", json!({ "unit": "u1", "kind": "review" })).unwrap();
    let shot = tmp.path().join("shot.png");
    std::fs::write(&shot, b"\x89PNG far").unwrap();
    let up = via_box.upload(&shot, "s-far", None).unwrap();
    assert!(up["path"].as_str().unwrap().starts_with("http://127.0.0.1:$(cat ~/.config/workriver/port)/api/attachments/"), "{up}");
    via_box.append("s-far", "task.delivered", json!({ "unit": "u1", "task": 1, "findings": [{ "severity": "P2", "problem": "p", "impact": "i", "where": ["a"], "attachments": [up["id"]] }] })).unwrap();
    let told = local.sent.lock().unwrap().last().unwrap().clone();
    assert!(told.0 == "s-boss" && told.1.contains("delivered by far") && told.1.contains(&tmp.path().join("attachments").display().to_string()), "{told:?}");
    // The far agent does not make itself the manager of the workspace the intent runs in;
    // the user seats it there (one workspace, one manager; one session, one seat): decisions
    // then go to its machine.
    let err = via_box.append("s-far", "workspace.manager_set", json!({ "workspace": "ws1", "manager": "s-far", "replaces": seat_of(&d, "ws1") })).unwrap_err().to_string();
    assert!(err.contains("only the user does this (workspace.manager_set)"), "{err}");
    local.agents.lock().unwrap().clear();
    all.poll_all();
    seat(&d, "ws1", Some("s-far")).unwrap();
    assert!(seat_of(&d, &far_ws).is_null(), "its old seat is empty");
    d.call("/api/decide", Some(json!({ "unit": "u1", "task": 1, "finding": "P2-1", "decision": "accept" }))).unwrap();
    let got = far.sent.lock().unwrap().last().unwrap().clone();
    assert!(got.0 == "s-far" && got.1.contains("Attachments: `workriver attach fetch ") && got.1.contains(".png` (run it: the file is saved in the folder you are in)") && !got.1.contains("curl") && !got.1.contains(&tmp.path().display().to_string()), "{got:?}");
    // The user's message with a file attached: to that manager, the file is a fetch command too.
    let mine = d.upload(&shot, "user", None).unwrap();
    d.call("/api/message", Some(json!({ "intent": "i1", "text": "See this.", "context": [{ "kind": "attachment", "label": "shot.png", "id": mine["id"] }] }))).unwrap();
    wait_until("the message reaches the far manager", || far.sent.lock().unwrap().last().is_some_and(|(_, m)| m.contains("See this.")));
    let got = far.sent.lock().unwrap().last().unwrap().clone();
    assert!(got.1.contains("the attachment \"shot.png\" at `workriver attach fetch ") && !got.1.contains(&tmp.path().display().to_string()), "{got:?}");

    let ws = d.get("/api/workspace").unwrap();
    assert_eq!(ws["intents"][0]["units"][0]["machine"], "box");
    let s = ws["sessions"].as_array().unwrap().iter().find(|s| s["agent"] == "far").unwrap();
    assert_eq!((s["machine"].as_str(), s["group"]["id"].as_str(), s["workspace"].as_str(), s["manager"].as_bool()), (Some("box"), Some("w1@box"), Some("ws1"), Some(true)), "in the workspace it holds the seat of; its runtime group is only where its terminal is");
    let seats: Vec<_> = ws["workspaces"].as_array().unwrap().iter().map(|w| (w["id"].as_str(), w["name"].as_str(), w["manager"]["session"].as_str(), w["manager"]["machine"].as_str())).collect();
    assert_eq!(seats, [(Some("ws1"), Some("chat"), Some("s-far"), Some("box")), (Some("ws2"), Some("far"), None, None)]);
    assert_eq!(ws["machines"].as_array().unwrap().len(), 2);
}

/// This machine's runtime not answering while another machine's does: its agents are as
/// last seen, in no state of theirs, like a machine that cannot be reached, and the read is
/// known not to be complete; once it answers they are read fresh again. With no other
/// machine followed, its failure is the runtime's.
#[test]
fn this_machines_agents_are_as_last_seen_while_its_runtime_does_not_answer() {
    use std::sync::atomic::Ordering::SeqCst;
    use workriver_core::runtime::{AgentState::*, Runtime};
    let local = std::sync::Arc::new(Fake::default());
    *local.agents.lock().unwrap() = vec![("w1:p1".into(), live("s-boss", "boss", "w1", Idle))];
    let far = std::sync::Arc::new(Fake::default());
    *far.agents.lock().unwrap() = vec![("w1:p1".into(), live("s-far", "far", "w1", Idle))];
    let all = crate::runtime::follow::Machines::with(local.clone(), vec![("box", far)]);
    let states = |all: &crate::runtime::follow::Machines| all.agents().unwrap().into_iter().map(|a| (a.session, a.state)).collect::<Vec<_>>();
    assert_eq!(states(&all), [("s-boss".to_string(), Idle), ("s-far".to_string(), Idle)]);
    assert!(all.here_read());
    local.down.store(true, SeqCst);
    assert_eq!(states(&all), [("s-boss".to_string(), Unknown), ("s-far".to_string(), Idle)]);
    assert!(!all.here_read());
    local.down.store(false, SeqCst);
    assert_eq!(states(&all), [("s-boss".to_string(), Idle), ("s-far".to_string(), Idle)]);
    assert!(all.here_read());
    let alone = crate::runtime::follow::Machines::with(local.clone(), vec![]);
    local.down.store(true, SeqCst);
    assert!(alone.agents().is_err() && !alone.here_read());
}

/// A worker on another machine that started since that machine's agents were last read is
/// told of a requirement added to its task all the same, once, and a file that came with it
/// is the command that fetches it there, never a path on this machine. Looking for it is
/// not the request's to wait for: the command is answered at once (recorded, its agent not
/// told yet), and the worker is told as soon as its own machine is read, however long other
/// machines take to answer.
#[test]
fn a_task_note_reaches_a_worker_on_another_machine_at_once() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let local = std::sync::Arc::new(Fake::default());
    local.strict.store(true, std::sync::atomic::Ordering::Relaxed);
    *local.agents.lock().unwrap() = vec![("w1:p1".into(), live("s-boss", "boss", "w1", Idle))];
    let far = std::sync::Arc::new(Fake::default());
    far.strict.store(true, std::sync::atomic::Ordering::Relaxed);
    // Two more machines, nothing to do with the task, that take seconds to be read.
    let slow = std::sync::Arc::new(Slow::default());
    // Read once here, with nobody there; no thread reads it again (the test does, below).
    let all = crate::runtime::follow::Machines::with(local.clone(), vec![("slow1", slow.clone()), ("box", far.clone()), ("slow2", slow.clone())]);
    slow.wait.store(3000, std::sync::atomic::Ordering::Relaxed);
    let store = workriver_data::Store::open(&tmp.path().join("w.db")).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let d = kernel.client(listener.local_addr().unwrap().port(), tmp.path());
    let (runtime, machines) = (all.clone(), all.clone());
    let options = crate::server::Options { machines: Some(machines), allow_as: true, ..kernel.options() };
    std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, runtime, options)));
    wait_until("the machine's listener", || all.local_port("box").is_some());
    let via_box = kernel.daemon(all.local_port("box").unwrap(), None, None);
    manage(&d, "w1:p1", "s-boss");
    d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "Far", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Drain" })).unwrap();

    // The worker comes up over there and starts its task through that machine's listener.
    far.agents.lock().unwrap().push(("w1:p2".into(), live("s-far", "far", "w1", Working)));
    at_place(&via_box, "w1:p2").append("", "task.started", json!({ "unit": "u1", "task": 1 })).unwrap();
    let shot = tmp.path().join("shot.png");
    std::fs::write(&shot, b"\x89PNG far").unwrap();
    let up = d.upload(&shot, "s-boss", None).unwrap();
    let asked = std::time::Instant::now();
    let out = d.append("s-boss", "task.noted", json!({ "unit": "u1", "task": 1, "text": "Also the queue", "attachments": [up["id"]] })).unwrap();
    assert!(asked.elapsed() < std::time::Duration::from_secs(1), "answered after {:?}", asked.elapsed());
    assert_eq!(crate::cli::made(&out), "added to u1#1; not told yet: far is not seen running: it is told once it is found on a machine workriver follows");
    let added = || far.sent.lock().unwrap().iter().filter(|(_, m)| m.contains("added requirement")).cloned().collect::<Vec<_>>();
    wait_until("the worker is told", || !added().is_empty());
    assert!(asked.elapsed() < std::time::Duration::from_millis(2500), "told after {:?}: not held up by the slow machines", asked.elapsed());
    let m = added();
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(m[0].0 == "s-far" && m[0].1.contains("Attachments: `workriver attach fetch ") && !m[0].1.contains(&tmp.path().display().to_string()), "{m:?}");
    // Once per note, whatever is read of the machine later.
    slow.wait.store(0, std::sync::atomic::Ordering::Relaxed);
    all.poll_all();
    std::thread::sleep(std::time::Duration::from_millis(2500));
    assert_eq!(added().len(), 1, "{:?}", added());
    assert!(local.sent.lock().unwrap().iter().all(|(_, m)| !m.contains("added requirement")));
}

/// The connection to a machine: what ssh is asked for; a port of its own for every
/// connection, written over there once it is up, so a reconnect never waits on the port of
/// a connection that died without closing (here: held for good); down the moment ssh ends;
/// growing waits; ended with its supervisor.
#[test]
fn a_machines_connection_is_kept_up() {
    use crate::runtime::ssh::{Pace, Tunnel, allocated, publish, slept, supervise, tunnel_words};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    // The runtime's sockets as its adapter names them: its API, and next to it the socket of
    // its terminal stream (the page's terminal view of an agent over there).
    let sockets = crate::runtime::forwards(std::path::Path::new("/data"), "box", "/home/me/.config/herdr/herdr.sock");
    let w = tunnel_words("me@box", &sockets, 51234);
    assert_eq!(&w[..2], ["ssh", "-N"]);
    for o in ["BatchMode=yes", "ForwardAgent=no", "StrictHostKeyChecking=yes", "ExitOnForwardFailure=yes", "ConnectTimeout=5", "ServerAliveInterval=10", "ServerAliveCountMax=3", "StreamLocalBindUnlink=yes", "LogLevel=INFO"] {
        assert!(w.windows(2).any(|p| p == ["-o", o]), "{o} in {w:?}");
    }
    // Clearing forwards would clear its own: the tunnel is the one ssh of ours that cannot.
    assert!(!w.iter().any(|o| o.starts_with("ClearAllForwardings")));
    let n = w.len();
    assert_eq!(&w[n - 8..], ["-L", "/data/herdr-box.sock:/home/me/.config/herdr/herdr.sock", "-L", "/data/herdr-box-client.sock:/home/me/.config/herdr/herdr-client.sock", "-R", "127.0.0.1:0:127.0.0.1:51234", "--", "me@box"]);
    // As OpenSSH says it (checked against a real sshd).
    assert_eq!(allocated("Allocated port 57793 for remote forward to 127.0.0.1:9"), Some(57793));
    assert_eq!(allocated("Warning: Permanently added"), None);
    assert!(slept(Duration::from_secs(600), Duration::from_secs(2)) && !slept(Duration::from_secs(3), Duration::from_secs(2)));

    let tmp = tempfile::tempdir().unwrap();
    let ssh = fake_ssh(tmp.path());
    let port_file = tmp.path().join("remote").join(crate::common::home::PORT_FILE);
    let published = || std::fs::read_to_string(&port_file).ok().and_then(|t| t.trim().parse::<u16>().ok());
    // The old way, a fixed port, after a connection died: refused for as long as it is held.
    let fixed: Vec<String> = [ssh.to_str().unwrap(), "-N", "-R", "127.0.0.1:41001:127.0.0.1:1", "--", "box"].iter().map(|s| s.to_string()).collect();
    std::fs::write(tmp.path().join("held"), "41001\n").unwrap();
    let out = std::process::Command::new(&fixed[0]).args(&fixed[1..]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stderr).contains("remote port forwarding failed for listen port 41001"));
    std::fs::remove_file(tmp.path().join("connections")).unwrap();

    let mut words = w.clone();
    words[0] = ssh.display().to_string();
    let tunnel = std::sync::Arc::new(std::sync::Mutex::new(Tunnel::default()));
    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let pace = Pace { min: Duration::from_millis(100), max: Duration::from_millis(400), healthy: Duration::from_secs(60) };
    let (t, s, program) = (tunnel.clone(), stop.clone(), ssh.display().to_string());
    let run = std::thread::spawn(move || supervise(&words, &t, &s, pace, &|port| publish(&program, "box", port, "mac")));
    // Soak: the line dies again and again; each time it is down at once, then up on a new
    // port, written over there, never one of a dead connection.
    let mut ports = vec![];
    // Looked at often: it is down only until the next attempt (`pace.min`, doubling).
    let soon = |what: &str, ok: &dyn Fn(&Tunnel) -> bool| {
        let until = std::time::Instant::now() + Duration::from_secs(10);
        while !ok(&tunnel.lock().unwrap()) {
            assert!(std::time::Instant::now() < until, "{what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    for round in 0..6 {
        soon("up", &|t| t.up);
        let t = tunnel.lock().unwrap().clone();
        let port = t.port.unwrap();
        assert_eq!((published(), t.error.clone(), t.since.clone()), (Some(port), None, None), "round {round}");
        assert!(!ports.contains(&port), "a new port each time: {ports:?} {port}");
        assert_eq!(std::fs::read_to_string(tmp.path().join("remote/.config/workriver/daemon")).unwrap().trim(), "mac");
        ports.push(port);
        std::fs::write(tmp.path().join("drop"), "").unwrap();
        soon("down at once", &|t| !t.up);
        let t = tunnel.lock().unwrap().clone();
        assert_eq!((t.error.as_deref(), t.port, t.since.is_some(), t.retry.is_some()), (Some("Timeout, server not responding."), None, true, true), "round {round}");
    }
    assert_eq!(std::fs::read_to_string(tmp.path().join("held")).unwrap().lines().count(), 7, "every old port still held over there (and 41001)");
    stop.store(true, Ordering::Relaxed);
    run.join().unwrap();
    assert_eq!(*tunnel.lock().unwrap(), Tunnel::default());
    // The stand-in itself is gone too, not left running.
    wait_until("the command ended with its supervisor", || {
        let out = std::process::Command::new("pgrep").args(["-f", &ssh.display().to_string()]).output().unwrap();
        out.stdout.is_empty()
    });

    // Waits grow, up to the cap, while it keeps failing.
    let fails: Vec<String> = ["sh", "-c", "echo 'ssh: connect to host box port 22: Operation timed out' >&2; exit 255"].iter().map(|s| s.to_string()).collect();
    let tunnel = std::sync::Arc::new(std::sync::Mutex::new(Tunnel::default()));
    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let (t, s) = (tunnel.clone(), stop.clone());
    let run = std::thread::spawn(move || supervise(&fails, &t, &s, pace, &|_| Ok(())));
    let mut waits = vec![];
    while waits.len() < 5 {
        let r = tunnel.lock().unwrap().retry;
        if let Some(r) = r {
            let left = r.saturating_duration_since(std::time::Instant::now());
            if waits.last().is_none_or(|w: &Duration| left > *w + Duration::from_millis(30) || left + Duration::from_millis(30) < *w) {
                waits.push(left);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    stop.store(true, Ordering::Relaxed);
    run.join().unwrap();
    assert!(waits.iter().all(|w| *w <= Duration::from_millis(400)), "capped: {waits:?}");

    // As `workriver machine list` and the page see it, from a machine followed for real
    // (through the stand-in): up with its port; down at once, since when, the next attempt.
    std::fs::remove_file(tmp.path().join("connections")).ok();
    let all = crate::runtime::follow::Machines::with_ssh(std::sync::Arc::new(Fake::default()), tmp.path().to_path_buf(), &ssh.display().to_string(), Pace { min: Duration::from_millis(300), ..pace });
    let setting = crate::runtime::Setting { ssh: "box".into(), options: crate::runtime::socket_option(Some("/run/herdr.sock".into())), port: 0, enabled: true };
    all.start("box", setting, 1);
    let state = || all.states().into_iter().next().unwrap();
    wait_until("connected", || state().tunnel);
    assert!(state().port.is_some() && published() == state().port, "the port of this connection is the one written there");
    std::fs::write(tmp.path().join("drop"), "").unwrap();
    wait_until("disconnected", || !state().tunnel);
    let s = state();
    assert!(s.down_since.is_some() && s.retry_in.is_some() && s.port.is_none() && s.tunnel_error.as_deref() == Some("Timeout, server not responding."), "{s:?}");
    assert!(all.runner("box").is_none(), "no git over a dead line");
    wait_until("connected again", || state().tunnel);
    all.stop("box");
}

/// The manager is on this machine, the work on another: the unit is recorded where the work
/// is (that machine's path and branch, checked there over ssh), by the manager of a linked
/// workspace or the user; one recorded in the wrong place is moved. And chores of no intent.
#[test]
fn units_on_another_machine() {
    use std::os::unix::fs::PermissionsExt;
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    // An ssh stand-in that runs the command line in a local shell, as in the scan's tests.
    let fake_ssh = tmp.path().join("fake-ssh");
    // With `slow` there, it takes 3 s: a machine whose line is dead and not yet known so.
    let slow = tmp.path().join("slow");
    std::fs::write(&fake_ssh, format!("#!/bin/sh\n[ -e '{}' ] && sleep 3\nwhile [ \"$1\" != \"--\" ]; do shift; done\nshift; shift\nexec sh -c \"$1\"\n", slow.display())).unwrap();
    std::fs::set_permissions(&fake_ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (here, there) = (tmp.path().join("mac repo"), tmp.path().join("box repo"));
    for r in [&here, &there] {
        std::fs::create_dir(r).unwrap();
        git(r, &["init", "-q", "-b", "main"]);
        git(r, &["commit", "-q", "--allow-empty", "-m", "base"]);
    }
    git(&there, &["switch", "-q", "-c", "feat/feature"]);
    std::fs::write(there.join("queue.rs"), "fn drain() {}\n").unwrap();
    git(&there, &["add", "."]);
    git(&there, &["commit", "-q", "-m", "work there"]);
    let path = |p: &std::path::Path| std::fs::canonicalize(p).unwrap().to_string_lossy().into_owned();

    let local = std::sync::Arc::new(Fake::default());
    *local.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle))];
    let far = std::sync::Arc::new(Fake::default());
    *far.agents.lock().unwrap() = vec![("p1".into(), live("s-worker", "worker", "w1", Idle))];
    let all = crate::runtime::follow::Machines::with(local, vec![("box", far)]);
    all.set_runner("box", workriver_scan::Runner::Ssh { program: fake_ssh.to_string_lossy().into_owned(), options: vec![], target: "box".into() });
    all.poll_all();
    let store = workriver_data::Store::open(&tmp.path().join("w.db")).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let port = listener.local_addr().unwrap().port();
    let (runtime, machines) = (all.clone(), all.clone());
    let options = crate::server::Options { machines: Some(machines), ..kernel.options() };
    std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, runtime, options)));
    let user = kernel.client(port, tmp.path());
    let boss = kernel.daemon(port, Some("p1".into()), None);
    let ws = seat_at(&user, &boss);
    user.append("", "intent.created", json!({ "title": "Sync", "state": "planned" })).unwrap();
    user.append("", "intent.state_changed", json!({ "intent": "i1", "state": "active", "workspace": ws })).unwrap();

    // What happened on the work machine: recorded on the manager's side, with a ref that only exists there.
    boss.append("", "unit.created", json!({ "intent": "i1", "title": "Engine", "repo": path(&here), "branch": "feat/hub", "base": "main" })).unwrap();
    // Recording it where the work is: the intent's manager puts a unit on any machine
    // workriver follows, with no link.
    let on_box = json!({ "intent": "i1", "title": "Queue", "repo": path(&there), "branch": "feat/feature", "base": "main", "machine": "box" });
    let made = boss.append("", "unit.created", on_box).unwrap();
    assert_eq!((made["body"]["machine"].as_str(), made["body"]["repo"].as_str()), (Some("box"), Some(path(&there).as_str())));
    // The path and the base are checked on that machine.
    let e = boss.append("", "unit.created", json!({ "intent": "i1", "title": "x", "repo": "/no/such/repo", "branch": "feat/f", "base": "main", "machine": "box" })).unwrap_err().to_string();
    assert!(e.contains("/no/such/repo on box is not a git repository"), "{e}");

    // The first unit is corrected: what is not given stays.
    let moved = boss.append("", "unit.relocated", json!({ "unit": "u1", "machine": "box", "repo": path(&there), "branch": "feat/feature", "base": "" })).unwrap();
    assert_eq!(moved["body"], json!({ "unit": "u1", "machine": "box", "repo": path(&there), "branch": "feat/feature", "base": "main" }));
    assert!(boss.append("", "unit.relocated", json!({ "unit": "u1", "repo": "", "branch": "", "base": "nope" })).unwrap_err().to_string().contains("base \"nope\" is not a commit or branch"));
    let back = user.append("", "unit.relocated", json!({ "unit": "u2", "machine": "local", "repo": path(&here), "branch": "", "base": "" })).unwrap();
    assert!(back["body"]["machine"].is_null() && back["body"]["branch"] == "feat/feature");

    // Read over ssh in the background, never while a request waits.
    wait_until("read over ssh", || user.get("/api/workspace").unwrap()["intents"][0]["units"][0]["commits"].as_array().unwrap().len() == 1);
    let ws = user.get("/api/workspace").unwrap();
    let u1 = &ws["intents"][0]["units"][0];
    assert_eq!((u1["machine"].as_str(), u1["branch"].as_str()), (Some("box"), Some("feat/feature")), "{u1}");
    // Its texts as the background read found them; the snapshot has only how many lines.
    let code = user.get("/api/unit/u1/code").unwrap();
    assert!(code["commits"][0]["files"][0]["after"].is_string() && u1["commits"][0]["files"][0].get("after").is_none() && u1["commits"][0]["files"][0]["add"].as_u64() > Some(0), "{code}");
    let board = crate::cli::board_text(&boss, None).unwrap();
    assert!(board.contains(&format!("u1 Engine  [on box: {}, branch feat/feature from main]", path(&there))), "{board}");

    // The line to box dies (the computer slept): nothing waits on box until it is back.
    boss.append("", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Drain", "agent": "s-worker" })).unwrap();
    std::fs::write(&slow, "").unwrap();
    all.drop_tunnel("box");
    let started = std::time::Instant::now();
    let board = crate::cli::board_text(&boss, None).unwrap();
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_secs(1), "board took {took:?} with box down\n{board}");
    assert!(board.contains("    box is disconnected since "), "{board}");
    let ws = user.get("/api/workspace").unwrap();
    assert_eq!(ws["intents"][0]["units"][0]["commits"].as_array().unwrap().len(), 1, "as last read");
    assert_eq!(user.get("/api/unit/u1/code").unwrap(), code, "texts as last read");
    let worker = ws["sessions"].as_array().unwrap().iter().find(|x| x["agent"] == "worker").unwrap().clone();
    assert_eq!((worker["unreachable"].as_bool(), worker["state"].as_str()), (Some(true), Some("unknown")), "as last seen, in no state: {worker}");
    let shown: crate::common::wire::TaskShow = serde_json::from_value(boss.get("/api/task?task=u1%231").unwrap()).unwrap();
    assert!(shown.place.unwrap().disconnected_since.is_some());
    // A message for an agent there is refused at once, saying why.
    let started = std::time::Instant::now();
    let e = workriver_core::runtime::Runtime::send(all.as_ref(), "s-worker", "hi").unwrap_err();
    // Nothing was typed: the error says so, and what waits to be given waits on.
    assert!(matches!(e, workriver_core::runtime::RuntimeError::Unreachable(_)), "{e:?}");
    let e = e.to_string();
    assert!(started.elapsed() < std::time::Duration::from_millis(200) && e.contains("machine box is disconnected since "), "{e}");
    assert!(boss.append("", "unit.created", json!({ "intent": "i1", "title": "y", "repo": path(&there), "branch": "feat/f", "base": "main", "machine": "box" })).unwrap_err().to_string().contains("machine box cannot be reached now (disconnected since "));
    std::fs::remove_file(&slow).unwrap();

    // Chores of no intent: a project's (a registered one's; a repository outside every
    // project is nobody's), and nobody's.
    boss.append("", "chore.created", json!({ "project": path(&here), "title": "Outside every project" })).unwrap();
    user.append("", "project.added", json!({ "path": path(&here), "title": "Mac" })).unwrap();
    boss.append("", "chore.created", json!({ "project": path(&here), "title": "Renew the certificate" })).unwrap();
    boss.append("", "chore.created", json!({ "title": "Pay the invoice" })).unwrap();
    let ws = user.get("/api/workspace").unwrap();
    assert_eq!(ws["chores"].as_array().unwrap().iter().map(|c| (c["title"].as_str().unwrap(), c["project"].as_str().unwrap().to_string())).collect::<Vec<_>>(), [("Outside every project", String::new()), ("Renew the certificate", path(&here)), ("Pay the invoice", String::new())]);
    assert!(crate::cli::board_text(&boss, None).unwrap().contains("chore c3 Pay the invoice (by boss)"));
    user.call("/api/chore-done", Some(json!({ "chore": "c1" }))).unwrap();

    // The build is told, to tell a CLI and a service of different builds apart.
    let status = user.get("/api/status").unwrap();
    assert_eq!(status["commit"], env!("WORKRIVER_COMMIT"));
    assert!(!env!("WORKRIVER_COMMIT").is_empty());
}

/// A machine just enabled is "connecting" until its first attempt ends one way or the other.
#[test]
fn a_machine_is_connecting_until_it_is_not() {
    use crate::runtime::follow::connecting;
    use crate::runtime::ssh::Tunnel;
    assert!(connecting(&Tunnel::default(), false, false), "nothing has happened yet");
    assert!(!connecting(&Tunnel { up: false, error: Some("ssh: Connection refused".into()), ..Tunnel::default() }, false, false), "ssh said why not");
    let up = Tunnel { up: true, ..Tunnel::default() };
    assert!(connecting(&up, false, false), "up, agents not read yet");
    assert!(!connecting(&up, true, false) && !connecting(&up, false, true));
}

/// Adding a machine takes its runtime socket as a path or nothing: anything else is refused
/// (422, as before the runtime's facade) and nothing is written; a path is kept under its
/// key in settings.json, null and no key at all write none.
#[test]
fn a_machines_socket_is_a_path_or_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().to_path_buf();
    let fake = std::sync::Arc::new(Fake::default());
    let all = crate::runtime::follow::Machines::new(fake.clone(), home.clone(), String::new());
    let (port, kernel) = daemon_over(fake, Some(all), &home.join("workriver.db"));
    let d = kernel.client(port, &home);
    let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).build().into();
    let add = |body: &str| {
        let mut res = d.send(|headers| headers.iter().fold(agent.post(format!("http://127.0.0.1:{port}/api/machines")), |r, (k, v)| r.header(*k, v)).header("content-type", "application/json").send(body)).unwrap();
        (res.status().as_u16(), res.body_mut().read_to_string().unwrap())
    };
    let saved = || serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(home.join("settings.json")).unwrap_or_default()).unwrap_or_default()["machines"].to_string();
    assert_eq!(add(r#"{"label":"string","action":"add","ssh":"fixture.invalid","herdrSocket":"/specific.sock"}"#).0, 200);
    assert_eq!(add(r#"{"label":"null","action":"add","ssh":"fixture.invalid","herdrSocket":null}"#).0, 200);
    assert_eq!(add(r#"{"label":"none","action":"add","ssh":"fixture.invalid"}"#).0, 200);
    let kept = r#"{"none":{"enabled":false,"ssh":"fixture.invalid"},"null":{"enabled":false,"ssh":"fixture.invalid"},"string":{"enabled":false,"herdrSocket":"/specific.sock","ssh":"fixture.invalid"}}"#;
    assert_eq!(saved(), kept);
    for (case, value, says) in [("number", "42", "integer `42`"), ("bool", "false", "boolean `false`"), ("array", r#"["/specific.sock"]"#, "sequence"), ("object", r#"{"path":"/specific.sock"}"#, "map")] {
        let (status, text) = add(&format!(r#"{{"label":"{case}","action":"add","ssh":"fixture.invalid","herdrSocket":{value}}}"#));
        assert_eq!(status, 422, "{case}: {text}");
        // Before the runtime's facade (84e6583) the text also named the key (`…target type: herdrSocket: invalid type…`)
        // and gave the value's column; the adapter's options are read as a group now, so the
        // key is not named and the column is the request's end.
        assert!(text.starts_with(&format!("Failed to deserialize the JSON body into the target type: invalid type: {says}, expected a string at line 1 column ")), "{case}: {text}");
        assert_eq!(saved(), kept, "{case}: nothing is written");
    }
}

/// An attachment's id is the store's own form: an agent over there cannot record one that
/// names a file outside the store, nor read such a file through the store's route.
#[test]
fn an_attachment_id_cannot_name_a_file_outside_the_store() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let secret = tmp.path().join("outside").join("secret");
    std::fs::create_dir_all(secret.parent().unwrap()).unwrap();
    std::fs::write(&secret, b"TOP_SECRET_OUTSIDE_THE_STORE").unwrap();
    let secret_s = secret.to_string_lossy().to_string();
    let local = std::sync::Arc::new(Fake::default());
    *local.agents.lock().unwrap() = vec![("w1:p1".into(), live("s-boss", "boss", "w1", Idle))];
    let far = std::sync::Arc::new(Fake::default());
    *far.agents.lock().unwrap() = vec![("w1:p1".into(), live("s-far", "far", "w1", Idle))];
    let all = crate::runtime::follow::Machines::with(local, vec![("box", far)]);
    let (port, kernel) = daemon_over(all.clone(), Some(all.clone()), &tmp.path().join("w.db"));
    wait_until("the service", || http(port, "GET", "/api/status", &[], None).0 == 200);
    wait_until("the machine's listener", || all.local_port("box").is_some());
    let far = kernel.daemon(all.local_port("box").unwrap(), Some("w1:p1".into()), None);
    let e = far.append("", "attachment.added", json!({ "id": secret_s, "name": "x", "type": "text/plain", "size": 1 })).unwrap_err().to_string();
    assert!(e.contains("16 lowercase hex"), "{e}");
    // Nor is a path read through the route, recorded or not.
    for id in [secret_s.replace('/', "%2F"), "..%2F..%2Fw.db".into()] {
        let got = far.fetch(&id).map(|b| String::from_utf8_lossy(&b).to_string());
        assert!(got.is_err() || !got.as_deref().unwrap().contains("TOP_SECRET"), "{id}: {got:?}");
    }
}
