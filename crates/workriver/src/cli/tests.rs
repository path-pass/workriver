// The CLI's own tests: its parsers, its embedded guides and its links; no service runs.
use serde_json::json;

/// The cause forms the CLI takes, and the external id.
#[test]
fn cli_causes_and_external_ids() {
    use crate::cli::{parse_cause, parse_external};
    assert_eq!(parse_cause("4:P2-1,P3-1").unwrap(), json!({ "task": 4, "findings": ["P2-1", "P3-1"] }));
    assert_eq!(parse_cause("u1#4:P2-1").unwrap(), json!({ "unit": "u1", "task": 4, "findings": ["P2-1"] }));
    assert_eq!(parse_cause("f3,f12").unwrap(), json!({ "task": 0, "findings": ["f3", "f12"] }));
    assert!(parse_cause("P2-1").is_err());
    assert_eq!(parse_external(Some("jira:ABC-12".into()), Some("https://x/ABC-12".into())).unwrap(), Some(json!({ "system": "jira", "key": "ABC-12", "url": "https://x/ABC-12" })));
    assert!(parse_external(Some("ABC-12".into()), None).is_err());
}

/// What `workriver guide` prints: the build first, the recipes by topic, the index for an
/// unknown one, and stubs that send their reader to the installed binary.
#[test]
fn guides_name_their_build_and_know_their_recipes() {
    let build = format!("workriver {} · ", crate::common::BUILD);
    for (what, topic, has) in [("manager", None, "## Hard rules"), ("import", None, "# Import a backlog"), ("changes", None, "Findings are decided by the intent's manager"), ("how", None, "| `branch-moved` |"), ("how", Some("branch-moved"), "workriver unit move u1 --branch"), ("how", Some("no-such-topic"), "| `commit-link` |")] {
        let text = crate::cli::guide(what, topic).unwrap();
        assert!(text.starts_with(&build) && text.contains(has) && !text.contains("\n---\nname:"), "{what} {topic:?}: {}", &text[..200.min(text.len())]);
    }
    // Every recipe the index names is there, and no recipe is missing from the index.
    let index = crate::cli::guide("how", None).unwrap();
    let named: Vec<&str> = index.lines().filter_map(|l| l.strip_prefix("| `")?.split('`').next()).collect();
    let files: Vec<String> = crate::cli::Recipes::iter().map(|f| f.rsplit('/').next().unwrap().trim_end_matches(".md").to_string()).collect();
    assert!(named.len() >= 17 && named.iter().all(|t| files.iter().any(|f| f == t)) && files.iter().all(|f| named.contains(&f.as_str())), "{named:?} / {files:?}");
    // The worker's paragraph stays one line, build first.
    let worker = crate::cli::guide("worker", None).unwrap();
    assert!(worker.starts_with(&format!("(workriver {}) ", crate::common::BUILD)) && worker.trim_end().lines().count() == 1);
    let stub = crate::cli::guide("install-stub", Some("manager")).unwrap();
    assert!(stub.starts_with("---\nname: workriver-manager\ndescription: ") && stub.contains("Run `workriver guide manager` and follow what it prints") && stub.split_whitespace().count() < 120);
    assert!(crate::cli::guide("install-stub", None).is_err());
}

/// `workriver link`: the page's path for each kind of reference, and nothing else.
#[test]
fn link_paths() {
    for (r, path) in [("u1", "/u1"), ("i3", "/i3"), ("u1#3", "/u1/3"), ("i3#1", "/i3/1"), ("u12#4:P2-1", "/u12/4?finding=P2-1")] {
        assert_eq!(crate::cli::link_path(r).unwrap(), path, "{r}");
    }
    for r in ["", "u", "x1", "u1#", "u1#a", "u1:P2-1", "u1#3:", "u1#3:P2 1", "u1/3", "../u1"] {
        assert!(crate::cli::link_path(r).is_err(), "{r}");
    }
}

/// `--option ID=LABEL` and `--require-note ID` become the stored choice; a note requirement
/// without options is refused before anything is sent.
#[test]
fn options_become_a_choice() {
    use crate::cli::parse_choice;
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(parse_choice(&[], &[]).unwrap(), None);
    let c = parse_choice(&s(&["merge=Merge = and push", " hold = Not now "]), &s(&["hold"])).unwrap().unwrap();
    let c: workriver_core::Choice = serde_json::from_value(c).unwrap();
    let got: Vec<(String, String)> = c.options().into_iter().map(|o| (o.value, o.label)).collect();
    assert_eq!(got, [("merge".to_string(), "Merge = and push".to_string()), ("hold".to_string(), "Not now".to_string())]);
    assert_eq!(c.needs_note, ["hold"]);
    assert!(parse_choice(&s(&["merge"]), &[]).unwrap_err().to_string().contains("--option takes ID=LABEL"));
    assert!(parse_choice(&[], &s(&["hold"])).unwrap_err().to_string().contains("--require-note goes with --option"));
}

/// The hint after a question asked without options: when its words list some.
#[test]
fn a_question_that_lists_options_is_noticed() {
    use crate::cli::lists_options;
    for yes in ["选一个：合并并推送／先再审一轮／先不合并", "Merge now, or wait?  Choose one", "A / B / C", "cursor | page", "Which option?", "pick the order", "One of: x, y", "先审还是先合并"] {
        assert!(lists_options(&[yes]), "{yes}");
    }
    for no in ["Merge u2 (feat/readme) into main?", "Deploy to staging and/or prod", "u1 is merged; u2 starts next."] {
        assert!(!lists_options(&[no]), "{no}");
    }
    assert!(lists_options(&["Next?", "", "选一个"]));
}

/// `task show` of a choice gate: the options before the output, then the one picked.
#[test]
fn a_choice_gate_shows_its_options_and_the_answer() {
    let opt = |v: &str, l: &str| workriver_core::ChoiceOption { value: v.into(), label: l.into() };
    let choice = workriver_core::Choice::new(&[opt("merge", "核对通过后合并并推送"), opt("review", "先再审一轮"), opt("hold", "先不合并")], &["hold".into()]);
    let part = json!({"id": "u2", "title": "Readme", "about": ""});
    let mut shown = json!({"ref": "u2#8", "kind": "gate", "state": "waiting", "title": "Next?", "intent": part, "unit": null, "place": null, "agent": null,
        "blind": false, "text": "", "writtenBy": "boss", "after": ["7"], "causes": [], "plans": [], "steps": [], "output": null,
        "commits": [], "lostCommits": [], "questions": [], "choice": choice});
    let text = |v: &serde_json::Value| crate::cli::text(&serde_json::from_value(v.clone()).unwrap());
    let waiting = text(&shown);
    assert!(waiting.contains("Choices (the user picks one):\n  merge   核对通过后合并并推送\n  review  先再审一轮\n  hold    先不合并 (a note required)\nOutput: not delivered yet"), "{waiting}");
    shown["output"] = json!({"at": "2026-11-21T10:00:00Z", "deliveredBy": "the user", "note": "after the release", "evidence": [], "findings": [], "files": [], "result": "answered", "selected": ["hold"]});
    let answered = text(&shown);
    assert!(answered.contains("result: answered:\n  Chose: hold \"先不合并\"\n  Note (written by the user):"), "{answered}");
}
