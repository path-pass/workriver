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
use workriver_scan as scan;

use crate::{auth, herdr};

/// One process as its operating system describes it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Proc {
    pub pid: u32,
    pub parent: u32,
    /// Its controlling terminal, as the system names it; none: it has none.
    pub tty: Option<String>,
    /// When it started, in its machine's own measure: equal for one process, different for
    /// another that got the same id later. On this machine it grows with time (`now`).
    pub start: u64,
    pub name: String,
}

/// A process and its parents, itself first, up to the first process of the system.
pub type Chain = Vec<Proc>;

/// No chain is followed further than this (as `metrics::top_of`).
const STEPS: usize = 64;

/// How long a proof stands for its process.
pub const PROOF: Duration = Duration::from_secs(30);

/// The socket's file in the data directory.
pub const SOCKET: &str = "caller.sock";

/// What the service answers a proof it no longer has, in the header the proof came in: the
/// CLI then fetches a new one and repeats its request.
pub const EXPIRED: &str = "expired";

/// The chain of `pid` on this machine, read in-process; empty: there is no such process.
pub fn chain(pid: u32) -> Chain {
    let mut out: Chain = vec![];
    let mut next = pid;
    while next != 0 && out.len() < STEPS && !out.iter().any(|p| p.pid == next) {
        let Some(p) = one(next) else { break };
        next = p.parent;
        out.push(p);
    }
    out
}

/// What `ps` itself reads (`sysctl kern.proc.pid`): it answers for every account's processes.
#[cfg(target_os = "macos")]
fn one(pid: u32) -> Option<Proc> {
    let k = libproc::libproc::proc_pid::kproc_info_raw(pid as i32).ok()?;
    // An id nobody has answers with an empty record.
    if k.kp_proc.p_pid as u32 != pid {
        return None;
    }
    let name: Vec<u8> = k.kp_proc.p_comm.iter().take_while(|c| **c != 0).map(|c| *c as u8).collect();
    Some(Proc {
        pid,
        parent: k.kp_eproc.e_ppid.max(0) as u32,
        tty: (k.kp_eproc.e_tdev != -1).then(|| k.kp_eproc.e_tdev.to_string()),
        // `p_un` holds the start time while the process runs.
        start: (k.kp_proc.p_un.tv_sec.max(0) as u64) * 1_000_000 + k.kp_proc.p_un.tv_usec.max(0) as u64,
        name: String::from_utf8_lossy(&name).into_owned(),
    })
}

#[cfg(target_os = "linux")]
fn one(pid: u32) -> Option<Proc> {
    let s = procfs::process::Process::new(pid as i32).ok()?.stat().ok()?;
    Some(Proc { pid, parent: s.ppid.max(0) as u32, tty: (s.tty_nr != 0).then(|| s.tty_nr.to_string()), start: s.starttime * 1_000_000 / procfs::ticks_per_second(), name: s.comm })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn one(_: u32) -> Option<Proc> {
    None
}

/// Now, in the measure of `Proc::start` on this machine.
#[cfg(target_os = "linux")]
pub fn now() -> u64 {
    use procfs::Current;
    procfs::Uptime::current().map(|u| u.uptime_duration().as_micros() as u64).unwrap_or(u64::MAX)
}

#[cfg(not(target_os = "linux"))]
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_micros() as u64).unwrap_or(u64::MAX)
}

/// The chain of process `$1` on a machine where nothing of ours runs but what the runner
/// starts: one line per step. `/proc/<pid>/stat` where there is `/proc` (a slim system has
/// no `ps`), else `ps`. `parse` reads both.
pub const SNIPPET: &str = concat!(
    "export LC_ALL=C; p=$1; i=0; ",
    "while [ \"$p\" -gt 0 ] 2>/dev/null && [ $i -lt 64 ]; do ",
    "if [ -r /proc/self/stat ]; then l=$(cat /proc/$p/stat 2>/dev/null) || break; [ -n \"$l\" ] || break; echo \"stat $l\"; set -- ${l##*) }; p=$2; ",
    "else l=$(ps -o pid= -o ppid= -o tty= -o lstart= -o comm= -p $p 2>/dev/null); [ -n \"$l\" ] || break; echo \"ps $l\"; set -- $l; p=$2; fi; ",
    "i=$((i+1)); done; true",
);

/// What `SNIPPET` printed, as a chain. A line that does not read ends it.
pub fn parse(text: &str) -> Chain {
    text.lines().map_while(|l| l.strip_prefix("stat ").map(stat).or_else(|| l.strip_prefix("ps ").map(ps)).flatten()).take(STEPS).collect()
}

/// A line of `/proc/<pid>/stat`: `pid (name) state parent … tty_nr … starttime …`; the name
/// may hold anything, so the fields are counted from its closing bracket.
fn stat(line: &str) -> Option<Proc> {
    let (head, rest) = line.rsplit_once(") ")?;
    let (pid, name) = head.split_once(" (")?;
    let f: Vec<&str> = rest.split_whitespace().collect();
    let tty = f.get(4)?.parse::<i64>().ok()?;
    Some(Proc { pid: pid.trim().parse().ok()?, parent: f.get(1)?.parse().ok()?, tty: (tty != 0).then(|| tty.to_string()), start: f.get(19)?.parse().ok()?, name: name.into() })
}

/// A line of `ps -o pid= -o ppid= -o tty= -o lstart= -o comm=`: `pid parent tty Thu Nov 19
/// 10:09:43 2026 /path/to/the program`. The start is whole seconds, kept as its digits.
fn ps(line: &str) -> Option<Proc> {
    let mut rest = line;
    let mut word = || {
        rest = rest.trim_start();
        let (w, after) = rest.split_at(rest.find(char::is_whitespace).unwrap_or(rest.len()));
        rest = after;
        (!w.is_empty()).then_some(w)
    };
    let (pid, parent, tty) = (word()?.parse().ok()?, word()?.parse().ok()?, word()?);
    let (_day, month, date, time, year) = (word()?, word()?, word()?, word()?, word()?);
    let month = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"].iter().position(|m| *m == month)? + 1;
    let start = format!("{year}{month:02}{:02}{}", date.parse::<u8>().ok()?, time.replace(':', "")).parse().ok()?;
    let name = rest.trim();
    let name = name.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or(name);
    Some(Proc { pid, parent, tty: (!tty.starts_with('?') && tty != "-").then(|| tty.to_string()), start, name: name.into() })
}

/// The chain of `pid` on the machine `runner` reaches.
pub fn chain_on(runner: &scan::Runner, pid: u32) -> Result<Chain, String> {
    let out = herdr::output_within(runner.command(&["sh", "-c", SNIPPET, "sh", &pid.to_string()]), Duration::from_secs(10)).map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("the process list was not read ({})", out.status));
    }
    Ok(parse(&String::from_utf8_lossy(&out.stdout)))
}

/// Reads chains: this system's, or what a test gives.
pub trait Procs: Send + Sync {
    /// The chain of `pid` on this machine (`at` none) or on the machine `at` reaches;
    /// empty: no such process. An error: it could not be read.
    fn chain(&self, at: Option<&scan::Runner>, pid: u32) -> Result<Chain, String>;
}

pub struct System;

impl Procs for System {
    fn chain(&self, at: Option<&scan::Runner>, pid: u32) -> Result<Chain, String> {
        match at {
            None => Ok(chain(pid)),
            Some(r) => chain_on(r, pid),
        }
    }
}

/// A chain as a refusal shows it: `sleep(4715) > zsh(90324) > herdr(89391)`, its first steps.
pub fn show(chain: &[Proc]) -> String {
    const SHOWN: usize = 4;
    if chain.is_empty() {
        return "(gone)".into();
    }
    let mut steps: Vec<String> = chain.iter().take(SHOWN).map(|p| format!("{}({})", p.name, p.pid)).collect();
    if chain.len() > SHOWN {
        steps.push("…".into());
    }
    steps.join(" > ")
}

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

/// The pins' file in the data directory.
pub const PINS: &str = "session-roots.json";

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

    fn p(pid: u32, parent: u32, tty: Option<&str>) -> Proc {
        Proc { pid, parent, tty: tty.map(str::to_string), start: 7, name: format!("n{pid}") }
    }

    fn root(place: &str, shell: Option<u32>, front: &[u32]) -> Root {
        Root { place: place.into(), shell, front: front.to_vec(), at: Default::default() }
    }

    /// What `SNIPPET` printed in `docker run --rm debian:stable-slim` for a `sleep` under
    /// two shells (no `ps` there), and on a Mac for a command run under Herdr: the name may
    /// hold spaces and brackets, the Mac's is a whole path.
    const STAT: &str = "stat 9 (sleep) S 8 1 1 34816 1 4194304 96 0 0 0 0 0 0 0 20 0 1 0 51234 2412544 211 18446744073709551615 1 1 0 0 0 0 0 0 0 0 0 0 17 5 0 0 0 0 0\nstat 8 (my (odd) sh) S 1 1 1 34816 1 4194560 317 2104 0 0 0 0 1 0 20 0 1 0 51201 2531328 233 18446744073709551615 1 1 0 0 0 0 0 0 0 0 0 0 17 2 0 0 0 0 0\nstat 1 (sh) S 0 1 1 0 1 4194560 317 2104 0 0 0 0 1 0 20 0 1 0 51200 2531328 233 18446744073709551615 1 1 0 0 0 0 0 0 0 0 0 0 17 2 0 0 0 0 0\n";
    const PS: &str = "ps 27157 27074 ??       Thu Nov 19 10:09:43 2026     /Users/x/target/debug/workriver\nps 27074 83970 ??       Thu Nov 19 10:09:42 2026     /bin/zsh\nps 83970 83912 ttys016  Thu Nov 19 09:40:43 2026     /Users/x/Library/Application Support/claude/2.1.328\nps 83912  6286 ttys016  Thu Nov 19 09:40:43 2026     -zsh\nps     1     0 ??       Fri Nov 13 18:21:17 2026     /sbin/launchd\n";

    #[test]
    fn the_snippets_two_formats_read_as_chains() {
        let c = parse(STAT);
        assert_eq!(c.len(), 3);
        assert_eq!((c[0].pid, c[0].parent, c[0].tty.as_deref(), c[0].start, c[0].name.as_str()), (9, 8, Some("34816"), 51234, "sleep"));
        assert_eq!((c[1].pid, c[1].parent, c[1].name.as_str()), (8, 1, "my (odd) sh"));
        assert_eq!((c[2].pid, c[2].parent, c[2].tty.as_deref()), (1, 0, None), "tty_nr 0: no terminal");
        let c = parse(PS);
        assert_eq!(c.len(), 5);
        assert_eq!((c[0].pid, c[0].parent, c[0].tty.as_deref(), c[0].name.as_str()), (27157, 27074, None, "workriver"));
        assert_eq!((c[2].tty.as_deref(), c[2].name.as_str()), (Some("ttys016"), "2.1.328"), "a path with a space");
        assert_eq!(c[3].name, "-zsh");
        // Two processes started in different seconds differ; the same line reads the same.
        assert_ne!(c[0].start, c[1].start);
        assert_eq!(c[2].start, c[3].start);
        assert_eq!(parse(PS)[0].start, c[0].start);
        // Nothing, or something else, is no chain; a broken line ends it.
        assert!(parse("").is_empty() && parse("ssh: connect refused\n").is_empty());
        assert_eq!(parse(&format!("{PS}ps junk\nps 1 0 ?? Fri Nov 13 18:21:17 2026 x\n")).len(), 5);
    }

    /// The snippet itself, run here on a real child: its chain is this system's own reading.
    #[test]
    fn the_snippet_reads_what_the_library_reads() {
        let mut child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
        let ours = chain(child.id());
        let theirs = chain_on(&scan::Runner::Local, child.id());
        let gone = chain_on(&scan::Runner::Local, 0x3fff_fff0);
        let _ = child.kill();
        let _ = child.wait();
        let theirs = theirs.unwrap();
        assert!(ours.len() >= 2 && ours[0].name == "sleep" && ours[1].pid == std::process::id(), "{ours:?}");
        assert!(ours[0].start <= now() && ours[0].start >= ours[1].start, "{ours:?}");
        let ids = |c: &Chain| c.iter().map(|p| (p.pid, p.parent, p.tty.is_some())).collect::<Vec<_>>();
        assert_eq!(ids(&ours), ids(&theirs));
        assert_eq!(theirs[0].name, "sleep");
        assert!(gone.unwrap().is_empty() && chain(0x3fff_fff0).is_empty(), "a process that is gone");
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
    fn a_chain_shows_its_first_steps() {
        assert_eq!(show(&[p(3, 2, None), p(2, 1, None)]), "n3(3) > n2(2)");
        assert_eq!(show(&(0..9).map(|i| p(9 - i, 8 - i, None)).collect::<Vec<_>>()), "n9(9) > n8(8) > n7(7) > n6(6) > …");
        assert_eq!(show(&[]), "(gone)");
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
