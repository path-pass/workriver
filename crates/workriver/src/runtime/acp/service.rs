// The service's side of the hosts: spawning one, its root process, one kept connection to
// each, its published agent, and reading a journal no host writes now. The host protocol is
// docs/design/hosted-agent.md's; this side only asks.
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;
use serde::Deserialize;
use serde_json::{Value, json};
use workriver_core::{Shell, hosted::Answer};

use crate::common::process;

use super::journal::{JournalPage, JournalReader, JournalReceipts, page_limit};
use super::wire::{HOST_FRAME_BYTES, HOST_PROTOCOL};

#[derive(Debug)]
pub struct HostHello {
    pub state: String,
    pub generation: u64,
    pub session: Option<String>,
    pub model: String,
    pub version: String,
    pub account: String,
    pub plan: String,
    pub approvals: String,
}

#[derive(Deserialize)]
pub struct HostStatus {
    pub state: String,
    pub generation: u64,
    pub head: u64,
    pub pending: Value,
}

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
    /// The session the host reported before its start is recorded: no other place's.
    pending: Option<String>,
    conn: Arc<Mutex<Option<Conn>>>,
}

struct Conn {
    stream: BufReader<UnixStream>,
    next: u64,
    epoch: u64,
}

/// The hosts of this machine's service.
pub struct Hosts {
    dir: PathBuf,
    known: Mutex<BTreeMap<String, Known>>,
    connection_count: AtomicU64,
    journals: Mutex<BTreeMap<String, Arc<Mutex<JournalReader>>>>,
    /// The hosts this run of the service spawned, reaped when they exit.
    children: Mutex<Vec<Child>>,
}

impl Hosts {
    pub fn new(home: &Path) -> Arc<Hosts> {
        Arc::new(Hosts {
            dir: home.join("agents"), known: Mutex::default(),
            connection_count: AtomicU64::default(), journals: Mutex::default(),
            children: Mutex::default(),
        })
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
                known.insert(id.into(), Known { pid, start, agent: None, pending: None, conn: Arc::default() });
            }
        }
    }

    /// Its agent is the records' from now on: its root is an agent's place.
    pub fn publish(&self, id: &str, agent: Published) {
        if let Some(k) = self.known.lock().unwrap().get_mut(id) {
            k.agent = Some(agent);
            k.pending = None;
        }
    }

    /// The session a starting host reported, before its start is recorded: from now on no
    /// other place that shows it is that agent (nor anyone).
    pub fn pending(&self, id: &str, session: &str) {
        if let Some(k) = self.known.lock().unwrap().get_mut(id) {
            k.pending = Some(session.into());
        }
    }

    /// The instance whose starting host reported `session`, if any.
    pub fn pending_for(&self, session: &str) -> Option<String> {
        self.known.lock().unwrap().iter().find(|(_, k)| k.pending.as_deref() == Some(session)).map(|(id, _)| id.clone())
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

    /// Reaps the hosts this run spawned that exited.
    pub fn reap(&self) {
        self.children.lock().unwrap().retain_mut(|c| matches!(c.try_wait(), Ok(None)));
    }

    /// Each known host: its id, its root, and its agent once published.
    pub(super) fn all(&self) -> Vec<(String, u32, u64, Option<Published>)> {
        self.known.lock().unwrap().iter().map(|(id, k)| (id.clone(), k.pid, k.start, k.agent.clone())).collect()
    }

    /// One request on the kept connection, with its epoch. If it fails on the wire, the
    /// connection is dropped; the next handshake gets a new epoch.
    fn call_on(&self, id: &str, method: &str, params: Value, wait: Duration) -> Result<(u64, Value), HostError> {
        let conn = self.known.lock().unwrap().get(id).map(|k| k.conn.clone()).ok_or_else(|| HostError::Unreachable(format!("{id} is not known")))?;
        let mut conn = conn.lock().unwrap();
        let until = Instant::now() + wait;
        if conn.is_none() {
            let (connected, hello) = self.connect(id, until)?;
            let epoch = connected.epoch;
            *conn = Some(connected);
            if method == "hello" {
                return Ok((epoch, hello));
            }
        }
        let connected = conn.as_mut().unwrap();
        let answer = ask(connected, method, params, until).map(|reply| (connected.epoch, reply));
        if matches!(answer, Err(HostError::Unreachable(_))) {
            *conn = None;
        }
        answer
    }

    fn call(&self, id: &str, method: &str, params: Value, wait: Duration) -> Result<Value, HostError> {
        self.call_on(id, method, params, wait).map(|(_, reply)| reply)
    }

    pub fn connection_epoch(&self, id: &str) -> Option<u64> {
        let conn = self.known.lock().unwrap().get(id)?.conn.clone();
        let epoch = conn.lock().unwrap().as_ref().map(|conn| conn.epoch);
        epoch
    }

    /// The host's own account of itself (`hello` again, with the proof).
    pub fn hello(&self, id: &str) -> Result<(u64, HostHello), HostError> {
        self.hello_for(id, Duration::from_secs(5))
    }

    pub fn hello_for(&self, id: &str, wait: Duration) -> Result<(u64, HostHello), HostError> {
        let (epoch, reply) = self.call_on(id, "hello", self.hello_params(id)?, wait)?;
        Ok((epoch, hello(reply)?))
    }

    pub fn status(&self, id: &str) -> Result<HostStatus, HostError> {
        self.status_for(id, Duration::from_secs(5))
    }

    pub(super) fn status_for(&self, id: &str, wait: Duration) -> Result<HostStatus, HostError> {
        parsed("status", self.call(id, "status", json!({}), wait)?)
    }

    pub(super) fn prompt(&self, id: &str, generation: u64, session: &str, key: &str, text: &str) -> Result<(), HostError> {
        let params = json!({"generation": generation, "session": session, "key": key, "text": text});
        let reply = self.call(id, "prompt", params, Duration::from_secs(5))?;
        reply["seq"].as_u64().map(|_| ()).ok_or_else(|| bad_reply("prompt"))
    }

    pub fn read(&self, id: &str, after: u64, limit: Option<usize>) -> Result<JournalPage, HostError> {
        let params = json!({"after": after, "limit": page_limit(limit)});
        parsed("read", self.call(id, "read", params, Duration::from_secs(10))?)
    }

    pub fn cancel(&self, id: &str, generation: u64) -> Result<String, HostError> {
        let reply = self.call(id, "cancel", json!({"generation": generation}), Duration::from_secs(15))?;
        reply["state"].as_str().map(str::to_owned).ok_or_else(|| bad_reply("cancel"))
    }

    pub fn close(&self, id: &str, generation: u64) -> Result<(), HostError> {
        accepted(self.call(id, "close", json!({"generation": generation}), Duration::from_secs(5))?)
    }

    pub fn answer(&self, id: &str, generation: u64, request: u64, decision: Answer) -> Result<bool, HostError> {
        let option = match decision { Answer::Allow => "allow", Answer::Reject => "reject" };
        let params = json!({"generation": generation, "request": request, "optionId": option});
        let reply = self.call(id, "answer", params, Duration::from_secs(5))?;
        reply["replied"].as_bool().ok_or_else(|| bad_reply("answer"))
    }

    /// The on-disk host identity may be used only to find a lost start for cleanup.
    pub fn instance_host(&self, id: &str) -> Option<(Shell, u64)> {
        #[derive(Deserialize)]
        struct RecoveryInstance {
            id: String,
            host: Shell,
            generation: Option<u64>,
        }
        let record: RecoveryInstance = serde_json::from_slice(&fs::read(self.dir(id).join("instance.json")).ok()?).ok()?;
        (record.id == id && runs(record.host.pid, record.host.start)).then(|| (record.host, record.generation.unwrap_or(1)))
    }

    /// Authenticate transiently without registering a root from the on-disk instance.
    pub fn orphan_hello(&self, id: &str, wait: Duration) -> Result<HostHello, HostError> {
        let (_, result) = self.connect(id, Instant::now() + wait)?;
        hello(result)
    }

    pub fn orphan_close(&self, id: &str, generation: u64) -> Result<(), HostError> {
        let until = Instant::now() + Duration::from_secs(5);
        let (mut conn, _) = self.connect(id, until)?;
        accepted(ask(&mut conn, "close", json!({"generation": generation}), until)?)
    }

    fn hello_params(&self, id: &str) -> Result<Value, HostError> {
        Ok(json!({"proof": self.proof(id)?, "protocolVersion": HOST_PROTOCOL}))
    }

    /// The proof of the instance's key, as `hello` takes it.
    fn proof(&self, id: &str) -> Result<String, HostError> {
        let path = self.dir(id).join("host.key");
        let key = fs::read(&path).map_err(|e| HostError::Unreachable(format!("{}: {e}", path.display())))?;
        Ok(key.iter().map(|b| format!("{b:02x}")).collect())
    }

    fn connect(&self, id: &str, until: Instant) -> Result<(Conn, Value), HostError> {
        let dir = self.dir(id);
        let down = |e: std::io::Error| HostError::Unreachable(format!("{}: {e}", dir.display()));
        let params = self.hello_params(id)?;
        let stream = UnixStream::connect(dir.join("host.sock")).map_err(down)?;
        let mut conn = Conn { stream: BufReader::new(stream), next: 0, epoch: 0 };
        let hello = ask(&mut conn, "hello", params, until.min(Instant::now() + Duration::from_secs(5)))?;
        if hello["protocolVersion"] != HOST_PROTOCOL {
            return Err(HostError::Refused("incompatible".into()));
        }
        conn.epoch = self.connection_count.fetch_add(1, Ordering::Relaxed) + 1;
        Ok((conn, hello))
    }

    /// The tag an instance's processes carry in their marker (`a7/1/<tag>`), from its key and
    /// its directory as the host was given it; none when its key cannot be read.
    pub fn tag(&self, id: &str) -> Option<String> {
        let key = fs::read(self.dir(id).join("host.key")).ok()?;
        Some(super::host::marker_tag(&key, &self.dir(id)))
    }

    /// Where an instance's journal is.
    fn journal(&self, id: &str) -> PathBuf {
        self.dir(id).join("journal.jsonl")
    }

    fn journal_reader(&self, id: &str) -> Arc<Mutex<JournalReader>> {
        self.journals.lock().unwrap().entry(id.into()).or_default().clone()
    }

    pub fn read_journal(&self, id: &str, after: u64, limit: Option<usize>) -> anyhow::Result<JournalPage> {
        self.journal_reader(id).lock().unwrap().page(&self.journal(id), after, limit)
    }

    pub fn journal_receipts(&self, id: &str) -> anyhow::Result<JournalReceipts> {
        self.journal_reader(id).lock().unwrap().receipts(&self.journal(id))
    }
}

fn bad_reply(method: &str) -> HostError {
    HostError::Unreachable(format!("{method}: invalid host reply"))
}

fn parsed<T: serde::de::DeserializeOwned>(method: &str, value: Value) -> Result<T, HostError> {
    serde_json::from_value(value).map_err(|_| bad_reply(method))
}

fn hello(value: Value) -> Result<HostHello, HostError> {
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let state = value["state"].as_str().ok_or_else(|| bad_reply("hello"))?.to_owned();
    let generation = value["generation"].as_u64().filter(|g| *g != 0).ok_or_else(|| bad_reply("hello"))?;
    Ok(HostHello {
        state, generation,
        session: value["session"].as_str().map(str::to_owned),
        model: text(&value["model"]),
        version: text(&value["versions"]["harnessVersion"]),
        account: text(&value["billing"]["account"]),
        plan: text(&value["billing"]["plan"]),
        approvals: text(&value["approvals"]["approvalPolicy"]),
    })
}

fn accepted(reply: Value) -> Result<(), HostError> {
    if reply["accepted"] == true { Ok(()) } else { Err(bad_reply("close")) }
}

fn ask(conn: &mut Conn, method: &str, params: Value, until: Instant) -> Result<Value, HostError> {
    let remaining = || {
        let duration = until.saturating_duration_since(Instant::now());
        if duration.is_zero() { Err(io::Error::from(io::ErrorKind::TimedOut)) } else { Ok(duration) }
    };
    let down = |e: std::io::Error| HostError::Unreachable(format!("{method}: {e}"));
    conn.next += 1;
    let id = conn.next;
    let stream = conn.stream.get_mut();
    stream.set_write_timeout(Some(remaining().map_err(down)?.min(Duration::from_secs(5)))).map_err(down)?;
    let mut line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
    line.push('\n');
    stream.write_all(line.as_bytes()).map_err(down)?;
    loop {
        conn.stream.get_ref().set_read_timeout(Some(remaining().map_err(down)?)).map_err(down)?;
        let mut answer = String::new();
        if (&mut conn.stream).take((HOST_FRAME_BYTES + 1) as u64).read_line(&mut answer).map_err(down)? == 0 {
            return Err(HostError::Unreachable(format!("{method}: the host closed the connection")));
        }
        if !answer.ends_with('\n') {
            return Err(bad_reply(method));
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

#[cfg(test)]
mod tests;
