//! Codex app-server's stable stdio protocol, projected onto the host's ACP subset.
use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

use super::wire::{ACP_CLOSE, ACP_INITIALIZE, ACP_PROMPT, error, notification, reply};

#[cfg(test)]
mod tests;

pub const TESTED_VERSION: &str = include_str!("VERSION");

pub enum Event {
    Send(Value),
    SessionNew(Value),
    Control(Value),
    RequestError(Value),
    Acp(Value),
    Unmapped(Value),
    Skipped { frame: Value, reason: String },
    Ready { session: String, meta: Value },
    Billing(Value),
    Usage(Value),
    Approval { native: Value, acp: Value },
    Resolved(Value),
    TurnEnded(Value),
    Closed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Call {
    Initialize,
    Config,
    Thread,
    Account,
    Prompt,
    Interrupt,
    Close,
}

impl Call {
    fn method(self) -> &'static str {
        match self {
            Self::Initialize => "initialize",
            Self::Config => "config/read",
            Self::Thread => "thread/start",
            Self::Account => "account/read",
            Self::Prompt => "turn/start",
            Self::Interrupt => "turn/interrupt",
            Self::Close => "thread/unsubscribe",
        }
    }
}

struct Turn {
    id: Option<String>,
    interrupt: bool,
    early_completion: Option<(String, Value)>,
}

pub(super) fn launch(cwd: &Path, model: Option<&str>, auth: &str, place: &str) -> Result<(Adapter, Child)> {
    let command = || {
        let names = ["HOME", "USER", "LOGNAME", "PATH", "SHELL", "LANG", "TMPDIR", "TERM", "CODEX_HOME", "WORKRIVER_HOME", "WORKRIVER_PORT"];
        let mut command = Command::new("codex");
        command.current_dir(cwd).env_clear().envs(std::env::vars_os().filter(|(key, _)| {
            names.iter().any(|name| key == OsStr::new(name)) || key.as_encoded_bytes().starts_with(b"LC_")
        }));
        if auth == "api_key" && let Some(key) = std::env::var_os("OPENAI_API_KEY") {
            command.env("OPENAI_API_KEY", key);
        }
        command.env("WORKRIVER_PLACE", place);
        command
    };
    let version = crate::common::process::output_within({
        let mut probe = command();
        probe.arg("--version");
        probe
    }, Duration::from_secs(5))?;
    ensure!(version.status.success(), "cannot read the Codex version");
    let version = String::from_utf8(version.stdout)?;
    let adapter = Adapter::new(cwd, model, auth, version.trim())?;
    let child = command().args(["app-server", "--listen", "stdio://"])
        .process_group(0).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    Ok((adapter, child))
}

pub struct Adapter {
    cwd: String,
    model: Option<String>,
    auth: &'static str,
    version: String,
    next_id: u64,
    calls: HashMap<u64, Call>,
    billing_read: Option<u64>,
    ready: bool,
    session: String,
    meta: Value,
    turn: Option<Turn>,
    diffs: HashMap<(String, String), Value>,
    files: HashMap<(String, String, String), Value>,
    sources: HashMap<String, Value>,
}

impl Adapter {
    pub fn new(cwd: &Path, model: Option<&str>, auth: &str, version: &str) -> Result<Self> {
        ensure!(cwd.is_absolute(), "Codex needs an absolute working directory");
        let auth = match auth {
            "chatgpt" => "chatgpt",
            "api_key" => "apiKey",
            _ => bail!("Codex needs a chatgpt or api_key credential choice"),
        };
        Ok(Self {
            cwd: cwd.to_str().context("Codex needs a UTF-8 working directory")?.into(),
            model: model.map(str::to_owned),
            auth,
            version: version.into(),
            next_id: 0,
            calls: HashMap::new(),
            billing_read: None,
            ready: false,
            session: String::new(),
            meta: Value::Null,
            turn: None,
            diffs: HashMap::new(),
            files: HashMap::new(),
            sources: HashMap::new(),
        })
    }

    fn request(&mut self, call: Call, params: Value) -> Value {
        self.next_id += 1;
        self.calls.insert(self.next_id, call);
        json!({"id": self.next_id, "method": call.method(), "params": params})
    }

    pub fn start(&mut self) -> Value {
        self.request(Call::Initialize, json!({
            "clientInfo": {"name": "workriver", "title": "workriver", "version": env!("CARGO_PKG_VERSION")}
        }))
    }

    pub fn prompt(&mut self, text: &str) -> Value {
        self.turn = Some(Turn { id: None, interrupt: false, early_completion: None });
        self.request(Call::Prompt, json!({
            "threadId": self.session, "input": [{"type": "text", "text": text, "text_elements": []}]
        }))
    }

    pub fn interrupt(&mut self) -> Option<Value> {
        let turn = self.turn.as_mut()?;
        if turn.interrupt {
            return None;
        }
        turn.interrupt = true;
        let turn_id = turn.id.as_deref()?;
        let params = json!({"threadId": self.session, "turnId": turn_id});
        Some(self.request(Call::Interrupt, params))
    }

    pub fn close(&mut self) -> Option<Value> {
        if self.session.is_empty() {
            return None;
        }
        Some(self.request(Call::Close, json!({"threadId": self.session})))
    }

    pub fn reply(&self, request: &Value, option: Option<&str>) -> Value {
        let result = if request["method"] == "item/permissions/requestApproval" {
            json!({"permissions": {}, "scope": "turn"})
        } else {
            let decision = match option {
                Some("allow") => "accept",
                None => "cancel",
                Some(_) => "decline",
            };
            json!({"decision": decision})
        };
        json!({"id": request["id"], "result": result})
    }

    pub fn receive(&mut self, frame: Value) -> Result<Vec<Event>> {
        let mut events = Vec::new();
        if let Some(method) = frame["method"].as_str() {
            if frame.get("id").is_some() {
                match self.approval(method, &frame) {
                    Ok(Some(acp)) => events.push(Event::Approval { native: frame, acp }),
                    Ok(None) => {
                        let response = json!({"id": frame["id"], "error": {"code": -32601, "message": "Method not found"}});
                        events.push(Event::Unmapped(safe_frame(frame)));
                        events.push(Event::RequestError(response));
                    }
                    Err(reason) => {
                        let response = rpc_id(&frame, "id").ok().map(|_| self.reply(&frame, None));
                        events.push(skipped(frame, &reason.to_string()));
                        if let Some(response) = response {
                            events.push(Event::RequestError(response));
                        }
                    }
                }
            } else if method == "turn/diff/updated" {
                if let Some(skipped) = self.remember_diff(frame) {
                    events.push(skipped);
                }
            } else {
                let params = &frame["params"];
                match self.notification(method, params, &mut events) {
                    Ok(true) => {}
                    Ok(false) => events.push(Event::Unmapped(safe_frame(frame))),
                    Err(reason) => {
                        let main_completion = method == "turn/completed"
                            && params["threadId"].as_str().unwrap_or(&self.session) == self.session;
                        let reported_id = params["turn"]["id"].as_str().map(str::to_owned);
                        let reason = reason.to_string();
                        events.push(skipped(frame, &reason));
                        if main_completion && let Some(turn) = &mut self.turn {
                            let failed = error(json!(ACP_PROMPT), -32603, &reason);
                            match (turn.id.as_deref(), reported_id) {
                                (None, Some(id)) => turn.early_completion = Some((id, failed)),
                                (Some(active), Some(id)) if active != id => {}
                                _ => self.end_turn(failed, &mut events),
                            }
                        }
                    }
                }
            }
            return Ok(events);
        }

        let Some(id) = frame["id"].as_u64() else {
            return Ok(vec![skipped(frame, "Codex response has no request id")]);
        };
        let Some(call) = self.calls.remove(&id) else {
            events.push(Event::Unmapped(json!({"id": id, "redacted": true})));
            return Ok(events);
        };
        if call == Call::Account && self.billing_read != Some(id) {
            return Ok(events);
        }
        if let Some(error) = frame.get("error") {
            match call {
                Call::Prompt => {
                    self.end_turn(acp_error(ACP_PROMPT, error), &mut events);
                }
                Call::Close => {
                    events.extend([Event::Acp(acp_error(ACP_CLOSE, error)), Event::Closed]);
                }
                Call::Account if self.ready => {
                    events.push(Event::Billing(json!({"verified": false, "provider": "openai", "reason": "account read failed"})));
                }
                Call::Interrupt => events.push(Event::Unmapped(frame)),
                _ => bail!("Codex {} failed ({})", call.method(), error["code"].as_i64().unwrap_or(-32603)),
            }
            return Ok(events);
        }
        let Some(result) = frame.get("result") else {
            if matches!(call, Call::Initialize | Call::Config | Call::Thread) || (call == Call::Account && !self.ready) {
                bail!("Codex {} response has no result", call.method());
            }
            let reason = "Codex response has no result";
            events.push(skipped(frame, reason));
            if call == Call::Prompt {
                self.end_turn(error(json!(ACP_PROMPT), -32603, reason), &mut events);
            }
            return Ok(events);
        };
        match call {
            Call::Initialize => {
                let output = self.version.trim();
                let version = output.strip_prefix("codex-cli ").unwrap_or(output);
                events.push(Event::Acp(reply(json!(ACP_INITIALIZE), json!({
                    "protocolVersion": 1,
                    "agentCapabilities": {"sessionCapabilities": {"close": {}}},
                    "agentInfo": {"name": "codex", "title": "Codex", "version": version},
                    "authMethods": [],
                    "_meta": {"workriver": {
                        "harness": "codex", "harnessVersion": version, "adapterVersion": env!("CARGO_PKG_VERSION"),
                        "testedHarnessVersion": TESTED_VERSION.trim()
                    }}
                }))));
                events.push(Event::Send(json!({"method": "initialized"})));
                let params = json!({"cwd": self.cwd, "includeLayers": true});
                events.push(Event::Send(self.request(Call::Config, params)));
            }
            Call::Config => {
                check_config(result)?;
                let mut params = json!({"cwd": self.cwd});
                if let Some(model) = &self.model {
                    params["model"] = json!(model);
                }
                events.push(Event::SessionNew(self.request(Call::Thread, params)));
            }
            Call::Thread => {
                ensure!(result["modelProvider"] == "openai", "Codex reported a model provider other than openai");
                self.session = field(&result["thread"], "id")?.into();
                self.meta = json!({
                    "model": field(result, "model")?,
                    "modelProvider": "openai",
                    "approvals": {
                        "approvalPolicy": result["approvalPolicy"],
                        "approvalsReviewer": result["approvalsReviewer"],
                        "sandbox": result["sandbox"]
                    }
                });
                events.push(Event::Send(self.read_billing()));
            }
            Call::Account => self.account(result, &mut events)?,
            Call::Prompt => {
                let Some(turn) = self.turn.as_mut() else { return Ok(events) };
                let Ok(turn_id) = field(&result["turn"], "id") else {
                    let reason = "Codex turn response needs a string id";
                    events.push(skipped(frame, reason));
                    self.end_turn(error(json!(ACP_PROMPT), -32603, reason), &mut events);
                    return Ok(events);
                };
                turn.id = Some(turn_id.into());
                if let Some((_, early)) = turn.early_completion.take().filter(|(id, _)| id == turn_id) {
                    self.end_turn(early, &mut events);
                } else if turn.interrupt {
                    let params = json!({"threadId": self.session, "turnId": turn_id});
                    events.push(Event::Control(self.request(Call::Interrupt, params)));
                }
            }
            Call::Close => events.extend([Event::Acp(reply(json!(ACP_CLOSE), json!({}))), Event::Closed]),
            Call::Interrupt => {}
        }
        Ok(events)
    }

    fn read_billing(&mut self) -> Value {
        // Only the latest result can unblock prompts, even if replies come out of order.
        let req = self.request(Call::Account, json!({"refreshToken": false}));
        self.billing_read = Some(self.next_id);
        req
    }

    fn account(&mut self, response: &Value, events: &mut Vec<Event>) -> Result<()> {
        let native = &response["account"];
        let kind = match native["type"].as_str() {
            Some("chatgpt") => "chatgpt",
            Some("apiKey") => "api_key",
            _ if native.is_null() => "none",
            _ => "unsupported",
        };
        let verified = native["type"] == self.auth;
        ensure!(self.ready || verified, "Codex's account kind {kind} does not match the credential choice");
        let mut billing = json!({"verified": verified, "provider": "openai", "account": kind});
        if kind == "chatgpt" && native["planType"].is_string() {
            billing["plan"] = native["planType"].clone();
        }
        if !verified {
            billing["reason"] = json!("account kind does not match the credential choice");
        }
        events.push(Event::Billing(billing.clone()));
        if !self.ready {
            self.ready = true;
            self.meta["billing"] = billing;
            events.push(Event::Ready { session: self.session.clone(), meta: std::mem::take(&mut self.meta) });
        }
        Ok(())
    }

    fn notification(&mut self, method: &str, params: &Value, events: &mut Vec<Event>) -> Result<bool> {
        let thread = params["threadId"].as_str().unwrap_or(&self.session).to_owned();
        match method {
            "account/updated" => {
                if !self.session.is_empty() {
                    events.push(Event::Billing(json!({"verified": false, "provider": "openai", "reason": "account changed"})));
                    events.push(Event::Send(self.read_billing()));
                }
            }
            "serverRequest/resolved" => {
                events.push(Event::Resolved(rpc_id(params, "requestId")?.clone()));
            }
            "thread/started" => {
                let thread = &params["thread"];
                let spawn = &thread["source"]["subAgent"]["thread_spawn"];
                let parent = thread["parentThreadId"].as_str().or_else(|| spawn["parent_thread_id"].as_str());
                if let Some(parent) = parent {
                    let agent = thread["agentNickname"].as_str()
                        .or_else(|| spawn["agent_nickname"].as_str())
                        .or_else(|| thread["agentRole"].as_str())
                        .or_else(|| spawn["agent_role"].as_str());
                    self.remember_source(field(thread, "id")?, parent, agent);
                }
                return Ok(false);
            }
            "turn/completed" => {
                let ended = &params["turn"];
                let id = field(ended, "id")?;
                self.files.retain(|(source, turn, _), _| source != &thread || turn != id);
                if let Some(diff) = self.diffs.remove(&(thread.clone(), id.into())) {
                    events.push(Event::Unmapped(diff));
                }
                let Some(turn) = self.turn.as_mut().filter(|_| thread == self.session) else { return Ok(false) };
                match turn.id.as_deref() {
                    Some(active) if id == active => self.end_turn(turn_result(ended)?, events),
                    None => turn.early_completion = Some((id.into(), turn_result(ended)?)),
                    _ => return Ok(false),
                }
            }
            _ if self.session.is_empty() => return Ok(false),
            "item/agentMessage/delta" | "item/reasoning/summaryTextDelta" | "item/reasoning/textDelta" => {
                let update = if method == "item/agentMessage/delta" { "agent_message_chunk" } else { "agent_thought_chunk" };
                events.push(self.update(&thread, json!({
                    "sessionUpdate": update, "content": {"type": "text", "text": field(params, "delta")?}
                })));
            }
            "item/started" | "item/completed" => {
                let Some(event) = self.item(&thread, params, method == "item/started")? else { return Ok(false) };
                events.push(event);
            }
            "turn/plan/updated" => {
                let entries = params["plan"].as_array().context("Codex plan must contain steps")?.iter().map(|step| {
                    let status = match field(step, "status")? {
                        "pending" => "pending",
                        "inProgress" => "in_progress",
                        "completed" => "completed",
                        _ => bail!("Codex reported an invalid plan step status"),
                    };
                    Ok(json!({"content": field(step, "step")?, "priority": "medium", "status": status}))
                }).collect::<Result<Vec<_>>>()?;
                events.push(self.update(&thread, json!({"sessionUpdate": "plan", "entries": entries})));
            }
            "thread/tokenUsage/updated" => {
                let tokens = &params["tokenUsage"];
                let mut usage = json!({"source": "codex:thread/tokenUsage/updated", "thread": thread,
                    "last": tokens["last"], "total": tokens["total"], "modelContextWindow": tokens["modelContextWindow"]});
                self.tag(&thread, &mut usage);
                events.push(Event::Usage(usage));
                if let (Some(used), Some(size)) = (tokens["last"]["totalTokens"].as_u64(), tokens["modelContextWindow"].as_u64()) {
                    events.push(self.update(&thread, json!({"sessionUpdate": "usage_update", "used": used, "size": size})));
                }
            }
            "item/autoApprovalReview/completed" if params["review"]["status"] == "denied" => {
                events.push(Event::Unmapped(json!({"method": method, "params": params})));
                let target = params["targetItemId"].as_str().or_else(|| params["reviewId"].as_str())
                    .context("Codex approval review has no target or review id")?;
                events.push(self.update(&thread, json!({"sessionUpdate": "tool_call_update",
                    "toolCallId": self.item_id(&thread, target), "status": "failed",
                    "_meta": {"workriver": {"denied": {"by": "reviewer"}}}})));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn item(&mut self, thread: &str, params: &Value, starting: bool) -> Result<Option<Event>> {
        let item = &params["item"];
        let item_type = field(item, "type")?;
        let kind = match item_type {
            "commandExecution" => "execute",
            "fileChange" => "edit",
            "mcpToolCall" | "dynamicToolCall" | "collabAgentToolCall" => "other",
            "webSearch" => "fetch",
            "contextCompaction" => {
                let id = self.item_id(thread, field(item, "id")?);
                let status = if starting { "in_progress" } else if item["status"] == "failed" { "failed" } else { "completed" };
                return Ok(Some(self.update(thread, json!({"sessionUpdate": "compaction_update", "compactionId": id, "status": status}))));
            }
            _ => return Ok(None),
        };
        let id = field(item, "id")?;
        if item_type == "fileChange" && let Some(turn) = params["turnId"].as_str() {
            let key = (thread.into(), turn.into(), id.into());
            if starting {
                self.files.insert(key, item["changes"].clone());
            } else {
                self.files.remove(&key);
            }
        }
        if item_type == "collabAgentToolCall" && item["tool"] == "spawnAgent"
            && let Some(receivers) = item["receiverThreadIds"].as_array()
        {
            let parent = item["senderThreadId"].as_str().unwrap_or(thread);
            for child in receivers.iter().filter_map(Value::as_str) {
                self.remember_source(child, parent, None);
            }
        }
        let title = match item_type {
            "commandExecution" => item["command"].as_str().unwrap_or("Command").to_owned(),
            "fileChange" => "File change".to_owned(),
            "mcpToolCall" => format!("{} / {}", field(item, "server")?, field(item, "tool")?),
            "dynamicToolCall" | "collabAgentToolCall" => field(item, "tool")?.to_owned(),
            _ => item["query"].as_str().unwrap_or("Web search").to_owned(),
        };
        let mut update = json!({"toolCallId": self.item_id(thread, id), "kind": kind, "title": title});
        if starting {
            update["sessionUpdate"] = json!("tool_call");
            update["status"] = json!("in_progress");
            update["rawInput"] = item.clone();
        } else {
            update["sessionUpdate"] = json!("tool_call_update");
            let failed = matches!(item["status"].as_str(), Some("failed" | "declined" | "inProgress"));
            update["status"] = json!(if failed { "failed" } else { "completed" });
            update["rawOutput"] = item.clone();
        }
        Ok(Some(self.update(thread, update)))
    }

    fn approval(&self, method: &str, frame: &Value) -> Result<Option<Value>> {
        let params = &frame["params"];
        let (kind, title, mut raw, why) = match method {
            "item/commandExecution/requestApproval" => {
                let command = params["command"].as_str().filter(|c| !c.trim().is_empty());
                let why = if command.is_none() || params["kind"] == "writeStdin" {
                    Some("The command to approve is unavailable.")
                } else if !params["additionalPermissions"].is_null() {
                    Some("This request includes additional permissions that cannot be approved here.")
                } else if !params["availableDecisions"].is_null() &&
                    !params["availableDecisions"].as_array().is_some_and(|options| options.iter().any(|option| option == "accept"))
                {
                    Some("Codex did not offer approval for this command.")
                } else { None };
                ("execute", "Command approval",
                    json!({"command": params["command"], "cwd": params["cwd"], "reason": params["reason"],
                        "networkApprovalContext": params["networkApprovalContext"], "environmentId": params["environmentId"],
                        "additionalPermissions": params["additionalPermissions"]}), why)
            }
            "item/fileChange/requestApproval" => {
                let thread = field(params, "threadId")?;
                let item = field(params, "itemId")?;
                let changes = params["turnId"].as_str().and_then(|turn| self.files.get(&(thread.into(), turn.into(), item.into())));
                let why = if !params["grantRoot"].is_null() {
                    Some("This request would grant directory access for the session.")
                } else if !changes.and_then(Value::as_array).is_some_and(|changes| !changes.is_empty() &&
                    changes.iter().all(|change| change["path"].is_string() && change["diff"].is_string()))
                {
                    Some("The complete file changes are unavailable.")
                } else { None };
                ("edit", "File change approval", json!({"changes": changes, "reason": params["reason"], "grantRoot": params["grantRoot"]}), why)
            }
            "item/permissions/requestApproval" => {
                ("other", "Permissions approval", json!({"permissions": params["permissions"], "scope": "turn", "cwd": params["cwd"], "reason": params["reason"]}),
                    Some("This request would grant permissions for the whole turn."))
            }
            _ => return Ok(None),
        };
        let thread = field(params, "threadId")?;
        let id = rpc_id(frame, "id")?;
        let mut options = Vec::new();
        if let Some(why) = why {
            raw["incomplete"] = json!(why);
        } else {
            options.push(json!({"optionId": "allow", "name": "Allow once", "kind": "allow_once"}));
        }
        options.push(json!({"optionId": "reject", "name": "Reject", "kind": "reject_once"}));
        let mut tool = json!({"toolCallId": self.item_id(thread, field(params, "itemId")?), "title": title, "kind": kind, "rawInput": raw});
        self.tag(thread, &mut tool);
        let mut args = json!({"sessionId": self.session, "toolCall": tool, "options": options});
        self.tag(thread, &mut args);
        Ok(Some(super::wire::rpc_request(id.clone(), "session/request_permission", args)))
    }

    fn update(&self, thread: &str, mut update: Value) -> Event {
        self.tag(thread, &mut update);
        let mut params = json!({"sessionId": self.session, "update": update});
        self.tag(thread, &mut params);
        Event::Acp(notification("session/update", params))
    }

    fn remember_diff(&mut self, frame: Value) -> Option<Event> {
        let params = &frame["params"];
        let (Some(thread), Some(id), Some(_)) = (params["threadId"].as_str(), params["turnId"].as_str(), params["diff"].as_str()) else {
            return Some(skipped(frame, "Codex diff needs a thread id, turn id and diff"));
        };
        if thread == self.session {
            let turn = self.turn.as_ref()?;
            if turn.id.as_deref().is_some_and(|active| active != id) || turn.early_completion.is_some() {
                return None;
            }
            self.diffs.retain(|(source, previous), _| source != thread || previous == id);
        }
        self.diffs.insert((thread.into(), id.into()), frame);
        None
    }

    fn end_turn(&mut self, result: Value, events: &mut Vec<Event>) {
        let turn = self.turn.take();
        let id = turn.as_ref().and_then(|turn| turn.id.as_deref());
        events.extend(self.diffs.drain().filter_map(|((thread, turn), diff)| {
            (thread != self.session || id.is_none_or(|id| id == turn)).then_some(Event::Unmapped(diff))
        }));
        self.files.retain(|(thread, _, _), _| thread != &self.session);
        self.calls.retain(|_, call| *call != Call::Prompt);
        events.push(Event::TurnEnded(result));
    }

    fn remember_source(&mut self, thread: &str, parent: &str, nickname: Option<&str>) {
        let source = self.sources.entry(thread.into()).or_insert_with(|| json!({"thread": thread}));
        source["parentThread"] = json!(parent);
        if let Some(nickname) = nickname {
            source["agent"] = json!(nickname);
        }
    }

    fn tag(&self, thread: &str, projected: &mut Value) {
        if thread != self.session && !thread.is_empty() {
            projected["_meta"]["workriver"]["source"] = self.sources.get(thread).cloned()
                .unwrap_or_else(|| json!({"thread": thread}));
        }
    }

    fn item_id(&self, thread: &str, item: &str) -> String {
        if thread == self.session { item.into() } else { format!("{thread}/{item}") }
    }
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key].as_str().with_context(|| format!("Codex frame needs a string {key}"))
}

fn rpc_id<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    let id = &value[key];
    ensure!(id.is_string() || id.as_i64().is_some() || id.as_u64().is_some(), "Codex frame needs a JSON-RPC id in {key}");
    Ok(id)
}

fn safe_frame(mut frame: Value) -> Value {
    // Authentication payloads can contain account identifiers or credentials.
    if frame["method"].as_str().is_some_and(|method| method.starts_with("account/")) {
        frame.as_object_mut().unwrap().remove("params");
        frame["redacted"] = json!(true);
    }
    frame
}

fn skipped(frame: Value, reason: &str) -> Event {
    let frame = if frame["method"].is_string() {
        safe_frame(frame)
    } else {
        // A response body may contain configuration or account details.
        json!({"id": frame["id"], "redacted": true})
    };
    Event::Skipped { frame, reason: reason.into() }
}

fn turn_result(turn: &Value) -> Result<Value> {
    let id = &turn["id"];
    Ok(match field(turn, "status")? {
        "completed" => reply(json!(ACP_PROMPT), json!({"stopReason": "end_turn", "_meta": {"workriver": {"turn": id}}})),
        "interrupted" => reply(json!(ACP_PROMPT), json!({"stopReason": "cancelled", "_meta": {"workriver": {"turn": id}}})),
        "failed" => acp_error(ACP_PROMPT, &json!({"code": -32603, "message": turn["error"]["message"], "data": {"turn": id}})),
        _ => bail!("Codex reported an invalid terminal turn status"),
    })
}

fn acp_error(id: u64, native: &Value) -> Value {
    let code = native["code"].as_i64().unwrap_or(-32603);
    let message = native["message"].as_str().unwrap_or("Codex request failed");
    let mut frame = error(json!(id), code, message);
    if let Some(data) = native.get("data") {
        frame["error"]["data"] = data.clone();
    }
    frame
}

fn check_config(response: &Value) -> Result<()> {
    let layers = response["layers"].as_array().context("Codex did not return its configuration layers")?;
    for layer in layers {
        let name = field(&layer["name"], "type")?;
        check_layer(&layer["config"], name, false)?;
    }
    check_layer(&response["config"], "effective", true)
}

fn check_layer(config: &Value, name: &str, effective: bool) -> Result<()> {
    ensure!(config.is_object(), "Codex {name} configuration is missing");
    // Codex's effective object always includes these built-in defaults. The raw layers
    // report explicit settings, including an explicit value equal to a default.
    for (key, built_in) in [
        ("model_provider", config["model_provider"] == "openai"),
        ("openai_base_url", false),
        ("chatgpt_base_url", effective && config["chatgpt_base_url"] == "https://chatgpt.com/backend-api/"),
        ("model_providers", effective && config["model_providers"].as_object().is_some_and(|map| map.is_empty())),
    ] {
        ensure!(config[key].is_null() || built_in, "Codex {name} configuration sets {key}");
    }
    Ok(())
}
