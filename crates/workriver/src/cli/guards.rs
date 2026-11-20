//! The CLI's guards (docs/design/guards.md): the
//! guide's topics with their recipes, and every command with its options. Agents run these
//! exact command lines (the recipes and workriver's own messages name them), so a lost
//! option or a misrouted topic breaks other agents' work. A move keeps these green
//! unchanged.
use std::collections::BTreeMap;

use clap::CommandFactory;

fn crate_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The recipes `workriver guide how <topic>` knows, by name. Moving or grouping a recipe
/// keeps its name (its file's basename); adding or removing one changes this list in the
/// same commit.
const TOPICS: &[&str] = &[
    "architecture-review",
    "ask",
    "ask-user",
    "attachments",
    "blind-review",
    "blocked",
    "branch-moved",
    "closing-units",
    "closing-workers",
    "commit-link",
    "contract",
    "deciding-findings",
    "deferred-batches",
    "design-gate",
    "discussion",
    "finding-additions",
    "gates-chores",
    "hand-off",
    "machines",
    "not-delivered",
    "push-pr-merge",
    "spike",
    "stalled",
    "start-intent",
    "units",
    "units-other-machine",
    "updating",
    "user-message",
];

/// Every recipe file under the howto skill's `references/`, by basename, wherever it sits
/// (grouped into folders or not).
fn recipe_files() -> BTreeMap<String, Vec<std::path::PathBuf>> {
    fn walk(dir: &std::path::Path, out: &mut BTreeMap<String, Vec<std::path::PathBuf>>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, out);
            } else if let Some(name) = p.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_suffix(".md")) {
                out.entry(name.to_string()).or_default().push(p);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(&crate_dir().join("../../skills/workriver-howto/references"), &mut out);
    out
}

/// Every topic answers with its own recipe, word for word, under a head naming it; the
/// embedded recipes are exactly these, by basename, each name once; and every kind of guide
/// still answers.
#[test]
fn the_guide_keeps_its_topics() {
    let files = recipe_files();
    for topic in TOPICS {
        let paths = &files[*topic];
        assert_eq!(paths.len(), 1, "{topic}: one recipe file with that name, not {paths:?}");
        let recipe = std::fs::read_to_string(&paths[0]).unwrap();
        let text = crate::cli::guide("how", Some(topic)).unwrap();
        assert_eq!(text, format!("workriver {} · guide how {topic}\n\n{recipe}", crate::common::BUILD), "{topic} answers with another text than its recipe");
    }
    assert_eq!(files.keys().map(String::as_str).collect::<Vec<_>>(), TOPICS);
    let embedded: Vec<String> = crate::cli::Recipes::iter().map(|f| f.rsplit('/').next().unwrap().trim_end_matches(".md").to_string()).collect();
    let mut unique = embedded.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), embedded.len(), "two embedded recipes share a name: {embedded:?}");
    assert_eq!(unique, TOPICS);
    for (what, topic) in [("worker", None), ("manager", None), ("import", None), ("how", None), ("changes", None), ("install-stub", Some("manager")), ("install-stub", Some("howto")), ("install-stub", Some("import"))] {
        assert!(crate::cli::guide(what, topic).is_ok_and(|t| !t.is_empty()), "{what} {topic:?}");
    }
}

/// One line per command (its path and aliases) and per argument: how it is written (long,
/// short, aliases, positional index), what it takes (value names, how many, possible
/// values, defaults, environment variable) and how (action, required, global, hidden).
fn tree(cmd: &clap::Command, path: &str, out: &mut String) {
    let aliases: Vec<&str> = cmd.get_all_aliases().collect();
    out.push_str(&format!("{path}{}{}\n", if aliases.is_empty() { String::new() } else { format!("  aliases={aliases:?}") }, if cmd.is_hide_set() { "  hidden" } else { "" }));
    let mut args: Vec<&clap::Arg> = cmd.get_arguments().filter(|a| path == "workriver" || !a.is_global_set()).collect();
    args.sort_by_key(|a| a.get_id().as_str().to_string());
    for a in args {
        let mut line = format!("{path}  arg {}", a.get_id());
        if let Some(i) = a.get_index() {
            line += &format!(" index={i}");
        }
        if let Some(l) = a.get_long() {
            line += &format!(" --{l}");
        }
        if let Some(s) = a.get_short() {
            line += &format!(" -{s}");
        }
        if let Some(al) = a.get_all_aliases() {
            line += &format!(" aliases={al:?}");
        }
        if let Some(al) = a.get_all_short_aliases() {
            line += &format!(" short-aliases={al:?}");
        }
        if let Some(v) = a.get_value_names() {
            line += &format!(" value={:?}", v.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        }
        if let Some(n) = a.get_num_args() {
            line += &format!(" num={n}");
        }
        let possible: Vec<String> = a.get_possible_values().iter().map(|p| p.get_name().to_string()).collect();
        if !possible.is_empty() {
            line += &format!(" one-of={possible:?}");
        }
        let defaults: Vec<String> = a.get_default_values().iter().map(|d| d.to_string_lossy().into_owned()).collect();
        if !defaults.is_empty() {
            line += &format!(" default={defaults:?}");
        }
        if let Some(e) = a.get_env() {
            line += &format!(" env={}", e.to_string_lossy());
        }
        line += &format!(" action={:?}", a.get_action());
        for (set, word) in [(a.is_required_set(), "required"), (a.is_global_set(), "global"), (a.is_hide_set(), "hidden"), (a.is_last_set(), "last"), (a.is_trailing_var_arg_set(), "trailing")] {
            if set {
                line += &format!(" {word}");
            }
        }
        for c in cmd.get_arg_conflicts_with(a) {
            line += &format!(" conflicts={}", c.get_id());
        }
        out.push_str(&line);
        out.push('\n');
    }
    let mut subs: Vec<&clap::Command> = cmd.get_subcommands().collect();
    subs.sort_by_key(|c| c.get_name().to_string());
    for s in subs {
        tree(s, &format!("{path} {}", s.get_name()), out);
    }
}

/// Every command and option the CLI takes, as clap knows them, pinned in `cli-tree.txt`:
/// a move of the CLI's code keeps every name, alias, default, requirement
/// and value. `WORKRIVER_BLESS=1` writes the file again after a deliberate change.
#[test]
fn every_command_keeps_its_options() {
    let mut cmd = crate::cli::Cli::command();
    cmd.build();
    let mut got = String::new();
    tree(&cmd, "workriver", &mut got);
    let path = crate_dir().join("src/cli/cli-tree.txt");
    if std::env::var("WORKRIVER_BLESS").is_ok_and(|v| v == "1") {
        std::fs::write(&path, &got).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap();
    if got != want {
        let (g, w): (Vec<&str>, Vec<&str>) = (got.lines().collect(), want.lines().collect());
        panic!("the CLI's commands changed; gone: {:?}; new: {:?}", w.iter().filter(|l| !g.contains(l)).collect::<Vec<_>>(), g.iter().filter(|l| !w.contains(l)).collect::<Vec<_>>());
    }
}
