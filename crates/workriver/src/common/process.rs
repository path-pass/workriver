// Processes as the operating system describes them: one process, its chain of parents, the
// same read on another machine over ssh, and running a command with a time limit.
use std::time::Duration;

use workriver_scan as scan;

/// One process as its operating system describes it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Proc {
    pub pid: u32,
    pub parent: u32,
    /// Its controlling terminal, as the system names it; none: it has none.
    pub tty: Option<String>,
    /// When it started, in its machine's own measure: equal for one process, different for
    /// another that got the same id later. On this machine it grows with time (`now`).
    pub start: u64,
    pub name: String,
}

/// A process and its parents, itself first, up to the first process of the system.
pub type Chain = Vec<Proc>;

/// No chain is followed further than this (as `metrics::top_of`).
const STEPS: usize = 64;

/// The chain of `pid` on this machine, read in-process; empty: there is no such process.
pub fn chain(pid: u32) -> Chain {
    let mut out: Chain = vec![];
    let mut next = pid;
    while next != 0 && out.len() < STEPS && !out.iter().any(|p| p.pid == next) {
        let Some(p) = one(next) else { break };
        next = p.parent;
        out.push(p);
    }
    out
}

/// What `ps` itself reads (`sysctl kern.proc.pid`): it answers for every account's processes.
#[cfg(target_os = "macos")]
pub(crate) fn one(pid: u32) -> Option<Proc> {
    let k = libproc::libproc::proc_pid::kproc_info_raw(pid as i32).ok()?;
    // An id nobody has answers with an empty record.
    if k.kp_proc.p_pid as u32 != pid {
        return None;
    }
    let name: Vec<u8> = k.kp_proc.p_comm.iter().take_while(|c| **c != 0).map(|c| *c as u8).collect();
    Some(Proc {
        pid,
        parent: k.kp_eproc.e_ppid.max(0) as u32,
        tty: (k.kp_eproc.e_tdev != -1).then(|| k.kp_eproc.e_tdev.to_string()),
        // `p_un` holds the start time while the process runs.
        start: (k.kp_proc.p_un.tv_sec.max(0) as u64) * 1_000_000 + k.kp_proc.p_un.tv_usec.max(0) as u64,
        name: String::from_utf8_lossy(&name).into_owned(),
    })
}

#[cfg(target_os = "linux")]
pub(crate) fn one(pid: u32) -> Option<Proc> {
    let s = procfs::process::Process::new(pid as i32).ok()?.stat().ok()?;
    Some(Proc { pid, parent: s.ppid.max(0) as u32, tty: (s.tty_nr != 0).then(|| s.tty_nr.to_string()), start: s.starttime * 1_000_000 / procfs::ticks_per_second(), name: s.comm })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn one(_: u32) -> Option<Proc> {
    None
}

/// Now, in the measure of `Proc::start` on this machine.
#[cfg(target_os = "linux")]
pub fn now() -> u64 {
    use procfs::Current;
    procfs::Uptime::current().map(|u| u.uptime_duration().as_micros() as u64).unwrap_or(u64::MAX)
}

#[cfg(not(target_os = "linux"))]
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_micros() as u64).unwrap_or(u64::MAX)
}

/// The chain of process `$1` on a machine where nothing of ours runs but what the runner
/// starts: one line per step. `/proc/<pid>/stat` where there is `/proc` (a slim system has
/// no `ps`), else `ps`. `parse` reads both.
pub const SNIPPET: &str = concat!(
    "export LC_ALL=C; p=$1; i=0; ",
    "while [ \"$p\" -gt 0 ] 2>/dev/null && [ $i -lt 64 ]; do ",
    "if [ -r /proc/self/stat ]; then l=$(cat /proc/$p/stat 2>/dev/null) || break; [ -n \"$l\" ] || break; echo \"stat $l\"; set -- ${l##*) }; p=$2; ",
    "else l=$(ps -o pid= -o ppid= -o tty= -o lstart= -o comm= -p $p 2>/dev/null); [ -n \"$l\" ] || break; echo \"ps $l\"; set -- $l; p=$2; fi; ",
    "i=$((i+1)); done; true",
);

/// What `SNIPPET` printed, as a chain. A line that does not read ends it.
pub fn parse(text: &str) -> Chain {
    text.lines().map_while(|l| l.strip_prefix("stat ").map(stat).or_else(|| l.strip_prefix("ps ").map(ps)).flatten()).take(STEPS).collect()
}

/// A line of `/proc/<pid>/stat`: `pid (name) state parent … tty_nr … starttime …`; the name
/// may hold anything, so the fields are counted from its closing bracket.
fn stat(line: &str) -> Option<Proc> {
    let (head, rest) = line.rsplit_once(") ")?;
    let (pid, name) = head.split_once(" (")?;
    let f: Vec<&str> = rest.split_whitespace().collect();
    let tty = f.get(4)?.parse::<i64>().ok()?;
    Some(Proc { pid: pid.trim().parse().ok()?, parent: f.get(1)?.parse().ok()?, tty: (tty != 0).then(|| tty.to_string()), start: f.get(19)?.parse().ok()?, name: name.into() })
}

/// A line of `ps -o pid= -o ppid= -o tty= -o lstart= -o comm=`: `pid parent tty Thu Nov 19
/// 10:09:43 2026 /path/to/the program`. The start is whole seconds, kept as its digits.
fn ps(line: &str) -> Option<Proc> {
    let mut rest = line;
    let mut word = || {
        rest = rest.trim_start();
        let (w, after) = rest.split_at(rest.find(char::is_whitespace).unwrap_or(rest.len()));
        rest = after;
        (!w.is_empty()).then_some(w)
    };
    let (pid, parent, tty) = (word()?.parse().ok()?, word()?.parse().ok()?, word()?);
    let (_day, month, date, time, year) = (word()?, word()?, word()?, word()?, word()?);
    let month = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"].iter().position(|m| *m == month)? + 1;
    let start = format!("{year}{month:02}{:02}{}", date.parse::<u8>().ok()?, time.replace(':', "")).parse().ok()?;
    let name = rest.trim();
    let name = name.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or(name);
    Some(Proc { pid, parent, tty: (!tty.starts_with('?') && tty != "-").then(|| tty.to_string()), start, name: name.into() })
}

/// The chain of `pid` on the machine `runner` reaches.
pub fn chain_on(runner: &scan::Runner, pid: u32) -> Result<Chain, String> {
    let out = output_within(runner.command(&["sh", "-c", SNIPPET, "sh", &pid.to_string()]), Duration::from_secs(10)).map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("the process list was not read ({})", out.status));
    }
    Ok(parse(&String::from_utf8_lossy(&out.stdout)))
}

/// Reads chains: this system's, or what a test gives.
pub trait Procs: Send + Sync {
    /// The chain of `pid` on this machine (`at` none) or on the machine `at` reaches;
    /// empty: no such process. An error: it could not be read.
    fn chain(&self, at: Option<&scan::Runner>, pid: u32) -> Result<Chain, String>;
}

pub struct System;

impl Procs for System {
    fn chain(&self, at: Option<&scan::Runner>, pid: u32) -> Result<Chain, String> {
        match at {
            None => Ok(chain(pid)),
            Some(r) => chain_on(r, pid),
        }
    }
}

/// A chain as a refusal shows it: `sleep(4715) > zsh(90324) > login(89391)`, its first steps.
pub fn show(chain: &[Proc]) -> String {
    const SHOWN: usize = 4;
    if chain.is_empty() {
        return "(gone)".into();
    }
    let mut steps: Vec<String> = chain.iter().take(SHOWN).map(|p| format!("{}({})", p.name, p.pid)).collect();
    if chain.len() > SHOWN {
        steps.push("…".into());
    }
    steps.join(" > ")
}

/// Runs a command to its end, or kills it after `wait`.
pub fn output_within(mut cmd: std::process::Command, wait: Duration) -> std::io::Result<std::process::Output> {
    use std::process::Stdio;
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let until = std::time::Instant::now() + wait;
    // ponytail: output is read after exit, fine for answers that fit a pipe buffer only if
    // drained; so drain on threads.
    let (mut out, mut err) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let o = std::thread::spawn(move || { let mut b = vec![]; let _ = std::io::Read::read_to_end(&mut out, &mut b); b });
    let e = std::thread::spawn(move || { let mut b = vec![]; let _ = std::io::Read::read_to_end(&mut err, &mut b); b });
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if std::time::Instant::now() > until {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, format!("no answer within {}s", wait.as_secs())));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Ok(std::process::Output { status, stdout: o.join().unwrap_or_default(), stderr: e.join().unwrap_or_default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, parent: u32, tty: Option<&str>) -> Proc {
        Proc { pid, parent, tty: tty.map(str::to_string), start: 7, name: format!("n{pid}") }
    }

    /// What `SNIPPET` printed in `docker run --rm debian:stable-slim` for a `sleep` under
    /// two shells (no `ps` there), and on a Mac for a command run under the agent runtime: the name may
    /// hold spaces and brackets, the Mac's is a whole path.
    const STAT: &str = "stat 9 (sleep) S 8 1 1 34816 1 4194304 96 0 0 0 0 0 0 0 20 0 1 0 51234 2412544 211 18446744073709551615 1 1 0 0 0 0 0 0 0 0 0 0 17 5 0 0 0 0 0\nstat 8 (my (odd) sh) S 1 1 1 34816 1 4194560 317 2104 0 0 0 0 1 0 20 0 1 0 51201 2531328 233 18446744073709551615 1 1 0 0 0 0 0 0 0 0 0 0 17 2 0 0 0 0 0\nstat 1 (sh) S 0 1 1 0 1 4194560 317 2104 0 0 0 0 1 0 20 0 1 0 51200 2531328 233 18446744073709551615 1 1 0 0 0 0 0 0 0 0 0 0 17 2 0 0 0 0 0\n";

    const PS: &str = "ps 27157 27074 ??       Thu Nov 19 10:09:43 2026     /Users/x/target/debug/workriver\nps 27074 83970 ??       Thu Nov 19 10:09:42 2026     /bin/zsh\nps 83970 83912 ttys016  Thu Nov 19 09:40:43 2026     /Users/x/Library/Application Support/claude/2.1.328\nps 83912  6286 ttys016  Thu Nov 19 09:40:43 2026     -zsh\nps     1     0 ??       Fri Nov 13 18:21:17 2026     /sbin/launchd\n";

    #[test]
    fn the_snippets_two_formats_read_as_chains() {
        let c = parse(STAT);
        assert_eq!(c.len(), 3);
        assert_eq!((c[0].pid, c[0].parent, c[0].tty.as_deref(), c[0].start, c[0].name.as_str()), (9, 8, Some("34816"), 51234, "sleep"));
        assert_eq!((c[1].pid, c[1].parent, c[1].name.as_str()), (8, 1, "my (odd) sh"));
        assert_eq!((c[2].pid, c[2].parent, c[2].tty.as_deref()), (1, 0, None), "tty_nr 0: no terminal");
        let c = parse(PS);
        assert_eq!(c.len(), 5);
        assert_eq!((c[0].pid, c[0].parent, c[0].tty.as_deref(), c[0].name.as_str()), (27157, 27074, None, "workriver"));
        assert_eq!((c[2].tty.as_deref(), c[2].name.as_str()), (Some("ttys016"), "2.1.328"), "a path with a space");
        assert_eq!(c[3].name, "-zsh");
        // Two processes started in different seconds differ; the same line reads the same.
        assert_ne!(c[0].start, c[1].start);
        assert_eq!(c[2].start, c[3].start);
        assert_eq!(parse(PS)[0].start, c[0].start);
        // Nothing, or something else, is no chain; a broken line ends it.
        assert!(parse("").is_empty() && parse("ssh: connect refused\n").is_empty());
        assert_eq!(parse(&format!("{PS}ps junk\nps 1 0 ?? Fri Nov 13 18:21:17 2026 x\n")).len(), 5);
    }

    /// The snippet itself, run here on a real child: its chain is this system's own reading.
    #[test]
    fn the_snippet_reads_what_the_library_reads() {
        let mut child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
        let ours = chain(child.id());
        let theirs = chain_on(&scan::Runner::Local, child.id());
        let gone = chain_on(&scan::Runner::Local, 0x3fff_fff0);
        let _ = child.kill();
        let _ = child.wait();
        let theirs = theirs.unwrap();
        assert!(ours.len() >= 2 && ours[0].name == "sleep" && ours[1].pid == std::process::id(), "{ours:?}");
        assert!(ours[0].start <= now() && ours[0].start >= ours[1].start, "{ours:?}");
        let ids = |c: &Chain| c.iter().map(|p| (p.pid, p.parent, p.tty.is_some())).collect::<Vec<_>>();
        assert_eq!(ids(&ours), ids(&theirs));
        assert_eq!(theirs[0].name, "sleep");
        assert!(gone.unwrap().is_empty() && chain(0x3fff_fff0).is_empty(), "a process that is gone");
    }

    #[test]
    fn a_chain_shows_its_first_steps() {
        assert_eq!(show(&[p(3, 2, None), p(2, 1, None)]), "n3(3) > n2(2)");
        assert_eq!(show(&(0..9).map(|i| p(9 - i, 8 - i, None)).collect::<Vec<_>>()), "n9(9) > n8(8) > n7(7) > n6(6) > …");
        assert_eq!(show(&[]), "(gone)");
    }
}
