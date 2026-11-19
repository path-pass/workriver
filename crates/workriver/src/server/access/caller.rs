// Which process a request comes from, by the kernel's word, and where that process runs.
//
// The CLI connects to a Unix socket of the service; the kernel says which process connected
// (nothing the caller can choose). The service reads that process's chain of parents and
// answers with a short-lived proof the CLI sends along with its HTTP requests. The place a
// caller is at is the one whose root process is in its chain (docs/design/caller-identity.md).
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use workriver_core::runtime::Root;

use crate::common::home::SOCKET;
use crate::common::process::{Chain, Proc, Procs, now};
use super::auth;

/// How long a proof stands for its process.
pub const PROOF: Duration = Duration::from_secs(30);

/// The place a process is at: the one whose root is the process or the nearest of its
/// parents; else the one whose root has the same controlling terminal (a job left behind by
/// `( cmd & )` keeps it); else none. `tty_of`: the terminal of a root.
pub fn place_of<'a>(chain: &[Proc], roots: &'a [Root], tty_of: &dyn Fn(u32) -> Option<String>) -> Option<&'a Root> {
    let own = chain.iter().find_map(|p| roots.iter().find(|r| r.pids().contains(&p.pid)));
    own.or_else(|| {
        let tty = chain.first()?.tty.as_ref()?;
        roots.iter().find(|r| r.pids().iter().any(|pid| tty_of(*pid).as_ref() == Some(tty)))
    })
}

/// The proofs given out and the chain each stands for.
pub struct Callers {
    proofs: Mutex<HashMap<String, (Chain, Instant)>>,
    ttl: Mutex<Duration>,
}

impl Default for Callers {
    fn default() -> Self {
        Callers { proofs: Mutex::default(), ttl: Mutex::new(PROOF) }
    }
}

impl Callers {
    /// A new proof for `chain`; none when no random bytes can be read.
    pub fn admit(&self, chain: Chain) -> Option<String> {
        let proof = random()?;
        let now = Instant::now();
        let mut proofs = self.proofs.lock().unwrap();
        proofs.retain(|_, (_, until)| *until > now);
        proofs.insert(proof.clone(), (chain, now + *self.ttl.lock().unwrap()));
        Some(proof)
    }

    /// The chain a proof stands for, while it lasts.
    pub fn chain(&self, proof: &str) -> Option<Chain> {
        self.proofs.lock().unwrap().get(proof).filter(|(_, until)| *until > Instant::now()).map(|(c, _)| c.clone())
    }

    #[cfg(test)]
    pub fn set_ttl(&self, ttl: Duration) {
        *self.ttl.lock().unwrap() = ttl;
    }
}

/// The longest path a Unix socket takes on this system.
const PATH_MAX: usize = if cfg!(target_os = "macos") { 103 } else { 107 };

/// Listens on `<home>/caller.sock` (only the account's own processes reach it) and answers
/// each connection with a proof for the process that made it, as the kernel names it.
pub fn listen(home: &Path, callers: Arc<Callers>, procs: Arc<dyn Procs>) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let path = home.join(SOCKET);
    let len = path.as_os_str().len();
    if len > PATH_MAX {
        anyhow::bail!("{} is {len} bytes long and a Unix socket's path takes {PATH_MAX} here: workriver cannot tell who calls it. Use a data directory with a shorter path (WORKRIVER_HOME)", path.display());
    }
    let _ = std::fs::remove_file(&path);
    let listener = tokio::net::UnixListener::bind(&path).map_err(|e| anyhow::anyhow!("cannot listen on {}: {e}", path.display()))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            };
            let accepted = now();
            let Some(pid) = stream.peer_cred().ok().and_then(|c| c.pid()) else { continue };
            // Read while the peer waits on the socket. A process that started after the
            // accept is another one that got the id since.
            let chain = procs.chain(None, pid as u32).unwrap_or_default();
            if chain.first().is_none_or(|p| p.start > accepted) {
                continue;
            }
            let Some(proof) = callers.admit(chain) else { continue };
            // One short line into the socket's buffer: written at once, off the async threads' way.
            if let Ok(stream) = stream.into_std()
                && stream.set_nonblocking(false).is_ok()
            {
                use std::io::Write;
                let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
                let _ = (&stream).write_all(format!("{proof}\n").as_bytes());
            }
        }
    });
    Ok(())
}

// `auth::random_hex` reads the system's random bytes.
fn random() -> Option<String> {
    auth::random_hex(16).ok()
}
