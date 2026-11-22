use serde_json::{Value, json};

use super::*;

fn opt(value: &str, label: &str) -> ChoiceOption {
    ChoiceOption { value: value.into(), label: label.into() }
}

fn three() -> Choice {
    Choice::new(&[opt("merge", "核对通过后合并并推送"), opt("review", "先再审一轮"), opt("hold", "先不合并")], &["hold".into()])
}

/// The stored form, exactly: the four components in their one shape. The `required` check sits
/// on the Button, which A2UI disables while a check fails; the button's words are a path the
/// renderer fills.
#[test]
fn a_choice_is_built_as_the_a2ui_subset() {
    let want = json!({
        "surface": {"version": "1.0", "createSurface": {
            "surfaceId": "choice",
            "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
            "components": [
                {"id": "root", "component": "Column", "children": ["options", "submit"]},
                {"id": "options", "component": "ChoicePicker", "variant": "mutuallyExclusive",
                 "options": [{"label": "核对通过后合并并推送", "value": "merge"}, {"label": "先再审一轮", "value": "review"}, {"label": "先不合并", "value": "hold"}],
                 "value": {"@path": "/selected"}},
                {"id": "submit", "component": "Button", "variant": "primary", "child": "submit-label",
                 "action": {"event": {"name": "answer", "context": {"selected": {"@path": "/selected"}}}},
                 "@checks": [{"condition": {"@call": "required", "args": {"value": {"@path": "/selected"}}}}]},
                {"id": "submit-label", "component": "Text", "text": {"@path": "/ui/submit"}}
            ],
            "dataModel": {"selected": []}}},
        "needs_note": ["hold"]
    });
    let c = three();
    assert_eq!(serde_json::to_value(&c).unwrap(), want);
    assert_eq!(c.options(), [opt("merge", "核对通过后合并并推送"), opt("review", "先再审一轮"), opt("hold", "先不合并")]);
    assert_eq!((c.label("review").as_deref(), c.label("nope")), (Some("先再审一轮"), None));
    assert!(c.needs_note("hold") && !c.needs_note("merge"));
    assert!(c.check().is_ok());
    // No note required: the field is left out.
    let plain = Choice::new(&[opt("a", "A"), opt("b", "B")], &[]);
    assert!(serde_json::to_value(&plain).unwrap().get("needs_note").is_none());
}

/// Validates against the A2UI v1.0 JSON Schemas vendored in `__fixtures__/a2ui-v1_0` (README
/// there: the source commit). `message` is `agent_to_renderer.json` or `renderer_to_agent.json`.
fn a2ui_errors(message: &str, data: &Value) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/choices/__fixtures__/a2ui-v1_0");
    let read = |f: &str| -> Value { serde_json::from_str(&std::fs::read_to_string(format!("{dir}/{f}")).unwrap()).unwrap() };
    let base = "https://a2ui.org/specification/v1_0/";
    // The messages name the catalog relative to themselves (`catalog.json`); the basic
    // catalog is the one this subset pins, registered where they look for it.
    let mut catalog = read("catalog.json");
    catalog["$id"] = json!(format!("{base}catalog.json"));
    let registry = jsonschema::Registry::new()
        .add(format!("{base}agent_to_renderer.json"), read("agent_to_renderer.json"))
        .unwrap()
        .add(format!("{base}renderer_to_agent.json"), read("renderer_to_agent.json"))
        .unwrap()
        .add(format!("{base}common_types.json"), read("common_types.json"))
        .unwrap()
        .add(format!("{base}catalog.json"), catalog)
        .unwrap()
        .prepare()
        .unwrap();
    let validator = jsonschema::options().with_registry(&registry).build(&json!({"$ref": format!("{base}{message}")})).unwrap();
    validator.iter_errors(data).map(|e| format!("{} at {}", e, e.instance_path())).collect()
}

/// Every surface the builder makes is a valid A2UI v1.0 message, and so is the answer as the
/// Button's action sends it; the schemas do refuse a shape outside the spec. Guards the user's
/// decision that the stored form is A2UI, so that the official renderer can draw it.
#[test]
fn every_choice_is_valid_a2ui_v1_0() {
    let labels = ["Merge \"now\"", "先再审一轮（写明原因）", "Not yet — say why", "a/b | c", "😀", "x", "y", "z"];
    for n in MIN_OPTIONS..=MAX_OPTIONS {
        let options: Vec<_> = (0..n).map(|k| opt(&format!("o-{k}"), labels[k])).collect();
        for notes in [vec![], vec!["o-0".to_string()]] {
            let c = Choice::new(&options, &notes);
            assert!(c.check().is_ok());
            assert_eq!(a2ui_errors("agent_to_renderer.json", &c.surface), Vec::<String>::new(), "{n} options");
        }
    }
    let answer = json!({"version": "1.0", "action": {"name": "answer", "surfaceId": "choice", "sourceComponentId": "submit", "timestamp": "2026-11-21T00:00:00Z", "context": {"selected": ["review"]}}});
    assert_eq!(a2ui_errors("renderer_to_agent.json", &answer), Vec::<String>::new());
    // A refused shape: a note requirement inside an option item (A2UI allows label and value only).
    let mut bad = three().surface;
    bad["createSurface"]["components"][1]["options"][0]["needs_note"] = json!(true);
    assert!(!a2ui_errors("agent_to_renderer.json", &bad).is_empty());
}

/// What core accepts: exactly what the builder makes from good options; anything else is refused.
#[test]
fn only_the_subset_built_from_good_options_is_accepted() {
    let refused = |c: Choice| c.check().unwrap_err().0;
    let many: Vec<_> = (0..9).map(|k| opt(&format!("o{k}"), "x")).collect();
    assert!(refused(Choice::new(&[opt("a", "A")], &[])).contains("2 to 8 options"));
    assert!(refused(Choice::new(&many, &[])).contains("2 to 8 options"));
    assert!(refused(Choice::new(&[opt("A", "A"), opt("b", "B")], &[])).contains("[a-z0-9-]"));
    assert!(refused(Choice::new(&[opt("", "A"), opt("b", "B")], &[])).contains("[a-z0-9-]"));
    assert!(refused(Choice::new(&[opt(&"a".repeat(33), "A"), opt("b", "B")], &[])).contains("[a-z0-9-]"));
    assert!(refused(Choice::new(&[opt("a", "A"), opt("a", "B")], &[])).contains("twice"));
    assert!(refused(Choice::new(&[opt("a", " "), opt("b", "B")], &[])).contains("label"));
    assert!(refused(Choice::new(&[opt("a", "two\nlines"), opt("b", "B")], &[])).contains("label"));
    assert!(refused(Choice::new(&[opt("a", &"长".repeat(201)), opt("b", "B")], &[])).contains("label"));
    assert!(Choice::new(&[opt("a", &"长".repeat(200)), opt("b", "B")], &[]).check().is_ok());
    assert!(refused(Choice::new(&[opt("a", "A"), opt("b", "B")], &["c".into()])).contains("--require-note c"));
    // Hand-made shapes: whatever is not the builder's own form is refused, with how to make one.
    let tampered = |change: &dyn Fn(&mut Value)| {
        let mut c = three();
        change(&mut c.surface);
        refused(c)
    };
    let cases: [&dyn Fn(&mut Value); 7] = [
        &|s| s["version"] = json!("0.9"),
        &|s| s["createSurface"]["catalogId"] = json!("travel:flights"),
        &|s| s["createSurface"]["components"][1]["variant"] = json!("multipleSelection"),
        &|s| s["createSurface"]["components"][1]["options"][0]["label"] = json!({"@path": "/x"}),
        &|s| s["createSurface"]["components"][2]["action"]["event"]["name"] = json!("merge_now"),
        &|s| s["createSurface"]["components"].as_array_mut().unwrap().push(json!({"id": "img", "component": "Image", "url": "https://x"})),
        &|s| s["createSurface"]["sendDataModel"] = json!(true),
    ];
    for change in cases {
        assert!(tampered(change).contains("--option"));
    }
    assert!(refused(Choice { surface: json!({}), needs_note: vec![] }).contains("--option"));
}

/// An answer: one of the stored values, with a note where the option asks one.
#[test]
fn an_answer_is_one_stored_value_with_its_note() {
    let c = three();
    assert!(c.check_answer(&["review".into()], "").is_ok());
    assert!(c.check_answer(&["hold".into()], "after the release").is_ok());
    let refused = |selected: &[&str], note: &str| c.check_answer(&selected.iter().map(|s| s.to_string()).collect::<Vec<_>>(), note).unwrap_err().0;
    assert!(refused(&[], "").contains("pick one"));
    assert!(refused(&["merge", "review"], "").contains("pick one"));
    assert!(refused(&["先再审一轮"], "").contains("not one of"));
    assert!(refused(&["hold"], "  ").contains("needs a note"));
}

/// The stored choice is closed outside its surface too: a field beside `surface` and
/// `needs_note` is refused when a record is read, not dropped; what was recorded still reads.
#[test]
fn a_choice_takes_no_other_field() {
    let said = |choice: Value| serde_json::from_value::<crate::Body>(json!({"kind": "message.said", "body": {"text": "Which?", "choice": choice}}));
    let mut extra = serde_json::to_value(three()).unwrap();
    extra["hidden"] = json!("dropped silently before");
    assert!(said(extra).is_err());
    assert!(said(serde_json::to_value(three()).unwrap()).is_ok());
    // As recorded with no note required: `needs_note` left out.
    assert!(said(serde_json::to_value(Choice::new(&[opt("a", "A"), opt("b", "B")], &[])).unwrap()).is_ok());
}

/// Every choice ends with Other, which workriver adds: not stored in the surface, answered as the
/// reserved value `other` with a note (the user's instruction instead of an option). An asker
/// cannot define it; a choice recorded with an option of that value before keeps it as theirs.
#[test]
fn every_choice_takes_other_with_a_note() {
    let c = three();
    assert!(!c.options().iter().any(|o| o.value == OTHER) && c.label(OTHER).is_none());
    assert!(c.needs_note(OTHER));
    assert!(c.check_answer(&[OTHER.into()], "split it into two gates").is_ok());
    let refused = c.check_answer(&[OTHER.into()], " \n").unwrap_err().0;
    assert!(refused.contains("Other") && refused.contains("note"), "{refused}");
    // The value is workriver's: an option of an asker's may not take it.
    let refused = Choice::new(&[opt("other", "Something else"), opt("b", "B")], &[]).check().unwrap_err().0;
    assert!(refused.contains("other") && refused.contains("workriver"), "{refused}");
    // Recorded before the rule: the asker's own option, as it was (here: no note asked).
    let old = Choice::new(&[opt("a", "A"), opt("other", "Something else")], &[]);
    assert_eq!(old.label(OTHER).as_deref(), Some("Something else"));
    assert!(!old.needs_note(OTHER) && old.check_answer(&[OTHER.into()], "").is_ok());
}
