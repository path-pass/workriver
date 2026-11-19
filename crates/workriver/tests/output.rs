//! What `workriver serve` writes to its terminal is on a screen agents can read (a Herdr
//! pane): it never carries anything that signs anyone in. Runs the real binary.
use std::io::{Read, Write};
use std::process::Command;
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_workriver");

/// Ends the service with the test, however the test ends.
struct Serve(std::process::Child);

impl Drop for Serve {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn serve_prints_nothing_that_signs_in() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let cli = || {
        let mut c = Command::new(BIN);
        c.env("WORKRIVER_HOME", &home).env("WORKRIVER_PORT", port.to_string()).env("HERDR_SOCKET_PATH", tmp.path().join("none.sock"));
        for k in ["HERDR_ENV", "HERDR_PANE_ID", "HERDR_WORKSPACE_ID", "WORKRIVER_AS", "WORKRIVER_ALLOW_AS"] {
            c.env_remove(k);
        }
        c
    };
    let log = tmp.path().join("serve.log");
    let out = std::fs::File::create(&log).unwrap();
    // A test run has no terminal, and outside every pane the user's commands need one: the
    // service is started as tests start it (`WORKRIVER_ALLOW_AS=1` drops that, see security.md).
    let _serve = Serve(cli().args(["serve", "--terminal-app", ""]).env("WORKRIVER_ALLOW_AS", "1").env("RUST_LOG", "debug").stdout(out.try_clone().unwrap()).stderr(out).spawn().unwrap());
    let until = Instant::now() + Duration::from_secs(20);
    let status = loop {
        let o = cli().arg("status").output().unwrap();
        if o.status.success() {
            break String::from_utf8_lossy(&o.stdout).into_owned();
        }
        assert!(Instant::now() < until, "the service did not start: {}", std::fs::read_to_string(&log).unwrap_or_default());
        std::thread::sleep(Duration::from_millis(200));
    };
    let token = std::fs::read_to_string(home.join("user-token")).unwrap();

    // A printed link: on the CLI's own output only, with a warning; and it signs in once.
    let printed = cli().args(["open", "--print"]).output().unwrap();
    let url = String::from_utf8_lossy(&printed.stdout).trim().to_string();
    let code = url.split_once("#k=").expect("a link with a code").1.to_string();
    assert!(String::from_utf8_lossy(&printed.stderr).contains("Anyone who can read this terminal"), "{printed:?}");
    let post = |body: &str| {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(s, "POST /api/session HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        let mut answer = String::new();
        s.read_to_string(&mut answer).unwrap();
        answer
    };
    let signed = post(&format!("{{\"code\":\"{code}\"}}"));
    let cookie = signed.lines().find(|l| l.to_lowercase().starts_with("set-cookie:")).expect("a session").split_once('=').unwrap().1.split(';').next().unwrap().to_string();
    assert!(post(&format!("{{\"code\":\"{code}\"}}")).starts_with("HTTP/1.1 401"), "once");
    // Things that fail, and things that list: errors and listings are output too.
    let refused = Command::new(BIN).env("WORKRIVER_HOME", tmp.path().join("no-home")).env("WORKRIVER_PORT", port.to_string()).env_remove("HERDR_PANE_ID").args(["finding", "decide", "u1#1", "P1-1", "accept"]).output().unwrap();
    assert!(!refused.status.success());
    let listed = cli().args(["machine", "list"]).output().unwrap();
    std::thread::sleep(Duration::from_millis(300));

    let said = std::fs::read_to_string(&log).unwrap();
    assert!(said.contains(&format!("workriver on http://127.0.0.1:{port}")) && said.contains("workriver open"), "{said}");
    for (what, secret) in [("the token", token.trim()), ("a one-time code", code.as_str()), ("a session", cookie.as_str())] {
        assert!(secret.len() >= 32, "{what}");
        for (place, text) in [("serve's output", said.as_str()), ("status", status.as_str()), ("an error", &String::from_utf8_lossy(&refused.stderr)), ("machine list", &String::from_utf8_lossy(&listed.stdout))] {
            assert!(!text.contains(secret), "{what} in {place}: {text}");
        }
    }
    assert!(!said.contains("#k=") && !status.contains("#k="), "no sign-in link anywhere: {said}");

    // The timing log: requests, snapshots and units are in it; no secret, no content, no path.
    let repo = tmp.path().join("repo-SECRET-PATH");
    std::fs::create_dir(&repo).unwrap();
    for args in [&["init", "-q", "-b", "main"][..], &["-c", "user.name=a", "-c", "user.email=a@example.invalid", "commit", "-q", "--allow-empty", "-m", "SECRET-COMMIT-MESSAGE"]] {
        assert!(Command::new("git").arg("-C").arg(&repo).args(args).status().unwrap().success());
    }
    assert!(cli().args(["backlog", "add", "Login", "--goal", "Safe login", "--project", &repo.display().to_string()]).status().unwrap().success());
    assert!(cli().args(["unit", "add", "i1", "--title", "Auth", "--branch", "feat/auth", "--base", "main", "--repo", &repo.display().to_string()]).status().unwrap().success());
    assert!(cli().args(["task", "add", "u1", "--title", "Check", "--text", "SECRET-TASK-TEXT asks for this"]).status().unwrap().success());
    let shown = cli().args(["task", "show", "u1#1"]).output().unwrap();
    assert!(String::from_utf8_lossy(&shown.stdout).contains("SECRET-TASK-TEXT"));
    let get = |path: &str| {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(s, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nCookie: workriver_session={cookie}\r\n\r\n").unwrap();
        let mut answer = String::new();
        s.read_to_string(&mut answer).unwrap();
        answer
    };
    assert!(get("/api/workspace").starts_with("HTTP/1.1 200"));
    assert!(cli().arg("board").status().unwrap().success());
    let timing = || std::fs::read_to_string(home.join("logs/perf.jsonl")).unwrap_or_default();
    let until = Instant::now() + Duration::from_secs(5);
    while !timing().contains("\"route\":\"/api/task\"") {
        assert!(Instant::now() < until, "no request in the timing log: {}", timing());
        std::thread::sleep(Duration::from_millis(100));
    }
    let lines = timing();
    for kind in ["\"kind\":\"http\"", "\"kind\":\"snapshot\"", "\"kind\":\"unit\"", "\"route\":\"/api/workspace\""] {
        assert!(lines.contains(kind), "{kind} in {lines}");
    }
    let perf = cli().arg("perf").output().unwrap();
    let summary = String::from_utf8_lossy(&perf.stdout).into_owned();
    assert!(summary.contains("GET /api/workspace") && summary.contains("Snapshots"), "{summary}");
    let bundle = cli().args(["perf", "--bundle"]).output().unwrap();
    let bundle = std::fs::read_to_string(String::from_utf8_lossy(&bundle.stdout).trim()).unwrap();
    for text in [&lines, &summary, &bundle] {
        for secret in [token.trim(), cookie.as_str(), "SECRET-TASK-TEXT", "SECRET-COMMIT-MESSAGE", "SECRET-PATH", &home.display().to_string(), &tmp.path().display().to_string()] {
            assert!(!text.contains(secret), "{secret:?} in the timing log or its summary");
        }
    }
}

/// The CLI on a machine workriver follows: it finds the daemon by the port written there,
/// and never hangs on a tunnel that is reconnecting: a port nobody listens on (refused), or
/// the forward of a dead connection that accepts and never answers.
#[test]
fn the_cli_over_there_never_hangs() {
    let tmp = tempfile::tempdir().unwrap();
    let remote_home = tmp.path().join("box-home");
    std::fs::create_dir_all(remote_home.join(".config/workriver")).unwrap();
    let dead = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_port = dead.local_addr().unwrap().port();
    let refused = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    std::fs::write(remote_home.join(".config/workriver/daemon"), "the-mac\n").unwrap();
    let run = |port: u16| {
        std::fs::write(remote_home.join(".config/workriver/port"), format!("{port}\n")).unwrap();
        let mut c = Command::new(BIN);
        c.env("HOME", &remote_home).env("WORKRIVER_HOME", tmp.path().join("nothing")).env_remove("WORKRIVER_PORT");
        for k in ["HERDR_ENV", "HERDR_PANE_ID", "HERDR_WORKSPACE_ID"] {
            c.env_remove(k);
        }
        let started = Instant::now();
        let out = c.args(["board"]).output().unwrap();
        (started.elapsed(), String::from_utf8_lossy(&out.stderr).into_owned(), out.status.success())
    };
    for (port, limit) in [(refused, Duration::from_secs(4)), (dead_port, Duration::from_secs(4))] {
        let (took, err, ok) = run(port);
        assert!(!ok && took < limit, "{took:?}: {err}");
        assert!(err.contains(&format!("cannot reach workriver on the-mac through the ssh tunnel (port {port}, from ~/.config/workriver/port; it may be reconnecting); try again shortly")), "{err}");
    }
    drop(dead);
}

/// Ctrl-C stops the service even while the page holds its event stream open, within a few
/// seconds, and the connections to other machines end with it.
#[test]
fn ctrl_c_stops_the_service() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir(&home).unwrap();
    // One machine followed, through an ssh stand-in that stays connected until killed.
    let bin = tmp.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let ssh = bin.join("ssh");
    std::fs::write(&ssh, "#!/bin/sh\ncase \"$1\" in -N) echo 'Allocated port 45123 for remote forward to 127.0.0.1:1' >&2; exec sleep 6123;; esac\nexit 0\n").unwrap();
    std::fs::set_permissions(&ssh, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    std::fs::write(home.join("settings.json"), r#"{"machines":{"box":{"ssh":"box","herdrSocket":"/run/herdr.sock","enabled":true}}}"#).unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
    let mut c = Command::new(BIN);
    c.env("WORKRIVER_HOME", &home).env("WORKRIVER_PORT", port.to_string()).env("HERDR_SOCKET_PATH", tmp.path().join("none.sock")).env("PATH", &path);
    for k in ["HERDR_ENV", "HERDR_PANE_ID", "HERDR_WORKSPACE_ID"] {
        c.env_remove(k);
    }
    let log = std::fs::File::create(tmp.path().join("serve.log")).unwrap();
    // As tests start it: the user's `open` below runs with no terminal.
    let mut serve = Serve(c.args(["serve", "--terminal-app", ""]).env("WORKRIVER_ALLOW_AS", "1").stdout(log.try_clone().unwrap()).stderr(log).spawn().unwrap());
    let sleeping = || {
        let out = Command::new("pgrep").args(["-f", "^sleep 6123$"]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).lines().count()
    };
    let until = Instant::now() + Duration::from_secs(20);
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() || sleeping() == 0 {
        assert!(Instant::now() < until, "the service or its tunnel did not start: {}", std::fs::read_to_string(tmp.path().join("serve.log")).unwrap_or_default());
        std::thread::sleep(Duration::from_millis(100));
    }
    // The page's event stream, held open: the page signs in with a one-time code and holds
    // a session (the token alone shows no process and is nobody).
    let printed = Command::new(BIN).env("WORKRIVER_HOME", &home).env("WORKRIVER_PORT", port.to_string()).env_remove("HERDR_PANE_ID").args(["open", "--print"]).output().unwrap();
    let url = String::from_utf8_lossy(&printed.stdout).trim().to_string();
    let code = url.split_once("#k=").unwrap_or_else(|| panic!("a link with a code: {printed:?}")).1.to_string();
    let body = format!("{{\"code\":\"{code}\"}}");
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(s, "POST /api/session HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let mut signed = String::new();
    s.read_to_string(&mut signed).unwrap();
    let cookie = signed.lines().find(|l| l.to_lowercase().starts_with("set-cookie:")).expect("a session").split_once(':').unwrap().1.split(';').next().unwrap().trim().to_string();
    let mut sse = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(sse, "GET /api/events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nCookie: {cookie}\r\n\r\n").unwrap();
    let mut first = [0u8; 64];
    let n = sse.read(&mut first).unwrap();
    assert!(String::from_utf8_lossy(&first[..n]).starts_with("HTTP/1.1 200"), "{}", String::from_utf8_lossy(&first[..n]));

    let started = Instant::now();
    Command::new("kill").args(["-INT", &serve.0.id().to_string()]).status().unwrap();
    let status = loop {
        if let Some(s) = serve.0.try_wait().unwrap() {
            break s;
        }
        assert!(started.elapsed() < Duration::from_secs(3), "still running 3 s after Ctrl-C: {}", std::fs::read_to_string(tmp.path().join("serve.log")).unwrap_or_default());
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success(), "{status:?}");
    // The stream was ended, not left hanging; the tunnel's ssh is gone too.
    let mut rest = vec![];
    let _ = sse.read_to_end(&mut rest);
    let until = Instant::now() + Duration::from_secs(2);
    while sleeping() > 0 {
        assert!(Instant::now() < until, "the tunnel's ssh outlived the service");
        std::thread::sleep(Duration::from_millis(50));
    }
}
