mod close;
mod instance;
mod wire;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, ensure};
use serde_json::{Value, json};

use super::codex::{Adapter, Event};
use super::journal::Journal;
use close::{Closing, Step, outcome};
use instance::{Directory, Socket};
use wire::{Lines, error, notification, reply, request};

const PROTOCOL_VERSION: u64 = 1;
const APPROVAL_BYTES: usize = 24 * 1024;
const STATUS_BYTES: usize = 512 * 1024;

pub(crate) fn run(path: &Path) -> anyhow::Result<()> {
    let (directory, instance) = Directory::open(path)?;
    // The service normally creates this session before exec. The internal command can
    // also be run directly by a caller that is not already a session or group leader.
    // SAFETY: getsid/setsid have no pointer arguments.
    unsafe {
        if libc::getsid(0) != libc::getpid() && libc::setsid() == -1 {
            return Err(io::Error::last_os_error()).context("cannot create the host session");
        }
    }
    let mut record = instance.record()?;
    let journal = Journal::open(path, instance.generation)?;
    let socket = directory.bind()?;
    directory.save(&record)?;
    let version = crate::common::process::output_within({
        let mut command = instance.codex();
        command.arg("--version");
        command
    }, Duration::from_secs(5))?;
    ensure!(version.status.success(), "cannot read the Codex version");
    let version = String::from_utf8(version.stdout)?;
    let mut adapter = Adapter::new(&instance.cwd, instance.model.as_deref(), &instance.auth, version.trim())?;
    let initialize = adapter.start();
    let mut child = instance.codex().args(["app-server", "--listen", "stdio://"])
        .process_group(0).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let input = child.stdin.take().expect("piped stdin");
    let output = child.stdout.take().expect("piped stdout");
    record["agentGroup"] = json!(child.id());
    let mut host = Host {
        directory, socket, record, journal, adapter, child, input: Some(input), output: Some(Lines::new(output)),
        generation: instance.generation, client: None, challenger: None, client_epoch: 0,
        pending: BTreeMap::new(), billing: json!({"verified":false}), working: false,
        cancelling: None, closing: None, reaped: false,
        startup_until: Instant::now() + Duration::from_secs(60),
    };

    let result = host.serve(&initialize);
    if let Err(failure) = &result {
        host.stop_after_failure(failure);
    }
    result
}

struct Client {
    stream: Lines<UnixStream>,
    expires: Instant,
}

struct Cancel {
    id: Value,
    epoch: u64,
    until: Instant,
}

struct Pending {
    id: Value,
    native: Value,
    view: Value,
    bytes: usize,
}

struct Host {
    // Socket removal must happen while this host still holds the directory lock.
    socket: Socket,
    directory: Directory,
    record: Value,
    journal: Journal,
    adapter: Adapter,
    child: Child,
    input: Option<ChildStdin>,
    output: Option<Lines<ChildStdout>>,
    generation: u64,
    client: Option<Client>,
    challenger: Option<Client>,
    client_epoch: u64,
    pending: BTreeMap<u64, Pending>,
    billing: Value,
    working: bool,
    cancelling: Option<Cancel>,
    closing: Option<Closing>,
    reaped: bool,
    startup_until: Instant,
}

impl Host {
    fn serve(&mut self, initialize: &Value) -> anyhow::Result<()> {
        wire::nonblocking(self.input.as_ref().unwrap().as_raw_fd())?;
        wire::nonblocking(self.output.as_ref().unwrap().io.as_raw_fd())?;
        self.directory.save(&self.record)?;
        self.acp("out", request(1, "initialize", json!({
            "protocolVersion":1,
            "clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false,"session":{"compaction":{}}},
            "clientInfo":{"name":"workriver","version":env!("CARGO_PKG_VERSION")}
        })))?;
        self.send(initialize)?;

        loop {
            let mut activity = self.accept()?;
            activity |= self.authenticate();
            activity |= self.agent_message()?;
            self.finish_cancel(None);
            if self.progress_close()? {
                return Ok(());
            }
            activity |= self.command()?;
            if self.closing.as_mut().is_some_and(|close| close.requested && !std::mem::replace(&mut close.cancelled, true)) {
                self.cancel_turn("close")?;
            }
            if !activity {
                self.wait()?;
            }
        }
    }

    fn accept(&mut self) -> anyhow::Result<bool> {
        match self.socket.listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(true)?;
                self.challenger = Some(Client { stream: Lines::new(stream), expires: Instant::now() + Duration::from_secs(2) });
                Ok(true)
            }
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    fn authenticate(&mut self) -> bool {
        let Some(mut candidate) = self.challenger.take() else { return false };
        let frame = match candidate.stream.next(4096) {
            Ok(Some(frame)) => frame,
            Ok(None) if Instant::now() < candidate.expires => {
                self.challenger = Some(candidate);
                return false;
            }
            _ => return false,
        };
        let id = frame["id"].clone();
        let params = &frame["params"];
        let proof = params["proof"].as_str().unwrap_or_default().as_bytes();
        let accepted = valid_request(&frame) && frame["method"] == "hello" && proves(proof, &self.directory.proof);
        let result = if !accepted {
            error(id, -32000, "unauthorized")
        } else if params["protocolVersion"] != PROTOCOL_VERSION {
            error(id, -32000, "incompatible")
        } else {
            reply(id, self.hello())
        };
        let sent = wire::write(&mut candidate.stream.io, &result, self.client_write_until()).is_ok();
        if accepted && params["protocolVersion"] == PROTOCOL_VERSION && sent {
            self.client_epoch += 1;
            self.client = Some(candidate);
        }
        true
    }

    fn command(&mut self) -> anyhow::Result<bool> {
        let Some(client) = &mut self.client else { return Ok(false) };
        let frame = match client.stream.next(2 * 1024 * 1024) {
            Ok(Some(frame)) => frame,
            Ok(None) => return Ok(false),
            Err(_) => {
                self.client = None;
                return Ok(true);
            }
        };
        let id = frame["id"].clone();
        if !valid_request(&frame) {
            self.respond(error(id, -32600, "invalid request"));
            return Ok(true);
        }
        let params = &frame["params"];
        let result = match frame["method"].as_str().unwrap() {
            "status" => Ok(self.status()),
            "hello" if !proves(params["proof"].as_str().unwrap_or_default().as_bytes(), &self.directory.proof) => Err("unauthorized"),
            "hello" if params["protocolVersion"] != PROTOCOL_VERSION => Err("incompatible"),
            "hello" => Ok(self.hello()),
            "read" => {
                let after = params["after"].as_u64().unwrap_or_default();
                let limit = params["limit"].as_u64().unwrap_or(200).clamp(1, 1000) as usize;
                self.journal.read(after, limit).map_err(|_| "cannot read the journal")
            }
            "prompt" => self.prompt(params)?,
            "answer" => self.answer(params)?,
            "cancel" if params["generation"] == self.generation && self.closing.is_none() && !self.record["session"].is_null() => {
                if self.cancelling.is_some() {
                    Err("busy")
                } else {
                    self.cancelling = Some(Cancel { id, epoch: self.client_epoch, until: Instant::now() + Duration::from_secs(10) });
                    self.cancel_turn("cancel")?;
                    if !self.working {
                        self.finish_cancel(Some("idle"));
                    }
                    return Ok(true);
                }
            }
            "close" if params["generation"] == self.generation => {
                if self.closing.is_none() {
                    self.closing = Some(Closing::new(Instant::now(), self.working, true));
                }
                if self.closing.as_ref().unwrap().requested { Ok(json!({"accepted":true})) } else { Err("gone") }
            }
            "cancel" | "close" => Err("gone"),
            _ => {
                self.respond(error(id, -32601, "method not found"));
                return Ok(true);
            }
        };
        self.respond(match result {
            Ok(value) => reply(id, value),
            Err(reason) => error(id, -32000, reason),
        });
        Ok(true)
    }

    fn respond(&mut self, frame: Value) {
        let until = self.client_write_until();
        if let Some(client) = &mut self.client {
            if wire::write(&mut client.stream.io, &frame, until).is_err() {
                self.client = None;
            }
        }
    }

    fn client_write_until(&self) -> Instant {
        let until = Instant::now() + Duration::from_secs(1);
        self.closing.as_ref().map_or(until, |close| until.min(close.deadline))
    }

    fn state(&self) -> &str {
        if self.closing.is_some() { "closing" }
        else if self.record["state"] == "starting" { "starting" }
        else if !self.pending.is_empty() { "blocked" }
        else if self.working { "working" }
        else { "idle" }
    }

    fn hello(&self) -> Value {
        json!({
            "protocolVersion":PROTOCOL_VERSION, "id":self.record["id"], "session":self.record["session"],
            "generation":self.generation, "state":if self.closing.is_some() { "closing" } else { self.record["state"].as_str().unwrap() },
            "head":self.journal.head(), "versions":self.record["versions"], "billing":self.billing,
            "model":self.record["model"], "approvals":self.record["approvals"]
        })
    }

    fn status(&self) -> Value {
        let pending = self.pending.iter().map(|(key, request)| (key.to_string(), request.view.clone())).collect::<serde_json::Map<_, _>>();
        json!({"state":self.state(),"generation":self.generation,"head":self.journal.head(),"pending":pending})
    }

    fn prompt(&mut self, params: &Value) -> anyhow::Result<Result<Value, &'static str>> {
        let (Some(key), Some(text)) = (params["key"].as_str(), params["text"].as_str()) else {
            return Ok(Err("invalid prompt"));
        };
        if key.is_empty() || key.len() > 256 {
            return Ok(Err("invalid delivery key"));
        }
        if let Some(&seq) = self.journal.deliveries().get(key) {
            return Ok(Ok(json!({"seq":seq,"repeat":true})));
        }
        if self.closing.is_some() || self.record["session"].is_null() || params["session"] != self.record["session"] {
            return Ok(Err("gone"));
        }
        if params["generation"].as_u64().is_some_and(|generation| generation < self.generation) {
            return Ok(Err("moved"));
        }
        if params["generation"] != self.generation {
            return Ok(Err("gone"));
        }
        if !self.pending.is_empty() {
            return Ok(Err("blocked"));
        }
        if self.working {
            return Ok(Err("busy"));
        }
        if self.billing["verified"] != true {
            return Ok(Err("billing"));
        }

        let msg = request(3, "session/prompt", json!({"sessionId":self.record["session"],"prompt":[{"type":"text","text":text}]}));
        // A durable key records acceptance. Even a crash before the write must never
        // inject the same prompt a second time.
        let seq = self.journal.append(json!({"kind":"acp","dir":"out","key":key,"msg":msg}), true)?;
        self.working = true;
        let frame = self.adapter.prompt(text);
        self.send(&frame)?;
        Ok(Ok(json!({"seq":seq})))
    }

    fn answer(&mut self, params: &Value) -> anyhow::Result<Result<Value, &'static str>> {
        if self.closing.is_some() || params["generation"] != self.generation {
            return Ok(Err("gone"));
        }
        let (Some(key), Some(option)) = (params["request"].as_u64(), params["optionId"].as_str()) else { return Ok(Err("settled")) };
        let Some(pending) = self.pending.get(&key) else { return Ok(Err("settled")) };
        let kind = pending.view["acp"]["params"]["options"].as_array().and_then(|options| options.iter().find_map(|item| {
            item["kind"].as_str().filter(|kind| item["optionId"] == option && matches!(*kind, "allow_once" | "reject_once"))
        }));
        let Some(kind) = kind else { return Ok(Err("settled")) };
        let (native, id) = (self.adapter.reply(&pending.native, Some(option)), pending.id.clone());
        let until = Instant::now() + Duration::from_secs(1);
        let (seq, delivered) = settle_decision(key, kind, |entry| self.journal.append(entry, true), || {
            wire::write(self.input.as_mut().ok_or(io::ErrorKind::BrokenPipe)?, &native, until)
        })?;
        self.pending.remove(&key);
        if delivered {
            self.acp("out", reply(id, json!({"outcome":{"outcome":"selected","optionId":option}})))?;
        } else {
            self.closing = Some(Closing::new(Instant::now(), false, false));
            self.input.take();
        }
        Ok(Ok(json!({"seq":seq,"replied":delivered})))
    }

    fn agent_message(&mut self) -> anyhow::Result<bool> {
        let Some(output) = &mut self.output else { return Ok(false) };
        let next = match output.next(8 * 1024 * 1024) {
            Ok(Some(frame)) => frame,
            Ok(None) => return Ok(false),
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                self.output = None;
                return Ok(true);
            }
            Err(e) => return self.agent_protocol_error(e.into()),
        };
        let events = match self.adapter.receive(next) {
            Ok(events) => events,
            Err(e) => return self.agent_protocol_error(e),
        };
        for event in events {
            match event {
                Event::Send(frame) => {
                    if frame["method"] == "thread/start" {
                        if self.closing.is_some() {
                            continue;
                        }
                        self.acp("out", request(2, "session/new", json!({"cwd":self.record["cwd"],"mcpServers":[]})))?;
                    }
                    if self.input.is_some() && (self.closing.is_none() || frame["method"] == "turn/interrupt" || frame.get("method").is_none()) {
                        self.send_control(&frame)?;
                    }
                }
                Event::Acp(frame) => { self.acp("in", frame)?; }
                Event::Ready { session, meta } => {
                    if self.closing.is_none() {
                        self.record["session"] = json!(session);
                        self.record["state"] = json!("running");
                        self.record["model"] = meta["model"].clone();
                        self.record["approvals"] = meta["approvals"].clone();
                        self.record["billing"] = self.billing.clone();
                        self.acp("in", reply(json!(2), json!({"sessionId":session,"_meta":{"workriver":meta}})))?;
                        self.host_line("started", json!({"host":self.record["host"],"agentGroup":self.child.id(),"versions":self.record["versions"]}))?;
                        self.directory.save(&self.record)?;
                    }
                }
                Event::Billing(billing) => {
                    self.host_line("billing", billing.clone())?;
                    self.billing = billing;
                }
                Event::Usage(usage) => { self.host_line("usage", usage)?; }
                Event::Unmapped(frame) => { self.journal.append(json!({"kind":"native","dir":"in","msg":frame}), false)?; }
                Event::Approval { native, acp } => self.add_approval(native, acp)?,
                Event::Resolved(id) => {
                    if let Some(key) = self.pending.iter().find_map(|(&key, item)| (item.id == id).then_some(key)) {
                        self.settle(key, "agent", false)?;
                    }
                }
                Event::TurnEnded(frame) => {
                    let cancelled = frame["result"]["stopReason"] == "cancelled";
                    self.acp("in", frame)?;
                    self.working = false;
                    self.finish_cancel(Some(if cancelled {"cancelled"} else {"ended"}));
                }
                Event::Closed => {
                    if let Some(close) = &mut self.closing {
                        close.unsubscribed(Instant::now());
                    }
                }
            }
        }
        Ok(true)
    }

    fn agent_protocol_error(&mut self, failure: anyhow::Error) -> anyhow::Result<bool> {
        if self.working {
            self.acp("in", error(json!(3), -32603, "Codex protocol failed"))?;
            self.working = false;
        }
        if self.closing.is_none() {
            return Err(failure);
        }
        eprintln!("host: {failure:#}");
        self.input.take();
        self.output.take();
        Ok(true)
    }

    fn add_approval(&mut self, native: Value, mut acp: Value) -> anyhow::Result<()> {
        let view = approval_view(&native, &acp);
        if !view["cut"].is_null() {
            acp["params"]["options"] = view["acp"]["params"]["options"].clone();
        }
        let key = self.acp("in", acp)?;
        let id = native["id"].clone();
        // A duplicate active RPC id makes replies ambiguous. Do not send one.
        ensure!(!self.pending.values().any(|pending| pending.id == id), "duplicate active Codex request id");
        let bytes = encoded_len(&view) + 32;
        let projected = self.pending.values().map(|item| item.bytes).sum::<usize>() + bytes + 512;
        let native = json!({"id":id,"method":native["method"]});
        self.pending.insert(key, Pending { id, native, view, bytes });
        if self.closing.is_some() || self.cancelling.is_some() || projected > STATUS_BYTES {
            let by = if self.closing.is_some() { "close" } else if self.cancelling.is_some() { "cancel" } else { "budget" };
            self.settle(key, by, self.input.is_some())?;
        }
        Ok(())
    }

    fn settle(&mut self, key: u64, by: &str, send: bool) -> anyhow::Result<()> {
        let pending = self.pending.remove(&key).expect("settle only a pending request");
        let outcome = if by == "agent" { "resolved" } else { "cancelled" };
        self.host_line("settled", json!({"request":key,"outcome":outcome,"by":by}))?;
        if send {
            let frame = self.adapter.reply(&pending.native, None);
            if self.send_control(&frame)? {
                self.acp("out", reply(pending.id, json!({"outcome":{"outcome":"cancelled"}})))?;
            }
        }
        Ok(())
    }

    fn cancel_turn(&mut self, by: &str) -> anyhow::Result<()> {
        while let Some((&key, _)) = self.pending.first_key_value() {
            self.settle(key, by, self.input.is_some())?;
        }
        if self.working && self.input.is_some() && (by != "close" || self.cancelling.is_none()) {
            self.acp("out", notification("session/cancel", json!({"sessionId":self.record["session"]})))?;
            if let Some(frame) = self.adapter.interrupt() {
                self.send_control(&frame)?;
            }
        }
        Ok(())
    }

    fn finish_cancel(&mut self, state: Option<&str>) {
        if self.cancelling.as_ref().is_some_and(|cancel| state.is_some() || Instant::now() >= cancel.until) {
            let cancel = self.cancelling.take().unwrap();
            if self.client_epoch == cancel.epoch {
                self.respond(reply(cancel.id, json!({"state":state.unwrap_or("sent")})));
            }
        }
    }

    fn send(&mut self, frame: &Value) -> io::Result<()> {
        let input = self.input.as_mut().ok_or(io::ErrorKind::BrokenPipe)?;
        let until = Instant::now() + Duration::from_secs(1);
        let until = self.closing.as_ref().map_or(until, |close| until.min(close.until).min(close.deadline));
        wire::write(input, frame, until)
    }

    fn send_control(&mut self, frame: &Value) -> io::Result<bool> {
        match self.send(frame) {
            Ok(()) => Ok(true),
            Err(_) if self.closing.is_some() => {
                self.input.take();
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    fn acp(&mut self, dir: &str, frame: Value) -> anyhow::Result<u64> {
        if dir == "in" && frame["id"] == 1 && frame["result"]["protocolVersion"] == 1 {
            self.record["versions"] = frame["result"]["_meta"]["workriver"].clone();
        }
        self.journal.append(json!({"kind":"acp","dir":dir,"msg":frame}), false)
    }

    fn host_line(&mut self, event: &str, mut body: Value) -> anyhow::Result<u64> {
        body["kind"] = json!("host");
        body["event"] = json!(event);
        self.journal.append(body, true)
    }

    fn wait(&self) -> io::Result<()> {
        let mut fds = [wire::interested(-1, libc::POLLIN); 4];
        fds[0].fd = self.socket.listener.as_raw_fd();
        if let Some(output) = &self.output { fds[1].fd = output.io.as_raw_fd(); }
        if let Some(client) = &self.client { fds[2].fd = client.stream.io.as_raw_fd(); }
        if let Some(client) = &self.challenger { fds[3].fd = client.stream.io.as_raw_fd(); }
        let now = Instant::now();
        let mut until = now + Duration::from_secs(1);
        if let Some(close) = &self.closing {
            until = until.min(close.until).min(now + Duration::from_millis(30));
        }
        if let Some(cancel) = &self.cancelling {
            until = until.min(cancel.until);
        }
        if let Some(client) = &self.challenger {
            until = until.min(client.expires);
        }
        wire::poll(&mut fds, Some(until.saturating_duration_since(now)))
    }

    fn progress_close(&mut self) -> anyhow::Result<bool> {
        let leader = self.child.id();
        let exited = close::exited(leader).context("cannot observe app-server exit")?;
        if self.closing.is_none() {
            if exited || self.output.is_none() {
                self.closing = Some(Closing::new(Instant::now(), false, false));
                self.input.take();
            } else if self.record["state"] == "starting" && Instant::now() >= self.startup_until {
                anyhow::bail!("Codex startup timed out");
            } else {
                return Ok(false);
            }
        }

        let close = self.closing.as_mut().unwrap();
        let members = if close.checks_group() { close::members(leader) } else { Ok(Vec::new()) };
        let (pids, listing_error) = close::remaining(leader, exited, members);
        let group_empty = listing_error.is_none() && pids.is_empty();
        let now = Instant::now();
        let Some(step) = close.advance(now, !self.working, exited, group_empty) else {
            return Ok(false);
        };
        match step {
            Step::Unsubscribe => {
                if close.requested {
                    if let Some(frame) = self.adapter.close().filter(|_| !exited && self.input.is_some() && !self.record["session"].is_null()) {
                        self.acp("out", request(4, "session/close", json!({"sessionId":self.record["session"]})))?;
                        if !self.send_control(&frame)? {
                            self.closing.as_mut().unwrap().unsubscribed(Instant::now());
                        }
                    } else {
                        self.closing.as_mut().unwrap().unsubscribed(Instant::now());
                    }
                }
            }
            Step::Eof => { self.input.take(); }
            Step::Terminate | Step::Kill => {
                let sig = if step == Step::Terminate { libc::SIGTERM } else { libc::SIGKILL };
                if !group_empty && let Err(error) = close::signal(leader, sig) {
                    eprintln!("host: signalling the app-server group ({sig}): {error}");
                }
            }
            Step::Finish => {
                // The child can exit with unread, already-written protocol frames.
                if exited && group_empty && self.output.is_some() && now < close.deadline {
                    close.until = close.deadline;
                    return Ok(false);
                }
                let requested = close.requested;
                let status = if exited {
                    let status = self.child.try_wait()?.context("exited leader could not be reaped")?;
                    self.reaped = true;
                    Some(status)
                } else {
                    None
                };
                let mut result = outcome(listing_error.is_none() && exited, &pids);
                if let Some(e) = listing_error {
                    result["listingError"] = json!(e.to_string());
                }
                self.cancel_turn(if requested { "close" } else { "exit" })?;
                if requested {
                    self.host_line("closed", result)?;
                    self.record["state"] = json!("closed");
                } else {
                    result["code"] = json!(status.and_then(|s| s.code()));
                    result["signal"] = json!(status.and_then(|s| s.signal()));
                    self.host_line("exited", result)?;
                    self.record["state"] = json!("stopped");
                }
                self.directory.save(&self.record)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn stop_after_failure(&mut self, failure: &anyhow::Error) {
        eprintln!("host: {failure:#}");
        if !self.reaped {
            let deadline = self.closing.as_ref().map_or_else(|| Instant::now() + Duration::from_secs(2), |close| close.deadline);
            self.input.take();
            let _ = close::signal(self.child.id(), libc::SIGKILL);
            while Instant::now() < deadline {
                if self.child.try_wait().ok().flatten().is_some() {
                    self.reaped = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(10).min(deadline.saturating_duration_since(Instant::now())));
            }
        }
        if self.record["state"] != "closed" && self.host_line("exited", json!({"reason":"host failure"})).is_ok() {
            self.record["state"] = json!("stopped");
            let _ = self.directory.save(&self.record);
        }
    }
}

fn valid_request(frame: &Value) -> bool {
    frame["jsonrpc"] == "2.0"
        && (frame["id"].is_number() || frame["id"].as_str().is_some_and(|id| id.len() <= 64 && encoded_len(&frame["id"]) <= 80))
        && frame["method"].is_string()
}

fn proves(sent: &[u8], proof: &[u8; 64]) -> bool {
    sent.len() == proof.len() && sent.iter().zip(proof).fold(0, |diff, (sent, proof)| diff | (sent ^ proof)) == 0
}

fn encoded_len(value: &Value) -> usize {
    serde_json::to_vec(value).expect("a JSON value always encodes").len()
}

fn approval_view(native: &Value, acp: &Value) -> Value {
    let whole = json!({"native":native,"acp":acp});
    let len = encoded_len(&whole);
    if len <= APPROVAL_BYTES {
        return whole;
    }
    let params = &acp["params"];
    let mut short = json!({
        "native":{"id":native["id"],"method":native["method"]},
        "acp":{"jsonrpc":"2.0","id":acp["id"],"method":"session/request_permission","params":{
            "sessionId":params["sessionId"],
            "toolCall":{"toolCallId":params["toolCall"]["toolCallId"],"title":"Approval details exceed the byte limit"},
            "options":[{"optionId":"reject","name":"Reject","kind":"reject_once"}]
        }},
        "cut":len
    });
    if let Some(meta) = params.get("_meta") {
        short["acp"]["params"]["_meta"] = meta.clone();
    }
    if encoded_len(&short) > APPROVAL_BYTES {
        short["native"]["id"] = Value::Null;
        short["acp"]["id"] = json!(0);
        short["acp"]["params"]["toolCall"]["toolCallId"] = json!("truncated");
        short["acp"]["params"]["sessionId"] = json!("truncated");
        short["acp"]["params"].as_object_mut().unwrap().remove("_meta");
    }
    short["cut"] = json!(len.saturating_sub(encoded_len(&short)));
    short
}

fn settle_decision(
    key: u64,
    kind: &str,
    mut write_entry: impl FnMut(Value) -> anyhow::Result<u64>,
    write_pipe: impl FnOnce() -> io::Result<()>,
) -> anyhow::Result<(u64, bool)> {
    let seq = write_entry(json!({"kind":"host","event":"decided","request":key,"optionKind":kind,"by":"user"}))?;
    let replied = write_pipe().is_ok() && write_entry(json!({"kind":"host","event":"replied","request":key})).is_ok();
    Ok((seq, replied))
}
