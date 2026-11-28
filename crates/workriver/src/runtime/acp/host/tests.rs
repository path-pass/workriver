use super::*;

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
