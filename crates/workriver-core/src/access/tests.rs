use serde_json::json;

use crate::records::tests::*;
use crate::*;

/// What is the user's is the user's whoever asks: an agent cannot submit a gate, finish a
/// chore, batch or drop findings, link workspaces or add projects; its decision on a finding
/// is recorded as an agent's whatever it claims; and it cannot call itself the user.
#[test]
fn only_the_user_does_what_is_the_users() {
    let mut s = Records::default();
    seeded(&mut s);
    add(&mut s, "agent-a", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "test", "title": "Try it", "inputs": [{"from": "1"}]}})).unwrap();
    add(&mut s, "agent-a", json!({"kind": "chore.created", "body": {"intent": "i1", "title": "Rotate the key"}})).unwrap();
    add(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-2", "decision": "defer"}})).unwrap();
    let theirs = [
        json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 3, "result": "done"}}),
        json!({"kind": "chore.done", "body": {"chore": "c1"}}),
        json!({"kind": "findings.batched", "body": {"intent": "i1", "findings": [{"unit": "u1", "task": 2, "finding": "P2-2"}]}}),
        json!({"kind": "finding.dropped", "body": {"unit": "u1", "task": 2, "finding": "P2-2", "reason": "x"}}),
        json!({"kind": "workspace.linked", "body": {"machine": "box", "workspace": "a", "local": "a"}}),
        json!({"kind": "project.added", "body": {"path": "/p", "title": "P"}}),
        json!({"kind": "project.renamed", "body": {"project": "/p", "title": "Q"}}),
    ];
    for who in ["agent-a", "workriver"] {
        for v in &theirs {
            let e = add(&mut s, who, v.clone()).unwrap_err();
            assert!(e.contains("only the user does this"), "{who}: {e}");
        }
    }
    let impostor = Principal::Agent { session: "user".into(), machine: None };
    assert!(s.append(&impostor, body(theirs[0].clone())).is_err());
    assert!(s.append(&impostor, body(json!({"kind": "task.started", "body": {"unit": "u1", "task": 1}}))).is_err(), "nor record anything under that name");
    // A manager is an agent registering itself.
    let w = json!({"id": "w1", "label": "chat"});
    assert!(add(&mut s, "agent-a", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": w}})).is_err());
    assert!(add(&mut s, "user", json!({"kind": "manager.registered", "body": {"workspace": w}})).is_err());
    let e = add(&mut s, "agent-a", json!({"kind": "manager.registered", "body": {"workspace": w}})).unwrap();
    assert_eq!(e.body, body(json!({"kind": "manager.registered", "body": {"agent": "agent-a", "workspace": w}})));
    managed(&mut s, "agent-a", "i1");
    // The intent's manager decides findings unless the user keeps them (nobody else says
    // which); no other agent ever does; and what an agent decides is an agent's decision.
    let decide = |d: &str| json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": d, "by": "user"}});
    assert_eq!(s.state().intent("i1").unwrap().findings, FindingsPolicy::Manager, "the default, for records from before too");
    assert!(add(&mut s, "agent-b", decide("accept")).unwrap_err().contains("for the manager of intent i1"));
    assert!(add(&mut s, "agent-a", json!({"kind": "intent.updated", "body": {"intent": "i1", "findings": "user"}})).unwrap_err().contains("only the user says who decides"));
    add(&mut s, "user", json!({"kind": "intent.updated", "body": {"intent": "i1", "findings": "user"}})).unwrap();
    assert!(add(&mut s, "agent-a", decide("accept")).unwrap_err().contains("is the user's to decide (intent i1: findings user)"));
    // The policy by severity is gone (severity is the reviewer's own word): its records are the stricter one.
    add(&mut s, "user", json!({"kind": "intent.updated", "body": {"intent": "i1", "findings": "manager-except-p1"}})).unwrap();
    assert_eq!(replayed(&s).intent("i1").unwrap().findings, FindingsPolicy::User);
    add(&mut s, "user", json!({"kind": "intent.updated", "body": {"intent": "i1", "findings": "manager"}})).unwrap();
    let e = add(&mut s, "agent-a", decide("reject")).unwrap();
    assert_eq!(e.body, body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "reject", "by": "agent"}})));
    // A decision is made once, except by the user, who may change the manager's (or their own).
    assert!(add(&mut s, "agent-a", decide("accept")).unwrap_err().contains("only the user changes a decision"));
    let over = add(&mut s, "user", decide("accept")).unwrap();
    assert_eq!(over.body, body(json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "accept", "by": "user", "was": "rejected"}})));
    assert!(add(&mut s, "user", decide("accept")).unwrap_err().contains("already accepted"));
    let f = replayed(&s).task("u1", 2).unwrap().findings[0].clone();
    assert_eq!((f.state, f.decided_by), (FindingState::Accepted, Some(Who::User)));
    // The user does all of it.
    for v in theirs {
        add(&mut s, "user", v).unwrap();
    }
}

/// An agent on a blind task sees its own task, its unit and its intent, nothing of anyone
/// else's work, and neither asks nor is asked; delivering ends it.
#[test]
fn a_blind_task_sees_only_its_own() {
    let mut s = Records::default();
    seeded(&mut s);
    assert!(add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "x", "blind": true}})).unwrap_err().contains("--blind is for"));
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "review", "reviewer": "security-review", "text": "Look for ways around the mention check", "inputs": [{"from": "1"}], "agent": "sec", "blind": true}})).unwrap();
    add(&mut s, "user", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "Other unit", "repo": "/r2", "branch": "feat/o", "base": "main"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "chore.created", "body": {"intent": "i1", "title": "Rotate the key"}})).unwrap();
    add(&mut s, "user", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": "Why a regex?"}})).unwrap();
    add(&mut s, "user", json!({"kind": "finding.added", "body": {"project": "/r", "finding": {"severity": "P2", "problem": "Old leak"}}})).unwrap();
    add(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "u1", "task": 2, "finding": "P2-1", "decision": "accept", "note": "Yes, fix"}})).unwrap();
    for (who, id) in [("sec", "aaaaaaaaaaaaaaaa"), ("agent-a", "bbbbbbbbbbbbbbbb")] {
        add(&mut s, who, json!({"kind": "attachment.added", "body": {"id": id, "name": "shot.png", "type": "image/png", "size": 3}})).unwrap();
    }

    assert_eq!(s.state().blind_tasks("sec"), [("u1".to_string(), 3)]);
    assert!(s.state().blind_view("agent-a").is_none() && s.state().blind_view("boss").is_none(), "only the agent on the blind task");
    let v = s.state().blind_view("sec").unwrap();
    assert!(v.reduced);
    assert_eq!(v.units.iter().map(|u| (u.id.as_str(), u.title.as_str(), u.branch.as_str(), u.tasks.len())).collect::<Vec<_>>(), [("u1", "Mentions", "feat/mention", 1)]);
    assert_eq!((v.units[0].tasks[0].n, v.units[0].tasks[0].text.as_str()), (3, "Look for ways around the mention check"));
    assert_eq!((v.intents.len(), v.intents[0].title.as_str()), (1, "Chat v2"));
    assert!(v.chores.is_empty() && v.questions.is_empty() && v.threads.is_empty() && v.backlog.is_empty() && v.roadmap.is_empty());
    assert_eq!(v.attachments.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["aaaaaaaaaaaaaaaa"], "only what it attached itself");
    let text = serde_json::to_string(&v).unwrap();
    for hidden in ["Find mentions", "npm test", "6 passed", "Duplicates", "Emoji names", "Two notices", "Yes, fix", "Other unit", "Rotate the key", "Why a regex", "Old leak"] {
        assert!(!text.contains(hidden), "{hidden} is hidden: {text}");
    }
    // Where agents work: its own as seen, others by name only.
    for (session, cwd) in [("sec", "/w/sec"), ("agent-a", "/w/secret-unit")] {
        add(&mut s, "workriver", json!({"kind": "agent.seen", "body": {"session": session, "name": session, "harness": "claude", "cwd": cwd, "machine": "box"}})).unwrap();
    }
    let v = s.state().blind_view("sec").unwrap();
    let seen = |who: &str| v.agents.iter().find(|a| a.session == who).map(|a| (a.name.as_str(), a.cwd.as_str(), a.machine.as_deref())).unwrap();
    assert_eq!((seen("sec"), seen("agent-a")), (("sec", "/w/sec", Some("box")), ("agent-a", "", None)));
    // It neither asks, nor is asked by another agent; the user may ask.
    assert!(add(&mut s, "sec", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 1, "text": "What did the author intend?"}})).unwrap_err().contains("tell your manager"));
    assert!(add(&mut s, "agent-a", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 3, "text": "Seen my note?"}})).unwrap_err().contains("blind task in progress"));
    add(&mut s, "user", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 3, "text": "How far are you?"}})).unwrap();
    // Delivered: everything is back, and the task stays marked.
    add(&mut s, "sec", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 3, "findings": [{"severity": "P1", "problem": "Bypass", "impact": "i", "where": ["a"]}]}})).unwrap();
    assert!(s.state().blind_view("sec").is_none());
    assert!(s.state().task("u1", 3).unwrap().blind);
    assert!(replayed(&s).task("u1", 3).unwrap().blind);
}

/// Who answers a question is workriver's to say: an agent that asked cannot record itself
/// (or anyone) as the one answering, nor can the user; the service's own record stands, and
/// only the agent it names may answer.
#[test]
fn only_workriver_says_who_answers_a_question() {
    let mut s = Records::default();
    seeded(&mut s);
    add(&mut s, "agent-a", json!({"kind": "question.asked", "body": {"unit": "u1", "task": 2, "text": "Why a regex?"}})).unwrap();
    let started = |agent: &str| json!({"kind": "question.started", "body": {"question": "q1", "agent": agent, "name": agent, "temporary": true}});
    for who in ["agent-a", "boss", "user"] {
        let e = add(&mut s, who, started(who)).unwrap_err();
        assert!(e.contains("only workriver records this (question.started)"), "{who}: {e}");
    }
    assert!(s.state().questions.iter().all(|q| q.agent.is_none()), "nobody answers yet");
    add(&mut s, "workriver", started("s-why")).unwrap();
    assert!(add(&mut s, "boss", json!({"kind": "question.answered", "body": {"question": "q1", "text": "Because."}})).is_err(), "an agent that could not name itself the answerer does not answer");
    add(&mut s, "s-why", json!({"kind": "question.answered", "body": {"question": "q1", "text": "It predates the parser."}})).unwrap();
}

/// What an agent is seen as is the runtime's word, through the service: no agent or user
/// names an agent, its harness, or where its session is kept.
#[test]
fn only_workriver_says_what_an_agent_is_seen_as() {
    let mut s = Records::default();
    seeded(&mut s);
    let seen = json!({"kind": "agent.seen", "body": {"session": "agent-a", "name": "boss", "harness": "claude", "cwd": "/elsewhere"}});
    for who in ["agent-a", "boss", "user"] {
        let e = add(&mut s, who, seen.clone()).unwrap_err();
        assert!(e.contains("only workriver records this (agent.seen)"), "{who}: {e}");
    }
    add(&mut s, "workriver", seen).unwrap();
    let a = s.state().agents.iter().find(|a| a.session == "agent-a").cloned().unwrap();
    assert_eq!((a.name.as_str(), a.cwd.as_str(), a.machine), ("boss", "/elsewhere", None));
}
