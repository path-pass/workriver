// One ssh per followed machine, kept up: `-L` brings that machine's runtime socket here,
// `-R` gives the `workriver` CLI over there a port that leads back to this daemon. It never
// asks for anything (BatchMode) and is restarted with growing waits.
//
// The port over there is a new one for every connection, chosen by ssh (`-R 0`), and
// written over there to `~/.config/workriver/port` once the connection is up; the CLI there
// reads it. A fixed port would be held for many minutes by the session of a connection
// that died without closing (a computer that slept, a network that changed): the server
// does not notice that its client is gone, and every reconnect would fail on it.
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use crate::common::home::{HOST_FILE, PORT_FILE};

#[derive(Default, Clone, Debug, PartialEq)]
pub struct Tunnel {
    pub up: bool,
    pub error: Option<String>,
    /// The port ssh got over there for this connection.
    pub port: Option<u16>,
    /// When the last connection ended (RFC 3339).
    pub since: Option<String>,
    /// When the next attempt starts.
    pub retry: Option<Instant>,
}

/// The ssh options every connection of ours uses, whatever the user's ssh config says:
/// never ask, never forward the user's ssh agent to the machine, never take a host key
/// that is not known yet, give up on a dead line: no answer to connect in 5 s, or to three
/// keep-alives 10 s apart (30 s).
pub const QUIET: [&str; 6] = ["BatchMode=yes", "ForwardAgent=no", "StrictHostKeyChecking=yes", "ConnectTimeout=5", "ServerAliveInterval=10", "ServerAliveCountMax=3"];

/// What to do about an ssh failure the user can fix, after ssh's own words.
pub fn hint(error: &str, ssh: &str) -> String {
    let e = error.to_lowercase();
    if e.contains("host key verification failed") || e.contains("no matching host key") || e.contains("remote host identification has changed") {
        format!("{error} (workriver takes no host key by itself: connect once by hand to check and accept it, `ssh {ssh}`)")
    } else if e.contains("forwarding") || e.contains("cannot listen") || e.contains("address already in use") {
        // ponytail: forwards from the user's config cannot be cleared without clearing ours
        // (ClearAllForwardings clears both); a Host alias of workriver's own is the way out.
        format!("{error} (if your ssh config sets LocalForward/RemoteForward for this host, they are set up on workriver's connection too and can fail it: give workriver a Host alias without them)")
    } else {
        error.to_string()
    }
}

/// This computer's name, for the CLI over there to say where it could not reach.
pub fn this_host() -> String {
    let out = Command::new("hostname").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    let plain: String = out.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).collect();
    if plain.is_empty() { "the computer workriver runs on".into() } else { plain }
}

/// Writes the port of a new connection over there (and this computer's name), atomically:
/// from now on the CLI there reaches this daemon through it, and nothing points at an
/// earlier connection's port.
pub fn publish(program: &str, target: &str, port: u16, host: &str) -> Result<(), String> {
    let script = format!(
        "umask 077; d=\"$HOME/.config/workriver\"; mkdir -p \"$d\" && printf '%s\\n' {port} > \"$d/port.$$\" && mv -f \"$d/port.$$\" \"$HOME/{PORT_FILE}\" && printf '%s\\n' '{host}' > \"$HOME/{HOST_FILE}\""
    );
    let mut cmd = Command::new(program);
    for o in QUIET {
        cmd.args(["-o", o]);
    }
    cmd.args(["-o", "ClearAllForwardings=yes", "--", target, &script]);
    let started = Instant::now();
    let out = crate::common::process::output_within(cmd, Duration::from_secs(15)).map_err(|e| format!("cannot run ssh: {e}"))?;
    tracing::debug!(target: "perf", kind = "ssh", purpose = "publish", ms = started.elapsed().as_millis() as u64, code = out.status.code().unwrap_or(-1));
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("cannot write the port over there: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("").trim()))
    }
}

/// Sleeps `d`, waking early when `stop` is set or the computer has slept meanwhile.
pub fn nap(d: Duration, stop: &AtomicBool) {
    let until = Instant::now() + d;
    let mut clock = Clock::now();
    while Instant::now() < until && !stop.load(Ordering::Relaxed) && !clock.woke() {
        std::thread::sleep(Duration::from_millis(50).min(d));
    }
}

/// Tells that the computer slept: the wall clock went on while the monotonic one (which
/// stands still in sleep, on macOS and Linux alike) did not.
struct Clock {
    wall: SystemTime,
    mono: Instant,
}

impl Clock {
    fn now() -> Clock {
        Clock { wall: SystemTime::now(), mono: Instant::now() }
    }

    /// Whether it slept since the last look.
    fn woke(&mut self) -> bool {
        let now = Clock::now();
        let wall = now.wall.duration_since(self.wall).unwrap_or_default();
        let mono = now.mono.duration_since(self.mono);
        *self = now;
        slept(wall, mono)
    }
}

/// More wall-clock time passed than monotonic time, by more than a stall would explain.
pub fn slept(wall: Duration, mono: Duration) -> bool {
    wall > mono + Duration::from_secs(5)
}

// ---- the connection: one ssh per machine, both forwards ----

/// The ssh that brings the machine's runtime sockets here (`sockets`: each as (here, there))
/// and forwards a port on the machine, which ssh picks and names on stderr ("Allocated
/// port N for remote forward…", hence LogLevel INFO whatever the config says), to this
/// daemon's `local_port`. It never asks for anything and ends if a forward cannot be set
/// up or the connection dies.
pub fn tunnel_words(target: &str, sockets: &[(String, String)], local_port: u16) -> Vec<String> {
    let mut w: Vec<String> = vec!["ssh".into(), "-N".into()];
    for o in QUIET.iter().chain(&["ExitOnForwardFailure=yes", "StreamLocalBindUnlink=yes", "LogLevel=INFO"]) {
        w.extend(["-o".to_string(), o.to_string()]);
    }
    for (here, there) in sockets {
        w.extend(["-L".to_string(), format!("{here}:{there}")]);
    }
    w.extend(["-R".to_string(), format!("127.0.0.1:0:127.0.0.1:{local_port}")]);
    w.extend(["--".to_string(), target.to_string()]);
    w
}

/// The port ssh says it got over there, from a line of its stderr.
pub fn allocated(line: &str) -> Option<u16> {
    line.trim().strip_prefix("Allocated port ")?.split_whitespace().next()?.parse().ok()
}

/// The supervisor's waits; shortened in tests.
#[derive(Clone, Copy, Debug)]
pub struct Pace {
    /// The wait before a restart: `min`, doubling to `max`; back to `min` after a run of `healthy`.
    pub min: Duration,
    pub max: Duration,
    pub healthy: Duration,
}

impl Default for Pace {
    fn default() -> Self {
        Pace { min: Duration::from_secs(2), max: Duration::from_secs(60), healthy: Duration::from_secs(60) }
    }
}

/// Keeps the command (the tunnel's ssh) running until `stop`: restarts it when it ends,
/// waiting longer after each quick failure (up to `pace.max`), and records in `tunnel`
/// whether it is up, on which port, and why it last ended. It is up once ssh has named the
/// port it got over there and `publish` has written it there. It is down the moment ssh
/// ends; after the computer slept it is started again at once, since its old connection is
/// most likely dead without knowing it.
pub fn supervise(words: &[String], tunnel: &Mutex<Tunnel>, stop: &AtomicBool, pace: Pace, publish: &dyn Fn(u16) -> Result<(), String>) {
    // For the timing log: which machine (ssh's destination is the last word), since when down.
    let machine = words.last().cloned().unwrap_or_default();
    let mut down_at = Instant::now();
    // The command runs under a shell that ends it when our end of its stdin closes, so it
    // does not outlive the daemon however the daemon ends.
    const WRAP: &str = r#"exec 3<&0; "$@" </dev/null & pid=$!; (cat <&3 >/dev/null 2>&1; kill $pid 2>/dev/null) 2>/dev/null & wait $pid"#;
    let mut delay = pace.min;
    while !stop.load(Ordering::Relaxed) {
        let started = Instant::now();
        let spawned = Command::new("sh").arg("-c").arg(WRAP).arg("sh").args(words).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn();
        let mut again_now = false;
        let why = match spawned {
            Err(e) => format!("cannot run {}: {e}", words[0]),
            Ok(mut child) => {
                // ssh's stderr, line by line, as it comes: the port it got is said there.
                let (tx, rx) = std::sync::mpsc::channel::<String>();
                if let Some(err) = child.stderr.take() {
                    std::thread::spawn(move || {
                        for line in std::io::BufRead::lines(std::io::BufReader::new(err)).map_while(Result::ok) {
                            if tx.send(line).is_err() {
                                break;
                            }
                        }
                    });
                }
                let mut said: Vec<String> = vec![];
                let mut clock = Clock::now();
                let mut failed: Option<String> = None;
                loop {
                    while let Ok(line) = rx.try_recv() {
                        match allocated(&line) {
                            Some(port) if failed.is_none() => match publish(port) {
                                Ok(()) => {
                                    *tunnel.lock().unwrap() = Tunnel { up: true, port: Some(port), ..Tunnel::default() };
                                    tracing::info!(target: "perf", kind = "tunnel", event = "up", machine = %machine, connect_ms = started.elapsed().as_millis() as u64, down_ms = down_at.elapsed().as_millis() as u64);
                                }
                                Err(why) => failed = Some(why),
                            },
                            _ => said.push(line),
                        }
                    }
                    if failed.is_some() || matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
                        break;
                    }
                    if stop.load(Ordering::Relaxed) {
                        let _ = child.kill();
                        let _ = child.wait();
                        *tunnel.lock().unwrap() = Tunnel::default();
                        return;
                    }
                    if clock.woke() {
                        failed = Some("the computer slept; connecting again".into());
                        again_now = true;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                let _ = child.kill();
                let status = child.wait().ok();
                // Closing stdin ends the watcher that still holds stderr open.
                drop(child.stdin.take());
                while let Ok(line) = rx.recv_timeout(Duration::from_millis(200)) {
                    if allocated(&line).is_none() {
                        said.push(line);
                    }
                }
                let last = said.iter().rev().find(|l| !l.trim().is_empty()).map(|l| l.trim().to_string());
                failed.or(last).unwrap_or_else(|| format!("ended ({})", status.map(|s| s.to_string()).unwrap_or_default()))
            }
        };
        delay = if again_now || started.elapsed() >= pace.healthy { pace.min } else { delay };
        let wait = if again_now { Duration::ZERO } else { delay };
        {
            let mut t = tunnel.lock().unwrap();
            if t.up || t.error.as_deref() != Some(why.as_str()) {
                tracing::warn!("tunnel `{}`: {why}", words.join(" "));
            }
            if t.up {
                tracing::info!(target: "perf", kind = "tunnel", event = "down", machine = %machine, up_ms = started.elapsed().as_millis() as u64, error = crate::common::diagnostics::kind_of(&why));
                down_at = Instant::now();
            }
            *t = Tunnel { up: false, error: Some(why), port: None, since: Some(workriver_core::now()), retry: Some(Instant::now() + wait) };
        }
        nap(wait, stop);
        delay = (delay * 2).min(pace.max);
    }
    *tunnel.lock().unwrap() = Tunnel::default();
}
