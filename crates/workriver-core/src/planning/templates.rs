// Templates: a named list of steps for one kind of unit. A unit names one when it is made,
// and its steps are placed in it as tasks nobody holds yet (`unit.created`'s `placed`). The
// user's library is in the records (`template.set`, `template.removed`); three are built in.
// Their YAML is the service's to read: here a template is a value and its rules.
use serde::{Deserialize, Serialize};

use crate::*;

/// The longest name of a template or a step.
pub const NAME_MAX: usize = 32;
/// The longest description of a template, in characters.
pub const DESCRIPTION_MAX: usize = 2000;
/// The longest description of a step, in characters.
pub const STEP_DESCRIPTION_MAX: usize = 1000;
/// The most steps a template has.
pub const STEPS_MAX: usize = 20;
/// The longest template text the user saves, in bytes.
pub const SOURCE_MAX: usize = 64 * 1024;

/// A template: what it is for (read when choosing one), the kind of unit it is for, and its
/// steps in order.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Template {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub unit: UnitKind,
    #[serde(default)]
    pub steps: Vec<TemplateStep>,
}

/// One step of a template, and of a unit as placed from it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct TemplateStep {
    pub id: String,
    pub kind: TaskKind,
    /// What kind of step this is, read when it is done.
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<Reviewer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<GateKind>,
    /// The steps it follows, by id. None: the step before it (the first: the unit's start);
    /// empty: the start, beside the first. As placed in a unit it is always given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<Vec<String>>,
}

/// Why a template is refused: the step at fault (by its index) and the key, when it is one,
/// so the reader of the text can give the line without parsing the words.
#[derive(Clone, PartialEq, Debug)]
pub struct TemplateFault {
    pub step: Option<usize>,
    pub key: Option<&'static str>,
    pub why: String,
}

impl std::fmt::Display for TemplateFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.why)
    }
}

/// A template of the library as it is read: the user's (with the revision that made it and
/// when), or one built in (which the service adds to what it answers; the records hold only
/// the user's).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct LibraryTemplate {
    #[serde(flatten)]
    pub template: Template,
    /// The sequence of the `template.set` that made it: never reused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub at: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub builtin: bool,
}

/// A task placed from a template: which step it is and what the step is for. The template
/// and its revision are its unit's.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Placed {
    pub id: String,
    pub description: String,
    /// Filled (`task.filled`): given its title, text and agent by the manager or the user.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub filled: bool,
}

fn fault(step: Option<usize>, key: Option<&'static str>, why: String) -> TemplateFault {
    TemplateFault { step, key, why }
}

/// A name of `[a-z0-9-]`, 1 to `NAME_MAX` characters.
fn named(s: &str) -> bool {
    (1..=NAME_MAX).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

impl Template {
    /// The template with its rules checked and its defaults filled in (a review's reviewer:
    /// code-review, as `task add` has it). Everything of the format but the YAML.
    pub fn checked(mut self) -> Result<Template, TemplateFault> {
        if !named(&self.name) {
            return Err(fault(None, Some("name"), format!("name {:?}: 1 to {NAME_MAX} of a-z, 0-9 and -", self.name)));
        }
        if blank(&self.description) {
            return Err(fault(None, Some("description"), "description is empty: say what the template is for and when a unit should use it".into()));
        }
        if self.description.chars().count() > DESCRIPTION_MAX {
            return Err(fault(None, Some("description"), format!("description is longer than {DESCRIPTION_MAX} characters")));
        }
        if self.steps.len() > STEPS_MAX {
            return Err(fault(None, Some("steps"), format!("{} steps: a template has at most {STEPS_MAX}", self.steps.len())));
        }
        let mut seen: Vec<&str> = vec![];
        for (k, s) in self.steps.iter_mut().enumerate() {
            let at = |key, why: String| Err(fault(Some(k), Some(key), why));
            if !named(&s.id) {
                return at("id", format!("step id {:?}: 1 to {NAME_MAX} of a-z, 0-9 and -", s.id));
            }
            if seen.contains(&s.id.as_str()) {
                return at("id", format!("step id {} is used twice", s.id));
            }
            if blank(&s.description) {
                return at("description", format!("step {}: description is empty: say what kind of step this is", s.id));
            }
            if s.description.chars().count() > STEP_DESCRIPTION_MAX {
                return at("description", format!("step {}: description is longer than {STEP_DESCRIPTION_MAX} characters", s.id));
            }
            match s.kind {
                TaskKind::Review => {
                    s.reviewer.get_or_insert(Reviewer::CodeReview);
                }
                _ if s.reviewer.is_some() => return at("reviewer", format!("step {}: reviewer is for a review step", s.id)),
                _ => {}
            }
            match (s.kind, s.gate) {
                (TaskKind::Gate, None) => return at("gate", format!("step {}: a gate step says its gate: decide, deploy or test", s.id)),
                (TaskKind::Gate, Some(_)) => {}
                (_, Some(_)) => return at("gate", format!("step {}: gate is for a gate step", s.id)),
                _ => {}
            }
            if self.unit == UnitKind::Research && matches!(s.kind, TaskKind::Implement | TaskKind::Tests) {
                return at("kind", format!("step {}: a research unit takes no {} step (it has no branch)", s.id, name(&s.kind)));
            }
            for a in s.after.iter().flatten() {
                if !seen.contains(&a.as_str()) {
                    return at("after", format!("step {}: after names {a:?}, which is not a step written before it", s.id));
                }
            }
            seen.push(&s.id);
        }
        Ok(self)
    }

    /// The steps step `k` follows, by id; empty: the unit's start.
    pub fn after_of(&self, k: usize) -> Vec<String> {
        match &self.steps[k].after {
            Some(after) => after.clone(),
            None if k == 0 => vec![],
            None => vec![self.steps[k - 1].id.clone()],
        }
    }

    /// Its steps as a unit places them: each with what it follows written out.
    pub fn placed(&self) -> Vec<TemplateStep> {
        (0..self.steps.len()).map(|k| TemplateStep { after: Some(self.after_of(k)), ..self.steps[k].clone() }).collect()
    }
}

fn step(id: &str, kind: TaskKind, description: &str) -> TemplateStep {
    TemplateStep { id: id.into(), kind, description: description.into(), reviewer: None, gate: None, after: None }
}

fn review(id: &str, reviewer: Reviewer, description: &str) -> TemplateStep {
    TemplateStep { reviewer: Some(reviewer), ..step(id, TaskKind::Review, description) }
}

fn gate(id: &str, gate: GateKind, description: &str) -> TemplateStep {
    TemplateStep { gate: Some(gate), ..step(id, TaskKind::Gate, description) }
}

fn after(s: TemplateStep, ids: &[&str]) -> TemplateStep {
    TemplateStep { after: Some(ids.iter().map(|i| i.to_string()).collect()), ..s }
}

/// The templates built in, as values: what `prepare` places when the user has none of that
/// name. Their texts are the service's (`template show --yaml`, the page); a guard test
/// reads each text and compares it with these.
pub fn builtin_templates() -> Vec<Template> {
    use TaskKind::*;
    vec![
        Template {
            name: "research".into(),
            unit: UnitKind::Research,
            description: "Finding out by reading before anything is designed or built: a question, its answer, and the user's word on where to go from it.".into(),
            steps: vec![
                step("research", Research, "What was found, as the note, with the report attached."),
                gate("direction", GateKind::Decide, "The user reads the result and says whether the work goes on this way."),
            ],
        },
        Template {
            name: "development".into(),
            unit: UnitKind::Code,
            description: "Work that lands in the repository. A design, its architecture review and the user's confirmation come first, then the implementation, three reviews of it, and the user's go before anything leaves the machine. A small fix drops the design steps and says why.".into(),
            steps: vec![
                step("design", Design, "Settle the shape of the work before code. Delivered as the task's note."),
                review("design-review", Reviewer::ArchitectureReview, "The design against the project's rules."),
                gate("confirm", GateKind::Decide, "The user confirms the design, or picks among its choices. Drop it with the reason when the design leaves nothing to choose."),
                step("implement", Implement, "The work itself, with its tests."),
                review("code-review", Reviewer::CodeReview, "What is wrong in the change."),
                after(review("security-review", Reviewer::SecurityReview, "Ways around what the change guards. Leave it out only when the change touches no input, access or secret, and say so."), &["implement"]),
                after(review("architecture-review", Reviewer::ArchitectureReview, "How the change is organized. Leave it out only when it added or moved little code, and say how little."), &["implement"]),
                after(
                    gate("go", GateKind::Decide, "The user's go before the work leaves the machine (push, pull request and merge follow, one task each). Filled last, after the last round of reviews."),
                    &["code-review", "security-review", "architecture-review"],
                ),
            ],
        },
        Template {
            name: "spike".into(),
            unit: UnitKind::Spike,
            description: "Finding out by trying: an experiment on a spike/ branch, never merged, and what it showed. Code worth keeping goes into a new code unit.".into(),
            steps: vec![
                step("experiment", Implement, "The code that tries it (measure, compare, prototype)."),
                step("learned", Research, "What the experiment showed, as the note, with numbers and scripts attached."),
            ],
        },
    ]
}

impl State {
    /// A template by name: the user's, else the one built in. With the user's revision.
    pub fn template(&self, name: &str) -> Option<(Template, Option<u64>)> {
        match self.templates.iter().find(|t| t.template.name == name && !t.builtin) {
            Some(t) => Some((t.template.clone(), t.revision)),
            None => builtin_templates().into_iter().find(|t| t.name == name).map(|t| (t, None)),
        }
    }

    /// Every template a unit can name: the user's, then each built-in the user has none of
    /// (marked built in).
    pub fn library(&self) -> Vec<LibraryTemplate> {
        let own: Vec<LibraryTemplate> = self.templates.iter().filter(|t| !t.builtin).cloned().collect();
        let built = builtin_templates().into_iter().filter(|b| !own.iter().any(|t| t.template.name == b.name));
        own.iter().cloned().chain(built.map(|template| LibraryTemplate { template, revision: None, at: String::new(), builtin: true })).collect()
    }

    /// The names and descriptions of the templates made for a kind of unit, one per line.
    pub(crate) fn templates_for(&self, kind: UnitKind) -> String {
        let fit: Vec<String> = self.library().into_iter().filter(|t| t.template.unit == kind).map(|t| format!("  {}: {}", t.template.name, t.template.description.trim())).collect();
        if fit.is_empty() { "  (none: the user adds one with `workriver template add FILE`)".into() } else { fit.join("\n") }
    }

    /// The rules of a template's events: `prepare`, for these kinds (the user's alone, which
    /// `prepare` checked before).
    pub(crate) fn prepare_templates(&self, _who: &Principal, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        Ok(match body {
            TemplateSet { template, source } => {
                if source.len() > SOURCE_MAX {
                    return no(format!("the template's text is longer than {} KB", SOURCE_MAX / 1024));
                }
                let template = template.checked().map_err(|f| Rejected(f.why))?;
                TemplateSet { template, source }
            }
            TemplateRemoved { name } => {
                if !self.templates.iter().any(|t| t.template.name == name) {
                    let built = builtin_templates().iter().any(|t| t.name == name);
                    return no(if built { format!("{name} is built in: there is nothing of yours to remove (saving one of that name replaces it)") } else { format!("no template {name}") });
                }
                TemplateRemoved { name }
            }
            other => unreachable!("{} is not one of a template's kinds", name_of_kind(&other)),
        })
    }

    /// What a template's events change: `apply`, for these kinds.
    pub(crate) fn apply_templates(&mut self, e: &Event) {
        use Body::*;
        match &e.body {
            TemplateSet { template, .. } => {
                let saved = LibraryTemplate { template: template.clone(), revision: Some(e.seq as u64), at: e.at.clone(), builtin: false };
                match self.templates.iter_mut().find(|t| t.template.name == template.name) {
                    Some(t) => *t = saved,
                    None => self.templates.push(saved),
                }
            }
            TemplateRemoved { name } => self.templates.retain(|t| &t.template.name != name),
            other => unreachable!("{} is not one of a template's kinds", name_of_kind(other)),
        }
    }
}
