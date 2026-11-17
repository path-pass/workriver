// A command the user runs from the page (a gate's or chore's step, or one typed in the side
// panel): typed into a plain shell of the user's in the runtime, so it runs exactly as if the
// user typed it there, with their credentials and hooks. What the shell is told, and how the
// end is seen in what the pane shows. Herdr is not named here: the runtime does the typing.

/// The runtime group the shells are in (a Herdr workspace of that name), made on first use.
pub const GROUP: &str = "workriver-commands";

/// How many lines of a pane are read each time.
pub const LINES: u32 = 400;

/// A command the page may run: one line, nothing a terminal would act on by itself.
pub fn check(command: &str) -> Result<(), String> {
    if command.trim().is_empty() {
        return Err("no command given".into());
    }
    if command.chars().any(char::is_control) {
        return Err("a command is one line, without control characters".into());
    }
    if command.chars().count() > 4000 {
        return Err("a command is at most 4000 characters".into());
    }
    Ok(())
}

/// `'text'` for a POSIX shell.
fn quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// What is typed: go to `dir`, run the command as written (through `eval`, so a trailing `#`
/// comment or `&` cannot swallow the rest), then print the end marker with its exit status.
/// The marker as typed carries `$?`, not digits, so the echo of this line never looks like
/// the end. `id` is random: the command's own output cannot fake its end.
// ponytail: POSIX shells (zsh, bash, sh); a fish user's shell would need `$status`.
pub fn line(dir: &str, command: &str, id: &str) -> String {
    format!("cd -- {} && eval {}; echo \"__workriver_end_{id}_$?__\"", quoted(dir), quoted(command))
}

/// What the command printed so far (the pane after the echo of the typed line, which a
/// shell may show twice: typed ahead, then at its prompt) and, once it ended, its exit status.
pub fn read(text: &str, id: &str) -> (String, Option<i32>) {
    let typed = format!("__workriver_end_{id}_$?__");
    let text = match text.rfind(&typed) {
        Some(k) => text[k..].split_once('\n').map_or("", |(_, rest)| rest),
        None => text,
    };
    let mark = format!("__workriver_end_{id}_");
    let mut at = 0;
    for l in text.split_inclusive('\n') {
        let code = l.trim().strip_prefix(&mark).and_then(|r| r.strip_suffix("__")).and_then(|d| d.parse().ok());
        if code.is_some() {
            return (text[..at].trim_end().to_string(), code);
        }
        at += l.len();
    }
    (text.trim_end().to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_line_and_end() {
        assert!(check("git push origin feat/x").is_ok());
        assert!(check("git push\nrm -rf ~").is_err() && check(" ").is_err() && check("ls \u{1b}[2J").is_err());
        let l = line("/w/it's", "git push # go", "ab12");
        assert_eq!(l, r#"cd -- '/w/it'\''s' && eval 'git push # go'; echo "__workriver_end_ab12_$?__""#);
        // The echo of the typed line (twice: typed ahead, then at the prompt) is not the end.
        let typed = format!("{l}\n% {l}\n");
        assert_eq!(read(&typed, "ab12"), (String::new(), None));
        assert_eq!(read(&format!("{typed}Counting objects"), "ab12"), ("Counting objects".into(), None));
        assert_eq!(read(&format!("{typed}Everything up-to-date\n__workriver_end_ab12_0__\n% "), "ab12"), ("Everything up-to-date".into(), Some(0)));
        assert_eq!(read("x\n__workriver_end_ab12_128__\n", "ab12"), ("x".into(), Some(128)));
        assert_eq!(read("__workriver_end_ff00_0__\n", "ab12").1, None, "another run's marker");
        assert_eq!(read("echo __workriver_end_ab12_0__ inside a line\n", "ab12").1, None, "only a line of its own");
    }
}
