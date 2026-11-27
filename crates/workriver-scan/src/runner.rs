// Where git runs (here, or over ssh) and the checked git calls every read goes through.
use std::path::Path;
use std::process::Command;

use crate::*;

/// Where git runs: on this machine, or on another one over ssh (the same commands).
#[derive(Clone, PartialEq, Debug, Default)]
pub enum Runner {
    #[default]
    Local,
    Ssh {
        /// `ssh`, or a stand-in in tests.
        program: String,
        /// Options before the target.
        options: Vec<String>,
        /// Anything ssh takes as a destination: a Host of the user's config, `user@host`.
        target: String,
    },
}

/// One argument as a POSIX shell word: ssh hands the remote shell one string.
pub(crate) fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

impl Runner {
    /// git over ssh to `target`, never asking for anything (BatchMode), sharing one
    /// connection through a control socket in `control` (a folder only the user can read).
    /// Whatever the user's ssh config says, it forwards no agent, takes no unknown host key
    /// and sets up none of the config's port forwards.
    pub fn ssh(target: &str, control: &Path) -> Runner {
        let opts = ["BatchMode=yes", "ForwardAgent=no", "StrictHostKeyChecking=yes", "ClearAllForwardings=yes", "ConnectTimeout=5", "ServerAliveInterval=10", "ServerAliveCountMax=3", "ControlMaster=auto", "ControlPersist=120"];
        let mut options: Vec<String> = opts.iter().flat_map(|o| ["-o".to_string(), o.to_string()]).collect();
        options.extend(["-o".to_string(), format!("ControlPath={}", control.join("ssh-%C").display())]);
        Runner::Ssh { program: "ssh".into(), options, target: target.into() }
    }

    /// The process that runs `words` (a program and its arguments) there.
    pub fn command(&self, words: &[&str]) -> Command {
        match self {
            Runner::Local => {
                let mut c = Command::new(words[0]);
                c.args(&words[1..]);
                c
            }
            Runner::Ssh { program, options, target } => {
                let mut c = Command::new(program);
                c.args(options).arg("--").arg(target).arg(words.iter().map(|w| quote(w)).collect::<Vec<_>>().join(" "));
                c.stdin(std::process::Stdio::null());
                c
            }
        }
    }

    /// A file's bytes there; None if it cannot be read.
    pub(crate) fn read(&self, path: &Path) -> Option<Vec<u8>> {
        match self {
            Runner::Local => std::fs::read(path).ok(),
            Runner::Ssh { .. } => self.command(&["cat", "--", &path.to_string_lossy()]).output().ok().filter(|o| o.status.success()).map(|o| o.stdout),
        }
    }

    /// A work-tree file as one stat sees it (a symlink's own, not what it points to); None
    /// over ssh (each file would cost a process: such a file is read every time) and for one
    /// that cannot be read.
    pub(crate) fn stat(&self, path: &Path) -> Option<FileStat> {
        use std::os::unix::fs::MetadataExt;
        match self {
            Runner::Local => {
                let m = std::fs::symlink_metadata(path).ok()?;
                let ns = |s: i64, n: i64| s as i128 * 1_000_000_000 + n as i128;
                Some(FileStat { len: m.len(), mtime: ns(m.mtime(), m.mtime_nsec()), ctime: ns(m.ctime(), m.ctime_nsec()), ino: m.ino(), link: m.is_symlink() })
            }
            Runner::Ssh { .. } => None,
        }
    }

    /// A file there that git does not track yet, as `git add` would take it: a symlink
    /// (true) with where it points, or the file's bytes. None if it cannot be read.
    pub(crate) fn read_new(&self, path: &Path) -> Option<(bool, Vec<u8>)> {
        match self {
            Runner::Local => match std::fs::symlink_metadata(path).ok()?.is_symlink() {
                true => self.read_link(path).map(|t| (true, t)),
                false => self.read(path).map(|b| (false, b)),
            },
            // One process, as reading the file alone is: its first byte says which it is.
            Runner::Ssh { .. } => {
                let script = r#"if [ -L "$1" ]; then printf L && readlink -- "$1"; else printf F && cat -- "$1"; fi"#;
                let out = self.command(&["sh", "-c", script, "sh", &path.to_string_lossy()]).output().ok().filter(|o| o.status.success())?.stdout;
                match out.split_first()? {
                    (b'L', t) => Some((true, t.strip_suffix(b"\n").unwrap_or(t).to_vec())),
                    (_, b) => Some((false, b.to_vec())),
                }
            }
        }
    }

    /// Where a symlink there points; None if it is not one.
    pub(crate) fn read_link(&self, path: &Path) -> Option<Vec<u8>> {
        match self {
            Runner::Local => std::fs::read_link(path).ok().map(|t| t.into_os_string().into_encoded_bytes()),
            Runner::Ssh { .. } => {
                let mut out = self.command(&["readlink", "--", &path.to_string_lossy()]).output().ok().filter(|o| o.status.success())?.stdout;
                out.pop_if(|b| *b == b'\n');
                Some(out)
            }
        }
    }
}

/// A work-tree file as one stat sees it: what stands for its bytes until any of it changes.
/// The change time and the inode catch what size and modification time alone miss (a write
/// that kept the time, a file replaced by another): neither can be set back by hand.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FileStat {
    pub len: u64,
    /// Modification and change times, in ns since the epoch.
    pub mtime: i128,
    pub ctime: i128,
    pub ino: u64,
    pub link: bool,
}

/// How many git processes were run, here and over ssh: for the timing log.
pub static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Runs git in `dir`; exit codes in `ok` besides 0 count as success. Optional locks are off
/// so reading never rewrites the index, which the watcher would take for a change.
pub(crate) fn run(at: &Runner, dir: &Path, args: &[&str], ok: &[i32]) -> Result<Vec<u8>> {
    run_with(at, dir, args, ok, None)
}

/// The same, with `input` on git's stdin.
pub(crate) fn run_with(at: &Runner, dir: &Path, args: &[&str], ok: &[i32], input: Option<Vec<u8>>) -> Result<Vec<u8>> {
    let dir_s = dir.to_string_lossy();
    let mut words = vec!["git", "--no-optional-locks", "-C", &dir_s];
    words.extend_from_slice(args);
    let started = std::time::Instant::now();
    CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut cmd = at.command(&words);
    let out = match input {
        None => cmd.output()?,
        Some(bytes) => {
            let mut child = cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn()?;
            let mut stdin = child.stdin.take().expect("piped");
            let writer = std::thread::spawn(move || std::io::Write::write_all(&mut stdin, &bytes));
            let out = child.wait_with_output()?;
            let _ = writer.join();
            out
        }
    };
    let code = out.status.code().unwrap_or(-1);
    // What git was asked, without where (a path names the user's folders) or which files.
    let said: Vec<&str> = args.iter().map(|a| if a.contains('/') { "…" } else { *a }).take(8).collect();
    tracing::debug!(target: "perf", kind = "git", via = if matches!(at, Runner::Local) { "local" } else { "ssh" }, args = %said.join(" "), ms = started.elapsed().as_millis() as u64, code, bytes = out.stdout.len());
    if code != 0 && !ok.contains(&code) {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(Error::Git(format!("git {} in {}: {}", args.join(" "), dir.display(), err.trim())));
    }
    Ok(out.stdout)
}

pub(crate) fn git(at: &Runner, dir: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8_lossy(&run(at, dir, args, &[])?).into_owned())
}
