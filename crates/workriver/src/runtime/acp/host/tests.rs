use super::*;

#[test]
fn host_records_stuck_after_the_deadline_if_members_remain_or_listing_fails() {
    for (group_failed, marked_failed) in [(false, false), (true, false), (false, true)] {
        let (_dir, mut host) = closing_host();
        let leader = host.child.id();
        let remaining = u32::MAX;
        let listing = |group| {
            assert_eq!(group, leader);
            if group_failed { Err(io::Error::other("test listing failure")) }
            else if marked_failed { Ok(vec![leader]) }
            else { Ok(vec![leader, remaining]) }
        };
        let scan = || (!marked_failed).then(Vec::new);
        // Once expired, each poll advances one phase against the same deadline.
        let deadline = host.closing.as_ref().unwrap().deadline;
        for _ in 0..4 {
            assert!(!host.progress_close(deadline, listing, scan).unwrap());
        }
        assert!(host.progress_close(deadline, listing, scan).unwrap());
        let receipt = close_receipt(&mut host);
        assert_eq!(receipt["outcome"], if marked_failed { "unverifiable" } else { "stuck" });
        assert_eq!(receipt["pids"], if marked_failed { Value::Null } else if group_failed { json!([]) } else { json!([remaining]) });
        assert_eq!(receipt["listingError"], if group_failed { json!("test listing failure") } else { Value::Null });
        assert_eq!(receipt["markedError"].is_string(), marked_failed);
    }
}

fn closing_host() -> (tempfile::TempDir, Host) {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::CommandExt;
    use std::process::Command;

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
    let host = Host {
        socket: directory.bind().unwrap(), directory, record: instance.record().unwrap(),
        journal: Journal::open(path, 1).unwrap(), adapter: Adapter::new(path, None, "chatgpt", "fake").unwrap(),
        child, input: None, output: None, generation: 1, client: None,
        challengers: [const { None }; CHALLENGE_SLOTS], client_epoch: 0,
        pending: BTreeMap::new(), billing: Value::Null, working: false, cancelling: None,
        closing: Some(Closing::new(start, false, true)), reaped: false,
    };
    (dir, host)
}

fn close_receipt(host: &mut Host) -> Value {
    let mut bytes = Vec::new();
    host.journal.read(0, 10, &mut bytes).unwrap();
    let page: Value = serde_json::from_slice(&bytes).unwrap();
    let entries = page["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["event"], "closed");
    assert_eq!(host.record["state"], "closed");
    entries[0].clone()
}

#[test]
fn host_ends_a_marked_process_born_during_cleanup() {
    use std::{cell::RefCell, process::{Child, Command, Stdio}};

    #[derive(Default)]
    struct Late(Option<Child>);
    impl Drop for Late {
        fn drop(&mut self) {
            if let Some(child) = &mut self.0 {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    let (_dir, mut host) = closing_host();
    let late = RefCell::new(Late::default());
    let marker = host.marker();
    let leader = host.child.id();
    let listing = |_| Ok(vec![leader]);
    for _ in 0..4 {
        assert!(!host.progress_close(Instant::now(), listing, || Some(vec![])).unwrap());
    }
    let scan_late = || {
        if late.borrow().0.is_none() {
            let child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "common::process::tests::a_marked_sleeper", "--ignored"])
                .env(crate::common::process::MARKER, &marker).env("MARKED_SLEEP", "1")
                .stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
            let pid = child.id();
            late.borrow_mut().0 = Some(child);
            let until = Instant::now() + Duration::from_secs(2);
            while crate::common::process::marker(pid).as_ref().ok().and_then(Option::as_ref) != Some(&marker) {
                assert!(Instant::now() < until, "late process did not inherit its marker");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        let pid = late.borrow().0.as_ref().unwrap().id();
        let marked = crate::common::process::owned(pid).filter(|p| !p.zombie)
            .zip(crate::common::process::marker(pid).ok().flatten());
        Some(marked.into_iter().map(|(owned, marker)| (pid, owned, marker)).collect())
    };
    let until = Instant::now() + Duration::from_secs(3);
    while !host.progress_close(Instant::now(), listing, scan_late).unwrap() {
        assert!(Instant::now() < until, "the late process was not ended");
        std::thread::sleep(Duration::from_millis(10));
    }
    let late = late.into_inner();
    let pid = late.0.as_ref().expect("the post-KILL scan must see and end the late process").id();
    let receipt = close_receipt(&mut host);
    assert_eq!(receipt["outcome"], "ended");
    assert_eq!(receipt["processes"], json!([{"pid": pid, "outcome": "exited"}]));
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
