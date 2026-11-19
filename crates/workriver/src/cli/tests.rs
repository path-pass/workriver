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
    let files: Vec<String> = crate::cli::Recipes::iter().map(|f| f.trim_end_matches(".md").to_string()).collect();
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
