//! The timing log, for the user to send back when something is slow: JSON lines in
//! `<home>/logs/perf.jsonl`, written by a tracing layer from events of target `perf`, and
//! `workriver perf` to sum them up. Never content (commit messages, file texts, task text,
//! findings, notes, prompts) nor secrets nor paths: route templates, unit ids, machine
//! names, counts and milliseconds.
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

mod report;

pub use report::{bundle, parse_since, summary};

/// One file grows to this, then the files move up one (`perf.1.jsonl`…) and the oldest goes.
// ponytail: five files of 5 MB; a size or count of the user's choosing when anyone asks.
const FILE_BYTES: u64 = 5 * 1024 * 1024;
const FILES: usize = 5;

/// Where the timing log is.
pub fn dir(home: &Path) -> PathBuf {
    home.join(crate::common::home::LOGS)
}

/// The log's files, oldest first.
pub(super) fn files(home: &Path) -> Vec<PathBuf> {
    (0..FILES).rev().map(|k| dir(home).join(crate::common::home::perf_log(k))).filter(|p| p.exists()).collect()
}

/// The writer the layer writes each line through.
pub struct Log {
    home: PathBuf,
    file: Mutex<Option<(File, u64)>>,
}

impl Log {
    pub fn new(home: &Path) -> Log {
        Log { home: home.to_path_buf(), file: Mutex::new(None) }
    }

    fn write_line(&self, buf: &[u8]) -> std::io::Result<()> {
        let mut open = self.file.lock().unwrap();
        let path = dir(&self.home).join(crate::common::home::perf_log(0));
        if open.as_ref().is_some_and(|(_, n)| *n + buf.len() as u64 > FILE_BYTES) {
            *open = None;
            for k in (1..FILES).rev() {
                let from = if k == 1 { path.clone() } else { dir(&self.home).join(crate::common::home::perf_log(k - 1)) };
                let _ = std::fs::rename(from, dir(&self.home).join(crate::common::home::perf_log(k)));
            }
        }
        if open.is_none() {
            std::fs::create_dir_all(dir(&self.home))?;
            let f = OpenOptions::new().create(true).append(true).open(&path)?;
            let n = f.metadata().map(|m| m.len()).unwrap_or(0);
            *open = Some((f, n));
        }
        let (f, n) = open.as_mut().expect("opened");
        f.write_all(buf)?;
        *n += buf.len() as u64;
        Ok(())
    }
}

/// One line as the layer formats it.
pub struct Line<'a>(&'a Log, Vec<u8>);

impl Write for Line<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.1.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for Line<'_> {
    fn drop(&mut self) {
        if !self.1.is_empty() {
            let _ = self.0.write_line(&self.1);
        }
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Log {
    type Writer = Line<'a>;
    fn make_writer(&'a self) -> Line<'a> {
        Line(self, Vec::new())
    }
}

/// The service's logging: what it says to its terminal (not the timing lines), and the
/// timing log (`detail`: each git and ssh call too).
pub fn init(home: &Path, detail: bool) {
    use tracing_subscriber::prelude::*;
    let terminal = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()).add_directive("perf=off".parse().expect("a directive"));
    let level = if detail { tracing::Level::DEBUG } else { tracing::Level::INFO };
    let timing = tracing_subscriber::filter::Targets::new().with_target("perf", level);
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(terminal))
        .with(tracing_subscriber::fmt::layer().json().flatten_event(true).with_target(false).with_current_span(false).with_span_list(false).with_writer(Log::new(home)).with_filter(timing))
        .init();
}

/// What kind of failure, in a word: never its text, which can name hosts and paths.
pub fn kind_of(error: &str) -> &'static str {
    let e = error.to_lowercase();
    match () {
        _ if e.contains("timed out") || e.contains("timeout") || e.contains("not responding") => "timeout",
        _ if e.contains("refused") => "refused",
        _ if e.contains("permission denied") || e.contains("host key") => "auth",
        _ if e.contains("forwarding") || e.contains("address already in use") => "forward",
        _ if e.contains("slept") => "sleep",
        _ => "other",
    }
}
