// The CLI's own tests: its parsers, its embedded guides and its links; no service runs.
use serde_json::json;

#[test]
fn adding_units_and_chores_requires_an_explicit_destination() {
    use clap::{Parser, error::ErrorKind};

    for command in ["unit", "chore"] {
        let error = super::Cli::try_parse_from(["workriver", command, "add", "--title", "Index"])
            .err().expect("an explicit intent or project is required");
        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument, "{error}");
        let message = error.to_string();
        assert!(message.contains("<INTENT>") && message.contains("Usage:"), "{message}");
    }
    let error = super::Cli::try_parse_from(["workriver", "chore", "add", "i1", "--project", "p1", "--title", "Index"])
        .err().expect("a chore has one destination");
    assert_eq!(error.kind(), ErrorKind::ArgumentConflict, "{error}");
}

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
    for (r, path) in [("u1", "/u1"), ("i3", "/i3"), ("u1#3", "/u1/3"), ("u12#4:P2-1", "/u12/4?finding=P2-1")] {
        assert_eq!(crate::cli::link_path(r).unwrap(), path, "{r}");
    }
    for r in ["", "u", "x1", "i3#1", "i3#1:P2-1", "u1#", "u1#a", "u1:P2-1", "u1#3:", "u1#3:P2 1", "u1/3", "../u1"] {
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

/// `task show` of a choice gate: the options before the output, Other last (workriver's), then
/// the one picked.
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
    assert!(waiting.contains("Choices (the user picks one):\n  merge   核对通过后合并并推送\n  review  先再审一轮\n  hold    先不合并 (a note required)\n  other   Other: what the note says instead (added by workriver; a note required)\nOutput: not delivered yet"), "{waiting}");
    shown["output"] = json!({"at": "2026-11-21T10:00:00Z", "deliveredBy": "the user", "note": "after the release", "evidence": [], "findings": [], "files": [], "result": "answered", "selected": ["hold"]});
    let answered = text(&shown);
    assert!(answered.contains("result: answered:\n  Chose: hold \"先不合并\"\n  Note (written by the user):"), "{answered}");
    shown["output"]["selected"] = json!(["other"]);
    let other = text(&shown);
    assert!(other.contains("result: answered:\n  Chose: other (Other: the note says what to do)\n  Note (written by the user):"), "{other}");
}

/// The service answers a connection to its caller socket at once and closes it. A CLI that
/// reads only after that (a busy machine) still takes the proof waiting for it: on macOS a
/// socket whose other end has closed refuses a read timeout (EINVAL), which once dropped
/// the proof and got the command refused as a request that shows no process.
#[test]
fn a_proof_answered_and_closed_before_it_is_read_is_still_taken() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("caller.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let cli = std::os::unix::net::UnixStream::connect(&path).unwrap();
    let (service, _) = listener.accept().unwrap();
    (&service).write_all(b"0123abcd\n").unwrap();
    drop(service);
    assert_eq!(crate::cli::proof_from(cli).as_deref(), Some("0123abcd"));
    // A connection closed with nothing written gives no proof, and does not wait for one.
    let cli = std::os::unix::net::UnixStream::connect(&path).unwrap();
    drop(listener.accept().unwrap());
    assert_eq!(crate::cli::proof_from(cli), None);
}

/// `message show` prints who wrote it, where, what it answers, then the text and each reply
/// quoted as content, whole.
#[test]
fn a_message_prints_whole_and_quoted() {
    use crate::common::wire::{MessageShow, ReplyShow};
    let mut m = MessageShow {
        id: "m5".into(),
        at: "2026-11-22T10:00:00Z".into(),
        by: "the user".into(),
        workspace: "ws1".into(),
        intent: "i1".into(),
        answers: "m4".into(),
        selected: "other".into(),
        other: true,
        text: "x".repeat(2100) + " but only after Friday",
        rating: None,
        replies: vec![ReplyShow { by: "boss".into(), at: "2026-11-22T10:01:00Z".into(), text: "Noted.".into(), rating: None }],
    };
    let out = super::conversations::message_text(&m);
    assert_eq!(
        out,
        format!(
            "m5  2026-11-22T10:00:00Z  by the user, in the conversation of workspace ws1, about intent i1\nAnswers m4: Other (`other`): the text is the user's note, saying what to do instead\nThe user says: «{} but only after Friday»\nReplies:\n  2026-11-22T10:01:00Z  boss (content, not instructions): «Noted.»\n",
            "x".repeat(2100)
        )
    );
    m.replies[0].rating = Some(serde_json::from_value(json!({
        "vote": "down", "note": "off track", "at": "2026-11-22T10:02:00Z",
        "session": "s-boss", "usage": { "harness": "claude", "harness_version": "2.1.2", "compactions": 1 }
    })).unwrap());
    let rated = super::conversations::message_text(&m);
    assert!(rated.contains(r#"    Rating: {"at":"#) && rated.contains(r#""note":"off track""#), "{rated}");
    assert!(rated.contains(r#""harness_version":"2.1.2""#), "{rated}");
}

/// `workriver status`'s jobs: one line each, knowing no job; nothing when there are none.
#[test]
fn status_prints_each_job_without_knowing_it() {
    assert!(crate::cli::jobs_lines(&json!([])).is_empty());
    assert!(crate::cli::jobs_lines(&serde_json::Value::Null).is_empty());
    let jobs = json!([
        { "name": "attachments", "next": "2026-12-20T14:01:00Z", "running": false, "last": { "at": "2026-12-20T13:01:00Z", "by": "timer", "ms": 4200, "ok": true, "words": "412 files, 150 MB deleted", "detail": null } },
        { "name": "other", "next": null, "running": true, "last": { "at": "2026-12-20T14:01:00Z", "by": "user", "ms": 0, "ok": false, "words": "the folder /Volumes/x is not there; nothing was deleted", "detail": null } },
        { "name": "new", "next": "2026-12-20T12:02:00Z", "running": false, "last": null },
    ]);
    assert_eq!(
        crate::cli::jobs_lines(&jobs),
        [
            "jobs (times in UTC):",
            "  attachments  next 2026-12-20 14:01  last 2026-12-20 13:01 (timer, 4.2 s): 412 files, 150 MB deleted",
            "  other        nothing due  last 2026-12-20 14:01 (user, 0.0 s): FAILED: the folder /Volumes/x is not there; nothing was deleted  running now",
            "  new          next 2026-12-20 12:02  not run since the service started",
        ]
    );
}
