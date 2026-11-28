//! Codex app-server's stable stdio protocol, projected onto the host's ACP subset.
use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

#[cfg(test)]
mod tests;

pub const TESTED_VERSION: &str = include_str!("VERSION");

pub enum Event {
    Send(Value),
    Acp(Value),
    Unmapped(Value),
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
    early_completion: Option<Value>,
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
                if let Some(acp) = self.approval(method, &frame)? {
                    events.push(Event::Approval { native: frame, acp });
                } else {
                    let error = json!({"id": frame["id"], "error": {"code": -32601, "message": "Method not found"}});
                    events.push(unmapped(frame));
                    events.push(Event::Send(error));
                }
            } else if !self.notification(method, &frame["params"], &mut events)? {
                events.push(unmapped(frame));
            }
            return Ok(events);
        }

        let id = frame["id"].as_u64().context("Codex response has no request id")?;
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
                    self.turn = None;
                    events.push(Event::TurnEnded(acp_error(3, error)));
                }
                Call::Close => {
                    events.extend([Event::Acp(acp_error(4, error)), Event::Closed]);
                }
                Call::Account if self.ready => {
                    events.push(Event::Billing(json!({"verified": false, "provider": "openai", "reason": "account read failed"})));
                }
                Call::Interrupt => events.push(Event::Unmapped(frame)),
                _ => bail!("Codex {} failed ({})", call.method(), error["code"].as_i64().unwrap_or(-32603)),
            }
            return Ok(events);
        }
        let result = frame.get("result").context("Codex response has no result")?;
        match call {
            Call::Initialize => {
                let output = self.version.trim();
                let version = output.strip_prefix("codex-cli ").unwrap_or(output);
                events.push(Event::Acp(acp_result(1, json!({
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
                events.push(Event::Send(self.request(Call::Thread, params)));
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
                let turn_id = field(&result["turn"], "id")?;
                let turn = self.turn.as_mut().context("Codex acknowledged a turn with no prompt")?;
                turn.id = Some(turn_id.into());
                if let Some(early) = turn.early_completion.take().filter(|early| early["id"] == turn_id) {
                    self.finish_turn(&early, &mut events)?;
                } else if turn.interrupt {
                    let params = json!({"threadId": self.session, "turnId": turn_id});
                    events.push(Event::Send(self.request(Call::Interrupt, params)));
                }
            }
            Call::Close => events.extend([Event::Acp(acp_result(4, json!({}))), Event::Closed]),
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
                let Some(turn) = self.turn.as_mut().filter(|_| thread == self.session) else { return Ok(false) };
                match turn.id.as_deref() {
                    Some(active) if id == active => self.finish_turn(ended, events)?,
                    None => turn.early_completion = Some(ended.clone()),
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
        Ok(Some(json!({"jsonrpc": "2.0", "id": id, "method": "session/request_permission", "params": args})))
    }

    fn update(&self, thread: &str, mut update: Value) -> Event {
        self.tag(thread, &mut update);
        let mut params = json!({"sessionId": self.session, "update": update});
        self.tag(thread, &mut params);
        Event::Acp(json!({"jsonrpc": "2.0", "method": "session/update", "params": params}))
    }

    fn finish_turn(&mut self, turn: &Value, events: &mut Vec<Event>) -> Result<()> {
        let id = field(turn, "id")?;
        let result = match field(turn, "status")? {
            "completed" => acp_result(3, json!({"stopReason": "end_turn", "_meta": {"workriver": {"turn": id}}})),
            "interrupted" => acp_result(3, json!({"stopReason": "cancelled", "_meta": {"workriver": {"turn": id}}})),
            "failed" => acp_error(3, &json!({"code": -32603, "message": turn["error"]["message"], "data": {"turn": id}})),
            _ => bail!("Codex reported an invalid terminal turn status"),
        };
        self.turn = None;
        events.push(Event::TurnEnded(result));
        Ok(())
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

fn unmapped(mut frame: Value) -> Event {
    // Authentication payloads can contain account identifiers or credentials.
    if frame["method"].as_str().is_some_and(|method| method.starts_with("account/")) {
        frame.as_object_mut().unwrap().remove("params");
        frame["redacted"] = json!(true);
    }
    Event::Unmapped(frame)
}

fn acp_result(id: u8, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn acp_error(id: u8, error: &Value) -> Value {
    let code = error["code"].as_i64().unwrap_or(-32603);
    let message = error["message"].as_str().unwrap_or("Codex request failed");
    let mut frame = json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}});
    if let Some(data) = error.get("data") {
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
