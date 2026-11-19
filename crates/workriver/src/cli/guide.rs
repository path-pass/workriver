// `workriver guide`: the skills and recipes embedded in the binary, each under a line naming
// the build, and the stubs to install once in a harness's skills folder.
use super::*;

pub(super) const GUIDE_WORKER: &str = include_str!("../../../../skills/workriver-manager/references/worker.md");

pub(super) const GUIDE_MANAGER: &str = include_str!("../../../../skills/workriver-manager/SKILL.md");

pub(super) const GUIDE_IMPORT: &str = include_str!("../../../../skills/workriver-backlog-import/SKILL.md");

pub(super) const GUIDE_HOW: &str = include_str!("../../../../skills/workriver-howto/SKILL.md");

pub(super) const GUIDE_CHANGES: &str = include_str!("../../../../skills/workriver-howto/CHANGES.md");

/// The recipes of `workriver guide how <topic>`, one file each.
#[derive(rust_embed::Embed)]
#[folder = "../../skills/workriver-howto/references/"]
pub(crate) struct Recipes;

/// A skill file without its front matter (the part between the two `---` lines).
pub(super) fn body(skill: &str) -> &str {
    skill.strip_prefix("---").and_then(|s| s.split_once("\n---\n")).map(|(_, b)| b.trim_start()).unwrap_or(skill)
}

/// What `workriver guide …` prints. Everything but the worker's paragraph starts with a line
/// naming the build, so a reader can tell which version it read; the worker's paragraph
/// stays one line (it goes into a terminal message) and names the build at its start.
pub(crate) fn guide(what: &str, topic: Option<&str>) -> anyhow::Result<String> {
    let head = |name: &str| format!("workriver {BUILD} · {name}\n\n");
    let recipe = |t: &str| Recipes::get(&format!("{t}.md")).map(|f| String::from_utf8_lossy(&f.data).into_owned());
    Ok(match (what, topic) {
        ("worker", _) => format!("(workriver {BUILD}) {}", GUIDE_WORKER),
        ("manager", _) => head("guide manager") + body(GUIDE_MANAGER),
        ("import", _) => head("guide import") + body(GUIDE_IMPORT),
        ("changes", _) => head("guide changes") + GUIDE_CHANGES,
        // An unknown topic gets the index, which lists the ones there are.
        ("how", Some(t)) if recipe(t).is_some() => head(&format!("guide how {t}")) + &recipe(t).unwrap_or_default(),
        ("how", _) => head("guide how") + body(GUIDE_HOW),
        ("install-stub", Some(skill @ ("manager" | "howto" | "import"))) => stub(skill),
        ("install-stub", _) => bail!("which skill? workriver guide install-stub manager|howto|import > <skills folder>/workriver-<name>/SKILL.md"),
        _ => unreachable!("clap checks the values"),
    })
}

/// A skill file to install once into a harness's skills folder: the real skill's name and
/// description (so it is found when needed), and a body that sends its reader to the
/// installed `workriver`, which is never older than itself.
pub(super) fn stub(skill: &str) -> String {
    let (source, command, name) = match skill {
        "manager" => (GUIDE_MANAGER, "workriver guide manager", "workriver manager"),
        "howto" => (GUIDE_HOW, "workriver guide how", "workriver how-to"),
        _ => (GUIDE_IMPORT, "workriver guide import", "workriver backlog import"),
    };
    let front = source.strip_prefix("---").and_then(|s| s.split_once("\n---\n")).map(|(f, _)| f.trim()).unwrap_or_default();
    format!(
        "---\n{front}\n---\n\n# {name}\n\nRun `{command}` and follow what it prints: it is this skill, and it matches the installed\nworkriver (a copy kept here would go stale with every update).\n\nIf the command is not found, workriver is not installed on this machine: tell the user.\n"
    )
}
