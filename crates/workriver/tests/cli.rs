//! What the CLI prints for the commands nothing else tested (docs/design/guards.md):
//! `project list`, `workspace list`, `backlog list` and `export`, `finding show`
//! and `finding drop`, and the user adding, enabling, disabling and removing a machine,
//! down to the settings file; and what `task add` refuses of a decide gate. Runs the real binary against a stand-in for the runtime's
//! socket and an ssh stand-in; a test run has no terminal, so the service is started as
//! tests start it (`WORKRIVER_ALLOW_AS=1`, see security.md).
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_workriver");

/// Ends the service with the test, however the test ends.
struct Serve(std::process::Child);

impl Drop for Serve {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The runtime's socket, as much as the service asks of it: one workspace, `exp`, empty.
fn runtime(path: &Path) {
    let listener = UnixListener::bind(path).unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let mut line = String::new();
            if BufReader::new(&stream).read_line(&mut line).is_err() {
                continue;
            }
            let req: Value = serde_json::from_str(&line).unwrap_or_default();
            let answer = match req["method"].as_str().unwrap_or_default() {
                "ping" => json!({ "id": req["id"], "result": { "version": "0.9.0", "protocol": 22 } }),
                "session.snapshot" => json!({ "id": req["id"], "result": { "snapshot": { "workspaces": [{ "workspace_id": "w1", "label": "exp" }], "panes": [], "agents": [] } } }),
                _ => json!({ "id": req["id"], "error": { "code": "not_in_this_stand_in", "message": "not in this stand-in" } }),
            };
            let _ = (&stream).write_all(format!("{answer}\n").as_bytes());
        }
    });
}

/// How long this run's tunnel sleeps: a number no other run of this test has (its fraction
/// is this process's id), so that `tunnels` counts this run's tunnel and no other's when
/// several runs of the suite overlap.
fn nap() -> String {
    format!("6417.{}", std::process::id())
}

/// The tunnel's ssh: up at once on a port over there, until killed; anything else (writing
/// the port file over there) succeeds.
fn ssh() -> String {
    format!("#!/bin/sh\ncase \"$1\" in -N) echo 'Allocated port 45124 for remote forward to 127.0.0.1:1' >&2; exec sleep {};; esac\nexit 0\n", nap())
}

fn tunnels() -> usize {
    let out = Command::new("pgrep").args(["-f", &format!("^sleep {}$", nap().replace('.', "\\."))]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).lines().count()
}

fn eventually(what: &str, ok: impl Fn() -> bool) {
    let until = Instant::now() + Duration::from_secs(15);
    while !ok() {
        assert!(Instant::now() < until, "{what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn lists_findings_and_machines_from_the_cli() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let socket = tmp.path().join("rt.sock");
    runtime(&socket);
    let bin = tmp.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(bin.join("ssh"), ssh()).unwrap();
    std::fs::set_permissions(bin.join("ssh"), std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let cli = || {
        let mut c = Command::new(BIN);
        c.env("WORKRIVER_HOME", &home).env("WORKRIVER_PORT", port.to_string()).env("HERDR_SOCKET_PATH", &socket).env("PATH", &path);
        for k in ["HERDR_ENV", "HERDR_PANE_ID", "HERDR_WORKSPACE_ID", "HERDR_TAB_ID", "WORKRIVER_AS", "WORKRIVER_ALLOW_AS"] {
            c.env_remove(k);
        }
        c
    };
    let log = std::fs::File::create(tmp.path().join("serve.log")).unwrap();
    let _serve = Serve(cli().args(["serve", "--terminal-app", ""]).env("WORKRIVER_ALLOW_AS", "1").stdout(log.try_clone().unwrap()).stderr(log).spawn().unwrap());
    eventually("the service starts", || cli().arg("status").output().unwrap().status.success());
    // One command: what it printed, or why it failed.
    let run = |args: &[&str]| {
        let out = cli().args(args).output().unwrap();
        assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    };

    let repo = tmp.path().join("chat");
    std::fs::create_dir(&repo).unwrap();
    for args in [&["init", "-q", "-b", "main"][..], &["-c", "user.name=a", "-c", "user.email=a@example.invalid", "commit", "-q", "--allow-empty", "-m", "base"]] {
        assert!(Command::new("git").arg("-C").arg(&repo).args(args).status().unwrap().success());
    }
    let repo = repo.canonicalize().unwrap().display().to_string();
    run(&["project", "add", &repo, "--title", "Chat"]);
    run(&["backlog", "add", "Search", "--goal", "Find messages", "--project", &repo, "--note", "See the spec.", "--external", "jira:CH-1", "--url", "https://example.invalid/CH-1"]);
    run(&["backlog", "add", "Emoji", "--goal", "React to messages", "--project", &repo]);
    run(&["backlog", "add-finding", "--project", &repo, "--severity", "P2", "--problem", "Old dependency", "--impact", "A known hole", "--where", "Cargo.toml", "--source", "audit", "--note", "Seen in the audit."]);

    assert_eq!(run(&["project", "list"]), format!("p1 Chat  {repo}\n  {repo}\n"));
    assert_eq!(run(&["workspace", "list"]), "exp  project unknown\n");
    run(&["workspace", "project", "exp", "p1"]);
    assert_eq!(run(&["workspace", "list"]), "exp  works on Chat (user)\n");
    let listed = format!("Chat  {repo}  (p1)\n  roadmap:\n    1. i1 Search — Find messages [jira:CH-1]\n    2. i2 Emoji — React to messages\n  deferred findings:\n    P2 f1 from audit: Old dependency\n");
    assert_eq!(run(&["backlog", "list"]), listed);
    assert_eq!(run(&["backlog", "list", "--project", "p1"]), listed);
    let export = run(&["backlog", "export", "--project", "p1"]);
    let built = export.lines().nth(2).unwrap().to_string();
    assert!(built.starts_with("Exported from workriver at 20") && built.ends_with("A read-only copy: change the backlog with `workriver backlog …` or in the web UI."), "{export}");
    assert_eq!(
        export,
        format!("# Chat backlog\n\n{built}\n\n## Roadmap\n\n1. **Search** (i1) ([jira:CH-1](https://example.invalid/CH-1))\n   Find messages\n   See the spec.\n2. **Emoji** (i2)\n   React to messages\n\n## Deferred findings\n\n- P2 f1 from audit: Old dependency\n  Seen in the audit.\n")
    );

    // A review's finding, shown in full, deferred, then dropped by the user.
    run(&["unit", "add", "i1", "--title", "Index", "--branch", "feat/index", "--base", "main", "--repo", &repo]);
    run(&["task", "add", "u1", "--kind", "review", "--title", "Review it"]);
    let findings = tmp.path().join("findings.json");
    std::fs::write(&findings, json!([{ "severity": "P2", "problem": "No limit", "impact": "Out of memory", "where": ["src/index.rs"], "suggestion": { "decision": "defer", "reason": "later" } }]).to_string()).unwrap();
    run(&["--as", "s-rev", "task", "deliver", "u1#1", "--findings", &findings.display().to_string()]);
    let shown = run(&["finding", "show", "u1#1", "P2-1"]);
    assert!(shown.starts_with("u1#1  review  Review it\n") && shown.contains("No limit") && shown.contains("Out of memory") && shown.contains("src/index.rs"), "{shown}");
    run(&["finding", "decide", "u1#1", "P2-1", "defer"]);
    run(&["finding", "drop", "u1#1", "P2-1", "--reason", "not worth it"]);
    let dropped = run(&["finding", "show", "u1#1", "P2-1"]);
    assert!(dropped.contains("dropped") && dropped.contains("not worth it"), "{dropped}");
    // An agent may not drop one; a backlog finding is dropped by its id alone.
    let refused = cli().args(["--as", "s-rev", "finding", "drop", "f1", "--reason", "mine"]).output().unwrap();
    assert!(!refused.status.success(), "an agent dropped a finding");
    run(&["finding", "drop", "f1", "--reason", "fixed upstream"]);
    // Its id as a finding is listed (`P2 f1 from …`): a word of its own, so that the project's
    // path in the same output cannot be taken for it.
    let left = run(&["backlog", "list"]);
    assert!(!left.split_whitespace().any(|word| word == "f1"), "{left}");

    // The user adds a machine (not followed yet), follows it, stops, and removes it; the
    // settings file says so at every step, and the connection lives only while followed.
    let settings = || -> Value { serde_json::from_str(&std::fs::read_to_string(home.join("settings.json")).unwrap_or_else(|_| "{}".into())).unwrap() };
    assert_eq!(run(&["machine", "list"]), "");
    assert_eq!(run(&["machine", "add", "box", "--ssh", "me@box", "--herdr-socket", "/run/rt.sock"]), "machine box (me@box): disabled\n");
    assert_eq!(settings()["machines"]["box"], json!({ "ssh": "me@box", "herdrSocket": "/run/rt.sock", "enabled": false }));
    assert_eq!(tunnels(), 0);
    let enabled = run(&["machine", "enable", "box"]);
    assert!(enabled.starts_with("machine box (me@box): ") && !enabled.contains("disabled") && enabled.ends_with("On box, `workriver status` must answer through this connection.\n"), "{enabled}");
    assert_eq!(settings()["machines"]["box"]["enabled"], true);
    eventually("the tunnel is up", || tunnels() == 1);
    assert!(run(&["machine", "list"]).starts_with("machine box (me@box): "));
    assert_eq!(run(&["machine", "disable", "box"]), "machine box (me@box): disabled\n");
    assert_eq!(settings()["machines"]["box"]["enabled"], false);
    eventually("the tunnel ends", || tunnels() == 0);
    assert_eq!(run(&["machine", "remove", "box"]), "");
    assert_eq!(settings()["machines"], json!({}));
    assert_eq!(run(&["machine", "list"]), "");
    let again = cli().args(["machine", "remove", "box"]).output().unwrap();
    assert!(!again.status.success() && String::from_utf8_lossy(&again.stderr).contains("no machine box"), "{:?}: {}", again.status, String::from_utf8_lossy(&again.stderr));
}

/// A decide gate asks either may I (`--go`) or which (two or more `--option`), never neither,
/// never one option and never both; `--go` is for a decide gate only. Each is refused before anything is sent:
/// no service runs here, so a command that tried to send would fail with another message.
#[test]
fn a_decide_gate_takes_go_or_options() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir(&home).unwrap();
    // A port nobody listens on.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let refused = |args: &[&str]| {
        let mut c = Command::new(BIN);
        c.env("WORKRIVER_HOME", &home).env("WORKRIVER_PORT", port.to_string());
        for k in ["HERDR_ENV", "HERDR_PANE_ID", "HERDR_WORKSPACE_ID", "HERDR_TAB_ID", "HERDR_SOCKET_PATH", "WORKRIVER_AS", "WORKRIVER_ALLOW_AS"] {
            c.env_remove(k);
        }
        let out = c.args(["task", "add", "u1"]).args(args).output().unwrap();
        assert!(!out.status.success(), "{args:?} was taken");
        String::from_utf8_lossy(&out.stderr).into_owned()
    };
    let gate = ["--kind", "gate", "--gate", "decide", "--title", "Merge u1?"];
    let neither = refused(&gate);
    assert!(neither.contains("a decide gate asks either may I (--go") && neither.contains("or which (--option ID=LABEL, two or more)"), "{neither}");
    let one = refused(&[&gate[..], &["--option", "a=A"]].concat());
    assert!(one.contains(&format!("at least {} --option, not 1", workriver_core::MIN_OPTIONS)), "{one}");
    let both = refused(&[&gate[..], &["--go", "--option", "a=A", "--option", "b=B"]].concat());
    assert!(both.contains("--go or --option, not both"), "{both}");
    for other in [&["--kind", "gate", "--gate", "test", "--title", "Try it", "--go"][..], &["--kind", "implement", "--title", "Build it", "--go"]] {
        let said = refused(other);
        assert!(said.contains("--go is for a decide gate"), "{other:?}: {said}");
    }
}
