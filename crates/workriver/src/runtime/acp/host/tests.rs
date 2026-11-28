use super::*;

#[test]
fn host_records_stuck_after_the_deadline_if_members_remain_or_listing_fails() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    for failed in [false, true] {
        let dir = tempfile::Builder::new().prefix("wr-close-").tempdir_in("/tmp").unwrap();
        let path = dir.path();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(path.join("host.key"), [0; 32]).unwrap();
        fs::set_permissions(path.join("host.key"), fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(path.join("instance.json"), json!({
            "id":"a1","harness":"codex","cwd":path,"auth":"chatgpt","generation":1,"state":"starting"
        }).to_string()).unwrap();
        let (directory, instance) = Directory::open(path).unwrap();
        let child = Command::new("/bin/sh").args(["-c", "exit 0"]).env_clear().process_group(0).spawn().unwrap();
        let leader = child.id();
        let start = Instant::now();
        while !close::exited(leader).unwrap() {
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(1));
        }
        let mut host = Host {
            socket: directory.bind().unwrap(), directory, record: instance.record().unwrap(),
            journal: Journal::open(path, 1).unwrap(), adapter: Adapter::new(path, None, "chatgpt", "fake").unwrap(),
            child, input: None, output: None, generation: 1, client: None,
            challengers: [const { None }; CHALLENGE_SLOTS], client_epoch: 0,
            pending: BTreeMap::new(), billing: Value::Null, working: false, cancelling: None,
            closing: Some(Closing::new(start, false, true)), reaped: false,
        };
        let remaining = u32::MAX;
        let listing = |group| {
            assert_eq!(group, leader);
            if failed { Err(io::Error::other("test listing failure")) } else { Ok(vec![leader, remaining]) }
        };
        // Once expired, each poll advances one phase against the same deadline.
        let deadline = host.closing.as_ref().unwrap().deadline;
        for _ in 0..4 {
            assert!(!host.progress_close(deadline, listing, || Some(vec![])).unwrap());
        }
        assert!(host.progress_close(deadline, listing, || Some(vec![])).unwrap());
        let mut bytes = Vec::new();
        host.journal.read(0, 10, &mut bytes).unwrap();
        let page: Value = serde_json::from_slice(&bytes).unwrap();
        let entries = page["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["event"], "closed");
        assert_eq!(entries[0]["outcome"], "stuck");
        assert_eq!(entries[0]["pids"], if failed { json!([]) } else { json!([remaining]) });
        assert_eq!(entries[0]["listingError"], if failed { json!("test listing failure") } else { Value::Null });
        assert_eq!(host.record["state"], "closed");
    }
}

#[test]
fn close_keeps_the_live_leader_and_shares_one_deadline() {
    let start = Instant::now();
    let mut closing = Closing::new(start, true, true);
    assert_eq!(closing.advance(start, false, false, false), None);
    assert_eq!(closing.advance(start + Duration::from_secs(10), false, false, false), Some(Step::Unsubscribe));
    assert_eq!(closing.advance(start + Duration::from_secs(15), false, false, false), Some(Step::Eof));
    assert_eq!(closing.advance(start + Duration::from_secs(20), false, false, false), Some(Step::Terminate));
    assert_eq!(closing.advance(start + Duration::from_secs(25), false, false, false), Some(Step::Kill));
    assert_eq!(closing.advance(start + Duration::from_secs(27), false, true, true), Some(Step::Finish));
    assert_eq!(outcome(false, &[23]), json!({"outcome":"stuck","pids":[23],"count":1}));
    assert_eq!(outcome(true, &[]), json!({"outcome":"ended"}));
    let (live, failed) = close::remaining(23, false, Ok(vec![]));
    assert!(failed.is_none());
    assert_eq!(live, [23], "a live leader remains even when the listing omits it");
    let (descendants, failed) = close::remaining(23, true, Ok(vec![23, 42]));
    assert!(failed.is_none());
    assert_eq!(descendants, [42], "only the verified exited leader is excluded");

    // A late poll clips every remaining phase to the original deadline.
    let mut late = Closing::new(start, true, true);
    for step in [Step::Unsubscribe, Step::Eof, Step::Terminate, Step::Kill, Step::Finish] {
        assert_eq!(late.advance(start + Duration::from_secs(30), false, false, false), Some(step));
    }
}

#[test]
fn approval_decision_survives_each_failure_without_retrying_the_pipe() {
    for stop_at in 0..=3 {
        let actions = std::cell::RefCell::new(Vec::new());
        let result = settle_decision(17, "reject_once", |event| {
            let mut actions = actions.borrow_mut();
            let at = actions.len();
            actions.push(event["event"].as_str().unwrap().to_owned());
            anyhow::ensure!(at != stop_at, "test append");
            Ok(at as u64 + 70)
        }, || {
            let mut actions = actions.borrow_mut();
            let at = actions.len();
            actions.push("pipe".into());
            if at == stop_at { Err(io::Error::other("test pipe")) } else { Ok(()) }
        });
        assert_eq!(actions.into_inner(), ["decided", "pipe", "replied"][..=(stop_at.min(2))]);
        if stop_at == 0 {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap(), (70, stop_at == 3));
        }
    }
}

#[test]
fn body_truncation_removes_allow_but_keeps_the_request_addressable() {
    let acp = json!({"jsonrpc":"2.0","id":0,"method":"session/request_permission","params":{
        "sessionId":"thread","toolCall":{"toolCallId":"tool","rawInput":"a".repeat(40_000)},
        "options":[{"optionId":"allow","kind":"allow_once","name":"Allow once"},{"optionId":"reject","kind":"reject_once","name":"Reject"}]
    }});
    let view = approval_view(&json!({"id":0,"method":"native","params":{"command":"b".repeat(40_000)}}), &acp);
    assert!(view["cut"].as_u64().unwrap() > 0);
    assert_eq!(view["acp"]["params"]["options"].as_array().unwrap().len(), 1);
    assert_eq!(view["acp"]["params"]["options"][0]["optionId"], "reject");
    assert!(serde_json::to_vec(&view).unwrap().len() <= APPROVAL_BYTES);
}
