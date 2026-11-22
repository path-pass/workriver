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

    /// The problem the harness reported as it started, as its screen shows it: after the line
    /// where the start typed its command (this kind and `args`, as words of that line; the
    /// quotes and backslashes a runtime types around an argument are not counted), the first
    /// line that starts with its mark, without the mark, with the indented lines the terminal
    /// wrapped it onto. What the shell printed before the command is the shell's, never read.
    /// None: no such line, or no line with the command (nothing to tell the harness's from).
    pub fn startup_error(&self, screen: &str, args: &[String]) -> Option<String> {
        let command: Vec<&str> = std::iter::once(self.kind).chain(args.iter().map(String::as_str)).collect();
        let typed = |line: &str| {
            let words: Vec<String> = line.split_whitespace().map(|w| w.replace(['\'', '"', '\\'], "")).collect();
            words.windows(command.len()).any(|w| w.iter().zip(&command).all(|(a, b)| a == b))
        };
        let mut lines = screen.lines().skip_while(|l| !typed(l)).skip(1).skip_while(|l| !l.starts_with(self.error_mark));
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

    fn args(model: &str) -> Vec<String> {
        vec!["--model".into(), model.into()]
    }

    /// What each harness shows as it starts with a model it does not know (Claude Code
    /// 2.1.330, Codex 0.171.0, measured), and with one it knows: read after the line where
    /// the start typed the harness's command.
    #[test]
    fn a_startup_error_is_the_marked_line_after_the_command() {
        let claude = harness("claude").unwrap();
        let banner = |model: &str| format!("zhangdongwei@air proj % claude --model {model}\n ▐▛███▜▌   Claude Code v2.1.330\n▝▜█████▛▘  {model} with high effort · Claude Ultra\n  ▘▘ ▝▝    /tmp/proj\n\n");
        let prompt = "\n────────\n❯ Try \"edit <filepath> to…\"\n────────\n  [opsu] │ proj                                ● high · /effort\n  ⏵⏵ auto mode on (shift+tab to cycle) · ← for agents\n";
        let screen = format!("{}⚠ Model \"opsu\" isn't recognized · did you mean \"opus\"?\n{prompt}", banner("opsu"));
        assert_eq!(claude.startup_error(&screen, &args("opsu")).as_deref(), Some("Model \"opsu\" isn't recognized · did you mean \"opus\"?"));
        // A line the terminal wrapped is one.
        let screen = format!("{}⚠ Model \"claude-nope-9-9\" isn't recognized · update Claude Code, or see\n  https://code.claude.com/docs/en/model-config\n{prompt}", banner("claude-nope-9-9"));
        assert_eq!(claude.startup_error(&screen, &args("claude-nope-9-9")).as_deref(), Some("Model \"claude-nope-9-9\" isn't recognized · update Claude Code, or see https://code.claude.com/docs/en/model-config"));
        assert_eq!(claude.startup_error(&format!("{}{prompt}", banner("opus")), &args("opus")), None);
        assert_eq!(claude.startup_error("", &args("opus")), None);
        // Codex's count of warnings at the end of its status line is not one.
        let codex = harness("codex").unwrap();
        let screen = "% codex --model gpt-nope-1\n  >_ OpenAI Codex (v0.171.0)\n\n⚠ Your ChatGPT model list doesn't include `gpt-nope-1`. Check the spelling or use /model to choose another model.\n\n\n\n› Ask Codex to do anything\n\n  gpt-nope-1 high · /tmp/proj · Context 0% used\n  ← for agents · ? for shortcuts                       ⚠ 6 warnings · f2 to view\n";
        assert_eq!(codex.startup_error(screen, &args("gpt-nope-1")).as_deref(), Some("Your ChatGPT model list doesn't include `gpt-nope-1`. Check the spelling or use /model to choose another model."));
        assert_eq!(codex.startup_error("% codex\n› Ask Codex to do anything\n  ← for agents · ? for shortcuts      ⚠ 6 warnings · f2 to view\n", &[]), None);
    }

    /// What the shell printed before the start typed the command is the shell's, never the
    /// harness's error: a ready agent there is seated.
    #[test]
    fn a_shells_warning_before_the_command_is_not_a_startup_error() {
        let claude = harness("claude").unwrap();
        let shell = "⚠ Optional development tools are not on PATH\n  install them with brew bundle\nzhangdongwei@air proj % ";
        let ready = "\n ▐▛███▜▌   Claude Code v2.1.330\n▝▜█████▛▘  Opus 5.5 · Claude Ultra\n\n────────\n❯ Try \"edit <filepath> to…\"\n────────\n";
        assert_eq!(claude.startup_error(&format!("{shell}claude --model opus{ready}"), &args("opus")), None);
        assert_eq!(claude.startup_error(&format!("{shell}claude{ready}"), &[]), None);
        // After it, the harness's own: still one.
        let wrong = format!("{shell}claude --model opsu\n⚠ Model \"opsu\" isn't recognized · did you mean \"opus\"?\n{ready}");
        assert_eq!(claude.startup_error(&wrong, &args("opsu")).as_deref(), Some("Model \"opsu\" isn't recognized · did you mean \"opus\"?"));
        // The command as the runtime typed it: an argument quoted, a prompt on the right.
        let quoted = format!("{shell}claude --model 'claude-opus-5-5[1m]'\n⚠ Model \"claude-opus-5-5[1m]\" isn't recognized\n{ready}");
        assert_eq!(claude.startup_error(&quoted, &args("claude-opus-5-5[1m]")).as_deref(), Some("Model \"claude-opus-5-5[1m]\" isn't recognized"));
        let right = format!("{shell}claude --model opsu                     ~/proj 10:42\n⚠ Model \"opsu\" isn't recognized\n{ready}");
        assert_eq!(claude.startup_error(&right, &args("opsu")).as_deref(), Some("Model \"opsu\" isn't recognized"));
        // Another command is not the start's: what follows it is not read.
        assert_eq!(claude.startup_error(&format!("{shell}claude --model sonnet\n⚠ Model \"x\" isn't recognized{ready}"), &args("opus")), None);
        // No line with the command (scrolled out of what was read): nothing is read as an error.
        assert_eq!(claude.startup_error("⚠ Optional development tools are not on PATH\n❯ Try\n", &args("opus")), None);
    }
}
