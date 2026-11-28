// The service's side of the hosts: spawning one, its root process, one kept connection to
// each, its published agent, and reading a journal no host writes now. The host protocol is
// docs/design/hosted-agent.md's; this side only asks.
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, ensure};
use serde_json::{Value, json};

use crate::common::process;

/// The host protocol this service speaks; a host that speaks another is not attached.
pub const PROTOCOL: u64 = 1;

/// What a published hosted agent is, as the records have it.
#[derive(Clone, Debug, PartialEq)]
pub struct Published {
    pub session: String,
    pub name: String,
    pub harness: String,
    pub cwd: String,
    pub generation: u64,
}

/// A host's answer that is not a result: it refused (its words: `busy`, `gone`, `settled`,
/// ...), or it could not be asked.
#[derive(Debug)]
pub enum HostError {
    Refused(String),
    Unreachable(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::Refused(why) => write!(f, "the host refused: {why}"),
            HostError::Unreachable(why) => write!(f, "the host cannot be reached: {why}"),
        }
    }
}

/// One host the service knows: its root process (as it spawned it, or as the records keep
/// it), the agent it holds once published, and the one connection to it.
struct Known {
    pid: u32,
    start: u64,
    agent: Option<Published>,
    conn: Arc<Mutex<Option<Conn>>>,
}

struct Conn {
    stream: BufReader<UnixStream>,
    next: u64,
}

/// The hosts of this machine's service.
pub struct Hosts {
    dir: PathBuf,
    known: Mutex<BTreeMap<String, Known>>,
    /// The hosts this run of the service spawned, reaped when they exit.
    children: Mutex<Vec<Child>>,
}

impl Hosts {
    pub fn new(home: &Path) -> Arc<Hosts> {
        Arc::new(Hosts { dir: home.join("agents"), known: Mutex::default(), children: Mutex::default() })
    }

    /// An instance's directory.
    pub fn dir(&self, id: &str) -> PathBuf {
        self.dir.join(id)
    }

    /// The instance directories on disk, by id.
    pub fn on_disk(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(&self.dir) else { return vec![] };
        entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| n.starts_with('a')).collect()
    }

    /// Makes the instance's directory with its key and its starting record, and spawns its
    /// host in a session of its own with no terminal; the host's process id and start, as the
    /// service spawned it (the root of the agent's place, never read from what the host writes).
    pub fn spawn(&self, id: &str, harness: &str, cwd: &str, model: &str, auth: &str) -> anyhow::Result<(u32, u64)> {
        fs::create_dir_all(&self.dir)?;
        fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700))?;
        let dir = self.dir(id);
        fs::create_dir(&dir).with_context(|| format!("cannot make {}", dir.display()))?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        let mut key = [0u8; 32];
        std::io::Read::read_exact(&mut fs::File::open("/dev/urandom")?, &mut key)?;
        OpenOptions::new().write(true).create_new(true).mode(0o600).open(dir.join("host.key"))?.write_all(&key)?;
        let model = (!model.is_empty()).then_some(model);
        let instance = json!({"id": id, "harness": harness, "cwd": cwd, "model": model, "auth": auth, "generation": 1, "state": "starting"});
        OpenOptions::new().write(true).create_new(true).mode(0o600).open(dir.join("instance.json"))?.write_all(instance.to_string().as_bytes())?;
        let log = OpenOptions::new().create(true).append(true).mode(0o600).open(dir.join("host.log"))?;
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("host").arg(&dir).current_dir(&dir).stdin(Stdio::null()).stdout(log.try_clone()?).stderr(log);
        // SAFETY: setsid is async-signal-safe and touches no memory of the parent.
        unsafe {
            command.pre_exec(|| if libc::setsid() == -1 { Err(std::io::Error::last_os_error()) } else { Ok(()) });
        }
        let child = command.spawn().context("cannot start the host")?;
        let pid = child.id();
        let start = process::one(pid).context("the host is gone at once")?.start;
        self.children.lock().unwrap().push(child);
        self.track(id, pid, start);
        Ok((pid, start))
    }

    /// Knows a host by its root process: one this run spawned, or one the records keep.
    pub fn track(&self, id: &str, pid: u32, start: u64) {
        let mut known = self.known.lock().unwrap();
        match known.get_mut(id) {
            Some(k) if (k.pid, k.start) == (pid, start) => {}
            _ => {
                known.insert(id.into(), Known { pid, start, agent: None, conn: Arc::default() });
            }
        }
    }

    /// Its agent is the records' from now on: its root is an agent's place.
    pub fn publish(&self, id: &str, agent: Published) {
        if let Some(k) = self.known.lock().unwrap().get_mut(id) {
            k.agent = Some(agent);
        }
    }

    /// Not known any more (closed, or its start ended).
    pub fn forget(&self, id: &str) {
        self.known.lock().unwrap().remove(id);
    }

    /// Whether no host is known.
    pub fn none(&self) -> bool {
        self.known.lock().unwrap().is_empty()
    }

    /// Whether its root process runs, as it was spawned (the same id and start).
    pub fn alive(&self, id: &str) -> bool {
        let Some((pid, start)) = self.known.lock().unwrap().get(id).map(|k| (k.pid, k.start)) else { return false };
        runs(pid, start)
    }

    /// The root process of a known host.
    pub fn root(&self, id: &str) -> Option<(u32, u64)> {
        self.known.lock().unwrap().get(id).map(|k| (k.pid, k.start))
    }

    /// Reaps the hosts this run spawned that exited.
    pub fn reap(&self) {
        self.children.lock().unwrap().retain_mut(|c| matches!(c.try_wait(), Ok(None)));
    }

    /// Each known host: its id, its root, and its agent once published.
    pub(super) fn all(&self) -> Vec<(String, u32, u64, Option<Published>)> {
        self.known.lock().unwrap().iter().map(|(id, k)| (id.clone(), k.pid, k.start, k.agent.clone())).collect()
    }

    /// One request to a host, on the one connection kept to it (made, with `hello`, when
    /// there is none). A connection that fails is dropped; the next request makes another.
    pub fn call(&self, id: &str, method: &str, params: Value, wait: Duration) -> Result<Value, HostError> {
        let conn = self.known.lock().unwrap().get(id).map(|k| k.conn.clone()).ok_or_else(|| HostError::Unreachable(format!("{id} is not known")))?;
        let mut conn = conn.lock().unwrap();
        if conn.is_none() {
            *conn = Some(self.connect(id)?);
        }
        let answer = ask(conn.as_mut().unwrap(), method, params, wait);
        if matches!(answer, Err(HostError::Unreachable(_))) {
            *conn = None;
        }
        answer
    }

    /// The host's own account of itself (`hello` again, with the proof).
    pub fn hello(&self, id: &str) -> Result<Value, HostError> {
        let params = json!({"proof": self.proof(id)?, "protocolVersion": PROTOCOL});
        self.call(id, "hello", params, Duration::from_secs(5))
    }

    /// The proof of the instance's key, as `hello` takes it.
    fn proof(&self, id: &str) -> Result<String, HostError> {
        let path = self.dir(id).join("host.key");
        let key = fs::read(&path).map_err(|e| HostError::Unreachable(format!("{}: {e}", path.display())))?;
        Ok(key.iter().map(|b| format!("{b:02x}")).collect())
    }

    fn connect(&self, id: &str) -> Result<Conn, HostError> {
        let dir = self.dir(id);
        let down = |e: std::io::Error| HostError::Unreachable(format!("{}: {e}", dir.display()));
        let proof = self.proof(id)?;
        let stream = UnixStream::connect(dir.join("host.sock")).map_err(down)?;
        let mut conn = Conn { stream: BufReader::new(stream), next: 0 };
        let hello = ask(&mut conn, "hello", json!({"proof": proof, "protocolVersion": PROTOCOL}), Duration::from_secs(5))?;
        if hello["protocolVersion"] != PROTOCOL {
            return Err(HostError::Refused("incompatible".into()));
        }
        Ok(conn)
    }

    /// Where an instance's journal is.
    pub fn journal(&self, id: &str) -> PathBuf {
        self.dir(id).join("journal.jsonl")
    }
}

fn ask(conn: &mut Conn, method: &str, params: Value, wait: Duration) -> Result<Value, HostError> {
    let down = |e: std::io::Error| HostError::Unreachable(format!("{method}: {e}"));
    conn.next += 1;
    let id = conn.next;
    let stream = conn.stream.get_mut();
    stream.set_read_timeout(Some(wait)).map_err(down)?;
    stream.set_write_timeout(Some(Duration::from_secs(5))).map_err(down)?;
    let mut line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
    line.push('\n');
    stream.write_all(line.as_bytes()).map_err(down)?;
    loop {
        let mut answer = String::new();
        if conn.stream.read_line(&mut answer).map_err(down)? == 0 {
            return Err(HostError::Unreachable(format!("{method}: the host closed the connection")));
        }
        let v: Value = serde_json::from_str(&answer).map_err(|e| HostError::Unreachable(format!("{method}: unreadable answer: {e}")))?;
        // A late answer to an earlier request that timed out is not this one's.
        if v["id"] != id {
            continue;
        }
        return match v.get("error") {
            Some(e) => Err(HostError::Refused(e["message"].as_str().unwrap_or("?").into())),
            None => Ok(v["result"].clone()),
        };
    }
}

/// Whether process `pid` runs and started at `start`.
pub(super) fn runs(pid: u32, start: u64) -> bool {
    process::one(pid).is_some_and(|p| p.start == start)
}

/// Each line of a journal file that no host writes now, in order, as its seq and bytes, until
/// `each` says stop: what the service reads of a stopped or closed instance. Opens the file
/// for reading only; a last line without its end is an append the host never finished.
pub fn journal_lines(path: &Path, mut each: impl FnMut(u64, &[u8]) -> anyhow::Result<bool>) -> anyhow::Result<()> {
    let file = match OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    let mut seq = 0;
    while reader.read_until(b'\n', &mut line)? != 0 {
        if !line.ends_with(b"\n") {
            break;
        }
        seq += 1;
        line.pop();
        if !each(seq, &line)? {
            break;
        }
        line.clear();
    }
    Ok(())
}

/// A page of a journal no host writes now, as the host's `read` answers: `{head, entries}`,
/// the entries after `after`, at most `limit` and about `budget` bytes.
pub fn journal_page(path: &Path, after: u64, limit: usize, budget: usize) -> anyhow::Result<Value> {
    let (mut entries, mut bytes, mut head) = (Vec::new(), 0, 0);
    journal_lines(path, |seq, line| {
        head = seq;
        if seq > after && entries.len() < limit && (entries.is_empty() || bytes + line.len() <= budget) {
            bytes += line.len();
            let entry: Value = serde_json::from_slice(line).context("cannot read a journal line")?;
            ensure!(entry["seq"] == seq, "unexpected sequence at journal line {seq}");
            entries.push(entry);
        }
        Ok(true)
    })?;
    Ok(json!({"head": head, "entries": entries}))
}
