//! Which pane a call truly comes from, end to end (docs/design/caller-identity.md): the real
//! binary, its real caller socket (the kernel's word for who connected), real processes, and
//! a stand-in for Herdr's socket that names real shells as the roots of its panes. The
//! service runs as it does for the user: no `--as`, a terminal asked for.
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_workriver");

/// Ends a process with the test, however the test ends.
struct Ends(Child);

impl Drop for Ends {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A pane as the stand-in Herdr tells it: its id, its shell's process id, the session of the
/// agent it holds (none: a plain pane).
type Panes = Arc<Mutex<Vec<(String, u32, Option<String>)>>>;

/// Herdr's socket, as much of it as the service asks: one request per connection.
fn herdr(path: &Path, panes: Panes) {
    let listener = UnixListener::bind(path).unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let mut line = String::new();
            if BufReader::new(&stream).read_line(&mut line).is_err() {
                continue;
            }
            let req: Value = serde_json::from_str(&line).unwrap_or_default();
            let panes = panes.lock().unwrap().clone();
            let result = match req["method"].as_str().unwrap_or_default() {
                "ping" => Ok(json!({ "version": "0.9.0", "protocol": 22 })),
                "session.snapshot" => Ok(json!({ "snapshot": {
                    "workspaces": [{ "workspace_id": "w1", "label": "exp" }],
                    "panes": panes.iter().map(|(id, ..)| json!({ "pane_id": id, "workspace_id": "w1" })).collect::<Vec<_>>(),
                    "agents": panes.iter().filter_map(|(id, _, s)| Some(json!({ "pane_id": id, "workspace_id": "w1", "agent": "anyharness", "agent_status": "idle", "agent_session": { "value": s.as_ref()? } }))).collect::<Vec<_>>(),
                }})),
                "pane.process_info" => match panes.iter().find(|(id, ..)| Some(id.as_str()) == req["params"]["pane_id"].as_str()) {
                    Some((id, shell, _)) => Ok(json!({ "process_info": { "pane_id": id, "shell_pid": shell, "foreground_processes": [] } })),
                    None => Err("pane_not_found"),
                },
                _ => Err("not_in_this_stand_in"),
            };
            let answer = match result {
                Ok(r) => json!({ "id": req["id"], "result": r }),
                Err(code) => json!({ "id": req["id"], "error": { "code": code, "message": code } }),
            };
            let _ = (&stream).write_all(format!("{answer}\n").as_bytes());
        }
    });
}

/// A real shell that stands for a pane's: commands typed into it run as its children.
struct Shell {
    child: Ends,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Shell {
    fn pid(&self) -> u32 {
        self.child.0.id()
    }

    /// Runs one line there; whether it succeeded, and everything it printed.
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
}

struct World {
    tmp: tempfile::TempDir,
    home: PathBuf,
    port: u16,
    panes: Panes,
    _serve: Ends,
}

impl World {
    /// The CLI with this service's home and port, and nothing of the test's own Herdr.
    fn cli(&self) -> Command {
        let mut c = Command::new(BIN);
        c.env("WORKRIVER_HOME", &self.home).env("WORKRIVER_PORT", self.port.to_string());
        for k in ["HERDR_ENV", "HERDR_PANE_ID", "HERDR_WORKSPACE_ID", "HERDR_SOCKET_PATH", "WORKRIVER_AS", "WORKRIVER_ALLOW_AS"] {
            c.env_remove(k);
        }
        c
    }

    /// A shell as Herdr starts one in a pane: the pane's id in its environment.
    fn shell(&self, pane: &str, session: Option<&str>) -> Shell {
        let mut c = Command::new("sh");
        c.env("WORKRIVER_HOME", &self.home).env("WORKRIVER_PORT", self.port.to_string()).env("HERDR_PANE_ID", pane).env("W", BIN);
        c.env_remove("WORKRIVER_AS").env_remove("WORKRIVER_ALLOW_AS");
        let mut child = c.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        let (stdin, stdout) = (child.stdin.take().unwrap(), BufReader::new(child.stdout.take().unwrap()));
        self.panes.lock().unwrap().push((pane.into(), child.id(), session.map(str::to_string)));
        Shell { child: Ends(child), stdin, stdout }
    }

    /// Who added each planned intent, as recorded: (title, by).
    fn recorded(&self) -> Vec<(String, String)> {
        let store = workriver_data::Store::open(&self.home.join("workriver.db")).unwrap();
        store.state().intents.iter().map(|i| (i.title.clone(), i.by.clone())).collect()
    }
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let panes: Panes = Arc::default();
    let socket = tmp.path().join("herdr.sock");
    herdr(&socket, panes.clone());
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let log = std::fs::File::create(tmp.path().join("serve.log")).unwrap();
    let mut serve = Command::new(BIN);
    serve.env("WORKRIVER_HOME", &home).env("WORKRIVER_PORT", port.to_string()).env("HERDR_SOCKET_PATH", &socket).env_remove("WORKRIVER_ALLOW_AS").env_remove("HERDR_PANE_ID");
    let serve = Ends(serve.args(["serve", "--terminal-app", ""]).stdout(log.try_clone().unwrap()).stderr(log).spawn().unwrap());
    let w = World { tmp, home, port, panes, _serve: serve };
    let until = Instant::now() + Duration::from_secs(20);
    while !w.cli().arg("status").output().unwrap().status.success() {
        assert!(Instant::now() < until, "the service did not start: {}", std::fs::read_to_string(w.tmp.path().join("serve.log")).unwrap_or_default());
        std::thread::sleep(Duration::from_millis(200));
    }
    w
}

fn said(o: &std::process::Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn the_service_takes_the_pane_from_the_process() {
    let w = world();
    let repo = w.tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    assert!(Command::new("git").arg("-C").arg(&repo).args(["init", "-q", "-b", "main"]).status().unwrap().success());
    let add = |title: &str| format!("\"$W\" backlog add '{title}' --goal g --project '{}'", repo.display());
    let mut plain = w.shell("w1:p1", None);
    let mut two = w.shell("w1:p2", Some("SESS-2"));

    // The socket is the account's alone, next to the token.
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(w.home.join("caller.sock")).unwrap().permissions().mode() & 0o777, 0o600);

    // A pane with no agent, with the token: the user. An agent's pane: that agent, though
    // the same token is in its reach.
    let (ok, out) = plain.run(&add("By the user"));
    assert!(ok, "{out}");
    let (ok, out) = two.run(&add("By two"));
    assert!(ok, "{out}");
    // From an agent's pane, naming none: still that agent, no longer the user.
    let (ok, out) = two.run(&format!("env -u HERDR_PANE_ID {}", add("Naming no pane")));
    assert!(ok, "{out}");
    // Naming another agent's pane, or a plain one, from an agent's pane: refused, both said.
    let (ok, out) = two.run(&format!("HERDR_PANE_ID=w1:p1 {}", add("As the user?")));
    assert!(!ok && out.contains("says it runs in pane w1:p1, but its process workriver(") && out.contains("runs in pane w1:p2") && out.contains("workriver guide how hand-off"), "{out}");
    // What is the user's alone is not an agent's, whatever it names.
    let (ok, out) = two.run("\"$W\" open --print");
    assert!(!ok && out.contains("the user's"), "{out}");

    // The incident this check was built for: a command run outside every pane, by a process that kept the
    // environment of a pane that is gone, with the token in reach. It was recorded as the
    // user; it is refused, and told why.
    let stale = w.cli().env("HERDR_PANE_ID", "wM:p4").args(["backlog", "add", "From a stale pane", "--goal", "g", "--project"]).arg(&repo).output().unwrap();
    let out = said(&stale);
    assert!(!stale.status.success() && out.contains("says it runs in pane wM:p4 (there is no pane wM:p4 any more)") && out.contains("runs in no pane"), "{out}");
    // The same process claiming a live agent's pane is not that agent.
    let claim = w.cli().env("HERDR_PANE_ID", "w1:p2").args(["backlog", "add", "As two?", "--goal", "g", "--project"]).arg(&repo).output().unwrap();
    assert!(!claim.status.success() && said(&claim).contains("says it runs in pane w1:p2") && said(&claim).contains("runs in no pane"), "{}", said(&claim));
    // Naming no pane, in none, without a terminal (a helper of a harness, a background job):
    // not the user, token or not.
    let mut helper = w.cli();
    helper.args(["backlog", "add", "From a helper", "--goal", "g", "--project"]).arg(&repo).stdin(Stdio::null());
    // SAFETY: only `setsid` between fork and exec, which is async-signal-safe.
    unsafe { helper.pre_exec(|| if libc::setsid() == -1 { Err(std::io::Error::last_os_error()) } else { Ok(()) }) };
    let helper = helper.output().unwrap();
    assert!(!helper.status.success() && said(&helper).contains("is in no pane and has no terminal"), "{}", said(&helper));

    // The token without a process (`curl`, a script, an older CLI) is nobody.
    let token = std::fs::read_to_string(w.home.join("user-token")).unwrap();
    let body = json!({ "kind": "intent.created", "body": { "title": "By curl", "state": "planned" } }).to_string();
    let mut s = std::net::TcpStream::connect(("127.0.0.1", w.port)).unwrap();
    write!(s, "POST /api/append HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nX-Workriver-Place: w1:p1\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", w.port, token.trim(), body.len()).unwrap();
    let mut answer = String::new();
    s.read_to_string(&mut answer).unwrap();
    assert!(answer.starts_with("HTTP/1.1 401") && answer.contains("does not show which process sends it"), "{answer}");

    // One session, one pane: another pane that reports an agent's session is not that agent
    // while the first pane's shell runs.
    let mut three = w.shell("w1:p3", Some("SESS-2"));
    let (ok, out) = three.run(&add("As two, from another pane"));
    assert!(!ok && out.contains("session SESS-2 was first seen in pane w1:p2, whose shell still runs"), "{out}");
    let (ok, out) = two.run(&add("Still two"));
    assert!(ok, "{out}");
    // The pin is in a file next to the database: a restart does not forget it.
    let pins: Value = serde_json::from_slice(&std::fs::read(w.home.join("session-roots.json")).unwrap()).unwrap();
    assert_eq!((pins["SESS-2"]["place"].as_str(), pins["SESS-2"]["pid"].as_u64()), (Some("w1:p2"), Some(two.pid() as u64)), "{pins}");

    // Nothing was recorded as anyone it was not.
    let want = [("By the user", "user"), ("By two", "SESS-2"), ("Naming no pane", "SESS-2"), ("Still two", "SESS-2")];
    assert_eq!(w.recorded(), want.map(|(t, by)| (t.to_string(), by.to_string())));
    // No refusal named a harness.
    let serve_log = std::fs::read_to_string(w.tmp.path().join("serve.log")).unwrap_or_default();
    assert!(!serve_log.to_lowercase().contains("codex"), "{serve_log}");
}
