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
    let unknown = s.append(&"user".into(), body(json!({"kind": "task.amended", "body": {"unit": "u1", "task": 1, "attachments": ["abc"]}}))).unwrap_err().to_string();
    assert!(unknown.contains("no stored attachment abc"), "{unknown}");
    s.append(&"agent-a".into(), body(json!({"kind": "attachment.added", "body": {"id": "abc", "name": "a.png", "type": "image/png", "size": 3}}))).unwrap();
    s.append(&"agent-a".into(), body(json!({"kind": "task.amended", "body": {"unit": "u1", "task": 1, "attachments": ["abc"]}}))).unwrap();
    // Added again only to link it to a finding: one record, two links.
    s.append(&"agent-a".into(), body(json!({"kind": "attachment.added", "body": {"id": "abc", "name": "a.png", "type": "image/png", "size": 3, "unit": "u1", "task": 2, "finding": "P2-1"}}))).unwrap();
    assert!(s.append(&"agent-a".into(), body(json!({"kind": "attachment.added", "body": {"id": "x", "name": "x", "type": "", "size": 1, "unit": "u1", "task": 2, "finding": "P9-9"}}))).is_err());
    assert_eq!(s.state().attachments.len(), 1);
    assert_eq!(s.state().task("u1", 1).unwrap().attachments, ["abc"]);
    assert_eq!(s.state().task("u1", 2).unwrap().findings[0].found.attachments, ["abc"]);
}

/// A question may point at an attachment shown with its task (its own, its findings', those of
/// the findings it answers), never at another's; a blind task not delivered, not even at those
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
    add(&mut s, "agent-a", file("own", json!({"unit": "u1", "task": 1}))).unwrap();
    add(&mut s, "agent-a", file("found", json!({"unit": "u1", "task": 2, "finding": "P2-1"}))).unwrap();
    add(&mut s, "agent-a", file("loose", json!({}))).unwrap();
    let ask = |task: u32, id: Option<&str>| json!({"kind": "question.asked", "body": {"unit": "u1", "task": task, "text": "What does it say?", "context": [{"kind": "attachment", "label": "report.md", "id": id}]}});
    add(&mut s, "user", ask(1, Some("own"))).unwrap();
    assert_eq!(s.state().questions[0].context[0].id.as_deref(), Some("own"));
    for wrong in [Some("found"), Some("loose"), Some("nope"), None] {
        assert!(add(&mut s, "user", ask(1, wrong)).unwrap_err().contains("is not one of u1#1's"), "{wrong:?}");
    }
    add(&mut s, "user", ask(2, Some("found"))).unwrap();
    // Work that answers a finding shows that finding's attachments.
    add(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "accept"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Dedupe", "causes": [{"task": 2, "findings": ["P2-1"]}]}})).unwrap();
    add(&mut s, "user", ask(3, Some("found"))).unwrap();
    assert_eq!(s.state().attachments_of("u1", 3), ["found"]);
    // A blind review in progress: nothing of other work, not even through the user's question.
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "reviewer": "security-review", "inputs": [{"from": "3"}], "causes": [{"task": 2, "findings": ["P2-1"]}], "agent": "sec", "blind": true}})).unwrap();
    assert!(s.state().attachments_of("u1", 4).is_empty());
    assert!(add(&mut s, "user", ask(4, Some("found"))).unwrap_err().contains("is not one of u1#4's"));
    add(&mut s, "sec", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 4, "findings": []}})).unwrap();
    add(&mut s, "user", ask(4, Some("found"))).unwrap();
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
    add(&mut s, "user", file("mine")).unwrap();
    add(&mut s, "agent-a", file("loose")).unwrap();
    let ask = |task: u32, id: &str| json!({"kind": "question.asked", "body": {"unit": "u1", "task": task, "text": "What does it say?", "context": [{"kind": "attachment", "label": "x.log", "id": id}]}});
    add(&mut s, "user", ask(1, "mine")).unwrap();
    add(&mut s, "user", ask(2, "mine")).unwrap();
    add(&mut s, "user", ask(0, "mine")).unwrap();
    assert!(add(&mut s, "agent-a", ask(1, "mine")).unwrap_err().contains("is not one of u1#1's"), "an agent's question");
    assert!(add(&mut s, "user", ask(1, "loose")).unwrap_err().contains("is not one of u1#1's"), "stored by an agent only");
    assert!(add(&mut s, "user", ask(1, "never")).unwrap_err().contains("is not one of u1#1's"), "never stored");
    // The same content stored by an agent first (one file per content): the user's once they store it too.
    add(&mut s, "user", file("loose")).unwrap();
    add(&mut s, "user", ask(1, "loose")).unwrap();
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
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": "rep", "name": "r.md", "type": "text/markdown", "size": 1, "unit": "u1", "task": 2}})).unwrap();
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": "loose", "name": "x.md", "type": "text/markdown", "size": 1}})).unwrap();
    let at = |id: &str| json!({"kind": "question.asked", "body": {"unit": "u1", "task": 0, "text": "?", "context": [{"kind": "attachment", "label": "r.md", "id": id}]}});
    add(&mut s, "user", at("rep")).unwrap();
    assert!(add(&mut s, "user", at("loose")).unwrap_err().contains("not one of u1#0's"));
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
    let w1 = json!({"id": "w1", "label": "chat"});
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Mentions", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Search", "state": "planned"}})).unwrap();
    let say = |intent: &str, workspace: &str| json!({"kind": "message.sent", "body": {"intent": intent, "workspace": workspace, "manager": "forged", "text": "Pause u2 until the API settles."}});
    // Nobody to tell yet.
    assert!(add(&mut s, "user", say("i1", "")).unwrap_err().contains("intent i1 has no manager"));
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": w1}})).unwrap();
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": w1}})).unwrap();
    // Only the user writes; the manager is the workspace's, whatever the request says.
    assert!(add(&mut s, "boss", say("i1", "")).unwrap_err().contains("only the user does this (message.sent)"));
    let e = add(&mut s, "user", say("i1", "")).unwrap();
    let Body::MessageSent { id, workspace, manager, .. } = &e.body else { panic!() };
    assert_eq!((id.as_str(), workspace.as_str(), manager.as_str()), ("m1", "chat", "boss"));
    assert_eq!(e.body.refs().0.as_deref(), Some("i1"));
    // By workspace alone, when nothing of an intent is picked.
    add(&mut s, "user", say("", "chat")).unwrap();
    assert!(add(&mut s, "user", say("", "nowhere")).unwrap_err().contains("workspace \"nowhere\" has no manager"));
    assert!(add(&mut s, "user", say("i2", "")).unwrap_err().contains("intent i2 has no manager"), "planned: it runs nowhere");
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
    assert_eq!(replayed(&s), *s.state());
}

/// A workspace's manager starts a message to the user (into its workspace's conversation);
/// nobody else does. Only the user records reading a conversation; it only moves forward,
/// and it all replays.
#[test]
fn a_manager_says_and_the_user_reads() {
    let mut s = Records::default();
    let w1 = json!({"id": "w1", "label": "chat"});
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Mentions", "state": "planned"}})).unwrap();
    let say = |text: &str| json!({"kind": "message.said", "body": {"workspace": "forged", "text": text}});
    assert!(add(&mut s, "boss", say("Hi")).unwrap_err().contains("only a workspace's manager"), "not registered yet");
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": w1}})).unwrap();
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": w1}})).unwrap();
    // A worker is pointed to its manager; the user writes from the page; nothing blank.
    assert!(add(&mut s, "worker", say("Hi")).unwrap_err().contains("workriver manager tell"));
    assert!(add(&mut s, "user", say("Hi")).unwrap_err().contains("workriver's page"));
    assert!(add(&mut s, "boss", say("  ")).unwrap_err().contains("--text"));
    // Into the workspace it manages, about its current intent, whatever the request says.
    let e = add(&mut s, "boss", say("u1 is merged.")).unwrap();
    let Body::MessageSaid { id, intent, workspace, .. } = &e.body else { panic!() };
    assert_eq!((id.as_str(), intent.as_str(), workspace.as_str()), ("m1", "i1", "chat"));
    assert_eq!(e.body.refs().0.as_deref(), Some("i1"));
    let m = s.state().message("m1").unwrap();
    assert!(m.by_manager && m.delivery.is_none());
    assert_eq!((m.manager.as_str(), m.text.as_str()), ("boss", "u1 is merged."));
    // The user answers it from the same box: an ordinary message; the manager may follow up.
    add(&mut s, "user", json!({"kind": "message.sent", "body": {"workspace": "chat", "text": "Thanks."}})).unwrap();
    add(&mut s, "boss", json!({"kind": "message.replied", "body": {"message": "m1", "text": "Also u2."}})).unwrap();
    // Reading: the user's alone; a time, kept as the service writes times; never backwards.
    let read = |at: &str| json!({"kind": "messages.read", "body": {"workspace": "chat", "at": at}});
    assert!(add(&mut s, "boss", read("2026-11-18T10:00:00Z")).unwrap_err().contains("only the user does this (messages.read)"));
    assert!(add(&mut s, "user", read("soon")).unwrap_err().contains("not a time"));
    add(&mut s, "user", read("2026-11-18T10:00:05.250Z")).unwrap();
    assert_eq!(s.state().read["chat"], "2026-11-18T10:00:05Z");
    add(&mut s, "user", read("2026-11-18T09:00:00Z")).unwrap();
    assert_eq!(s.state().read["chat"], "2026-11-18T10:00:05Z");
    assert_eq!(replayed(&s), *s.state());
}

/// What the user picked goes with a message as records: a unit, one of its tasks, and context
/// (findings, files, lines, attachments, other tasks of the unit by number). The user's own
/// files need no unit; everything else does, and must be of the message's intent. Nothing
/// goes to a manager on a blind task. Messages recorded before replay unchanged.
#[test]
fn a_message_carries_what_the_user_picked() {
    let mut s = Records::default();
    let w1 = json!({"id": "w1", "label": "chat"});
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Chat v2", "state": "planned"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": w1}})).unwrap();
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": w1}})).unwrap();
    // The seed's unit and tasks, in that intent.
    for b in seed().into_iter().skip(1) {
        s.append(&"agent-a".into(), b).unwrap();
    }
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Search", "state": "planned"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "unit.created", "body": {"intent": "i2", "title": "Paging", "repo": "/r", "branch": "feat/paging", "base": "main"}})).unwrap();
    add(&mut s, "user", json!({"kind": "attachment.added", "body": {"id": "mine", "name": "shot.png", "type": "image/png", "size": 3}})).unwrap();
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": "rep", "name": "r.md", "type": "text/markdown", "size": 1, "unit": "u1", "task": 2}})).unwrap();
    add(&mut s, "agent-a", json!({"kind": "attachment.added", "body": {"id": "loose", "name": "x.md", "type": "text/markdown", "size": 1}})).unwrap();
    let say = |intent: &str, workspace: &str, unit: &str, task: u32, context: serde_json::Value| {
        json!({"kind": "message.sent", "body": {"intent": intent, "workspace": workspace, "unit": unit, "task": task, "text": "Why not paged?", "context": context}})
    };
    let lines = json!({"kind": "lines", "label": "mention.ts L3–4", "path": "app/mention.ts", "from": 3, "to": 4});
    let file = |id: &str| json!({"kind": "attachment", "label": "x", "id": id});

    // With everything a picked review shows: recorded with it, as sent.
    let context = json!([{"kind": "task", "label": "#1", "task": 1}, {"kind": "finding", "label": "P2-1"}, lines, file("rep"), file("mine")]);
    let e = add(&mut s, "user", say("i1", "", "u1", 2, context.clone())).unwrap();
    let Body::MessageSent { unit, task, context: got, .. } = &e.body else { panic!() };
    assert_eq!((unit.as_str(), *task, got.len()), ("u1", 2, 5));
    assert_eq!(got[0].task, Some(1));
    let m = s.state().message("m1").unwrap();
    assert_eq!((m.unit.as_str(), m.task, m.context.len()), ("u1", 2, 5));
    // By workspace alone, a unit of an intent that runs there.
    add(&mut s, "user", say("", "chat", "u1", 1, json!([lines]))).unwrap();
    // Only the user's own files: no unit needed, from any workspace's conversation.
    add(&mut s, "user", say("", "chat", "", 0, json!([file("mine")]))).unwrap();
    add(&mut s, "user", say("i1", "", "", 0, json!([file("mine")]))).unwrap();

    // Everything else needs a unit.
    for c in [json!({"kind": "finding", "label": "P2-1"}), json!({"kind": "file", "label": "a.ts", "path": "app/a.ts"}), lines.clone(), json!({"kind": "task", "label": "#1", "task": 1}), file("rep")] {
        let e = add(&mut s, "user", say("i1", "", "", 0, json!([c]))).unwrap_err();
        assert!(e.contains("needs the unit it is about"), "{c}: {e}");
    }
    assert!(add(&mut s, "user", say("i1", "", "", 1, json!([]))).unwrap_err().contains("needs the unit it is about"), "a task with no unit");
    // Of the message's intent; a task of the unit; an extra task of the unit, by number.
    assert!(add(&mut s, "user", say("i1", "", "u2", 0, json!([]))).unwrap_err().contains("u2 is not of intent i1"));
    assert!(add(&mut s, "user", say("", "chat", "u2", 0, json!([]))).unwrap_err().contains("u2 is not of an intent of workspace \"chat\""));
    assert!(add(&mut s, "user", say("i1", "", "u9", 0, json!([]))).unwrap_err().contains("no unit u9"));
    assert!(add(&mut s, "user", say("i1", "", "u1", 9, json!([]))).unwrap_err().contains("no task u1#9"));
    assert!(add(&mut s, "user", say("i1", "", "u1", 2, json!([{"kind": "task", "label": "#7", "task": 7}]))).unwrap_err().contains("no task u1#7"));
    assert!(add(&mut s, "user", say("i1", "", "u1", 2, json!([{"kind": "task", "label": "#1"}]))).unwrap_err().contains("names its task by number"));
    // An attachment neither the user's nor shown with the task.
    assert!(add(&mut s, "user", say("i1", "", "u1", 1, json!([file("rep")]))).unwrap_err().contains("is not one of u1#1's"));
    assert!(add(&mut s, "user", say("i1", "", "u1", 1, json!([file("loose")]))).unwrap_err().contains("is not one of u1#1's"));
    assert!(add(&mut s, "user", say("i1", "", "", 0, json!([file("loose")]))).unwrap_err().contains("needs the unit it is about"));
    // With the unit alone, one shown with any of its tasks.
    add(&mut s, "user", say("i1", "", "u1", 0, json!([file("rep")]))).unwrap();
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
    let s = recorded_before(s, "user", json!({"kind": "message.sent", "body": {"id": format!("m{}", n + 1), "intent": "i1", "workspace": "chat", "manager": "boss", "text": "Old."}}));
    let m = s.state().messages.last().unwrap();
    assert_eq!((m.text.as_str(), m.unit.as_str(), m.task, m.context.len()), ("Old.", "", 0, 0));
}
