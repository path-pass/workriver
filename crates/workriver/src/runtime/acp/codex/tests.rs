use super::*;

fn answer(request: &Value, result: Value) -> Value {
    json!({"id": request["id"], "result": result})
}

fn receive(adapter: &mut Adapter, method: &str, params: Value) -> Vec<Event> {
    adapter.receive(json!({"method": method, "params": params})).unwrap()
}

fn send(events: &[Event], method: &str) -> Value {
    events.iter().find_map(|event| match event {
        Event::Send(frame) | Event::SessionNew(frame) | Event::Control(frame) if frame["method"] == method => Some(frame.clone()),
        _ => None,
    }).unwrap_or_else(|| panic!("missing send for {method}"))
}

fn acp(events: &[Event]) -> &Value {
    events.iter().find_map(|event| match event {
        Event::Acp(frame) => Some(frame),
        _ => None,
    }).expect("missing ACP frame")
}

fn to_config() -> (Adapter, Value) {
    let mut adapter = Adapter::new(Path::new("/project"), Some("chosen"), "chatgpt", "codex-cli 0.172.0\n").unwrap();
    let init = adapter.start();
    assert_eq!(init["method"], "initialize");
    assert!(init["params"].get("capabilities").is_none());
    let events = adapter.receive(answer(&init, json!({"userAgent": "Codex"}))).unwrap();
    assert_eq!(acp(&events)["result"]["protocolVersion"], 1);
    let versions = &acp(&events)["result"]["_meta"]["workriver"];
    assert_eq!(versions["harnessVersion"], versions["testedHarnessVersion"]);
    assert_eq!(send(&events, "initialized")["method"], "initialized");
    (adapter, send(&events, "config/read"))
}

fn config_result() -> Value {
    json!({
        "config": {
            "model_provider": "openai", "openai_base_url": null,
            "model_providers": {}, "chatgpt_base_url": "https://chatgpt.com/backend-api/"
        },
        "layers": [{"name":{"type":"user"},"config":{"model_provider":"openai"}}]
    })
}

fn thread_result(provider: &str) -> Value {
    json!({"thread": {"id": "root"}, "model": "selected", "modelProvider": provider,
        "approvalPolicy": "on-request", "approvalsReviewer":"user", "sandbox":{"type":"readOnly"}})
}

fn chatgpt() -> Value {
    json!({"account":{"type":"chatgpt","planType":"pro","email":"private@example.invalid"},
        "userId":"private","requiresOpenaiAuth":true})
}

fn ready() -> Adapter {
    let (mut adapter, config) = to_config();
    let next = adapter.receive(answer(&config, config_result())).unwrap();
    let start = send(&next, "thread/start");
    assert_eq!(start["params"], json!({"cwd":"/project","model":"chosen"}));
    let next = adapter.receive(answer(&start, thread_result("openai"))).unwrap();
    let account = send(&next, "account/read");
    assert_eq!(account["params"], json!({"refreshToken":false}));
    let next = adapter.receive(answer(&account, chatgpt())).unwrap();
    assert!(matches!(&next[0], Event::Billing(b) if b == &json!({
        "verified": true, "provider":"openai", "account":"chatgpt", "plan":"pro"
    })));
    assert!(matches!(&next[1], Event::Ready{session, meta} if session == "root"
        && meta["model"] == "selected" && meta["approvals"]["approvalPolicy"] == "on-request"));
    adapter
}

#[test]
fn real_defaults_pass_but_explicit_and_effective_overrides_refuse_start() {
    ready();
    for (key, val) in [
        ("openai_base_url", json!("http://127.0.0.1:9/v1")),
        ("chatgpt_base_url", json!("https://chatgpt.com/backend-api/")),
        ("model_providers", json!({})),
        ("model_provider", json!("gateway")),
    ] {
        let (mut adapter, config) = to_config();
        let mut data = config_result();
        data["layers"][0]["config"][key] = val;
        let err = adapter.receive(answer(&config, data)).err().unwrap().to_string();
        assert!(err.contains(key) && err.contains("user"), "{err}");
        assert!(!err.contains("127.0.0.1"), "{err}");
    }
    let (mut adapter, config) = to_config();
    let mut data = config_result();
    data["config"]["chatgpt_base_url"] = json!("https://example.invalid/api");
    let err = adapter.receive(answer(&config, data)).err().unwrap().to_string();
    assert!(err.contains("chatgpt_base_url") && err.contains("effective"), "{err}");
}

#[test]
fn startup_refuses_bad_provider_and_wrong_account_and_rechecks_the_latest_account() {
    let (mut adapter, config) = to_config();
    let start = send(&adapter.receive(answer(&config, config_result())).unwrap(), "thread/start");
    assert!(adapter.receive(answer(&start, thread_result("gateway"))).is_err());

    let (mut adapter, config) = to_config();
    let start = send(&adapter.receive(answer(&config, config_result())).unwrap(), "thread/start");
    let account = send(&adapter.receive(answer(&start, thread_result("openai"))).unwrap(), "account/read");
    assert!(adapter.receive(answer(&account, json!({"account":{"type":"apiKey"}}))).is_err());

    let mut adapter = ready();
    let first = receive(&mut adapter, "account/updated", json!({}));
    assert!(matches!(&first[0], Event::Billing(b) if b["verified"] == false));
    let second = receive(&mut adapter, "account/updated", json!({}));
    let first_id = send(&first, "account/read");
    let second_id = send(&second, "account/read");
    assert_ne!(first_id["id"], second_id["id"]);
    assert!(adapter.receive(answer(&first_id, chatgpt())).unwrap().is_empty());
    let mismatch = adapter.receive(answer(&second_id, json!({"account":{"type":"apiKey"}}))).unwrap();
    assert!(matches!(&mismatch[0], Event::Billing(b) if b["verified"] == false));
    let next = receive(&mut adapter, "account/updated", json!({}));
    let matching = adapter.receive(answer(&send(&next, "account/read"), chatgpt())).unwrap();
    assert!(matches!(&matching[0], Event::Billing(b) if b["verified"] == true));
    assert_eq!(matching.len(), 1);
}

#[test]
fn streams_before_turn_receipt_and_only_root_completion_ends_the_prompt() {
    let mut adapter = ready();
    let start = adapter.prompt("hello");
    assert_eq!(start["params"]["input"], json!([{"type":"text","text":"hello","text_elements":[]}]));
    let early = receive(&mut adapter, "item/agentMessage/delta", json!({"threadId":"root","delta":"early"}));
    assert_eq!(acp(&early)["params"]["update"]["content"]["text"], "early");
    assert!(adapter.interrupt().is_none());
    let returned = adapter.receive(answer(&start, json!({"turn":{"id":"turn"}}))).unwrap();
    assert_eq!(send(&returned, "turn/interrupt")["params"], json!({"threadId":"root","turnId":"turn"}));
    let child = receive(&mut adapter, "turn/completed", json!({"threadId":"child","turn":{"id":"turn","status":"completed"}}));
    assert!(child.iter().all(|event| !matches!(event, Event::TurnEnded(_))));
    let finished = receive(&mut adapter, "turn/completed", json!({"threadId":"root","turn":{"id":"turn","status":"interrupted"}}));
    assert!(matches!(&finished[0], Event::TurnEnded(frame) if frame["result"]["stopReason"] == "cancelled"));
    let close = adapter.close().unwrap();
    assert_eq!(close["method"], "thread/unsubscribe");
    let closed = adapter.receive(answer(&close, json!({"status":"unsubscribed"}))).unwrap();
    assert_eq!(acp(&closed)["result"], json!({}));
    assert!(matches!(closed.last(), Some(Event::Closed)));
}

#[test]
fn a_bad_early_completion_fails_only_the_turn_named_in_the_receipt() {
    for (late_id, should_fail) in [("previous-turn", false), ("next-turn", true)] {
        let mut adapter = ready();
        let start = adapter.prompt("hello");
        let result = receive(&mut adapter, "turn/completed", json!({
            "threadId":"root","turn":{"id":late_id,"status":"unknown"}
        }));
        assert!(matches!(result.as_slice(), [Event::Skipped { .. }]));
        let result = adapter.receive(answer(&start, json!({"turn":{"id":"next-turn"}}))).unwrap();
        if should_fail {
            assert!(matches!(result.as_slice(), [Event::TurnEnded(frame)] if frame["error"].is_object()));
        } else {
            assert!(result.is_empty());
            let end = receive(&mut adapter, "turn/completed", json!({
                "threadId":"root","turn":{"id":"next-turn","status":"completed"}
            }));
            assert!(matches!(end.as_slice(), [Event::TurnEnded(frame)] if frame["result"]["stopReason"] == "end_turn"));
        }
    }
}

#[test]
fn a_missing_bootstrap_result_fails_startup() {
    let (mut adapter, config) = to_config();
    let error = adapter.receive(json!({"id":config["id"]})).err().unwrap();
    assert_eq!(error.to_string(), "Codex config/read response has no result");
}

fn approval(adapter: &mut Adapter, method: &str, id: Value, params: Value) -> (Value, Value) {
    let frame = json!({"id": id, "method":method, "params": params});
    let events = adapter.receive(frame.clone()).unwrap();
    assert!(matches!(&events[0], Event::Approval { native, .. } if native == &frame));
    match events.into_iter().next().unwrap() {
        Event::Approval { native, acp } => (native, acp),
        _ => unreachable!(),
    }
}

#[test]
fn approvals_offer_only_complete_once_actions_and_preserve_typed_ids() {
    let mut adapter = ready();
    let (native, frame) = approval(&mut adapter, "item/commandExecution/requestApproval", json!("0"),
        json!({"threadId":"root","itemId":"command","command":"pwd","cwd":"/project","reason":"show the directory","networkApprovalContext":{"host":"example.invalid","protocol":"https"}}));
    assert_eq!(frame["id"], "0");
    assert_eq!(frame["params"]["options"][0]["kind"], "allow_once");
    assert_eq!(frame["params"]["toolCall"]["rawInput"]["networkApprovalContext"]["host"], "example.invalid");
    assert_eq!(adapter.reply(&native, Some("allow")), json!({"id":"0","result":{"decision":"accept"}}));
    assert_eq!(adapter.reply(&native, Some("reject"))["result"], json!({"decision":"decline"}));
    assert_eq!(adapter.reply(&native, None)["result"], json!({"decision":"cancel"}));

    let (_, frame) = approval(&mut adapter, "item/commandExecution/requestApproval", json!(0),
        json!({"threadId":"root","itemId":"command","kind":"writeStdin","command":null}));
    assert_eq!(frame["id"], 0);
    assert_eq!(frame["params"]["options"].as_array().unwrap().len(), 1);

    receive(&mut adapter, "item/started", json!({"threadId":"root","turnId":"one","item":{
        "type":"fileChange","id":"file","changes":[{"path":"a","diff":"+b","kind":{"type":"add"}}]
    }}));
    let (native, frame) = approval(&mut adapter, "item/fileChange/requestApproval", json!(1),
        json!({"threadId":"root","turnId":"one","itemId":"file"}));
    assert_eq!(frame["params"]["options"][0]["kind"], "allow_once");
    assert_eq!(frame["params"]["toolCall"]["rawInput"]["changes"][0]["diff"], "+b");
    assert_eq!(adapter.reply(&native, Some("reject"))["result"], json!({"decision":"decline"}));
    assert_eq!(adapter.reply(&native, None)["result"], json!({"decision":"cancel"}));
    let (_, frame) = approval(&mut adapter, "item/fileChange/requestApproval", json!(2),
        json!({"threadId":"root","turnId":"one","itemId":"file","grantRoot":"/"}));
    assert_eq!(frame["params"]["options"].as_array().unwrap().len(), 1);

    let (_, frame) = approval(&mut adapter, "item/fileChange/requestApproval", json!(4),
        json!({"threadId":"root","turnId":"two","itemId":"file"}));
    assert_eq!(frame["params"]["options"].as_array().unwrap().len(), 1, "the earlier turn's diff does not authorize a new item");

    let (_, frame) = approval(&mut adapter, "item/commandExecution/requestApproval", json!(5),
        json!({"threadId":"root","itemId":"command","command":"pwd","additionalPermissions":{"network":{"enabled":true}}}));
    assert_eq!(frame["params"]["options"].as_array().unwrap().len(), 1);

    let (_, frame) = approval(&mut adapter, "item/commandExecution/requestApproval", json!(6),
        json!({"threadId":"root","itemId":"command","command":"pwd","availableDecisions":["decline","cancel"]}));
    assert_eq!(frame["params"]["options"].as_array().unwrap().len(), 1);

    let (native, frame) = approval(&mut adapter, "item/permissions/requestApproval", json!(3),
        json!({"threadId":"root","itemId":"wide","permissions":{"network":{"enabled":true}}}));
    assert_eq!(frame["params"]["options"].as_array().unwrap().len(), 1);
    for option in [Some("reject"), None] {
        assert_eq!(adapter.reply(&native, option)["result"], json!({"permissions":{},"scope":"turn"}));
    }
    let settled = receive(&mut adapter, "serverRequest/resolved", json!({"threadId":"root","requestId":"0"}));
    assert!(matches!(&settled[0], Event::Resolved(id) if id == "0"));
}

#[test]
fn projects_source_metadata_for_tools_approvals_usage_and_guardian_denial() {
    let mut adapter = ready();
    receive(&mut adapter, "thread/started", json!({"thread":{"id":"child","parentThreadId":null,
        "source":{"subAgent":{"thread_spawn":{"parent_thread_id":"root","agent_nickname":"Ada","depth":1}}}}}));
    let source = json!({"thread":"child","agent":"Ada","parentThread":"root"});
    let events = receive(&mut adapter, "item/started", json!({"threadId":"child","item":{"id":"cmd","type":"commandExecution","command":"pwd"}}));
    assert_eq!(acp(&events)["params"]["update"]["_meta"]["workriver"]["source"], source);
    let (_, frame) = approval(&mut adapter, "item/commandExecution/requestApproval", json!(0),
        json!({"threadId":"child","itemId":"cmd","command":"pwd"}));
    assert_eq!(frame["params"]["_meta"]["workriver"]["source"], source);
    assert_eq!(frame["params"]["toolCall"]["_meta"]["workriver"]["source"], source);

    let events = receive(&mut adapter, "thread/tokenUsage/updated", json!({"threadId":"child",
        "tokenUsage":{"last":{"totalTokens":7},"total":{"totalTokens":15},"modelContextWindow":null}}));
    assert!(matches!(&events[..], [Event::Usage(body)] if body["thread"] == "child" && body["last"]["totalTokens"] == 7));
    let events = receive(&mut adapter, "thread/tokenUsage/updated", json!({"threadId":"root",
        "tokenUsage":{"last":{"totalTokens":8},"total":{"totalTokens":18},"modelContextWindow":100}}));
    assert_eq!(acp(&events)["params"]["update"]["used"], 8);
    assert_eq!(acp(&events)["params"]["update"]["size"], 100);
    let events = receive(&mut adapter, "turn/plan/updated", json!({"threadId":"child","turnId":"one",
        "plan":[{"step":"Check the adapter","status":"inProgress"}]}));
    assert_eq!(acp(&events)["params"]["update"]["entries"][0],
        json!({"content":"Check the adapter","priority":"medium","status":"in_progress"}));
    let events = receive(&mut adapter, "item/completed", json!({"threadId":"child","item":{"id":"compact","type":"contextCompaction"}}));
    assert_eq!(acp(&events)["params"]["update"]["status"], "completed");
    assert_eq!(acp(&events)["params"]["update"]["compactionId"], "child/compact");

    let events = receive(&mut adapter, "item/autoApprovalReview/completed", json!({"threadId":"child",
        "targetItemId":"cmd","reviewId":"review","review":{"status":"denied"}}));
    assert!(matches!(&events[0], Event::Unmapped(_)));
    assert_eq!(acp(&events)["params"]["update"]["_meta"]["workriver"]["denied"]["by"], "reviewer");
}

#[test]
fn unknown_requests_are_journaled_before_the_method_not_found_reply() {
    let mut adapter = ready();
    let native = json!({"id":"ask","method":"item/tool/requestUserInput","params":{}});
    let events = adapter.receive(native.clone()).unwrap();
    assert!(matches!(&events[0], Event::Unmapped(frame) if frame == &native));
    assert!(matches!(&events[1], Event::RequestError(frame) if frame["id"] == "ask" && frame["error"]["code"] == -32601));
}

#[test]
fn unsupported_auth_payloads_and_unsolicited_response_bodies_are_not_journaled() {
    let mut adapter = ready();
    let refresh = json!({"id":"auth","method":"account/chatgptAuthTokens/refresh",
        "params":{"previousAccountId":"private"}});
    let events = adapter.receive(refresh).unwrap();
    assert!(matches!(&events[0], Event::Unmapped(frame) if frame == &json!({
        "id":"auth", "method":"account/chatgptAuthTokens/refresh", "redacted":true
    })));
    assert!(matches!(&events[1], Event::RequestError(frame) if frame["id"] == "auth"));
    let events = adapter.receive(json!({"id":8749,"result":chatgpt()})).unwrap();
    assert!(matches!(&events[0], Event::Unmapped(frame) if frame == &json!({"id":8749,"redacted":true})));
}
