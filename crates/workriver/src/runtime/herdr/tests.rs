use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;

use serde_json::Value;
use workriver_core::runtime::AgentState;

use super::agents::agents_of;
use super::words::*;
use super::*;

/// A stand-in for Herdr's socket: answers each connection's one request with the next
/// reply, checks nothing more comes on it, and hands back the requests.
fn fake(replies: Vec<Value>) -> (tempfile::TempDir, Herdr, std::thread::JoinHandle<Vec<Value>>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("h.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = std::thread::spawn(move || {
        let mut seen = vec![];
        for mut reply in replies {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(&stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let req: Value = serde_json::from_str(&line).unwrap();
            reply["id"] = req["id"].clone();
            (&stream).write_all(format!("{reply}\n").as_bytes()).unwrap();
            let mut more = String::new();
            assert_eq!(reader.read_line(&mut more).unwrap(), 0, "one request per connection");
            seen.push(req);
        }
        seen
    });
    let herdr = Herdr::new(path, String::new());
    (dir, herdr, server)
}

fn snapshot() -> Value {
    json!({ "result": { "type": "session_snapshot", "snapshot": {
        "version": "0.9.0", "protocol": 22,
        "workspaces": [{ "workspace_id": "w5", "label": "chat", "number": 1 }],
        "panes": [
            { "pane_id": "w5:p1", "workspace_id": "w5", "label": "lead" },
            { "pane_id": "w5:p2", "workspace_id": "w5", "label": "side" },
            { "pane_id": "w5:p3", "workspace_id": "w5" }
        ],
        "agents": [
            { "pane_id": "w5:p1", "workspace_id": "w5", "agent": "claude", "agent_status": "working",
              "agent_session": { "agent": "claude", "kind": "id", "source": "herdr:claude", "value": "s-1" },
              "cwd": "/r", "foreground_cwd": "/r/sub", "unknown_field": 1 },
            { "pane_id": "w5:p2", "workspace_id": "w5", "agent": "codex", "name": "reviewer", "agent_status": "done" },
            { "pane_id": "w5:p3", "workspace_id": "w5", "agent": null, "agent_status": "unknown" },
            { "pane_id": "w5:p12", "workspace_id": "w5", "agent": "claude", "agent_status": "idle", "terminal_title_stripped": "Fix the login" },
            { "pane_id": "w5:p4", "workspace_id": "w5", "agent": "claude", "agent_status": "idle" }
        ]
    }}})
}

#[test]
fn reads_agents_one_request_per_connection() {
    let (_dir, herdr, server) = fake(vec![snapshot(), snapshot()]);
    let list = herdr.agents().unwrap();
    assert_eq!(list.len(), 4, "a pane without an agent is no agent");
    // Unnamed agents are numbered per harness within their workspace, in pane order.
    let named: Vec<(&str, Option<&str>)> = list.iter().map(|a| (a.name.as_str(), a.topic.as_deref())).collect();
    assert_eq!(named[2..], [("claude 1", None), ("claude 2", Some("Fix the login"))]);
    let a = &list[0];
    assert_eq!((a.session.as_str(), a.name.as_str(), a.harness.as_str(), a.cwd.as_str()), ("s-1", "lead", "claude", "/r/sub"));
    assert_eq!(a.state, AgentState::Working);
    assert_eq!(a.group, Some(RuntimeRef { id: "w5".into(), label: "chat".into(), machine: None }));
    let b = &list[1];
    assert_eq!((b.session.as_str(), b.name.as_str(), b.state), ("herdr:w5:p2", "reviewer", AgentState::Done), "the Herdr name before the pane label");
    // A shell pane: its workspace, no agent.
    let shell = herdr.locate(None, "w5:p3").unwrap();
    assert_eq!((shell.agent, shell.group.map(|g| g.label)), (None, Some("chat".into())));
    let seen = server.join().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0]["method"], "session.snapshot");
    assert_ne!(seen[0]["id"], seen[1]["id"]);
}

#[test]
fn sends_to_the_sessions_pane_and_reports_errors() {
    let error = json!({ "error": { "code": "agent_blocked", "message": "agent is blocked" } });
    let (_dir, herdr, server) = fake(vec![snapshot(), json!({ "result": { "type": "agent_prompted", "agent": {} } }), error]);
    herdr.send("s-1", "hello").unwrap();
    // Blocked is its own error: the message waits rather than fails.
    let err = herdr.send("s-1", "again").unwrap_err();
    assert!(matches!(&err, RuntimeError::Blocked(w) if w.contains("agent_blocked") && w.contains("agent is blocked")), "{err}");
    let seen = server.join().unwrap();
    assert_eq!(seen[1]["method"], "agent.prompt");
    assert_eq!(seen[1]["params"], json!({ "target": "w5:p1", "text": "hello" }));
}

/// A message for a session goes to the pane that holds it, read fresh; a pane whose agent
/// only goes by that as a name gets nothing: nothing is asked of Herdr but who is there.
#[test]
fn sends_to_a_session_never_to_a_name() {
    let (_dir, herdr, server) = fake(vec![snapshot(), snapshot(), json!({ "result": { "type": "agent_prompted", "agent": {} } })]);
    let err = herdr.send_to(None, "reviewer", "hello").unwrap_err();
    assert!(matches!(&err, RuntimeError::NoAgent(a) if a == "reviewer"), "{err}");
    herdr.send_to(None, "herdr:w5:p2", "hello").unwrap();
    let seen = server.join().unwrap();
    assert_eq!(seen.iter().map(|r| r["method"].as_str().unwrap()).collect::<Vec<_>>(), ["session.snapshot", "session.snapshot", "agent.prompt"]);
    assert_eq!(seen[2]["params"], json!({ "target": "w5:p2", "text": "hello" }));
}

/// A run's shell is closed by its pane, the place `run_shell` gave, and only while that
/// pane is still a plain shell in the runs' workspace: one that is gone, sits in another
/// workspace or holds an agent by now is refused, and nothing is closed.
#[test]
fn closes_only_a_runs_shell() {
    let ok = || json!({ "result": { "type": "ok" } });
    let snap = || {
        let mut s = snapshot();
        let inner = &mut s["result"]["snapshot"];
        inner["workspaces"].as_array_mut().unwrap().push(json!({ "workspace_id": "w7", "label": "workriver-commands", "number": 2 }));
        inner["panes"].as_array_mut().unwrap().extend([json!({ "pane_id": "w7:p3", "workspace_id": "w7", "label": "run" }), json!({ "pane_id": "w7:p4", "workspace_id": "w7", "label": "review-ab" })]);
        inner["agents"].as_array_mut().unwrap().push(json!({ "pane_id": "w7:p4", "workspace_id": "w7", "agent": "claude", "name": "review-ab", "agent_status": "idle" }));
        s
    };
    let (_dir, herdr, server) = fake(vec![snap(), ok()]);
    herdr.close_shell(None, "workriver-commands", "w7:p3").unwrap();
    let seen = server.join().unwrap();
    assert_eq!((seen[0]["method"].as_str(), seen[1]["method"].as_str(), &seen[1]["params"]), (Some("session.snapshot"), Some("pane.close"), &json!({ "pane_id": "w7:p3" })));

    // Another workspace's pane (the user's, an agent's), a pane with an agent, one gone:
    // the snapshot is read and nothing else is asked.
    for place in ["w5:p3", "w5:p1", "w7:p4", "w7:p9"] {
        let (_dir, herdr, server) = fake(vec![snap()]);
        assert!(matches!(herdr.close_shell(None, "workriver-commands", place), Err(RuntimeError::NoPlace(_))), "{place}");
        let (_dir2, herdr2, server2) = fake(vec![snap()]);
        assert!(matches!(herdr2.shell_processes(None, "workriver-commands", place), Err(RuntimeError::NoPlace(_))), "{place}");
        assert_eq!(server.join().unwrap().len() + server2.join().unwrap().len(), 2, "{place}");
    }

    // What runs there: its shell, and what is in front.
    let info = json!({ "result": { "type": "pane_process_info", "process_info": { "pane_id": "w7:p3", "shell_pid": 100, "foreground_processes": [{ "pid": 100 }, { "pid": 101 }] } } });
    let (_dir, herdr, server) = fake(vec![snap(), info]);
    assert_eq!(herdr.shell_processes(None, "workriver-commands", "w7:p3").unwrap(), (100, vec![100, 101]));
    assert_eq!(server.join().unwrap()[1]["params"], json!({ "pane_id": "w7:p3" }));
}

/// A new agent on a first-run prompt is blocked and not interactive-ready: taken at once,
/// its tab kept. One Herdr could not start: its tab is closed.
#[test]
fn starts_a_temporary_agent() {
    let tab = json!({ "result": { "type": "tab_created", "root_pane": { "pane_id": "w5:p7" } } });
    let started = json!({ "result": { "type": "agent_started" } });
    let mut snap = snapshot();
    snap["result"]["snapshot"]["agents"].as_array_mut().unwrap().push(json!({ "pane_id": "w5:p7", "workspace_id": "w5", "agent": "claude", "name": "why-q1", "agent_status": "blocked", "interactive_ready": false }));
    let (_dir, herdr, server) = fake(vec![tab.clone(), started, snap]);
    let a = herdr.start_temporary("w5", "claude", "why-q1", "/r").unwrap();
    assert_eq!((a.name.as_str(), a.state), ("why-q1", AgentState::Blocked));
    let seen = server.join().unwrap();
    assert_eq!(seen.iter().map(|r| r["method"].as_str().unwrap()).collect::<Vec<_>>(), ["tab.create", "agent.start", "session.snapshot"], "no pane.close");

    let refused = json!({ "error": { "code": "agent_start_failed", "message": "no such harness" } });
    let (_dir, herdr, server) = fake(vec![tab, refused, json!({ "result": { "type": "ok" } })]);
    assert!(herdr.start_temporary("w5", "nope", "why-q2", "/r").is_err());
    let seen = server.join().unwrap();
    assert_eq!((seen[2]["method"].as_str(), &seen[2]["params"]), (Some("pane.close"), &json!({ "pane_id": "w5:p7" })));
}

/// A socket that reads each connection's one request and gives it the next answer, or
/// closes it unanswered (None). Hands back the methods asked.
fn dropping(answers: Vec<Option<Value>>) -> (tempfile::TempDir, Herdr, std::thread::JoinHandle<Vec<String>>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("h.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = std::thread::spawn(move || {
        let mut seen = vec![];
        for answer in answers {
            let (stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            let req: Value = serde_json::from_str(&line).unwrap();
            seen.push(req["method"].as_str().unwrap().to_string());
            if let Some(mut reply) = answer {
                reply["id"] = req["id"].clone();
                (&stream).write_all(format!("{reply}\n").as_bytes()).unwrap();
            }
        }
        seen
    });
    (dir, Herdr::new(path, String::new()), server)
}

/// Herdr's connection drops while the agents are read before typing (no answer to the
/// snapshot): nothing was typed, so the runtime cannot be reached now, which the message
/// waits through. One that drops after the prompt was asked for may have typed it: failed.
#[test]
fn a_drop_before_the_prompt_is_unreachable_after_it_failed() {
    let (_dir, herdr, server) = dropping(vec![None]);
    let err = herdr.send_to(None, "s-1", "hello").unwrap_err();
    assert!(matches!(&err, RuntimeError::Unreachable(w) if w.contains("session.snapshot")), "{err:?}");
    assert_eq!(server.join().unwrap(), ["session.snapshot"]);

    let (_dir, herdr, server) = dropping(vec![Some(snapshot()), None]);
    let err = herdr.send_to(None, "s-1", "hello").unwrap_err();
    assert!(matches!(&err, RuntimeError::Failed(w) if w.contains("agent.prompt")), "{err:?}");
    assert_eq!(server.join().unwrap(), ["session.snapshot", "agent.prompt"]);
}

#[test]
fn no_socket_means_no_runtime() {
    let herdr = Herdr::new("/nonexistent/herdr.sock".into(), String::new());
    assert!(matches!(herdr.agents(), Err(RuntimeError::None)));
    assert!(herdr.check().unwrap().contains("not running"));
}

#[test]
fn reads_the_processes_of_each_agent_pane() {
    let info = |shell: Value, fg: Value| json!({ "result": { "type": "pane_process_info", "process_info": { "pane_id": "x", "shell_pid": shell, "foreground_processes": fg } } });
    let refused = json!({ "error": { "code": "pane_not_found", "message": "gone" } });
    let replies = vec![snapshot(), info(json!(100), json!([{ "pid": 101, "name": "claude" }])), info(Value::Null, json!([{ "pid": 200, "name": "codex" }])), refused, info(json!(400), json!([]))];
    let (_dir, herdr, server) = fake(replies);
    let got: Vec<(String, Vec<u32>)> = herdr.processes(None).unwrap().into_iter().map(|(a, p)| (a.name, p)).collect();
    assert_eq!(got, [("lead".into(), vec![100, 101]), ("reviewer".into(), vec![200]), ("claude 1".into(), vec![]), ("claude 2".into(), vec![400])]);
    let seen = server.join().unwrap();
    assert_eq!(seen[1]["method"], "pane.process_info");
    assert_eq!(seen[1]["params"], json!({ "pane_id": "w5:p1" }));
}
/// A session id is the harness's own. One of the form workriver gives a pane without an
/// id (`herdr:<pane>`) is not taken from a report: the agent is named by the pane it is in.
#[test]
fn a_reported_pane_name_is_not_a_session() {
    let mut snap = snapshot();
    let agents = snap["result"]["snapshot"]["agents"].as_array_mut().unwrap();
    agents[4]["agent_session"] = json!({ "agent": "claude", "kind": "id", "source": "report", "value": "herdr:w5:p1" });
    agents[3]["agent_session"] = json!({ "agent": "claude", "kind": "id", "source": "report", "value": "s-9" });
    let (_dir, herdr, _server) = fake(vec![snap]);
    let list = herdr.agents().unwrap();
    let sessions: Vec<&str> = list.iter().map(|a| a.session.as_str()).collect();
    assert_eq!(sessions, ["s-1", "herdr:w5:p2", "herdr:w5:p4", "s-9"], "{sessions:?}");
    // On another machine the pane's name carries the machine.
    let far = agents_of(&serde_json::from_value(json!({ "agents": [{ "pane_id": "w1:p2", "workspace_id": "w1", "agent": "claude", "agent_session": { "value": "herdr:w1:p1@box" } }] })).unwrap(), Some("box"));
    assert_eq!(far[0].1.session, "herdr:w1:p2@box");
}

/// The root of one place, or of all: its shell, what is in front, and what is there.
#[test]
fn reads_the_root_of_a_place() {
    let info = |shell: Value, fg: Value| json!({ "result": { "type": "pane_process_info", "process_info": { "pane_id": "x", "shell_pid": shell, "foreground_processes": fg } } });
    let refused = || json!({ "error": { "code": "pane_not_found", "message": "gone" } });
    let (_dir, herdr, server) = fake(vec![snapshot(), info(json!(100), json!([{ "pid": 101 }]))]);
    let got = herdr.roots(None, Some("w5:p1")).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!((got[0].place.as_str(), got[0].shell, &got[0].front, got[0].pids()), ("w5:p1", Some(100), &vec![101], vec![100]));
    assert_eq!((got[0].at.agent.as_ref().map(|a| a.session.as_str()), got[0].at.group.as_ref().map(|g| g.label.as_str())), (Some("s-1"), Some("chat")));
    assert_eq!(server.join().unwrap()[1]["params"], json!({ "pane_id": "w5:p1" }));
    // A pane that is gone: asked of nobody, or refused by Herdr, it is not there.
    let (_dir, herdr, server) = fake(vec![snapshot()]);
    assert!(herdr.roots(None, Some("wM:p4")).unwrap().is_empty());
    assert_eq!(server.join().unwrap().len(), 1);
    let (_dir, herdr, _server) = fake(vec![snapshot(), refused()]);
    assert!(herdr.roots(None, Some("w5:p3")).unwrap().is_empty());
    // Every place: plain panes too, and one where Herdr knows no shell.
    let replies = vec![snapshot(), info(json!(100), json!([])), info(Value::Null, json!([{ "pid": 200 }, { "pid": 201 }])), info(json!(300), json!([])), refused(), info(json!(500), json!([]))];
    let (_dir, herdr, _server) = fake(replies);
    let all: Vec<(String, Vec<u32>, bool)> = herdr.roots(None, None).unwrap().into_iter().map(|r| (r.place.clone(), r.pids(), r.at.agent.is_some())).collect();
    assert_eq!(all, [("w5:p1".into(), vec![100], true), ("w5:p2".into(), vec![200, 201], true), ("w5:p3".into(), vec![300], false), ("w5:p4".into(), vec![500], true)]);
    // Only the places that hold an agent: a plain pane is not asked about.
    let (_dir, herdr, server) = fake(vec![snapshot(), info(json!(100), json!([])), info(json!(200), json!([])), info(json!(400), json!([])), info(json!(500), json!([]))]);
    let held: Vec<String> = herdr.agent_roots(None).unwrap().into_iter().map(|r| r.place).collect();
    assert_eq!(held, ["w5:p1", "w5:p2", "w5:p12", "w5:p4"]);
    let asked: Vec<Value> = server.join().unwrap().iter().skip(1).map(|r| r["params"]["pane_id"].clone()).collect();
    assert_eq!(asked, [json!("w5:p1"), json!("w5:p2"), json!("w5:p12"), json!("w5:p4")]);
    // An agent's pane whose root cannot be asked still shows its session, with no known shell.
    let (_dir, herdr, _server) = fake(vec![snapshot(), info(json!(100), json!([])), refused(), info(json!(400), json!([])), info(json!(500), json!([]))]);
    let held: Vec<(String, Option<u32>, Option<String>)> = herdr.agent_roots(None).unwrap().into_iter().map(|r| (r.place, r.shell, r.at.agent.map(|a| a.session))).collect();
    assert_eq!(held[1], ("w5:p2".into(), None, Some("herdr:w5:p2".into())), "{held:?}");
    assert_eq!(held.len(), 4);
}

#[test]
fn refusals_name_no_harness() {
    let said = [elsewhere("wM:p4", true, "bash(7) > app(6)", None, None), elsewhere("w1:p3", false, "sleep(4)", Some("w1:p2"), Some("box")), no_terminal("bash(7)"), held("SESS-A", "w1:p2", None), twice("SESS-A", "w1:p2", "w1:p3"), NO_PROCESS.to_string()];
    assert!(said[0].contains("says it runs in pane wM:p4 (there is no pane wM:p4 any more), but its process bash(7) > app(6) runs in no pane") && said[0].contains("workriver guide how hand-off"), "{}", said[0]);
    assert!(said[1].contains("runs in pane w1:p2 on machine box"), "{}", said[1]);
    assert!(said[2].contains("is in no pane and has no terminal") && said[2].contains("workriver guide how hand-off"), "{}", said[2]);
    assert!(said[3].contains("session SESS-A was first seen in pane w1:p2, whose shell still runs") && said[3].contains("start a new session here"), "{}", said[3]);
    for s in &said {
        let low = s.to_lowercase();
        assert!(!["codex", "claude", "daemon", "gemini"].iter().any(|h| low.contains(h)), "{s}");
    }
}

/// A machine's setting keeps Herdr's socket as it always did in settings.json. The fixture
/// holds eight files and what the build before the runtime's facade (84e6583) wrote back for each after
/// one change of the stall threshold: a socket that is a path stays under its key; none, or
/// null, writes no key; another key of a machine's entry is dropped; a socket that is no
/// path (a number, a boolean, a list, an object) makes the whole file unreadable, so the
/// defaults are written. Each is read and written here the same way, byte for byte.
#[test]
fn a_machines_socket_keeps_its_key() {
    use crate::common::home::SETTINGS;
    use crate::runtime::{Setting, Settings};
    let rows: Vec<Value> = serde_json::from_str(include_str!("fixtures/settings-before-phase4.json")).unwrap();
    assert_eq!(rows.iter().map(|r| r["case"].as_str().unwrap()).collect::<Vec<_>>(), ["string", "missing", "null", "unknown", "number", "bool", "array", "object"]);
    for row in &rows {
        let case = row["case"].as_str().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(SETTINGS), serde_json::to_vec(&row["input"]).unwrap()).unwrap();
        let read = Settings::read(dir.path());
        let wanted: Vec<(&String, Option<&str>)> = read.machines.iter().filter(|(_, s)| s.enabled).map(|(l, s)| (l, s.options.socket.as_deref())).collect();
        match case {
            "string" | "unknown" => assert_eq!(format!("{wanted:?}"), r#"[("box", Some("/old/specific.sock"))]"#, "{case}"),
            "missing" | "null" => assert_eq!(format!("{wanted:?}"), r#"[("box", None)]"#, "{case}"),
            _ => assert!(read.machines.is_empty() && Settings::stall_minutes(dir.path()) == 10, "{case}: an unreadable file is the defaults"),
        }
        Settings::set_stall_minutes(dir.path(), 74).unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join(SETTINGS)).unwrap(), row["written"].as_str().unwrap(), "{case}");
    }
    // One entry alone: the port of older files is read and never written.
    let old = r#"{"ssh":"box","herdrSocket":"/run/herdr.sock","enabled":true}"#;
    let s: Setting = serde_json::from_str(old).unwrap();
    assert_eq!(s, Setting { ssh: "box".into(), options: remote::Options { socket: Some("/run/herdr.sock".into()) }, port: 0, enabled: true });
    assert_eq!(serde_json::to_string(&s).unwrap(), old);
    let plain: Setting = serde_json::from_str(r#"{"ssh":"box","port":47821,"enabled":false}"#).unwrap();
    assert_eq!((plain.options.socket.as_deref(), plain.port), (None, 47821));
    assert_eq!(serde_json::to_string(&plain).unwrap(), r#"{"ssh":"box","enabled":false}"#);
}

/// What `machine add` sends: the socket under its key, and the key with null when the option
/// is not given, as the CLI's request always had it (with `ssh`, `label` and `action`).
#[test]
fn machine_add_sends_the_socket_or_null() {
    let body = |socket: Option<&str>| {
        let mut body = remote::request(socket.map(str::to_string));
        body["ssh"] = json!("fixture.invalid");
        body["label"] = json!("box");
        body["action"] = json!("add");
        body.to_string()
    };
    // The request of the build before the runtime's facade (84e6583), as it was written there.
    let before = |socket: Option<&str>| {
        let mut body = json!({ "ssh": "fixture.invalid", "herdrSocket": socket });
        body["label"] = json!("box");
        body["action"] = json!("add");
        body.to_string()
    };
    assert_eq!(body(None), r#"{"action":"add","herdrSocket":null,"label":"box","ssh":"fixture.invalid"}"#);
    assert_eq!(body(Some("/s")), r#"{"action":"add","herdrSocket":"/s","label":"box","ssh":"fixture.invalid"}"#);
    assert_eq!((body(None), body(Some("/s"))), (before(None), before(Some("/s"))));
}
