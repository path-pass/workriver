//! Telling the user from everyone else. The user is whoever holds the user token, a file
//! only the user's account can read (`<home>/user-token`): the CLI sends it, and the web UI
//! gets a session cookie for a one-time code that `workriver open` (or the daemon's start
//! line) puts into the page's address. A session is a random value that nothing derives:
//! the service keeps only its hash (`<home>/user-sessions`), so neither the token nor that
//! file yields one.
//!
//! What this is worth: agents run under the same account and could read the file. It stops
//! a request that merely claims to be the user (a prompt-injected `curl`, a remote machine),
//! not a program on this machine that sets out to take the token.
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

/// The session cookie's name.
pub const COOKIE: &str = "workriver_session";
/// How long a session lasts; the page asks for a new one after that.
pub const SESSION_DAYS: u64 = 30;
/// How long a one-time code can be used: one handed straight to the browser.
const CODE_LIFE: Duration = Duration::from_secs(5 * 60);
/// One printed in a terminal, where others may read it.
const PRINTED_LIFE: Duration = Duration::from_secs(60);

pub fn token_path(home: &Path) -> PathBuf {
    home.join("user-token")
}

/// The sessions the page was given: a hash and the second it ends, one per line.
pub fn sessions_path(home: &Path) -> PathBuf {
    home.join("user-sessions")
}

fn hash(value: &str) -> String {
    Sha256::digest(value.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(u64::MAX)
}

/// The token as the CLI reads it; None when there is none or it cannot be read.
pub fn read_token(home: &Path) -> Option<String> {
    std::fs::read_to_string(token_path(home)).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

/// `bytes` random bytes from the system, as hex.
pub fn random_hex(bytes: usize) -> std::io::Result<String> {
    let mut buf = vec![0u8; bytes];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut buf)?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// Equal, in time that does not depend on where they differ.
pub fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

pub struct Auth {
    token: String,
    /// One-time codes not yet used, with when each was made and how long it lasts.
    codes: Mutex<Vec<(String, Instant, Duration)>>,
    life: Duration,
    /// The hash of each session given out, with the second it ends.
    sessions: Mutex<Vec<(String, u64)>>,
    home: PathBuf,
}

impl Auth {
    /// Reads the token, making it first if there is none: 32 random bytes, for the user only.
    pub fn open(home: &Path) -> std::io::Result<Auth> {
        let path = token_path(home);
        let token = match read_token(home) {
            Some(t) => t,
            None => {
                // Written whole under another name, then given its name only if there is none
                // yet: two starting at once end up with the same token, never half of one.
                let t = random_hex(32)?;
                let tmp = home.join(format!("user-token.{}.tmp", random_hex(4)?));
                let mut opts = std::fs::OpenOptions::new();
                opts.write(true).create_new(true);
                #[cfg(unix)]
                std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
                opts.open(&tmp)?.write_all(t.as_bytes())?;
                let made = std::fs::hard_link(&tmp, &path);
                let _ = std::fs::remove_file(&tmp);
                match made {
                    Ok(()) => t,
                    Err(e) => read_token(home).ok_or(e)?,
                }
            }
        };
        // A token someone else can read is no token; tighten it rather than trust it.
        #[cfg(unix)]
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
        // Sessions given before a restart still count; a file that cannot be read gives none.
        let sessions = std::fs::read_to_string(sessions_path(home)).unwrap_or_default();
        let sessions = sessions.lines().filter_map(|l| l.split_once(' ')).filter_map(|(h, until)| Some((h.to_string(), until.parse().ok()?))).collect();
        Ok(Auth { token, codes: Mutex::default(), life: CODE_LIFE, sessions: Mutex::new(sessions), home: home.to_path_buf() })
    }

    #[cfg(test)]
    pub fn with_code_life(mut self, life: Duration) -> Auth {
        self.life = life;
        self
    }

    /// An `Authorization` header that carries the token.
    pub fn bearer(&self, header: Option<&str>) -> bool {
        header.and_then(|h| h.strip_prefix("Bearer ")).is_some_and(|t| same(t.trim(), &self.token))
    }

    /// A new one-time code for the page; `printed`: it will show in a terminal, so it
    /// lasts a minute only.
    pub fn code(&self, printed: bool) -> std::io::Result<String> {
        let code = random_hex(16)?;
        let mut codes = self.codes.lock().unwrap();
        codes.retain(|(_, at, life)| at.elapsed() < *life);
        codes.push((code.clone(), Instant::now(), if printed { PRINTED_LIFE.min(self.life) } else { self.life }));
        Ok(code)
    }

    /// Takes a code, once and in time: the session cookie's value for it.
    pub fn redeem(&self, code: &str) -> Option<String> {
        let mut codes = self.codes.lock().unwrap();
        codes.retain(|(_, at, life)| at.elapsed() < *life);
        let k = codes.iter().position(|(c, _, _)| same(c, code))?;
        codes.remove(k);
        let value = random_hex(32).ok()?;
        let mut sessions = self.sessions.lock().unwrap();
        let now = now();
        sessions.retain(|(_, until)| *until > now);
        sessions.push((hash(&value), now + SESSION_DAYS * 24 * 3600));
        // Kept so that it outlives a restart; if it cannot be, it lasts until then.
        if let Err(e) = self.keep(&sessions) {
            tracing::warn!("cannot keep the page's session in {}: {e}; it ends when the service stops", sessions_path(&self.home).display());
        }
        Some(value)
    }

    /// Writes the sessions whole under another name, then gives it the file's name.
    fn keep(&self, sessions: &[(String, u64)]) -> std::io::Result<()> {
        let tmp = self.home.join(format!("user-sessions.{}.tmp", random_hex(4)?));
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
        let text: String = sessions.iter().map(|(h, until)| format!("{h} {until}\n")).collect();
        let written = opts.open(&tmp).and_then(|mut f| f.write_all(text.as_bytes())).and_then(|()| std::fs::rename(&tmp, sessions_path(&self.home)));
        if written.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        written
    }

    /// A `Cookie` header that carries a session of ours, given by `redeem` and not over.
    pub fn session(&self, cookies: Option<&str>) -> bool {
        let value = cookies.into_iter().flat_map(|c| c.split(';')).filter_map(|c| c.trim().strip_prefix(COOKIE)?.strip_prefix('=')).next();
        let Some(value) = value.filter(|v| !v.is_empty()) else { return false };
        let (h, now) = (hash(value), now());
        self.sessions.lock().unwrap().iter().any(|(kept, until)| *until > now && same(kept, &h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_codes_and_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let auth = Auth::open(dir.path()).unwrap().with_code_life(Duration::from_millis(300));
        let token = read_token(dir.path()).unwrap();
        assert_eq!(token.len(), 64);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(token_path(dir.path())).unwrap().permissions().mode() & 0o777, 0o600);
            // A token left readable by others is tightened, and kept, at the next start.
            std::fs::set_permissions(token_path(dir.path()), std::fs::Permissions::from_mode(0o644)).unwrap();
            Auth::open(dir.path()).unwrap();
            assert_eq!(std::fs::metadata(token_path(dir.path())).unwrap().permissions().mode() & 0o777, 0o600);
            assert_eq!(read_token(dir.path()).unwrap(), token);
        }
        assert!(auth.bearer(Some(&format!("Bearer {token}"))));
        assert!(!auth.bearer(Some("Bearer nope")) && !auth.bearer(Some(&token)) && !auth.bearer(None));

        // A code works once.
        let code = auth.code(false).unwrap();
        let cookie = auth.redeem(&code).unwrap();
        assert!(auth.redeem(&code).is_none(), "single use");
        assert!(auth.session(Some(&format!("theme=dark; {COOKIE}={cookie}"))));
        assert!(!auth.session(Some(&format!("{COOKIE}={}x", cookie))) && !auth.session(None));
        // Whoever reads the token cannot make a session from it: the old form, a random part
        // signed with the token, is nobody's.
        let nonce = "0123456789abcdef0123456789abcdef";
        let signed: String = Sha256::digest(format!("{token}:{nonce}")).iter().map(|b| format!("{b:02x}")).collect();
        assert!(!auth.session(Some(&format!("{COOKIE}={nonce}.{signed}"))), "a cookie computed from the token");
        // A session outlives a restart, and the file it is kept in holds no cookie.
        assert!(Auth::open(dir.path()).unwrap().session(Some(&format!("{COOKIE}={cookie}"))), "after a restart");
        let kept = std::fs::read_to_string(sessions_path(dir.path())).unwrap();
        assert!(!kept.contains(&cookie) && kept.lines().count() == 1, "{kept}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(sessions_path(dir.path())).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // One past its days is over, and is dropped when the next is kept.
        let old = auth.redeem(&auth.code(false).unwrap()).unwrap();
        auth.sessions.lock().unwrap().iter_mut().filter(|(h, _)| *h == hash(&old)).for_each(|(_, until)| *until = 0);
        assert!(!auth.session(Some(&format!("{COOKIE}={old}"))), "expired");
        auth.redeem(&auth.code(false).unwrap()).unwrap();
        assert_eq!(std::fs::read_to_string(sessions_path(dir.path())).unwrap().lines().count(), 2, "the expired one is gone");
        // And only in time.
        let late = auth.code(true).unwrap();
        std::thread::sleep(Duration::from_millis(400));
        assert!(auth.redeem(&late).is_none(), "expired");
        // Another home knows none of these sessions.
        let other = tempfile::tempdir().unwrap();
        assert!(!Auth::open(other.path()).unwrap().session(Some(&format!("{COOKIE}={cookie}"))));
    }
}
