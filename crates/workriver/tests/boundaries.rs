//! Where the runtime's names may appear (docs/design/restructure.md, section 5.1): a
//! ratchet over the whole repository's sources. `herdr` (any case) and `pane(s)` may appear
//! outside the runtime adapter only in the files listed here, as of the start of the
//! restructure; a new file naming them fails, and a listed file that stops naming them must
//! leave its list, so the lists only shrink. A moved file's line follows it in the same commit.
//!
//! Not counted: i18n key names (a dictionary's keys, and a quoted string that is one of
//! those keys, such as 'machines.noHerdr': keys stay as they are, their texts count; any
//! other quoted name, such as a runtime method "pane.close", counts), and words used in
//! workriver's own sense (`OWN_SENSE`). `tab` and `workspace` are not checked: workriver
//! has workspaces of its own (an intent runs in one), and `tab` names nothing of the
//! runtime's in the sources outside the adapter.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The runtime adapter's folder, whose files may name them.
const ADAPTER: &str = "crates/workriver/src/runtime/herdr/";

/// Files where a word means something of workriver's own, not the runtime's.
const OWN_SENSE: &[(&str, &str, &str)] = &[("frontend/web/src/components/CodeLayer.tsx", "pane", "the file tree beside the diff, a layout term (selector `data-tree=\"pane\"`)")];

const STILL_HERDR: &[&str] = &[
    "crates/workriver-core/src/lib.rs",
    "crates/workriver-core/src/planning/projects.rs",
    "crates/workriver-core/src/runtime.rs",
    "crates/workriver/src/common/process.rs",
    "crates/workriver/src/metrics.rs",
    "crates/workriver/src/run.rs",
    // The facade: it declares the adapter's module and picks it, by its name.
    "crates/workriver/src/runtime/mod.rs",
    "crates/workriver/src/serve.rs",
    "crates/workriver/src/snapshot.rs",
    "crates/workriver/src/tests.rs",
    "crates/workriver/tests/caller.rs",
    "crates/workriver/tests/cli.rs",
    "crates/workriver/tests/output.rs",
    "frontend/web/src/components/Run.tsx",
    "frontend/web/src/components/Start.tsx",
    "frontend/web/src/i18n/en.ts",
    "frontend/web/src/i18n/ja.ts",
    "frontend/web/src/i18n/zh-CN.ts",
    "frontend/web/src/runtime.ts",
    "frontend/web/src/types.ts",
];
const STILL_PANE: &[&str] = &[
    "crates/workriver-core/src/planning/intents.rs",
    "crates/workriver-core/src/planning/projects.rs",
    "crates/workriver-core/src/work/findings.rs",
    "crates/workriver/src/cli/client.rs",
    "crates/workriver/src/cli/mod.rs",
    "crates/workriver/src/run.rs",
    "crates/workriver/src/serve.rs",
    "crates/workriver/src/tests.rs",
    "crates/workriver/tests/caller.rs",
    "crates/workriver/tests/cli.rs",
    "crates/workriver/tests/output.rs",
    "frontend/web/src/i18n/en.ts",
    "frontend/web/src/runtime.ts",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

/// Every file under `dir` with one of these extensions, skipping build output and packages.
fn sources(dir: &Path, ext: &[&str]) -> Vec<PathBuf> {
    let mut out = vec![];
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy();
        if p.is_dir() {
            if !matches!(name.as_ref(), "target" | "node_modules" | "dist" | ".git") {
                out.extend(sources(&p, ext));
            }
        } else if p.extension().is_some_and(|e| ext.iter().any(|x| e == *x)) {
            out.push(p);
        }
    }
    out
}

/// The key of a dictionary line (`  noHerdr: '…'`, `  'runtime-down': '…'`, `  machines: {`),
/// and what follows its colon.
fn key_of(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start();
    let (key, rest) = match line.strip_prefix('\'') {
        Some(quoted) => quoted.split_once("':")?,
        None => line.split_once(':')?,
    };
    (!key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')).then_some((key, rest))
}

/// Every key of the English dictionary (it defines the key set), by its dotted path, and each
/// plural key also without its `_one`/`_other` (as `t()` names it).
fn keys() -> BTreeSet<String> {
    let text = std::fs::read_to_string(root().join("frontend/web/src/i18n/en.ts")).unwrap();
    let body = text.split_once("export const en = {").expect("the dictionary").1;
    let (mut at, mut out) = (Vec::<String>::new(), BTreeSet::new());
    for line in body.lines() {
        if line.trim_start().starts_with('}') {
            at.pop();
        } else if let Some((key, rest)) = key_of(line) {
            if rest.trim() == "{" {
                at.push(key.to_string());
            } else {
                let path = at.iter().map(String::as_str).chain([key]).collect::<Vec<_>>().join(".");
                for plural in ["_one", "_other", "_zero", "_two", "_few", "_many"] {
                    if let Some(base) = path.strip_suffix(plural) {
                        out.insert(base.to_string());
                    }
                }
                out.insert(path);
            }
        }
    }
    out
}

/// The text without i18n key names: a dictionary's keys (in `i18n/`) and quoted strings that
/// are keys (`'machines.noHerdr'`) in the page's code. Nothing else is taken out.
fn without_keys(rel: &str, text: &str, keys: &BTreeSet<String>) -> String {
    if !(rel.ends_with(".ts") || rel.ends_with(".tsx")) {
        return text.to_string();
    }
    let dictionary = rel.contains("/i18n/");
    let mut out = String::new();
    for line in text.lines() {
        let mut line = line.to_string();
        if dictionary && let Some((_, rest)) = key_of(&line) {
            line = rest.to_string();
        }
        let mut kept = String::new();
        let mut rest = line.as_str();
        while let Some(at) = rest.find(['\'', '"']) {
            let q = rest.as_bytes()[at] as char;
            kept += &rest[..=at];
            let after = &rest[at + 1..];
            let Some(end) = after.find(q) else {
                rest = after;
                continue;
            };
            let inner = &after[..end];
            if !keys.contains(inner) {
                kept += inner;
            }
            kept.push(q);
            rest = &after[end + 1..];
        }
        kept += rest;
        out += &kept;
        out.push('\n');
    }
    out
}

fn names(text: &str, word: &str) -> bool {
    let lower = text.to_lowercase();
    if word == "herdr" {
        return lower.contains("herdr");
    }
    lower.match_indices(word).any(|(at, _)| {
        let before = lower[..at].chars().next_back().is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let rest = &lower[at + word.len()..];
        let rest = rest.strip_prefix('s').unwrap_or(rest);
        before && rest.chars().next().is_none_or(|c| !c.is_alphanumeric() && c != '_')
    })
}

#[test]
fn no_runtime_names_outside_the_adapter_but_those_still_listed() {
    let root = root();
    let mut files = sources(&root.join("crates"), &["rs"]);
    files.extend(sources(&root.join("frontend/web/src"), &["ts", "tsx"]));
    let me = "crates/workriver/tests/boundaries.rs";
    let keys = keys();
    for (word, still) in [("herdr", STILL_HERDR), ("pane", STILL_PANE)] {
        let found: BTreeSet<String> = files
            .iter()
            .map(|f| (f.strip_prefix(&root).unwrap().to_string_lossy().into_owned(), f))
            .filter(|(rel, _)| rel != me && !rel.starts_with(ADAPTER) && !OWN_SENSE.iter().any(|(f, w, _)| f == rel && *w == word))
            .filter(|(rel, f)| names(&without_keys(rel, &std::fs::read_to_string(f).unwrap(), &keys), word))
            .map(|(rel, _)| rel)
            .collect();
        let listed: BTreeSet<String> = still.iter().map(|s| s.to_string()).collect();
        let new: Vec<_> = found.difference(&listed).collect();
        let gone: Vec<_> = listed.difference(&found).collect();
        assert!(new.is_empty(), "{word:?} named outside the runtime adapter, in files not on the list: {new:?}");
        assert!(gone.is_empty(), "{word:?} no longer named in {gone:?}: take them off the list (it only shrinks)");
    }
}

#[test]
fn key_names_and_own_words_do_not_count() {
    let keys = keys();
    assert!(keys.contains("machines.noHerdr") && keys.iter().any(|k| k.ends_with(".reason.runtime-down")) && keys.contains("unit.blocking"), "{keys:?}");
    let page = without_keys("frontend/web/src/components/Settings.tsx", "t('machines.noHerdr') + 'Herdr is down' + \"pane.close\" + // \"pane.close\"", &keys);
    assert_eq!(page, "t('') + 'Herdr is down' + \"pane.close\" + // \"pane.close\"\n");
    assert!(names(&page, "pane"), "a quoted runtime method that is not a key counts");
    assert_eq!(without_keys("frontend/web/src/i18n/en.ts", "    noHerdr: 'Herdr does not answer',", &keys), " 'Herdr does not answer',\n");
    assert_eq!(without_keys("frontend/web/src/i18n/en.ts", "      'runtime-down': 'Herdr is not reachable',", &keys), " 'Herdr is not reachable',\n");
    assert!(names("in its pane.", "pane") && names("Panes", "pane") && !names("panel", "pane") && !names("a_pane", "pane"));
}
