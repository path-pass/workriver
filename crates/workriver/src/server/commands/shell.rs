// A command the user runs from the page (a gate's or chore's step, or one typed in the side
// panel): typed into a plain shell of the user's in the runtime, so it runs exactly as if the
// user typed it there, with their credentials and hooks. What the shell is told, and how the
// end is seen in what the shell shows. No runtime is named here: the runtime does the typing.
use std::time::Duration;

use workriver_core::runtime::RuntimeError;
use workriver_core::{Body, Principal};

use super::*;

/// The runtime group the shells are in (by that name), made on first use.
pub(in crate::server) const GROUP: &str = "workriver-commands";

/// How many lines of a shell's place are read each time.
const LINES: u32 = 400;

/// A command the page may run: one line, nothing a terminal would act on by itself.
pub(super) fn check(command: &str) -> Result<(), String> {
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
pub(super) fn line(dir: &str, command: &str, id: &str) -> String {
    format!("cd -- {} && eval {}; echo \"__workriver_end_{id}_$?__\"", quoted(dir), quoted(command))
}

/// What the command printed so far (the text after the echo of the typed line, which a
/// shell may show twice: typed ahead, then at its prompt) and, once it ended, its exit status.
fn read(text: &str, id: &str) -> (String, Option<i32>) {
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

impl App {
    /// Reads a run's place every second until it shows the end (then records the exit status),
    /// its place is gone, or a day has passed.
    pub(super) fn watch_run(&self, id: &str) {
        let Some((machine, place)) = self.runs.lock().unwrap().get(id).and_then(|r| Some((r.machine.clone(), r.place.clone()?))) else { return };
        let (until, mut misses, mut well) = (std::time::Instant::now() + Duration::from_secs(24 * 3600), 0, false);
        // A place that cannot be read five times running is gone (closed, or its shell exited).
        while std::time::Instant::now() < until && misses < 5 {
            std::thread::sleep(Duration::from_secs(1));
            let Ok(text) = self.runtime.read_place(machine.as_deref(), &place, shell::LINES) else {
                misses += 1;
                continue;
            };
            misses = 0;
            let (output, end) = shell::read(&text, id);
            if let Some(r) = self.runs.lock().unwrap().get_mut(id) {
                (r.output, r.exit) = (output, end);
                // Ended well: its shell is to go, said with the end so no reader sees one
                // without the other.
                r.closed |= end == Some(0);
            }
            if let Some(exit) = end {
                if let Err(e) = self.record(&Principal::Service, Body::CommandEnded { id: id.to_string(), exit }) {
                    tracing::warn!("cannot record run {id} ended: {}", e.1);
                }
                // Ended well: its output is kept above; its shell goes, below.
                well = exit == 0;
                break;
            }
        }
        if let Some(r) = self.runs.lock().unwrap().get_mut(id) {
            r.watching = false;
        }
        // No longer followed: the page's recent runs show it (an end recorded says so too).
        let _ = self.changed.send(());
        if well {
            self.close_shells(vec![(id.to_string(), machine, place)], CLOSE_DELAY);
        }
    }

    /// Closes, after `wait`, the shells of runs that ended and were claimed for it (`closed`
    /// set): each on its own machine, and only one with nothing left running in it. A shell
    /// that is busy (a background job of the run's, or the user went on working there), or
    /// that could not be asked or closed just now, is left and given back: the next run tries
    /// again. One that is gone, or no longer a run's shell, is never asked about again. Only
    /// places `run_shell` gave a run are ever passed on; a run whose end was not seen
    /// (running, waiting for input) is never claimed. Blocking.
    // ponytail: the places are in memory, so shells left by a service that restarted stay
    // until the user closes them; recording a run's place would let the next start close them.
    pub(super) fn close_shells(&self, shells: Vec<Shell>, wait: Duration) {
        std::thread::sleep(wait);
        let mut left = false;
        for (id, machine, place) in shells {
            let done = self.shell_idle(machine.as_deref(), &place).and_then(|idle| match idle {
                true => self.runtime.close_shell(machine.as_deref(), shell::GROUP, &place).map(|()| true),
                false => Ok(false),
            });
            match done {
                Ok(true) => continue,
                Err(RuntimeError::NoPlace(why)) => {
                    tracing::debug!("the shell of run {id} is not closed: {why}");
                    continue;
                }
                Ok(false) => tracing::debug!("the shell of run {id} is busy: left open"),
                Err(e) => tracing::warn!("cannot close the shell of run {id} now: {e}"),
            }
            if let Some(r) = self.runs.lock().unwrap().get_mut(&id) {
                r.closed = false;
                left = true;
            }
        }
        if left {
            let _ = self.changed.send(());
        }
    }

    /// Whether nothing is left running in a run's shell: nothing but the shell in front, and
    /// nothing the shell started still alive (a job in the background is not in front, so the
    /// machine's own process list is read for it).
    // ponytail: a shell whose prompt keeps a helper process of its own alive always counts as
    // busy and is never closed for the user; safe, and none of the stock shells does.
    fn shell_idle(&self, machine: Option<&str>, place: &str) -> Result<bool, RuntimeError> {
        let (shell, front) = self.runtime.shell_processes(machine, shell::GROUP, place)?;
        if front.iter().any(|p| *p != shell) {
            return Ok(false);
        }
        let runner = self.runner(machine).map_err(|e| RuntimeError::Failed(e.1))?;
        let busy = crate::server::machines::metrics::has_children(&runner, shell).map_err(|e| RuntimeError::Failed(format!("cannot list the processes: {e}")))?;
        Ok(!busy)
    }
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
