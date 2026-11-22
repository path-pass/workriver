// The harnesses a manager can be started with from the page, and how a model is given to
// each: workriver's own table. A start names one of them by its kind and a model; nothing
// else of a start's command comes from a request.

/// A harness: the runtime's kind for it (what the runtime starts), the flag that takes a
/// model, the aliases the harness resolves itself (shown as choices; any other model is one
/// token of the forms `model_ok` takes), and the mark that starts a line of its screen when
/// it reports a problem as it starts (a model it does not know).
pub struct Harness {
    pub kind: &'static str,
    pub model_flag: &'static str,
    pub aliases: &'static [&'static str],
    pub error_mark: &'static str,
}

/// Claude Code and Codex. Codex names its models by their slugs and has no aliases. Both
/// start ready, with a session, and only a line of their screen saying so when the model is
/// one they do not know (Claude Code 2.1.330, Codex 0.171.0, measured).
pub const HARNESSES: &[Harness] = &[
    Harness { kind: "claude", model_flag: "--model", aliases: &["opus", "sonnet", "fable"], error_mark: "⚠" },
    Harness { kind: "codex", model_flag: "--model", aliases: &[], error_mark: "⚠" },
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

    /// The problem the harness reported as it started, as its screen shows it: the first line
    /// that starts with its mark, without the mark, with the indented lines the terminal
    /// wrapped it onto. None: its screen reports none.
    pub fn startup_error(&self, screen: &str) -> Option<String> {
        let mut lines = screen.lines().skip_while(|l| !l.starts_with(self.error_mark));
        let first = lines.next()?.trim_start_matches(self.error_mark).trim();
        let wrapped = lines.take_while(|l| l.starts_with(char::is_whitespace) && !l.trim().is_empty()).map(str::trim);
        Some(std::iter::once(first).chain(wrapped).collect::<Vec<_>>().join(" "))
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

    /// What each harness shows as it starts with a model it does not know (Claude Code
    /// 2.1.330, Codex 0.171.0, measured), and with one it knows.
    #[test]
    fn a_startup_error_is_the_marked_line_of_the_screen() {
        let claude = harness("claude").unwrap();
        let banner = "% claude --model opsu\n ▐▛███▜▌   Claude Code v2.1.330\n▝▜█████▛▘  opsu with high effort · Claude Ultra\n  ▘▘ ▝▝    /tmp/proj\n\n";
        let prompt = "\n────────\n❯ Try \"edit <filepath> to…\"\n────────\n  [opsu] │ proj                                ● high · /effort\n  ⏵⏵ auto mode on (shift+tab to cycle) · ← for agents\n";
        let screen = format!("{banner}⚠ Model \"opsu\" isn't recognized · did you mean \"opus\"?\n{prompt}");
        assert_eq!(claude.startup_error(&screen).as_deref(), Some("Model \"opsu\" isn't recognized · did you mean \"opus\"?"));
        // A line the terminal wrapped is one.
        let screen = format!("{banner}⚠ Model \"claude-nope-9-9\" isn't recognized · update Claude Code, or see\n  https://code.claude.com/docs/en/model-config\n{prompt}");
        assert_eq!(claude.startup_error(&screen).as_deref(), Some("Model \"claude-nope-9-9\" isn't recognized · update Claude Code, or see https://code.claude.com/docs/en/model-config"));
        assert_eq!(claude.startup_error(&format!("{banner}{prompt}")), None);
        assert_eq!(claude.startup_error(""), None);
        // Codex's count of warnings at the end of its status line is not one.
        let codex = harness("codex").unwrap();
        let screen = "  >_ OpenAI Codex (v0.171.0)\n\n⚠ Your ChatGPT model list doesn't include `gpt-nope-1`. Check the spelling or use /model to choose another model.\n\n\n\n› Ask Codex to do anything\n\n  gpt-nope-1 high · /tmp/proj · Context 0% used\n  ← for agents · ? for shortcuts                       ⚠ 6 warnings · f2 to view\n";
        assert_eq!(codex.startup_error(screen).as_deref(), Some("Your ChatGPT model list doesn't include `gpt-nope-1`. Check the spelling or use /model to choose another model."));
        assert_eq!(codex.startup_error("› Ask Codex to do anything\n  ← for agents · ? for shortcuts      ⚠ 6 warnings · f2 to view\n"), None);
    }
}
