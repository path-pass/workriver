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
    let _serve = Serve(cli().args(["serve", "--terminal-app", ""]).env("RUST_LOG", "debug").stdout(out.try_clone().unwrap()).stderr(out).spawn().unwrap());
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
}
