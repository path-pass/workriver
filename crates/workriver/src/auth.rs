//! Telling the user from everyone else. The user is whoever holds the user token, a file
//! only the user's account can read (`<home>/user-token`): the CLI sends it, and the web UI
//! gets a session cookie for a one-time code that `workriver open` (or the daemon's start
//! line) puts into the page's address.
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
/// How long a one-time code can be used.
const CODE_LIFE: Duration = Duration::from_secs(5 * 60);

pub fn token_path(home: &Path) -> PathBuf {
    home.join("user-token")
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
    /// One-time codes not yet used, with when each was made.
    codes: Mutex<Vec<(String, Instant)>>,
    life: Duration,
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
        Ok(Auth { token, codes: Mutex::default(), life: CODE_LIFE })
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

    /// A new one-time code for the page.
    pub fn code(&self) -> std::io::Result<String> {
        let code = random_hex(16)?;
        let mut codes = self.codes.lock().unwrap();
        codes.retain(|(_, at)| at.elapsed() < self.life);
        codes.push((code.clone(), Instant::now()));
        Ok(code)
    }

    /// Takes a code, once and in time: the session cookie's value for it.
    pub fn redeem(&self, code: &str) -> Option<String> {
        let mut codes = self.codes.lock().unwrap();
        codes.retain(|(_, at)| at.elapsed() < self.life);
        let k = codes.iter().position(|(c, _)| same(c, code))?;
        codes.remove(k);
        let nonce = random_hex(16).ok()?;
        Some(format!("{nonce}.{}", self.sign(&nonce)))
    }

    /// A session is a random part signed with the token, so it needs no list and outlives a
    /// restart; a new token ends every session.
    fn sign(&self, nonce: &str) -> String {
        Sha256::digest(format!("{}:{nonce}", self.token)).iter().map(|b| format!("{b:02x}")).collect()
    }

    /// A `Cookie` header that carries a session of ours.
    pub fn session(&self, cookies: Option<&str>) -> bool {
        let value = cookies.into_iter().flat_map(|c| c.split(';')).filter_map(|c| c.trim().strip_prefix(COOKIE)?.strip_prefix('=')).next();
        let Some((nonce, mac)) = value.and_then(|v| v.split_once('.')) else { return false };
        // The fixed shape of the random part keeps anything else from being signed for.
        nonce.len() == 32 && nonce.bytes().all(|b| b.is_ascii_hexdigit()) && same(mac, &self.sign(nonce))
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
        let code = auth.code().unwrap();
        let cookie = auth.redeem(&code).unwrap();
        assert!(auth.redeem(&code).is_none(), "single use");
        assert!(auth.session(Some(&format!("theme=dark; {COOKIE}={cookie}"))));
        assert!(!auth.session(Some(&format!("{COOKIE}={}x", cookie))) && !auth.session(None));
        // Only the fixed shape is ever accepted, whatever it is signed with.
        let long = format!("{}00", cookie.split_once('.').unwrap().0);
        assert!(!auth.session(Some(&format!("{COOKIE}={long}.{}", auth.sign(&long)))));
        // And only in time.
        let late = auth.code().unwrap();
        std::thread::sleep(Duration::from_millis(400));
        assert!(auth.redeem(&late).is_none(), "expired");
        // Another token (another home) knows none of these sessions.
        let other = tempfile::tempdir().unwrap();
        assert!(!Auth::open(other.path()).unwrap().session(Some(&format!("{COOKIE}={cookie}"))));
    }
}
