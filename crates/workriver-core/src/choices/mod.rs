// A question's options, as they are recorded: a decide gate's or a manager's message's. The
// record is a fixed subset of A2UI v1.0 (a `createSurface` message: a Column holding a
// single-choice ChoicePicker and a Button whose label is a Text), so that A2UI's own renderer
// can draw it one day. It is a record format, not UI: this module holds its write rule (build
// it from options, check it, read the options back). Core neither draws it nor holds any UI
// text; `/ui/submit` is only a path in the record's data model that the page fills.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::*;

pub const MIN_OPTIONS: usize = 2;
pub const MAX_OPTIONS: usize = 8;
const MAX_LABEL: usize = 200;
const VERSION: &str = "1.0";
const CATALOG: &str = "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json";

/// A question's options: the A2UI surface, and the values whose answer needs a note (a rule of
/// workriver's own: A2UI's option items hold a label and a value only).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub surface: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs_note: Vec<String>,
}

/// One option: the value stored and sent back, and the words shown (the asker's).
#[derive(Clone, PartialEq, Debug)]
pub struct ChoiceOption {
    pub value: String,
    pub label: String,
}

fn good_value(v: &str) -> bool {
    (1..=32).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

impl Choice {
    /// The stored form of these options; `check` says whether they may be recorded.
    pub fn new(options: &[ChoiceOption], needs_note: &[String]) -> Choice {
        let selected = json!({"@path": "/selected"});
        let items: Vec<Value> = options.iter().map(|o| json!({"label": o.label, "value": o.value})).collect();
        let surface = json!({"version": VERSION, "createSurface": {
            "surfaceId": "choice",
            "catalogId": CATALOG,
            "components": [
                {"id": "root", "component": "Column", "children": ["options", "submit"]},
                {"id": "options", "component": "ChoicePicker", "variant": "mutuallyExclusive", "options": items, "value": selected},
                // A2UI disables a Button while one of its checks fails: no answer without a pick.
                {"id": "submit", "component": "Button", "variant": "primary", "child": "submit-label",
                 "action": {"event": {"name": "answer", "context": {"selected": selected}}},
                 "@checks": [{"condition": {"@call": "required", "args": {"value": selected}}}]},
                {"id": "submit-label", "component": "Text", "text": {"@path": "/ui/submit"}}
            ],
            "dataModel": {"selected": []}}});
        Choice { surface, needs_note: needs_note.to_vec() }
    }

    /// The options, in the order shown; none when the surface is not of the subset's shape.
    pub fn options(&self) -> Vec<ChoiceOption> {
        let items = self.surface.pointer("/createSurface/components/1/options").and_then(Value::as_array);
        items
            .into_iter()
            .flatten()
            .map_while(|o| Some(ChoiceOption { value: o.get("value")?.as_str()?.to_string(), label: o.get("label")?.as_str()?.to_string() }))
            .collect()
    }

    /// The words of the option with this value.
    pub fn label(&self, value: &str) -> Option<String> {
        self.options().into_iter().find(|o| o.value == value).map(|o| o.label)
    }

    pub fn needs_note(&self, value: &str) -> bool {
        self.needs_note.iter().any(|v| v == value)
    }

    /// Whether it may be recorded: good options, and exactly the form `new` makes of them.
    pub(crate) fn check(&self) -> Result<(), Rejected> {
        let options = self.options();
        if *self != Choice::new(&options, &self.needs_note) {
            return no("a choice is stored as workriver's A2UI 1.0 subset; give it with --option ID=LABEL");
        }
        if !(MIN_OPTIONS..=MAX_OPTIONS).contains(&options.len()) {
            return no(format!("a choice has {MIN_OPTIONS} to {MAX_OPTIONS} options, not {}", options.len()));
        }
        for (k, o) in options.iter().enumerate() {
            if !good_value(&o.value) {
                return no(format!("an option's ID is 1 to 32 of [a-z0-9-], not {:?}", o.value));
            }
            if options[..k].iter().any(|p| p.value == o.value) {
                return no(format!("option {} is given twice", o.value));
            }
            if blank(&o.label) || o.label.contains(['\n', '\r']) || o.label.chars().count() > MAX_LABEL {
                return no(format!("option {}'s label is one line of 1 to {MAX_LABEL} characters", o.value));
            }
        }
        if let Some(v) = self.needs_note.iter().find(|v| !options.iter().any(|o| &o.value == *v)) {
            return no(format!("--require-note {v} names no option"));
        }
        Ok(())
    }

    /// Whether this is an answer to it: one of its values, with a note where that one asks one.
    pub(crate) fn check_answer(&self, selected: &[String], note: &str) -> Result<(), Rejected> {
        let [value] = selected else { return no("pick one option to answer") };
        if self.label(value).is_none() {
            let values: Vec<_> = self.options().into_iter().map(|o| o.value).collect();
            return no(format!("{value:?} is not one of the options ({})", values.join(", ")));
        }
        if self.needs_note(value) && blank(note) {
            return no(format!("option {value} needs a note saying why"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
