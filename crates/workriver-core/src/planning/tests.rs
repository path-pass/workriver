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
    add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": w1}})).unwrap();  // the first start of a new workspace is the user's
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
    let w = json!({"id": "wM", "label": "workriver"});
    // As recorded before 0.2 (no rule then on where an intent was made).
    let mut s = recorded_before(Records::default(), "workriver", json!({"kind": "intent.created", "body": {"id": "i1", "title": "workriver", "goal": "", "runtime_ref": w}}));
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

/// A registration that names an intent (as before 0.2) is for the workspace that intent runs
/// in: not for an intent that runs in none, not with another workspace. What was recorded
/// before this rule replays as it did.
#[test]
fn a_manager_registers_for_its_intents_workspace() {
    let mut s = Records::default();
    let (w1, w2) = (json!({"id": "w1", "label": "chat"}), json!({"id": "w2", "label": "search"}));
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Nowhere", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Chat", "runtime_ref": w1}})).unwrap();
    let e = add(&mut s, "far", json!({"kind": "manager.registered", "body": {"intent": "i1"}})).unwrap_err();
    assert!(e.contains("i1 runs in no workspace") && e.contains("workriver manager here"), "{e}");
    let e = add(&mut s, "far", json!({"kind": "manager.registered", "body": {"intent": "i1", "workspace": w2}})).unwrap_err();
    assert!(e.contains("i1 runs in no workspace"), "{e}");
    let e = add(&mut s, "far", json!({"kind": "manager.registered", "body": {"intent": "i2", "workspace": w2}})).unwrap_err();
    assert!(e.contains("i2 runs in workspace chat, not search"), "{e}");
    assert_eq!((s.state().manager_of("i1"), s.state().manager_of("i2")), (None, None));
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"intent": "i2", "workspace": w1}})).unwrap();
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"intent": "i2"}})).unwrap();
    assert_eq!(s.state().manager_of("i2"), Some("boss"));
    // Recorded before: replayed as it was.
    let s = recorded_before(s, "far", json!({"kind": "manager.registered", "body": {"intent": "i1", "agent": "far", "workspace": w2}}));
    assert_eq!(s.state().manager_of("i1"), Some("far"));
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
    add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": local}})).unwrap();  // the first start of a new workspace is the user's
    assert!(s.state().workspace_of(&remote).is_none(), "the same label on another machine is another workspace");
    assert!(s.state().intent_for(&remote).is_none());
    add(&mut s, "user", json!({"kind": "workspace.linked", "body": {"machine": "box", "workspace": "chat", "local": "chat"}})).unwrap();
    assert!(add(&mut s, "user", json!({"kind": "workspace.linked", "body": {"machine": "box", "workspace": "chat", "local": "other"}})).is_err());
    let st = replayed(&s);
    assert_eq!(st.intent_for(&remote).map(|i| i.id.as_str()), Some("i1"));
    assert_eq!(st.workspace_of(&remote).and_then(|w| w.manager.as_deref()), Some("boss"));
    // A manager on the other machine registers for the linked workspace (naming its intent too:
    // the intent runs there): the one record, which stays the local one.
    add(&mut s, "far", json!({"kind": "manager.registered", "body": {"intent": "i1", "agent": "far", "workspace": remote}})).unwrap();
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
    add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": w1}})).unwrap();  // the first start of a new workspace is the user's
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

/// Who edits what of an intent, by its state: title and goal are the user's; the note is
/// also its manager's while it is active; a planned one takes additions from a manager of a
/// workspace of its project; a done or dropped one is the user's. Dropping a planned intent
/// is the user's too, and an agent's merge keeps the title.
#[test]
fn who_edits_an_intent() {
    let mut s = Records::default();
    let w1 = json!({"id": "w1", "label": "chat"});
    let w2 = json!({"id": "w2", "label": "idle"});
    let w3 = json!({"id": "w3", "label": "docs"});
    add(&mut s, "user", json!({"kind": "project.added", "body": {"path": "/p", "title": "Chat"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Mentions", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Docs", "project": "/q", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Search", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": w1}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i3", "state": "active", "workspace": w1}})).unwrap();  // the first start of a new workspace is the user's
    add(&mut s, "idle", json!({"kind": "manager.registered", "body": {"agent": "idle", "workspace": w2}})).unwrap();
    let set = |i: &str, f: serde_json::Value| {
        let mut b = json!({"intent": i});
        b.as_object_mut().unwrap().extend(f.as_object().unwrap().clone());
        json!({"kind": "intent.updated", "body": b})
    };

    // Planned: a manager whose workspace works on its project (here: by its current intent) adds; nothing more.
    add(&mut s, "boss", set("i1", json!({"add_note": "The user's example: @ann"}))).unwrap();
    assert_eq!(s.state().intent("i1").unwrap().note, "The user's example: @ann");
    let e = add(&mut s, "boss", set("i1", json!({"note": "x", "base": 2}))).unwrap_err();
    assert!(e.contains("--add-note"), "{e}");
    for f in [json!({"title": "X", "base": 2}), json!({"goal": "X", "base": 2})] {
        let e = add(&mut s, "boss", set("i1", f)).unwrap_err();
        assert!(e.contains("are the user's"), "{e}");
    }
    let e = add(&mut s, "agent-x", set("i1", json!({"add_note": "x"}))).unwrap_err();
    assert!(e.contains("a manager of a workspace of its project"), "{e}");
    // Another project's planned intent: refused, naming it; a workspace whose project is unknown: refused.
    let e = add(&mut s, "boss", set("i2", json!({"add_note": "x"}))).unwrap_err();
    assert!(e.contains("i2 belongs to project /q"), "{e}");
    let e = add(&mut s, "idle", set("i1", json!({"add_note": "x"}))).unwrap_err();
    assert!(e.contains("i1 belongs to project Chat") && e.contains("not known") && e.contains("workriver workspace project"), "{e}");
    // What the user set wins over the current intent's; any workspace it manages counts.
    add(&mut s, "user", json!({"kind": "workspace.project_set", "body": {"workspace": w2, "project": "/p"}})).unwrap();
    add(&mut s, "idle", set("i1", json!({"add_note": "From idle"}))).unwrap();
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": w3}})).unwrap();
    add(&mut s, "user", json!({"kind": "workspace.project_set", "body": {"workspace": w3, "project": "/q"}})).unwrap();
    add(&mut s, "boss", set("i2", json!({"add_note": "From docs"}))).unwrap();

    // Active: its manager adds and replaces the note; title and goal stay the user's; another manager does nothing.
    add(&mut s, "boss", set("i3", json!({"add_note": "Decided: SQLite"}))).unwrap();
    add(&mut s, "boss", set("i3", json!({"note": "Decided: Postgres", "base": 2}))).unwrap();
    assert_eq!(s.state().intent("i3").unwrap().note, "Decided: Postgres");
    let e = add(&mut s, "boss", set("i3", json!({"title": "Find", "base": 3}))).unwrap_err();
    assert!(e.contains("are the user's"), "{e}");
    let e = add(&mut s, "idle", set("i3", json!({"add_note": "x"}))).unwrap_err();
    assert!(e.contains("only the user or the manager of intent i3"), "{e}");
    let e = add(&mut s, "boss", set("i3", json!({"note": "a", "add_note": "b", "base": 3}))).unwrap_err();
    assert!(e.contains("--note or --add-note"), "{e}");
    // The user: everything, an empty note included; never an empty title.
    add(&mut s, "user", set("i3", json!({"title": "Find", "goal": "Fast", "note": "", "base": 3}))).unwrap();
    assert_eq!(s.state().intent("i3").unwrap().note, "");
    assert!(add(&mut s, "user", set("i3", json!({"title": " ", "base": 4}))).unwrap_err().contains("needs a title"));

    // Done: the user's alone.
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i3", "state": "done"}})).unwrap();
    let e = add(&mut s, "boss", set("i3", json!({"add_note": "x"}))).unwrap_err();
    assert!(e.contains("i3 is done") && e.contains("only the user"), "{e}");
    add(&mut s, "user", set("i3", json!({"add_note": "Shipped: https://x/1"}))).unwrap();

    // Back in the roadmap, an intent is the user's to drop, even for the manager it ran under.
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": w1}})).unwrap();
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "planned"}})).unwrap();
    let e = add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "dropped", "reason": "dup"}})).unwrap_err();
    assert!(e.contains("only the user drops planned intent i1"), "{e}");
    add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "dropped", "reason": "dup"}})).unwrap();

    // An agent merges what agents added, but keeps the title: that is the user's.
    add(&mut s, "boss", json!({"kind": "intent.created", "body": {"title": "Emoji", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "intent.created", "body": {"title": "Emoji names", "project": "/p", "state": "planned"}})).unwrap();
    let merge = |title: Option<&str>| json!({"kind": "intent.merged", "body": {"intent": "i4", "from": ["i5"], "title": title}});
    let e = add(&mut s, "boss", merge(Some("Emoji everywhere"))).unwrap_err();
    assert!(e.contains("the title is the user's"), "{e}");
    add(&mut s, "boss", merge(None)).unwrap();
    assert_eq!(s.state().intent("i4").unwrap().title, "Emoji");
    assert_eq!(replayed(&s), *s.state());
}

/// Every change of an intent's title, goal or note is a version; a replace based on an
/// older version is refused only when what it replaces changed since.
#[test]
fn intent_versions_and_conflicts() {
    let mut s = Records::default();
    let w1 = json!({"id": "w1", "label": "chat"});
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Mentions", "goal": "Tag", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": w1}})).unwrap();
    add(&mut s, "boss", json!({"kind": "agent.seen", "body": {"session": "boss", "name": "claude 1", "harness": "claude"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i1", "state": "active", "workspace": w1}})).unwrap();  // the first start of a new workspace is the user's
    let edits = |s: &Records| s.state().intent("i1").unwrap().edits.clone();
    assert_eq!((edits(&s).version, edits(&s).title, edits(&s).goal, edits(&s).note, edits(&s).by.as_str()), (1, 1, 1, 1, "user"), "created is v1");
    let set = |f: serde_json::Value| {
        let mut b = json!({"intent": "i1"});
        b.as_object_mut().unwrap().extend(f.as_object().unwrap().clone());
        json!({"kind": "intent.updated", "body": b})
    };
    add(&mut s, "user", set(json!({"title": "@mentions", "base": 1}))).unwrap();
    add(&mut s, "boss", set(json!({"add_note": "First"}))).unwrap();
    let e = edits(&s);
    assert_eq!((e.version, e.title, e.goal, e.note, e.by.as_str()), (3, 2, 1, 3, "boss"));
    // Who decides findings, or a state change: no version.
    add(&mut s, "user", set(json!({"findings": "user"}))).unwrap();
    assert_eq!(edits(&s).version, 3);
    // The note changed at v3: a replace from v2 is refused, saying by whom; one of the goal goes through.
    let e = add(&mut s, "user", set(json!({"note": "Mine", "base": 2}))).unwrap_err();
    assert!(e.contains("i1's note changed since v2") && e.contains("(in v3;") && e.contains("last by claude 1") && e.contains("intent history i1 --version 3") && e.contains("--base 3"), "{e}");
    let e = add(&mut s, "user", set(json!({"title": "x", "goal": "y", "base": 1}))).unwrap_err();
    assert!(e.contains("title changed since v1"), "{e}");
    add(&mut s, "user", set(json!({"goal": "Tag people", "base": 2}))).unwrap();
    assert!(add(&mut s, "user", set(json!({"goal": "z", "base": 9}))).unwrap_err().contains("no version 9"));
    // An addition needs no base and overwrites nothing.
    add(&mut s, "boss", set(json!({"add_note": "Second", "base": 1}))).unwrap();
    add(&mut s, "boss", set(json!({"note": "First\n\nSecond, corrected", "base": 5}))).unwrap();
    let e = edits(&s);
    assert_eq!((e.version, e.title, e.goal, e.note), (6, 2, 4, 6));
    // Rebuilt the same from the records; old records (no base) replay as versions too.
    assert_eq!(replayed(&s), *s.state());
    let old = recorded_before(s, "boss", json!({"kind": "intent.updated", "body": {"intent": "i1", "goal": "Old"}}));
    assert_eq!((old.state().intent("i1").unwrap().edits.version, old.state().intent("i1").unwrap().edits.goal), (7, 7));
}

/// An intent's history is rebuilt from its events and those of the intents merged into it:
/// each version holds what its event set, and the whole text of any version is the replay.
#[test]
fn intent_history_rebuilds_from_events() {
    let mut s = Records::default();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Search", "goal": "Find", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Emoji", "note": "By name", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Elsewhere", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.updated", "body": {"intent": "i2", "add_note": "And by code"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.updated", "body": {"intent": "i3", "add_note": "Not ours"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.merged", "body": {"intent": "i1", "from": ["i2"], "title": "Search and emoji"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.updated", "body": {"intent": "i1", "add_note": "After"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.updated", "body": {"intent": "i1", "findings": "user"}})).unwrap();

    let all = s.events();
    let versions = intent_versions(all, "i1");
    let summary: Vec<_> = versions.iter().map(|v| (v.n, v.how, v.title.as_deref(), v.added.as_deref(), v.merged.clone())).collect();
    assert_eq!(summary, [
        (1, How::Created, Some("Search"), None, vec![]),
        (2, How::Merged, Some("Search and emoji"), None, vec!["i2".to_string()]),
        (3, How::Edited, None, Some("After"), vec![]),
    ]);
    assert_eq!(versions[0].goal.as_deref(), Some("Find"));
    // The merge's text comes from i2's own events, as it stood then.
    let v2 = intent_text(all, "i1", 2).unwrap();
    assert_eq!((v2.title.as_str(), v2.goal.as_str()), ("Search and emoji", "Find"));
    assert_eq!(v2.note, "Merged from i2: Emoji\nBy name\n\nAnd by code");
    let now = intent_text(all, "i1", 3).unwrap();
    assert_eq!(now.note, s.state().intent("i1").unwrap().note);
    assert!(intent_text(all, "i1", 4).is_none());
    // Only the events that make it, and its sources', are needed.
    let own: Vec<_> = all.iter().filter(|e| matches!(e.body.refs().0.as_deref(), Some("i1" | "i2"))).cloned().collect();
    assert_eq!(intent_versions(&own, "i1"), versions);
    assert_eq!(intent_text(&own, "i1", 3), Some(now));
}

/// /p: i1 (planned, the user's), i2 (active in w1, boss). /q: i3 (planned, the user's).
fn two_projects() -> Records {
    let mut s = Records::default();
    let w1 = json!({"id": "w1", "label": "chat"});
    add(&mut s, "user", json!({"kind": "project.added", "body": {"path": "/p", "title": "P"}})).unwrap();
    add(&mut s, "user", json!({"kind": "project.added", "body": {"path": "/q", "title": "Q"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "One", "goal": "G1", "note": "N1", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Two", "goal": "G2", "note": "N2", "project": "/p", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Three", "goal": "G3", "note": "The user's own words", "project": "/q", "state": "planned"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": w1}})).unwrap();
    add(&mut s, "user", json!({"kind": "workspace.project_set", "body": {"workspace": w1, "project": "/p"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i2", "state": "active", "workspace": w1}})).unwrap();
    s
}

/// A manager does not start a planned intent of another project than its workspace's (the
/// user may), so it cannot become that intent's manager to replace its note, nor make its
/// workspace another project's to add to that project's planned intents. Where its
/// workspace's project is not known yet, its first start says it.
#[test]
fn a_manager_starts_only_its_workspaces_projects_intents() {
    let w1 = json!({"id": "w1", "label": "chat"});
    let start = |i: &str| json!({"kind": "intent.state_changed", "body": {"intent": i, "state": "active", "workspace": {"id": "w1", "label": "chat"}}});
    let set = |i: &str, f: serde_json::Value| {
        let mut b = json!({"intent": i});
        b.as_object_mut().unwrap().extend(f.as_object().unwrap().clone());
        json!({"kind": "intent.updated", "body": b})
    };
    // Replacing another project's planned note by starting it, then putting it back.
    let mut s = two_projects();
    let e = add(&mut s, "boss", start("i3")).unwrap_err();
    assert!(e.contains("i3 belongs to project Q") && e.contains("your workspace works on P") && e.contains("ask the user"), "{e}");
    assert!(add(&mut s, "boss", set("i3", json!({"note": "REPLACED by boss", "base": 1}))).is_err());
    assert_eq!(s.state().intent("i3").unwrap().note, "The user's own words");
    // Steering its workspace's project by starting an intent of another project.
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Four", "note": "Q's", "project": "/q", "state": "planned"}})).unwrap();
    assert!(add(&mut s, "boss", start("i3")).is_err());
    assert!(add(&mut s, "boss", set("i4", json!({"add_note": "added by a manager of /p"}))).is_err());
    assert_eq!(s.state().intent("i4").unwrap().note, "Q's");
    // Its own project's intents it starts; the user starts any, anywhere.
    add(&mut s, "boss", start("i1")).unwrap();
    add(&mut s, "user", start("i3")).unwrap();
    // What the user set wins over its current intent's project.
    add(&mut s, "user", json!({"kind": "workspace.project_set", "body": {"workspace": w1, "project": "/q"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Five", "project": "/p", "state": "planned"}})).unwrap();
    assert!(add(&mut s, "boss", start("i5")).unwrap_err().contains("your workspace works on Q"));
    // A workspace whose project nothing recorded says: its manager starts nothing of a known
    // project (that start would make it that project's); the user starts the first one.
    let mut s = two_projects();
    let w2 = json!({"id": "w2", "label": "idle"});
    let start2 = |i: &str| json!({"kind": "intent.state_changed", "body": {"intent": i, "state": "active", "workspace": {"id": "w2", "label": "idle"}}});
    add(&mut s, "idle", json!({"kind": "manager.registered", "body": {"agent": "idle", "workspace": w2}})).unwrap();
    let e = add(&mut s, "idle", start2("i3")).unwrap_err();
    assert!(e.contains("i3 belongs to project Q; your workspace's project is not known") && e.contains("workriver workspace project"), "{e}");
    // Nor one of no known project (records from before projects): the user starts it.
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Somewhere", "note": "The user's own words", "state": "planned"}})).unwrap();
    let e = add(&mut s, "idle", start2("i4")).unwrap_err();
    assert!(e.contains("i4 has no project") && e.contains("ask the user to start it"), "{e}");
    assert!(add(&mut s, "boss", start("i4")).is_err(), "not even in a workspace whose project is known");
    add(&mut s, "user", start2("i4")).unwrap();
    add(&mut s, "user", start2("i3")).unwrap();
    assert!(add(&mut s, "idle", start2("i1")).unwrap_err().contains("your workspace works on Q"));
    // Once its intents are over it still works on that project: the next of Q it starts.
    add(&mut s, "idle", json!({"kind": "intent.state_changed", "body": {"intent": "i3", "state": "done"}})).unwrap();
    add(&mut s, "idle", json!({"kind": "intent.state_changed", "body": {"intent": "i4", "state": "done"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Six", "project": "/q", "state": "planned"}})).unwrap();
    add(&mut s, "idle", start2("i5")).unwrap();
    assert!(add(&mut s, "idle", start2("i1")).unwrap_err().contains("your workspace works on Q"));
}

/// An active intent is marked done or put back in the roadmap by the user or its manager,
/// as dropping it is; no other agent.
#[test]
fn an_active_intent_is_closed_or_shelved_by_the_user_or_its_manager() {
    let mut s = two_projects();
    add(&mut s, "other", json!({"kind": "manager.registered", "body": {"agent": "other", "workspace": {"id": "w2", "label": "docs"}}})).unwrap();
    for (who, state) in [("worker", "done"), ("worker", "planned"), ("other", "done"), ("other", "planned")] {
        let e = add(&mut s, who, json!({"kind": "intent.state_changed", "body": {"intent": "i2", "state": state}})).unwrap_err();
        assert!(e.contains("only the user or the manager of intent i2"), "{who} {state}: {e}");
    }
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i2", "state": "planned"}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i2", "state": "active", "workspace": {"id": "w1", "label": "chat"}}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i2", "state": "done"}})).unwrap();
}

/// A whole note is kept as it was written, its indentation and blank lines included: a
/// Markdown code block stays one, in the intent, its history and a restore.
#[test]
fn a_whole_note_keeps_its_whitespace() {
    let mut s = two_projects();
    let note = "Run it:\n\n    cargo test --workspace\n    pnpm test\n\nThen deliver.\n";
    add(&mut s, "boss", json!({"kind": "intent.updated", "body": {"intent": "i2", "note": note, "base": 1}})).unwrap();
    assert_eq!(s.state().intent("i2").unwrap().note, note);
    assert_eq!(intent_text(s.events(), "i2", 2).unwrap().note, note);
    assert_eq!(intent_versions(s.events(), "i2")[1].note.as_deref(), Some(note));
    // Restored from that version, as the page and the CLI send it back.
    add(&mut s, "user", json!({"kind": "intent.updated", "body": {"intent": "i2", "note": "short", "base": 2}})).unwrap();
    add(&mut s, "user", json!({"kind": "intent.updated", "body": {"intent": "i2", "note": note, "base": 3}})).unwrap();
    assert_eq!(s.state().intent("i2").unwrap().note, note);
    // A note that starts with a code block keeps its first indentation.
    let code = "    let x = 1;
    x + 1
";
    add(&mut s, "user", json!({"kind": "intent.updated", "body": {"intent": "i2", "note": code, "base": 4}})).unwrap();
    assert_eq!(intent_text(s.events(), "i2", 5).unwrap().note, code);
    assert_eq!(replayed(&s), *s.state());
}

/// Where an intent runs, and so which workspace's project it is and who manages it, is set
/// only by a start the rules checked or by the user: an agent adds an intent planned and
/// with no workspace, and never links one to a workspace. Old records that did replay as
/// they were.
#[test]
fn agents_do_not_set_where_an_intent_runs() {
    let mut s = two_projects();
    let w1 = json!({"id": "w1", "label": "chat"});
    let w2 = json!({"id": "w2", "label": "docs"});
    add(&mut s, "other", json!({"kind": "manager.registered", "body": {"agent": "other", "workspace": w2}})).unwrap();
    // Creating an intent in its own workspace, active or planned, to make the workspace Q's.
    for state in ["active", "planned"] {
        let e = add(&mut s, "boss", json!({"kind": "intent.created", "body": {"title": "Decoy", "project": "/q", "state": state, "runtime_ref": w1}})).unwrap_err();
        assert!(e.contains("an agent adds an intent to the roadmap"), "{state}: {e}");
    }
    let e = add(&mut s, "boss", json!({"kind": "intent.created", "body": {"title": "Decoy", "project": "/p", "state": "active"}})).unwrap_err();
    assert!(e.contains("an agent adds an intent to the roadmap"), "{e}");
    add(&mut s, "boss", json!({"kind": "intent.created", "body": {"title": "Fine", "project": "/p", "state": "planned"}})).unwrap();
    // Linking another workspace's active intent to its own, to become its manager.
    add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i3", "state": "active", "workspace": w2}})).unwrap();
    let e = add(&mut s, "boss", json!({"kind": "intent.linked", "body": {"intent": "i3", "runtime_ref": w1}})).unwrap_err();
    assert!(e.contains("only the user does this (intent.linked)"), "{e}");
    assert_eq!(s.state().manager_of("i3"), Some("other"));
    // The user may do both.
    add(&mut s, "user", json!({"kind": "intent.linked", "body": {"intent": "i3", "runtime_ref": w1}})).unwrap();
    assert_eq!(s.state().manager_of("i3"), Some("boss"));
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Here", "project": "/p", "state": "active", "runtime_ref": w1}})).unwrap();
    // Recorded before the rule, an agent's intent made in its group still replays as it was.
    let old = recorded_before(s, "boss", json!({"kind": "intent.created", "body": {"id": "i6", "title": "Old", "state": "active", "runtime_ref": w2}}));
    assert_eq!(old.state().intent_for(&RuntimeRef { id: "w2".into(), label: "docs".into(), machine: None }).map(|i| i.id.as_str()), Some("i6"));
}

/// The intent's `intent.updated` with a kind, size or tags, as a JSON body.
fn tagged(i: &str, f: serde_json::Value) -> serde_json::Value {
    let mut b = json!({"intent": i});
    b.as_object_mut().unwrap().extend(f.as_object().unwrap().clone());
    json!({"kind": "intent.updated", "body": b})
}

/// An intent's kind, size and tags: optional, checked and kept lower-case; who sets them goes
/// by its state, as for its note (the user always; whoever adds it; its manager while active).
#[test]
fn an_intents_kind_size_and_tags() {
    let mut s = Records::default();
    let w1 = json!({"id": "w1", "label": "chat"});
    add(&mut s, "user", json!({"kind": "project.added", "body": {"path": "/p", "title": "Chat"}})).unwrap();
    // Whoever adds it sets them, normalised: trimmed, lower-case (other scripts as written), no repeats.
    add(&mut s, "agent-x", json!({"kind": "intent.created", "body": {"title": "Login", "project": "/p", "state": "planned", "kind": "fix", "size": "s", "tags": [" Security", "CLI", "security", "認証"]}})).unwrap();
    let i = s.state().intent("i1").unwrap();
    assert_eq!((i.kind, i.size, i.tags.clone()), (Some(IntentKind::Fix), Some(IntentSize::S), vec!["security".to_string(), "cli".into(), "認証".into()]));
    assert_eq!((i.edits.version, i.edits.kind, i.edits.size, i.edits.tags), (1, 1, 1, 1));
    // None of them: as before.
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Search", "project": "/p", "state": "planned"}})).unwrap();
    let i = s.state().intent("i2").unwrap();
    assert_eq!((i.kind, i.size, i.tags.len()), (None, None, 0));
    assert_eq!((i.edits.kind, i.edits.size, i.edits.tags), (0, 0, 0));
    // The limits, each refusal naming the tag.
    let long = "x".repeat(33);
    for (tags, says) in [
        (json!(["a b"]), "«a b»"),
        (json!(["a,b"]), "«a,b»"),
        (json!(["  "]), "1–32 characters"),
        (json!([long]), "1–32 characters"),
        (json!(["1", "2", "3", "4", "5", "6", "7", "8", "9"]), "at most 8"),
    ] {
        let e = add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "T", "project": "/p", "state": "planned", "tags": tags}})).unwrap_err();
        assert!(e.contains(says), "{tags}: {e}");
        let e = add(&mut s, "user", tagged("i2", json!({"tags": tags, "base": 1}))).unwrap_err();
        assert!(e.contains(says), "{tags}: {e}");
    }
    // Eight after repeats are dropped is fine; an unknown kind or size is no record at all.
    add(&mut s, "user", tagged("i2", json!({"tags": ["a", "b", "c", "d", "e", "f", "g", "h", "A"], "base": 1}))).unwrap();
    assert_eq!(s.state().intent("i2").unwrap().tags.len(), 8);
    assert!(serde_json::from_value::<Body>(tagged("i2", json!({"kind": "bug"}))).is_err());
    assert!(serde_json::from_value::<Body>(tagged("i2", json!({"size": "xl"}))).is_err());

    // Only a kind, size or tags is something to change; the refusal names them.
    let e = add(&mut s, "user", tagged("i2", json!({}))).unwrap_err();
    assert!(e.contains("--kind, --size or --tags"), "{e}");
    // Set, then cleared: null clears kind and size, [] clears tags; absent leaves each as it is.
    add(&mut s, "user", tagged("i2", json!({"kind": "feature", "size": "m", "base": 2}))).unwrap();
    add(&mut s, "user", tagged("i2", json!({"goal": "Find", "base": 3}))).unwrap();
    let i = s.state().intent("i2").unwrap();
    assert_eq!((i.kind, i.size, i.tags.len()), (Some(IntentKind::Feature), Some(IntentSize::M), 8), "an unrelated edit keeps them");
    add(&mut s, "user", tagged("i2", json!({"kind": null, "tags": [], "base": 4}))).unwrap();
    let i = s.state().intent("i2").unwrap();
    assert_eq!((i.kind, i.size, i.tags.len()), (None, Some(IntentSize::M), 0));
    let e = &i.edits;
    assert_eq!((e.version, e.kind, e.size, e.tags, e.goal), (5, 5, 3, 5, 4));

    // A replace from an older version is refused when that field changed since; another goes through.
    let e = add(&mut s, "user", tagged("i2", json!({"size": "l", "base": 2}))).unwrap_err();
    assert!(e.contains("i2's size changed since v2") && e.contains("(in v3;"), "{e}");
    let e = add(&mut s, "user", tagged("i2", json!({"tags": ["x"], "base": 4}))).unwrap_err();
    assert!(e.contains("i2's tags changed since v4"), "{e}");
    add(&mut s, "user", tagged("i2", json!({"tags": ["x"], "base": 3}))).unwrap_err();
    add(&mut s, "user", tagged("i2", json!({"size": "l", "base": 3}))).unwrap();

    // Planned: the user's alone, even for a manager of its project (who adds to its note).
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": w1}})).unwrap();
    add(&mut s, "user", json!({"kind": "workspace.project_set", "body": {"workspace": w1, "project": "/p"}})).unwrap();
    for f in [json!({"kind": "fix", "base": 6}), json!({"size": null, "base": 6}), json!({"tags": ["ui"], "base": 6})] {
        let e = add(&mut s, "boss", tagged("i2", f)).unwrap_err();
        assert!(e.contains("i2 is planned") && e.contains("kind, size and tags") && e.contains("the user's"), "{e}");
    }
    add(&mut s, "boss", tagged("i2", json!({"add_note": "Seen"}))).unwrap();
    // Active: its manager sets all three; another agent does not.
    add(&mut s, "user", json!({"kind": "intent.state_changed", "body": {"intent": "i2", "state": "active", "workspace": w1}})).unwrap();
    add(&mut s, "boss", tagged("i2", json!({"kind": "chore", "size": "s", "tags": ["build"], "base": 7}))).unwrap();
    let i = s.state().intent("i2").unwrap();
    assert_eq!((i.kind, i.size, i.tags.clone()), (Some(IntentKind::Chore), Some(IntentSize::S), vec!["build".to_string()]));
    let e = add(&mut s, "agent-x", tagged("i2", json!({"kind": "fix", "base": 8}))).unwrap_err();
    assert!(e.contains("only the user or the manager of intent i2"), "{e}");
    // Done: the user's alone.
    add(&mut s, "boss", json!({"kind": "intent.state_changed", "body": {"intent": "i2", "state": "done"}})).unwrap();
    let e = add(&mut s, "boss", tagged("i2", json!({"size": "m", "base": 8}))).unwrap_err();
    assert!(e.contains("i2 is done") && e.contains("only the user"), "{e}");
    add(&mut s, "user", tagged("i2", json!({"size": "m", "base": 8}))).unwrap();
    assert_eq!(replayed(&s), *s.state());
}

/// Absent, null and [] stay three things through the stored form: an absent field is not
/// written, a cleared one is written as null (kind, size) or [] (tags), and each reads back as
/// it was written; a creation without them is written as before.
#[test]
fn kind_size_and_tags_keep_absent_and_cleared_apart_on_the_wire() {
    let round = |v: serde_json::Value| serde_json::to_value(body(v.clone())).unwrap();
    for b in [
        json!({"intent": "i1", "goal": "x"}),
        json!({"intent": "i1", "kind": null}),
        json!({"intent": "i1", "size": null, "tags": []}),
        json!({"intent": "i1", "kind": "research", "size": "l", "tags": ["a"]}),
    ] {
        let v = json!({"kind": "intent.updated", "body": b});
        assert_eq!(round(v.clone()), v);
    }
    let Body::IntentUpdated { kind, size, tags, .. } = body(json!({"kind": "intent.updated", "body": {"intent": "i1", "kind": null, "tags": []}})) else { panic!() };
    assert_eq!((kind, size, tags), (Some(None), None, Some(vec![])));
    let created = json!({"kind": "intent.created", "body": {"id": "i1", "title": "T", "state": "planned"}});
    assert_eq!(round(created.clone()), created, "nothing new is written for an intent without them");
    let Body::IntentCreated { kind, size, tags, .. } = body(json!({"kind": "intent.created", "body": {"title": "T", "kind": null, "tags": []}})) else { panic!() };
    assert_eq!((kind, size, tags.len()), (None, None, 0));
}

/// A merge keeps the target's kind and size. Only the user's merge unions the tags (its own
/// first, cut to 8), a version of its tags; an agent's leaves the target's as they are. Either
/// way each merged intent's kind, size and tags go into its block of the note.
#[test]
fn a_merge_keeps_the_kind_and_unions_tags_only_for_the_user() {
    let mut s = Records::default();
    let plan = |title: &str, extra: serde_json::Value| {
        let mut b = json!({"title": title, "project": "/p", "state": "planned"});
        b.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        json!({"kind": "intent.created", "body": b})
    };
    add(&mut s, "boss", json!({"kind": "manager.registered", "body": {"agent": "boss", "workspace": {"id": "w1", "label": "chat"}}})).unwrap();
    add(&mut s, "importer", plan("Login", json!({"kind": "feature", "size": "m", "tags": ["ui"]}))).unwrap(); // i1
    add(&mut s, "importer", plan("Login bug", json!({"kind": "fix", "size": "s", "tags": ["security", "cli"]}))).unwrap(); // i2
    add(&mut s, "importer", plan("Plain", json!({}))).unwrap(); // i3
    add(&mut s, "user", tagged("i1", json!({"tags": ["ui", "auth"], "base": 1}))).unwrap();
    // An agent's merge: the user's tags stay as they are; the note keeps the source's.
    add(&mut s, "boss", json!({"kind": "intent.merged", "body": {"intent": "i1", "from": ["i2", "i3"]}})).unwrap();
    let i = s.state().intent("i1").unwrap();
    assert_eq!((i.kind, i.size, i.tags.clone()), (Some(IntentKind::Feature), Some(IntentSize::M), vec!["ui".to_string(), "auth".into()]));
    assert!(i.note.contains("Merged from i2: Login bug\nfix · S · `security`, `cli`"), "{}", i.note);
    assert!(i.note.ends_with("Merged from i3: Plain"), "{}", i.note);
    assert_eq!((i.edits.version, i.edits.tags), (3, 2), "an agent's merge is no version of the tags");
    // The user's merge: the union, its own first, cut to 8; a version of the tags.
    add(&mut s, "importer", plan("Many", json!({"tags": ["auth", "a", "b", "c", "d", "e", "f", "g"]}))).unwrap(); // i4
    add(&mut s, "user", json!({"kind": "intent.merged", "body": {"intent": "i1", "from": ["i4"]}})).unwrap();
    let i = s.state().intent("i1").unwrap();
    assert_eq!(i.tags, ["ui", "auth", "a", "b", "c", "d", "e", "f"]);
    assert_eq!((i.edits.version, i.edits.tags, i.edits.kind), (4, 4, 1));
    // A stale tags edit now conflicts; a user's merge that adds nothing is no version of the tags.
    let e = add(&mut s, "user", tagged("i1", json!({"tags": [], "base": 3}))).unwrap_err();
    assert!(e.contains("i1's tags changed since v3"), "{e}");
    add(&mut s, "user", plan("Same", json!({"tags": ["ui"]}))).unwrap(); // i5
    add(&mut s, "user", json!({"kind": "intent.merged", "body": {"intent": "i1", "from": ["i5"]}})).unwrap();
    assert_eq!(s.state().intent("i1").unwrap().edits.tags, 4);
    assert_eq!(replayed(&s), *s.state());
}

/// A merged intent's tags go into its block of the note as written, each a code span: a tag
/// that looks like Markdown (a fence, a heading's underline, a backtick) changes nothing
/// around it; a block of a source with no kind, size or tags is as before.
#[test]
fn merged_tags_are_literal_in_the_note() {
    let mut s = Records::default();
    let plan = |title: &str, tags: serde_json::Value| json!({"kind": "intent.created", "body": {"title": title, "goal": "Its goal", "project": "/p", "state": "planned", "tags": tags}});
    add(&mut s, "user", plan("Target", json!([]))).unwrap(); // i1
    add(&mut s, "user", plan("Fence", json!(["~~~"]))).unwrap(); // i2
    add(&mut s, "user", plan("Odd", json!(["---", "a`b", "`x", "``"]))).unwrap(); // i3
    add(&mut s, "user", plan("Plain", json!([]))).unwrap(); // i4
    add(&mut s, "user", json!({"kind": "intent.merged", "body": {"intent": "i1", "from": ["i2", "i3", "i4"]}})).unwrap();
    assert_eq!(s.state().intent("i1").unwrap().note, "Merged from i2: Fence\n`~~~`\nIts goal\n\nMerged from i3: Odd\n`---`, ``a`b``, `` `x ``, ``` `` ```\nIts goal\n\nMerged from i4: Plain\nIts goal");
}

/// The tag check is bounded before any tag is read (it runs in the rules, under the service's
/// one lock on the store, and any agent may add an intent): a list of any length is refused at
/// once. An edit's refusal of who may set them comes before it.
#[test]
fn a_long_tag_list_is_refused_at_once() {
    let mut s = Records::default();
    let many: Vec<String> = (0..50_000).map(|n| format!("t{n}")).collect();
    let quick = std::time::Duration::from_secs(1);
    let t = std::time::Instant::now();
    let e = add(&mut s, "agent-x", json!({"kind": "intent.created", "body": {"title": "Flood", "project": "/p", "state": "planned", "tags": many}})).unwrap_err();
    assert!(t.elapsed() < quick && e.contains("an intent has at most 8 tags; 50000 given"), "{:?}: {e}", t.elapsed());
    // Repeats of one tag are as many given.
    let e = add(&mut s, "agent-x", json!({"kind": "intent.created", "body": {"title": "Flood", "project": "/p", "state": "planned", "tags": vec!["ui"; 50_000]}})).unwrap_err();
    assert!(e.contains("at most 8 tags; 50000 given"), "{e}");
    add(&mut s, "agent-x", json!({"kind": "intent.created", "body": {"title": "Mine", "project": "/p", "state": "planned"}})).unwrap();
    let t = std::time::Instant::now();
    let e = add(&mut s, "agent-y", tagged("i1", json!({"tags": many, "base": 1}))).unwrap_err();
    assert!(t.elapsed() < quick && e.contains("i1 is planned: its kind, size and tags are the user's"), "{:?}: {e}", t.elapsed());
    let e = add(&mut s, "user", tagged("i1", json!({"tags": many, "base": 1}))).unwrap_err();
    assert!(e.contains("at most 8 tags; 50000 given"), "{e}");
    assert!(s.state().intents.len() == 1 && s.state().intent("i1").unwrap().tags.is_empty());
}

/// A version says the kind, size and tags it set (null: cleared), and a version's text holds
/// them, so a restore brings them back.
#[test]
fn the_history_holds_kind_size_and_tags() {
    let mut s = Records::default();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Search", "project": "/p", "state": "planned", "kind": "feature"}})).unwrap();
    add(&mut s, "user", tagged("i1", json!({"size": "m", "tags": ["ui"], "base": 1}))).unwrap();
    add(&mut s, "user", tagged("i1", json!({"kind": null, "base": 2}))).unwrap();
    add(&mut s, "user", tagged("i1", json!({"goal": "Find", "base": 3}))).unwrap();
    let all = s.events();
    let v = intent_versions(all, "i1");
    let seen: Vec<_> = v.iter().map(|v| (v.n, v.kind, v.size, v.tags.clone())).collect();
    assert_eq!(seen, [
        (1, Some(Some(IntentKind::Feature)), None, None),
        (2, None, Some(Some(IntentSize::M)), Some(vec!["ui".to_string()])),
        (3, Some(None), None, None),
        (4, None, None, None),
    ]);
    let wire = serde_json::to_value(&v[2]).unwrap();
    assert!(wire["kind"].is_null() && wire.as_object().unwrap().contains_key("kind") && !wire.as_object().unwrap().contains_key("size"), "{wire}");
    let t = intent_text(all, "i1", 2).unwrap();
    assert_eq!((t.kind, t.size, t.tags), (Some(IntentKind::Feature), Some(IntentSize::M), vec!["ui".to_string()]));
    let t = intent_text(all, "i1", 4).unwrap();
    assert_eq!((t.kind, t.size), (None, Some(IntentSize::M)));
}
