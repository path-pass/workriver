// Herdr's socket: where it is, the protocol spoken on it, one checked request per connection.
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::{Value, json};
use workriver_core::runtime::RuntimeError;

use super::Herdr;

/// The protocol this was written against (Herdr 0.9).
pub const PROTOCOL: u64 = 22;

/// `HERDR_SOCKET_PATH`, or Herdr's default socket.
pub fn socket_path() -> PathBuf {
    match std::env::var_os("HERDR_SOCKET_PATH") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => std::env::home_dir().unwrap_or_default().join(".config/herdr/herdr.sock"),
    }
}

/// Whether a Herdr process may be serving at `socket` although its file is not there: one
/// holds a listener bound at that path, or that cannot be told.
pub fn serves_there(socket: &std::path::Path) -> bool {
    crate::common::process::unix_bound(socket, "herdr") != Some(false)
}

impl Herdr {
    /// One request on its own connection. Herdr is not running (`None`, so it has no place)
    /// only when its socket is not there and no Herdr process holds a listener bound at its
    /// path; a socket that is there but does not answer, or one whose file is gone while its
    /// listener lives (its terminals may still run), is `Unreachable`: nothing can be said of
    /// its places.
    pub(super) fn call(&self, method: &str, params: Value) -> Result<Value, RuntimeError> {
        self.call_within(method, params, Duration::from_secs(10))
    }

    /// The same, for a request that answers only after `wait` at most (starting an agent).
    pub(super) fn call_within(&self, method: &str, params: Value, wait: Duration) -> Result<Value, RuntimeError> {
        let fail = |e: std::io::Error| RuntimeError::Failed(format!("herdr {method}: {e}"));
        let stream = UnixStream::connect(&self.socket).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound if !(self.may_run)(&self.socket) => RuntimeError::None,
            _ => RuntimeError::Unreachable(format!("Herdr does not answer at {}: {e}", self.socket.display())),
        })?;
        stream.set_read_timeout(Some(wait)).map_err(fail)?;
        stream.set_write_timeout(Some(Duration::from_secs(5))).map_err(fail)?;
        let id = format!("workriver:{}", self.ids.fetch_add(1, Ordering::Relaxed));
        let mut line = json!({ "id": id, "method": method, "params": params }).to_string();
        line.push('\n');
        (&stream).write_all(line.as_bytes()).map_err(fail)?;
        let mut answer = String::new();
        BufReader::new(&stream).read_line(&mut answer).map_err(fail)?;
        let v: Value = serde_json::from_str(&answer).map_err(|e| RuntimeError::Failed(format!("herdr {method}: unreadable answer: {e}")))?;
        if let Some(e) = v.get("error") {
            let (message, code) = (e["message"].as_str().unwrap_or("?"), e["code"].as_str().unwrap_or("?"));
            let words = format!("herdr {method}: {message} ({code})");
            return Err(match code {
                "agent_blocked" => RuntimeError::Blocked(words),
                "agent_not_ready" => RuntimeError::NotReady(words),
                _ => RuntimeError::Failed(words),
            });
        }
        v.get("result").cloned().ok_or_else(|| RuntimeError::Failed(format!("herdr {method}: answer without a result")))
    }

    /// A line on what is wrong with the connection, or None when Herdr answers with our protocol.
    pub fn check(&self) -> Option<String> {
        match self.call("ping", json!({})) {
            Ok(r) if r["protocol"].as_u64() == Some(PROTOCOL) => None,
            Ok(r) => Some(format!("Herdr {} speaks protocol {}, workriver expects {PROTOCOL}", r["version"].as_str().unwrap_or("?"), r["protocol"])),
            Err(RuntimeError::None) => Some(format!("Herdr is not running (no socket at {})", self.socket.display())),
            Err(e) => Some(e.to_string()),
        }
    }
}
