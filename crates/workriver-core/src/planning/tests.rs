use serde_json::json;

use crate::records::tests::*;
use crate::*;

/// A unit's branch is its own and says what it holds; units named before that rule still
/// load from the records and keep working.
#[test]
fn unit_branches_take_a_prefix_but_old_ones_replay() {
    let mut s = Records::default();
    seeded(&mut s);
    let unit = |branch: &str, base: &str| json!({"kind": "unit.created", "body": {"intent": "i1", "title": "x", "repo": "/r", "branch": branch, "base": base}});
    for (branch, base, says) in [("ja-ui", "main", "starts with feat/, chore/ or fix/"), ("feat/", "main", "and a name"), ("feature/x", "main", "e.g. --branch feat/x"), ("main", "main", "must not be its base"), ("feat/x", "feat/x", "must not be its base"), ("main", "origin/main", "must not be its base")] {
        let e = add(&mut s, "user", unit(branch, base)).unwrap_err();
        assert!(e.contains(says), "{branch} from {base}: {e}");
    }
    for branch in ["feat/x", "chore/deps", "fix/login"] {
        add(&mut s, "user", unit(branch, "main")).unwrap();
    }
    let moved = |branch: &str, base: &str| json!({"kind": "unit.relocated", "body": {"unit": "u2", "repo": "/r", "branch": branch, "base": base}});
    assert!(add(&mut s, "user", moved("x2", "main")).unwrap_err().contains("starts with feat/"));
    add(&mut s, "user", moved("fix/x2", "main")).unwrap();

    // Written before the rule: straight into the records, as an old database has it.
    let mut s = recorded_before(s, "user", json!({"kind": "unit.created", "body": {"id": "u5", "intent": "i1", "title": "ja", "repo": "/r", "branch": "ja-ui", "base": "main"}}));
    assert_eq!(s.state().unit("u5").unwrap().branch, "ja-ui");
    add(&mut s, "user", json!({"kind": "task.created", "body": {"unit": "u5", "kind": "implement", "title": "Translate"}})).unwrap();
    // Moved to another base, its name stays; a new name follows the rule.
    add(&mut s, "user", json!({"kind": "unit.relocated", "body": {"unit": "u5", "repo": "/r", "branch": "ja-ui", "base": "develop"}})).unwrap();
    assert!(add(&mut s, "user", json!({"kind": "unit.relocated", "body": {"unit": "u5", "repo": "/r", "branch": "ja-ui-2", "base": "develop"}})).unwrap_err().contains("starts with feat/"));
    assert_eq!(replayed(&s).unit("u5").unwrap().base, "develop");
}

/// Intents move planned → active → done (or dropped); a workspace's manager starts them,
/// and the workspace's current intent is where its new work goes.
#[test]
fn intent_states_and_workspaces() {
    let mut s = Records::default();
    let w1 = json!({"id": "w1", "label": "chat"});
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Mentions", "goal": "Tag people", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Search", "project": "/p", "state": "planned"}})).unwrap();
    assert_eq!(s.state().roadmap, ["i1", "i2"]);
    // Starting needs the workspace's manager, in that workspace.
    let e = add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": w1}})).unwrap_err();
    assert!(e.contains("manager here"), "{e}");
    let e = add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active"}})).unwrap_err();
    assert!(e.contains("intent start i1"), "{e}");
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": w1}})).unwrap();
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": w1}})).unwrap();
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i2", "state": "active", "workspace": w1}})).unwrap();
    let st = s.state();
    assert!(st.roadmap.is_empty());
    assert_eq!(st.intent_for(&RuntimeRef { id: "w1".into(), label: "chat".into(), machine: None }).map(|i| i.id.as_str()), Some("i2"), "the last started is current");
    assert_eq!((st.manager_of("i1"), st.manager_of("i2")), (Some("boss"), Some("boss")), "both active intents have the workspace's manager");
    // Done: no longer current, no new units; done cannot start again; dropping needs a reason.
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i2", "state": "done"}})).unwrap();
    assert_eq!(s.state().intent_for(&RuntimeRef { id: "w1".into(), label: "chat".into(), machine: None }).map(|i| i.id.as_str()), Some("i1"), "the other active intent there is current again");
    let e = add(&mut s, "boss", json!({"kind": "unit.created", "body": {"intent": "i2", "title": "x", "repo": "/r", "branch": "feat/b", "base": "main"}})).unwrap_err();
    assert!(e.contains("is done"), "{e}");
    let e = add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i2", "state": "active", "workspace": w1}})).unwrap_err();
    assert!(e.contains("cannot become active"), "{e}");
    let e = add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "dropped"}})).unwrap_err();
    assert!(e.contains("reason"), "{e}");
    add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "dropped", "reason": "not needed"}})).unwrap();
    assert_eq!(s.state().intent("i1").unwrap().reason.as_deref(), Some("not needed"));
}

/// The roadmap keeps the order intents were added in, and moves change it.
#[test]
fn roadmap_order_and_moves() {
    let mut s = Records::default();
    for t in ["A", "B", "C", "D"] {
        add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": t, "project": "/p", "state": "planned", "note": "line 1\nhttps://example.com/x", "external": {"system": "jira", "key": "ABC-1"}}})).unwrap();
    }
    assert_eq!(s.state().roadmap, ["i1", "i2", "i3", "i4"]);
    add(&mut s, "user", json!({"kind": "intent.moved", "body": {"intent": "i4", "before": "i1"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.moved", "body": {"intent": "i1", "after": "i3"}})).unwrap();
    assert_eq!(s.state().roadmap, ["i4", "i2", "i3", "i1"]);
    assert!(add(&mut s, "user", json!({"kind": "intent.moved", "body": {"intent": "i1", "before": "i1"}})).is_err());
    assert!(add(&mut s, "user", json!({"kind": "intent.moved", "body": {"intent": "i1"}})).is_err());
    let i = s.state().intent("i1").unwrap().clone();
    assert_eq!((i.note.as_str(), i.external.as_ref().map(|e| e.key.as_str())), ("line 1\nhttps://example.com/x", Some("ABC-1")));
    // Order and external fields survive a replay.
    let replayed = replayed(&s);
    assert_eq!(replayed.roadmap, s.state().roadmap);
    assert_eq!(replayed.intent("i1").unwrap().external, i.external);
}

/// Planned intents split too finely are merged into one: the others are dropped and what
/// they said goes into its note, in the order given; it keeps its place in the roadmap.
#[test]
fn planned_intents_merge() {
    let mut s = Records::default();
    let plan = |title: &str, project: &str| json!({"kind": "intent.created", "body": {"title": title, "goal": format!("{title} goal"), "project": project, "state": "planned"}});
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Search", "goal": "Find things", "project": "/p", "state": "planned", "note": "Own note\n"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Filters", "goal": "By date", "project": "/p", "state": "planned", "note": "see #12", "external": {"system": "jira", "key": "ABC-2", "url": "https://x/ABC-2"}}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Ranking", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", plan("Other project", "/q")).unwrap(); // i4
    add(&mut s, "user", plan("Has a task", "/p")).unwrap(); // i5
    // Tasks of the intent itself, from before research units.
    let mut s = recorded_before(s, "boss", json!({"kind": "task.created", "body": {"unit": "i5", "intent": "i5", "n": 1, "kind": "research", "title": "Look", "inputs": [{"from": "start"}]}}));
    add(&mut s, "user", plan("Has a chore", "/p")).unwrap(); // i6
    add(&mut s, "user", json!({"kind": "chore.created", "body": {"intent": "i6", "title": "Rotate"}})).unwrap();
    add(&mut s, "user", plan("Has a finding", "/p")).unwrap(); // i7
    add(&mut s, "user", json!({"kind": "finding.added", "body": {"project": "/p", "intent": "i7", "finding": {"severity": "P2", "problem": "Slow"}}})).unwrap();
    add(&mut s, "user", plan("Dropped", "/p")).unwrap(); // i8
    add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i8", "state": "dropped", "reason": "no"}})).unwrap();
    add(&mut s, "user", plan("Last", "/p")).unwrap(); // i9
    add(&mut s, "user", json!({"kind": "intent.moved", "body": {"intent": "i9", "before": "i1"}})).unwrap();
    let merge = |into: &str, from: &[&str]| json!({"kind": "intent.merged", "body": {"intent": into, "from": from}});

    for (into, from, says) in [
        ("i1", &[][..], "name the planned intents"),
        ("i1", &["i1"][..], "named twice"),
        ("i1", &["i2", "i2"][..], "named twice"),
        ("i1", &["i99"][..], "no intent i99"),
        ("i99", &["i2"][..], "no intent i99"),
        ("i8", &["i2"][..], "i8 is dropped"),
        ("i1", &["i8"][..], "i8 is dropped"),
        ("i1", &["i4"][..], "another project"),
        ("i1", &["i5"][..], "has tasks"),
        ("i1", &["i6"][..], "has chores"),
        ("i1", &["i7"][..], "has backlog findings"),
    ] {
        let e = add(&mut s, "user", merge(into, from)).unwrap_err();
        assert!(e.contains(says), "{into} <- {from:?}: {e}");
    }
    // What the user added, only the user merges; what agents added (an import), agents may tidy.
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": {"id": "w1", "label": "chat"}}})).unwrap();
    assert!(add(&mut s, "boss", merge("i1", &["i2"])).unwrap_err().contains("i1 was added by the user"));
    add(&mut s, "importer", plan("Import a", "/p")).unwrap(); // i10
    add(&mut s, "importer", plan("Import b", "/p")).unwrap(); // i11
    assert!(add(&mut s, "boss", merge("i10", &["i11", "i2"])).unwrap_err().contains("i2 was added by the user"));
    assert_eq!(s.state().intent("i10").unwrap().by, "importer");
    add(&mut s, "boss", merge("i10", &["i11"])).unwrap();
    assert_eq!(s.state().intent("i11").unwrap().reason.as_deref(), Some("merged into i10"));

    let before = s.state().roadmap.clone();
    add(&mut s, "user", merge("i1", &["i3", "i2"])).unwrap();
    let st = s.state();
    assert_eq!(st.intent("i1").unwrap().note, "Own note\n\nMerged from i3: Ranking\n\nMerged from i2: Filters\nBy date\nsee #12\njira:ABC-2 https://x/ABC-2");
    for f in ["i2", "i3"] {
        let x = st.intent(f).unwrap();
        assert_eq!((x.state, x.reason.as_deref()), (IntentState::Dropped, Some("merged into i1")));
    }
    let after: Vec<_> = before.iter().filter(|x| !["i2", "i3"].contains(&x.as_str())).cloned().collect();
    assert_eq!(st.roadmap, after, "the target keeps its place");
    assert_eq!(st.roadmap[..2], ["i9", "i1"]);
    assert_eq!(st.intent("i1").unwrap().external, None, "the target keeps its own external id");
    assert!(add(&mut s, "user", merge("i1", &["i2"])).unwrap_err().contains("i2 is dropped"), "merged once");
    assert_eq!(s.state().intent("i1").unwrap().title, "Search", "no title given: the target keeps its own");
    // A new title for the merged whole: never blank; given, the target takes it.
    let titled = |title: &str| json!({"kind": "intent.merged", "body": {"intent": "i1", "from": ["i9"], "title": title}});
    assert!(add(&mut s, "user", titled("  ")).unwrap_err().contains("the new title is empty"));
    add(&mut s, "user", titled(" Search and filters ")).unwrap();
    assert_eq!(s.state().intent("i1").unwrap().title, "Search and filters");
    assert!(s.state().intent("i1").unwrap().note.ends_with("Merged from i9: Last\nLast goal"));
    let replayed = replayed(&s);
    assert_eq!(serde_json::to_value(&replayed).unwrap(), serde_json::to_value(s.state()).unwrap());
}

/// Research and design are a unit of their own with no branch; research hands in a note or a
/// report. Nothing new goes into an intent itself, but its tasks recorded before that replay
/// and keep working.
#[test]
fn research_units_and_old_intent_level_work() {
    let mut s = Records::default();
    add(&mut s, "user", json!({"kind": "project.added", "body": {"path": "/p", "title": "P"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Sync", "project": "p1", "state": "planned"}})).unwrap();
    for kind in ["research", "design", "review", "implement"] {
        let e = add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "i1", "kind": kind, "title": "x"}})).unwrap_err();
        assert!(e.contains("tasks go into a unit") && e.contains("workriver unit add i1 --kind research"), "{e}");
    }
    let research = |extra: serde_json::Value| {
        let mut b = json!({"intent": "i1", "title": "Compare sync engines", "kind": "research"});
        b.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        json!({"kind": "unit.created", "body": b})
    };
    for extra in [json!({"branch": "feat/x"}), json!({"repo": "/r"}), json!({"base": "main"}), json!({"machine": "box"})] {
        assert!(add(&mut s, "boss", research(extra.clone())).unwrap_err().contains("has no repository, branch"), "{extra}");
    }
    let e = add(&mut s, "boss", research(json!({}))).unwrap();
    assert_eq!(e.body.refs(), (Some("i1".into()), Some("u1".into()), None));
    let u = s.state().unit("u1").unwrap().clone();
    assert_eq!((u.kind, u.repo.as_str(), u.branch.as_str(), u.has_code()), (UnitKind::Research, "/p", "", false), "it reads in the project's folder");
    // No implement or tests work, no commits, no merge, no move.
    for kind in ["implement", "tests"] {
        assert!(add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": kind, "title": "x"}})).unwrap_err().contains("is a research unit"));
    }
    assert!(add(&mut s, "user", json!({"kind": "unit.merged", "body": {"unit": "u1"}})).unwrap_err().contains("unit done u1"));
    assert!(add(&mut s, "user", json!({"kind": "unit.relocated", "body": {"unit": "u1", "repo": "/r", "branch": "feat/x", "base": "main"}})).unwrap_err().contains("research unit"));
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "research", "title": "Compare sync engines"}})).unwrap();
    assert!(add(&mut s, "user", json!({"kind": "commit.linked", "body": {"unit": "u1", "hash": "abc", "task": 1}})).unwrap_err().contains("research unit"));
    let e = add(&mut s, "r", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1}})).unwrap_err();
    assert!(e.contains("research task delivers"), "{e}");
    let e = add(&mut s, "r", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1, "note": "n", "findings": [{"severity": "P2", "problem": "p", "impact": "i", "where": ["a"]}]}})).unwrap_err();
    assert!(e.contains("findings"), "{e}");
    add(&mut s, "r", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1, "note": "Use CRDTs; see report"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "decide", "title": "Confirm", "inputs": [{"from": "1"}]}})).unwrap();
    assert_eq!(s.state().progress(&s.state().unit("u1").unwrap().clone()), Progress::Waiting);
    add(&mut s, "user", json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 2, "result": "done", "note": "CRDTs"}})).unwrap();
    assert_eq!(s.state().progress(&s.state().unit("u1").unwrap().clone()), Progress::Ready);
    add(&mut s, "user", json!({"kind": "unit.done", "body": {"unit": "u1", "reason": "decided"}})).unwrap();
    assert_eq!(s.state().unit("u1").unwrap().tasks.len(), 2);

    // Recorded before research units: the intent's own tasks replay and still take the
    // rest of their life (start, deliver, decide, a cause of work in a unit).
    let s = recorded_before(s, "boss", json!({"kind": "task.created", "body": {"unit": "i1", "intent": "i1", "n": 1, "kind": "research", "title": "Look", "inputs": [{"from": "start"}]}}));
    let s = recorded_before(s, "boss", json!({"kind": "task.created", "body": {"unit": "i1", "intent": "i1", "n": 2, "kind": "review", "reviewer": "code-review", "inputs": [{"from": "1"}]}}));
    let mut s = s;
    let t = s.state().task("i1", 1).unwrap();
    assert_eq!((t.state, t.intent.as_str()), (TaskState::Todo, "i1"));
    add(&mut s, "r", json!({"kind": "task.started", "body": {"unit": "i1", "task": 1}})).unwrap();
    add(&mut s, "r", json!({"kind": "task.delivered", "body": {"unit": "i1", "task": 1, "note": "Found it"}})).unwrap();
    add(&mut s, "r2", json!({"kind": "task.delivered", "body": {"unit": "i1", "task": 2, "findings": [{"severity": "P2", "problem": "p", "impact": "i", "where": ["a"]}]}})).unwrap();
    add(&mut s, "user", json!({"kind": "finding.decided", "body": {"unit": "i1", "task": 2, "finding": "P2-1", "decision": "accept"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "Engine", "repo": "/r", "branch": "feat/b", "base": "main"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u2", "kind": "implement", "title": "Fix", "causes": [{"unit": "i1", "task": 2, "findings": ["P2-1"]}]}})).unwrap();
    assert_eq!(s.state().task("u2", 1).unwrap().intent, "i1", "a unit's tasks record their intent too");
    assert!(add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "i1", "kind": "research", "title": "More", "inputs": [{"from": "1"}]}})).unwrap_err().contains("tasks go into a unit"));
    assert_eq!(serde_json::to_value(replayed(&s)).unwrap(), serde_json::to_value(s.state()).unwrap());
}

/// A spike is code on a spike/ branch of its own: it takes every kind of task and commits,
/// moves like a code unit, is never merged, and is closed with what was learned.
#[test]
fn spike_units_keep_code_but_are_never_merged() {
    let mut s = Records::default();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Sync"}})).unwrap();
    let unit = |kind: &str, branch: &str| json!({"kind": "unit.created", "body": {"intent": "i1", "title": "Try CRDTs", "repo": "/r", "branch": branch, "base": "main", "kind": kind}});
    for (kind, branch, says) in [("spike", "feat/crdt", "--branch spike/crdt"), ("spike", "spike/", "a spike's branch starts with spike/"), ("spike", "main", "must not be its base"), ("code", "spike/crdt", "spike/<name> is for --kind spike")] {
        let e = add(&mut s, "boss", unit(kind, branch)).unwrap_err();
        assert!(e.contains(says), "{kind} {branch}: {e}");
    }
    add(&mut s, "boss", unit("spike", "spike/crdt")).unwrap();
    let u = s.state().unit("u1").unwrap().clone();
    assert_eq!((u.kind, u.branch.as_str(), u.has_code(), u.kind.merges()), (UnitKind::Spike, "spike/crdt", true, false));
    // Every kind of task.
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "Prototype"}})).unwrap();
    for (kind, extra) in [("research", json!({"title": "Measure"})), ("review", json!({})), ("tests", json!({})), ("design", json!({"title": "Shape"})), ("gate", json!({"gate": "decide", "title": "Look"}))] {
        let mut b = json!({"unit": "u1", "kind": kind, "inputs": [{"from": "1"}]});
        b.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        add(&mut s, "boss", json!({"kind": "task.created", "body": b})).unwrap_or_else(|e| panic!("{kind}: {e}"));
    }
    add(&mut s, "user", json!({"kind": "commit.linked", "body": {"unit": "u1", "hash": "abc", "task": 1}})).unwrap();
    let moved = |branch: &str| json!({"kind": "unit.relocated", "body": {"unit": "u1", "repo": "/r", "branch": branch, "base": "main"}});
    assert!(add(&mut s, "user", moved("feat/crdt2")).unwrap_err().contains("spike/crdt2"));
    add(&mut s, "user", moved("spike/crdt2")).unwrap();
    let e = add(&mut s, "user", json!({"kind": "unit.merged", "body": {"unit": "u1"}})).unwrap_err();
    assert!(e.contains("is a spike") && e.contains("workriver unit done u1 --reason \"<what was learned>\""), "{e}");
    add(&mut s, "user", json!({"kind": "unit.done", "body": {"unit": "u1", "reason": "CRDTs are 3x slower"}})).unwrap();
    assert_eq!(s.state().progress(&s.state().unit("u1").unwrap().clone()), Progress::Done);
    assert_eq!(serde_json::to_value(replayed(&s)).unwrap(), serde_json::to_value(s.state()).unwrap());
    assert_eq!(replayed(&s).unit("u1").unwrap().kind, UnitKind::Spike);
}

/// The intent's manager or the user adds a paragraph to its note; nobody else.
#[test]
fn a_note_grows_by_paragraphs() {
    let mut s = Records::default();
    seeded(&mut s);
    let more = |text: &str| json!({"kind": "intent.updated", "body": {"intent": "i1", "add_note": text}});
    assert!(add(&mut s, "agent-b", more("x")).unwrap_err().contains("only the user or the manager"));
    assert!(add(&mut s, "user", more("  ")).unwrap_err().contains("empty"));
    add(&mut s, "user", more("First: see https://x/1\n")).unwrap();
    assert_eq!(s.state().intent("i1").unwrap().note, "First: see https://x/1");
    add(&mut s, "user", more("Second\nline")).unwrap();
    assert_eq!(s.state().intent("i1").unwrap().note, "First: see https://x/1\n\nSecond\nline");
    assert_eq!(replayed(&s).intent("i1").unwrap().note, s.state().intent("i1").unwrap().note);
}

/// Records from before projects and intent states replay onto the new model: an intent made
/// for a runtime group is active and that group's current intent, its manager the group's.
#[test]
fn old_records_map_onto_workspaces() {
    let mut s = Records::default();
    let w = json!({"id": "wM", "label": "workriver"});
    add(&mut s, "workriver", json!({"kind": "intent.created", "body": {"id": "", "title": "workriver", "goal": "", "runtime_ref": w}})).unwrap();
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"intent": "i1", "agent": "boss"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "unit.created", "body": {"intent": "i1", "title": "ja", "repo": "/r", "branch": "feat/ja-ui", "base": "main"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "impl"}})).unwrap();
    let st = replayed(&s);
    let i = st.intent("i1").unwrap();
    assert_eq!((i.state, i.workspace.as_ref().map(|g| g.id.as_str())), (IntentState::Active, Some("wM")));
    assert_eq!(st.workspaces.len(), 1);
    assert_eq!((st.workspaces[0].manager.as_deref(), st.workspaces[0].current.as_deref()), (Some("boss"), Some("i1")));
    assert_eq!(st.manager_of("i1"), Some("boss"));
    assert_eq!(st.task("u1", 1).unwrap().intent, "i1");
}

/// Workspaces of two machines are never one by their labels alone: only a recorded link
/// makes a remote workspace the local one (one manager, the same current intent).
#[test]
fn remote_workspaces_are_linked_explicitly() {
    let mut s = Records::default();
    let local = json!({"id": "w1", "label": "chat"});
    let remote = RuntimeRef { id: "w1@box".into(), label: "chat".into(), machine: Some("box".into()) };
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "A", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": local}})).unwrap();
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": local}})).unwrap();
    assert!(s.state().workspace_of(&remote).is_none(), "the same label on another machine is another workspace");
    assert!(s.state().intent_for(&remote).is_none());
    add(&mut s, "user", json!({"kind": "workspace.linked", "body": {"machine": "box", "workspace": "chat", "local": "chat"}})).unwrap();
    assert!(add(&mut s, "user", json!({"kind": "workspace.linked", "body": {"machine": "box", "workspace": "chat", "local": "other"}})).is_err());
    let st = replayed(&s);
    assert_eq!(st.intent_for(&remote).map(|i| i.id.as_str()), Some("i1"));
    assert_eq!(st.workspace_of(&remote).and_then(|w| w.manager.as_deref()), Some("boss"));
    // A manager on the other machine registers for the linked workspace: the one record, which stays the local one.
    add(&mut s, "far", json!({"kind": "manager.registered", "body": {"agent": "far", "workspace": remote}})).unwrap();
    let st = s.state();
    assert_eq!((st.workspaces.len(), st.workspaces[0].manager.as_deref(), st.workspaces[0].group.id.as_str()), (1, Some("far"), "w1"));
    assert_eq!(st.manager_of("i1"), Some("far"));
    add(&mut s, "user", json!({"kind": "workspace.unlinked", "body": {"machine": "box", "workspace": "chat"}})).unwrap();
    assert!(s.state().workspace_of(&remote).is_none());
}

/// Where a unit's work is: on the recorder's own machine by default; on another only by the
/// user, or by the intent's manager where a workspace there is linked with the intent's.
/// It can be corrected afterwards. And chores may belong to a project or to nothing.
#[test]
fn units_are_where_the_work_is() {
    let mut s = Records::default();
    let w1 = json!({"id": "w1", "label": "chat"});
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "A", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"workspace": w1}})).unwrap();
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": w1}})).unwrap();
    let unit = |machine: Option<&str>| json!({"kind": "unit.created", "body": {"intent": "i1", "title": "U", "repo": "/srv/app", "branch": "feat/f", "base": "main", "machine": machine}});
    // The manager is on this machine, the workers on box.
    let e = add(&mut s, "boss", unit(Some("box"))).unwrap_err();
    assert!(e.contains("workriver workspace link box:<workspace there> chat"), "{e}");
    add(&mut s, "user", json!({"kind": "workspace.linked", "body": {"machine": "box", "workspace": "work", "local": "chat"}})).unwrap();
    assert!(add(&mut s, "worker", unit(Some("box"))).unwrap_err().contains("only the user or the manager"));
    add(&mut s, "boss", unit(Some("box"))).unwrap();
    assert_eq!(s.state().unit("u1").unwrap().machine.as_deref(), Some("box"));
    // An agent over there records a unit over there without more ado; the user, anywhere.
    let far = Principal::Agent { session: "far".into(), machine: Some("box".into()) };
    s.append(&far, body(unit(Some("box")))).unwrap();
    assert!(s.append(&far, body(unit(Some("other")))).is_err());
    add(&mut s, "user", unit(Some("other"))).unwrap();
    add(&mut s, "boss", unit(None)).unwrap();

    // Recorded in the wrong place: moved, by the manager or the user only.
    let moved = json!({"kind": "unit.relocated", "body": {"unit": "u4", "machine": "box", "repo": "/home/me/app", "branch": "feat/feature", "base": "main"}});
    assert!(add(&mut s, "worker", moved.clone()).unwrap_err().contains("moves its units"));
    add(&mut s, "boss", moved).unwrap();
    let u = replayed(&s).unit("u4").unwrap().clone();
    assert_eq!((u.machine.as_deref(), u.repo.as_str(), u.branch.as_str()), (Some("box"), "/home/me/app", "feat/feature"));
    assert!(add(&mut s, "boss", json!({"kind": "unit.relocated", "body": {"unit": "u4", "machine": "nowhere", "repo": "/x", "branch": "feat/f", "base": "main"}})).unwrap_err().contains("no workspace on nowhere is linked"));

    // Chores: an intent's, a project's, nobody's.
    add(&mut s, "boss", json!({"kind": "chore.created", "body": {"intent": "i1", "title": "Rotate the key"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "chore.created", "body": {"project": "/p", "title": "Renew the certificate"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "chore.created", "body": {"title": "Pay the invoice"}})).unwrap();
    assert!(add(&mut s, "boss", json!({"kind": "chore.created", "body": {"project": "/p", "unit": "u1", "title": "x"}})).is_err(), "a unit's chore is its intent's");
    let c = &replayed(&s).chores;
    assert_eq!(c.iter().map(|c| (c.intent.as_str(), c.project.as_str())).collect::<Vec<_>>(), [("i1", ""), ("", "/p"), ("", "")]);
}

#[test]
fn the_user_renames_a_project() {
    let mut s = Records::default();
    {
        add(&mut s, "user", json!({"kind": "project.added", "body": {"path": "/p", "title": "P"}})).unwrap();
        assert!(add(&mut s, "user", json!({"kind": "project.renamed", "body": {"project": "/nope", "title": "Q"}})).unwrap_err().contains("no project /nope"));
        assert!(add(&mut s, "user", json!({"kind": "project.renamed", "body": {"project": "p1", "title": " "}})).is_err());
        // By folder or by id; recorded by id.
        let e = add(&mut s, "user", json!({"kind": "project.renamed", "body": {"project": "/p", "title": "Q"}})).unwrap();
        assert_eq!(e.body, body(json!({"kind": "project.renamed", "body": {"project": "p1", "title": "Q"}})));
        add(&mut s, "user", json!({"kind": "project.renamed", "body": {"project": "p1", "title": "R"}})).unwrap();
    }
    assert_eq!(replayed(&s).projects[0].title, "R", "replayed");
}
