// Templates: their rules, the user's library, a unit's steps placed from one, filling and
// leaving out a step, and what a gate lists of them.
use serde_json::json;

use crate::records::tests::*;
use crate::*;

fn set(s: &mut Records, actor: &str, t: serde_json::Value) -> Result<Event, String> {
    add(s, actor, json!({"kind": "template.set", "body": t}))
}

fn steps(s: &Records, unit: &str) -> Vec<(u32, String, TaskKind, TaskState, Vec<String>)> {
    let u = s.state().unit(unit).unwrap();
    u.tasks.iter().map(|t| (t.n, t.placed.as_ref().map(|p| p.id.clone()).unwrap_or_default(), t.kind, t.state, t.inputs.iter().map(|i| i.from.clone()).collect())).collect()
}

/// An intent managed by `boss`, with a development unit u1 (or of `template`, `kind`).
fn developing(s: &mut Records, template: &str, kind: &str) {
    add(s, "user", json!({"kind": "intent.created", "body": {"title": "Sync"}})).unwrap();
    managed(s, "boss", "i1");
    let unit = match kind {
        "research" => json!({"intent": "i1", "title": "Look", "kind": "research", "template": template}),
        "spike" => json!({"intent": "i1", "title": "Try", "kind": "spike", "repo": "/r", "branch": "spike/t", "base": "main", "template": template}),
        _ => json!({"intent": "i1", "title": "Faster", "repo": "/r", "branch": "feat/sync", "base": "main", "template": template}),
    };
    add(s, "boss", json!({"kind": "unit.created", "body": unit})).unwrap();
}

/// The three built in pass their own rules, each has steps, and each is for its kind of unit.
#[test]
fn the_built_in_templates_hold_their_rules() {
    let all = builtin_templates();
    let kinds: Vec<(&str, UnitKind, usize)> = all.iter().map(|t| (t.name.as_str(), t.unit, t.steps.len())).collect();
    assert_eq!(kinds, [("research", UnitKind::Research, 2), ("development", UnitKind::Code, 8), ("spike", UnitKind::Spike, 2)]);
    for t in all {
        assert_eq!(t.clone().checked().unwrap(), t, "{} is as checked", t.name);
    }
    // The development flow: the design confirmed by the user, three reviews, the go last.
    let dev = State::default().template("development").unwrap().0;
    let flow: Vec<(String, Vec<String>)> = (0..dev.steps.len()).map(|k| (dev.steps[k].id.clone(), dev.after_of(k))).collect();
    let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(
        flow,
        [
            ("design".into(), v(&[])),
            ("design-review".into(), v(&["design"])),
            ("confirm".into(), v(&["design-review"])),
            ("implement".into(), v(&["confirm"])),
            ("code-review".into(), v(&["implement"])),
            ("security-review".into(), v(&["implement"])),
            ("architecture-review".into(), v(&["implement"])),
            ("go".into(), v(&["code-review", "security-review", "architecture-review"])),
        ]
    );
}

/// Each rule of the format, with the step and key at fault.
#[test]
fn a_template_is_checked_with_the_step_and_key_at_fault() {
    let base = || Template { name: "mine".into(), description: "Mine.".into(), unit: UnitKind::Code, steps: vec![] };
    let s = |id: &str, kind: TaskKind| TemplateStep { id: id.into(), kind, description: "d".into(), reviewer: None, gate: None, after: None };
    // No steps is the user's to save.
    assert!(base().checked().is_ok());
    let at = |t: Template| t.checked().map(|_| ()).unwrap_err();
    let f = at(Template { name: "Mine".into(), ..base() });
    assert_eq!((f.step, f.key), (None, Some("name")));
    assert_eq!(at(Template { name: "x".repeat(33), ..base() }).key, Some("name"));
    assert_eq!(at(Template { description: "  ".into(), ..base() }).key, Some("description"));
    assert_eq!(at(Template { description: "d".repeat(2001), ..base() }).key, Some("description"));
    assert_eq!(at(Template { steps: (0..21).map(|k| s(&format!("s{k}"), TaskKind::Implement)).collect(), ..base() }).key, Some("steps"));
    let one = |st: TemplateStep| at(Template { steps: vec![s("a", TaskKind::Implement), st], ..base() });
    for (st, key, says) in [
        (s("A", TaskKind::Implement), "id", "a-z"),
        (s("a", TaskKind::Implement), "id", "used twice"),
        (TemplateStep { description: " ".into(), ..s("b", TaskKind::Implement) }, "description", "empty"),
        (TemplateStep { description: "d".repeat(1001), ..s("b", TaskKind::Implement) }, "description", "longer"),
        (TemplateStep { reviewer: Some(Reviewer::CodeReview), ..s("b", TaskKind::Implement) }, "reviewer", "for a review"),
        (s("b", TaskKind::Gate), "gate", "decide, deploy or test"),
        (TemplateStep { gate: Some(GateKind::Test), ..s("b", TaskKind::Review) }, "gate", "for a gate"),
        (TemplateStep { after: Some(vec!["c".into()]), ..s("b", TaskKind::Review) }, "after", "not a step written before"),
        (TemplateStep { after: Some(vec!["b".into()]), ..s("b", TaskKind::Review) }, "after", "not a step written before"),
    ] {
        let f = one(st);
        assert_eq!((f.step, f.key), (Some(1), Some(key)), "{}", f.why);
        assert!(f.why.contains(says), "{} does not say {says}", f.why);
    }
    // A research unit has no branch: no implement or tests step.
    let f = at(Template { unit: UnitKind::Research, steps: vec![s("r", TaskKind::Research), s("t", TaskKind::Tests)], ..base() });
    assert_eq!((f.step, f.key), (Some(1), Some("kind")));
    // A review says its reviewer: code-review when left out.
    let t = Template { steps: vec![s("r", TaskKind::Review)], ..base() }.checked().unwrap();
    assert_eq!(t.steps[0].reviewer, Some(Reviewer::CodeReview));
}

/// The library is the user's: saved whole each time, its revision the event's sequence,
/// removed to bring a built-in back; nobody else touches it.
#[test]
fn the_user_keeps_the_library() {
    let mut s = Records::default();
    let mine = json!({"name": "development", "description": "Mine.", "unit": "code", "steps": [{"id": "do", "kind": "implement", "description": "Do it."}], "source": "name: development\n"});
    let e = set(&mut s, "boss", mine.clone()).unwrap_err();
    assert!(e.contains("only the user does this (template.set)"), "{e}");
    let e = set(&mut s, "user", mine.clone()).unwrap();
    let (t, revision) = s.state().template("development").unwrap();
    assert_eq!((t.steps.len(), revision), (1, Some(e.seq as u64)));
    // The text is not kept in the state.
    assert!(!serde_json::to_string(s.state()).unwrap().contains("name: development\\n"));
    // Saved again: a new revision, never reused.
    let again = set(&mut s, "user", mine.clone()).unwrap();
    assert_eq!(s.state().template("development").unwrap().1, Some(again.seq as u64));
    let lib: Vec<(String, bool)> = s.state().library().into_iter().map(|t| (t.template.name, t.builtin)).collect();
    assert!(lib.contains(&("development".into(), false)) && lib.contains(&("research".into(), true)) && !lib.contains(&("development".into(), true)), "{lib:?}");
    // Its rules hold on the record too.
    assert!(set(&mut s, "user", json!({"name": "x", "description": "", "steps": []})).unwrap_err().contains("description is empty"));
    assert!(set(&mut s, "user", json!({"name": "x", "description": "d", "steps": [], "source": "#".repeat(64 * 1024 + 1)})).unwrap_err().contains("64 KB"));

    assert!(add(&mut s, "boss", json!({"kind": "template.removed", "body": {"name": "development"}})).unwrap_err().contains("only the user"));
    add(&mut s, "user", json!({"kind": "template.removed", "body": {"name": "development"}})).unwrap();
    assert_eq!(s.state().template("development").unwrap(), (builtin_templates()[1].clone(), None), "the built-in is back");
    assert!(add(&mut s, "user", json!({"kind": "template.removed", "body": {"name": "development"}})).unwrap_err().contains("is built in"));
    assert!(add(&mut s, "user", json!({"kind": "template.removed", "body": {"name": "nope"}})).unwrap_err().contains("no template nope"));
    assert_eq!(replayed(&s), *s.state());
}

/// A unit names a template made for its kind; its steps are placed as tasks nobody holds,
/// from the template as it is then, whatever the request says.
#[test]
fn a_unit_is_made_with_its_template_placed() {
    let mut s = Records::default();
    add(&mut s, "user", json!({"kind": "intent.created", "body": {"title": "Sync"}})).unwrap();
    managed(&mut s, "boss", "i1");
    let unit = |more: serde_json::Value| {
        let mut b = json!({"intent": "i1", "title": "Faster", "repo": "/r", "branch": "feat/sync", "base": "main"});
        b.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        json!({"kind": "unit.created", "body": b})
    };
    // None named: refused, with the templates for a code unit and how to name one.
    let e = add(&mut s, "boss", unit(json!({}))).unwrap_err();
    assert!(e.contains("--template") && e.contains("development: Work that lands") && !e.contains("research: Finding out"), "{e}");
    let e = add(&mut s, "boss", unit(json!({"template": "research"}))).unwrap_err();
    assert!(e.contains("template research is for a research unit") && e.contains("development"), "{e}");
    assert!(add(&mut s, "boss", unit(json!({"template": "nope"}))).unwrap_err().contains("no template nope"));
    // What the request says it places is thrown away.
    let forged = json!([{"id": "x", "kind": "implement", "description": "mine", "after": []}]);
    let e = add(&mut s, "boss", unit(json!({"template": "development", "placed": forged, "revision": 7}))).unwrap();
    let Body::UnitCreated { template, revision, placed, .. } = &e.body else { unreachable!() };
    assert_eq!((template.as_str(), *revision, placed.len()), ("development", None, 8));
    let u = s.state().unit("u1").unwrap();
    assert_eq!((u.template.as_str(), u.revision), ("development", None));
    let st = steps(&s, "u1");
    assert_eq!(st[0], (1, "design".into(), TaskKind::Design, TaskState::Todo, vec!["start".into()]));
    assert_eq!(st[2], (3, "confirm".into(), TaskKind::Gate, TaskState::Todo, vec!["2".into()]), "a placed gate does not open");
    assert_eq!(st[7], (8, "go".into(), TaskKind::Gate, TaskState::Todo, vec!["5".into(), "6".into(), "7".into()]));
    let review = s.state().task("u1", 6).unwrap();
    assert_eq!((review.reviewer, review.agent.as_deref(), review.by.as_str(), review.title.as_str()), (Some(Reviewer::SecurityReview), None, "workriver", ""));
    assert_eq!(s.state().progress(s.state().unit("u1").unwrap()), Progress::Working);
    assert_eq!(s.state().steps_to_fill("u1").len(), 8);

    // A snapshot: the user's template changed later touches only units made later.
    let e = set(&mut s, "user", json!({"name": "development", "description": "Mine.", "steps": [{"id": "do", "kind": "implement", "description": "Do it."}]})).unwrap();
    add(&mut s, "boss", unit(json!({"template": "development", "branch": "feat/two"}))).unwrap();
    assert_eq!((s.state().unit("u2").unwrap().revision, s.state().unit("u2").unwrap().tasks.len()), (Some(e.seq as u64), 1));
    assert_eq!(s.state().unit("u1").unwrap().tasks.len(), 8);
    // A template of no steps (the user's alone to make) places nothing.
    set(&mut s, "user", json!({"name": "bare-code", "description": "Nothing placed.", "steps": []})).unwrap();
    add(&mut s, "boss", unit(json!({"template": "bare-code", "branch": "feat/three"}))).unwrap();
    assert!(s.state().unit("u3").unwrap().tasks.is_empty());
    assert_eq!(replayed(&s), *s.state());

    // Recorded before templates: no template, nothing placed, still read.
    let s = recorded_before(s, "user", json!({"kind": "unit.created", "body": {"id": "u4", "intent": "i1", "title": "Old", "repo": "/r", "branch": "feat/old", "base": "main"}}));
    assert!(s.state().unit("u4").unwrap().template.is_empty() && s.state().unit("u4").unwrap().tasks.is_empty());
}

/// A placed task is filled once, by the manager or the user, keeping the template's kind;
/// until then it is not started, delivered or noted, and a placed gate does not open.
#[test]
fn a_placed_step_is_filled_by_the_manager() {
    let mut s = Records::default();
    developing(&mut s, "research", "research");
    let fill = |n: u32, more: serde_json::Value| {
        let mut b = json!({"unit": "u1", "task": n});
        b.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        json!({"kind": "task.filled", "body": b})
    };
    for (what, says) in [
        (json!({"kind": "task.started", "body": {"unit": "u1", "task": 1}}), "not filled yet"),
        (json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1, "note": "n"}}), "not filled yet"),
        (json!({"kind": "task.noted", "body": {"unit": "u1", "task": 1, "text": "more"}}), "not filled yet"),
    ] {
        let e = add(&mut s, "boss", what).unwrap_err();
        assert!(e.contains(says) && e.contains("workriver task fill"), "{e}");
    }
    assert!(add(&mut s, "reader", fill(1, json!({"title": "Survey"}))).unwrap_err().contains("filled by the manager of intent i1"));
    assert!(add(&mut s, "boss", fill(1, json!({}))).unwrap_err().contains("needs a title"));
    assert!(add(&mut s, "boss", fill(1, json!({"title": "Survey", "inputs": [{"from": "1"}]}))).unwrap_err().contains("itself"));
    assert!(add(&mut s, "boss", fill(1, json!({"title": "Survey", "inputs": [{"from": "2"}]}))).unwrap_err().contains("cannot come before it"));
    add(&mut s, "boss", fill(1, json!({"title": "Survey", "text": "How others sync", "agent": "reader"}))).unwrap();
    let t = s.state().task("u1", 1).unwrap();
    assert_eq!((t.title.as_str(), t.agent.as_deref(), t.by.as_str(), t.kind, t.placed.as_ref().map(|p| p.filled)), ("Survey", Some("reader"), "boss", TaskKind::Research, Some(true)));
    assert!(add(&mut s, "boss", fill(1, json!({"title": "Again"}))).unwrap_err().contains("already filled"));
    assert!(add(&mut s, "user", fill(9, json!({"title": "x"}))).unwrap_err().contains("no task u1#9"));
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "research", "title": "Mine", "inputs": [{"from": "start"}]}})).unwrap();
    assert!(add(&mut s, "boss", fill(3, json!({"title": "x"}))).unwrap_err().contains("not a step of a template"));

    add(&mut s, "reader", json!({"kind": "task.started", "body": {"unit": "u1", "task": 1}})).unwrap();
    add(&mut s, "reader", json!({"kind": "task.delivered", "body": {"unit": "u1", "task": 1, "note": "Found."}})).unwrap();
    // The direction gate's inputs are done, but nobody filled it: it does not open.
    assert_eq!(s.state().task("u1", 2).unwrap().state, TaskState::Todo);
    let e = add(&mut s, "user", json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 2, "result": "done"}})).unwrap_err();
    assert!(e.contains("not filled yet"), "{e}");
    // The user may fill it too; --after adds to what it follows.
    add(&mut s, "user", fill(2, json!({"title": "Go on?", "inputs": [{"from": "#3"}]}))).unwrap();
    let g = s.state().task("u1", 2).unwrap();
    assert_eq!((g.state, g.inputs.iter().map(|i| i.from.as_str()).collect::<Vec<_>>(), g.by.as_str()), (TaskState::Todo, vec!["1", "3"], "user"));
    add(&mut s, "boss", json!({"kind": "task.dropped", "body": {"unit": "u1", "task": 3, "reason": "not needed"}})).unwrap();
    assert_eq!(s.state().task("u1", 2).unwrap().state, TaskState::Waiting, "open once what it follows is done or dropped");
    assert_eq!(replayed(&s), *s.state());
}

/// Every gate of a unit with placed steps lists them: done (by whom), left out (who, why),
/// not done. A placed gate lists those before it; one added by hand all of them; a blocker
/// none.
#[test]
fn a_gate_lists_the_steps() {
    let mut s = Records::default();
    developing(&mut s, "development", "code");
    let fill = |n: u32, title: &str, agent: Option<&str>| json!({"kind": "task.filled", "body": {"unit": "u1", "task": n, "title": title, "agent": agent}});
    let deliver = |s: &mut Records, n: u32, by: &str| {
        add(s, by, json!({"kind": "task.started", "body": {"unit": "u1", "task": n}})).unwrap();
        add(s, by, json!({"kind": "task.delivered", "body": {"unit": "u1", "task": n, "note": "Done."}})).unwrap();
    };
    for (n, who) in [(1, "designer"), (2, "arch")] {
        add(&mut s, "boss", fill(n, "x", Some(who))).unwrap();
        deliver(&mut s, n, who);
    }
    add(&mut s, "boss", json!({"kind": "task.dropped", "body": {"unit": "u1", "task": 3, "reason": "nothing to choose"}})).unwrap();
    add(&mut s, "boss", fill(4, "Build", Some("dev"))).unwrap();
    deliver(&mut s, 4, "dev");
    add(&mut s, "boss", json!({"kind": "task.dropped", "body": {"unit": "u1", "task": 6, "reason": "no input, access or secret"}})).unwrap();
    // A gate added by hand after the implementation: every step of the unit.
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "decide", "title": "Merge?", "inputs": [{"from": "4"}]}})).unwrap();
    let marks = |l: &[StepMark]| l.iter().map(|m| (m.id.clone(), m.n, m.by.clone(), m.reason.clone())).collect::<Vec<_>>();
    let hand = s.state().gate_steps("u1", 9).unwrap();
    assert_eq!((hand.template.as_str(), hand.users), ("development", false));
    assert_eq!(marks(&hand.done), [("design".into(), 1, Some("designer".into()), None), ("design-review".into(), 2, Some("arch".into()), None), ("implement".into(), 4, Some("dev".into()), None)]);
    assert_eq!(marks(&hand.left_out), [("confirm".into(), 3, Some("boss".into()), Some("nothing to choose".into())), ("security-review".into(), 6, Some("boss".into()), Some("no input, access or secret".into()))]);
    assert_eq!(marks(&hand.not_done), [("code-review".into(), 5, None, None), ("architecture-review".into(), 7, None, None)], "gates aside");
    // The placed go gate: the steps before it; once filled and its reviews done, it opens.
    for (n, who) in [(5, "rev"), (7, "arch")] {
        add(&mut s, "boss", fill(n, "x", Some(who))).unwrap();
        deliver(&mut s, n, who);
    }
    add(&mut s, "boss", fill(8, "Merge feat/sync?", None)).unwrap();
    let go = s.state().gate_steps("u1", 8).unwrap();
    assert_eq!((go.done.len(), go.left_out.len(), go.not_done.len()), (5, 2, 0));
    assert_eq!(s.state().task("u1", 8).unwrap().state, TaskState::Waiting);
    // A gate done is done by the user.
    add(&mut s, "user", json!({"kind": "gate.submitted", "body": {"unit": "u1", "task": 9, "result": "done"}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "More", "inputs": [{"from": "8"}]}})).unwrap();
    add(&mut s, "boss", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "test", "title": "Try it", "inputs": [{"from": "8"}]}})).unwrap();
    let later = s.state().gate_steps("u1", 11).unwrap();
    assert!(!later.done.iter().chain(&later.not_done).any(|m| m.id == "go"), "a gate waiting is neither done nor, being a gate, not done");
    // A blocker lists nothing; neither does a task that is no gate, nor a unit with no steps.
    add(&mut s, "dev", json!({"kind": "task.started", "body": {"unit": "u1", "task": 10}})).unwrap();
    add(&mut s, "dev", json!({"kind": "task.created", "body": {"unit": "u1", "kind": "gate", "gate": "test", "title": "Need a key", "blocks": 10}})).unwrap();
    assert!(s.state().gate_steps("u1", 12).is_none() && s.state().gate_steps("u1", 4).is_none());
    assert!(s.state().steps_to_fill("u1").is_empty());
}

/// What an agent on a blind task sees keeps the library: it says nothing of anyone's work.
#[test]
fn a_blind_view_keeps_the_templates() {
    let mut s = Records::default();
    developing(&mut s, "development", "code");
    add(&mut s, "boss", json!({"kind": "task.filled", "body": {"unit": "u1", "task": 2, "title": "x", "agent": "arch", "blind": true}})).unwrap();
    let blind = s.state().blind_view("arch").unwrap();
    assert_eq!(blind.templates, s.state().templates);
    assert!(!blind.templates.is_empty());
}
