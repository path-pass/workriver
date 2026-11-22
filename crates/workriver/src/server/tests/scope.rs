// Conversations are read by their workspace only: through every route that returns records,
// an agent reads the conversation of the workspace whose seat it holds and no other, and of
// a message to every manager only its own copy and replies (docs/security.md).
use serde_json::{Value, json};
use workriver_core::runtime::AgentState::Idle;

use super::support::*;
use crate::cli::Daemon;

/// Managers ay (p1) and bee (p2) and a worker (p3), with the service: the user's client.
fn service(tmp: &std::path::Path) -> (std::sync::Arc<Fake>, Daemon) {
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-a", "ay", "w1", Idle)), ("p2".into(), live("s-b", "bee", "w1", Idle)), ("p3".into(), live("s-w", "w", "w1", Idle))];
    let d = daemon_with(fake.clone(), &tmp.join("w.db"), Default::default());
    for place in ["p1", "p2", "p3"] {
        at_place(&d, place).call("/api/here", Some(json!({}))).unwrap();
    }
    (fake, d)
}

/// Two workspaces named `first` and `second` (the names may be the same), ay seated in ws1
/// and bee in ws2, an intent started in each (i1, i2).
fn two_workspaces(d: &Daemon, first: &str, second: &str) {
    assert_eq!(workspace_named(d, first), "ws1");
    let made = d.call("/api/workspaces", Some(json!({ "action": "add", "project": "p1", "name": second }))).unwrap();
    assert_eq!(made["body"]["id"], "ws2");
    for (ws, agent) in [("ws1", "s-a"), ("ws2", "s-b")] {
        seat(d, ws, Some(agent)).unwrap();
        let i = d.append("", "intent.created", json!({ "title": format!("In {ws}"), "state": "planned" })).unwrap()["body"]["id"].as_str().unwrap().to_string();
        d.call("/api/intent/start", Some(json!({ "intent": i, "workspace": ws }))).unwrap();
    }
}

/// Everything a caller at `place` reads of the records, as text: `/api/state` and the
/// page's snapshot.
fn reads(d: &Daemon, place: &str) -> String {
    let at = at_place(d, place);
    at.get("/api/state").unwrap().to_string() + &at.get("/api/workspace").unwrap().to_string()
}

/// The review's probe, as it now reads: two workspaces of one name have two conversations
/// and two sets of read marks, and neither manager reads the other's, through `/api/state`
/// or the snapshot; an agent that holds no seat reads none; the user reads all.
#[test]
fn a_manager_reads_its_own_workspaces_conversation_only() {
    let tmp = tempfile::tempdir().unwrap();
    let (_fake, d) = service(tmp.path());
    two_workspaces(&d, "docs", "docs");
    d.call("/api/message", Some(json!({ "intent": "i1", "text": "SECRET_TO_A" }))).unwrap();
    d.append("s-a", "message.said", json!({ "text": "SECRET_FROM_A" })).unwrap();
    d.append("s-a", "message.replied", json!({ "message": "m1", "text": "REPLY_FROM_A" })).unwrap();
    d.call("/api/message", Some(json!({ "workspace": "ws2", "text": "SECRET_TO_B" }))).unwrap();
    let said = d.append("s-b", "message.said", json!({ "text": "SECRET_FROM_B" })).unwrap();
    assert_eq!(said["body"]["workspace"], "ws2", "the workspace it holds the seat of, whatever it is named");
    // The user reads the first conversation: the second, of the same name, stays unread.
    let all = d.get("/api/workspace").unwrap();
    let by = |text: &str| all["messages"].as_array().unwrap().iter().find(|m| m["text"] == text).unwrap().clone();
    d.call("/api/messages/read", Some(json!({ "workspace": "ws1", "at": by("SECRET_FROM_A")["at"] }))).unwrap();
    let all = d.get("/api/workspace").unwrap();
    let unread = |text: &str| all["messages"].as_array().unwrap().iter().find(|m| m["text"] == text).unwrap()["unread"] == true;
    assert!(!unread("SECRET_FROM_A") && unread("SECRET_FROM_B"), "two conversations, two read marks");

    let (a, b, w, user) = (reads(&d, "p1"), reads(&d, "p2"), reads(&d, "p3"), d.get("/api/state").unwrap().to_string());
    for mine in ["SECRET_TO_A", "SECRET_FROM_A", "REPLY_FROM_A"] {
        assert!(a.contains(mine) && !b.contains(mine) && !w.contains(mine) && user.contains(mine), "{mine}");
    }
    for theirs in ["SECRET_TO_B", "SECRET_FROM_B"] {
        assert!(b.contains(theirs) && !a.contains(theirs) && !w.contains(theirs) && user.contains(theirs), "{theirs}");
    }
    // The read marks likewise: its own workspace's, none other.
    let marks = |place: &str| at_place(&d, place).get("/api/state").unwrap()["read"].as_object().map(|r| r.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
    assert_eq!((marks("p1"), marks("p2"), marks("p3")), (vec!["ws1".to_string()], vec![], vec![]));
    // Everything that is no conversation stays readable by every agent: the other
    // workspace's intent, and who manages what.
    let seen: Value = at_place(&d, "p1").get("/api/state").unwrap();
    assert_eq!(seen["intents"].as_array().unwrap().len(), 2);
    assert_eq!(seen["workspaces"][1]["manager"]["session"], "s-b");
    // A manager the user replaces reads it no longer; the one seated reads all of it.
    seat(&d, "ws1", Some("s-w")).unwrap();
    let (a, w) = (reads(&d, "p1"), reads(&d, "p3"));
    assert!(!a.contains("SECRET_TO_A") && !a.contains("REPLY_FROM_A") && w.contains("SECRET_TO_A") && w.contains("REPLY_FROM_A") && !w.contains("SECRET_TO_B"));
}

/// A message to every manager is one record: each manager reads the message, its own copy
/// and its own replies; nothing of another's copy (its workspace, manager, delivery) or
/// replies, in `/api/state` or the snapshot. A manager seated after it was sent reads none
/// of it.
#[test]
fn a_message_to_every_manager_is_cut_to_the_readers_own_copy() {
    let tmp = tempfile::tempdir().unwrap();
    let (fake, d) = service(tmp.path());
    two_workspaces(&d, "docs", "web");
    let shot = tmp.path().join("shot.png");
    std::fs::write(&shot, b"\x89PNG to all").unwrap();
    let shot = d.upload(&shot, "user", None).unwrap();
    d.call("/api/message", Some(json!({ "all": true, "text": "TO_EVERY_MANAGER", "context": [{ "kind": "attachment", "label": "shot.png", "id": shot["id"] }] }))).unwrap();
    wait_until("both have their copy", || ["s-a", "s-b"].iter().all(|s| fake.sent.lock().unwrap().iter().any(|(to, m)| to == s && m.contains("TO_EVERY_MANAGER"))));
    d.append("s-a", "message.replied", json!({ "message": "m1", "text": "A_REPLIES" })).unwrap();
    d.append("s-b", "message.replied", json!({ "message": "m1", "text": "B_REPLIES" })).unwrap();
    wait_until("both delivered", || d.get("/api/state").unwrap()["messages"][0]["to"].as_array().unwrap().iter().all(|r| r["delivery"]["step"] == "delivered"));
    assert_eq!(d.get("/api/state").unwrap()["messages"][0]["to"].as_array().unwrap().len(), 2, "the user reads every copy");

    for (place, me, own, ws, other, others, other_ws) in [("p1", "s-a", "A_REPLIES", "ws1", "s-b", "B_REPLIES", "ws2"), ("p2", "s-b", "B_REPLIES", "ws2", "s-a", "A_REPLIES", "ws1")] {
        let at = at_place(&d, place);
        let m = at.get("/api/state").unwrap()["messages"][0].clone();
        assert_eq!(m["text"], "TO_EVERY_MANAGER");
        let copies: Vec<(&str, &str, &str)> = m["to"].as_array().unwrap().iter().map(|r| (r["manager"].as_str().unwrap(), r["workspace"].as_str().unwrap(), r["delivery"]["step"].as_str().unwrap())).collect();
        assert_eq!(copies, [(me, ws, "delivered")], "its own copy only");
        let replies: Vec<(&str, &str)> = m["replies"].as_array().unwrap().iter().map(|r| (r["by"].as_str().unwrap(), r["text"].as_str().unwrap())).collect();
        assert_eq!(replies, [(me, own)], "its own replies only");
        let text = m.to_string();
        assert!(!text.contains(other) && !text.contains(others) && !text.contains(other_ws), "nothing of the other copy: {text}");
        let page = at.get("/api/workspace").unwrap()["messages"][0].clone();
        assert_eq!(page["to"].as_array().unwrap().len(), 1, "{page}");
        assert!(page.to_string().contains(own) && !page.to_string().contains(others), "{page}");
        // The user's file on it is part of what it reads.
        assert_eq!(from_place(&d, place, "GET", &format!("/api/attachments/{}", shot["id"].as_str().unwrap()), None).0, 200);
    }
    // An agent with no seat, and a manager seated after the message went out (another
    // appointment): no copy went to it, so none of it is read.
    seat(&d, "ws1", Some("s-w")).unwrap();
    for place in ["p1", "p3"] {
        let seen = reads(&d, place);
        assert!(!seen.contains("TO_EVERY_MANAGER") && !seen.contains("A_REPLIES") && !seen.contains("B_REPLIES"), "{place}");
        assert_eq!(from_place(&d, place, "GET", &format!("/api/attachments/{}", shot["id"].as_str().unwrap()), None).0, 404, "{place}: named only by what it does not read");
    }
}

/// The attachment route goes by the same view: a file only a message of another workspace
/// names is not served to an agent; one that a task also names is, as before; the manager
/// the message went to and the user get both.
#[test]
fn an_attachment_only_another_conversation_names_is_not_served() {
    let tmp = tempfile::tempdir().unwrap();
    let (_fake, d) = service(tmp.path());
    two_workspaces(&d, "docs", "web");
    d.append("s-a", "unit.created", json!({ "intent": "i1", "title": "Survey", "kind": "research" })).unwrap();
    d.append("s-a", "task.created", json!({ "unit": "u1", "kind": "research", "title": "Read", "agent": "s-w" })).unwrap();
    let file = |name: &str, bytes: &[u8]| {
        let p = tmp.path().join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    };
    let only = d.upload(&file("only.png", b"\x89PNG only for b"), "user", None).unwrap();
    let both = d.upload(&file("both.png", b"\x89PNG for b and a task"), "user", None).unwrap();
    // The same content, stored again for the task: one id, named by both.
    let again = d.upload(&file("both.png", b"\x89PNG for b and a task"), "user", Some(("u1", 1, None))).unwrap();
    assert_eq!(again["id"], both["id"]);
    for a in [&only, &both] {
        d.call("/api/message", Some(json!({ "workspace": "ws2", "text": "See this", "context": [{ "kind": "attachment", "label": "f.png", "id": a["id"] }] }))).unwrap();
    }
    let get = |place: &str, a: &Value| from_place(&d, place, "GET", &format!("/api/attachments/{}", a["id"].as_str().unwrap()), None).0;
    assert_eq!((get("p2", &only), get("p2", &both)), (200, 200), "the manager it went to");
    for place in ["p1", "p3"] {
        assert_eq!((get(place, &only), get(place, &both)), (404, 200), "{place}: cut with the message; kept by the task");
        let listed = at_place(&d, place).get("/api/state").unwrap()["attachments"].to_string();
        assert!(!listed.contains(only["id"].as_str().unwrap()) && listed.contains(both["id"].as_str().unwrap()), "{listed}");
    }
    let (code, _) = as_user(&d, "GET", &format!("/api/attachments/{}", only["id"].as_str().unwrap()), None);
    assert_eq!(code, 200, "the user reads all");
}

/// Only a record that holds an attachment as one (a task's or a finding's files, a check's
/// report, what a question or a kept message points at) keeps it in an agent's view: an id
/// written in free text (a unit's title, a note) grants nothing.
#[test]
fn an_attachment_id_in_free_text_grants_no_access() {
    let tmp = tempfile::tempdir().unwrap();
    let (_fake, d) = service(tmp.path());
    two_workspaces(&d, "docs", "web");
    let p = tmp.path().join("only.png");
    std::fs::write(&p, b"\x89PNG only for b").unwrap();
    let only = d.upload(&p, "user", None).unwrap();
    let id = only["id"].as_str().unwrap();
    d.call("/api/message", Some(json!({ "workspace": "ws2", "text": "See this", "context": [{ "kind": "attachment", "label": "f.png", "id": id }] }))).unwrap();
    // An agent writes the id in words, where anyone reads it.
    d.append("s-a", "unit.created", json!({ "intent": "i1", "title": format!("Look at {id}"), "kind": "research" })).unwrap();
    d.append("s-a", "task.created", json!({ "unit": "u1", "kind": "research", "title": format!("Fetch /api/attachments/{id}"), "agent": "s-w" })).unwrap();
    for place in ["p1", "p3"] {
        assert_eq!(from_place(&d, place, "GET", &format!("/api/attachments/{id}"), None).0, 404, "{place}");
        let listed = at_place(&d, place).get("/api/state").unwrap()["attachments"].to_string();
        assert!(!listed.contains(id), "{place}: {listed}");
    }
    assert_eq!(from_place(&d, "p2", "GET", &format!("/api/attachments/{id}"), None).0, 200, "the manager it went to");
}
