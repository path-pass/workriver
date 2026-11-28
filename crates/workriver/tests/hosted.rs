//! The real host's socket and one isolated, scripted Codex app-server per test.
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_workriver");
const GENERATION: u64 = 2;
const TEST_PORT: &str = "47992";

struct Host {
    child: Child,
    dir: PathBuf,
    _temp: tempfile::TempDir,
}

impl Host {
    fn spawn(options: Value) -> Self {
        // Darwin's Unix-domain socket addresses are shorter than $TMPDIR on some Macs.
        let temp = tempfile::Builder::new().prefix("wr-host-test-").tempdir_in("/tmp").unwrap();
        let root = temp.path();
        let dir = root.join("instance");
        for name in ["instance", "os-home", "codex-home", "workriver-home", "cwd", "bin"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        private(&dir, 0o700);
        fs::write(dir.join("host.key"), [0x8d; 32]).unwrap();
        private(&dir.join("host.key"), 0o600);
        let config = json!({
            "id": "a1", "generation": GENERATION, "state": "starting", "harness": "codex",
            "cwd": root.join("cwd"), "model": "requested-model", "auth": "chatgpt",
        });
        fs::write(dir.join("instance.json"), config.to_string()).unwrap();
        private(&dir.join("instance.json"), 0o600);
        fs::write(root.join("cwd/options.json"), options.to_string()).unwrap();
        let executable = std::env::current_exe().unwrap();
        let executable = executable.to_str().unwrap().replace('\'', "'\\''");
        let shim = format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'codex-cli 0.172.0'; exit; fi\n\
             [ \"$*\" = 'app-server --listen stdio://' ] || exit 3\n\
             export HOSTED_TEST_PIPE=1\n\
             exec '{executable}' --exact fake::fake_codex_app_server --ignored --nocapture 3>&1 1>&2\n"
        );
        fs::write(root.join("bin/codex"), shim).unwrap();
        private(&root.join("bin/codex"), 0o700);
        let child = Self::command(root, &dir).spawn().unwrap();
        Self { child, dir, _temp: temp }
    }

    fn command(root: &Path, dir: &Path) -> Command {
        let log = OpenOptions::new().create(true).append(true).open(root.join("host.log")).unwrap();
        let mut command = Command::new(BIN);
        command.env_clear()
            .env("HOME", root.join("os-home"))
            .env("CODEX_HOME", root.join("codex-home"))
            .env("WORKRIVER_HOME", root.join("workriver-home"))
            .env("WORKRIVER_PORT", TEST_PORT)
            .env("WORKRIVER_UNRELATED", "must not reach Codex")
            .env("OPENAI_BASE_URL", "http://example.invalid/")
            .env("OPENAI_API_KEY", "fake-secret")
            .env("PATH", format!("{}:/usr/bin:/bin", root.join("bin").display()))
            .current_dir(root.join("cwd")).args(["host", dir.to_str().unwrap()])
            .stdin(Stdio::null()).stdout(log.try_clone().unwrap()).stderr(log);
        command
    }

    fn root(&self) -> &Path {
        self.dir.parent().unwrap()
    }

    fn connect(&mut self) -> Wire {
        let at = self.dir.join("host.sock");
        until("host accepts connections", || match UnixStream::connect(&at) {
            Ok(socket) => Some(Wire::new(socket)),
            Err(error) => {
                assert!(self.child.try_wait().unwrap().is_none(), "{error}; {}", self.log());
                None
            }
        })
    }

    fn ready(&mut self) -> (Wire, Value) {
        let mut wire = self.connect();
        let mut hello = wire.hello(1, "8d".repeat(32));
        assert!(hello.get("error").is_none(), "{hello}; {}", self.log());
        if hello["result"]["session"].is_null() {
            until("host finishes startup", || {
                hello = wire.hello(1, "8d".repeat(32));
                (!hello["result"]["session"].is_null()).then_some(())
            });
        }
        assert!(matches!(hello["result"]["state"].as_str(), Some("idle" | "running")), "{hello}");
        (wire, hello["result"].clone())
    }

    fn log(&self) -> String {
        fs::read_to_string(self.root().join("host.log")).unwrap_or_default()
    }

    fn native(&self) -> Vec<Value> {
        lines(&self.root().join("cwd/native.jsonl"))
    }

    fn journal(&self) -> Vec<Value> {
        lines(&self.dir.join("journal.jsonl"))
    }

    fn ended(&mut self) {
        until("host exits", || self.child.try_wait().unwrap());
    }

    fn restart(&mut self, generation: u64, options: Value) {
        let _ = self.child.kill();
        self.child.wait().unwrap();
        let at = self.dir.join("instance.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&at).unwrap()).unwrap();
        config["generation"] = json!(generation);
        config["state"] = json!("starting");
        config["session"] = Value::Null;
        config["model"] = json!("requested-model");
        fs::write(at, config.to_string()).unwrap();
        fs::write(self.root().join("cwd/options.json"), options.to_string()).unwrap();
        self.child = Self::command(self.root(), &self.dir).spawn().unwrap();
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn private(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn lines(path: &Path) -> Vec<Value> {
    let Ok(input) = File::open(path) else { return vec![] };
    BufReader::new(input).lines().map_while(Result::ok)
        .map_while(|line| serde_json::from_str(&line).ok()).collect()
}

fn until<T>(description: &str, mut check: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(value) = check() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {description}");
        thread::sleep(Duration::from_millis(15));
    }
}

struct Wire {
    stream: BufReader<UnixStream>,
    id: u64,
}

impl Wire {
    fn new(socket: UnixStream) -> Self {
        socket.set_read_timeout(Some(Duration::from_secs(12))).unwrap();
        socket.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
        Self { stream: BufReader::new(socket), id: 0 }
    }

    fn send(&mut self, method: &str, params: Value) {
        self.id += 1;
        let frame = json!({"jsonrpc":"2.0", "id":self.id, "method":method, "params":params});
        self.stream.get_mut().write_all(format!("{frame}\n").as_bytes()).unwrap();
    }

    fn receive(&mut self) -> (Value, usize) {
        let mut line = String::new();
        let len = self.stream.read_line(&mut line).unwrap();
        assert!(len > 0, "host closed the socket before replying");
        let frame: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(frame["jsonrpc"], "2.0", "{frame}");
        assert_eq!(frame["id"], self.id, "{frame}");
        (frame, len)
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.send(method, params);
        self.receive().0
    }

    fn result(&mut self, method: &str, params: Value) -> Value {
        let response = self.call(method, params);
        assert!(response.get("result").is_some(), "{method}: {response}");
        response["result"].clone()
    }

    fn hello(&mut self, version: u64, proof: String) -> Value {
        self.call("hello", json!({"proof":proof, "protocolVersion":version}))
    }

    fn refuses(&mut self, method: &str, params: Value, expected: &str) {
        let response = self.call(method, params);
        assert_eq!(response["error"]["message"], expected, "{method}: {response}");
    }

    fn pendings(&mut self, count: usize) -> Value {
        until("the expected pending count", || {
            let status = self.result("status", json!({}));
            (status["pending"].as_object().unwrap().len() == count).then(|| status["pending"].clone())
        })
    }
}

fn fixture() -> Value {
    serde_json::from_str(include_str!("../src/runtime/acp/__fixtures__/conversation.json")).unwrap()
}

#[test]
fn hello_requires_a_compatible_authenticated_client_and_one_writer() {
    let mut host = Host::spawn(json!({}));
    let (mut wire, hello) = host.ready();
    assert_eq!(hello["protocolVersion"], 1);
    assert_eq!(hello["id"], "a1");
    assert_eq!(hello["session"], fixture()["session"]);
    assert_eq!(hello["generation"], GENERATION);
    assert_eq!(hello["billing"]["verified"], true);
    let env: Value = serde_json::from_str(&fs::read_to_string(host.root().join("cwd/environment.json")).unwrap()).unwrap();
    assert_eq!(env["WORKRIVER_HOME"], host.root().join("workriver-home").to_str().unwrap());
    assert_eq!(env["CODEX_HOME"], host.root().join("codex-home").to_str().unwrap());
    assert_eq!(env["WORKRIVER_PORT"], TEST_PORT);
    assert_eq!(env["WORKRIVER_PLACE"], format!("acp:a1/{GENERATION}"));
    for name in ["OPENAI_BASE_URL", "OPENAI_API_KEY", "WORKRIVER_UNRELATED"] {
        assert!(env.get(name).is_none(), "{name} reached the app-server");
    }

    let mut incompatible = host.connect();
    assert_eq!(incompatible.hello(2, "8d".repeat(32))["error"]["message"], "incompatible");
    let mut wrong_key = host.connect();
    assert!(wrong_key.hello(1, "aa".repeat(32)).get("error").is_some());
    assert_eq!(wire.result("status", json!({}))["generation"], GENERATION);

    let rejected = Host::command(host.root(), &host.dir).status().unwrap();
    assert!(!rejected.success(), "a second host acquired the instance lock");
    let mut reattached = host.connect();
    let next = reattached.hello(1, "8d".repeat(32));
    assert_eq!(next["result"]["session"], hello["session"]);
    assert!(next["result"]["head"].as_u64().unwrap() >= hello["head"].as_u64().unwrap());
    assert_eq!(reattached.result("close", json!({"generation":GENERATION}))["accepted"], true);
    host.ended();
}

#[path = "__fixtures__/hosted_app_server.rs"]
mod fake;

fn prompt(generation: u64, key: &str, text: &str) -> Value {
    json!({"generation":generation, "session":fixture()["session"], "key":key, "text":text})
}

fn answer(request: u64, option: &str) -> Value {
    json!({"generation":GENERATION, "request":request, "optionId":option})
}

fn pending_for<'a>(pending: &'a Value, native_id: Value) -> (u64, &'a Value) {
    pending.as_object().unwrap().iter()
        .find(|(_, entry)| entry["native"]["id"] == native_id)
        .map(|(k, v)| (k.parse().unwrap(), v))
        .unwrap_or_else(|| panic!("no pending request with native id {native_id}"))
}

fn allows(entry: &Value) -> bool {
    entry["acp"]["params"]["options"].as_array().unwrap().iter()
        .any(|option| option["optionId"] == "allow")
}

fn validate_frames(entries: &[Value]) {
    let schema: Value = serde_json::from_str(include_str!("../src/runtime/acp/__fixtures__/acp-v1-schema.json")).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let schema_uri = "https://example.invalid/acp-v1-schema.json";
    let registry = jsonschema::Registry::new().add(schema_uri, schema).unwrap().prepare().unwrap();
    let mut definitions = HashMap::new();
    let mut calls = HashMap::new();
    let messages: Vec<_> = entries.iter().filter(|line| line["kind"] == "acp").collect();
    for direction in ["in", "out"] {
        assert!(messages.iter().any(|line| line["dir"] == direction));
    }
    for line in messages {
        let msg = &line["msg"];
        assert_eq!(msg["jsonrpc"], "2.0");
        assert!(validator.validate(msg).is_ok(), "#{seq}: {msg}: {err:?}", seq=line["seq"], err=validator.validate(msg));
        let direction = line["dir"].as_str().unwrap();
        let (name, body) = if let Some(method) = msg["method"].as_str() {
            let (request, response) = match (direction, method) {
                ("out", "initialize") => ("InitializeRequest", Some("InitializeResponse")),
                ("out", "session/new") => ("NewSessionRequest", Some("NewSessionResponse")),
                ("out", "session/prompt") => ("PromptRequest", Some("PromptResponse")),
                ("out", "session/cancel") => ("CancelNotification", None),
                ("out", "session/close") => ("CloseSessionRequest", Some("CloseSessionResponse")),
                ("in", "session/update") => ("SessionNotification", None),
                ("in", "session/request_permission") => ("RequestPermissionRequest", Some("RequestPermissionResponse")),
                _ => panic!("unexpected ACP method {method} in direction {direction}"),
            };
            if let Some(response) = response {
                let reply_direction = if direction == "in" { "out" } else { "in" };
                calls.insert((reply_direction, msg["id"].clone()), response);
            }
            (request, &msg["params"])
        } else {
            let response = calls.remove(&(direction, msg["id"].clone())).expect("matching ACP request");
            if msg.get("error").is_some() { ("Error", &msg["error"]) } else { (response, &msg["result"]) }
        };
        let validator = definitions.entry(name).or_insert_with(|| {
            jsonschema::options().with_registry(&registry)
                .build(&json!({"$ref": format!("{schema_uri}#/$defs/{name}")})).unwrap()
        });
        assert!(validator.validate(body).is_ok(), "#{seq}: {body}: {err:?}", seq=line["seq"], err=validator.validate(body));
    }
}

fn native_outcome(host: &Host, id: Value, expected: Value, schema: &str) {
    let schema: Value = serde_json::from_str(schema).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let matches: Vec<_> = host.native().into_iter()
        .filter(|line| line["id"] == id && line["result"] == expected).collect();
    assert_eq!(matches.len(), 1, "expected a single native {expected} reply to {id}");
    validator.validate(&matches[0]["result"]).unwrap();
}

const COMMAND_RESPONSE: &str = include_str!("../src/runtime/acp/codex/__fixtures__/CommandExecutionRequestApprovalResponse.json");
const FILE_RESPONSE: &str = include_str!("../src/runtime/acp/codex/__fixtures__/FileChangeRequestApprovalResponse.json");
const PERMISSIONS_RESPONSE: &str = include_str!("../src/runtime/acp/codex/__fixtures__/PermissionsRequestApprovalResponse.json");

#[test]
fn streams_acp_and_settles_each_typed_native_approval_once() {
    let mut host = Host::spawn(json!({}));
    let (mut wire, _) = host.ready();
    let expected = fixture();
    let receipt = wire.result("prompt", prompt(GENERATION, "conversation", expected["prompt"].as_str().unwrap()));
    let first = wire.pendings(4);
    let (command, child) = pending_for(&first, json!("0"));
    let (file, patch) = pending_for(&first, json!(0));
    let (permissions, permissions_entry) = pending_for(&first, json!("permission"));
    let (allow, _) = pending_for(&first, json!("complete"));
    assert_ne!(command, file);
    assert!(allows(child) && allows(patch));
    assert!(!allows(permissions_entry));
    let details = &child["acp"]["params"]["toolCall"]["rawInput"];
    let expected_details = json!({
        "command":expected["command"], "cwd":host.root().join("cwd").canonicalize().unwrap(),
        "reason":"review this command", "networkApprovalContext":{"host":"example.invalid","protocol":"https"},
    });
    for (key, expected_value) in expected_details.as_object().unwrap() {
        assert_eq!(&details[key], expected_value, "the visible command approval must include {key}");
    }
    assert_eq!(patch["acp"]["params"]["toolCall"]["rawInput"]["changes"][0]["diff"], "-old\n+new\n");
    assert_eq!(child["acp"]["params"]["_meta"]["workriver"]["source"]["thread"], expected["subagent"]);
    assert_eq!(child["acp"]["params"]["sessionId"], expected["session"]);
    wire.refuses("prompt", prompt(GENERATION, "blocked", "quiet"), "blocked");

    // The second of the simultaneous requests can be rejected first.
    wire.result("answer", answer(file, "reject"));
    let after_file = wire.pendings(3);
    assert!(after_file.get(file.to_string()).is_none());
    assert_eq!(pending_for(&after_file, json!("0")).0, command);
    wire.refuses("answer", answer(file, "reject"), "settled");
    wire.result("answer", answer(command, "reject"));
    wire.result("answer", answer(permissions, "reject"));
    wire.result("answer", answer(allow, "allow"));

    let second = until("the next approval wave after native id reuse", || {
        let status = wire.result("status", json!({}));
        let mut entries = status["pending"].as_object().unwrap().values();
        entries.any(|p| p["native"]["id"] == "root-grant").then(|| status["pending"].clone())
    });
    let (reused, _) = pending_for(&second, json!("0"));
    assert!(reused > command, "native id reuse must get a fresh journal sequence");
    wire.refuses("answer", answer(command, "reject"), "settled");
    let second = wire.pendings(3);
    for id in ["0", "root-grant", "cancel-permission"] {
        let (key, entry) = pending_for(&second, json!(id));
        assert!(!allows(entry), "incomplete {id} allowed");
        let response = wire.call("answer", answer(key, "allow"));
        assert!(response.get("error").is_some(), "incomplete {id} accepted");
    }

    wire.result("cancel", json!({"generation":GENERATION}));
    assert!(wire.pendings(0).as_object().unwrap().is_empty());
    native_outcome(&host, json!(0), json!({"decision":"decline"}), FILE_RESPONSE);
    native_outcome(&host, json!("0"), json!({"decision":"decline"}), COMMAND_RESPONSE);
    native_outcome(&host, json!("0"), json!({"decision":"cancel"}), COMMAND_RESPONSE);
    native_outcome(&host, json!("root-grant"), json!({"decision":"cancel"}), FILE_RESPONSE);
    native_outcome(&host, json!("complete"), json!({"decision":"accept"}), COMMAND_RESPONSE);
    for id in ["permission", "cancel-permission"] {
        native_outcome(&host, json!(id), json!({"permissions":{},"scope":"turn"}), PERMISSIONS_RESPONSE);
    }
    assert!(host.native().iter().any(|message| message["id"] == "unknown-request" && message["error"]["code"] == -32601));

    let page = wire.result("read", json!({"after":0,"limit":1000}));
    let entries = page["entries"].as_array().unwrap();
    validate_frames(entries);
    let updates: Vec<_> = entries.iter().filter(|line| line["msg"]["method"] == "session/update")
        .map(|line| (&line["seq"], &line["msg"]["params"]["update"])).collect();
    assert_eq!(updates[0].1["content"]["text"], expected["earlyText"]);
    assert!(updates[0].0.as_u64().unwrap() > receipt["seq"].as_u64().unwrap());
    assert!(updates.iter().any(|(_, update)| update["content"]["text"] == expected["thought"]));
    assert!(updates.iter().any(|(_, update)| update["entries"][0]["content"] == expected["plan"]));
    let usage: Vec<_> = updates.iter().filter(|(_, u)| u["sessionUpdate"] == "usage_update").collect();
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].1["used"], expected["usage"]["used"]);
    assert_eq!(usage[0].1["size"], expected["usage"]["size"]);
    assert_eq!(entries.iter().filter(|e| e["event"] == "usage").count(), 2);
    let statuses: Vec<_> = updates.iter().filter(|(_, u)| u["compactionId"] == expected["compaction"])
        .map(|(_, u)| u["status"].as_str().unwrap()).collect();
    assert_eq!(statuses, ["in_progress", "completed"]);
    assert!(updates.iter().any(|(_, update)| update["sessionUpdate"] == "tool_call"
        && update["_meta"]["workriver"]["source"]["thread"] == expected["subagent"]));
    assert!(entries.iter().any(|e| e["event"] == "settled" && e["by"] == "agent"));
    for request in [file, command, permissions, allow] {
        let decided = entries.iter().find(|e| e["event"] == "decided" && e["request"] == request).unwrap();
        let replied = entries.iter().find(|e| e["event"] == "replied" && e["request"] == request).unwrap();
        assert!(decided["seq"].as_u64() < replied["seq"].as_u64());
    }

    wire.result("close", json!({"generation":GENERATION}));
    host.ended();
    let entries = host.journal();
    validate_frames(&entries);
    assert!(entries.iter().any(|e| e["dir"] == "in" && e["msg"]["id"] == 4 && e["msg"]["result"] == json!({})));
}

#[test]
fn a_lost_prompt_receipt_and_host_restart_keep_the_original_delivery_sequence() {
    let mut host = Host::spawn(json!({}));
    let (mut wire, _) = host.ready();
    wire.refuses("prompt", prompt(GENERATION - 1, "unused", "quiet"), "moved");
    wire.send("prompt", prompt(GENERATION, "durable", "quiet"));
    drop(wire);
    until("the adapter gets the prompt whose response was lost", || host.native().iter()
        .any(|message| message["method"] == "turn/start").then_some(()));
    let mut wire = host.connect();
    assert_eq!(wire.hello(1, "8d".repeat(32))["result"]["state"], "running");
    assert_eq!(wire.result("status", json!({}))["state"], "working");
    wire.refuses("prompt", prompt(GENERATION, "competing", "quiet"), "busy");
    let repeat = wire.result("prompt", prompt(GENERATION, "durable", "quiet"));
    assert_eq!(repeat["repeat"], true);
    let seq = repeat["seq"].as_u64().unwrap();
    assert!(host.journal().iter().any(|entry| entry["seq"] == seq && entry["msg"]["method"] == "session/prompt"));
    assert_eq!(host.native().iter().filter(|m| m["method"] == "turn/start").count(), 1);

    wire.result("cancel", json!({"generation":GENERATION}));
    let conversation = fixture()["prompt"].as_str().unwrap().to_owned();
    wire.result("prompt", prompt(GENERATION, "old-approval", &conversation));
    let old_key = pending_for(&wire.pendings(4), json!(0)).0;
    host.restart(GENERATION + 1, json!({}));
    let (mut wire, hello) = host.ready();
    assert_eq!(hello["generation"], GENERATION + 1);
    let retry = wire.result("prompt", prompt(GENERATION, "durable", "quiet"));
    assert_eq!(retry["repeat"], true);
    assert_eq!(retry["seq"], seq);
    wire.result("prompt", prompt(GENERATION + 1, "new", &conversation));
    let new_key = pending_for(&wire.pendings(4), json!(0)).0;
    assert!(new_key > old_key);
    wire.refuses("answer", answer(old_key, "reject"), "gone");
    wire.refuses("answer", json!({"generation":GENERATION+1,"request":old_key,"optionId":"reject"}), "settled");
    until("one new prompt and two previous prompts on the native pipe", || {
        let count = host.native().iter().filter(|m| m["method"] == "turn/start").count();
        (count == 3).then_some(())
    });
    wire.result("close", json!({"generation":GENERATION+1}));
    host.ended();
}

#[test]
fn a_native_pipe_failure_still_leaves_the_original_durable_prompt_receipt() {
    let mut host = Host::spawn(json!({"retire_stdin":true}));
    let (mut wire, _) = host.ready();
    until("the native app-server discards its read pipe", || host.root().join("cwd/stdin-retired").exists().then_some(()));
    wire.send("prompt", prompt(GENERATION, "accepted-before-pipe", "finish"));
    drop(wire);
    host.ended();
    assert!(!host.native().iter().any(|m| m["method"] == "turn/start"));
    let seq = host.journal().iter().find(|e| e["key"] == "accepted-before-pipe")
        .unwrap()["seq"].as_u64().unwrap();

    host.restart(GENERATION + 1, json!({}));
    let (mut wire, _) = host.ready();
    let retry = wire.result("prompt", prompt(GENERATION, "accepted-before-pipe", "finish"));
    assert_eq!(retry, json!({"seq":seq,"repeat":true}));
    wire.result("close", json!({"generation":GENERATION+1}));
    host.ended();
    assert!(!host.native().iter().any(|m| m["method"] == "turn/start"));
}

#[test]
fn status_and_read_enforce_encoded_byte_budgets_and_cut_approvals_refuse_allow() {
    let mut host = Host::spawn(json!({}));
    let (mut wire, _) = host.ready();
    wire.result("prompt", prompt(GENERATION, "large", "large"));
    until("all oversized approval frames have arrived", || host.journal().iter()
        .any(|e| e["msg"]["params"]["update"]["content"]["text"] == "end of large fixture").then_some(()));
    wire.send("status", json!({}));
    let (reply, status_bytes) = wire.receive();
    assert!(status_bytes <= 512 * 1024, "encoded status response is {status_bytes} bytes");
    let pending = &reply["result"]["pending"];
    assert!(pending.as_object().unwrap().len() < 33, "overflow must not leave an invisible live request");
    assert!(host.journal().iter().any(|e| e["event"] == "settled" && e["by"] == "budget"));
    let (key, entry) = pending_for(pending, json!("large"));
    assert!(entry["cut"].as_u64().unwrap() > 0);
    assert!(serde_json::to_vec(entry).unwrap().len() <= 24 * 1024);
    assert!(!allows(entry));
    assert!(wire.call("answer", answer(key, "allow")).get("error").is_some());

    let mut after = 0;
    let mut pages = 0;
    let mut found = false;
    loop {
        wire.send("read", json!({"after":after,"limit":1000}));
        let (reply, bytes) = wire.receive();
        assert!(bytes <= 2 * 1024 * 1024, "encoded journal page is {bytes} bytes");
        let page = &reply["result"];
        let head = page["head"].as_u64().unwrap();
        for entry in page["entries"].as_array().unwrap() {
            let seq = entry["seq"].as_u64().unwrap();
            assert_eq!(seq, after + 1, "pages must preserve every cursor");
            after = seq;
            if seq == key {
                let encoded = serde_json::to_vec(entry).unwrap().len();
                assert!(encoded > 1024 * 1024 && entry.get("cut").is_none(), "the journal approval retained only {encoded} bytes");
                found = true;
            }
        }
        pages += 1;
        if after == head {
            break;
        }
        assert!(pages < 10, "paged read failed to progress");
    }
    assert!(found && pages > 1);
    wire.result("close", json!({"generation":GENERATION}));
    host.ended();
}

#[test]
fn endpoint_overrides_are_refused_before_thread_start_and_billing_is_rechecked() {
    for (key, value) in [
        ("openai_base_url", json!("http://127.0.0.1:9/v1")),
        ("chatgpt_base_url", json!("http://127.0.0.1:9/")),
        ("model_providers", json!({"custom":{"base_url":"http://127.0.0.1:9/"}})),
        ("model_provider", json!("custom")),
    ] {
        let mut host = Host::spawn(json!({"config":{
            "config":{"model_provider":"openai"}, "origins":{}, "layers":[{
                "name":{"type":"user","file":"/fixture/config.toml"}, "version":"fixture","config":{(key):value},
            }],
        }}));
        host.ended();
        let messages = host.native();
        assert!(messages.iter().any(|m| m["method"] == "config/read" && m["params"]["includeLayers"] == true));
        assert!(!messages.iter().any(|m| m["method"] == "thread/start"), "{key} reached thread/start");
        assert!(host.log().contains(key), "diagnostic omits {key}: {}", host.log());
    }
    for options in [json!({"provider":"other"}), json!({"account":{"type":"apiKey"}})] {
        let mut host = Host::spawn(options);
        host.ended();
        assert!(host.native().iter().any(|m| m["method"] == "thread/start"));
        assert!(!host.journal().iter().any(|e| e["event"] == "started"), "invalid billing started a session");
    }
    let mut host = Host::spawn(json!({}));
    let (mut wire, hello) = host.ready();
    assert!(!hello.to_string().contains("private@example.invalid"));
    let account = host.root().join("cwd/account");
    fs::write(&account, "apiKey").unwrap();
    wait_account_reads(&host, 2);
    wire.refuses("prompt", prompt(GENERATION, "unverified", "finish"), "billing");
    fs::write(&account, "chatgpt").unwrap();
    until("billing recheck clears on the originally selected account", || {
        let response = wire.call("prompt", prompt(GENERATION, "reverified", "finish"));
        (response.get("result").is_some()).then_some(())
    });
    wait_account_reads(&host, 3);
    assert!(!fs::read_to_string(host.dir.join("journal.jsonl")).unwrap().contains("private@example.invalid"));
    wire.result("close", json!({"generation":GENERATION}));
    host.ended();
}

fn wait_account_reads(host: &Host, expected: usize) {
    until("the native account checks", || {
        let count = host.native().iter().filter(|m| m["method"] == "account/read").count();
        (count == expected).then_some(())
    });
}

#[test]
fn close_is_immediately_visible_and_reaps_a_leader_that_ignores_sigterm() {
    let mut host = Host::spawn(json!({"hold_unsubscribe":true, "ignore_term":true}));
    let (mut wire, _) = host.ready();
    let instance: Value = serde_json::from_slice(&fs::read(host.dir.join("instance.json")).unwrap()).unwrap();
    let leader = instance["agentGroup"].as_i64().unwrap() as libc::pid_t;
    assert_ne!(leader, host.child.id() as libc::pid_t);
    assert_eq!(unsafe { libc::getpgid(leader) }, leader);
    assert_eq!(wire.result("close", json!({"generation":GENERATION}))["accepted"], true);
    assert_eq!(wire.result("status", json!({}))["state"], "closing");
    let late = wire.call("prompt", prompt(GENERATION, "too-late", "finish"));
    assert!(late.get("error").is_some());
    assert!(wire.call("answer", answer(1, "reject")).get("error").is_some());
    fs::write(host.root().join("cwd/close-release"), "").unwrap();
    host.ended();
    let entries = host.journal();
    let closed: Vec<_> = entries.iter().filter(|e| e["event"] == "closed").collect();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0]["outcome"], "ended", "{closed:?}");
    assert_eq!(unsafe { libc::kill(leader, 0) }, -1, "the terminated leader must be reaped");
}

#[test]
fn close_keeps_the_exited_leader_unreaped_until_its_other_group_member_dies() {
    let mut host = Host::spawn(json!({"lingering_member":true}));
    let (mut wire, _) = host.ready();
    let path = host.root().join("cwd/member-ready");
    let member: libc::pid_t = until("the descendant starts and ignores SIGTERM", || {
        fs::read_to_string(&path).ok()?.parse().ok()
    });
    let instance: Value = serde_json::from_slice(&fs::read(host.dir.join("instance.json")).unwrap()).unwrap();
    let leader = instance["agentGroup"].as_i64().unwrap() as libc::pid_t;
    assert_ne!(member, leader);
    assert_eq!(unsafe { libc::getpgid(member) }, leader);

    wire.result("close", json!({"generation":GENERATION}));
    until("the leader has exited but is still unreaped while its descendant is live", || {
        (process_state(leader).is_some_and(|state| state.starts_with('Z'))
            && process_state(member).is_some_and(|state| !state.starts_with('Z'))).then_some(())
    });
    assert_eq!(wire.result("status", json!({}))["state"], "closing");
    host.ended();
    assert!(host.journal().iter().any(|e| e["event"] == "closed" && e["outcome"] == "ended"));
    assert!(process_state(leader).is_none(), "the host must reap the leader after group cleanup");
    assert!(process_state(member).is_none_or(|state| state.starts_with('Z')), "the group member survived close");
}

#[test]
fn close_drains_final_native_output_before_its_receipt() {
    let mut host = Host::spawn(json!({"closing_burst":128}));
    let (mut wire, _) = host.ready();
    wire.result("close", json!({"generation":GENERATION}));
    host.ended();
    let entries = host.journal();
    let closed = entries.last().unwrap();
    assert_eq!(closed["event"], "closed", "{}", host.log());
    assert_eq!(closed["outcome"], "ended");
    let observed = entries.iter().filter(|e| e["msg"]["params"]["update"]["content"]["text"]
        .as_str().is_some_and(|line| line.starts_with("closing delta "))).count();
    assert_eq!(observed, 128, "buffered updates must precede the only close receipt");
}

fn process_state(pid: libc::pid_t) -> Option<String> {
    let ps = Command::new("/bin/ps").env_clear().args(["-o", "stat=", "-p", &pid.to_string()]).output().unwrap();
    ps.status.success().then(|| String::from_utf8(ps.stdout).unwrap().trim().to_owned())
}
