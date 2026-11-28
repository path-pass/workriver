// The user's templates through the service: their YAML read into core's value with the line
// of whatever is wrong, the texts of the ones built in, and the routes that save, remove and
// read one. Only the service reads YAML; the page and the CLI send text.
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use saphyr::{LoadableYamlNode, MarkedYaml, Scalar, YamlData};
use serde::Deserialize;
use serde_json::json;
use workriver_core::{self as core, Body, Template, TemplateStep};

use crate::server::access::*;
use crate::server::app::*;
use crate::server::http::*;

/// The texts of the templates built in, as `template show --yaml` and the page show them. A
/// guard test reads each one and compares it with core's value (`builtin_templates`).
pub(in crate::server) const BUILTIN_TEXTS: [(&str, &str); 3] = [
    (
        "research",
        "name: research
unit: research
description: >
  Finding out by reading before anything is designed or built: a question, its answer,
  and the user's word on where to go from it.
steps:
  - id: research
    kind: research
    description: What was found, as the note, with the report attached.
  - id: direction
    kind: gate
    gate: decide
    description: The user reads the result and says whether the work goes on this way.
",
    ),
    (
        "development",
        "name: development
unit: code
description: >
  Work that lands in the repository. A design, its architecture review and the user's
  confirmation come first, then the implementation, three reviews of it, and the user's go
  before anything leaves the machine. A small fix drops the design steps and says why.
steps:
  - id: design
    kind: design
    description: Settle the shape of the work before code. Delivered as the task's note.
  - id: design-review
    kind: review
    reviewer: architecture-review
    description: The design against the project's rules.
  - id: confirm
    kind: gate
    gate: decide
    description: The user confirms the design, or picks among its choices. Drop it with the
      reason when the design leaves nothing to choose.
  - id: implement
    kind: implement
    description: The work itself, with its tests.
  - id: code-review
    kind: review
    reviewer: code-review
    description: What is wrong in the change.
  - id: security-review
    kind: review
    reviewer: security-review
    after: [implement]
    description: Ways around what the change guards. Leave it out only when the change
      touches no input, access or secret, and say so.
  - id: architecture-review
    kind: review
    reviewer: architecture-review
    after: [implement]
    description: How the change is organized. Leave it out only when it added or moved
      little code, and say how little.
  - id: go
    kind: gate
    gate: decide
    after: [code-review, security-review, architecture-review]
    description: The user's go before the work leaves the machine (push, pull request and
      merge follow, one task each). Filled last, after the last round of reviews.
",
    ),
    (
        "spike",
        "name: spike
unit: spike
description: >
  Finding out by trying: an experiment on a spike/ branch, never merged, and what it
  showed. Code worth keeping goes into a new code unit.
steps:
  - id: experiment
    kind: implement
    description: The code that tries it (measure, compare, prototype).
  - id: learned
    kind: research
    description: What the experiment showed, as the note, with numbers and scripts attached.
",
    ),
];

/// Why a template's text is refused, with the line it is about (1-based) when there is one.
#[derive(Debug, PartialEq)]
pub(in crate::server) struct Refused {
    pub(in crate::server) line: Option<usize>,
    pub(in crate::server) why: String,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(l) => write!(f, "line {l}: {}", self.why),
            None => f.write_str(&self.why),
        }
    }
}

fn refused<T>(line: usize, why: impl Into<String>) -> Result<T, Refused> {
    Err(Refused { line: Some(line), why: why.into() })
}

fn line(n: &MarkedYaml) -> usize {
    n.span.start.line()
}

/// A scalar as text: a string, or what a number or a boolean was written as. Anything else
/// (a list, a mapping, nothing) is refused at its line.
fn text(n: &MarkedYaml, key: &str) -> Result<String, Refused> {
    match &n.data {
        YamlData::Value(Scalar::String(s)) => Ok(s.to_string()),
        YamlData::Value(Scalar::Integer(i)) => Ok(i.to_string()),
        YamlData::Value(Scalar::FloatingPoint(f)) => Ok(f.to_string()),
        YamlData::Value(Scalar::Boolean(b)) => Ok(b.to_string()),
        YamlData::Representation(s, ..) => Ok(s.to_string()),
        _ => refused(line(n), format!("{key} is a text")),
    }
}

/// A word of a fixed list (a kind, a reviewer, a gate, a unit), as core names it.
fn word<T: serde::de::DeserializeOwned>(n: &MarkedYaml, key: &str, one_of: &str) -> Result<T, Refused> {
    let w = text(n, key)?;
    serde_json::from_value(json!(w)).or_else(|_| refused(line(n), format!("{key} {w:?} is not one of {one_of}")))
}

/// The keys of a mapping in order, each with its value; refused when it is no mapping, a key
/// is not text, or a key is not one of `known`.
fn keys<'a, 'b>(n: &'a MarkedYaml<'b>, what: &str, known: &[&str]) -> Result<Vec<(String, usize, &'a MarkedYaml<'b>)>, Refused> {
    let YamlData::Mapping(m) = &n.data else { return refused(line(n), format!("{what} is a mapping of {}", known.join(", "))) };
    let mut out = vec![];
    for (k, v) in m {
        let name = text(k, "a key")?;
        if !known.contains(&name.as_str()) {
            return refused(line(k), format!("{name} is not a key of {what}: {} only", known.join(", ")));
        }
        out.push((name, line(k), v));
    }
    Ok(out)
}

/// Where each part of a template was written, for the line of a refusal.
#[derive(Default)]
struct Lines {
    keys: Vec<(String, usize)>,
    steps: Vec<(usize, Vec<(String, usize)>)>,
}

/// The user's text read into a template, with its rules checked (`Template::checked`): every
/// refusal names its line. The size is checked before anything is read.
pub(in crate::server) fn read(source: &str) -> Result<Template, Refused> {
    if source.len() > core::SOURCE_MAX {
        return Err(Refused { line: None, why: format!("the text is longer than {} KB", core::SOURCE_MAX / 1024) });
    }
    let docs = MarkedYaml::load_from_str(source).map_err(|e| Refused { line: Some(e.marker().line()), why: e.info().to_string() })?;
    let [doc] = &docs[..] else { return refused(1, "the text holds one template: one YAML document") };
    let mut at = Lines::default();
    let (mut name, mut description, mut unit, mut steps) = (None, None, core::UnitKind::Code, None);
    for (key, l, v) in keys(doc, "a template", &["name", "description", "unit", "steps"])? {
        match key.as_str() {
            "name" => name = Some(text(v, "name")?),
            // A folded description (`>`) ends with a line break: not part of what it says.
            "description" => description = Some(text(v, "description")?.trim().to_string()),
            "unit" => unit = word(v, "unit", "code, research, spike")?,
            _ => {
                let YamlData::Sequence(list) = &v.data else { return refused(line(v), "steps is a list of steps") };
                if list.len() > core::STEPS_MAX {
                    return refused(l, format!("{} steps: a template has at most {}", list.len(), core::STEPS_MAX));
                }
                steps = Some(list.iter().map(|s| step(s, &mut at)).collect::<Result<Vec<_>, _>>()?);
            }
        }
        at.keys.push((key, l));
    }
    let need = |what: &str| refused::<Template>(1, format!("{what} is missing"));
    let (Some(name), Some(description), Some(steps)) = (name, description, steps) else {
        let missing = at.keys.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>();
        return need(["name", "description", "steps"].into_iter().find(|k| !missing.contains(k)).unwrap_or("a key"));
    };
    Template { name, description, unit, steps }.checked().map_err(|f| {
        let of = |list: &[(String, usize)], key: Option<&str>| key.and_then(|k| list.iter().find(|(x, _)| x == k)).map(|(_, l)| *l);
        let line = match f.step {
            Some(k) => at.steps.get(k).map(|(l, ks)| of(ks, f.key).unwrap_or(*l)),
            None => of(&at.keys, f.key),
        };
        Refused { line: line.or(Some(1)), why: f.why }
    })
}

/// One step of the list, its keys' lines kept in `at`.
fn step(n: &MarkedYaml, at: &mut Lines) -> Result<TemplateStep, Refused> {
    let mut lines = vec![];
    let (mut id, mut kind, mut description, mut reviewer, mut gate, mut after) = (None, None, None, None, None, None);
    for (key, l, v) in keys(n, "a step", &["id", "kind", "description", "reviewer", "gate", "after"])? {
        match key.as_str() {
            "id" => id = Some(text(v, "id")?),
            "kind" => kind = Some(word(v, "kind", "implement, review, tests, gate, design, research")?),
            "description" => description = Some(text(v, "description")?),
            "reviewer" => reviewer = Some(word(v, "reviewer", "code-review, security-review, architecture-review")?),
            "gate" => gate = Some(word(v, "gate", "decide, deploy, test")?),
            _ => {
                let YamlData::Sequence(list) = &v.data else { return refused(line(v), "after is a list of step ids: [design] (or [] for the start)") };
                after = Some(list.iter().map(|x| text(x, "a step id")).collect::<Result<Vec<_>, _>>()?);
            }
        }
        lines.push((key, l));
    }
    let (Some(id), Some(kind), Some(description)) = (id, kind, description) else {
        let have: Vec<&str> = lines.iter().map(|(k, _)| k.as_str()).collect();
        let missing = ["id", "kind", "description"].into_iter().find(|k| !have.contains(k)).unwrap_or("a key");
        return refused(line(n), format!("a step needs its {missing}"));
    };
    at.steps.push((line(n), lines));
    Ok(TemplateStep { id, kind, description, reviewer, gate, after })
}

/// The text of a built-in template, by name.
pub(in crate::server) fn builtin_text(name: &str) -> Option<&'static str> {
    BUILTIN_TEXTS.iter().find(|(n, _)| *n == name).map(|(_, t)| *t)
}

#[derive(Deserialize)]
pub(in crate::server) struct Save {
    text: String,
}

/// The user saves a template (`workriver template add FILE`, the Templates page): the text is
/// read here, and the template and its text are recorded together (`/api/append` refuses a
/// `template.set`, so the two cannot differ). A text that is wrong is refused with its line.
pub(in crate::server) async fn template_save(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(s): Json<Save>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    let template = read(&s.text).map_err(|r| ApiError(StatusCode::UNPROCESSABLE_ENTITY, r.to_string()))?;
    blocking(move || app.record(&who, Body::TemplateSet { template, source: s.text }).map(Json)).await
}

#[derive(Deserialize)]
pub(in crate::server) struct Remove {
    name: String,
}

/// The user removes a template of theirs; a built-in of that name is back.
pub(in crate::server) async fn template_remove(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(r): Json<Remove>) -> Result<Json<core::Event>, ApiError> {
    let who = app.user(&caller)?;
    blocking(move || app.record(&who, Body::TemplateRemoved { name: r.name }).map(Json)).await
}

#[derive(Deserialize)]
pub(in crate::server) struct Named {
    name: String,
    #[serde(default, rename = "as")]
    named: Option<String>,
}

/// One template and its text (`workriver template show --yaml`, the page's edit form): the
/// user's as saved, else the built-in's. Read through `App::view` like every record; a store
/// that cannot be read fails the request.
pub(in crate::server) async fn template_show(State(app): State<Shared>, Extension(caller): Extension<Caller>, Query(q): Query<Named>) -> Result<Json<serde_json::Value>, ApiError> {
    blocking(move || {
        let r = Reader { named: q.named, light: true, have: String::new() };
        let (_, state) = app.view(&caller, &r)?;
        let Some(t) = state.library().into_iter().find(|t| t.template.name == q.name) else {
            return Err(ApiError(StatusCode::NOT_FOUND, format!("no template {}: `workriver template list` lists them", q.name)));
        };
        let text = match t.revision {
            Some(rev) => app.store.lock().unwrap().template_text(rev)?.ok_or_else(|| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("the text of template {} (revision {rev}) is not in the records", q.name)))?,
            None => builtin_text(&q.name).unwrap_or_default().to_string(),
        };
        Ok(Json(json!({ "template": t, "text": text })))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each built-in's text reads to core's value of it: the two cannot drift.
    #[test]
    fn the_built_in_texts_are_the_built_in_values() {
        let values = core::builtin_templates();
        assert_eq!(BUILTIN_TEXTS.map(|(n, _)| n), ["research", "development", "spike"]);
        for (name, text) in BUILTIN_TEXTS {
            let read = read(text).unwrap_or_else(|r| panic!("{name}: {r}"));
            assert_eq!(Some(&read), values.iter().find(|v| v.name == name), "{name}");
        }
    }

    /// Every refusal names the line it is about: the YAML's own, a key it does not know, a
    /// word it does not know, a rule of core's.
    #[test]
    fn a_refusal_names_its_line() {
        let refusal = |text: &str| read(text).unwrap_err();
        let head = "name: mine\ndescription: Mine.\nsteps:\n";
        let cases = [
            ("name: [mine\n", 2, "flow sequence"),
            ("name: mine\ndescription: d\nsteps: []\ncolor: red\n", 4, "color is not a key of a template"),
            ("name: mine\nname: again\ndescription: d\nsteps: []\n", 2, "duplicated key"),
            (&format!("{head}  - id: a\n    kind: implement\n    description: d\n    agent: x\n"), 7, "agent is not a key of a step"),
            (&format!("{head}  - id: a\n    kind: build\n    description: d\n"), 5, "kind \"build\" is not one of"),
            (&format!("{head}  - id: a\n    kind: implement\n"), 4, "a step needs its description"),
            (&format!("{head}  - id: a\n    kind: implement\n    description: d\n  - id: a\n    kind: review\n    description: d\n"), 7, "used twice"),
            (&format!("{head}  - id: a\n    kind: gate\n    description: d\n"), 4, "says its gate"),
            (&format!("{head}  - id: a\n    kind: review\n    description: d\n    after: [b]\n"), 7, "not a step written before"),
            ("name: Mine\ndescription: d\nsteps: []\n", 1, "a-z"),
            ("description: d\nname: mine\nsteps: []\nunit: lab\n", 4, "unit \"lab\" is not one of"),
            ("name: mine\nsteps: []\n", 1, "description is missing"),
            ("name: mine\ndescription: d\n", 1, "steps is missing"),
            ("- a\n- b\n", 1, "a template is a mapping"),
            ("name: mine\ndescription: d\nsteps: []\n---\nname: two\n", 1, "one YAML document"),
        ];
        for (text, line, says) in cases {
            let r = refusal(text);
            assert_eq!(r.line, Some(line), "{text:?}: {r}");
            assert!(r.why.contains(says), "{text:?}: {r} does not say {says:?}");
        }
        let long = format!("name: mine\ndescription: d\nsteps: []\n#{}\n", "x".repeat(core::SOURCE_MAX));
        assert_eq!(refusal(&long).line, None);
        let many = format!("{head}{}", (0..21).map(|k| format!("  - id: s{k}\n    kind: implement\n    description: d\n")).collect::<String>());
        assert_eq!(refusal(&many), Refused { line: Some(3), why: "21 steps: a template has at most 20".into() });
    }

    /// A step's `after` and its defaults, and the user's comments, as written.
    #[test]
    fn a_template_reads_as_written() {
        let t = read("# mine\nname: mine\ndescription: >\n  Two\n  lines.\nunit: spike\nsteps:\n  - id: try\n    kind: implement\n    description: Try.\n  - id: look\n    kind: review\n    after: []\n    description: Look.\n").unwrap();
        assert_eq!((t.name.as_str(), t.description.as_str(), t.unit), ("mine", "Two lines.", core::UnitKind::Spike));
        assert_eq!((t.steps[1].reviewer, t.steps[1].after.as_deref()), (Some(core::Reviewer::CodeReview), Some(&[][..])));
        assert_eq!(read("name: bare\ndescription: d\nsteps: []\n").unwrap().steps, vec![]);
    }
}
