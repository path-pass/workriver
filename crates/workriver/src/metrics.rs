//! CPU and memory of this machine and of every machine it follows, for the side panel, and
//! alerts when one stays past the thresholds the user set.
//!
//! Every machine is read the same way: one small POSIX `sh` probe, run through the runner
//! git already uses (here, or over the machine's ssh connection). Nothing is installed
//! anywhere, and "memory used" means the same on every machine: total − available. The
//! probe prints raw counters; the parsers here are pure. sysinfo was turned down although
//! it costs only about 63 KB: it cannot read the other machines, so a shell probe is needed
//! anyway, and two readers would mean two definitions.
//!
//! The samples are kept in memory, the last few minutes per machine, and folded into one
//! line a minute in `<data dir>/metrics/<label>.jsonl` for looking back (7 days); never in
//! the event store, since samples are not records of work. Alerts are derived as samples come, like the agents
//! waiting in their terminal, and never recorded either.
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use workriver_scan as scan;

use crate::herdr;
use crate::machines::Machines;

/// The name of the machine workriver runs on (a followed machine may not take it).
pub const HERE: &str = "local";

/// How often each machine is read.
pub const STEP: Duration = Duration::from_secs(5);

/// How many samples are kept per machine: ten minutes at `STEP`, the longest window.
const KEEP: usize = 120;

/// A threshold's window may be 1 to this many minutes (what the samples kept cover).
pub const MAX_MINUTES: u8 = 10;

/// An alert clears once every sample of this long is below its threshold.
const CLEAR_MS: i64 = 60_000;

/// The minute history kept on disk, per machine: this many days.
const HISTORY_MS: i64 = 7 * 24 * 3_600_000;

/// A machine's history file is rewritten without its lines older than `HISTORY_MS` at
/// most this often (and at the first minute after the service starts).
const TRIM_EVERY: Duration = Duration::from_secs(3600);

/// The longest look back into the history, in hours (what it keeps).
pub const MAX_HOURS: u32 = 168;

/// A probe that has not answered in this long is given up; the machine counts as unreachable.
const TIMEOUT: Duration = Duration::from_secs(15);

/// One sample of a machine, as raw counters. Linux has CPU counters, so the CPU share is the
/// difference between two samples; macOS has none a shell can read, so iostat measures one
/// second itself. `LC_ALL=C`: numbers with a dot whatever the user's locale. One line: over
/// ssh it reaches the login shell as one quoted word, and csh-family shells refuse a quoted
/// word with a newline in it (and expand `!` even in single quotes: none here). What only
/// some kernels have (PSI, `oom_kill`, the pressure sysctl) is read with its errors dropped,
/// and the probe ends with `true`: a missing piece is a field without a value, never a
/// failed reading. `df -Pk` reads the home folder's filesystem and `/`.
pub const PROBE: &str = concat!(
    "export LC_ALL=C; ",
    "if [ -r /proc/stat ]; then ",
    "echo 'os linux'; head -n 1 /proc/stat; grep -E '^(MemTotal|MemAvailable|SwapTotal|SwapFree):' /proc/meminfo; ",
    "grep '^oom_kill ' /proc/vmstat 2>/dev/null; cat /proc/pressure/memory 2>/dev/null; ",
    "else ",
    "echo 'os darwin'; echo \"memsize $(sysctl -n hw.memsize)\"; vm_stat; echo \"iostat $(iostat -n 0 -c 2 -w 1 | tail -n 1)\"; ",
    "echo \"pressure $(sysctl -n kern.memorystatus_vm_pressure_level 2>/dev/null)\"; echo \"swap $(sysctl -n vm.swapusage 2>/dev/null)\"; ",
    "fi; ",
    "df -Pk \"$HOME\" / 2>/dev/null; true",
);

// ---- reading what the probe printed ----

/// The CPU as the probe gives it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cpu {
    /// Measured over the probe's own second (macOS).
    Percent(f32),
    /// Counters since boot (Linux): busy and total ticks.
    Ticks { busy: u64, total: u64 },
}

/// One probe's output, read.
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    pub cpu: Cpu,
    pub mem_used: u64,
    pub mem_total: u64,
    pub detail: Detail,
}

/// How hard macOS finds it to hand out memory, as its kernel says (what Activity Monitor
/// colours its memory pressure graph by).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub enum Pressure {
    Normal,
    Warning,
    Critical,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Os {
    Linux,
    Darwin,
}

/// One filesystem's room, in bytes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    pub mount: String,
    pub free: u64,
    pub total: u64,
}

/// What a machine says beyond CPU and memory used; none where its system does not say it,
/// or the probe could not read it there.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    /// Which system answered: says which of the fields below it can have.
    pub os: Option<Os>,
    /// macOS: memory the compressor occupies (its pages, as Activity Monitor's "Compressed").
    pub mem_compressed: Option<u64>,
    /// macOS: the kernel's memory pressure level.
    pub pressure: Option<Pressure>,
    /// Linux PSI (`/proc/pressure/memory`), percent of the last 10 s in which some tasks,
    /// or all of them, waited for memory. None on a kernel or container without PSI.
    pub pressure_some: Option<f32>,
    pub pressure_full: Option<f32>,
    /// Linux: processes the OOM killer killed since boot.
    pub oom_kills: Option<u64>,
    pub swap_used: Option<u64>,
    pub swap_total: Option<u64>,
    /// The filesystem of the home folder, then `/` when it is another one.
    pub disks: Vec<Disk>,
}

/// Reads the probe's output, from Linux or macOS.
pub fn parse(text: &str) -> Result<Reading, String> {
    let os = text.lines().next().and_then(|l| l.strip_prefix("os ")).unwrap_or("").trim();
    match os {
        "linux" => parse_linux(text),
        "darwin" => parse_darwin(text),
        other => Err(format!("cannot read this machine's CPU and memory (system {other:?})")),
    }
}

fn number(s: &str) -> Option<u64> {
    s.trim().trim_end_matches('.').parse().ok()
}

fn parse_linux(text: &str) -> Result<Reading, String> {
    let line = text.lines().find(|l| l.starts_with("cpu ")).ok_or("no cpu line in /proc/stat")?;
    let ticks: Vec<u64> = line.split_whitespace().skip(1).filter_map(number).collect();
    if ticks.len() < 4 {
        return Err(format!("cannot read /proc/stat: {line:?}"));
    }
    // user nice system idle iowait irq softirq steal; guest time is in user already.
    let total: u64 = ticks.iter().take(8).sum();
    let idle = ticks[3] + ticks.get(4).copied().unwrap_or(0);
    let kib = |key: &str| text.lines().find_map(|l| l.strip_prefix(key)).and_then(|v| number(v.trim_end_matches("kB"))).map(|k| k * 1024);
    let total_mem = kib("MemTotal:").ok_or("no MemTotal in /proc/meminfo")?;
    // Memory used is MemTotal − MemAvailable: the kernel's own estimate of what can be
    // handed out without swapping, so available is shown as total − used.
    let available = kib("MemAvailable:").ok_or("no MemAvailable in /proc/meminfo")?;
    let (swap_total, swap_free) = (kib("SwapTotal:"), kib("SwapFree:"));
    // "some avg10=0.59 avg60=0.12 avg300=0.02 total=123928", and the same for "full".
    let psi = |kind: &str| text.lines().find_map(|l| l.strip_prefix(kind)?.split_whitespace().find_map(|w| w.strip_prefix("avg10="))?.parse().ok());
    let detail = Detail {
        os: Some(Os::Linux),
        pressure_some: psi("some "),
        pressure_full: psi("full "),
        oom_kills: text.lines().find_map(|l| l.strip_prefix("oom_kill ")).and_then(number),
        swap_used: swap_total.zip(swap_free).map(|(t, f)| t.saturating_sub(f)),
        swap_total,
        disks: disks(text),
        ..Detail::default()
    };
    Ok(Reading { cpu: Cpu::Ticks { busy: total - idle, total }, mem_used: total_mem.saturating_sub(available), mem_total: total_mem, detail })
}

fn parse_darwin(text: &str) -> Result<Reading, String> {
    let total = text.lines().find_map(|l| l.strip_prefix("memsize ")).and_then(number).ok_or("no hw.memsize")?;
    let page = text
        .lines()
        .find_map(|l| l.split_once("page size of ").and_then(|(_, r)| number(r.split_whitespace().next()?)))
        .ok_or("no page size in vm_stat")?;
    let some_pages = |key: &str| text.lines().find_map(|l| l.strip_prefix(key)).and_then(number);
    let pages = |key: &str| some_pages(key).unwrap_or(0);
    // What could be handed out without swapping: as Activity Monitor and sysinfo count it.
    let available = (pages("Pages free:") + pages("Pages inactive:") + pages("Pages speculative:") + pages("Pages purgeable:")) * page;
    // iostat: us sy id, then the three load averages; the idle share is fourth from the end.
    let fields: Vec<&str> = text.lines().find_map(|l| l.strip_prefix("iostat ")).ok_or("no iostat line")?.split_whitespace().collect();
    let idle: f32 = fields.len().checked_sub(4).and_then(|k| fields[k].parse().ok()).ok_or("cannot read iostat's idle share")?;
    // kern.memorystatus_vm_pressure_level: 1, 2 or 4, as the dispatch source's levels.
    let pressure = match text.lines().find_map(|l| l.strip_prefix("pressure ")).and_then(number) {
        Some(1) => Some(Pressure::Normal),
        Some(2) => Some(Pressure::Warning),
        Some(4) => Some(Pressure::Critical),
        _ => None,
    };
    // vm.swapusage: "total = 4096.00M  used = 3659.88M  free = 436.12M  (encrypted)".
    let swap = text.lines().find_map(|l| l.strip_prefix("swap ")).unwrap_or("");
    let swap_of = |key: &str| swap.split_once(key).and_then(|(_, r)| size(r.split_whitespace().next()?));
    let detail = Detail {
        os: Some(Os::Darwin),
        mem_compressed: some_pages("Pages occupied by compressor:").map(|p| p * page),
        pressure,
        swap_used: swap_of("used = "),
        swap_total: swap_of("total = "),
        disks: disks(text),
        ..Detail::default()
    };
    Ok(Reading { cpu: Cpu::Percent((100.0 - idle).clamp(0.0, 100.0)), mem_used: total.saturating_sub(available), mem_total: total, detail })
}

/// "3659.88M" (as sysctl prints sizes) in bytes.
fn size(s: &str) -> Option<u64> {
    let shift = match s.chars().last()? {
        'K' => 10,
        'M' => 20,
        'G' => 30,
        'T' => 40,
        _ => return None,
    };
    let v: f64 = s[..s.len() - 1].parse().ok()?;
    Some((v * (1u64 << shift) as f64) as u64)
}

/// `df -Pk` lines after its header, the home folder's first. The fields are counted back from
/// the capacity ("82%"), since a device name may hold a space. A filesystem is listed once,
/// and so is an APFS volume next to another of the same container with the same room (`/`
/// beside `/System/Volumes/Data` on a Mac).
fn disks(text: &str) -> Vec<Disk> {
    let mut out: Vec<Disk> = vec![];
    for line in text.lines().skip_while(|l| !l.starts_with("Filesystem")).skip(1) {
        let w: Vec<&str> = line.split_whitespace().collect();
        let Some(c) = w.iter().position(|x| x.strip_suffix('%').is_some_and(|p| p.parse::<u8>().is_ok())).filter(|&c| c >= 4 && c + 1 < w.len()) else { continue };
        let (Some(total), Some(free)) = (number(w[c - 3]), number(w[c - 1])) else { continue };
        let d = Disk { mount: w[c + 1..].join(" "), free: free * 1024, total: total * 1024 };
        if !out.iter().any(|o| o.mount == d.mount || (o.total, o.free) == (d.total, d.free)) {
            out.push(d);
        }
    }
    out
}

/// The CPU share between two readings; none for the first reading of counters.
pub fn cpu_share(before: Option<&Reading>, now: &Reading) -> Option<f32> {
    match (before.map(|r| r.cpu), now.cpu) {
        (_, Cpu::Percent(p)) => Some(p),
        (Some(Cpu::Ticks { busy: b0, total: t0 }), Cpu::Ticks { busy, total }) if total > t0 => Some(((busy.saturating_sub(b0)) as f32 * 100.0 / (total - t0) as f32).clamp(0.0, 100.0)),
        _ => None,
    }
}

// ---- samples, thresholds, alerts ----

/// What is kept of one reading, and what the page draws.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    /// When, in milliseconds since 1970.
    pub at: i64,
    /// Percent of the whole machine; none until two readings of counters are there.
    pub cpu: Option<f32>,
    pub mem_used: u64,
    pub mem_total: u64,
    /// OOM kills since workriver began to read the machine (`oom_kills` counts since boot).
    pub oom_kills_watched: Option<u64>,
    #[serde(flatten)]
    pub detail: Detail,
}

impl Sample {
    fn value(&self, metric: Metric) -> Option<f32> {
        match metric {
            Metric::Cpu => self.cpu,
            Metric::Memory => (self.mem_total > 0).then(|| self.mem_used as f32 * 100.0 / self.mem_total as f32),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Metric {
    Cpu,
    Memory,
}

const METRICS: [Metric; 2] = [Metric::Cpu, Metric::Memory];

/// When one metric alerts: at or above `percent` for `minutes` without a break.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Limit {
    pub on: bool,
    pub percent: u8,
    pub minutes: u8,
}

/// The thresholds, the same for every machine.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Thresholds {
    pub cpu: Limit,
    pub memory: Limit,
}

impl Default for Thresholds {
    fn default() -> Self {
        let limit = Limit { on: true, percent: 90, minutes: 5 };
        Thresholds { cpu: limit, memory: limit }
    }
}

impl Thresholds {
    pub fn of(&self, metric: Metric) -> &Limit {
        match metric {
            Metric::Cpu => &self.cpu,
            Metric::Memory => &self.memory,
        }
    }

    /// A threshold is a percent of 1 to 100, held for 1 to `MAX_MINUTES` minutes.
    pub fn check(&self) -> Result<(), String> {
        for (name, l) in [("CPU", &self.cpu), ("memory", &self.memory)] {
            if !(1..=100).contains(&l.percent) {
                return Err(format!("the {name} threshold is a percent from 1 to 100, not {}", l.percent));
            }
            if !(1..=MAX_MINUTES).contains(&l.minutes) {
                return Err(format!("the {name} threshold holds for 1 to {MAX_MINUTES} minutes, not {}", l.minutes));
            }
        }
        Ok(())
    }
}

/// Whether a metric is in alert after the newest sample, and since when (ms): raised once
/// every sample of the last `minutes` is at or above the threshold, cleared once every
/// sample of a minute is below. A gap in the samples (the machine was away) breaks a run,
/// so the window starts again. Switched off, it never alerts; while the machine cannot be
/// reached, or the metric has no value yet, it stays as it was.
pub fn evaluate(samples: &[Sample], metric: Metric, limit: &Limit, raised: Option<i64>, reachable: bool, step_ms: i64) -> Option<i64> {
    if !limit.on {
        return None;
    }
    let Some(last) = samples.last().filter(|s| reachable && s.value(metric).is_some()) else { return raised };
    let over = |s: &Sample| s.value(metric).map(|v| v >= limit.percent as f32);
    // Where the run of samples like the newest one starts: the samples on either side of a
    // gap of more than three steps do not belong to one run.
    let start_of_run = |want: bool| {
        let mut start = last.at;
        for pair in samples.windows(2).rev() {
            if pair[1].at - pair[0].at > 3 * step_ms || over(&pair[0]) != Some(want) {
                break;
            }
            start = pair[0].at;
        }
        start
    };
    // A run of n samples covers n steps: the last one stands for the step after it.
    let covers = |want: bool, ms: i64| over(last) == Some(want) && last.at - start_of_run(want) + step_ms >= ms;
    match raised {
        None => covers(true, limit.minutes as i64 * 60_000).then(|| start_of_run(true)),
        Some(_) if covers(false, CLEAR_MS) => None,
        Some(since) => Some(since),
    }
}

/// A machine in alert, as the workspace snapshot carries it for Needs you.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub machine: String,
    /// It is the machine workriver runs on.
    pub here: bool,
    pub metric: Metric,
    /// Since when (RFC 3339): the first sample of the run over the threshold.
    pub since: String,
    /// The newest value, in percent.
    pub value: f32,
    pub percent: u8,
    pub minutes: u8,
}

/// One machine as GET /api/metrics gives it.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MachineMetrics {
    pub label: String,
    pub here: bool,
    /// Its newest probe answered.
    pub reachable: bool,
    /// The connection is down: since when (RFC 3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub down_since: Option<String>,
    /// Why the newest probe gave nothing, when it ran but failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Oldest first.
    pub samples: Vec<Sample>,
    /// The metrics in alert: since when (RFC 3339).
    pub alerts: BTreeMap<String, String>,
}

/// Everything the page draws in the Machines section and Settings.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub thresholds: Thresholds,
    pub step_seconds: u64,
    pub machines: Vec<MachineMetrics>,
}

// ---- the sampler ----

#[derive(Default)]
struct Track {
    samples: VecDeque<Sample>,
    before: Option<Reading>,
    /// The newest OOM kill count since boot, and the kills counted since the first reading.
    oom: Option<(u64, u64)>,
    /// The samples of the minute under way, made one history line when the next begins.
    minute: Vec<Sample>,
    /// When the history file was last rid of its old lines.
    trimmed: Option<Instant>,
    reachable: bool,
    error: Option<String>,
    raised: BTreeMap<Metric, i64>,
}

/// Reads every machine on a thread of its own, keeps the samples, raises and clears alerts.
pub struct Metrics {
    home: PathBuf,
    /// Where the probe runs for this machine: here (a stand-in in tests).
    local: scan::Runner,
    machines: Option<Arc<Machines>>,
    tracks: Mutex<BTreeMap<String, Track>>,
    running: Mutex<HashSet<String>>,
    thresholds: Mutex<Thresholds>,
    /// The page hears when an alert is raised or cleared, and only then.
    changed: broadcast::Sender<()>,
    step: Duration,
    stop: AtomicBool,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn rfc3339(ms: i64) -> String {
    humantime::format_rfc3339_seconds(UNIX_EPOCH + Duration::from_millis(ms.max(0) as u64)).to_string()
}

// ---- the minute history on disk ----

/// Adds `s` to the minute under way; when it starts another minute, gives the one before as
/// its history line.
fn next_minute(minute: &mut Vec<Sample>, s: &Sample) -> Option<Sample> {
    let done = minute.first().filter(|f| f.at.div_euclid(60_000) != s.at.div_euclid(60_000)).map(|_| minute_of(minute));
    if done.is_some() {
        minute.clear();
    }
    minute.push(s.clone());
    done
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, n) = values.fold((0.0, 0u32), |(s, n), v| (s + v, n + 1));
    (n > 0).then(|| sum / n as f64)
}

/// One minute of samples as one line: the mean of each reading; the worst pressure level;
/// counters and the disks as they were at the minute's end. `at` is the minute's start.
fn minute_of(samples: &[Sample]) -> Sample {
    let last = samples.last().expect("a minute has a sample");
    // Shares to two decimals: a week of lines is smaller, and nothing reads finer.
    let avg = |f: &dyn Fn(&Sample) -> Option<f64>| mean(samples.iter().filter_map(f)).map(|v| (v * 100.0).round() / 100.0);
    let bytes = |f: &dyn Fn(&Sample) -> Option<u64>| avg(&|s| f(s).map(|v| v as f64)).map(|v| v.round() as u64);
    Sample {
        at: last.at.div_euclid(60_000) * 60_000,
        cpu: avg(&|s| s.cpu.map(f64::from)).map(|v| v as f32),
        mem_used: bytes(&|s| Some(s.mem_used)).unwrap_or(0),
        mem_total: bytes(&|s| Some(s.mem_total)).unwrap_or(0),
        oom_kills_watched: last.oom_kills_watched,
        detail: Detail {
            os: last.detail.os,
            mem_compressed: bytes(&|s| s.detail.mem_compressed),
            pressure: samples.iter().filter_map(|s| s.detail.pressure).reduce(|a, b| if b > a { b } else { a }),
            pressure_some: avg(&|s| s.detail.pressure_some.map(f64::from)).map(|v| v as f32),
            pressure_full: avg(&|s| s.detail.pressure_full.map(f64::from)).map(|v| v as f32),
            oom_kills: last.detail.oom_kills,
            swap_used: bytes(&|s| s.detail.swap_used),
            swap_total: bytes(&|s| s.detail.swap_total),
            disks: last.detail.disks.clone(),
        },
    }
}

/// `<home>/metrics/<label>.jsonl`. Labels are plain already (letters, digits, `-_.`); any
/// other character, a `/` above all, becomes `_`.
fn history_path(home: &std::path::Path, label: &str) -> PathBuf {
    let name: String = label.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' }).collect();
    home.join("metrics").join(format!("{name}.jsonl"))
}

/// Appends a minute to a machine's history (0600, in a 0700 folder); with `drop_before`,
/// first rewrites the file without the lines older than that. One rewrite of a week of
/// minutes is about 3 MB read and written, hence at most once an hour.
fn keep_minute(home: &std::path::Path, label: &str, line: &Sample, drop_before: Option<i64>) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let path = history_path(home, label);
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(path.parent().unwrap_or(home))?;
    let open = |p: &std::path::Path, append: bool| std::fs::OpenOptions::new().create(true).write(true).append(append).truncate(!append).mode(0o600).open(p);
    if let Some(before) = drop_before
        && path.exists()
    {
        let kept: String = std::fs::read_to_string(&path)?.lines().filter(|l| parse_line(l).is_some_and(|s| s.at >= before)).map(|l| format!("{l}\n")).collect();
        let tmp = path.with_extension("jsonl.tmp");
        open(&tmp, false)?.write_all(kept.as_bytes())?;
        std::fs::rename(&tmp, &path)?;
    }
    let mut text = serde_json::to_string(line).map_err(std::io::Error::other)?;
    text.push('\n');
    // One write of a whole line: a reader at the same time sees it whole or not at all.
    open(&path, true)?.write_all(text.as_bytes())
}

fn parse_line(l: &str) -> Option<Sample> {
    serde_json::from_str(l).ok()
}

/// A machine's history lines from `since` (ms) on; none when it has none yet. A line that
/// cannot be read (cut off by a crash) is skipped.
fn read_history(home: &std::path::Path, label: &str, since: i64) -> std::io::Result<Vec<Sample>> {
    match std::fs::read_to_string(history_path(home, label)) {
        Ok(text) => Ok(text.lines().filter_map(parse_line).filter(|s| s.at >= since).collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e),
    }
}

impl Metrics {
    pub fn new(home: PathBuf, local: scan::Runner, machines: Option<Arc<Machines>>, changed: broadcast::Sender<()>, step: Duration) -> Arc<Metrics> {
        let thresholds = crate::machines::thresholds(&home);
        Arc::new(Metrics { home, local, machines, tracks: Mutex::default(), running: Mutex::default(), thresholds: Mutex::new(thresholds), changed, step, stop: AtomicBool::new(false) })
    }

    /// Starts reading: this machine at once, a followed machine from when it is followed.
    pub fn start(self: &Arc<Self>) {
        let me = self.clone();
        std::thread::spawn(move || {
            while !me.stop.load(Ordering::Relaxed) {
                for label in me.wanted() {
                    if me.running.lock().unwrap().insert(label.clone()) {
                        let m = me.clone();
                        std::thread::spawn(move || m.follow(&label));
                    }
                }
                std::thread::sleep(me.step);
            }
        });
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// This machine, and every machine followed now.
    fn wanted(&self) -> Vec<String> {
        std::iter::once(HERE.to_string()).chain(self.machines.iter().flat_map(|m| m.states()).map(|s| s.label)).collect()
    }

    /// One machine's loop, until it is no longer followed.
    fn follow(self: Arc<Self>, label: &str) {
        while !self.stop.load(Ordering::Relaxed) && self.wanted().iter().any(|l| l == label) {
            let started = Instant::now();
            self.sample(label);
            std::thread::sleep(self.step.saturating_sub(started.elapsed()));
        }
        self.tracks.lock().unwrap().remove(label);
        self.running.lock().unwrap().remove(label);
    }

    /// Where the probe runs for a machine: None while it cannot be reached.
    fn runner(&self, label: &str) -> Option<scan::Runner> {
        if label == HERE { Some(self.local.clone()) } else { self.machines.as_ref()?.runner(label) }
    }

    /// Reads a machine once and looks at its alerts.
    pub fn sample(&self, label: &str) {
        let read = self.runner(label).map(|r| {
            let started = Instant::now();
            let out = herdr::output_within(r.command(&["sh", "-c", PROBE]), TIMEOUT).map_err(|e| format!("cannot run the probe: {e}"));
            tracing::debug!(target: "perf", kind = "probe", machine = label, ms = started.elapsed().as_millis() as u64);
            out.and_then(|o| if o.status.success() { parse(&String::from_utf8_lossy(&o.stdout)) } else { Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("the probe failed").trim().to_string()) })
        });
        let thresholds = *self.thresholds.lock().unwrap();
        let mut tracks = self.tracks.lock().unwrap();
        let track = tracks.entry(label.to_string()).or_default();
        let mut done_minute = None;
        match read {
            Some(Ok(reading)) => {
                let cpu = cpu_share(track.before.as_ref(), &reading);
                let oom_kills_watched = reading.detail.oom_kills.map(|now| {
                    let (last, seen) = track.oom.unwrap_or((now, 0));
                    // Fewer than last time: the machine restarted and counts from 0 again.
                    let seen = seen + if now >= last { now - last } else { now };
                    track.oom = Some((now, seen));
                    seen
                });
                let sample = Sample { at: now_ms(), cpu, mem_used: reading.mem_used, mem_total: reading.mem_total, oom_kills_watched, detail: reading.detail };
                done_minute = next_minute(&mut track.minute, &sample).map(|line| {
                    let trim = track.trimmed.is_none_or(|t| t.elapsed() >= TRIM_EVERY);
                    if trim {
                        track.trimmed = Some(Instant::now());
                    }
                    (line, trim)
                });
                track.samples.push_back(sample);
                track.before = Some(Reading { detail: Detail::default(), ..reading });
                while track.samples.len() > KEEP {
                    track.samples.pop_front();
                }
                (track.reachable, track.error) = (true, None);
            }
            // The counters of before an outage would make the next CPU an average over the
            // whole outage: the first sample after it has no CPU, as the first one ever.
            Some(Err(why)) => (track.reachable, track.error, track.before) = (false, Some(why), None),
            None => (track.reachable, track.error, track.before) = (false, None, None),
        }
        if Self::judge(track, &thresholds, self.step) {
            let _ = self.changed.send(());
        }
        drop(tracks);
        // On disk outside the lock: the page's reads do not wait for it.
        if let Some((line, trim)) = done_minute
            && let Err(e) = keep_minute(&self.home, label, &line, trim.then(|| now_ms() - HISTORY_MS))
        {
            tracing::warn!(machine = label, "cannot keep the metrics history: {e}");
        }
    }

    /// A machine's minutes of the last `hours`, oldest first, read from its history file.
    pub fn history(&self, label: &str, hours: u32) -> Result<Vec<Sample>, String> {
        if !(1..=MAX_HOURS).contains(&hours) {
            return Err(format!("the history goes back 1 to {MAX_HOURS} hours, not {hours}"));
        }
        read_history(&self.home, label, now_ms() - hours as i64 * 3_600_000).map_err(|e| format!("cannot read the metrics history: {e}"))
    }

    /// Raises and clears a machine's alerts; says whether any changed.
    fn judge(track: &mut Track, thresholds: &Thresholds, step: Duration) -> bool {
        let samples = track.samples.make_contiguous();
        let mut changed = false;
        for metric in METRICS {
            let was = track.raised.get(&metric).copied();
            let now = evaluate(samples, metric, thresholds.of(metric), was, track.reachable, step.as_millis() as i64);
            changed |= now.is_some() != was.is_some();
            match now {
                Some(since) => track.raised.insert(metric, since),
                None => track.raised.remove(&metric),
            };
        }
        changed
    }

    pub fn thresholds(&self) -> Thresholds {
        *self.thresholds.lock().unwrap()
    }

    /// Saves new thresholds and looks at every machine's alerts again at once.
    pub fn set_thresholds(&self, t: Thresholds) -> Result<(), String> {
        t.check()?;
        crate::machines::set_thresholds(&self.home, t)?;
        *self.thresholds.lock().unwrap() = t;
        let changed = self.tracks.lock().unwrap().values_mut().fold(false, |any, track| Self::judge(track, &t, self.step) | any);
        if changed {
            let _ = self.changed.send(());
        }
        Ok(())
    }

    /// Every machine with its samples, this machine first.
    pub fn report(&self) -> Report {
        let tracks = self.tracks.lock().unwrap();
        let machines = self
            .wanted()
            .into_iter()
            .map(|label| {
                let t = tracks.get(&label);
                MachineMetrics {
                    here: label == HERE,
                    reachable: t.is_some_and(|t| t.reachable),
                    down_since: (label != HERE).then(|| self.machines.as_ref().and_then(|m| m.down_since(&label))).flatten(),
                    error: t.and_then(|t| t.error.clone()),
                    samples: t.map(|t| t.samples.iter().cloned().collect()).unwrap_or_default(),
                    alerts: t.map(|t| t.raised.iter().map(|(m, since)| (serde_json::to_value(m).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default(), rfc3339(*since))).collect()).unwrap_or_default(),
                    label,
                }
            })
            .collect();
        Report { thresholds: self.thresholds(), step_seconds: self.step.as_secs().max(1), machines }
    }

    /// The machines in alert now, for Needs you: only those still followed. A machine no
    /// longer followed keeps its track until its loop next wakes, up to a step later, and the
    /// page refetches at once when it is dropped.
    pub fn alerts(&self) -> Vec<Alert> {
        let thresholds = self.thresholds();
        let wanted = self.wanted();
        let tracks = self.tracks.lock().unwrap();
        let mut out = vec![];
        for (label, t) in tracks.iter().filter(|(label, _)| wanted.contains(label)) {
            for (metric, since) in &t.raised {
                let limit = thresholds.of(*metric);
                let value = t.samples.back().and_then(|s| s.value(*metric)).unwrap_or_default();
                out.push(Alert { machine: label.clone(), here: label == HERE, metric: *metric, since: rfc3339(*since), value, percent: limit.percent, minutes: limit.minutes });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the probe printed on a Mac (2026-11-17, macOS 26), shortened to the lines it reads
    /// and a few it skips.
    const DARWIN: &str = "os darwin
memsize 34359738368
Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                                     4127.
Pages active:                                 491449.
Pages inactive:                               490767.
Pages speculative:                               104.
Pages throttled:                                   0.
Pages wired down:                             203743.
Pages purgeable:                                   4.
\"Translation faults\":                     1591096854.
Pages stored in compressor:                  2093088.
Pages occupied by compressor:                 867634.
Swapouts:                                     241400.
iostat  11 15 74  3.87 3.66 3.65
pressure 2
swap total = 4096.00M  used = 3659.88M  free = 436.12M  (encrypted)
Filesystem     1024-blocks      Used Available Capacity  Mounted on
/dev/disk3s5     482797652 372783576  84464052    82%    /System/Volumes/Data
/dev/disk3s1s1   482797652  12349052  84464052    13%    /
";

    /// What the probe printed in `docker run --rm --memory=64m --memory-swap=1g
    /// debian:stable-slim` (kernel 6.10) while a process there ran out of memory, OOM kills
    /// earlier in the same VM.
    const LINUX_PSI: &str = "os linux
cpu  830 0 1086 60601 1137 0 105 0 0 0
MemTotal:        8025424 kB
MemAvailable:    6863792 kB
SwapTotal:       1048572 kB
SwapFree:         732392 kB
oom_kill 4
some avg10=0.59 avg60=0.12 avg300=0.02 total=123928
full avg10=0.59 avg60=0.12 avg300=0.02 total=122836
Filesystem     1024-blocks     Used Available Capacity Mounted on
overlay         1055761844 13925952 988132468       2% /
overlay         1055761844 13925952 988132468       2% /
";

    /// The same container with the probe pointed at paths that do not exist instead of
    /// /proc/vmstat and /proc/pressure/memory, as a kernel or container without them answers.
    const LINUX_NO_PSI: &str = "os linux
cpu  498 0 558 48139 768 0 58 0 0 0
MemTotal:        8025424 kB
MemAvailable:    7373248 kB
SwapTotal:       1048572 kB
SwapFree:        1048572 kB
Filesystem     1024-blocks     Used Available Capacity Mounted on
overlay         1055761844 13925964 988132456       2% /
overlay         1055761844 13925964 988132456       2% /
";

    /// Counters made up to check the arithmetic, as an old kernel prints them: no swap lines
    /// read, no PSI, no oom_kill, no df.
    const LINUX_1: &str = "os linux
cpu  1000 0 500 8000 500 0 0 0 0 0
MemTotal:       16303428 kB
MemAvailable:    4075857 kB
";

    const LINUX_2: &str = "os linux
cpu  1600 0 800 8500 600 0 0 0 0 0
MemTotal:       16303428 kB
MemAvailable:    4075857 kB
";

    #[test]
    fn reads_a_mac() {
        let r = parse(DARWIN).unwrap();
        assert_eq!(r.mem_total, 34_359_738_368);
        assert_eq!(r.mem_used, 34_359_738_368 - (4127 + 490767 + 104 + 4) * 16384);
        assert_eq!(r.cpu, Cpu::Percent(26.0));
        assert_eq!(cpu_share(None, &r), Some(26.0), "a Mac measures its own second");
        let d = &r.detail;
        assert_eq!(d.mem_compressed, Some(867_634 * 16384));
        assert_eq!(d.pressure, Some(Pressure::Warning));
        assert_eq!((d.swap_used, d.swap_total), (Some((3659.88 * 1_048_576.0) as u64), Some(4096 << 20)));
        assert_eq!((d.pressure_some, d.oom_kills), (None, None), "Linux only");
        // `/` is a volume of the same APFS container as the home folder's: listed once.
        assert_eq!(d.disks, [Disk { mount: "/System/Volumes/Data".into(), free: 84_464_052 * 1024, total: 482_797_652 * 1024 }]);
        // Without the pressure and swap sysctls (an older macOS, a sandbox): none, still read.
        let bare = DARWIN.replace("pressure 2", "pressure ").replace("swap total = 4096.00M  used = 3659.88M  free = 436.12M  (encrypted)", "swap ");
        let r = parse(&bare).unwrap();
        assert_eq!((r.detail.pressure, r.detail.swap_used, r.detail.swap_total), (None, None, None));
        assert_eq!(parse(&DARWIN.replace("pressure 2", "pressure 4")).unwrap().detail.pressure, Some(Pressure::Critical));
    }

    #[test]
    fn reads_linux_pressure_oom_kills_swap_and_disk() {
        let r = parse(LINUX_PSI).unwrap();
        assert_eq!(r.mem_used, (8_025_424 - 6_863_792) * 1024, "used is total − MemAvailable");
        let d = &r.detail;
        assert_eq!((d.pressure_some, d.pressure_full, d.oom_kills), (Some(0.59), Some(0.59), Some(4)));
        assert_eq!((d.swap_used, d.swap_total), (Some((1_048_572 - 732_392) * 1024), Some(1_048_572 * 1024)));
        assert_eq!(d.disks, [Disk { mount: "/".into(), free: 988_132_468 * 1024, total: 1_055_761_844 * 1024 }], "home is on /: once");
        assert_eq!((d.mem_compressed, d.pressure), (None, None), "macOS only");
        let d = parse(LINUX_NO_PSI).unwrap().detail;
        assert_eq!((d.pressure_some, d.pressure_full, d.oom_kills), (None, None, None), "unknown, not an error");
        assert_eq!((d.swap_used, d.disks.len()), (Some(0), 1));
        let d = parse(LINUX_1).unwrap().detail;
        assert_eq!(d, Detail { os: Some(Os::Linux), ..Detail::default() }, "nothing beyond CPU and memory: still a reading");
    }

    #[test]
    fn df_lines_with_spaces_and_another_filesystem_for_home() {
        let text = "Filesystem 1024-blocks Used Available Capacity Mounted on
map auto_home         0        0         0   100%    /System/Volumes/Data/home
/dev/sda1    100 60 40 60% /
not a df line
";
        let d = disks(text);
        assert_eq!(d.iter().map(|d| (d.mount.as_str(), d.free)).collect::<Vec<_>>(), [("/System/Volumes/Data/home", 0), ("/", 40 * 1024)]);
    }

    #[test]
    fn reads_linux_counters_between_two_samples() {
        let first = parse(LINUX_1).unwrap();
        assert_eq!(first.mem_total, 16_303_428 * 1024);
        assert_eq!(first.mem_used, (16_303_428 - 4_075_857) * 1024);
        assert_eq!(cpu_share(None, &first), None, "the first sample of counters has no CPU yet");
        let second = parse(LINUX_2).unwrap();
        // busy went from 1500 to 2400 (+900), total from 10000 to 11500 (+1500): 60 %.
        assert_eq!(cpu_share(Some(&first), &second), Some(60.0));
    }

    #[test]
    fn the_probe_reads_the_machine_the_tests_run_on() {
        let out = std::process::Command::new("sh").args(["-c", PROBE]).output().unwrap();
        let r = parse(&String::from_utf8_lossy(&out.stdout)).unwrap();
        assert!(r.mem_total > 0 && r.mem_used <= r.mem_total, "{r:?}");
        assert!(r.detail.disks.first().is_some_and(|d| d.total > 0), "the home folder's filesystem: {r:?}");
        if cfg!(target_os = "macos") {
            assert!(r.detail.pressure.is_some() && r.detail.swap_total.is_some() && r.detail.mem_compressed.is_some(), "{r:?}");
        }
    }

    #[test]
    fn the_probe_is_one_line_a_csh_login_shell_takes() {
        assert!(!PROBE.contains('\n'), "a csh-family shell refuses a quoted word with a newline");
        // Over ssh the command reaches the login shell as one string of quoted words: a
        // stand-in hands its last argument to tcsh, as such a login shell would run it.
        if !std::path::Path::new("/bin/tcsh").exists() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("ssh");
        std::fs::write(&program, "#!/bin/sh\nfor a; do last=$a; done\nexec /bin/tcsh -c \"$last\"\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let runner = scan::Runner::Ssh { program: program.to_string_lossy().into(), options: vec![], target: "csh-box".into() };
        let out = runner.command(&["sh", "-c", PROBE]).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let r = parse(&String::from_utf8_lossy(&out.stdout)).unwrap();
        assert!(r.mem_total > 0, "{r:?}");
    }

    #[test]
    fn says_what_it_cannot_read() {
        assert!(parse("os plan9\n").unwrap_err().contains("plan9"));
        assert!(parse("os linux\nMemTotal: 1 kB\n").is_err());
        assert!(parse("os darwin\nmemsize 1\n").is_err());
    }

    const STEP_MS: i64 = 5_000;
    const CPU: Limit = Limit { on: true, percent: 90, minutes: 1 };

    /// A sample of CPU and memory only.
    fn bare(at: i64, cpu: Option<f32>, mem_used: u64, mem_total: u64) -> Sample {
        Sample { at, cpu, mem_used, mem_total, oom_kills_watched: None, detail: Detail::default() }
    }

    /// Samples every 5 s, CPU as given, from t = 0.
    fn run(cpu: &[f32]) -> Vec<Sample> {
        cpu.iter().enumerate().map(|(k, &c)| bare(k as i64 * STEP_MS, Some(c), 1, 2)).collect()
    }

    #[test]
    fn raises_only_after_the_whole_window() {
        // 11 samples over 90 % cover 55 s of a 60 s window: not yet; the 12th makes a minute.
        assert_eq!(evaluate(&run(&[95.0; 11]), Metric::Cpu, &CPU, None, true, STEP_MS), None);
        assert_eq!(evaluate(&run(&[95.0; 12]), Metric::Cpu, &CPU, None, true, STEP_MS), Some(0));
        // The run starts after the last sample below.
        let mut s = vec![10.0, 10.0];
        s.extend([95.0; 12]);
        assert_eq!(evaluate(&run(&s), Metric::Cpu, &CPU, None, true, STEP_MS), Some(2 * STEP_MS));
    }

    #[test]
    fn a_spike_does_not_raise() {
        let mut s = vec![95.0; 6];
        s.push(50.0);
        s.extend([95.0; 6]);
        assert_eq!(evaluate(&run(&s), Metric::Cpu, &CPU, None, true, STEP_MS), None);
    }

    #[test]
    fn clears_after_a_minute_below() {
        let mut s = vec![95.0; 12];
        s.extend([50.0; 11]);
        assert_eq!(evaluate(&run(&s), Metric::Cpu, &CPU, Some(0), true, STEP_MS), Some(0), "55 s below is not yet a minute");
        s.push(50.0);
        assert_eq!(evaluate(&run(&s), Metric::Cpu, &CPU, Some(0), true, STEP_MS), None);
        // One sample back over the threshold keeps it raised.
        let mut s = vec![50.0; 11];
        s.push(95.0);
        assert_eq!(evaluate(&run(&s), Metric::Cpu, &CPU, Some(0), true, STEP_MS), Some(0));
    }

    #[test]
    fn frozen_while_unreachable_and_never_when_switched_off() {
        let hot = run(&[95.0; 12]);
        assert_eq!(evaluate(&hot, Metric::Cpu, &CPU, None, false, STEP_MS), None, "not raised while away");
        assert_eq!(evaluate(&run(&[10.0; 20]), Metric::Cpu, &CPU, Some(0), false, STEP_MS), Some(0), "nor cleared");
        let off = Limit { on: false, ..CPU };
        assert_eq!(evaluate(&hot, Metric::Cpu, &off, Some(0), true, STEP_MS), None);
    }

    #[test]
    fn a_gap_starts_the_window_again() {
        // Six samples, the machine away for a minute, six more: no run of a minute.
        let mut s = run(&[95.0; 6]);
        s.extend(run(&[95.0; 6]).into_iter().map(|x| Sample { at: x.at + 90_000, ..x }));
        assert_eq!(evaluate(&s, Metric::Cpu, &CPU, None, true, STEP_MS), None);
    }

    #[test]
    fn memory_is_used_over_total() {
        let full: Vec<Sample> = (0..12).map(|k| bare(k * STEP_MS, None, 95, 100)).collect();
        assert_eq!(evaluate(&full, Metric::Memory, &CPU, None, true, STEP_MS), Some(0));
        assert_eq!(evaluate(&full, Metric::Cpu, &CPU, None, true, STEP_MS), None, "no CPU value yet: nothing to judge");
    }

    #[test]
    fn thresholds_are_checked_and_kept_in_the_settings() {
        assert!(Thresholds::default().check().is_ok());
        let mut t = Thresholds::default();
        t.cpu.percent = 0;
        assert!(t.check().unwrap_err().contains("CPU"));
        t.cpu.percent = 80;
        t.memory.minutes = 11;
        assert!(t.check().unwrap_err().contains("memory"));
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(crate::machines::thresholds(dir.path()), Thresholds::default(), "the defaults until set");
        let set = Thresholds { cpu: Limit { on: false, percent: 80, minutes: 2 }, memory: Limit { on: true, percent: 95, minutes: 10 } };
        crate::machines::set_thresholds(dir.path(), set).unwrap();
        assert_eq!(crate::machines::thresholds(dir.path()), set);
    }

    /// A sample of a Mac at `at` with the given CPU, pressure and OOM count.
    fn mac_at(at: i64, cpu: f32, pressure: Pressure, oom: u64) -> Sample {
        let detail = Detail { os: Some(Os::Darwin), pressure: Some(pressure), oom_kills: Some(oom), swap_used: Some(100), disks: vec![Disk { mount: "/".into(), free: at as u64, total: 9 }], ..Detail::default() };
        Sample { at, cpu: Some(cpu), mem_used: 40, mem_total: 100, oom_kills_watched: Some(oom), detail }
    }

    #[test]
    fn a_minute_becomes_one_line_of_means() {
        let mut minute = vec![];
        let t0 = 1_800_000_000_000; // a whole minute
        assert_eq!(next_minute(&mut minute, &mac_at(t0 + 1_000, 10.0, Pressure::Normal, 1)), None);
        assert_eq!(next_minute(&mut minute, &mac_at(t0 + 30_000, 30.0, Pressure::Critical, 2)), None);
        assert_eq!(next_minute(&mut minute, &mac_at(t0 + 59_999, 50.0, Pressure::Warning, 3)), None);
        let line = next_minute(&mut minute, &mac_at(t0 + 60_000, 99.0, Pressure::Normal, 4)).expect("the next minute began");
        assert_eq!((line.at, line.cpu, line.mem_used), (t0, Some(30.0), 40));
        assert_eq!(line.detail.pressure, Some(Pressure::Critical), "the worst of the minute");
        assert_eq!((line.detail.oom_kills, line.oom_kills_watched, line.detail.swap_used), (Some(3), Some(3), Some(100)), "counters at the minute's end");
        assert_eq!(line.detail.disks[0].free, (t0 + 59_999) as u64, "the disks at the minute's end");
        assert_eq!(minute.len(), 1, "the new minute has its first sample");
    }

    #[test]
    fn the_history_is_kept_a_week_and_read_back_after_a_restart() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let now = now_ms();
        let old = mac_at(now - HISTORY_MS - 60_000, 5.0, Pressure::Normal, 0);
        let day_ago = mac_at(now - 24 * 3_600_000 + 60_000, 20.0, Pressure::Warning, 0);
        let recent = mac_at(now - 60_000, 40.0, Pressure::Normal, 1);
        keep_minute(dir.path(), "box", &old, None).unwrap();
        keep_minute(dir.path(), "box", &day_ago, None).unwrap();
        let path = dir.path().join("metrics/box.jsonl");
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(path.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 2);
        // The trim: the week-old line goes, the file is rewritten 0600, the new line follows.
        keep_minute(dir.path(), "box", &recent, Some(now - HISTORY_MS)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().map(|l| parse_line(l).unwrap().at).collect::<Vec<_>>(), [day_ago.at, recent.at], "{text}");
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        // A line cut off by a crash is skipped, not an error.
        std::fs::OpenOptions::new().append(true).open(&path).and_then(|mut f| std::io::Write::write_all(&mut f, b"{\"at\":1")).unwrap();
        // Another service on the same folder (a restart): the history is there to read; the
        // samples alerts judge start empty, as before.
        let (tx, _) = broadcast::channel(4);
        let m = Metrics::new(dir.path().to_path_buf(), printing(dir.path(), "here", DARWIN), None, tx, Duration::from_secs(5));
        assert_eq!(m.history("box", 168).unwrap(), [day_ago.clone(), recent.clone()]);
        assert_eq!(m.history("box", 1).unwrap(), [recent], "the last hour only");
        assert!(m.history("box", 0).is_err() && m.history("box", 169).unwrap_err().contains("168"));
        assert_eq!(m.history("never-seen", 24).unwrap(), []);
        assert!(m.report().machines[0].samples.is_empty());
        let summary = crate::history_text(&serde_json::json!({ "label": "box" }), &serde_json::to_value(m.history("box", 168).unwrap()).unwrap());
        assert_eq!(summary, "box  cpu mean 30% max 40%  memory mean 40% max 40%  (2 min)");
        // A label is a file name, never a path.
        assert_eq!(history_path(dir.path(), "../x"), dir.path().join("metrics/.._x.jsonl"));
    }

    /// An ssh stand-in that prints `text` whatever it is asked.
    fn printing(dir: &std::path::Path, name: &str, text: &str) -> scan::Runner {
        let data = dir.join(format!("{name}.txt"));
        std::fs::write(&data, text).unwrap();
        let program = dir.join(name);
        std::fs::write(&program, format!("#!/bin/sh\ncat '{}'\n", data.display())).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        scan::Runner::Ssh { program: program.to_string_lossy().into(), options: vec![], target: name.into() }
    }

    #[test]
    fn samples_this_machine_and_followed_ones_through_their_runner() {
        let dir = tempfile::tempdir().unwrap();
        let local: Arc<dyn workriver_core::runtime::Runtime> = Arc::new(crate::tests::Fake::default());
        let all = Machines::with(local.clone(), vec![("box", local.clone()), ("gone", local)]);
        all.set_runner("box", printing(dir.path(), "box", LINUX_1));
        all.drop_tunnel("gone");
        let (tx, mut rx) = broadcast::channel(4);
        let m = Metrics::new(dir.path().to_path_buf(), printing(dir.path(), "here", DARWIN), Some(all.clone()), tx, Duration::from_millis(10));
        for label in [HERE, "box", "gone"] {
            m.sample(label);
        }
        all.set_runner("box", printing(dir.path(), "box2", LINUX_2));
        m.sample("box");
        let r = m.report();
        let get = |l: &str| r.machines.iter().find(|x| x.label == l).unwrap().clone();
        assert_eq!(r.machines.iter().map(|x| x.label.as_str()).collect::<Vec<_>>(), [HERE, "box", "gone"]);
        let here = get(HERE);
        assert!(here.here && here.reachable && here.samples.len() == 1 && here.samples[0].cpu == Some(26.0));
        let b = get("box");
        assert_eq!(b.samples.iter().map(|s| s.cpu).collect::<Vec<_>>(), [None, Some(60.0)]);
        assert_eq!(b.samples[0].oom_kills_watched, None, "no oom_kill line, no count");
        // OOM kills: counted from the first reading, through a restart of the machine.
        for (k, kills) in [4, 6, 1].into_iter().enumerate() {
            all.set_runner("box", printing(dir.path(), &format!("oom{k}"), &LINUX_PSI.replace("oom_kill 4", &format!("oom_kill {kills}"))));
            m.sample("box");
        }
        let oom: Vec<_> = m.report().machines[1].samples.iter().rev().take(3).map(|s| (s.detail.oom_kills, s.oom_kills_watched)).collect();
        assert_eq!(oom, [(Some(1), Some(3)), (Some(6), Some(2)), (Some(4), Some(0))]);
        let text = crate::load_text(&serde_json::to_value(m.report()).unwrap());
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[1], "  available 7.6 GB  pressure warning  compressed 13.2 GB  swap 3.6/4.0 GB  disk 80.6 GB free of 460.4 GB", "{text}");
        assert_eq!(lines[3], "  available 6.5 GB  pressure some 0.59% full 0.59% (last 10 s)  oom kills 1 (3 while watched)  swap 0.3/1.0 GB  disk 942.4 GB free of 1006.9 GB", "{text}");
        let g = get("gone");
        assert!(!g.reachable && g.samples.is_empty() && g.down_since.is_some(), "{g:?}");
        // After a failed sample, the next one has no CPU: the counters of before the outage
        // would make it an average over the whole outage.
        all.set_runner("box", printing(dir.path(), "broken", "os linux\n"));
        m.sample("box");
        all.set_runner("box", printing(dir.path(), "box3", LINUX_2.replace("1600 0 800 8500", "2000 0 900 9000").as_str()));
        m.sample("box");
        assert_eq!(m.report().machines[1].samples.last().unwrap().cpu, None, "no CPU right after an outage");
        // A machine in alert that is no longer followed leaves Needs you at once, before its
        // loop next wakes and drops its track.
        m.tracks.lock().unwrap().get_mut("box").unwrap().raised.insert(Metric::Cpu, now_ms());
        assert_eq!(m.alerts().iter().map(|a| a.machine.as_str()).collect::<Vec<_>>(), ["box"]);
        all.stop("box");
        assert!(m.alerts().is_empty(), "the alert of a machine no longer followed");
        assert!(rx.try_recv().is_err(), "nothing raised, nothing said");
        // Thresholds low enough for this machine's memory (77 %): raised at once? Not before
        // the window is covered: one sample covers 5 s of a minute.
        m.set_thresholds(Thresholds { cpu: Limit { on: false, percent: 90, minutes: 1 }, memory: Limit { on: true, percent: 50, minutes: 1 } }).unwrap();
        assert!(m.alerts().is_empty());
        assert!(m.set_thresholds(Thresholds { cpu: Limit { on: true, percent: 0, minutes: 1 }, ..Thresholds::default() }).is_err());
    }

    #[test]
    fn says_when_an_alert_is_raised_and_cleared_only() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = broadcast::channel(4);
        let m = Metrics::new(dir.path().to_path_buf(), printing(dir.path(), "here", DARWIN), None, tx, Duration::from_secs(5));
        // A minute of samples at 77 % memory, as the sampler would have kept them.
        {
            let mut tracks = m.tracks.lock().unwrap();
            let t = tracks.entry(HERE.into()).or_default();
            t.reachable = true;
            let now = now_ms();
            t.samples.extend((0..12).map(|k| bare(now - (11 - k) * 5_000, Some(10.0), 77, 100)));
        }
        m.set_thresholds(Thresholds { cpu: Limit { on: true, percent: 90, minutes: 1 }, memory: Limit { on: true, percent: 70, minutes: 1 } }).unwrap();
        assert!(rx.try_recv().is_ok(), "raised: the page hears of it");
        let a = m.alerts();
        assert_eq!((a.len(), a[0].metric, a[0].percent, a[0].here), (1, Metric::Memory, 70, true));
        assert_eq!(m.report().machines[0].alerts.keys().collect::<Vec<_>>(), ["memory"]);
        // The same again changes nothing and says nothing.
        m.set_thresholds(Thresholds { cpu: Limit { on: true, percent: 90, minutes: 1 }, memory: Limit { on: true, percent: 70, minutes: 1 } }).unwrap();
        assert!(rx.try_recv().is_err());
        // Switched off: cleared, and said.
        m.set_thresholds(Thresholds { cpu: Limit { on: true, percent: 90, minutes: 1 }, memory: Limit { on: false, percent: 70, minutes: 1 } }).unwrap();
        assert!(rx.try_recv().is_ok() && m.alerts().is_empty());
    }
}
