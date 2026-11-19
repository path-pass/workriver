// Which process a request comes from, by the kernel's word, and where that process runs.
//
// The CLI connects to a Unix socket of the service; the kernel says which process connected
// (nothing the caller can choose). The service reads that process's chain of parents and
// answers with a short-lived proof the CLI sends along with its HTTP requests. The place a
// caller is at is the one whose root process is in its chain (docs/design/caller-identity.md).
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use workriver_core::runtime::Root;

use crate::common::home::{PINS, SOCKET};
use crate::common::process::{Chain, Proc, Procs, now};
use crate::auth;

/// How long a proof stands for its process.
pub const PROOF: Duration = Duration::from_secs(30);

/// What the service answers a proof it no longer has, in the header the proof came in: the
/// CLI then fetches a new one and repeats its request.
pub const EXPIRED: &str = "expired";

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

/// The CLI's side: a proof for this process from the service whose data directory is `home`;
/// none when there is no service socket there (another machine, an older service).
pub fn fetch(home: &Path) -> Option<String> {
    use std::io::BufRead;
    let stream = std::os::unix::net::UnixStream::connect(home.join(SOCKET)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    let mut line = String::new();
    std::io::BufReader::new(stream).read_line(&mut line).ok()?;
    Some(line.trim().to_string()).filter(|p| !p.is_empty())
}

/// The root a session was first seen at.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Pin {
    /// The machine the root is on; none: this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    pub place: String,
    pub pid: u32,
    pub start: u64,
}

/// One session, one place: each session with the root it was first seen at, kept in a file
/// next to the database so that a restart does not forget.
pub struct Pins {
    path: PathBuf,
    map: Mutex<BTreeMap<String, Pin>>,
}

impl Pins {
    pub fn open(home: &Path) -> Pins {
        let path = home.join(PINS);
        let map = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Pins { path, map: Mutex::new(map) }
    }

    pub fn get(&self, session: &str) -> Option<Pin> {
        self.map.lock().unwrap().get(session).cloned()
    }

    pub fn all(&self) -> Vec<(String, Pin)> {
        self.map.lock().unwrap().iter().map(|(s, p)| (s.clone(), p.clone())).collect()
    }

    /// Sets (`Some`) or releases (`None`) pins, and writes the file when anything changed.
    pub fn change(&self, changes: Vec<(String, Option<Pin>)>) {
        let mut map = self.map.lock().unwrap();
        let before = map.clone();
        for (session, pin) in changes {
            match pin {
                Some(p) => map.insert(session, p),
                None => map.remove(&session),
            };
        }
        if *map != before
            && let Err(e) = serde_json::to_vec_pretty(&*map).map_err(std::io::Error::other).and_then(|b| std::fs::write(&self.path, b))
        {
            tracing::warn!("cannot write {}: {e}", self.path.display());
        }
    }
}

// `auth::random_hex` reads the system's random bytes.
fn random() -> Option<String> {
    auth::random_hex(16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::process::System;

    fn p(pid: u32, parent: u32, tty: Option<&str>) -> Proc {
        Proc { pid, parent, tty: tty.map(str::to_string), start: 7, name: format!("n{pid}") }
    }

    fn root(place: &str, shell: Option<u32>, front: &[u32]) -> Root {
        Root { place: place.into(), shell, front: front.to_vec(), at: Default::default() }
    }

    #[test]
    fn a_process_is_at_the_place_whose_root_it_descends_from() {
        let roots = [root("p1", Some(10), &[]), root("p2", Some(20), &[21]), root("p3", None, &[30, 31])];
        let none = |_: u32| None;
        let at = |c: &[Proc]| place_of(c, &roots, &none).map(|r| r.place.as_str());
        assert_eq!(at(&[p(99, 21, None), p(21, 20, None), p(20, 1, None), p(1, 0, None)]), Some("p2"), "a grandchild of its shell");
        assert_eq!(at(&[p(10, 1, None)]), Some("p1"), "the shell itself");
        assert_eq!(at(&[p(99, 31, None), p(31, 1, None)]), Some("p3"), "no shell known: what is in front is the root");
        assert_eq!(at(&[p(99, 21, None), p(21, 1, None)]), None, "what is in front of a known shell is not a root");
        assert_eq!(at(&[p(99, 5, None), p(5, 1, None)]), None);
        assert_eq!(at(&[]), None);
        // A place inside a place: the nearest root.
        let nested = [root("outer", Some(20), &[]), root("inner", Some(21), &[])];
        assert_eq!(place_of(&[p(99, 21, None), p(21, 20, None), p(20, 1, None)], &nested, &none).map(|r| r.place.as_str()), Some("inner"));
        // A job left behind (`( cmd & )`) keeps its terminal; one that left its session has none.
        let tty = |pid: u32| (pid == 10).then(|| "ttys022".to_string());
        assert_eq!(place_of(&[p(99, 1, Some("ttys022")), p(1, 0, None)], &roots, &tty).map(|r| r.place.as_str()), Some("p1"));
        assert_eq!(place_of(&[p(99, 1, Some("ttys000")), p(1, 0, None)], &roots, &tty).map(|r| r.place.as_str()), None);
        assert_eq!(place_of(&[p(99, 1, None), p(1, 0, None)], &roots, &tty).map(|r| r.place.as_str()), None);
        // Descent comes before the terminal.
        assert_eq!(place_of(&[p(99, 20, Some("ttys022")), p(20, 1, None)], &roots, &tty).map(|r| r.place.as_str()), Some("p2"));
    }

    #[test]
    fn a_proof_stands_for_its_chain_while_it_lasts() {
        let callers = Callers::default();
        let chain = vec![p(3, 2, None)];
        let proof = callers.admit(chain.clone()).unwrap();
        assert_eq!(proof.len(), 32);
        assert_ne!(callers.admit(chain.clone()).unwrap(), proof);
        assert_eq!(callers.chain(&proof), Some(chain.clone()));
        assert_eq!(callers.chain("no-such-proof"), None);
        callers.set_ttl(Duration::from_millis(30));
        let short = callers.admit(chain.clone()).unwrap();
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(callers.chain(&short), None, "expired");
        assert!(callers.chain(&proof).is_some(), "the earlier one still stands");
    }

    /// A real socket: the proof a process is given stands for its own chain, as the kernel
    /// names it. (A real child that connects by itself: tests/caller.rs, with the binary.)
    #[test]
    fn the_kernel_says_which_process_connected() {
        let home = tempfile::tempdir().unwrap();
        let callers = Arc::new(Callers::default());
        let rt = tokio::runtime::Runtime::new().unwrap();
        {
            let _in = rt.enter();
            listen(home.path(), callers.clone(), Arc::new(System)).unwrap();
        }
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(home.path().join(SOCKET)).unwrap().permissions().mode() & 0o777, 0o600);
        // This process.
        let mine = callers.chain(&fetch(home.path()).expect("a proof")).unwrap();
        assert_eq!(mine[0].pid, std::process::id());
        // No service there: no proof.
        assert_eq!(fetch(&home.path().join("nowhere")), None);
        // A path a socket cannot have is said at start.
        let long = home.path().join("x".repeat(120));
        std::fs::create_dir(&long).unwrap();
        let _in = rt.enter();
        let err = listen(&long, callers, Arc::new(System)).unwrap_err().to_string();
        assert!(err.contains("a Unix socket's path takes") && err.contains("WORKRIVER_HOME"), "{err}");
    }

    #[test]
    fn pins_outlive_a_restart() {
        let home = tempfile::tempdir().unwrap();
        let pin = Pin { machine: None, place: "w1:p2".into(), pid: 28023, start: 1795075812053486 };
        let far = Pin { machine: Some("box".into()), place: "w1:p1".into(), pid: 7, start: 9 };
        let pins = Pins::open(home.path());
        assert_eq!(pins.get("SESS-A"), None);
        pins.change(vec![("SESS-A".into(), Some(pin.clone())), ("SESS-B".into(), Some(far.clone())), ("SESS-C".into(), Some(far.clone()))]);
        pins.change(vec![("SESS-C".into(), None)]);
        let again = Pins::open(home.path());
        assert_eq!((again.get("SESS-A"), again.get("SESS-B"), again.get("SESS-C")), (Some(pin), Some(far), None));
        assert_eq!(again.all().len(), 2);
    }
}
