// What the CLI reads from its arguments: task and cause references, evidence, steps,
// external ids, projects and links.
use super::*;

/// `A-B` (or one line `A`).
pub(super) fn parse_lines(s: &str) -> anyhow::Result<(u32, u32)> {
    let err = || anyhow!("--lines {s:?}: write A-B, e.g. 10-24");
    let (a, b) = s.split_once('-').unwrap_or((s, s));
    Ok((a.trim().parse().map_err(|_| err())?, b.trim().parse().map_err(|_| err())?))
}

/// The page's path for `i3`, `u1`, `u1#3` (`/u1/3`) or `u1#3:P2-1` (`/u1/3?finding=P2-1`).
pub(crate) fn link_path(s: &str) -> anyhow::Result<String> {
    let bad = || anyhow!("{s:?} is not an intent, unit or task; write it as i3, u1, u1#3 or u1#3:P2-1");
    let (at, finding) = s.split_once(':').map_or((s, None), |(a, f)| (a, Some(f)));
    let (id, n) = at.split_once('#').map_or((at, None), |(u, n)| (u, Some(n)));
    let digits = |x: &str| !x.is_empty() && x.bytes().all(|b| b.is_ascii_digit());
    if !(id.strip_prefix('u').or_else(|| id.strip_prefix('i')).is_some_and(digits) && n.is_none_or(digits)) {
        return Err(bad());
    }
    let mut path = format!("/{id}");
    if let Some(n) = n {
        path += &format!("/{n}");
    }
    match finding {
        None => {}
        Some(f) if n.is_some() && !f.is_empty() && f.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') => path += &format!("?finding={f}"),
        Some(_) => return Err(bad()),
    }
    Ok(path)
}

pub(super) fn task_ref(s: &str) -> anyhow::Result<(String, u32)> {
    parse_task_ref(s).map(|(u, n)| (u.to_string(), n)).ok_or_else(|| anyhow!("{s:?} is not a task; write it as <unit>#<n>, e.g. u1#3"))
}

/// `4:P2-1,P3-1` (this unit), `u1#4:P2-1` (another unit), `f3,f4` (backlog findings).
pub(crate) fn parse_cause(s: &str) -> anyhow::Result<Value> {
    let ids = |x: &str| x.split(',').map(str::trim).filter(|i| !i.is_empty()).map(str::to_string).collect::<Vec<_>>();
    let Some((n, list)) = s.split_once(':') else {
        if s.split(',').all(|f| f.trim().strip_prefix('f').is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))) {
            return Ok(json!({ "task": 0, "findings": ids(s) }));
        }
        bail!("--cause {s:?}: write N:ID[,ID…] (4:P2-1), UNIT#N:ID (u1#4:P2-1), or backlog findings (f3,f4)");
    };
    if let Some((unit, n)) = parse_task_ref(n.trim()) {
        return Ok(json!({ "unit": unit, "task": n, "findings": ids(list) }));
    }
    let n: u32 = n.trim().trim_start_matches('#').parse().map_err(|_| anyhow!("--cause {s:?}: {n:?} is not a task number"))?;
    Ok(json!({ "task": n, "findings": ids(list) }))
}

/// `SYSTEM:KEY` and an optional link.
pub(crate) fn parse_external(external: Option<String>, url: Option<String>) -> anyhow::Result<Option<Value>> {
    let Some(e) = external else { return Ok(None) };
    let (system, key) = e.split_once(':').filter(|(s, k)| !s.trim().is_empty() && !k.trim().is_empty()).ok_or_else(|| anyhow!("--external {e:?}: write SYSTEM:KEY, e.g. jira:ABC-12"))?;
    Ok(Some(json!({ "system": system.trim(), "key": key.trim(), "url": url })))
}

/// A project as the daemon takes it: an id (p1) as is, a folder as its full path.
pub(super) fn project_arg(p: &str) -> anyhow::Result<String> {
    if p.strip_prefix('p').is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit())) && !Path::new(p).exists() {
        return Ok(p.to_string());
    }
    Ok(std::fs::canonicalize(p).with_context(|| format!("project {p:?}: no such id or folder"))?.to_string_lossy().into_owned())
}

pub(super) fn cwd_folder() -> anyhow::Result<String> {
    project_arg(".")
}

/// `TEXT` or `TEXT => COMMAND`
pub(super) fn parse_step(s: &str) -> Value {
    match s.split_once("=>") {
        Some((text, cmd)) => json!({ "text": text.trim(), "command": cmd.trim() }),
        None => json!({ "text": s.trim() }),
    }
}

/// `COMMAND => passed: SUMMARY`; the command may itself hold `=>`, so the last one splits.
/// `COMMAND => passed: SUMMARY`, optionally `=> FILE` after it (a screenshot or report of
/// the check); the file comes back as the second value.
pub(super) fn parse_evidence_file(s: &str) -> anyhow::Result<(Value, Option<PathBuf>)> {
    if let Some((head, last)) = s.rsplit_once("=>")
        && head.contains("=>")
        && parse_evidence(head).is_ok()
        && !last.trim().starts_with("passed")
        && !last.trim().starts_with("failed")
    {
        return Ok((parse_evidence(head)?, Some(PathBuf::from(last.trim()))));
    }
    Ok((parse_evidence(s)?, None))
}

pub(super) fn parse_evidence(s: &str) -> anyhow::Result<Value> {
    let err = || anyhow!("--evidence {s:?}: write 'COMMAND => passed: SUMMARY' or 'COMMAND => failed: SUMMARY' (and `=> FILE` to attach one)");
    let (cmd, rest) = s.rsplit_once("=>").ok_or_else(err)?;
    let (result, summary) = rest.split_once(':').ok_or_else(err)?;
    let result = result.trim();
    if result != "passed" && result != "failed" {
        return Err(err());
    }
    Ok(json!({ "command": cmd.trim(), "result": result, "summary": summary.trim() }))
}

/// `--option ID=LABEL` (repeated, in the order shown) and `--require-note ID`: the stored
/// choice of a decide gate or a manager's message (core checks it), or none without options.
pub(crate) fn parse_choice(options: &[String], require_note: &[String]) -> anyhow::Result<Option<Value>> {
    if options.is_empty() {
        if !require_note.is_empty() {
            bail!("--require-note goes with --option: it names an option whose answer needs a note");
        }
        return Ok(None);
    }
    let options = options
        .iter()
        .map(|o| match o.split_once('=') {
            Some((value, label)) => Ok(core::ChoiceOption { value: value.trim().to_string(), label: label.trim().to_string() }),
            None => Err(anyhow!("--option takes ID=LABEL (e.g. --option 'review=Review again'): {o}")),
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let notes: Vec<String> = require_note.iter().map(|n| n.trim().to_string()).collect();
    Ok(Some(serde_json::to_value(core::Choice::new(&options, &notes))?))
}

/// Whether a message (`say`, `reply`) asked without options lists some in its words (then the
/// command says how to give them; a hint, never a refusal). A decide gate needs no guess: it
/// says what it asks, --go or --option.
pub(crate) fn lists_options(texts: &[&str]) -> bool {
    const WORDS: [&str; 7] = ["选一个", "选项", "还是", "choose", "pick", "one of", "option"];
    texts.iter().any(|t| {
        let t = t.to_lowercase();
        WORDS.iter().any(|w| t.contains(w)) || t.contains('／') || t.matches(" / ").count() >= 2 || t.contains(" | ")
    })
}

/// What `lists_options` makes a command print after it recorded.
pub(crate) const OPTIONS_HINT: &str = "hint: this lists options; give them with --option ID=LABEL each so the user picks one (workriver guide how user-message)";
