use serde_json::json;

use crate::records::tests::*;
use crate::*;

#[test]
fn questions_are_answered_by_their_own_agent_or_the_user() {
    let mut s = Records::default();
    seeded(&mut s);
    let e = s.append(&"user".into(), body(json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": "Why a regex?",
        "context": [{"kind": "lines", "label": "a.ts L3–4", "path": "app/a.ts", "from": 3, "to": 4}]}}))).unwrap();
    let Body::QuestionAsked { id, .. } = e.body else { panic!() };
    assert_eq!(id, "q1");
    assert!(s.append(&"user".into(), body(json!({"kind": "question.asked", "body": {"unit": "u1", "task": 9, "text": "?"}}))).is_err());
    s.append(&"workriver".into(), body(json!({"kind": "question.started", "body": {"question": "q1", "agent": "s-why", "name": "why-q1"}}))).unwrap();
    // Another agent may not answer; the started one may, known by its name even under a new session.
    let other = s.append(&"agent-b".into(), body(json!({"kind": "question.answered", "body": {"question": "q1", "text": "x"}}))).unwrap_err().to_string();
    assert!(other.contains("why-q1"), "{other}");
    s.append(&"workriver".into(), body(json!({"kind": "agent.seen", "body": {"session": "s-why-2", "name": "why-q1", "harness": "claude"}}))).unwrap();
    s.append(&"s-why-2".into(), body(json!({"kind": "question.answered", "body": {"question": "q1", "text": "It is short.", "asked": "agent-a"}}))).unwrap();
    let q = s.state().question("q1").unwrap();
    assert_eq!((q.answer.as_deref(), q.asked.as_deref(), q.answered_by.as_deref()), (Some("It is short."), Some("agent-a"), Some("s-why-2")));
    let again = s.append(&"user".into(), body(json!({"kind": "question.answered", "body": {"question": "q1", "text": "y"}}))).unwrap_err().to_string();
    // The task's own agent may answer questions about its work.
    s.append(&"user".into(), body(json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": "Own?"}}))).unwrap();
    s.append(&"agent-a".into(), body(json!({"kind": "question.answered", "body": {"question": "q2", "text": "Mine."}}))).unwrap();
    assert!(again.contains("already answered"), "{again}");
    // A question can fail once, while open; the user can still answer an open one.
    s.append(&"user".into(), body(json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": "And?"}}))).unwrap();
    s.append(&"workriver".into(), body(json!({"kind": "question.failed", "body": {"question": "q3", "reason_code": "timeout", "reason": "no answer in 10 minutes"}}))).unwrap();
    assert!(s.append(&"user".into(), body(json!({"kind": "question.answered", "body": {"question": "q3", "text": "late"}}))).is_err());
}

#[test]
fn attachments_are_stored_before_they_are_used() {
    let mut s = Records::default();
    seeded(&mut s);
    let unknown = s.append(&"user".into(), body(json!({"kind": "task.amended", "body": {"unit": "u1", "task": 1, "attachments": ["00000000000000ab"]}}))).unwrap_err().to_string();
    assert!(unknown.contains("no stored attachment 00000000000000ab"), "{unknown}");
    s.append(&"agent-a".into(), body(json!({"kind": "attachment.added", "body": {"id": "00000000000000ab", "name": "a.png", "type": "image/png", "size": 3}}))).unwrap();
    s.append(&"agent-a".into(), body(json!({"kind": "task.amended", "body": {"unit": "u1", "task": 1, "attachments": ["00000000000000ab"]}}))).unwrap();
    // Added again only to link it to a finding: one record, two links.
    s.append(&"agent-a".into(), body(json!({"kind": "attachment.added", "body": {"id": "00000000000000ab", "name": "a.png", "type": "image/png", "size": 3, "unit": "u1", "task": 2, "finding": "P2-1"}}))).unwrap();
    assert!(s.append(&"agent-a".into(), body(json!({"kind": "attachment.added", "body": {"id": "00000000000000c2", "name": "x", "type": "", "size": 1, "unit": "u1", "task": 2, "finding": "P9-9"}}))).is_err());
    assert_eq!(s.state().attachments.len(), 1);
    assert_eq!(s.state().task("u1", 1).unwrap().attachments, ["00000000000000ab"]);
    assert_eq!(s.state().task("u1", 2).unwrap().findings[0].found.attachments, ["00000000000000ab"]);
}

/// A question may point at an attachment shown with its task (its own, those added to what it
/// asks, its findings', those of the findings it answers), never at another's; a blind task not delivered, not even at those
/// of the findings it answers.
#[test]
fn questions_point_only_at_their_tasks_attachments() {
    let mut s = Records::default();
    seeded(&mut s);
    let file = |id: &str, link: serde_json::Value| {
        let mut b = json!({"id": id, "name": format!("{id}.md"), "type": "text/markdown", "size": 3});
        b.as_object_mut().unwrap().extend(link.as_object().unwrap().clone());
        json!({"kind": "attachment.added", "body": b})
    };
    add(&mut s, "agent-a", file("00000000000000a1", json!({"unit": "u1", "task": 1}))).unwrap();
    add(&mut s, "agent-a", file("00000000000000f1", json!({"unit": "u1", "task": 2, "finding": "P2-1"}))).unwrap();
    add(&mut s, "agent-a", file("0000000000000001", json!({}))).unwrap();
    let ask = |task: u32, id: Option<&str>| json!({"kind": "question.asked", "body": {"unit": "u1", "task": task, "text": "What does it say?", "context": [{"kind": "attachment", "label": "report.md", "id": id}]}});
    add(&mut s, "user", ask(1, Some("00000000000000a1"))).unwrap();
    assert_eq!(s.state().questions[0].context[0].id.as_deref(), Some("00000000000000a1"));
    for wrong in [Some("00000000000000f1"), Some("0000000000000001"), Some("00000000000000b1"), None] {
        assert!(add(&mut s, "user", ask(1, wrong)).unwrap_err().contains("is not one of u1#1's"), "{wrong:?}");
    }
    add(&mut s, "user", ask(2, Some("00000000000000f1"))).unwrap();
    // Work that answers a finding shows that finding's attachments.
    add(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "accept"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Dedupe", "causes": [{"task": 2, "findings": ["P2-1"]}]}})).unwrap();
    add(&mut s, "user", ask(3, Some("00000000000000f1"))).unwrap();
    assert_eq!(s.state().attachments_of("u1", 3), ["00000000000000f1"]);
    // A blind review in progress: nothing of other work, not even through the user's question.
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "reviewer": "security-review", "inputs": [{"from": "3"}], "causes": [{"task": 2, "findings": ["P2-1"]}], "agent": "sec", "blind": true}})).unwrap();
    assert!(s.state().attachments_of("u1", 4).is_empty());
    assert!(add(&mut s, "user", ask(4, Some("00000000000000f1"))).unwrap_err().contains("is not one of u1#4's"));
    // What was added to what a task asks shows with it, its files too, blind or not; not with other work.
    add(&mut s, "boss", file("00000000000000e4", json!({}))).unwrap();
    add(&mut s, "user", json!({"kind": "task.noted", "body": {"unit": "u1", "task": 4, "text": "Read the logs too", "attachments": ["00000000000000e4"]}})).unwrap();
    assert_eq!(s.state().attachments_of("u1", 4), ["00000000000000e4"]);
    add(&mut s, "user", ask(4, Some("00000000000000e4"))).unwrap();
    assert!(add(&mut s, "user", ask(1, Some("00000000000000e4"))).unwrap_err().contains("is not one of u1#1's"));
    add(&mut s, "sec", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 4, "findings": []}})).unwrap();
    add(&mut s, "user", ask(4, Some("00000000000000f1"))).unwrap();
    add(&mut s, "agent-a", ask(4, Some("00000000000000e4"))).unwrap();
    assert!(replayed(&s).questions.iter().all(|q| q.context[0].kind == "attachment"));
}

/// The user's question may also point at a file the user stored for it (dropped, pasted or
/// picked in the page): about any task, or the unit. An agent's question may not, and nobody
/// points at a file only an agent stored and no task shows.
#[test]
fn the_users_question_points_at_files_the_user_stored() {
    let mut s = Records::default();
    seeded(&mut s);
    let file = |id: &str| json!({"kind": "attachment.added", "body": {"id": id, "name": format!("{id}.log"), "type": "text/plain", "size": 3}});
    add(&mut s, "user", file("00000000000000e1")).unwrap();
    add(&mut s, "agent-a", file("0000000000000001")).unwrap();
    let ask = |task: u32, id: &str| json!({"kind": "question.asked", "body": {"unit": "u1", "task": task, "text": "What does it say?", "context": [{"kind": "attachment", "label": "x.log", "id": id}]}});
    add(&mut s, "user", ask(1, "00000000000000e1")).unwrap();
    add(&mut s, "user", ask(2, "00000000000000e1")).unwrap();
    add(&mut s, "user", ask(0, "00000000000000e1")).unwrap();
    assert!(add(&mut s, "agent-a", ask(1, "00000000000000e1")).unwrap_err().contains("is not one of u1#1's"), "an agent's question");
    assert!(add(&mut s, "user", ask(1, "0000000000000001")).unwrap_err().contains("is not one of u1#1's"), "stored by an agent only");
    assert!(add(&mut s, "user", ask(1, "00000000000000b2")).unwrap_err().contains("is not one of u1#1's"), "never stored");
    // The same content stored by an agent first (one file per content): the user's once they store it too.
    add(&mut s, "user", file("0000000000000001")).unwrap();
    add(&mut s, "user", ask(1, "0000000000000001")).unwrap();
    assert_eq!(s.state().attachments.len(), 2);
    assert_eq!(replayed(&s).attachments, s.state().attachments);
}

/// The user asks about a unit itself (task 0): its own threads, attachments of its tasks;
/// agents still ask about a task. Old questions (every one names a task) replay unchanged.
#[test]
fn the_user_asks_about_a_unit() {
    let mut s = Records::default();
    seeded(&mut s);
    let ask = |task: u32, thread: &str| json!({"kind": "question.asked", "body": {"unit": "u1", "task": task, "thread": thread, "text": "Why this order?", "context": [{"kind": "unit", "label": "Mentions"}]}});
    let e = add(&mut s, "user", ask(0, "")).unwrap();
    assert_eq!(e.body.refs(), (None, Some("u1".into()), None), "about the unit, no task");
    // Without `task` at all: the unit too.
    add(&mut s, "user", json!({"kind": "question.asked", "body": {"unit": "u1", "text": "And?"}})).unwrap();
    assert_eq!((s.state().questions[1].task, s.state().threads[1].task), (0, 0));
    // A follow-up stays in the unit's thread; a task's thread is another conversation.
    add(&mut s, "user", ask(0, "q1")).unwrap();
    assert!(add(&mut s, "user", ask(1, "q1")).unwrap_err().contains("about another task"));
    add(&mut s, "user", ask(1, "")).unwrap();
    assert!(add(&mut s, "user", ask(0, "q4")).unwrap_err().contains("about another task"));
    // Agents do not ask about whole units; nobody asks about a unit that is not there.
    assert!(add(&mut s, "agent-b", ask(0, "")).unwrap_err().contains("workriver ask <unit>#<n>"));
    assert!(add(&mut s, "user", json!({"kind": "question.asked", "body": {"unit": "u9", "text": "?"}})).unwrap_err().contains("no unit u9"));
    // An attachment shown with one of its tasks may be pointed at; another one not.
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": "00000000000000c1", "name": "r.md", "type": "text/markdown", "size": 1, "unit": "u1", "task": 2}})).unwrap();
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": "0000000000000001", "name": "x.md", "type": "text/markdown", "size": 1}})).unwrap();
    let at = |id: &str| json!({"kind": "question.asked", "body": {"unit": "u1", "task": 0, "text": "?", "context": [{"kind": "attachment", "label": "r.md", "id": id}]}});
    add(&mut s, "user", at("00000000000000c1")).unwrap();
    assert!(add(&mut s, "user", at("0000000000000001")).unwrap_err().contains("not one of u1#0's"));
    // Answered by the agent started for it, with when.
    add(&mut s, "workriver", json!({"kind": "question.started", "body": {"question": "q1", "agent": "s-why", "name": "why-q1"}})).unwrap();
    add(&mut s, "s-why", json!({"kind": "question.answered", "body": {"question": "q1", "text": "Parse first."}})).unwrap();
    assert!(s.state().question("q1").unwrap().answered_at.is_some());
    assert_eq!(replayed(&s), *s.state());
}

/// The user writes to a workspace's manager; only that manager replies, workriver alone
/// records where the message is, and it all replays.
#[test]
fn the_user_talks_to_a_manager() {
    let mut s = Records::default();
    let w1 = workspace(&mut s, "chat", "/r");
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Mentions", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Search", "state": "planned"}})).unwrap();
    let say = |intent: &str, workspace: &str| json!({"kind": "message.sent", "body": {"intent": intent, "workspace": workspace, "manager": "forged", "text": "Pause u2 until the API settles."}});
    // Nobody to tell yet.
    assert!(add(&mut s, "user", say("i1", "")).unwrap_err().contains("intent i1 runs in no workspace"));
    let e = add(&mut s, "user", say("", "ws1")).unwrap_err();
    assert!(e.contains("workspace ws1 has no manager") && e.contains(SEATING), "{e}");
    seat(&mut s, &w1, "boss");
    add(&mut s, "user", start("i1", &w1)).unwrap();
    // Only the user writes; the manager is the workspace's, whatever the request says.
    assert!(add(&mut s, "boss", say("i1", "")).unwrap_err().contains("only the user does this (message.sent)"));
    let e = add(&mut s, "user", say("i1", "")).unwrap();
    let Body::MessageSent { id, workspace, manager, .. } = &e.body else { panic!() };
    assert_eq!((id.as_str(), workspace.as_str(), manager.as_str()), ("m1", "ws1", "boss"));
    assert_eq!(e.body.refs().0.as_deref(), Some("i1"));
    // By workspace alone, when nothing of an intent is picked.
    add(&mut s, "user", say("", "ws1")).unwrap();
    // A name finds nothing: a workspace is its id.
    for named in ["chat", "nowhere", ""] {
        assert!(add(&mut s, "user", say("", named)).unwrap_err().contains(&format!("no workspace {named}")), "{named}");
    }
    assert!(add(&mut s, "user", say("i2", "")).unwrap_err().contains("intent i2 runs in no workspace"), "planned: it runs nowhere");
    // Where it is: workriver's word only.
    let delivery = |m: &str| json!({"kind": "message.delivery", "body": {"id": m, "step": "delivered", "to": "boss", "reason": null, "message": "[workriver] ..."}});
    assert!(add(&mut s, "boss", delivery("m1")).unwrap_err().contains("only workriver"));
    assert!(add(&mut s, "user", delivery("m1")).unwrap_err().contains("only workriver"));
    assert!(add(&mut s, "workriver", delivery("m9")).unwrap_err().contains("no message m9"));
    add(&mut s, "workriver", delivery("m1")).unwrap();
    assert_eq!(s.state().message("m1").unwrap().delivery.as_ref().map(|d| d.step), Some(DeliveryStep::Delivered));
    // Replies: the manager it went to, and nobody else.
    let reply = |m: &str| json!({"kind": "message.replied", "body": {"message": m, "text": "Paused."}});
    assert!(add(&mut s, "worker", reply("m1")).unwrap_err().contains("only that manager replies"));
    assert!(add(&mut s, "user", reply("m1")).unwrap_err().contains("only that manager replies"));
    assert!(add(&mut s, "boss", reply("m9")).unwrap_err().contains("no message m9"));
    add(&mut s, "boss", reply("m1")).unwrap();
    add(&mut s, "boss", reply("m1")).unwrap();
    let m = s.state().message("m1").unwrap();
    assert_eq!((m.replies.len(), m.replies[0].by.as_str(), m.replies[0].text.as_str()), (2, "boss", "Paused."));
    assert_eq!(m.since, s.state().workspace("ws1").unwrap().manager.as_ref().unwrap().since, "it went to that appointment");
    assert_eq!(replayed(&s), *s.state());
}

/// A workspace's manager starts a message to the user (into its workspace's conversation);
/// nobody else does. Only the user records reading a conversation; it only moves forward,
/// and it all replays.
#[test]
fn a_manager_says_and_the_user_reads() {
    let mut s = Records::default();
    let w1 = workspace(&mut s, "chat", "/r");
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Mentions", "state": "planned"}})).unwrap();
    let say = |text: &str| json!({"kind": "message.said", "body": {"workspace": "forged", "text": text}});
    assert!(add(&mut s, "boss", say("Hi")).unwrap_err().contains("only a workspace's manager"), "not seated yet");
    seat(&mut s, &w1, "boss");
    add(&mut s, "user", start("i1", &w1)).unwrap();
    // A worker is pointed to its manager; the user writes from the page; nothing blank.
    assert!(add(&mut s, "worker", say("Hi")).unwrap_err().contains("workriver manager tell"));
    assert!(add(&mut s, "user", say("Hi")).unwrap_err().contains("workriver's page"));
    assert!(add(&mut s, "boss", say("  ")).unwrap_err().contains("--text"));
    // Into the workspace it manages, about its current intent, whatever the request says.
    let e = add(&mut s, "boss", say("u1 is merged.")).unwrap();
    let Body::MessageSaid { id, intent, workspace, .. } = &e.body else { panic!() };
    assert_eq!((id.as_str(), intent.as_str(), workspace.as_str()), ("m1", "i1", "ws1"));
    assert_eq!(e.body.refs().0.as_deref(), Some("i1"));
    let m = s.state().message("m1").unwrap();
    assert!(m.by_manager && m.delivery.is_none());
    assert_eq!((m.manager.as_str(), m.text.as_str()), ("boss", "u1 is merged."));
    // The user answers it from the same box: an ordinary message; the manager may follow up.
    add(&mut s, "user", json!({"kind": "message.sent", "body": {"workspace": "ws1", "text": "Thanks."}})).unwrap();
    add(&mut s, "boss", json!({"kind": "message.replied", "body": {"message": "m1", "text": "Also u2."}})).unwrap();
    // Reading: the user's alone; a time, kept as the service writes times; never backwards.
    let read = |at: &str| json!({"kind": "messages.read", "body": {"workspace": "ws1", "at": at}});
    assert!(add(&mut s, "boss", read("2026-11-18T10:00:00Z")).unwrap_err().contains("only the user does this (messages.read)"));
    assert!(add(&mut s, "user", read("soon")).unwrap_err().contains("not a time"));
    assert!(add(&mut s, "user", json!({"kind": "messages.read", "body": {"workspace": "chat", "at": "2026-11-18T10:00:00Z"}})).unwrap_err().contains("no workspace chat"), "by id, never by name");
    add(&mut s, "user", read("2026-11-18T10:00:05.250Z")).unwrap();
    assert_eq!(s.state().read["ws1"], "2026-11-18T10:00:05Z");
    add(&mut s, "user", read("2026-11-18T09:00:00Z")).unwrap();
    assert_eq!(s.state().read["ws1"], "2026-11-18T10:00:05Z");
    assert_eq!(replayed(&s), *s.state());
}

/// What the user picked goes with a message as records: a unit, one of its tasks, and context
/// (findings, files, lines, attachments, other tasks of the unit by number). The user's own
/// files need no unit; everything else does, and must be of the message's intent. Nothing
/// goes to a manager on a blind task. Messages recorded before replay unchanged.
#[test]
fn a_message_carries_what_the_user_picked() {
    let mut s = Records::default();
    let w1 = workspace(&mut s, "chat", "/r");
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Chat v2", "state": "planned"}})).unwrap();
    seat(&mut s, &w1, "boss");
    add(&mut s, "user", start("i1", &w1)).unwrap();
    // The seed's unit and tasks, in that intent.
    for b in seed().into_iter().skip(1) {
        // The unit is its manager's to add.
        let who = if matches!(b, Body::UnitCreated { .. }) { "boss" } else { "agent-a" };
        s.append(&who.into(), b).unwrap();
    }
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Search", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "unit.created", "body": {"template": "bare", "intent": "i2", "title": "Paging", "repo": "/r", "branch": "feat/paging", "base": "main"}})).unwrap();
    add(&mut s, "user", json!({"kind": "attachment.added", "body": {"id": "00000000000000e1", "name": "shot.png", "type": "image/png", "size": 3}})).unwrap();
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": "00000000000000c1", "name": "r.md", "type": "text/markdown", "size": 1, "unit": "u1", "task": 2}})).unwrap();
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": "0000000000000001", "name": "x.md", "type": "text/markdown", "size": 1}})).unwrap();
    let say = |intent: &str, workspace: &str, unit: &str, task: u32, context: serde_json::Value| {
        json!({"kind": "message.sent", "body": {"intent": intent, "workspace": workspace, "unit": unit, "task": task, "text": "Why not paged?", "context": context}})
    };
    let lines = json!({"kind": "lines", "label": "mention.ts L3–4", "path": "app/mention.ts", "from": 3, "to": 4});
    let file = |id: &str| json!({"kind": "attachment", "label": "x", "id": id});

    // With everything a picked review shows: recorded with it, as sent.
    let context = json!([{"kind": "task", "label": "#1", "task": 1}, {"kind": "finding", "label": "P2-1"}, lines, file("00000000000000c1"), file("00000000000000e1")]);
    let e = add(&mut s, "user", say("i1", "", "u1", 2, context.clone())).unwrap();
    let Body::MessageSent { unit, task, context: got, .. } = &e.body else { panic!() };
    assert_eq!((unit.as_str(), *task, got.len()), ("u1", 2, 5));
    assert_eq!(got[0].task, Some(1));
    let m = s.state().message("m1").unwrap();
    assert_eq!((m.unit.as_str(), m.task, m.context.len()), ("u1", 2, 5));
    // By workspace alone, a unit of an intent that runs there.
    add(&mut s, "user", say("", "ws1", "u1", 1, json!([lines]))).unwrap();
    // Only the user's own files: no unit needed, from any workspace's conversation.
    add(&mut s, "user", say("", "ws1", "", 0, json!([file("00000000000000e1")]))).unwrap();
    add(&mut s, "user", say("i1", "", "", 0, json!([file("00000000000000e1")]))).unwrap();

    // Everything else needs a unit.
    for c in [json!({"kind": "finding", "label": "P2-1"}), json!({"kind": "file", "label": "a.ts", "path": "app/a.ts"}), lines.clone(), json!({"kind": "task", "label": "#1", "task": 1}), file("00000000000000c1")] {
        let e = add(&mut s, "user", say("i1", "", "", 0, json!([c]))).unwrap_err();
        assert!(e.contains("needs the unit it is about"), "{c}: {e}");
    }
    assert!(add(&mut s, "user", say("i1", "", "", 1, json!([]))).unwrap_err().contains("needs the unit it is about"), "a task with no unit");
    // Of the message's intent; a task of the unit; an extra task of the unit, by number.
    assert!(add(&mut s, "user", say("i1", "", "u2", 0, json!([]))).unwrap_err().contains("u2 is not of intent i1"));
    assert!(add(&mut s, "user", say("", "ws1", "u2", 0, json!([]))).unwrap_err().contains("u2 is not of an intent of workspace ws1 (chat)"));
    assert!(add(&mut s, "user", say("i1", "", "u9", 0, json!([]))).unwrap_err().contains("no unit u9"));
    assert!(add(&mut s, "user", say("i1", "", "u1", 9, json!([]))).unwrap_err().contains("no task u1#9"));
    assert!(add(&mut s, "user", say("i1", "", "u1", 2, json!([{"kind": "task", "label": "#7", "task": 7}]))).unwrap_err().contains("no task u1#7"));
    assert!(add(&mut s, "user", say("i1", "", "u1", 2, json!([{"kind": "task", "label": "#1"}]))).unwrap_err().contains("names its task by number"));
    // An attachment neither the user's nor shown with the task.
    assert!(add(&mut s, "user", say("i1", "", "u1", 1, json!([file("00000000000000c1")]))).unwrap_err().contains("is not one of u1#1's"));
    assert!(add(&mut s, "user", say("i1", "", "u1", 1, json!([file("0000000000000001")]))).unwrap_err().contains("is not one of u1#1's"));
    assert!(add(&mut s, "user", say("i1", "", "", 0, json!([file("0000000000000001")]))).unwrap_err().contains("needs the unit it is about"));
    // With the unit alone, one shown with any of its tasks.
    add(&mut s, "user", say("i1", "", "u1", 0, json!([file("00000000000000c1")]))).unwrap();
    // A workspace of the same name is another workspace: its intent's unit is not one of
    // ws1's, whose manager the message goes to.
    let far = Principal::Agent { session: "far".into(), machine: Some("box".into()) };
    let twin = workspace(&mut s, "chat", "/r");
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Far", "state": "planned"}})).unwrap();
    seat_at(&mut s, &twin, Some("far"), Some("box"), "/home/far").unwrap();
    add(&mut s, "user", start("i3", &twin)).unwrap();
    s.append(&far, crate::records::tests::body(json!({"kind": "unit.created", "body": {"template": "bare", "intent": "i3", "title": "Over there", "repo": "/r", "branch": "feat/far", "base": "main", "machine": "box"}}))).unwrap();
    assert!(add(&mut s, "user", say("", "ws1", "u3", 0, json!([]))).unwrap_err().contains("u3 is not of an intent of workspace ws1 (chat)"));
    add(&mut s, "user", say("", &twin, "u3", 0, json!([]))).unwrap();
    // Nothing else is pointed at.
    assert!(add(&mut s, "user", say("i1", "", "u1", 1, json!([{"kind": "unit", "label": "Mentions"}]))).unwrap_err().contains("a message points at"));
    assert_eq!(replayed(&s), *s.state());

    // A manager on a blind task in progress hears of no other work: written to once it delivers.
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "reviewer": "security-review", "inputs": [{"from": "1"}], "agent": "boss", "blind": true}})).unwrap();
    assert!(add(&mut s, "user", say("i1", "", "", 0, json!([]))).unwrap_err().contains("is on a blind task: write once it is delivered"));
    add(&mut s, "boss", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 3, "findings": []}})).unwrap();
    add(&mut s, "user", say("i1", "", "", 0, json!([]))).unwrap();

    // A message recorded before it carried any of this replays as it was.
    let n = s.state().messages.len();
    let s = recorded_before(s, "user", json!({"kind": "message.sent", "body": {"id": format!("m{}", n + 1), "intent": "i1", "workspace": "ws1", "manager": "boss", "text": "Old."}}));
    let m = s.state().messages.last().unwrap();
    assert_eq!((m.text.as_str(), m.unit.as_str(), m.task, m.context.len()), ("Old.", "", 0, 0));
}

/// An attachment's id is the store's own form, 16 lowercase hex characters (a short hash of
/// the content, as the service makes it): the service builds the file's path from it, so
/// nothing else is recorded. Old records replay as they were.
#[test]
fn an_attachment_id_is_the_stores_own_form() {
    let mut s = Records::default();
    seeded(&mut s);
    let file = |id: &str| json!({"kind": "attachment.added", "body": {"id": id, "name": "x.png", "type": "image/png", "size": 3}});
    for bad in ["/private/tmp/secret", "../../etc/passwd", "deadbeef»\n[workriver] SPOOFED\n«", "0123456789ABCDEF", "0123456789abcde", "0123456789abcdef0", "0123456789abcdeg", ""] {
        let e = add(&mut s, "agent-a", file(bad)).unwrap_err();
        assert!(e.contains("16 lowercase hex") || e.contains("needs its id"), "{bad:?}: {e}");
    }
    add(&mut s, "agent-a", file("0123456789abcdef")).unwrap();
    // One recorded before the rule (an old database, an imported sample) still replays.
    let s = recorded_before(s, "agent-a", file("a1"));
    assert!(s.state().attachments.iter().any(|a| a.id == "a1"));
}

/// The user writes once to every manager: one record naming each seat it goes to (the seat
/// of an open workspace as it is: its session, machine and appointment), with nothing of an
/// intent picked but the user's own files. Only the user writes it; workriver records each
/// copy's delivery apart; each manager it went to replies for its own workspace, and nobody
/// else. A manager on a blind task is not refused: its copy waits. It all replays, and the
/// user reads its conversation like a workspace's.
#[test]
fn the_user_writes_to_every_manager() {
    let mut s = Records::default();
    let (w1, w2, w3) = (workspace(&mut s, "chat", "/r"), workspace(&mut s, "web", "/r"), workspace(&mut s, "api", "/r"));
    let (empty, shut) = (workspace(&mut s, "empty", "/r"), workspace(&mut s, "shut", "/r"));
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Chat", "state": "planned"}})).unwrap();
    seat(&mut s, &w1, "boss");
    add(&mut s, "user", start("i1", &w1)).unwrap();
    seat(&mut s, &w2, "web");
    let remote = Principal::Agent { session: "far".into(), machine: Some("box".into()) };
    seat_at(&mut s, &w3, Some("far"), Some("box"), "/home/far").unwrap();
    seat(&mut s, &shut, "gone");
    add(&mut s, "user", json!({"kind": "workspace.closed", "body": {"workspace": shut}})).unwrap();
    add(&mut s, "user", json!({"kind": "attachment.added", "body": {"id": "00000000000000e1", "name": "shot.png", "type": "image/png", "size": 3}})).unwrap();

    // The seat of every open workspace whose session runs on the seat's machine, one copy
    // each; an agent that holds no seat gets none, nor does a seat nobody runs in.
    let runs = |session: &str, machine: Option<&str>| crate::runtime::RuntimeAgent {
        session: session.into(),
        harness: "claude".into(),
        state: crate::runtime::AgentState::Idle,
        cwd: String::new(),
        group: None,
        name: session.into(),
        topic: None,
        machine: machine.map(str::to_string),
    };
    let live = [runs("boss", None), runs("worker", None), runs("web", None), runs("far", Some("box")), runs("gone", None)];
    let every = s.state().managers_among(&live);
    let named: Vec<(&str, &str, Option<&str>, u64)> = every.iter().map(|r| (r.manager.as_str(), r.workspace.as_str(), r.machine.as_deref(), r.since)).collect();
    let since = |w: &str| s.state().workspace(w).unwrap().manager.as_ref().unwrap().since;
    assert_eq!(named, [("boss", "ws1", None, since("ws1")), ("web", "ws2", None, since("ws2")), ("far", "ws3", Some("box"), since("ws3"))]);
    assert!(s.state().managers_among(&[runs("far", None), runs("boss", Some("box"))]).is_empty(), "a session of that id on another machine is not the seat's");
    assert_eq!((empty.as_str(), s.state().workspace(&empty).unwrap().manager.is_none()), ("ws4", true));
    let to = |list: &[&crate::Recipient]| serde_json::to_value(list).unwrap();
    let all: Vec<&crate::Recipient> = every.iter().collect();
    let say = |to: serde_json::Value, extra: serde_json::Value| {
        let mut b = json!({"text": "I shut down at six.", "to": to, "workspace": "forged", "manager": "forged"});
        b.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        json!({"kind": "message.sent", "body": b})
    };
    // The user's alone.
    assert!(add(&mut s, "boss", say(to(&all), json!({}))).unwrap_err().contains("only the user does this (message.sent)"));
    // Only to seats as they are, each once: not another session, appointment, machine or workspace.
    let wrong = |f: &dyn Fn(&mut crate::Recipient)| {
        let mut r = every[0].clone();
        f(&mut r);
        r
    };
    for (r, who) in [(wrong(&|r| r.manager = "worker".into()), "worker"), (wrong(&|r| r.since += 1), "boss"), (wrong(&|r| r.machine = Some("box".into())), "boss"), (wrong(&|r| r.workspace = "ws2".into()), "boss"), (wrong(&|r| r.workspace = "ws4".into()), "boss")] {
        let e = add(&mut s, "user", say(to(&[&r]), json!({}))).unwrap_err();
        assert!(e.contains(&format!("{who} does not hold the seat of workspace {} as given", r.workspace)), "{e}");
    }
    assert!(add(&mut s, "user", say(to(&[&wrong(&|r| r.workspace = "ws9".into())]), json!({}))).unwrap_err().contains("no workspace ws9"));
    assert!(add(&mut s, "user", say(to(&[&every[0], &every[0]]), json!({}))).unwrap_err().contains("boss twice"));
    // About no intent or unit: only the user's own files go with it.
    assert!(add(&mut s, "user", say(to(&all), json!({"intent": "i1"}))).unwrap_err().contains("to every manager is about no intent"));
    assert!(add(&mut s, "user", say(to(&all), json!({"unit": "u1"}))).unwrap_err().contains("to every manager is about no intent"));
    assert!(add(&mut s, "user", say(to(&all), json!({"context": [{"kind": "finding", "label": "P2-1"}]}))).unwrap_err().contains("needs the unit it is about"));
    assert!(add(&mut s, "user", say(to(&all), json!({"text": "  "}))).unwrap_err().contains("needs its text"));

    // A manager on a blind task gets its copy once it delivers: not refused.
    for b in seed().into_iter().skip(1) {
        // The unit is its manager's to add.
        let who = if matches!(b, Body::UnitCreated { .. }) { "boss" } else { "agent-a" };
        s.append(&who.into(), b).unwrap();
    }
    add(&mut s, "web", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "reviewer": "security-review", "inputs": [{"from": "1"}], "agent": "web", "blind": true}})).unwrap();
    let e = add(&mut s, "user", say(to(&all), json!({"context": [{"kind": "attachment", "label": "shot.png", "id": "00000000000000e1"}]}))).unwrap();
    let Body::MessageSent { id, workspace, manager, intent, to: got, .. } = &e.body else { panic!() };
    assert_eq!((id.as_str(), workspace.as_str(), manager.as_str(), intent.as_str(), got.len()), ("m1", "", "", "", 3));
    assert_eq!(e.body.refs(), (None, None, None));
    let m = s.state().message("m1").unwrap();
    assert!(m.delivery.is_none() && m.to.iter().all(|r| r.delivery.is_none()));

    // Where each copy is: workriver's word only, one manager's at a time, of those it went to.
    let delivery = |manager: &str| json!({"kind": "message.delivery", "body": {"id": "m1", "manager": manager, "step": "delivered", "to": manager, "reason": null, "message": "[workriver] ..."}});
    assert!(add(&mut s, "user", delivery("boss")).unwrap_err().contains("only workriver"));
    assert!(add(&mut s, "workriver", delivery("worker")).unwrap_err().contains("message m1 did not go to worker"));
    assert!(add(&mut s, "workriver", delivery("")).unwrap_err().contains("message m1 went to every manager: say whose copy"));
    add(&mut s, "workriver", delivery("far")).unwrap();
    let m = s.state().message("m1").unwrap();
    assert_eq!(m.to.iter().map(|r| r.delivery.as_ref().map(|d| d.step)).collect::<Vec<_>>(), [None, None, Some(DeliveryStep::Delivered)]);
    assert!(m.delivery.is_none());

    // Each manager it went to replies, for its own workspace; nobody else.
    let reply = |text: &str| json!({"kind": "message.replied", "body": {"message": "m1", "text": text}});
    assert!(add(&mut s, "worker", reply("Ok")).unwrap_err().contains("only the managers it went to reply"));
    assert!(add(&mut s, "user", reply("Ok")).unwrap_err().contains("only the managers it went to reply"));
    // A reply to it asks nothing to choose: a question goes in the manager's own workspace.
    let opt = |v: &str| ChoiceOption { value: v.into(), label: v.into() };
    let choice = serde_json::to_value(Choice::new(&[opt("a"), opt("b")], &[])).unwrap();
    let asking = json!({"kind": "message.replied", "body": {"message": "m1", "text": "Which?", "choice": choice}});
    assert!(add(&mut s, "boss", asking).unwrap_err().contains("asks nothing to choose"));
    add(&mut s, "boss", reply("chat: nothing running.")).unwrap();
    s.append(&remote, crate::records::tests::body(reply("api: two worktrees removed."))).unwrap();
    let m = s.state().message("m1").unwrap();
    assert_eq!(m.replies.iter().map(|r| r.by.as_str()).collect::<Vec<_>>(), ["boss", "far"]);

    // A message to one manager keeps its own rules: its delivery names no copy.
    add(&mut s, "user", json!({"kind": "message.sent", "body": {"workspace": "ws1", "text": "Only you."}})).unwrap();
    let one = |manager: &str| json!({"kind": "message.delivery", "body": {"id": "m2", "manager": manager, "step": "delivered", "to": "boss", "reason": null, "message": "..."}});
    assert!(add(&mut s, "workriver", one("boss")).unwrap_err().contains("message m2 went to one manager"));
    add(&mut s, "workriver", one("")).unwrap();
    assert_eq!(s.state().message("m2").unwrap().delivery.as_ref().map(|d| d.step), Some(DeliveryStep::Delivered));

    // Its conversation is read like a workspace's, under no workspace's id.
    add(&mut s, "user", json!({"kind": "messages.read", "body": {"workspace": "", "at": "2026-11-21T10:00:00Z"}})).unwrap();
    assert_eq!(s.state().read[""], "2026-11-21T10:00:00Z");
    assert!(add(&mut s, "user", json!({"kind": "messages.read", "body": {"workspace": " ", "at": "2026-11-21T10:00:00Z"}})).unwrap_err().contains("no workspace"));
    assert_eq!(replayed(&s), *s.state());
}

/// A copy to every manager belongs to the appointment it went to: once the seat is another's
/// (or the same session's, seated anew), the earlier copy takes no reply, and the new
/// manager gets a copy only of what is sent from then on.
#[test]
fn a_copy_belongs_to_the_appointment_it_went_to() {
    let mut s = Records::default();
    let (w1, w2) = (workspace(&mut s, "chat", "/r"), workspace(&mut s, "web", "/r"));
    seat(&mut s, &w1, "boss");
    seat(&mut s, &w2, "web");
    let runs = |session: &str, machine: Option<&str>| crate::runtime::RuntimeAgent {
        session: session.into(),
        harness: "claude".into(),
        state: crate::runtime::AgentState::Idle,
        cwd: String::new(),
        group: None,
        name: session.into(),
        topic: None,
        machine: machine.map(str::to_string),
    };
    let live = [runs("boss", None), runs("web", None), runs("next", None)];
    let say = |to: &[crate::Recipient]| json!({"kind": "message.sent", "body": {"text": "I shut down at six.", "to": to}});
    let every = s.state().managers_among(&live);
    add(&mut s, "user", say(&every)).unwrap();
    let reply = json!({"kind": "message.replied", "body": {"message": "m1", "text": "Ok"}});
    add(&mut s, "boss", reply.clone()).unwrap();
    // web's seat goes to another session: the copy was web's appointment's, nobody's now.
    seat(&mut s, &w2, "next");
    assert!(add(&mut s, "web", reply.clone()).unwrap_err().contains("went to an earlier manager of workspace ws2"));
    assert!(add(&mut s, "next", reply.clone()).unwrap_err().contains("only the managers it went to reply"));
    // boss seated anew in another folder: a new appointment, the same session.
    seat_at(&mut s, &w1, Some("boss"), None, "/elsewhere").unwrap();
    assert!(add(&mut s, "boss", reply.clone()).unwrap_err().contains("went to an earlier manager of workspace ws1"));
    // A copy as it was listed before the seats changed is refused; listed again, it goes.
    assert!(add(&mut s, "user", say(&every)).unwrap_err().contains("does not hold the seat"));
    let now = s.state().managers_among(&live);
    assert_eq!(now.iter().map(|r| r.manager.as_str()).collect::<Vec<_>>(), ["boss", "next"]);
    add(&mut s, "user", say(&now)).unwrap();
    add(&mut s, "next", json!({"kind": "message.replied", "body": {"message": "m2", "text": "Ok"}})).unwrap();
    assert_eq!(replayed(&s), *s.state());
}

/// A delivered design or research is discussed by the user with its own agent, in one thread
/// per task, until the gate that asks about it is submitted.
#[test]
fn discussions_are_the_users_with_the_tasks_own_agent() {
    let mut s = Records::default();
    seeded(&mut s);
    let ok = |s: &mut Records, actor: &str, v: serde_json::Value| add(s, actor, v).unwrap();
    let says = |s: &mut Records, actor: &str, v: serde_json::Value, what: &str| {
        let e = add(s, actor, v).unwrap_err();
        assert!(e.contains(what), "{e:?} does not say {what:?}");
    };
    let task = |kind: &str, inputs: &[&str], more: serde_json::Value| {
        let mut b = json!({"unit": "u1", "kind": kind, "title": kind, "inputs": inputs.iter().map(|f| json!({"from": f})).collect::<Vec<_>>()});
        b.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        json!({"kind": "task.created", "body": b})
    };
    ok(&mut s, "boss", task("design", &["2"], json!({}))); // 3
    ok(&mut s, "boss", task("review", &["3"], json!({"reviewer": "architecture-review"}))); // 4
    ok(&mut s, "boss", task("gate", &["4"], json!({"gate": "decide"}))); // 5: confirms design 3
    ok(&mut s, "designer", json!({"kind": "task.started", "body": {"unit": "u1", "task": 3}}));
    let discuss = |text: &str, more: serde_json::Value| {
        let mut b = json!({"unit": "u1", "task": 3, "text": text, "discuss": true});
        b.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        json!({"kind": "question.asked", "body": b})
    };
    // Only a delivered design or research, and only the user.
    says(&mut s, "user", discuss("Why?", json!({})), "is not delivered");
    ok(&mut s, "designer", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 3, "note": "Use a trie."}}));
    says(&mut s, "boss", discuss("Why?", json!({})), "only the user discusses");
    says(&mut s, "user", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": "Why?", "discuss": true}}), "research or design");
    says(&mut s, "user", discuss("Sum it up", json!({"conclude": true})), "nothing to conclude");

    // The first question opens the discussion; the next joins it with no thread named.
    ok(&mut s, "user", discuss("Why a trie?", json!({}))); // q1
    ok(&mut s, "user", discuss("And memory?", json!({}))); // q2
    let st = s.state().clone();
    assert_eq!(st.questions.iter().map(|q| (q.id.as_str(), q.thread.as_str(), q.discuss)).collect::<Vec<_>>(), [("q1", "q1", true), ("q2", "q1", true)]);
    assert_eq!(st.threads.iter().map(|t| (t.id.as_str(), t.discuss)).collect::<Vec<_>>(), [("q1", true)]);
    // Another thread of the task is not the discussion, and an agent's question cannot join it.
    ok(&mut s, "agent-a", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 3, "text": "Which trie?"}})); // q3, its own thread
    says(&mut s, "user", discuss("x", json!({"thread": "q3"})), "not the discussion");
    says(&mut s, "agent-a", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 3, "thread": "q1", "text": "x"}}), "is a discussion");
    // Nobody ends a discussion.
    says(&mut s, "user", json!({"kind": "thread.ended", "body": {"thread": "q1"}}), "is not ended");

    // Only the task's own agent answers it: not the user, not another agent.
    says(&mut s, "user", json!({"kind": "question.answered", "body": {"question": "q1", "text": "x"}}), "answered by the agent of u1#3");
    says(&mut s, "agent-a", json!({"kind": "question.answered", "body": {"question": "q1", "text": "x"}}), "answered by the agent of u1#3");
    ok(&mut s, "designer", json!({"kind": "question.answered", "body": {"question": "q1", "text": "Fast prefix lookups."}}));
    ok(&mut s, "user", discuss("Sum it up", json!({"conclude": true}))); // q4
    assert_eq!(s.state().question("q4").unwrap().thread, "q1");

    // The user has read it up to a time: user-only, stored per task.
    says(&mut s, "designer", json!({"kind": "questions.read", "body": {"unit": "u1", "task": 3, "at": "2026-01-01T00:00:00Z"}}), "only the user does this");
    ok(&mut s, "user", json!({"kind": "questions.read", "body": {"unit": "u1", "task": 3, "at": "2026-01-01T00:00:00Z"}}));
    assert_eq!(s.state().questions_read.get("u1#3").map(String::as_str), Some("2026-01-01T00:00:00Z"));

    // Once the gate that asks about it is submitted: no more questions, no late answers.
    ok(&mut s, "arch", json!({"kind": "task.started", "body": {"unit": "u1", "task": 4}}));
    ok(&mut s, "arch", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 4, "findings": []}}));
    ok(&mut s, "user", json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 5, "result": "done"}}));
    says(&mut s, "user", discuss("One more", json!({})), "u1#5 was submitted");
    says(&mut s, "designer", json!({"kind": "question.answered", "body": {"question": "q2", "text": "late", "conclusion": false}}), "u1#5 was submitted");

    // Old records replay as they were: a question without the flag is no discussion.
    let state = replayed(&s);
    assert_eq!(&state, s.state());
}

/// That a question failed is workriver's receipt: nobody else records it, so no agent can
/// close the user's question (a discussion's or any other) in workriver's words.
#[test]
fn only_workriver_fails_a_question() {
    let mut s = Records::default();
    seeded(&mut s);
    add(&mut s, "user", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": "Why?"}})).unwrap();
    let failed = json!({"kind": "question.failed", "body": {"question": "q1", "reason_code": "gate-submitted", "reason": "its gate u1#2 was submitted"}});
    for who in ["agent-x", "agent-a", "user"] {
        let e = add(&mut s, who, failed.clone()).unwrap_err();
        assert!(e.contains("only workriver records this (question.failed)"), "{who}: {e}");
    }
    add(&mut s, "workriver", failed).unwrap();
}

/// Whether a result was amended while it was discussed, in event order: an amendment before
/// the first question does not count, one after it does; never written out with the state.
#[test]
fn a_discussion_knows_its_result_was_amended() {
    let mut s = Records::default();
    seeded(&mut s);
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "research", "title": "Survey", "inputs": [{"from": "start"}]}})).unwrap(); // 3
    add(&mut s, "r", json!({"kind": "task.started", "body": {"unit": "u1", "task": 3}})).unwrap();
    add(&mut s, "r", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 3, "note": "v1"}})).unwrap();
    add(&mut s, "r", json!({"kind": "task.amended", "body": {"unit": "u1", "task": 3, "note": "v2"}})).unwrap();
    add(&mut s, "user", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 3, "text": "Why?", "discuss": true}})).unwrap();
    assert!(!s.state().discussion("u1", 3).unwrap().amended);
    add(&mut s, "r", json!({"kind": "task.amended", "body": {"unit": "u1", "task": 3, "note": "v3"}})).unwrap();
    assert!(s.state().discussion("u1", 3).unwrap().amended);
    assert!(replayed(&s).discussion("u1", 3).unwrap().amended);
    assert!(serde_json::to_value(s.state()).unwrap()["threads"][0].get("amended").is_none());
}

/// A manager's message or reply may carry options; the user answers with a message of their own
/// that names it and the value picked: once, to the manager who asked, a note where the option
/// asks one. Nothing else follows from it. A question of an earlier manager is closed.
#[test]
fn the_user_answers_a_managers_choice_with_a_message() {
    let mut s = Records::default();
    let w1 = workspace(&mut s, "chat", "/r");
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Mentions", "state": "planned"}})).unwrap();
    seat(&mut s, &w1, "boss");
    add(&mut s, "user", start("i1", &w1)).unwrap();
    let opt = |v: &str, l: &str| ChoiceOption { value: v.into(), label: l.into() };
    let choice = serde_json::to_value(Choice::new(&[opt("cursor", "By cursor"), opt("page", "By page")], &[])).unwrap();
    // Asked with `say` (m1) and in a reply to the user's message (m2, its first reply).
    let mut bad = choice.clone();
    bad["surface"]["version"] = json!("0.9");
    assert!(add(&mut s, "boss", json!({"kind": "message.said", "body": {"text": "Which paging?", "choice": bad}})).unwrap_err().contains("--option"));
    add(&mut s, "boss", json!({"kind": "message.said", "body": {"text": "Which paging?", "choice": choice}})).unwrap();
    add(&mut s, "user", json!({"kind": "message.sent", "body": {"workspace": "ws1", "text": "How do we page?"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "message.replied", "body": {"message": "m2", "text": "Pick one:", "choice": choice}})).unwrap();
    assert!(s.state().message("m1").unwrap().choice.is_some() && s.state().message("m2").unwrap().replies[0].choice.is_some());

    let answer = |message: &str, reply: u32, selected: &[&str], text: &str| {
        json!({"kind": "message.sent", "body": {"workspace": "ws1", "text": text, "answer": {"message": message, "reply": reply, "selected": selected}}})
    };
    let refused = |s: &mut Records, v: serde_json::Value| add(s, "user", v).unwrap_err();
    assert!(add(&mut s, "boss", answer("m1", 0, &["page"], "")).unwrap_err().contains("only the user"));
    assert!(refused(&mut s, answer("m9", 0, &["page"], "")).contains("no message m9"));
    assert!(refused(&mut s, answer("m2", 0, &["page"], "")).contains("m2 asks nothing to choose"));
    assert!(refused(&mut s, answer("m2", 2, &["page"], "")).contains("m2 has no reply 2"));
    assert!(refused(&mut s, answer("m1", 0, &["pages"], "")).contains("not one of"));
    assert!(refused(&mut s, answer("m1", 0, &["other"], " ")).contains("needs a note"));
    // An answer needs no text of its own; it is recorded on the question, once.
    let e = add(&mut s, "user", answer("m1", 0, &["page"], "")).unwrap();
    let Body::MessageSent { id, manager, .. } = &e.body else { panic!() };
    assert_eq!((id.as_str(), manager.as_str()), ("m3", "boss"));
    let asked = s.state().message("m1").unwrap().answered.clone().unwrap();
    assert_eq!((asked.by.as_str(), asked.selected.clone()), ("m3", vec!["page".to_string()]));
    assert!(refused(&mut s, answer("m1", 0, &["cursor"], "")).contains("m1 is answered already"));
    add(&mut s, "user", answer("m2", 1, &["other"], "keyset paging")).unwrap();
    assert_eq!(s.state().message("m2").unwrap().replies[0].answered.as_ref().map(|a| a.by.as_str()), Some("m4"));
    // A plain message still needs its text.
    assert!(refused(&mut s, json!({"kind": "message.sent", "body": {"workspace": "ws1", "text": " "}})).contains("needs its text"));

    // Another manager seated: the earlier one's open question is closed.
    add(&mut s, "boss", json!({"kind": "message.said", "body": {"text": "And sorting?", "choice": choice}})).unwrap(); // m5
    assert!(s.state().choice_open("m5", 0));
    assert!(!s.state().choice_open("m1", 0), "answered");
    seat(&mut s, &w1, "boss2");
    assert!(!s.state().choice_open("m5", 0));
    assert!(refused(&mut s, answer("m5", 0, &["page"], "")).contains("asked by an earlier manager"));
    let replayed = replayed(&s);
    assert_eq!(serde_json::to_value(&replayed).unwrap(), serde_json::to_value(s.state()).unwrap());
    // A message with no choice is written as before.
    let plain = serde_json::to_value(replayed.message("m2").unwrap()).unwrap();
    assert!(plain.get("choice").is_none() && plain.get("answered").is_none());
}

/// A manager on another machine, in a workspace of the same name as one here with its own
/// manager, asks with options: the question is open while it holds that seat, and the user's
/// answer goes to it, whatever workspace the page names; not to the manager of the other
/// workspace of that name.
#[test]
fn a_far_managers_choice_is_answered_to_it() {
    let mut s = Records::default();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Here", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Far", "state": "planned"}})).unwrap();
    let (w1, w2) = (workspace(&mut s, "chat", "/r"), workspace(&mut s, "chat", "/r"));
    seat(&mut s, &w1, "boss");
    add(&mut s, "user", start("i1", &w1)).unwrap();
    let far = Principal::Agent { session: "far".into(), machine: Some("box".into()) };
    let as_far = |s: &mut Records, v: serde_json::Value| s.append(&far, crate::records::tests::body(v));
    seat_at(&mut s, &w2, Some("far"), Some("box"), "/home/far").unwrap();
    add(&mut s, "user", start("i2", &w2)).unwrap();
    let opt = |v: &str, l: &str| ChoiceOption { value: v.into(), label: l.into() };
    let choice = serde_json::to_value(Choice::new(&[opt("a", "A"), opt("b", "B")], &[])).unwrap();
    as_far(&mut s, json!({"kind": "message.said", "body": {"text": "Which?", "choice": choice}})).unwrap();
    assert!(s.state().choice_open("m1", 0), "its manager still holds the seat");
    assert_eq!(s.state().message("m1").unwrap().workspace, "ws2");
    // Whatever workspace the request names, the answer goes to the asker, in its workspace.
    let e = add(&mut s, "user", json!({"kind": "message.sent", "body": {"workspace": "ws1", "text": "", "answer": {"message": "m1", "selected": ["a"]}}})).unwrap();
    let Body::MessageSent { manager, intent, workspace, .. } = &e.body else { panic!() };
    assert_eq!((manager.as_str(), intent.as_str(), workspace.as_str()), ("far", "i2", "ws2"));
    assert!(!s.state().choice_open("m1", 0));
    // By intent, it goes to the asker too.
    as_far(&mut s, json!({"kind": "message.said", "body": {"text": "And?", "choice": choice}})).unwrap();
    let e = add(&mut s, "user", json!({"kind": "message.sent", "body": {"intent": "i2", "text": "", "answer": {"message": "m3", "selected": ["b"]}}})).unwrap();
    let Body::MessageSent { manager, .. } = &e.body else { panic!() };
    assert_eq!(manager, "far");
    assert_eq!(replayed(&s), *s.state());
}

/// A choice in a conversation, for the tests below.
fn options() -> serde_json::Value {
    let opt = |v: &str| ChoiceOption { value: v.into(), label: v.to_uppercase() };
    serde_json::to_value(Choice::new(&[opt("a"), opt("b")], &[])).unwrap()
}

/// Two workspaces of one name are two workspaces: two conversations, two sets of read
/// marks, and neither manager sees the other's in what it reads.
#[test]
fn two_workspaces_of_one_name_have_two_conversations() {
    let mut s = Records::default();
    let (w1, w2) = (workspace(&mut s, "chat", "/r"), workspace(&mut s, "chat", "/r"));
    seat(&mut s, &w1, "s-a");
    seat(&mut s, &w2, "s-b");
    add(&mut s, "user", json!({"kind": "message.sent", "body": {"workspace": w1, "text": "SECRET_TO_A"}})).unwrap();
    add(&mut s, "s-a", json!({"kind": "message.said", "body": {"text": "SECRET_FROM_A"}})).unwrap();
    add(&mut s, "s-a", json!({"kind": "message.replied", "body": {"message": "m1", "text": "REPLY_FROM_A"}})).unwrap();
    add(&mut s, "user", json!({"kind": "message.sent", "body": {"workspace": w2, "text": "SECRET_TO_B"}})).unwrap();
    add(&mut s, "user", json!({"kind": "messages.read", "body": {"workspace": w1, "at": "2026-11-22T10:00:00Z"}})).unwrap();
    add(&mut s, "user", json!({"kind": "messages.read", "body": {"workspace": w2, "at": "2026-11-22T11:00:00Z"}})).unwrap();
    let st = s.state().clone();
    assert_eq!(st.messages.iter().map(|m| m.workspace.as_str()).collect::<Vec<_>>(), ["ws1", "ws1", "ws2"]);
    assert_eq!((st.read["ws1"].as_str(), st.read["ws2"].as_str()), ("2026-11-22T10:00:00Z", "2026-11-22T11:00:00Z"));
    // B replies to nothing of A's.
    assert!(add(&mut s, "s-b", json!({"kind": "message.replied", "body": {"message": "m1", "text": "x"}})).unwrap_err().contains("only that manager replies"));
    let seen = |s: &Records, who: &str| serde_json::to_string(&s.state().clone().conversations_of(who)).unwrap();
    let (a, b, worker) = (seen(&s, "s-a"), seen(&s, "s-b"), seen(&s, "worker"));
    for secret in ["SECRET_TO_A", "SECRET_FROM_A", "REPLY_FROM_A", "2026-11-22T10:00:00Z"] {
        assert!(a.contains(secret) && !b.contains(secret) && !worker.contains(secret), "{secret}");
    }
    for secret in ["SECRET_TO_B", "2026-11-22T11:00:00Z"] {
        assert!(b.contains(secret) && !a.contains(secret) && !worker.contains(secret), "{secret}");
    }
    // The user's whole state holds both; the rest of the records is every agent's.
    let v = s.state().clone().conversations_of("s-b");
    assert_eq!((v.workspaces.clone(), v.projects.clone(), v.agents.clone()), (st.workspaces.clone(), st.projects.clone(), st.agents.clone()));
    assert_eq!(v.messages.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["m3"]);
    // A manager no longer seated reads none of it; the one seated after it reads the history.
    seat(&mut s, &w1, "s-next");
    assert!(!seen(&s, "s-a").contains("SECRET_TO_A") && seen(&s, "s-next").contains("SECRET_TO_A") && seen(&s, "s-next").contains("REPLY_FROM_A"));
}

/// A rename keeps a question open: the workspace is its id, and its name decides nothing.
#[test]
fn a_rename_keeps_a_question_open() {
    let mut s = Records::default();
    let w1 = workspace(&mut s, "chat", "/r");
    seat(&mut s, &w1, "boss");
    add(&mut s, "boss", json!({"kind": "message.said", "body": {"text": "Which?", "choice": options()}})).unwrap();
    add(&mut s, "user", json!({"kind": "workspace.renamed", "body": {"workspace": w1, "name": "talk"}})).unwrap();
    workspace(&mut s, "chat", "/r");
    assert!(s.state().choice_open("m1", 0));
    let e = add(&mut s, "user", json!({"kind": "message.sent", "body": {"workspace": w1, "text": "", "answer": {"message": "m1", "selected": ["a"]}}})).unwrap();
    let Body::MessageSent { workspace, manager, .. } = &e.body else { panic!() };
    assert_eq!((workspace.as_str(), manager.as_str()), ("ws1", "boss"));
    assert_eq!(s.state().message("m1").unwrap().answered.as_ref().map(|a| a.selected.clone()), Some(vec!["a".to_string()]));
}

/// A choice belongs to the appointment that asked it. The same session seated again where
/// it was (the same machine and folder) keeps it open; seated in another folder, on another
/// machine, or another session seated: the earlier appointment's choice is closed, its answer
/// refused, and its message takes no reply.
#[test]
fn a_choice_belongs_to_the_appointment_that_asked_it() {
    let asked = |folder: &str, machine: Option<&str>| {
        let mut s = Records::default();
        let w1 = workspace(&mut s, "chat", "/r");
        seat_at(&mut s, &w1, Some("boss"), machine, folder).unwrap();
        add(&mut s, "boss", json!({"kind": "message.said", "body": {"text": "Which?", "choice": options()}})).unwrap(); // m1
        add(&mut s, "user", json!({"kind": "message.sent", "body": {"workspace": "ws1", "text": "And the plan?"}})).unwrap(); // m2
        add(&mut s, "boss", json!({"kind": "message.replied", "body": {"message": "m2", "text": "Pick:", "choice": options()}})).unwrap();
        s
    };
    let answer = |m: &str, reply: u32| json!({"kind": "message.sent", "body": {"workspace": "ws1", "text": "", "answer": {"message": m, "reply": reply, "selected": ["a"]}}});
    let reply = json!({"kind": "message.replied", "body": {"message": "m2", "text": "More."}});
    // Seated again where it was: the same appointment.
    let mut s = asked("/w", Some("box"));
    seat_at(&mut s, "ws1", Some("boss"), Some("box"), "/w").unwrap();
    assert!(s.state().choice_open("m1", 0) && s.state().choice_open("m2", 1));
    add(&mut s, "boss", reply.clone()).unwrap();
    add(&mut s, "user", answer("m1", 0)).unwrap();
    add(&mut s, "user", answer("m2", 1)).unwrap();
    // Anything else: a new appointment.
    for (session, machine, folder) in [("boss", Some("box"), "/elsewhere"), ("boss", None, "/w"), ("next", Some("box"), "/w")] {
        let mut s = asked("/w", Some("box"));
        seat_at(&mut s, "ws1", Some(session), machine, folder).unwrap();
        assert!(!s.state().choice_open("m1", 0) && !s.state().choice_open("m2", 1), "{session} {machine:?} {folder}");
        for (m, k) in [("m1", 0), ("m2", 1)] {
            assert!(add(&mut s, "user", answer(m, k)).unwrap_err().contains("asked by an earlier manager"), "{session} {m}");
        }
        let e = add(&mut s, "boss", reply.clone()).unwrap_err();
        assert!(e.contains("went to an earlier manager of workspace ws1"), "{e}");
        assert!(add(&mut s, "next", reply.clone()).is_err(), "nor does a new manager reply for it");
        // What the new appointment asks is open, and its own.
        add(&mut s, session, json!({"kind": "message.said", "body": {"text": "Again: which?", "choice": options()}})).unwrap();
        let m = s.state().messages.last().unwrap();
        assert!(s.state().choice_open(&m.id, 0) && m.since == s.state().workspace("ws1").unwrap().manager.as_ref().unwrap().since);
        assert_eq!(replayed(&s), *s.state());
    }
    // The seat emptied, or the workspace closed: closed too.
    let mut s = asked("/w", None);
    seat_at(&mut s, "ws1", None, None, "").unwrap();
    assert!(!s.state().choice_open("m1", 0));
    assert!(add(&mut s, "boss", json!({"kind": "message.said", "body": {"text": "x"}})).unwrap_err().contains("only a workspace's manager"));
}

/// A message to every manager is one record; what a manager reads of it is the message, its
/// own copy and its own replies: nothing of another manager's copy (its workspace, session,
/// delivery) or replies. Only a manager it went to, as the appointment it went to, reads it.
#[test]
fn a_manager_reads_only_its_own_copy_of_a_message_to_every_manager() {
    let mut s = Records::default();
    let (w1, w2, w3) = (workspace(&mut s, "chat", "/r"), workspace(&mut s, "web", "/r"), workspace(&mut s, "late", "/r"));
    seat(&mut s, &w1, "s-a");
    seat_at(&mut s, &w2, Some("s-b"), Some("box-b"), "/home/b").unwrap();
    add(&mut s, "user", json!({"kind": "attachment.added", "body": {"id": "00000000000000e1", "name": "plan.png", "type": "image/png", "size": 3}})).unwrap();
    let runs = |session: &str, machine: Option<&str>| crate::runtime::RuntimeAgent { session: session.into(), harness: "claude".into(), state: crate::runtime::AgentState::Idle, cwd: String::new(), group: None, name: session.into(), topic: None, machine: machine.map(str::to_string) };
    let to = s.state().managers_among(&[runs("s-a", None), runs("s-b", Some("box-b"))]);
    add(&mut s, "user", json!({"kind": "message.sent", "body": {"text": "TO_EVERYONE", "to": to, "context": [{"kind": "attachment", "label": "plan.png", "id": "00000000000000e1"}]}})).unwrap();
    let b = Principal::Agent { session: "s-b".into(), machine: Some("box-b".into()) };
    add(&mut s, "s-a", json!({"kind": "message.replied", "body": {"message": "m1", "text": "REPLY_FROM_A"}})).unwrap();
    s.append(&b, crate::records::tests::body(json!({"kind": "message.replied", "body": {"message": "m1", "text": "REPLY_FROM_B"}}))).unwrap();
    add(&mut s, "s-a", json!({"kind": "message.replied", "body": {"message": "m1", "text": "MORE_FROM_A"}})).unwrap();
    for manager in ["s-a", "s-b"] {
        add(&mut s, "workriver", json!({"kind": "message.delivery", "body": {"id": "m1", "manager": manager, "step": "delivered", "to": manager, "reason": null, "message": format!("TYPED_TO_{manager}")}})).unwrap();
    }
    add(&mut s, "user", json!({"kind": "messages.read", "body": {"workspace": "", "at": "2026-11-22T10:00:00Z"}})).unwrap();
    seat(&mut s, &w3, "s-late");
    assert_eq!(s.state().message("m1").unwrap().replies.len(), 3, "the user's record holds every reply");

    let a = s.state().clone().conversations_of("s-a");
    let m = &a.messages[0];
    assert_eq!((m.text.as_str(), m.context.len(), a.messages.len()), ("TO_EVERYONE", 1, 1));
    assert_eq!(m.to.iter().map(|r| (r.manager.as_str(), r.workspace.as_str())).collect::<Vec<_>>(), [("s-a", "ws1")]);
    assert_eq!(m.replies.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["REPLY_FROM_A", "MORE_FROM_A"]);
    let text = serde_json::to_string(&a.messages).unwrap();
    for hidden in ["REPLY_FROM_B", "TYPED_TO_s-b", "box-b", "ws2", "s-b"] {
        assert!(!text.contains(hidden), "{hidden}: {text}");
    }
    assert!(a.read.is_empty(), "the read mark of the conversation with every manager is the user's");
    assert!(a.attachments.iter().any(|x| x.id == "00000000000000e1"), "the user's file went with its copy");
    let b = s.state().clone().conversations_of("s-b");
    assert_eq!(b.messages[0].replies.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["REPLY_FROM_B"]);
    assert_eq!(b.messages[0].to.iter().map(|r| r.manager.as_str()).collect::<Vec<_>>(), ["s-b"]);
    // Nobody else: a worker, a manager seated after it was sent, or the same session under
    // another appointment.
    for who in ["worker", "s-late"] {
        let v = s.state().clone().conversations_of(who);
        assert!(v.messages.is_empty() && v.attachments.is_empty(), "{who}");
    }
    seat_at(&mut s, &w1, Some("s-a"), None, "/elsewhere").unwrap();
    assert!(s.state().clone().conversations_of("s-a").messages.is_empty(), "a new appointment is not the one it went to");
    seat(&mut s, &w1, "s-next");
    assert!(s.state().clone().conversations_of("s-next").messages.is_empty(), "nor a new manager of its workspace");
}

/// What an agent reads leaves out an attachment only when every record that names it is a
/// part cut away: one a task it reads names too stays, and so does one it stored itself and
/// one nothing names. Everything else is as the whole state has it.
#[test]
fn an_attachment_goes_only_when_all_that_names_it_is_cut() {
    let mut s = Records::default();
    seeded(&mut s);
    managed(&mut s, "s-a", "i1");
    let w2 = workspace(&mut s, "web", "/r");
    seat(&mut s, &w2, "s-b");
    for (who, id) in [("user", "00000000000000a1"), ("user", "00000000000000a2"), ("user", "00000000000000a3"), ("user", "00000000000000a4"), ("s-b", "00000000000000b1"), ("agent-a", "00000000000000c1")] {
        add(&mut s, who, json!({"kind": "attachment.added", "body": {"id": id, "name": "f.png", "type": "image/png", "size": 3}})).unwrap();
    }
    let file = |id: &str| json!({"kind": "attachment", "label": "f.png", "id": id});
    // To A only: a1 (nothing else names it), a2 (a task's note names it too), b1 (B stored it).
    add(&mut s, "user", json!({"kind": "message.sent", "body": {"workspace": "ws1", "text": "For A", "context": [file("00000000000000a1"), file("00000000000000a2")]}})).unwrap();
    add(&mut s, "user", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "More", "inputs": [{"from": "2"}]}})).unwrap();
    add(&mut s, "user", json!({"kind": "task.noted", "body": {"unit": "u1", "task": 3, "text": "See the file", "attachments": ["00000000000000a2"]}})).unwrap();
    // a3: sent to A and to B.
    add(&mut s, "user", json!({"kind": "message.sent", "body": {"workspace": "ws1", "text": "For A", "context": [file("00000000000000a3")]}})).unwrap();
    add(&mut s, "user", json!({"kind": "message.sent", "body": {"workspace": w2, "text": "For B", "context": [file("00000000000000a3")]}})).unwrap();
    let ids = |who: &str| s.state().clone().conversations_of(who).attachments.iter().map(|a| a.id[14..].to_string()).collect::<Vec<_>>();
    assert_eq!(ids("s-a"), ["a1", "a2", "a3", "a4", "b1", "c1"], "its own conversation: everything");
    assert_eq!(ids("s-b"), ["a2", "a3", "a4", "b1", "c1"], "a1 was only in A's conversation");
    assert_eq!(ids("worker"), ["a2", "a4", "b1", "c1"], "a3 was only in conversations");
    // The rest of what a worker reads is the whole state's.
    let v = s.state().clone().conversations_of("worker");
    let whole = s.state();
    assert_eq!((&v.units, &v.intents, &v.questions, &v.workspaces, &v.backlog), (&whole.units, &whole.intents, &whole.questions, &whole.workspaces, &whole.backlog));
    assert!(v.messages.is_empty() && v.read.is_empty() && !v.reduced);
}
