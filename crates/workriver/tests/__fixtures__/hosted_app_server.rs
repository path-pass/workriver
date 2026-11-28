use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufWriter, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

#[test]
#[ignore = "launched on inherited pipes by the hosted integration tests"]
fn fake_codex_app_server() {
    if std::env::var_os("HOSTED_TEST_PIPE").is_none() {
        return;
    }
    fs::write("environment.json", serde_json::to_vec(&std::env::vars().collect::<std::collections::BTreeMap<_, _>>()).unwrap()).unwrap();
    let sent = BufWriter::new(OpenOptions::new().write(true).open("/dev/fd/3").unwrap());
    // SAFETY: the shim hands this process fd 3; the newly opened copy owns the wire.
    unsafe { libc::close(3); }
    let received = OpenOptions::new().create(true).append(true).open("native.jsonl").unwrap();
    let options = serde_json::from_str(&fs::read_to_string("options.json").unwrap()).unwrap();
    let mut fake = Fake {
        sent, received, options, fixture: super::fixture(),
        account: "chatgpt".into(), turn: String::new(), count: 0, first_approvals: 0, pending: HashMap::new(),
    };
    if let Some(path) = fake.options["probe_socket"].as_str() {
        let probe = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "fake::probe_host_socket", "--ignored"])
            .env("HOSTED_TEST_PROBE_SOCKET", path)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap();
        fs::write("probe-result", if probe.success() { "refused" } else { "open" }).unwrap();
    }
    if fake.options["ignore_term"] == true {
        // The real workriver host must terminate and reap this uniquely owned fake.
        unsafe { libc::signal(libc::SIGTERM, libc::SIG_IGN); }
    }
    if fake.options["lingering_member"] == true {
        let member = Command::new("/bin/sh")
            .args(["-c", "trap '' TERM; printf '%s' \"$$\" > member-ready; exec /bin/sleep 17"])
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
        // Drop without reaping: the fixture proves the group's lifetime can outlast the leader.
        drop(member);
    }
    if fake.options["retire_stdin"] == true {
        for line in io::stdin().lock().lines() {
            let frame: Value = serde_json::from_str(&line.unwrap()).unwrap();
            let ready = frame["method"] == "account/read";
            fake.receive(frame);
            if ready {
                break;
            }
        }
        // SAFETY: this fake process deliberately retires its own read pipe.
        unsafe { libc::close(libc::STDIN_FILENO); }
        fs::write("stdin-retired", "").unwrap();
        thread::sleep(Duration::from_secs(8));
        return;
    }

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let frame: Value = serde_json::from_str(&line.unwrap()).unwrap();
            if tx.send(frame).is_err() {
                break;
            }
        }
    });
    loop {
        match rx.recv_timeout(Duration::from_millis(25)) {
            Ok(frame) => fake.receive(frame),
            Err(RecvTimeoutError::Timeout) => fake.check_account(),
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    if let Some(count) = fake.options["closing_burst"].as_u64() {
        for index in 0..count {
            writeln!(fake.sent, "{}", json!({"method":"item/agentMessage/delta","params":{
                "threadId":fake.fixture["session"], "delta":format!("closing delta {index}")
            }})).unwrap();
        }
        fake.sent.flush().unwrap();
        std::process::exit(0);
    }
    if fake.options["ignore_term"] == true {
        // Escape timeout if the test itself crashes before closing the fake.
        thread::sleep(Duration::from_secs(17));
    }
}

#[test]
#[ignore = "launched as the fake app-server's child"]
fn probe_host_socket() {
    use std::io::Read;
    use std::os::unix::net::UnixStream;

    let Some(path) = std::env::var_os("HOSTED_TEST_PROBE_SOCKET") else { return };
    let mut socket = UnixStream::connect(path).unwrap();
    socket.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
    assert_eq!(socket.read(&mut [0]).unwrap(), 0, "the host must close without waiting for hello");
}

struct Fake {
    sent: BufWriter<File>,
    received: File,
    options: Value,
    fixture: Value,
    account: String,
    turn: String,
    count: usize,
    first_approvals: usize,
    pending: HashMap<Value, Value>,
}

impl Fake {
    fn emit(&mut self, frame: Value) {
        writeln!(self.sent, "{frame}").unwrap();
        self.sent.flush().unwrap();
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.emit(json!({"method":method, "params":params}));
    }

    fn reply(&mut self, id: &Value, result: Value) {
        self.emit(json!({"id":id, "result":result}));
    }

    fn check_account(&mut self) {
        let Ok(account) = fs::read_to_string("account") else { return };
        if !account.is_empty() && account != self.account {
            self.account = account;
            let mode = if self.account == "chatgpt" { "chatgpt" } else { "apikey" };
            self.notify("account/updated", json!({"authMode":mode}));
        }
    }

    fn receive(&mut self, frame: Value) {
        writeln!(self.received, "{frame}").unwrap();
        let id = &frame["id"];
        let params = &frame["params"];
        let result = match frame["method"].as_str() {
            Some("initialize") => {
                assert_ne!(params["capabilities"]["experimentalApi"], true);
                json!({"userAgent":"fake-codex/0.172.0"})
            }
            Some("config/read") => self.options.get("config").cloned().unwrap_or_else(|| json!({
                "config": {"model_provider":"openai"}, "origins": {}, "layers": [],
            })),
            Some("thread/start") => {
                assert_eq!(params["model"], "requested-model");
                for field in ["approvalPolicy", "approvalsReviewer", "sandbox"] {
                    assert!(params.get(field).is_none(), "host overrides {field}");
                }
                json!({
                    "thread":{"id":self.fixture["session"]}, "model":"effective-model",
                    "modelProvider":self.options["provider"].as_str().unwrap_or("openai"),
                    "approvalPolicy":"on-request", "approvalsReviewer":"user",
                    "sandbox":{"type":"readOnly","access":{"type":"fullAccess"}},
                })
            }
            Some("account/read") => {
                let account = self.options.get("account").cloned().unwrap_or_else(|| match self.account.as_str() {
                    "chatgpt" => json!({"type":"chatgpt", "planType":"plus", "email":"private@example.invalid"}),
                    _ => json!({"type":"apiKey"}),
                });
                json!({"account":account, "requiresOpenaiAuth":true})
            }
            Some("thread/unsubscribe") => {
                if self.options["hold_unsubscribe"] == true {
                    let until = Instant::now() + Duration::from_secs(6);
                    while !std::path::Path::new("close-release").exists() && Instant::now() < until {
                        thread::sleep(Duration::from_millis(15));
                    }
                }
                json!({"status":"unsubscribed"})
            }
            Some("initialized") => return,
            Some("turn/start") => {
                self.prompt(id, params["input"][0]["text"].as_str().unwrap());
                return;
            }
            Some("turn/interrupt") => {
                assert_eq!(params["turnId"], self.turn);
                assert!(self.pending.is_empty(), "the native interrupt passed pending approvals");
                self.reply(id, json!({}));
                self.complete("interrupted");
                return;
            }
            None if frame.get("result").is_some() || frame.get("error").is_some() => {
                if let Some(thread) = self.pending.remove(id) {
                    self.notify("serverRequest/resolved", json!({"threadId":thread, "requestId":id}));
                    if frame["result"]["decision"] == "cancel" {
                        self.first_approvals = 0;
                    }
                    if self.first_approvals != 0 {
                        self.first_approvals -= 1;
                        if self.first_approvals == 0 {
                            self.second_approvals();
                        }
                    }
                } else {
                    assert_eq!(id, "unknown-request");
                    assert_eq!(frame["error"]["code"], -32601);
                }
                return;
            }
            other => panic!("unexpected native frame {other:?}: {frame}"),
        };
        self.reply(id, result);
    }

    fn prompt(&mut self, id: &Value, text: &str) {
        self.count += 1;
        self.turn = format!("turn-{}", self.count);
        if text == "bad-ack" {
            self.reply(id, json!({"turn":{"status":"inProgress"}}));
            return;
        }
        if text == self.fixture["prompt"] {
            self.notify("item/agentMessage/delta", json!({
                "threadId":self.fixture["session"], "turnId":self.turn, "itemId":"agent-message",
                "delta":self.fixture["earlyText"],
            }));
        }
        self.reply(id, json!({"turn":{"id":self.turn, "status":"inProgress", "items":[]}}));
        if text == self.fixture["prompt"] {
            self.conversation();
        } else if text == "large" {
            self.large();
        } else if text == "finish" {
            self.complete("completed");
        } else if text == "bad-optional" {
            writeln!(self.sent, "{{malformed").unwrap();
            self.sent.flush().unwrap();
            self.notify("item/agentMessage/delta", json!({
                "threadId":self.fixture["session"], "delta":"a".repeat(9 * 1024 * 1024),
            }));
            self.notify("turn/plan/updated", json!({
                "threadId":self.fixture["session"], "plan":[{"step":"Check","status":"skipped"}],
            }));
            self.notify("item/agentMessage/delta", json!({"threadId":self.fixture["session"]}));
            let session = self.fixture["session"].clone();
            self.pending.insert(json!("malformed"), session.clone());
            self.emit(json!({"id":"malformed","method":"item/commandExecution/requestApproval","params":{
                "threadId":session, "turnId":self.turn, "command":"pwd",
            }}));
            self.notify("item/agentMessage/delta", json!({
                "threadId":self.fixture["session"], "delta":"after bad frames",
            }));
            self.complete("completed");
        } else if text == "bad-terminal" {
            let session = self.fixture["session"].clone();
            self.request("commandExecution", json!("failed-turn"), &session, "tool", json!({"command":"pwd"}));
            self.complete("some-future-terminal-status");
        } else if text == "duplicate-approval" {
            let session = self.fixture["session"].clone();
            self.request("commandExecution", json!("same"), &session, "first", json!({"command":"pwd"}));
            self.emit(json!({"id":"same","method":"item/commandExecution/requestApproval","params":{
                "threadId":session, "turnId":self.turn, "itemId":"second", "command":"pwd",
            }}));
            self.notify("item/agentMessage/delta", json!({"threadId":session, "delta":"past duplicate"}));
        } else if text == "diff" {
            for diff in ["first version", "second version"] {
                self.notify("turn/diff/updated", json!({
                    "threadId":self.fixture["session"], "turnId":self.turn, "diff":diff,
                }));
            }
            self.notify("item/agentMessage/delta", json!({
                "threadId":self.fixture["session"], "delta":"diffs sent",
            }));
        } else {
            assert_eq!(text, "quiet");
        }
    }

    fn complete(&mut self, status: &str) {
        self.notify("turn/completed", json!({
            "threadId":self.fixture["session"], "turn":{"id":self.turn, "status":status, "items":[]},
        }));
    }

    fn item(&mut self, stage: &str, thread: &Value, item: Value) {
        self.notify(&format!("item/{stage}"), json!({
            "threadId":thread, "turnId":self.turn, "item":item, "startedAtMs":0, "completedAtMs":1,
        }));
    }

    fn request(&mut self, kind: &str, id: Value, thread: &Value, item: &str, extra: Value) {
        let mut params = json!({"threadId":thread, "turnId":self.turn, "itemId":item, "startedAtMs":1});
        params.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        assert!(self.pending.insert(id.clone(), thread.clone()).is_none());
        self.emit(json!({"id":id, "method":format!("item/{kind}/requestApproval"), "params":params}));
    }

    fn conversation(&mut self) {
        let root = self.fixture["session"].clone();
        let child = self.fixture["subagent"].clone();
        self.notify("thread/started", json!({"thread":{
            "id": child, "parentThreadId":root, "agentNickname":"Checker",
            "source":{"subAgent":{"thread_spawn":{
                "parent_thread_id":root, "depth":1, "agent_nickname":"Checker", "agent_role":"reviewer",
            }}},
        }}));
        self.notify("item/reasoning/summaryTextDelta", json!({
            "threadId":root, "turnId":self.turn, "itemId":"thought", "delta":self.fixture["thought"], "summaryIndex":0,
        }));
        self.notify("turn/plan/updated", json!({
            "threadId":root, "turnId":self.turn, "plan":[{"step":self.fixture["plan"],"status":"pending"}],
        }));
        let usage = self.fixture["usage"].clone();
        for window in [usage["size"].clone(), Value::Null] {
            let tokens = json!({
                "totalTokens":usage["used"], "inputTokens":10, "outputTokens":16,
                "cachedInputTokens":4, "reasoningOutputTokens":5,
            });
            self.notify("thread/tokenUsage/updated", json!({
                "threadId":root, "turnId":self.turn,
                "tokenUsage":{"modelContextWindow":window, "last":tokens, "total":tokens},
            }));
        }
        for stage in ["started", "completed"] {
            self.item(stage, &root, json!({"type":"contextCompaction","id":self.fixture["compaction"]}));
        }
        let cwd = std::env::current_dir().unwrap();
        let command = self.fixture["command"].clone();
        for (stage, state) in [("started", "inProgress"), ("completed", "completed")] {
            self.item(stage, &child, json!({
                "type":"commandExecution","id":"child-command","command":command,
                "cwd":cwd,"commandActions":[],"status":state,
            }));
        }
        let changes = json!([{"path":cwd.join("file"),"kind":{"type":"update"},"diff":"-old\n+new\n"}]);
        self.item("started", &root, json!({"type":"fileChange","id":"patch","status":"inProgress","changes":changes}));
        self.first_approvals = 4;
        self.request("commandExecution", json!("0"), &child, "child-command", json!({
            "command":command,"cwd":cwd,"reason":"review this command",
            "networkApprovalContext":{"host":"example.invalid","protocol":"https"},
        }));
        self.request("fileChange", json!(0), &root, "patch", json!({"reason":"review this patch"}));
        self.request("permissions", json!("permission"), &root, "grant", json!({"cwd":cwd,"permissions":{}}));
        self.request("commandExecution", json!("complete"), &root, "complete", json!({"command":command,"cwd":cwd}));
        self.emit(json!({"id":"unknown-request","method":"future/unsupported","params":{}}));
    }

    fn second_approvals(&mut self) {
        let root = self.fixture["session"].clone();
        let cwd = std::env::current_dir().unwrap();
        self.request("commandExecution", json!("0"), &root, "stdin", json!({"cwd":cwd}));
        self.request("fileChange", json!("root-grant"), &root, "patch", json!({"grantRoot":cwd}));
        self.request("permissions", json!("cancel-permission"), &root, "grant", json!({"cwd":cwd,"permissions":{}}));
        let external = json!("external");
        self.request("commandExecution", external.clone(), &root, "external", json!({"command":"true","cwd":cwd}));
        self.pending.remove(&external).unwrap();
        self.notify("serverRequest/resolved", json!({"threadId":root,"requestId":external}));
    }

    fn large(&mut self) {
        let root = self.fixture["session"].clone();
        let cwd = std::env::current_dir().unwrap();
        self.request("commandExecution", json!("large"), &root, "large", json!({"command":"x".repeat(1024 * 1024),"cwd":cwd}));
        // Escaped control bytes increase the encoded size of an ordinary ACP frame.
        for _ in 0..24 {
            self.notify("item/agentMessage/delta", json!({
                "threadId":root, "turnId":self.turn, "itemId":"many", "delta":"長\n".repeat(32 * 1024),
            }));
        }
        for index in 0..32 {
            self.request("commandExecution", json!(format!("bulk-{index}")), &root, "bulk", json!({
                "command":"yz".repeat(5 * 1024),"cwd":cwd,
            }));
        }
        self.notify("item/agentMessage/delta", json!({
            "threadId":root,"turnId":self.turn,"itemId":"tail","delta":"end of large fixture",
        }));
    }
}
