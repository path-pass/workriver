// A hosted agent's receipts: the user's reservation, the service's start, end, answers and
// close, and who may drive a hosted worker (a manager is nobody's worker).
use serde_json::{Value, json};

use crate::records::tests::*;
use crate::*;

fn reserve(name: &str) -> Value {
    json!({"kind": "agent.start_reserved", "body": {"workspace": "ws1", "name": name, "harness": "codex", "cwd": "/p", "auth": "chatgpt"}})
}

fn started(agent: &str, session: &str) -> Value {
    json!({"kind": "agent.started", "body": {"agent": agent, "session": session, "host": {"pid": 40, "start": 7}, "generation": 1, "account": "chatgpt"}})
}

/// ws1 "chat" with boss seated, ws2 "docs" with other seated, and a1 "w" started as session s1.
fn setup() -> Records {
    let mut s = Records::default();
    workspace(&mut s, "chat", "/p");
    workspace(&mut s, "docs", "/p");
    seat(&mut s, "ws1", "boss");
    seat(&mut s, "ws2", "other");
    let e = add(&mut s, "user", reserve("w")).unwrap();
    assert!(matches!(e.body, Body::AgentStartReserved { ref id, .. } if id == "a1"));
    add(&mut s, "workriver", started("a1", "s1")).unwrap();
    s
}

fn refused(s: &mut Records, actor: &str, v: Value, says: &str) {
    let e = add(s, actor, v).unwrap_err();
    assert!(e.contains(says), "{e:?} does not say {says:?}");
}

#[test]
fn only_the_user_reserves_and_only_workriver_records_the_rest() {
    let mut s = setup();
    refused(&mut s, "boss", reserve("x"), "only the user does this (agent.start_reserved)");
    for v in [started("a1", "s9"), json!({"kind": "agent.start_ended", "body": {"agent": "a1", "why": "no"}}), json!({"kind": "agent.answered", "body": {"agent": "a1", "request": 3, "decision": "allow"}}), json!({"kind": "agent.closed", "body": {"agent": "a1", "outcome": "ended"}})] {
        for who in ["user", "boss"] {
            refused(&mut s, who, v.clone(), "only workriver records this");
        }
    }
}

#[test]
fn a_name_is_one_open_agent_and_an_id_is_never_given_again() {
    let mut s = setup();
    refused(&mut s, "user", reserve("w"), "w is a1 already");
    for bad in ["", "user", "a7", "processes", "start"] {
        refused(&mut s, "user", reserve(bad), "cannot name an agent");
    }
    let mut far = reserve("x");
    far["body"]["cwd"] = json!("p");
    refused(&mut s, "user", far, "not an absolute folder");
    add(&mut s, "user", reserve("x")).unwrap();
    add(&mut s, "workriver", json!({"kind": "agent.start_ended", "body": {"agent": "a2", "why": "did not start in time"}})).unwrap();
    refused(&mut s, "workriver", started("a2", "s2"), "the start of a2 has ended");
    // The name is free again once its start ended; the id is not.
    let e = add(&mut s, "user", reserve("x")).unwrap();
    assert!(matches!(e.body, Body::AgentStartReserved { ref id, .. } if id == "a3"));
    add(&mut s, "workriver", json!({"kind": "agent.closed", "body": {"agent": "a1", "outcome": "stuck", "pids": [5]}})).unwrap();
    add(&mut s, "user", reserve("w")).unwrap();
    assert_eq!(s.state().hosted_named("w").unwrap().id, "a4");
    assert_eq!(s.state().hosted_agent("a1").unwrap().closed.as_ref().unwrap().pids, [5]);
}

#[test]
fn a_start_records_only_a_session_nobody_knew() {
    let mut s = setup();
    refused(&mut s, "workriver", started("a1", "s9"), "the start of a1 is recorded already");
    add(&mut s, "user", reserve("x")).unwrap();
    for known in ["s1", "boss"] {
        refused(&mut s, "workriver", started("a2", known), "was known before a2 started");
    }
    add(&mut s, "workriver", json!({"kind": "agent.seen", "body": {"session": "seen", "name": "seen", "harness": "codex"}})).unwrap();
    refused(&mut s, "workriver", started("a2", "seen"), "was known before");
    add(&mut s, "workriver", started("a2", "s2")).unwrap();
    assert_eq!(s.state().hosted_by_session("s2").unwrap().id, "a2");
}

#[test]
fn an_answer_is_recorded_once_while_the_agent_runs() {
    let mut s = setup();
    let answer = json!({"kind": "agent.answered", "body": {"agent": "a1", "request": 12, "decision": "reject"}});
    let receipt = add(&mut s, "workriver", answer.clone()).unwrap();
    assert_eq!(serde_json::to_value(&receipt.body).unwrap()["body"]["by"], "user");
    refused(&mut s, "workriver", answer.clone(), "is recorded already");
    add(&mut s, "workriver", json!({"kind": "agent.closed", "body": {"agent": "a1", "outcome": "ended"}})).unwrap();
    let mut late = answer;
    late["body"]["request"] = json!(13);
    refused(&mut s, "workriver", late, "a1 is closed");
    refused(&mut s, "workriver", json!({"kind": "agent.closed", "body": {"agent": "a1", "outcome": "ended"}}), "a1 is closed");
    assert_eq!(replayed(&s), *s.state());
}

#[test]
fn a_manager_drives_its_workers_and_a_manager_is_nobody_s_worker() {
    let mut s = setup();
    let boss = Principal::Agent { session: "boss".into(), machine: None };
    let other = Principal::Agent { session: "other".into(), machine: None };
    let a1 = |s: &Records| s.state().hosted_agent("a1").unwrap().clone();
    assert!(s.state().drives(&boss, &a1(&s)));
    assert!(!s.state().drives(&other, &a1(&s)));
    assert!(s.state().drives(&Principal::User, &a1(&s)));
    // The user seats the worker as ws2's manager: ws1's manager no longer drives it.
    seat(&mut s, "ws2", "s1");
    assert!(!s.state().drives(&boss, &a1(&s)));
    assert!(s.state().drives(&Principal::User, &a1(&s)));
    // The seat taken away again: its workspace's manager may act again.
    seat(&mut s, "ws2", "other");
    assert!(s.state().drives(&boss, &a1(&s)));
    // A worker does not drive another worker; a closed one is nobody's.
    let worker = Principal::Agent { session: "s1".into(), machine: None };
    assert!(!s.state().drives(&worker, &a1(&s)));
    add(&mut s, "workriver", json!({"kind": "agent.closed", "body": {"agent": "a1", "outcome": "ended"}})).unwrap();
    assert!(!s.state().drives(&boss, &a1(&s)));
}
