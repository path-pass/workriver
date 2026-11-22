// The harnesses a manager can be started with from the page, and how a model is given to
// each: workriver's own table. A start names one of them by its kind and a model; nothing
// else of a start's command comes from a request.

/// A harness: the runtime's kind for it (what the runtime starts), the flag that takes a
/// model, and the aliases the harness resolves itself (shown as choices; any other model is
/// one token of the forms `model_ok` takes).
pub struct Harness {
    pub kind: &'static str,
    pub model_flag: &'static str,
    pub aliases: &'static [&'static str],
}

/// Claude Code and Codex. Codex names its models by their slugs and has no aliases.
pub const HARNESSES: &[Harness] = &[
    Harness { kind: "claude", model_flag: "--model", aliases: &["opus", "sonnet", "fable"] },
    Harness { kind: "codex", model_flag: "--model", aliases: &[] },
];

/// The harness of that kind, if it is in the table.
pub fn harness(kind: &str) -> Option<&'static Harness> {
    HARNESSES.iter().find(|h| h.kind == kind)
}

/// Longest model accepted, in bytes.
const MODEL_MAX: usize = 100;

/// A model as a start may give it: empty (the harness's own default), or one token of
/// letters, digits and `. _ : / @ [ ] -`, not starting with `-` (it would be read as a flag).
pub fn model_ok(model: &str) -> bool {
    model.is_empty() || (model.len() <= MODEL_MAX && !model.starts_with('-') && model.chars().all(|c| c.is_ascii_alphanumeric() || "._:/@[]-".contains(c)))
}

impl Harness {
    /// The arguments for `model`: none for the default, else the flag and the model as two
    /// items of a list (never a line of shell). A model of another form is refused.
    pub fn args(&self, model: &str) -> Result<Vec<String>, String> {
        if !model_ok(model) {
            return Err(format!("{model:?} is not a model: leave it empty for the harness's default, or give one name ({}), letters, digits and . _ : / @ [ ] - only", self.choices()));
        }
        Ok(if model.is_empty() { vec![] } else { vec![self.model_flag.to_string(), model.to_string()] })
    }

    fn choices(&self) -> String {
        match self.aliases {
            [] => "its model's name".to_string(),
            a => format!("{} or a model's full name", a.join(", ")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_is_the_default_an_alias_or_one_token() {
        let claude = harness("claude").unwrap();
        assert_eq!(claude.args("").unwrap(), Vec::<String>::new());
        assert_eq!(claude.args("opus").unwrap(), ["--model", "opus"]);
        assert_eq!(claude.args("claude-opus-5-5[1m]").unwrap(), ["--model", "claude-opus-5-5[1m]"]);
        assert_eq!(harness("codex").unwrap().args("gpt-6.2-sol").unwrap(), ["--model", "gpt-6.2-sol"]);
        for bad in ["opus sonnet", "--dangerously-skip-permissions", "-m", "a;b", "$(x)", "a\"b", "a\nb", &"x".repeat(101)] {
            assert!(claude.args(bad).is_err(), "{bad:?}");
        }
        assert!(harness("bash").is_none());
    }
}
