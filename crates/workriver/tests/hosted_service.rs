//! A hosted agent through the real service, end to end: the user's CLI in a terminal of a
//! stand-in runtime, the real `workriver serve` on a temporary home and port, its real hosts,
//! and one scripted Codex app-server per host. No model, nothing on 47820.
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_workriver");

#[path = "__fixtures__/hosted_app_server.rs"]
mod fake;

fn fixture() -> Value {
    serde_json::from_str(include_str!("__fixtures__/acp/conversation.json")).unwrap()
}

struct Ends(Child);

impl Drop for Ends {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A stand-in for the terminals' runtime: one plain terminal, the user's, whose shell is `shell`.
fn terminals(path: &Path, shell: u32) {
    let listener = UnixListener::bind(path).unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let mut line = String::new();
            if BufReader::new(&stream).read_line(&mut line).is_err() {
                continue;
            }
            let req: Value = serde_json::from_str(&line).unwrap_or_default();
            let result = match req["method"].as_str().unwrap_or_default() {
                "ping" => json!({ "version": "0.9.0", "protocol": 22 }),
                "session.snapshot" => json!({ "snapshot": { "workspaces": [{ "workspace_id": "w1", "label": "exp" }], "panes": [{ "pane_id": "w1:p1", "workspace_id": "w1" }], "agents": [] } }),
                "pane.process_info" => json!({ "process_info": { "pane_id": "w1:p1", "shell_pid": shell, "foreground_processes": [] } }),
                _ => Value::Null,
            };
            let answer = if result.is_null() { json!({ "id": req["id"], "error": { "code": "no", "message": "no" } }) } else { json!({ "id": req["id"], "result": result }) };
            let _ = (&stream).write_all(format!("{answer}\n").as_bytes());
        }
    });
}

/// The user's shell in that terminal: commands typed into it run as its children.
struct Shell {
    _child: Ends,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Shell {
    fn run(&mut self, line: &str) -> (bool, String) {
        writeln!(self.stdin, "{{ {line} ; }} 2>&1; echo \"__done $?\"").unwrap();
        let mut out = String::new();
        loop {
            let mut l = String::new();
            assert!(self.stdout.read_line(&mut l).unwrap() > 0, "the shell ended: {out}");
            if let Some(code) = l.strip_prefix("__done ") {
                return (code.trim() == "0", out);
            }
            out += &l;
        }
    }

    fn ok(&mut self, line: &str) -> String {
        let (ok, out) = self.run(line);
        assert!(ok, "{line}: {out}");
        out
    }
}

struct World {
    root: PathBuf,
    home: PathBuf,
    port: u16,
    serve: Option<Ends>,
    _temp: tempfile::TempDir,
}

impl World {
    fn env(&self, c: &mut Command) {
        c.env_clear().env("HOME", self.root.join("os-home")).env("CODEX_HOME", self.root.join("codex-home")).env("WORKRIVER_HOME", &self.home).env("WORKRIVER_PORT", self.port.to_string())
            .env("HERDR_SOCKET_PATH", self.root.join("t.sock")).env("PATH", format!("{}:/usr/bin:/bin", self.root.join("bin").display())).env("W", BIN);
    }

    fn serve(&mut self) {
        let log = std::fs::OpenOptions::new().create(true).append(true).open(self.root.join("serve.log")).unwrap();
        let mut c = Command::new(BIN);
        self.env(&mut c);
        self.serve = Some(Ends(c.args(["serve", "--terminal-app", ""]).stdin(Stdio::null()).stdout(log.try_clone().unwrap()).stderr(log).spawn().unwrap()));
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            let mut status = Command::new(BIN);
            self.env(&mut status);
            if status.arg("status").output().unwrap().status.success() {
                break;
            }
            assert!(Instant::now() < until, "the service did not start: {}", self.log());
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.root.join("serve.log")).unwrap_or_default()
    }

    fn records(&self) -> Vec<Value> {
        use workriver_core::ports::WorkStore;
        let store = workriver_data::Store::open(&self.home.join("workriver.db")).unwrap();
        store.history(workriver_core::ports::Revision(0), 10_000).unwrap().events.into_iter().map(|e| json!({"actor": e.actor, "kind": serde_json::to_value(&e.body).unwrap()["kind"], "body": serde_json::to_value(&e.body).unwrap()["body"]})).collect()
    }

    fn kinds(&self, kind: &str) -> Vec<Value> {
        self.records().into_iter().filter(|e| e["kind"] == kind).collect()
    }
}

impl Drop for World {
    /// Hosts outlive their service by design: a test ends the ones it started (their agents
    /// read the end of their input and exit with them).
    fn drop(&mut self) {
        let _ = Command::new("pkill").arg("-KILL").arg("-f").arg(format!("workriver host {}", self.home.join("agents").display())).status();
    }
}

fn world() -> (World, Shell) {
    // A Unix socket's path is short on macOS: a temporary folder under /tmp.
    let temp = tempfile::Builder::new().prefix("wr-hs-").tempdir_in("/tmp").unwrap();
    let root = temp.path().to_path_buf();
    for name in ["os-home", "codex-home", "home", "cwd", "bin", "repo"] {
        std::fs::create_dir(root.join(name)).unwrap();
    }
    let executable = std::env::current_exe().unwrap().to_str().unwrap().replace('\'', "'\\''");
    let shim = format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'codex-cli 0.172.0'; exit; fi\n\
         export HOSTED_TEST_PIPE=1\n\
         exec '{executable}' --exact fake::fake_codex_app_server --ignored --nocapture 3>&1 1>&2\n"
    );
    std::fs::write(root.join("bin/codex"), shim).unwrap();
    std::fs::set_permissions(root.join("bin/codex"), std::fs::Permissions::from_mode(0o700)).unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut w = World { home: root.join("home"), root, port, serve: None, _temp: temp };
    let mut c = Command::new("/bin/sh");
    w.env(&mut c);
    c.env("HERDR_PANE_ID", "w1:p1");
    let mut child = c.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let (stdin, stdout) = (child.stdin.take().unwrap(), BufReader::new(child.stdout.take().unwrap()));
    terminals(&w.root.join("t.sock"), child.id());
    w.serve();
    (w, Shell { _child: Ends(child), stdin, stdout })
}

fn options(w: &World, v: Value) {
    std::fs::write(w.root.join("cwd/options.json"), v.to_string()).unwrap();
}

fn show(user: &mut Shell, agent: &str) -> String {
    user.ok(&format!("\"$W\" agent show {agent}"))
}

fn eventually(what: &str, mut f: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(30);
    while !f() {
        assert!(Instant::now() < until, "{what} did not happen");
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[test]
fn the_user_starts_drives_and_closes_a_hosted_worker_across_a_restart() {
    let (mut w, mut user) = world();
    let cwd = w.root.join("cwd");
    let repo = w.root.join("repo");
    assert!(Command::new("git").arg("-C").arg(&repo).args(["init", "-q", "-b", "main"]).status().unwrap().success());
    user.ok(&format!("\"$W\" project add '{}' --title Repo", repo.display()));
    user.ok(&format!("\"$W\" workspace add '{}' --name chat", repo.display()));
    let start = format!("\"$W\" agent start --workspace ws1 --name w --cwd '{}' --model requested-model --auth chatgpt --wait", cwd.display());

    // Off by default: no hosted start.
    let (ok, out) = user.run(&start);
    assert!(!ok && out.contains("the hosted runtime is off"), "{out}");
    user.ok("\"$W\" agent hosted on");

    // A tool of the agent's that calls before its start is recorded is nobody, not the user,
    // though the token is in its reach.
    let backlog = |title: &str| format!("{BIN} backlog add \"{title}\" --goal g --project \"{}\"", repo.display());
    options(&w, json!({"run_at_start": backlog("Before the start")}));
    let out = user.ok(&start);
    assert!(out.contains("a1 w (codex): idle") && out.contains("session: root-thread"), "{out}\n{}", w.log());
    let early = std::fs::read_to_string(cwd.join("start-run.out")).unwrap();
    assert!(early.starts_with("false") && early.contains("still starting"), "{early}");
    let started = w.kinds("agent.started");
    assert_eq!(started.len(), 1, "{started:?}");
    assert_eq!((started[0]["actor"].as_str(), started[0]["body"]["session"].as_str(), started[0]["body"]["account"].as_str()), (Some("workriver"), Some("root-thread"), Some("chatgpt")));
    // The root is the host the service spawned: a `workriver host` process.
    let host = started[0]["body"]["host"]["pid"].as_u64().unwrap();
    let ps = Command::new("ps").args(["-o", "command=", "-p", &host.to_string()]).output().unwrap();
    assert!(String::from_utf8_lossy(&ps.stdout).contains(" host "), "{}", String::from_utf8_lossy(&ps.stdout));
    // A second start under the open name is refused.
    let (ok, out) = user.run(&start);
    assert!(!ok && out.contains("w is a1 already"), "{out}");

    // Its tools' CLI calls are the hosted worker's, by their process.
    user.ok(&format!("\"$W\" agent prompt w --text 'run:{}'", backlog("By the worker")));
    eventually("the worker's tool ran", || cwd.join("run-1.out").exists());
    let ran = std::fs::read_to_string(cwd.join("run-1.out")).unwrap();
    assert!(ran.starts_with("true"), "{ran}");
    let by: Vec<(String, String)> = w.kinds("intent.created").iter().map(|e| (e["body"]["title"].as_str().unwrap().into(), e["actor"].as_str().unwrap().into())).collect();
    assert_eq!(by, [("By the worker".to_string(), "root-thread".to_string())]);

    // Approval requests wait for the user; one answered is recorded once, without its text.
    user.ok(&format!("\"$W\" agent prompt w --text '{}'", fixture()["prompt"].as_str().unwrap()));
    let mut shown = String::new();
    eventually("its approvals are pending", || {
        shown = show(&mut user, "w");
        shown.contains(": blocked") && shown.contains("[allow, reject]")
    });
    let (ok, out) = user.run("\"$W\" agent prompt w --text finish");
    assert!(!ok && out.contains("takes no message now"), "{out}");
    let status: Value = serde_json::from_str(&user.ok("\"$W\" agent status w")).unwrap();
    let (key, _) = status["pending"].as_object().unwrap().iter().find(|(_, r)| r["native"]["id"] == "complete").unwrap();
    user.ok(&format!("\"$W\" agent answer w {key} allow"));
    let (ok, out) = user.run(&format!("\"$W\" agent answer w {key} allow"));
    assert!(!ok && out.contains("not waiting"), "{out}");
    let answered = w.kinds("agent.answered");
    assert_eq!(answered.len(), 1);
    assert_eq!(answered[0]["body"], json!({"agent": "a1", "request": key.parse::<u64>().unwrap(), "decision": "allow"}));
    let out = user.ok("\"$W\" agent cancel w");
    assert!(out.contains("w: cancelled"), "{out}");

    // A restart of the service leaves the host running; it is attached again.
    w.serve = None;
    w.serve();
    eventually("the agent is attached again", || show(&mut user, "w").contains("a1 w (codex): idle"));
    let page = user.ok("\"$W\" agent read w --limit 3");
    assert_eq!(page.lines().filter(|l| l.starts_with('{')).count(), 3, "{page}");
    assert!(page.contains("more: `workriver agent read w --after 3`"), "{page}");

    // The user's close: the host ends its group and says how; the receipt follows its journal.
    let out = user.ok("\"$W\" agent close w --wait");
    assert!(out.contains("a1 w (codex): closed") && out.contains("closed: ended"), "{out}\n{}", w.log());
    assert_eq!(w.kinds("agent.closed")[0]["body"], json!({"agent": "a1", "outcome": "ended"}));
    eventually("the host exits", || !Command::new("kill").args(["-0", &host.to_string()]).status().unwrap().success());
    // A closed agent's journal is still read.
    let page = user.ok("\"$W\" agent read w --after 0 --limit 1000");
    assert!(page.contains("\"event\":\"closed\""), "{page}");
    // The name is free again; the switch off refuses new starts only.
    user.ok("\"$W\" agent hosted off");
    let (ok, out) = user.run(&start);
    assert!(!ok && out.contains("the hosted runtime is off"), "{out}");
}

/// A process of the agent's that left its group is listed as its: running while its launch
/// runs, left over once its host is gone. Nothing ends it by itself; the user ends it.
#[test]
fn a_leftover_process_is_listed_and_ended_only_by_the_user() {
    let (w, mut user) = world();
    let cwd = w.root.join("cwd");
    let repo = w.root.join("repo");
    assert!(Command::new("git").arg("-C").arg(&repo).args(["init", "-q", "-b", "main"]).status().unwrap().success());
    user.ok(&format!("\"$W\" project add '{}' --title Repo", repo.display()));
    user.ok(&format!("\"$W\" workspace add '{}' --name chat", repo.display()));
    user.ok("\"$W\" agent hosted on");
    options(&w, json!({"escape": true}));
    user.ok(&format!("\"$W\" agent start --workspace ws1 --name w --cwd '{}' --model requested-model --auth chatgpt --wait", cwd.display()));
    let mut escaped = String::new();
    eventually("the escaped process starts", || {
        escaped = std::fs::read_to_string(cwd.join("escaped-pid")).unwrap_or_default().trim().to_string();
        !escaped.is_empty()
    });
    let system: String = std::fs::read_to_string(cwd.join("escaped-system-pid")).unwrap_or_default().trim().into();
    let listed = |user: &mut Shell| user.ok("\"$W\" agent processes");
    let running_part = |out: &str| out.split("left over").next().unwrap_or_default().to_string();
    let out = listed(&mut user);
    assert!(running_part(&out).contains(&format!("  {escaped} ")) && out.contains("left over (`--close PID` or `--all` ends them): none"), "{out}");
    // The host dies: its agent is stopped, and what it left is listed, never ended by itself.
    let host = w.kinds("agent.started")[0]["body"]["host"]["pid"].as_u64().unwrap();
    assert!(Command::new("kill").args(["-KILL", &host.to_string()]).status().unwrap().success());
    eventually("the agent is stopped", || show(&mut user, "w").contains(": stopped"));
    let out = listed(&mut user);
    assert!(out.contains(&format!("  {escaped} ")) && out.contains("its agent's host is gone") && out.contains("running: none"), "{out}");
    std::thread::sleep(Duration::from_secs(3));
    assert!(Command::new("kill").args(["-0", &escaped]).status().unwrap().success(), "a leftover was ended by itself");
    let (ok, out) = user.run("\"$W\" agent processes --close 1");
    assert!(!ok && out.contains("is not a leftover"), "{out}");
    let out = user.ok(&format!("\"$W\" agent processes --close {escaped}"));
    assert!(out.contains(&escaped) && out.contains("exited"), "{out}");
    eventually("the leftover is gone", || !Command::new("kill").args(["-0", &escaped]).status().unwrap().success());
    // The system binary's environment is withheld (macOS): it was never listed.
    if !system.is_empty() {
        let _ = Command::new("kill").args(["-KILL", &system]).status();
    }
}
