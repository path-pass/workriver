// How the CLI reaches the service: the port (here, or through a followed machine's tunnel),
// the proof of this process, the token, and requests with bounded answers.
use crate::common::home::SOCKET;

use super::*;

/// How the CLI shows the service which process it is: the proof of its process (fetched
/// anew when `fresh`) and its process id. The CLI's own: `from_socket`.
pub(crate) type Prover = std::sync::Arc<dyn Fn(&Daemon, bool) -> (Option<String>, u32) + Send + Sync>;

#[derive(Clone)]
pub(crate) struct Daemon {
    pub(crate) port: u16,
    /// The place the CLI finds in its environment: sent along (as a header and nowhere
    /// else), and only cross-checked. Who this is, the service takes from this process.
    pub(crate) place: Option<String>,
    /// Where the user token is, if this machine has it; read for each request.
    pub(crate) home: Option<PathBuf>,
    pub(crate) prove: Prover,
}

/// The proof of this process from the service's caller socket (the kernel tells the
/// service who connected), fetched once per command, and this process's id. Where there
/// is no such socket (a machine workriver follows: the service is elsewhere) only the id
/// goes along, and the service checks it over there.
pub(super) fn from_socket(d: &Daemon, fresh: bool) -> (Option<String>, u32) {
    static PROOF: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
    let mut kept = PROOF.lock().unwrap();
    if fresh || kept.is_none() {
        *kept = d.home.as_deref().and_then(fetch);
    }
    (kept.clone(), std::process::id())
}

pub(crate) const DEFAULT_PORT: u16 = 47820;

/// The computer the daemon runs on, when this CLI reaches it through a machine's ssh
/// tunnel (its port read from the file the daemon writes over there).
pub(super) static THROUGH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// The daemon's port: as given; else, on a machine workriver follows, its connection's
/// port as the daemon wrote it there; else the default.
pub(super) fn port_of(given: Option<u16>) -> u16 {
    if let Some(p) = given {
        return p;
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let read = |f: &str| home.as_ref().and_then(|h| std::fs::read_to_string(h.join(f)).ok()).map(|t| t.trim().to_string());
    match read(crate::common::home::PORT_FILE).and_then(|t| t.parse().ok()) {
        Some(p) => {
            let _ = THROUGH.set(read(crate::common::home::HOST_FILE).filter(|h| !h.is_empty()).unwrap_or_else(|| "the computer workriver runs on".into()));
            p
        }
        None => DEFAULT_PORT,
    }
}

/// How long the CLI waits to connect (a tunnel that is reconnecting refuses at once, or
/// not at all), and for an answer: it never hangs.
pub(super) const CONNECT: Duration = Duration::from_secs(3);

pub(super) const ANSWER: Duration = Duration::from_secs(30);

impl Daemon {
    /// Why the daemon could not be reached, as whoever runs the CLI can act on it.
    pub(crate) fn unreachable(&self, e: impl std::fmt::Display) -> anyhow::Error {
        match THROUGH.get() {
            Some(host) => anyhow!(
                "cannot reach workriver through the SSH tunnel (port {}, {e}). \
                 The service runs on {host} and starts the tunnel there. \
                 When {host} returns, including after sleep, the tunnel reconnects automatically and the new port is written to ~/{}. \
                 Nothing to start or fix here. Retry the same command every minute or so until it answers. This can take as long as {host} is away.",
                self.port,
                crate::common::home::PORT_FILE
            ),
            None => anyhow!("cannot reach workriver on 127.0.0.1:{} ({e}). Is `workriver serve` running?", self.port),
        }
    }

    /// Why a request got no answer. A connection closed after it was made is not a service
    /// that cannot be reached: the service took it and closed it without answering, which it
    /// does to a request it does not take that is larger than it reads, and when it stops.
    fn failed(&self, e: ureq::Error) -> anyhow::Error {
        use std::io::ErrorKind::*;
        if !matches!(&e, ureq::Error::Io(io) if matches!(io.kind(), BrokenPipe | ConnectionReset | ConnectionAborted)) {
            return self.unreachable(e);
        }
        let most = crate::common::wire::MAX_ATTACHMENT / 1024 / 1024;
        match THROUGH.get() {
            Some(host) => anyhow!(
                "workriver on {host}, through the ssh tunnel (port {}), closed the connection before answering ({e}): it does so to a request it does not take that is larger than {most} MB, the most it reads of one, and when it stops; so does the tunnel when it reconnects",
                self.port
            ),
            None => anyhow!("workriver on 127.0.0.1:{} closed the connection before answering ({e}): it does so to a request it does not take that is larger than {most} MB, the most it reads of one, and when it stops", self.port),
        }
    }

    /// Through a tunnel, first a quick look that the daemon answers at all: the forward of a
    /// connection that died accepts and then says nothing, and a request would wait out
    /// its whole time. Once per command.
    pub(crate) fn probe(&self) -> anyhow::Result<()> {
        static PROBED: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
        if THROUGH.get().is_none() {
            return Ok(());
        }
        let port = self.port;
        let seen = PROBED.get_or_init(|| {
            let agent: ureq::Agent = ureq::Agent::config_builder().timeout_connect(Some(CONNECT)).timeout_global(Some(CONNECT)).build().into();
            agent.get(format!("http://127.0.0.1:{port}/api/status")).call().map(|_| ()).map_err(|e| e.to_string())
        });
        seen.clone().map_err(|e| self.unreachable(e))
    }

    /// The proof of this process and its id, as `prove` gives them.
    pub(crate) fn process(&self, fresh: bool) -> (Option<String>, u32) {
        (self.prove)(self, fresh)
    }

    /// Sends a request (`make` adds the headers it is given) with everything that shows the
    /// service who sends it. The token goes along whenever this machine has it: it makes this
    /// the user only where the service finds this process in no agent's place. A command that
    /// lasts longer than its proof gets a new one and repeats the request, once.
    pub(crate) fn send(&self, make: impl Fn(&[(&str, String)]) -> Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> anyhow::Result<ureq::http::Response<ureq::Body>> {
        let token = self.token().map(|t| format!("Bearer {t}")).unwrap_or_default();
        let mut fresh = false;
        loop {
            let (proof, pid) = self.process(fresh);
            let headers = [("authorization", token.clone()), ("x-workriver-place", self.place.clone().unwrap_or_default()), ("x-workriver-caller", proof.unwrap_or_default()), ("x-workriver-pid", pid.to_string())];
            let res = make(&headers).map_err(|e| self.failed(e))?;
            let expired = res.headers().get("x-workriver-caller").is_some_and(|v| v == crate::common::wire::EXPIRED);
            if !expired || fresh {
                return Ok(res);
            }
            fresh = true;
        }
    }

    pub(crate) fn call(&self, path: &str, body: Option<Value>) -> anyhow::Result<Value> {
        self.probe()?;
        let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).timeout_connect(Some(CONNECT)).timeout_global(Some(ANSWER)).build().into();
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let mut res = self.send(|headers| match &body {
            Some(b) => headers.iter().fold(agent.post(&url), |r, (k, v)| r.header(*k, v)).send_json(b),
            // Reading is as whoever this is, too.
            None => headers.iter().fold(agent.get(&url), |r, (k, v)| r.header(*k, v)).call(),
        })?;
        let ok = res.status().is_success();
        let text = read_answer(&mut res)?;
        if !ok {
            bail!("{text}");
        }
        serde_json::from_str(&text).with_context(|| format!("unexpected answer from 127.0.0.1:{}: {text}", self.port))
    }

    /// A file of the service's store by its name there (`<id>.<ext>`), as whoever this is.
    pub(crate) fn fetch(&self, name: &str) -> anyhow::Result<Vec<u8>> {
        self.probe()?;
        let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).timeout_connect(Some(CONNECT)).timeout_global(Some(Duration::from_secs(120))).build().into();
        let url = format!("http://127.0.0.1:{}/api/attachments/{name}", self.port);
        let mut res = self.send(|headers| headers.iter().fold(agent.get(&url), |r, (k, v)| r.header(*k, v)).call())?;
        if !res.status().is_success() {
            bail!("{}", read_answer(&mut res)?);
        }
        res.body_mut().with_config().limit(256 * 1024 * 1024).read_to_vec().map_err(|e| anyhow!("the service answered, but its answer could not be read: {e}"))
    }

    pub(crate) fn get(&self, path: &str) -> anyhow::Result<Value> {
        self.call(path, None)
    }

    pub(crate) fn token(&self) -> Option<String> {
        self.home.as_deref().and_then(common::home::read_token)
    }

    /// What a request says of who this is: nothing (the service finds it from this
    /// process); or, with `--as` (an agent's name, where the service allows it), that name.
    /// "user" in tests: nothing either.
    pub(crate) fn me(&self, actor: &str) -> Value {
        match actor {
            "" | "user" => json!({}),
            name => json!({ "as": name }),
        }
    }

    /// Puts a file into the attachment store, linked to a task (and a finding) if given.
    pub(crate) fn upload(&self, file: &Path, actor: &str, link: Option<(&str, u32, Option<&str>)>) -> anyhow::Result<Value> {
        let bytes = std::fs::read(file).with_context(|| format!("cannot read {}", file.display()))?;
        if bytes.len() > crate::common::wire::MAX_ATTACHMENT {
            bail!("{} is larger than {} MB, the most workriver stores", file.display(), crate::common::wire::MAX_ATTACHMENT / 1024 / 1024);
        }
        self.probe()?;
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
        let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).timeout_connect(Some(CONNECT)).timeout_global(Some(Duration::from_secs(120))).build().into();
        let me = self.me(actor);
        let mut res = self.send(|headers| {
            let mut req = headers.iter().fold(agent.post(format!("http://127.0.0.1:{}/api/attachments", self.port)), |r, (k, v)| r.header(*k, v)).query("name", &name).header("content-type", common::files::mime(&name, None));
            if let Some(v) = me["as"].as_str() {
                req = req.query("as", v);
            }
            if let Some((unit, task, finding)) = link {
                req = req.query("unit", unit).query("task", task.to_string());
                if let Some(f) = finding {
                    req = req.query("finding", f);
                }
            }
            req.send(&bytes[..])
        })?;
        let ok = res.status().is_success();
        let text = read_answer(&mut res)?;
        if !ok {
            bail!("{text}");
        }
        Ok(serde_json::from_str(&text)?)
    }

    /// A task's staging folder for screenshots, made if needed.
    pub(crate) fn inbox(&self, unit: &str, task: u32) -> anyhow::Result<PathBuf> {
        let v = self.call("/api/attachments/inbox", Some(json!({ "unit": unit, "task": task })))?;
        // The service is on another machine: the folder is on this one's disk, made here.
        if v["elsewhere"] == true {
            let d = crate::common::home::inbox(&common::home::data_dir(), unit, task);
            crate::common::home::mkdir(&d).with_context(|| format!("cannot make {}", d.display()))?;
            return Ok(d);
        }
        Ok(PathBuf::from(v["dir"].as_str().unwrap_or_default()))
    }

    pub(crate) fn append(&self, actor: &str, kind: &str, body: Value) -> anyhow::Result<Value> {
        let mut req = self.me(actor);
        req["kind"] = json!(kind);
        req["body"] = body;
        self.call("/api/append", Some(req))
    }
}

/// The service's answer as text. A reply that cannot be read is said as that, not as the
/// service being unreachable: it answered.
// ponytail: whole answer in memory, capped at 256 MB; stream it if snapshots ever get there.
pub(super) fn read_answer(res: &mut ureq::http::Response<ureq::Body>) -> anyhow::Result<String> {
    res.body_mut().with_config().limit(256 * 1024 * 1024).read_to_string().map_err(|e| anyhow!("the service answered, but its answer could not be read: {e}"))
}

/// The CLI's side: a proof for this process from the service whose data directory is `home`;
/// none when there is no service socket there (another machine, an older service).
pub(crate) fn fetch(home: &Path) -> Option<String> {
    proof_from(std::os::unix::net::UnixStream::connect(home.join(SOCKET)).ok()?)
}

/// The proof the service writes into a connection to its caller socket, as one line.
pub(super) fn proof_from(stream: std::os::unix::net::UnixStream) -> Option<String> {
    use std::io::BufRead;
    // The service answers at once and closes. On macOS a socket whose other end has closed
    // refuses a timeout (EINVAL): its answer is then all there already, so read it without
    // waiting. Either way the read never hangs.
    if stream.set_read_timeout(Some(Duration::from_secs(3))).is_err() {
        stream.set_nonblocking(true).ok()?;
    }
    let mut line = String::new();
    std::io::BufReader::new(stream).read_line(&mut line).ok()?;
    Some(line.trim().to_string()).filter(|p| !p.is_empty())
}
