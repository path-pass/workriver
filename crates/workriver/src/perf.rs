//! The timing log, for the user to send back when something is slow: JSON lines in
//! `<home>/logs/perf.jsonl`, written by a tracing layer from events of target `perf`, and
//! `workriver perf` to sum them up. Never content (commit messages, file texts, task text,
//! findings, notes, prompts) nor secrets nor paths: route templates, unit ids, machine
//! names, counts and milliseconds.
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

/// One file grows to this, then the files move up one (`perf.1.jsonl`…) and the oldest goes.
// ponytail: five files of 5 MB; a size or count of the user's choosing when anyone asks.
const FILE_BYTES: u64 = 5 * 1024 * 1024;
const FILES: usize = 5;

/// Where the timing log is.
pub fn dir(home: &Path) -> PathBuf {
    home.join(crate::common::home::LOGS)
}

/// The log's files, oldest first.
fn files(home: &Path) -> Vec<PathBuf> {
    (0..FILES).rev().map(|k| dir(home).join(if k == 0 { "perf.jsonl".to_string() } else { format!("perf.{k}.jsonl") })).filter(|p| p.exists()).collect()
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
        let path = dir(&self.home).join(crate::common::home::PERF_LOG);
        if open.as_ref().is_some_and(|(_, n)| *n + buf.len() as u64 > FILE_BYTES) {
            *open = None;
            for k in (1..FILES).rev() {
                let from = if k == 1 { path.clone() } else { dir(&self.home).join(format!("perf.{}.jsonl", k - 1)) };
                let _ = std::fs::rename(from, dir(&self.home).join(format!("perf.{k}.jsonl")));
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

// ---- workriver perf ----

/// The log's lines since `since` (a duration like `1h`, `30m`, `2d`; none: all).
fn lines(home: &Path, since: Option<std::time::Duration>) -> Vec<Value> {
    let from = since.map(|d| std::time::SystemTime::now() - d);
    files(home)
        .into_iter()
        .flat_map(|f| std::fs::read_to_string(f).unwrap_or_default().lines().map(str::to_string).collect::<Vec<_>>())
        .filter_map(|l| serde_json::from_str::<Value>(&l).ok())
        .filter(|v| match (from, v["timestamp"].as_str().and_then(|t| humantime::parse_rfc3339_weak(t.trim_end_matches('Z').split('.').next().unwrap_or(t)).ok())) {
            (Some(from), Some(at)) => at >= from,
            _ => true,
        })
        .collect()
}

fn pct(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

/// `workriver perf`: where the time went.
pub fn summary(home: &Path, since: Option<std::time::Duration>, top: usize) -> String {
    let all = lines(home, since);
    let mut out = String::new();
    let span = match (all.first(), all.last()) {
        (Some(a), Some(b)) => format!("{} to {}", a["timestamp"].as_str().unwrap_or("?"), b["timestamp"].as_str().unwrap_or("?")),
        _ => "nothing logged".into(),
    };
    let _ = writeln!(out, "workriver timing log: {} lines, {span}", all.len());
    let of = |kind: &'static str| all.iter().filter(move |v| v["kind"] == kind);
    let ms = |v: &Value| v["ms"].as_u64().unwrap_or(0);
    // Requests by route.
    let mut routes: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for v in of("http") {
        routes.entry(format!("{} {}", v["method"].as_str().unwrap_or("?"), v["route"].as_str().unwrap_or("?"))).or_default().push(ms(v));
    }
    let mut rows: Vec<(String, Vec<u64>)> = routes.into_iter().map(|(r, mut t)| {
        t.sort();
        (r, t)
    }).collect();
    rows.sort_by_key(|(_, t)| std::cmp::Reverse(*t.last().unwrap_or(&0)));
    let _ = writeln!(out, "\nRequests (slowest first): count, p50, p95, max ms");
    for (r, t) in rows.iter().take(top) {
        let _ = writeln!(out, "  {r:<40} {:>6} {:>6} {:>6} {:>6}", t.len(), pct(t, 0.5), pct(t, 0.95), t.last().unwrap_or(&0));
    }
    // Snapshots.
    for (kind, title) in [("snapshot", "Snapshots (what a page or board reads)"), ("remote_read", "Background reads of other machines")] {
        let mut t: Vec<u64> = of(kind).map(ms).collect();
        t.sort();
        let sum = |f: &str| of(kind).map(|v| v[f].as_u64().unwrap_or(0)).sum::<u64>();
        let _ = writeln!(out, "\n{title}: {} times, p50 {} ms, p95 {} ms, max {} ms; units worked out again {}, reused {}; git calls {}", t.len(), pct(&t, 0.5), pct(&t, 0.95), t.last().unwrap_or(&0), sum("recomputed"), sum("reused"), sum("git"));
    }
    // Units.
    let mut units: BTreeMap<String, Vec<(u64, u64)>> = BTreeMap::new();
    for v in of("unit") {
        units.entry(format!("{} ({})", v["unit"].as_str().unwrap_or("?"), v["machine"].as_str().unwrap_or("?"))).or_default().push((ms(v), v["git"].as_u64().unwrap_or(0)));
    }
    let mut rows: Vec<(String, u64, u64, usize)> = units.into_iter().map(|(u, t)| (u, t.iter().map(|x| x.0).max().unwrap_or(0), t.iter().map(|x| x.1).sum(), t.len())).collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1));
    let _ = writeln!(out, "\nUnits read (slowest first): reads, max ms, git calls in all");
    for (u, max, git, n) in rows.iter().take(top) {
        let _ = writeln!(out, "  {u:<30} {n:>6} {max:>6} {git:>6}");
    }
    // Machines.
    let mut polls: BTreeMap<String, (usize, usize, u64)> = BTreeMap::new();
    for v in of("poll") {
        let e = polls.entry(v["machine"].as_str().unwrap_or("?").to_string()).or_default();
        e.0 += 1;
        e.1 += (v["ok"] != true) as usize;
        e.2 = e.2.max(ms(v));
    }
    if !polls.is_empty() {
        let _ = writeln!(out, "\nAgent reads per machine: reads, failed, max ms");
        for (m, (n, failed, max)) in &polls {
            let _ = writeln!(out, "  {m:<20} {n:>6} {failed:>6} {max:>6}");
        }
    }
    let mut down: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    for v in of("tunnel").filter(|v| v["event"] == "down") {
        let e = down.entry(v["machine"].as_str().unwrap_or("?").to_string()).or_default();
        e.0 += 1;
    }
    for v in of("tunnel").filter(|v| v["event"] == "up") {
        let e = down.entry(v["machine"].as_str().unwrap_or("?").to_string()).or_default();
        e.1 += v["down_ms"].as_u64().unwrap_or(0);
    }
    if !down.is_empty() {
        let _ = writeln!(out, "\nConnections to other machines: times down, time down in all");
        for (m, (n, ms)) in &down {
            let _ = writeln!(out, "  {m:<20} {n:>6} {:>9.1} s", *ms as f64 / 1000.0);
        }
    }
    // Calls (detail level only).
    let mut calls: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    for v in of("git").chain(of("ssh")) {
        let what = match v["kind"].as_str() {
            Some("git") => format!("git {} ({})", v["args"].as_str().unwrap_or("").split_whitespace().find(|a| !a.starts_with('-') && *a != "…").unwrap_or("?"), v["via"].as_str().unwrap_or("?")),
            _ => format!("ssh {}", v["purpose"].as_str().unwrap_or("?")),
        };
        let e = calls.entry(what).or_default();
        e.0 += 1;
        e.1 += ms(v);
    }
    if calls.is_empty() {
        let _ = writeln!(out, "\n(each git and ssh call is logged with `workriver serve --perf-detail`)");
    } else {
        let mut rows: Vec<_> = calls.into_iter().collect();
        rows.sort_by_key(|(_, (_, t))| std::cmp::Reverse(*t));
        let _ = writeln!(out, "\ngit and ssh calls: count, ms in all");
        for (w, (n, t)) in rows.iter().take(top) {
            let _ = writeln!(out, "  {w:<40} {n:>6} {t:>8}");
        }
    }
    let slow = all.iter().filter(|v| v["slow"] == true).count();
    let _ = writeln!(out, "\nSlow (over 500 ms): {slow}");
    out
}

/// `workriver perf --bundle`: the summary and the newest raw lines (at most `bytes`), in one
/// file that can be sent; its path.
pub fn bundle(home: &Path, bytes: usize) -> std::io::Result<PathBuf> {
    let mut raw: Vec<String> = files(home).into_iter().flat_map(|f| std::fs::read_to_string(f).unwrap_or_default().lines().map(str::to_string).collect::<Vec<_>>()).collect();
    let mut kept = 0;
    let mut start = raw.len();
    while start > 0 && kept + raw[start - 1].len() < bytes {
        start -= 1;
        kept += raw[start].len() + 1;
    }
    raw.drain(..start);
    let path = dir(home).join(format!("perf-bundle-{}.txt", humantime::format_rfc3339_seconds(std::time::SystemTime::now()).to_string().replace(':', "")));
    let mut text = format!(
        "workriver {} timing bundle\nTimes, counts, route templates, unit ids and machine names only: no commit messages, file texts, task texts, findings, notes, prompts, tokens, cookies or paths.\n\n",
        crate::BUILD
    );
    text += &summary(home, None, 30);
    text += "\n---- raw lines, newest last ----\n";
    text += &raw.join("\n");
    text.push('\n');
    std::fs::create_dir_all(dir(home))?;
    std::fs::write(&path, text)?;
    Ok(path)
}

/// `1h`, `30m`, `2d`, `45s`.
pub fn parse_since(s: &str) -> anyhow::Result<std::time::Duration> {
    humantime::parse_duration(s).map_err(|e| anyhow::anyhow!("--since {s}: {e} (for example 30m, 1h, 2d)"))
}
