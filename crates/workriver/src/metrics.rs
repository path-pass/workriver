//! CPU, memory and disks of this machine and of every machine it follows, for the side
//! panel, and alerts at two levels (warn, crit) when one passes the thresholds the user set.
//! An alert that rises, or rises a level, names the processes using the most memory then,
//! and the agent each belongs to.
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
use workriver_core::runtime::{Runtime, RuntimeAgent};
use workriver_scan as scan;

use crate::common::config::{DiskLimit, Levels, Limit, Thresholds};
use crate::runtime::follow::Machines;

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
/// some kernels have (PSI, `oom_kill`, the pressure sysctls) is read with its errors dropped,
/// and the probe ends with `true`: a missing piece is a field without a value, never a
/// failed reading. On macOS one `sysctl` reads every name as `name: value` (one it does not
/// know is skipped, the others still print). `df -Pk` reads the home folder's filesystem and `/`.
pub const PROBE: &str = concat!(
    "export LC_ALL=C; ",
    "if [ -r /proc/stat ]; then ",
    "echo 'os linux'; head -n 1 /proc/stat; grep -E '^(MemTotal|MemAvailable|SwapTotal|SwapFree):' /proc/meminfo; ",
    "grep '^oom_kill ' /proc/vmstat 2>/dev/null; cat /proc/pressure/memory 2>/dev/null; ",
    "else ",
    "echo 'os darwin'; sysctl hw.memsize kern.memorystatus_vm_pressure_level kern.memorystatus_level vm.swapusage ",
    "vm.compressor.segment.total vm.compressor.segment.limit vm.compressor_segment_pages_compressed vm.compressor_segment_pages_compressed_limit 2>/dev/null; ",
    "vm_stat; echo \"iostat $(iostat -n 0 -c 2 -w 1 | tail -n 1)\"; ",
    "fi; ",
    "df -Pk \"$HOME\" / 2>/dev/null; true",
);

/// Every process with its parent, resident memory (KiB) and name: `pid ppid rss name`. Run
/// only when an alert is raised or rises, not every sample. Linux reads `/proc` itself, since
/// a slim system may have no `ps` (procps); `cat` goes on past a process that ended meanwhile.
/// Kernel threads have no `VmRSS` and are left out.
pub const PS: &str = concat!(
    "export LC_ALL=C; ",
    "if [ -r /proc/self/status ]; then ",
    "cat /proc/[0-9]*/status 2>/dev/null | awk '/^Name:/{n=$0; sub(/^Name:[ \\t]*/, \"\", n)} /^Pid:/{p=$2} /^PPid:/{q=$2} /^VmRSS:/{print p, q, $2, n}'; ",
    "else ps -A -o pid= -o ppid= -o rss= -o comm=; fi; true",
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
    /// macOS: how full the compressor is, in percent of its limit: the higher of its
    /// segments over their limit (`vm.compressor.segment.total` / `.limit`, the kernel's
    /// `c_segment_count` / `c_segments_limit`) and its compressed pages over theirs. At 100 %
    /// the kernel is out of compressor space (`vm_compressor_out_of_space`).
    pub compressor: Option<f32>,
    /// macOS: the share of memory the kernel counts as free, in percent
    /// (`kern.memorystatus_level`: `memory_pressure`'s "System-wide memory free percentage").
    pub system_free: Option<f32>,
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
    // sysctl prints `name: value`.
    let sysctl = |name: &str| text.lines().find_map(|l| l.strip_prefix(name)?.strip_prefix(": "));
    let total = sysctl("hw.memsize").and_then(number).ok_or("no hw.memsize")?;
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
    let pressure = match sysctl("kern.memorystatus_vm_pressure_level").and_then(number) {
        Some(1) => Some(Pressure::Normal),
        Some(2) => Some(Pressure::Warning),
        Some(4) => Some(Pressure::Critical),
        _ => None,
    };
    // vm.swapusage: "total = 4096.00M  used = 3659.88M  free = 436.12M  (encrypted)".
    let swap = sysctl("vm.swapusage").unwrap_or("");
    let swap_of = |key: &str| swap.split_once(key).and_then(|(_, r)| size(r.split_whitespace().next()?));
    let share = |used: &str, limit: &str| Some(sysctl(used).and_then(number)? as f32 * 100.0 / sysctl(limit).and_then(number).filter(|l| *l > 0)? as f32);
    let compressor = share("vm.compressor.segment.total", "vm.compressor.segment.limit").into_iter().chain(share("vm.compressor_segment_pages_compressed", "vm.compressor_segment_pages_compressed_limit")).reduce(f32::max);
    let detail = Detail {
        os: Some(Os::Darwin),
        mem_compressed: some_pages("Pages occupied by compressor:").map(|p| p * page),
        compressor,
        system_free: sysctl("kern.memorystatus_level").and_then(number).map(|v| v as f32),
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

/// One GB as the page counts it (steps of 1024).
const GIB: f32 = 1_073_741_824.0;

impl Disk {
    /// The share of it in use, in percent.
    fn used(&self) -> f32 {
        (self.total - self.free.min(self.total)) as f32 * 100.0 / self.total as f32
    }

    fn level(&self, d: &DiskLimit) -> Option<Level> {
        let (used, free) = (self.used(), self.free as f32 / GIB);
        if used > d.crit || free < d.crit_free {
            Some(Level::Crit)
        } else if used > d.warn || free < d.warn_free {
            Some(Level::Warn)
        } else {
            None
        }
    }
}

fn above(v: f32, l: &Levels) -> Option<Level> {
    if v > l.crit { Some(Level::Crit) } else if v > l.warn { Some(Level::Warn) } else { None }
}

fn below(v: f32, l: &Levels) -> Option<Level> {
    if v < l.crit { Some(Level::Crit) } else if v < l.warn { Some(Level::Warn) } else { None }
}

impl Sample {
    /// The filesystem nearest its limits: the highest level, then the fullest. One of no
    /// size (an automount map) is no filesystem to judge.
    fn worst_disk(&self, d: &DiskLimit) -> Option<&Disk> {
        self.detail.disks.iter().filter(|x| x.total > 0).max_by(|a, b| (a.level(d), a.used()).partial_cmp(&(b.level(d), b.used())).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// A metric's newest value in its own unit (percent; GB for swap; kills seen for OOM; the
    /// used share of the fullest filesystem for disk; 0 for pressure, which its level says)
    /// and the level this sample alone is at; none where the machine has no such value. OOM
    /// kills are judged across samples (`evaluate_oom`), so here they have no level.
    pub fn reading(&self, metric: Metric, t: &Thresholds) -> Option<(f32, Option<Level>)> {
        let d = &self.detail;
        let at_least = |v: f32, l: &Limit| (v, (v >= l.percent as f32).then_some(Level::Warn));
        let mem_share = (self.mem_total > 0).then(|| self.mem_used.min(self.mem_total) as f32 * 100.0 / self.mem_total as f32);
        match metric {
            Metric::Cpu => self.cpu.map(|v| at_least(v, &t.cpu)),
            Metric::Memory => mem_share.map(|v| at_least(v, &t.memory)),
            Metric::Compressor => d.compressor.map(|v| (v, above(v, &t.compressor))),
            Metric::Swap => d.swap_used.map(|b| b as f32 / GIB).map(|v| (v, above(v, &t.swap))),
            Metric::Pressure => d.pressure.map(|p| (0.0, [None, Some(Level::Warn), Some(Level::Crit)][p as usize])),
            Metric::SystemFree => d.system_free.map(|v| (v, below(v, &t.system_free))),
            // Linux only: on macOS "available" is an estimate of ours; its kernel's own is
            // the free percentage above.
            Metric::Available => mem_share.filter(|_| d.os == Some(Os::Linux)).map(|used| 100.0 - used).map(|v| (v, below(v, &t.available))),
            Metric::Psi => d.pressure_full.map(|v| (v, above(v, &t.psi))),
            Metric::Oom => self.oom_kills_watched.map(|v| (v as f32, None)),
            Metric::Disk => self.worst_disk(&t.disk).map(|x| (x.used(), x.level(&t.disk))),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Metric {
    Cpu,
    Memory,
    /// macOS
    Compressor,
    Swap,
    Pressure,
    SystemFree,
    /// Linux
    Available,
    Psi,
    Oom,
    /// Every machine: each filesystem listed.
    Disk,
}

const METRICS: [Metric; 10] = [Metric::Cpu, Metric::Memory, Metric::Compressor, Metric::Swap, Metric::Pressure, Metric::SystemFree, Metric::Available, Metric::Psi, Metric::Oom, Metric::Disk];

/// How bad an alert is.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Warn,
    Crit,
}

impl Thresholds {
    pub fn on(&self, metric: Metric) -> bool {
        match metric {
            Metric::Cpu => self.cpu.on,
            Metric::Memory => self.memory.on,
            Metric::Compressor => self.compressor.on,
            Metric::Swap => self.swap.on,
            Metric::Pressure => self.pressure.on,
            Metric::SystemFree => self.system_free.on,
            Metric::Available => self.available.on,
            Metric::Psi => self.psi.on,
            Metric::Oom => self.oom.on,
            Metric::Disk => self.disk.on,
        }
    }

    /// How long a metric must hold before it rises; none: at once. OOM: how long it is held.
    pub fn minutes(&self, metric: Metric) -> Option<u8> {
        match metric {
            Metric::Cpu => Some(self.cpu.minutes),
            Metric::Memory => Some(self.memory.minutes),
            Metric::Compressor => self.compressor.minutes,
            Metric::Swap => self.swap.minutes,
            Metric::Pressure => Some(self.pressure.minutes),
            Metric::SystemFree => self.system_free.minutes,
            Metric::Available => self.available.minutes,
            Metric::Psi => self.psi.minutes,
            Metric::Oom => Some(self.oom.minutes),
            Metric::Disk => Some(self.disk.minutes),
        }
    }

    /// The threshold a level is past, in the metric's unit (none for pressure and OOM, whose
    /// levels are fixed), and for disk the free GB too.
    fn threshold(&self, metric: Metric, level: Level) -> (Option<f32>, Option<f32>) {
        let pick = |l: &Levels| Some(if level == Level::Crit { l.crit } else { l.warn });
        match metric {
            Metric::Cpu => (Some(self.cpu.percent as f32), None),
            Metric::Memory => (Some(self.memory.percent as f32), None),
            Metric::Compressor => (pick(&self.compressor), None),
            Metric::Swap => (pick(&self.swap), None),
            Metric::SystemFree => (pick(&self.system_free), None),
            Metric::Available => (pick(&self.available), None),
            Metric::Psi => (pick(&self.psi), None),
            Metric::Pressure | Metric::Oom => (None, None),
            Metric::Disk if level == Level::Crit => (Some(self.disk.crit), Some(self.disk.crit_free)),
            Metric::Disk => (Some(self.disk.warn), Some(self.disk.warn_free)),
        }
    }

    /// Each value in its range, warn before crit, a window of 1 to `MAX_MINUTES` minutes
    /// (what the samples kept cover); the compressor alerts at once, so it has none.
    pub fn check(&self) -> Result<(), String> {
        let window = |name: &str, m: u8| if (1..=MAX_MINUTES).contains(&m) { Ok(()) } else { Err(format!("the {name} alert holds for 1 to {MAX_MINUTES} minutes, not {m}")) };
        for (name, l) in [("CPU", &self.cpu), ("memory", &self.memory)] {
            if !(1..=100).contains(&l.percent) {
                return Err(format!("the {name} threshold is a percent from 1 to 100, not {}", l.percent));
            }
            window(name, l.minutes)?;
        }
        // (name, levels, highest value, alerts over the thresholds rather than under them)
        let levels = [("compressor", &self.compressor, 100.0, true), ("swap", &self.swap, 1024.0, true), ("system free", &self.system_free, 100.0, false), ("available memory", &self.available, 100.0, false), ("PSI", &self.psi, 100.0, true)];
        for (name, l, max, over) in levels {
            let range = 0.0..=max;
            if !range.contains(&l.warn) || !range.contains(&l.crit) {
                return Err(format!("the {name} thresholds are from 0 to {max}, not {} and {}", l.warn, l.crit));
            }
            if over && l.crit < l.warn || !over && l.crit > l.warn {
                return Err(format!("the {name} critical threshold is past the warning one ({} / {})", l.warn, l.crit));
            }
            match (name, l.minutes) {
                ("compressor", None) => {}
                ("compressor", Some(_)) => return Err("the compressor alerts on the sample that sees it: no minutes".into()),
                (_, Some(m)) => window(name, m)?,
                (_, None) => return Err(format!("the {name} alert needs its minutes")),
            }
        }
        window("pressure", self.pressure.minutes)?;
        window("OOM", self.oom.minutes)?;
        let d = &self.disk;
        if !(0.0..=100.0).contains(&d.warn) || !(0.0..=100.0).contains(&d.crit) || d.crit < d.warn {
            return Err(format!("the disk thresholds are percents used from 0 to 100, warning first, not {} and {}", d.warn, d.crit));
        }
        if d.crit_free < 0.0 || d.warn_free < d.crit_free {
            return Err(format!("the disk free thresholds are GB, warning the larger, not {} and {}", d.warn_free, d.crit_free));
        }
        window("disk", d.minutes)
    }
}

/// Whether a metric is in alert after the newest sample, since when (ms) and at what level.
/// `level` reads one sample: none when it has no value, else its level. A level rises once
/// every sample of the last `minutes` is at it or above (none: the newest sample alone
/// decides), with no break: a gap in the samples (the machine was away) starts the window
/// again. It steps down from critical, or clears, once every sample of a minute is below.
/// While the machine cannot be reached, or the metric has no value, it stays as it was.
pub fn evaluate(samples: &[Sample], level: &dyn Fn(&Sample) -> Option<Option<Level>>, minutes: Option<u8>, raised: Option<(i64, Level)>, reachable: bool, step_ms: i64) -> Option<(i64, Level)> {
    let Some(last) = samples.last().filter(|s| reachable && level(s).is_some()) else { return raised };
    // Whether a sample is at `l` or above (`up`), or below it.
    let is = |s: &Sample, l: Level, up: bool| level(s).map(|x| (x >= Some(l)) == up);
    // Where the run of samples like the newest one starts: the samples on either side of a
    // gap of more than three steps do not belong to one run.
    let start_of_run = |l: Level, up: bool| {
        let mut start = last.at;
        for pair in samples.windows(2).rev() {
            if pair[1].at - pair[0].at > 3 * step_ms || is(&pair[0], l, up) != Some(true) {
                break;
            }
            start = pair[0].at;
        }
        start
    };
    // A run of n samples covers n steps: the last one stands for the step after it.
    let covers = |l: Level, up: bool, ms: i64| is(last, l, up) == Some(true) && last.at - start_of_run(l, up) + step_ms >= ms;
    let reached = [Level::Crit, Level::Warn].into_iter().find(|&l| match minutes {
        None => is(last, l, true) == Some(true),
        Some(m) => covers(l, true, m as i64 * 60_000),
    });
    match raised {
        None => reached.map(|l| (start_of_run(l, true), l)),
        Some(_) if covers(Level::Warn, false, CLEAR_MS) => None,
        Some((since, was)) => {
            let stays = if was == Level::Crit && covers(Level::Crit, false, CLEAR_MS) { Level::Warn } else { was };
            Some((since, reached.map_or(stays, |r| r.max(stays))))
        }
    }
}

/// OOM kills: critical on the sample that sees the count grow, held until `hold` minutes
/// have passed since the last kill. While the machine cannot be reached it stays as it was.
pub fn evaluate_oom(samples: &[Sample], hold: u8, raised: Option<(i64, Level)>, reachable: bool) -> Option<(i64, Level)> {
    let Some(last) = samples.last().filter(|s| reachable && s.oom_kills_watched.is_some()) else { return raised };
    let kill = samples.windows(2).rev().find(|p| p[1].oom_kills_watched > p[0].oom_kills_watched && p[0].oom_kills_watched.is_some()).map(|p| p[1].at)?;
    (last.at - kill < hold as i64 * 60_000).then(|| (raised.map_or(kill, |r| r.0), Level::Crit))
}

/// One of the processes that use the most memory on a machine, when an alert rose.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Proc {
    pub pid: u32,
    pub name: String,
    /// Resident memory, in bytes.
    pub rss: u64,
    /// The agent it belongs to (it descends from the agent's shell), and where that runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
}

/// How many processes an alert names.
const TOP: usize = 5;

/// Whether process `pid` on the machine `runner` reaches has started anything that still runs
/// (for a shell: a command in front, or a background job). An error unless the list was
/// read and holds `pid` itself: a list that could not be read (ssh failed and printed
/// nothing) never says "nothing runs".
pub fn has_children(runner: &scan::Runner, pid: u32) -> std::io::Result<bool> {
    let out = crate::common::process::output_within(runner.command(&["sh", "-c", PS]), TIMEOUT)?;
    let ps = parse_ps(&String::from_utf8_lossy(&out.stdout));
    if !out.status.success() || !ps.iter().any(|p| p.0 == pid) {
        return Err(std::io::Error::other(format!("the process list was not read, or has no process {pid} ({})", out.status)));
    }
    Ok(ps.iter().any(|p| p.1 == pid))
}

/// `PS`'s lines as (pid, parent, resident bytes, name). macOS names a process by its whole
/// path: the last part is kept. A name may hold spaces; it is the rest of the line.
fn parse_ps(text: &str) -> Vec<(u32, u32, u64, String)> {
    text.lines()
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            let (pid, ppid, rss) = (w.next()?.parse().ok()?, w.next()?.parse().ok()?, w.next()?.parse::<u64>().ok()?);
            let name = w.collect::<Vec<_>>().join(" ");
            let name = if name.starts_with('/') { name.rsplit('/').next().unwrap_or(&name).to_string() } else { name };
            Some((pid, ppid, rss * 1024, name))
        })
        .collect()
}

/// The processes using the most memory, largest first, each with the agent it descends
/// from: its parents are walked up until one is at the root of an agent's place.
fn top_of(ps: &[(u32, u32, u64, String)], agents: &[(RuntimeAgent, Vec<u32>)]) -> Vec<Proc> {
    let parent: std::collections::HashMap<u32, u32> = ps.iter().map(|p| (p.0, p.1)).collect();
    let owner = |mut pid: u32| {
        // Bounded: a parent table read while processes come and go may hold a loop.
        for _ in 0..64 {
            if let Some((a, _)) = agents.iter().find(|(_, roots)| roots.contains(&pid)) {
                return Some(a);
            }
            pid = *parent.get(&pid).filter(|p| **p > 1)?;
        }
        None
    };
    let mut big: Vec<_> = ps.iter().collect();
    big.sort_by_key(|p| std::cmp::Reverse(p.2));
    big.into_iter()
        .take(TOP)
        .map(|(pid, _, rss, name)| {
            let a = owner(*pid);
            Proc { pid: *pid, name: name.clone(), rss: *rss, agent: a.map(|a| a.name.clone()), place: a.and_then(|a| a.group.as_ref()).map(|g| g.label.clone()) }
        })
        .collect()
}

/// A machine in alert, as the workspace snapshot carries it for Needs you.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub machine: String,
    /// It is the machine workriver runs on.
    pub here: bool,
    pub metric: Metric,
    pub level: Level,
    /// Since when (RFC 3339): the first sample of the run past the threshold.
    pub since: String,
    /// The newest value, in the metric's unit (`Sample::reading`).
    pub value: f32,
    /// The threshold `level` is past, in the same unit; none for pressure and OOM.
    pub threshold: Option<f32>,
    /// Disk: the free GB of `level`, and the filesystem nearest its limits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_free: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk: Option<Disk>,
    /// How long it had to hold, in minutes; none: on the sample that saw it. OOM: how long
    /// it is held after the last kill.
    pub minutes: Option<u8>,
    /// The processes using the most memory when it rose or last rose a level, largest
    /// first; empty until they are read.
    pub top: Vec<Proc>,
}

/// An alert of a machine as the Machines section shows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Raise {
    /// RFC 3339.
    pub since: String,
    pub level: Level,
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
    /// The metrics in alert, by name.
    pub alerts: BTreeMap<String, Raise>,
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
    raised: BTreeMap<Metric, Raised>,
}

/// An alert of a machine: since when (ms), its level, and the processes using the most
/// memory when it rose to that level (none: to be read).
#[derive(Clone, Debug, PartialEq)]
struct Raised {
    since: i64,
    level: Level,
    top: Option<Vec<Proc>>,
}

/// Reads every machine on a thread of its own, keeps the samples, raises and clears alerts.
pub struct Metrics {
    home: PathBuf,
    /// Where the probe runs for this machine: here (a stand-in in tests).
    local: scan::Runner,
    machines: Option<Arc<Machines>>,
    /// Which agent a process belongs to, when an alert names the processes.
    runtime: Arc<dyn Runtime>,
    tracks: Mutex<BTreeMap<String, Track>>,
    running: Mutex<HashSet<String>>,
    thresholds: Mutex<Thresholds>,
    /// The page hears when an alert is raised, changes level or clears, and only then.
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
            compressor: avg(&|s| s.detail.compressor.map(f64::from)).map(|v| v as f32),
            system_free: avg(&|s| s.detail.system_free.map(f64::from)).map(|v| v as f32),
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
    home.join(crate::common::home::METRICS).join(format!("{name}.jsonl"))
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
    pub fn new(home: PathBuf, local: scan::Runner, machines: Option<Arc<Machines>>, runtime: Arc<dyn Runtime>, changed: broadcast::Sender<()>, step: Duration) -> Arc<Metrics> {
        let thresholds = crate::runtime::Settings::thresholds(&home);
        Arc::new(Metrics { home, local, machines, runtime, tracks: Mutex::default(), running: Mutex::default(), thresholds: Mutex::new(thresholds), changed, step, stop: AtomicBool::new(false) })
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
            let out = crate::common::process::output_within(r.command(&["sh", "-c", PROBE]), TIMEOUT).map_err(|e| format!("cannot run the probe: {e}"));
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
        let mut changed = Self::judge(track, &thresholds, self.step);
        // An alert that rose, or rose a level, names the processes using the most memory: one
        // more probe then, never on a quiet sample. A machine away is asked once it is back.
        let wants_top = track.reachable && track.raised.values().any(|r| r.top.is_none());
        drop(tracks);
        if wants_top {
            let top = self.top(label);
            if let Some(track) = self.tracks.lock().unwrap().get_mut(label) {
                for r in track.raised.values_mut().filter(|r| r.top.is_none()) {
                    r.top = Some(top.clone());
                    changed = true;
                }
            }
        }
        if changed {
            let _ = self.changed.send(());
        }
        // On disk outside the lock: the page's reads do not wait for it.
        if let Some((line, trim)) = done_minute
            && let Err(e) = keep_minute(&self.home, label, &line, trim.then(|| now_ms() - HISTORY_MS))
        {
            tracing::warn!(machine = label, "cannot keep the metrics history: {e}");
        }
    }

    /// The processes using the most memory on a machine, with the agents they belong to;
    /// none when it cannot be read (that alert then names none rather than ask again).
    fn top(&self, label: &str) -> Vec<Proc> {
        let Some(runner) = self.runner(label) else { return vec![] };
        let started = Instant::now();
        let out = crate::common::process::output_within(runner.command(&["sh", "-c", PS]), TIMEOUT);
        tracing::debug!(target: "perf", kind = "ps", machine = label, ms = started.elapsed().as_millis() as u64);
        let ps = match out {
            Ok(o) => parse_ps(&String::from_utf8_lossy(&o.stdout)),
            Err(e) => {
                tracing::warn!(machine = label, "cannot list the processes for an alert: {e}");
                return vec![];
            }
        };
        let agents = self.runtime.processes((label != HERE).then_some(label)).unwrap_or_default();
        top_of(&ps, &agents)
    }

    /// A machine's minutes of the last `hours`, oldest first, read from its history file.
    pub fn history(&self, label: &str, hours: u32) -> Result<Vec<Sample>, String> {
        if !(1..=MAX_HOURS).contains(&hours) {
            return Err(format!("the history goes back 1 to {MAX_HOURS} hours, not {hours}"));
        }
        read_history(&self.home, label, now_ms() - hours as i64 * 3_600_000).map_err(|e| format!("cannot read the metrics history: {e}"))
    }

    /// Raises, levels and clears a machine's alerts; says whether any changed. An alert that
    /// rose a level is to name its processes again.
    fn judge(track: &mut Track, t: &Thresholds, step: Duration) -> bool {
        let samples = track.samples.make_contiguous();
        let mut changed = false;
        for metric in METRICS {
            let was = track.raised.get(&metric).cloned();
            let before = was.as_ref().map(|r| (r.since, r.level));
            let now = match metric {
                _ if !t.on(metric) => None,
                Metric::Oom => evaluate_oom(samples, t.oom.minutes, before, track.reachable),
                _ => evaluate(samples, &|s| s.reading(metric, t).map(|r| r.1), t.minutes(metric), before, track.reachable, step.as_millis() as i64),
            };
            changed |= now.map(|n| n.1) != before.map(|b| b.1);
            match now {
                Some((since, level)) => {
                    let top = was.filter(|w| level <= w.level).and_then(|w| w.top);
                    track.raised.insert(metric, Raised { since, level, top })
                }
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
        crate::runtime::Settings::set_thresholds(&self.home, t)?;
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
                    alerts: t.map(|t| t.raised.iter().map(|(m, r)| (serde_json::to_value(m).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default(), Raise { since: rfc3339(r.since), level: r.level })).collect()).unwrap_or_default(),
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
            for (&metric, r) in &t.raised {
                let last = t.samples.back();
                let value = last.and_then(|s| s.reading(metric, &thresholds)).map_or(0.0, |v| v.0);
                let (threshold, threshold_free) = thresholds.threshold(metric, r.level);
                let disk = last.filter(|_| metric == Metric::Disk).and_then(|s| s.worst_disk(&thresholds.disk)).cloned();
                let top = r.top.clone().unwrap_or_default();
                out.push(Alert { machine: label.clone(), here: label == HERE, metric, level: r.level, since: rfc3339(r.since), value, threshold, threshold_free, disk, minutes: thresholds.minutes(metric), top });
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
hw.memsize: 34359738368
kern.memorystatus_vm_pressure_level: 2
kern.memorystatus_level: 50
vm.swapusage: total = 4096.00M  used = 3659.88M  free = 436.12M  (encrypted)
vm.compressor.segment.total: 279400
vm.compressor.segment.limit: 1647102
vm.compressor_segment_pages_compressed: 1980784
vm.compressor_segment_pages_compressed_limit: 13176816
Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                                    33895.
Pages active:                                 502965.
Pages inactive:                               482372.
Pages speculative:                             18959.
Pages throttled:                                   0.
Pages wired down:                             199417.
Pages purgeable:                                2908.
\"Translation faults\":                     1627581914.
Pages stored in compressor:                  1980724.
Pages occupied by compressor:                 819908.
Swapouts:                                     241400.
iostat  10 12 78  5.17 4.79 4.41
Filesystem     1024-blocks      Used Available Capacity  Mounted on
/dev/disk3s5     482797652 375308792  81939804    83%    /System/Volumes/Data
/dev/disk3s1s1   482797652  12349052  81939804    14%    /
";

    /// A process list that was not read, or does not hold the process asked about, is an
    /// error: never "it started nothing".
    #[test]
    fn children_are_told_only_from_a_list_that_was_read() {
        let null = std::process::Stdio::null;
        let mut sh = std::process::Command::new("sh").args(["-c", "sleep 30 & wait"]).stdin(null()).stdout(null()).stderr(null()).spawn().unwrap();
        let mut alone = std::process::Command::new("sleep").arg("30").stdin(null()).stdout(null()).stderr(null()).spawn().unwrap();
        // The shell forks its job a moment after it starts.
        assert!((0..50).any(|_| has_children(&scan::Runner::Local, sh.id()).unwrap() || { std::thread::sleep(Duration::from_millis(100)); false }));
        assert!(!has_children(&scan::Runner::Local, alone.id()).unwrap());
        assert!(has_children(&scan::Runner::Local, 4_000_000_000).is_err(), "not in the list");
        let failing = scan::Runner::Ssh { program: "false".into(), options: vec![], target: "box".into() };
        assert!(has_children(&failing, alone.id()).is_err(), "the list was not read");
        for c in [&mut sh, &mut alone] {
            c.kill().unwrap();
            c.wait().unwrap();
        }
    }

    /// What `PS` printed on the same Mac, a few of its lines (one agent's shell made the
    /// parent of its agent, as under the agent runtime).
    const PS_DARWIN: &str = "    1     0  22960 /sbin/launchd
 6286     1   1200 /usr/bin/login
 6287  6286   1328 -zsh
 6567  6287 783504 claude
57934  1354 2396528 /Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Versions/157.0.8103.51/Helpers/Google Chrome Helper (Renderer).app/Contents/MacOS/Google Chrome Helper (Renderer)
 1354     1 484432 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome
 1431     1 382016 /System/Library/CoreServices/Spotlight.app/Contents/MacOS/Spotlight
  522     1  27728 /usr/libexec/logd
";

    /// What `PS` printed in `docker run --rm debian:stable-slim` (no `ps` there), a
    /// background `sleep` running; and a line of a thread whose name has a space.
    const PS_LINUX: &str = "1 0 1068 sh
14 1 1236 sh
15 14 1020 cat
7 1 1116 sleep
31 1 2048 tmux: server
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
        assert_eq!(r.mem_used, 34_359_738_368 - (33895 + 482372 + 18959 + 2908) * 16384);
        assert_eq!(r.cpu, Cpu::Percent(22.0));
        assert_eq!(cpu_share(None, &r), Some(22.0), "a Mac measures its own second");
        let d = &r.detail;
        assert_eq!(d.mem_compressed, Some(819_908 * 16384));
        assert_eq!(d.pressure, Some(Pressure::Warning));
        assert_eq!(d.system_free, Some(50.0));
        // The segments are nearer their limit (17.0 %) than the compressed pages (15.0 %).
        assert_eq!(d.compressor, Some(279_400.0 * 100.0 / 1_647_102.0));
        assert_eq!((d.swap_used, d.swap_total), (Some((3659.88 * 1_048_576.0) as u64), Some(4096 << 20)));
        assert_eq!((d.pressure_some, d.oom_kills), (None, None), "Linux only");
        // `/` is a volume of the same APFS container as the home folder's: listed once.
        assert_eq!(d.disks, [Disk { mount: "/System/Volumes/Data".into(), free: 81_939_804 * 1024, total: 482_797_652 * 1024 }]);
        // Without the pressure, swap and compressor sysctls (an older macOS, a sandbox): none, still read.
        let bare: String = DARWIN.lines().filter(|l| !l.starts_with("kern.") && !l.starts_with("vm.")).map(|l| format!("{l}\n")).collect();
        let d = parse(&bare).unwrap().detail;
        assert_eq!((d.pressure, d.swap_used, d.swap_total, d.compressor, d.system_free), (None, None, None, None, None));
        assert_eq!(parse(&DARWIN.replace("level: 2", "level: 4")).unwrap().detail.pressure, Some(Pressure::Critical));
        // Fewer segments than compressed pages: the pages decide.
        assert_eq!(parse(&DARWIN.replace("total: 279400", "total: 1")).unwrap().detail.compressor, Some(1_980_784.0 * 100.0 / 13_176_816.0));
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
            let d = &r.detail;
            assert!(d.pressure.is_some() && d.swap_total.is_some() && d.mem_compressed.is_some() && d.system_free.is_some(), "{r:?}");
            assert!(d.compressor.is_some_and(|c| c > 0.0 && c < 100.0), "{r:?}");
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

    /// CPU or memory judged by `limit`, as `judge` does: raised at the one level they have.
    fn eval(samples: &[Sample], metric: Metric, limit: &Limit, raised: Option<i64>, reachable: bool) -> Option<i64> {
        let t = Thresholds { cpu: *limit, memory: *limit, ..Thresholds::default() };
        if !t.on(metric) {
            return None;
        }
        let r = evaluate(samples, &|s| s.reading(metric, &t).map(|r| r.1), t.minutes(metric), raised.map(|r| (r, Level::Warn)), reachable, STEP_MS);
        r.map(|r| r.0)
    }

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
        assert_eq!(eval(&run(&[95.0; 11]), Metric::Cpu, &CPU, None, true), None);
        assert_eq!(eval(&run(&[95.0; 12]), Metric::Cpu, &CPU, None, true), Some(0));
        // The run starts after the last sample below.
        let mut s = vec![10.0, 10.0];
        s.extend([95.0; 12]);
        assert_eq!(eval(&run(&s), Metric::Cpu, &CPU, None, true), Some(2 * STEP_MS));
    }

    #[test]
    fn a_spike_does_not_raise() {
        let mut s = vec![95.0; 6];
        s.push(50.0);
        s.extend([95.0; 6]);
        assert_eq!(eval(&run(&s), Metric::Cpu, &CPU, None, true), None);
    }

    #[test]
    fn clears_after_a_minute_below() {
        let mut s = vec![95.0; 12];
        s.extend([50.0; 11]);
        assert_eq!(eval(&run(&s), Metric::Cpu, &CPU, Some(0), true), Some(0), "55 s below is not yet a minute");
        s.push(50.0);
        assert_eq!(eval(&run(&s), Metric::Cpu, &CPU, Some(0), true), None);
        // One sample back over the threshold keeps it raised.
        let mut s = vec![50.0; 11];
        s.push(95.0);
        assert_eq!(eval(&run(&s), Metric::Cpu, &CPU, Some(0), true), Some(0));
    }

    #[test]
    fn frozen_while_unreachable_and_never_when_switched_off() {
        let hot = run(&[95.0; 12]);
        assert_eq!(eval(&hot, Metric::Cpu, &CPU, None, false), None, "not raised while away");
        assert_eq!(eval(&run(&[10.0; 20]), Metric::Cpu, &CPU, Some(0), false), Some(0), "nor cleared");
        let off = Limit { on: false, ..CPU };
        assert_eq!(eval(&hot, Metric::Cpu, &off, Some(0), true), None);
    }

    #[test]
    fn a_gap_starts_the_window_again() {
        // Six samples, the machine away for a minute, six more: no run of a minute.
        let mut s = run(&[95.0; 6]);
        s.extend(run(&[95.0; 6]).into_iter().map(|x| Sample { at: x.at + 90_000, ..x }));
        assert_eq!(eval(&s, Metric::Cpu, &CPU, None, true), None);
    }

    #[test]
    fn memory_is_used_over_total() {
        let full: Vec<Sample> = (0..12).map(|k| bare(k * STEP_MS, None, 95, 100)).collect();
        assert_eq!(eval(&full, Metric::Memory, &CPU, None, true), Some(0));
        assert_eq!(eval(&full, Metric::Cpu, &CPU, None, true), None, "no CPU value yet: nothing to judge");
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
        assert_eq!(crate::runtime::Settings::thresholds(dir.path()), Thresholds::default(), "the defaults until set");
        let set = Thresholds { cpu: Limit { on: false, percent: 80, minutes: 2 }, memory: Limit { on: true, percent: 95, minutes: 10 }, ..Thresholds::default() };
        crate::runtime::Settings::set_thresholds(dir.path(), set).unwrap();
        assert_eq!(crate::runtime::Settings::thresholds(dir.path()), set);
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
        let m = Metrics::new(dir.path().to_path_buf(), printing(dir.path(), "here", DARWIN), None, Arc::new(workriver_core::runtime::NoRuntime), tx, Duration::from_secs(5));
        assert_eq!(m.history("box", 168).unwrap(), [day_ago.clone(), recent.clone()]);
        assert_eq!(m.history("box", 1).unwrap(), [recent], "the last hour only");
        assert!(m.history("box", 0).is_err() && m.history("box", 169).unwrap_err().contains("168"));
        assert_eq!(m.history("never-seen", 24).unwrap(), []);
        assert!(m.report().machines[0].samples.is_empty());
        let summary = crate::cli::history_text(&serde_json::json!({ "label": "box" }), &serde_json::to_value(m.history("box", 168).unwrap()).unwrap());
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
        let local: Arc<dyn workriver_core::runtime::Runtime> = Arc::new(crate::server::tests::support::Fake::default());
        let all = Machines::with(local.clone(), vec![("box", local.clone()), ("gone", local)]);
        all.set_runner("box", printing(dir.path(), "box", LINUX_1));
        all.drop_tunnel("gone");
        let (tx, mut rx) = broadcast::channel(4);
        let m = Metrics::new(dir.path().to_path_buf(), printing(dir.path(), "here", DARWIN), Some(all.clone()), all.clone(), tx, Duration::from_millis(10));
        for label in [HERE, "box", "gone"] {
            m.sample(label);
        }
        all.set_runner("box", printing(dir.path(), "box2", LINUX_2));
        m.sample("box");
        let r = m.report();
        let get = |l: &str| r.machines.iter().find(|x| x.label == l).unwrap().clone();
        assert_eq!(r.machines.iter().map(|x| x.label.as_str()).collect::<Vec<_>>(), [HERE, "box", "gone"]);
        let here = get(HERE);
        assert!(here.here && here.reachable && here.samples.len() == 1 && here.samples[0].cpu == Some(22.0));
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
        assert!(rx.try_recv().is_ok(), "the kills raised the OOM alert");
        let text = crate::cli::load_text(&serde_json::to_value(m.report()).unwrap());
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[1], "  available 8.2 GB  pressure warning  compressed 12.5 GB  compressor 17% of its limit  system free 50%  swap 3.6/4.0 GB  disk 78.1 GB free of 460.4 GB", "{text}");
        assert!(lines[2].contains("  ALERT: oom CRIT since 20"), "{text}");
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
        m.tracks.lock().unwrap().get_mut("box").unwrap().raised.insert(Metric::Cpu, Raised { since: now_ms(), level: Level::Warn, top: Some(vec![]) });
        assert_eq!(m.alerts().iter().map(|a| (a.machine.as_str(), a.metric)).collect::<Vec<_>>(), [("box", Metric::Cpu), ("box", Metric::Oom)], "the OOM alert is held while the machine is away");
        all.stop("box");
        assert!(m.alerts().is_empty(), "the alert of a machine no longer followed");
        assert!(rx.try_recv().is_err(), "nothing raised, nothing said");
        // Thresholds low enough for this machine's memory (77 %): raised at once? Not before
        // the window is covered: one sample covers 5 s of a minute.
        m.set_thresholds(Thresholds { cpu: Limit { on: false, percent: 90, minutes: 1 }, memory: Limit { on: true, percent: 50, minutes: 1 }, ..Thresholds::default() }).unwrap();
        assert!(m.alerts().is_empty());
        assert!(m.set_thresholds(Thresholds { cpu: Limit { on: true, percent: 0, minutes: 1 }, ..Thresholds::default() }).is_err());
    }

    #[test]
    fn says_when_an_alert_is_raised_and_cleared_only() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = broadcast::channel(4);
        let m = Metrics::new(dir.path().to_path_buf(), printing(dir.path(), "here", DARWIN), None, Arc::new(workriver_core::runtime::NoRuntime), tx, Duration::from_secs(5));
        // A minute of samples at 77 % memory, as the sampler would have kept them.
        {
            let mut tracks = m.tracks.lock().unwrap();
            let t = tracks.entry(HERE.into()).or_default();
            t.reachable = true;
            let now = now_ms();
            t.samples.extend((0..12).map(|k| bare(now - (11 - k) * 5_000, Some(10.0), 77, 100)));
        }
        m.set_thresholds(Thresholds { cpu: Limit { on: true, percent: 90, minutes: 1 }, memory: Limit { on: true, percent: 70, minutes: 1 }, ..Thresholds::default() }).unwrap();
        assert!(rx.try_recv().is_ok(), "raised: the page hears of it");
        let a = m.alerts();
        assert_eq!((a.len(), a[0].metric, a[0].level, a[0].threshold, a[0].minutes, a[0].here), (1, Metric::Memory, Level::Warn, Some(70.0), Some(1), true));
        assert_eq!(m.report().machines[0].alerts.iter().map(|(k, r)| (k.as_str(), r.level)).collect::<Vec<_>>(), [("memory", Level::Warn)]);
        // The same again changes nothing and says nothing.
        m.set_thresholds(Thresholds { cpu: Limit { on: true, percent: 90, minutes: 1 }, memory: Limit { on: true, percent: 70, minutes: 1 }, ..Thresholds::default() }).unwrap();
        assert!(rx.try_recv().is_err());
        // Switched off: cleared, and said.
        m.set_thresholds(Thresholds { cpu: Limit { on: true, percent: 90, minutes: 1 }, memory: Limit { on: false, percent: 70, minutes: 1 }, ..Thresholds::default() }).unwrap();
        assert!(rx.try_recv().is_ok() && m.alerts().is_empty());
    }
    /// A sample at `at` (ms) with `detail`, memory half used.
    fn with(at: i64, detail: Detail) -> Sample {
        Sample { at, cpu: Some(1.0), mem_used: 1, mem_total: 2, oom_kills_watched: None, detail }
    }

    #[test]
    fn each_threshold_has_its_warn_and_crit_edges() {
        use Level::{Crit, Warn};
        let t = Thresholds::default();
        let level = |s: Sample, m: Metric| s.reading(m, &t).map(|r| r.1);
        let os = |os: Os, f: &dyn Fn(&mut Detail)| {
            let mut d = Detail { os: Some(os), ..Detail::default() };
            f(&mut d);
            with(0, d)
        };
        let gb = |g: f64| (g * GIB as f64).round() as u64;
        for (v, want) in [(60.0, None), (60.1, Some(Warn)), (80.0, Some(Warn)), (80.1, Some(Crit))] {
            assert_eq!(level(os(Os::Darwin, &|d| d.compressor = Some(v)), Metric::Compressor), Some(want), "compressor {v}");
        }
        for (v, want) in [(4.0, None), (4.01, Some(Warn)), (10.0, Some(Warn)), (10.01, Some(Crit))] {
            assert_eq!(level(os(Os::Darwin, &|d| d.swap_used = Some(gb(v))), Metric::Swap), Some(want), "swap {v} GB");
        }
        for (p, want) in [(Pressure::Normal, None), (Pressure::Warning, Some(Warn)), (Pressure::Critical, Some(Crit))] {
            assert_eq!(level(os(Os::Darwin, &|d| d.pressure = Some(p)), Metric::Pressure), Some(want), "{p:?}");
        }
        for (v, want) in [(25.0, None), (24.9, Some(Warn)), (20.0, Some(Warn)), (19.9, Some(Crit))] {
            assert_eq!(level(os(Os::Darwin, &|d| d.system_free = Some(v)), Metric::SystemFree), Some(want), "system free {v}");
        }
        // Linux available: MemAvailable over MemTotal.
        for (available, want) in [(2500, None), (2499, Some(Warn)), (1200, Some(Warn)), (1199, Some(Crit))] {
            let s = Sample { mem_used: 10_000 - available, mem_total: 10_000, ..os(Os::Linux, &|_| ()) };
            assert_eq!(level(s, Metric::Available), Some(want), "available {available} of 10000");
        }
        assert_eq!(level(Sample { mem_used: 9_999, mem_total: 10_000, ..os(Os::Darwin, &|_| ()) }, Metric::Available), None, "Linux only");
        for (v, want) in [(5.0, None), (5.1, Some(Warn)), (20.0, Some(Warn)), (20.1, Some(Crit))] {
            assert_eq!(level(os(Os::Linux, &|d| d.pressure_full = Some(v)), Metric::Psi), Some(want), "PSI full {v}");
        }
        // Disk: used past 80 / 90 %, or free under 50 / 20 GB.
        let disk = |free: f64, total: f64| Disk { mount: "/".into(), free: gb(free), total: gb(total) };
        for (free, total, want) in [(200.0, 1000.0, None), (199.0, 1000.0, Some(Warn)), (100.0, 1000.0, Some(Warn)), (99.0, 1000.0, Some(Crit)), (50.0, 60.0, None), (49.0, 60.0, Some(Warn)), (20.0, 60.0, Some(Warn)), (19.0, 60.0, Some(Crit))] {
            assert_eq!(level(os(Os::Linux, &|d| d.disks = vec![disk(free, total)]), Metric::Disk), Some(want), "disk {free} free of {total} GB");
        }
        // Each filesystem listed counts: the worst decides; one of no size is not judged.
        let s = os(Os::Darwin, &|d| d.disks = vec![Disk { mount: "/home".into(), free: 0, total: 0 }, disk(500.0, 1000.0), Disk { mount: "/data".into(), ..disk(10.0, 1000.0) }]);
        assert_eq!(s.worst_disk(&t.disk).map(|d| d.mount.as_str()), Some("/data"));
        assert_eq!(s.reading(Metric::Disk, &t), Some((99.0, Some(Crit))));
        // What a machine does not have is not judged (a value of none, not a level of none).
        assert_eq!(os(Os::Linux, &|_| ()).reading(Metric::Compressor, &t), None);
    }

    /// One of the memory metrics judged by the default thresholds, as `judge` does.
    fn judged(samples: &[Sample], metric: Metric, raised: Option<(i64, Level)>) -> Option<(i64, Level)> {
        let t = Thresholds::default();
        evaluate(samples, &|s| s.reading(metric, &t).map(|r| r.1), t.minutes(metric), raised, true, STEP_MS)
    }

    #[test]
    fn the_compressor_alerts_on_the_sample_that_sees_it_and_rises_in_place() {
        use Level::{Crit, Warn};
        let at = |k: i64, v: f32| with(k * STEP_MS, Detail { compressor: Some(v), ..Detail::default() });
        assert_eq!(judged(&[at(0, 10.0), at(1, 61.0)], Metric::Compressor, None), Some((STEP_MS, Warn)), "one sample is enough");
        assert_eq!(judged(&[at(0, 10.0), at(1, 85.0)], Metric::Compressor, None), Some((STEP_MS, Crit)));
        // Rising a level keeps its since.
        assert_eq!(judged(&[at(0, 10.0), at(1, 61.0), at(2, 85.0)], Metric::Compressor, Some((STEP_MS, Warn))), Some((STEP_MS, Crit)));
        // It steps down after a minute below critical, and clears after a minute below warning.
        let mut s = vec![at(0, 85.0)];
        s.extend((1..=11).map(|k| at(k, 70.0)));
        assert_eq!(judged(&s, Metric::Compressor, Some((0, Crit))), Some((0, Crit)), "55 s below");
        s.push(at(12, 70.0));
        assert_eq!(judged(&s, Metric::Compressor, Some((0, Crit))), Some((0, Warn)));
        s.extend((13..=24).map(|k| at(k, 10.0)));
        assert_eq!(judged(&s, Metric::Compressor, Some((0, Warn))), None);
        // A spike while critical is still critical: no step down to rise again.
        assert_eq!(judged(&[at(0, 90.0), at(1, 10.0)], Metric::Compressor, Some((0, Crit))), Some((0, Crit)));
    }

    #[test]
    fn swap_rises_a_level_once_the_window_holds_it() {
        use Level::{Crit, Warn};
        let gb = |g: f32| (g * GIB) as u64;
        let at = |k: i64, g: f32| with(k * STEP_MS, Detail { swap_used: Some(gb(g)), ..Detail::default() });
        let mut s: Vec<Sample> = (0..11).map(|k| at(k, 5.0)).collect();
        assert_eq!(judged(&s, Metric::Swap, None), None, "55 s of a minute");
        s.push(at(11, 5.0));
        assert_eq!(judged(&s, Metric::Swap, None), Some((0, Warn)));
        // Critical for half a minute: still warn; for a minute: critical, the same alert.
        s.extend((12..18).map(|k| at(k, 12.0)));
        assert_eq!(judged(&s, Metric::Swap, Some((0, Warn))), Some((0, Warn)));
        s.extend((18..24).map(|k| at(k, 12.0)));
        assert_eq!(judged(&s, Metric::Swap, Some((0, Warn))), Some((0, Crit)));
        // Straight to critical when the whole minute is.
        assert_eq!(judged(&(0..12).map(|k| at(k, 12.0)).collect::<Vec<_>>(), Metric::Swap, None), Some((0, Crit)));
    }

    #[test]
    fn an_oom_kill_is_critical_at_once_and_held_five_minutes() {
        let at = |secs: i64, kills: u64| Sample { oom_kills_watched: Some(kills), ..with(secs * 1000, Detail::default()) };
        let mut s: Vec<Sample> = (0..10).map(|k| at(k * 5, 0)).collect();
        assert_eq!(evaluate_oom(&s, 5, None, true), None);
        s.push(at(50, 1));
        assert_eq!(evaluate_oom(&s, 5, None, true), Some((50_000, Level::Crit)), "on the sample that sees it");
        s.extend((11..70).map(|k| at(k * 5, 1)));
        assert_eq!(s.last().unwrap().at, 345_000);
        assert_eq!(evaluate_oom(&s, 5, Some((50_000, Level::Crit)), true), Some((50_000, Level::Crit)), "4 min 55 s after the kill");
        let mut cleared = s.clone();
        cleared.push(at(350, 1));
        assert_eq!(evaluate_oom(&cleared, 5, Some((50_000, Level::Crit)), true), None, "5 minutes after the last kill");
        // Another kill during the hold starts the five minutes again; the alert stays the same.
        s.push(at(350, 3));
        s.extend((71..120).map(|k| at(k * 5, 3)));
        assert_eq!(evaluate_oom(&s, 5, Some((50_000, Level::Crit)), true), Some((50_000, Level::Crit)));
        // While the machine is away it neither rises nor clears.
        assert_eq!(evaluate_oom(&cleared, 5, Some((50_000, Level::Crit)), false), Some((50_000, Level::Crit)));
        assert_eq!(evaluate_oom(&s[..11], 5, None, false), None);
    }

    #[test]
    fn thresholds_are_checked_level_by_level() {
        let bad = |f: &dyn Fn(&mut Thresholds), says: &str| {
            let mut t = Thresholds::default();
            f(&mut t);
            let e = t.check().unwrap_err();
            assert!(e.contains(says), "{e}");
        };
        bad(&|t| t.compressor.minutes = Some(1), "compressor");
        bad(&|t| t.swap.crit = 3.0, "swap");
        bad(&|t| t.swap.minutes = None, "swap");
        bad(&|t| t.available.crit = 30.0, "available");
        bad(&|t| t.system_free.warn = 101.0, "system free");
        bad(&|t| t.psi.minutes = Some(11), "PSI");
        bad(&|t| t.oom.minutes = 0, "OOM");
        bad(&|t| t.disk.crit = 70.0, "disk");
        bad(&|t| t.disk.crit_free = 60.0, "disk");
        let t = Thresholds { swap: Levels { on: false, warn: 1.0, crit: 1.0, minutes: Some(10) }, ..Thresholds::default() };
        assert!(t.check().is_ok());
    }

    #[test]
    fn an_older_settings_file_keeps_its_machines_and_takes_the_new_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let old = r#"{"machines":{"box":{"ssh":"box","enabled":true}},"alerts":{"cpu":{"on":false,"percent":80,"minutes":2},"memory":{"on":true,"percent":95,"minutes":10}}}"#;
        std::fs::write(dir.path().join("settings.json"), old).unwrap();
        let t = crate::runtime::Settings::thresholds(dir.path());
        let (cpu, memory) = (Limit { on: false, percent: 80, minutes: 2 }, Limit { on: true, percent: 95, minutes: 10 });
        assert_eq!(t, Thresholds { cpu, memory, ..Thresholds::default() });
        crate::runtime::Settings::set_thresholds(dir.path(), t).unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.path().join("settings.json")).unwrap()).unwrap();
        assert_eq!(saved["machines"]["box"]["ssh"], "box", "the machines stay");
        assert_eq!(saved["alerts"]["compressor"], serde_json::json!({ "on": true, "warn": 60.0, "crit": 80.0 }), "no minutes: at once");
        assert_eq!(saved["alerts"]["disk"]["warnFree"], 50.0);
    }

    #[test]
    fn reads_the_processes_of_either_system() {
        let mac = parse_ps(PS_DARWIN);
        assert_eq!(mac.len(), 8);
        assert_eq!(mac[3], (6567, 6287, 783_504 * 1024, "claude".to_string()));
        assert_eq!(mac[4].3, "Google Chrome Helper (Renderer)", "the last part of the path, spaces and all");
        assert_eq!(mac[2].3, "-zsh");
        let linux = parse_ps(PS_LINUX);
        assert_eq!(linux.iter().map(|p| (p.0, p.1, p.3.as_str())).collect::<Vec<_>>(), [(1, 0, "sh"), (14, 1, "sh"), (15, 14, "cat"), (7, 1, "sleep"), (31, 1, "tmux: server")]);
        assert!(parse_ps("not a line\n12 x 3 y\n").is_empty());
    }

    /// A runtime that knows only its agents and the processes at their roots.
    struct Pids(Vec<(RuntimeAgent, Vec<u32>)>);

    impl Runtime for Pids {
        fn agents(&self) -> Result<Vec<RuntimeAgent>, workriver_core::runtime::RuntimeError> {
            Ok(self.0.iter().map(|a| a.0.clone()).collect())
        }
        fn locate(&self, _: Option<&str>, _: &str) -> Result<workriver_core::runtime::Place, workriver_core::runtime::RuntimeError> {
            Err(workriver_core::runtime::RuntimeError::None)
        }
        fn focus(&self, _: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
            Err(workriver_core::runtime::RuntimeError::None)
        }
        fn send(&self, _: &str, _: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
            Err(workriver_core::runtime::RuntimeError::None)
        }
        fn start_temporary(&self, _: &str, _: &str, _: &str, _: &str) -> Result<RuntimeAgent, workriver_core::runtime::RuntimeError> {
            Err(workriver_core::runtime::RuntimeError::None)
        }
        fn close(&self, _: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
            Err(workriver_core::runtime::RuntimeError::None)
        }
        fn processes(&self, _: Option<&str>) -> Result<Vec<(RuntimeAgent, Vec<u32>)>, workriver_core::runtime::RuntimeError> {
            Ok(self.0.clone())
        }
    }

    fn agent(name: &str) -> RuntimeAgent {
        let group = Some(workriver_core::RuntimeRef { id: "w1".into(), label: "chat".into(), machine: None });
        RuntimeAgent { session: format!("s-{name}"), harness: "claude".into(), state: workriver_core::runtime::AgentState::Working, cwd: String::new(), group, name: name.into(), topic: None, machine: None }
    }

    #[test]
    fn the_largest_processes_are_named_with_the_agent_they_descend_from() {
        // The agent's shell is 6287; claude (6567) is its child. Another agent's shell is not
        // running anything here.
        let agents = [(agent("reviewer"), vec![6287]), (agent("idle"), vec![9999])];
        let top = top_of(&parse_ps(PS_DARWIN), &agents);
        let rows: Vec<_> = top.iter().map(|p| (p.pid, p.name.as_str(), p.agent.as_deref(), p.place.as_deref())).collect();
        assert_eq!(
            rows,
            [
                (57934, "Google Chrome Helper (Renderer)", None, None),
                (6567, "claude", Some("reviewer"), Some("chat")),
                (1354, "Google Chrome", None, None),
                (1431, "Spotlight", None, None),
                (522, "logd", None, None),
            ]
        );
        assert_eq!(top[1].rss, 783_504 * 1024);
        // A loop in the parents (read while processes came and went) ends.
        let looped = [(10, 11, 1, "a".to_string()), (11, 10, 1, "b".to_string())];
        assert_eq!(top_of(&looped, &agents)[0].agent, None);
    }

    /// An ssh stand-in that answers the probe with `probe` and `PS` with `ps`, and writes a
    /// line to the returned file each time it is asked for the processes.
    fn answering(dir: &std::path::Path, name: &str, probe: &str, ps: &str) -> (scan::Runner, PathBuf) {
        let (p, q, log) = (dir.join(format!("{name}.probe")), dir.join(format!("{name}.ps")), dir.join(format!("{name}.log")));
        std::fs::write(&p, probe).unwrap();
        std::fs::write(&q, ps).unwrap();
        let program = dir.join(name);
        std::fs::write(&program, format!("#!/bin/sh\ncase \"$*\" in *'ps -A'*) echo ps >> '{}'; cat '{}';; *) cat '{}';; esac\n", log.display(), q.display(), p.display())).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        (scan::Runner::Ssh { program: program.to_string_lossy().into(), options: vec![], target: name.into() }, log)
    }

    #[test]
    fn the_processes_are_read_only_when_an_alert_rises() {
        let dir = tempfile::tempdir().unwrap();
        let local: Arc<dyn Runtime> = Arc::new(Pids(vec![]));
        // A followed machine whose own runtime has an agent in the shell 6287.
        let all = Machines::with(local.clone(), vec![("box", Arc::new(Pids(vec![(agent("reviewer"), vec![6287])])))]);
        let (tx, mut rx) = broadcast::channel(16);
        let m = Metrics::new(dir.path().to_path_buf(), printing(dir.path(), "here", DARWIN), Some(all.clone()), all.clone(), tx, Duration::from_secs(5));
        let asked = |log: &PathBuf| std::fs::read_to_string(log).map(|t| t.lines().count()).unwrap_or(0);
        // Calm: the compressor at 17 %. Samples come within a second here, so nothing that
        // needs a minute rises.
        let (calm, log) = answering(dir.path(), "calm", DARWIN, PS_DARWIN);
        all.set_runner("box", calm);
        for _ in 0..3 {
            m.sample("box");
        }
        assert_eq!(asked(&log), 0, "no alert, no process list");
        assert!(m.alerts().is_empty() && rx.try_recv().is_err());
        // The compressor at 66 %: warn at once, and the processes are read once.
        let (warm, log) = answering(dir.path(), "warm", &DARWIN.replace("total: 279400", "total: 1100000"), PS_DARWIN);
        all.set_runner("box", warm);
        m.sample("box");
        assert_eq!(asked(&log), 1);
        assert!(rx.try_recv().is_ok(), "the page hears of it");
        let a = m.alerts();
        assert_eq!((a.len(), a[0].metric, a[0].level, a[0].threshold, a[0].minutes), (1, Metric::Compressor, Level::Warn, Some(60.0), None));
        assert_eq!((a[0].top.len(), a[0].top[1].agent.as_deref(), a[0].top[1].place.as_deref()), (5, Some("reviewer"), Some("chat")), "{:?}", a[0].top);
        assert!((a[0].value - 66.78).abs() < 0.01, "{}", a[0].value);
        m.sample("box");
        m.sample("box");
        assert_eq!(asked(&log), 1, "the same level again: not read again");
        assert!(rx.try_recv().is_err(), "nothing changed, nothing said");
        // 85 %: critical, the same alert, read again.
        let (hot, log) = answering(dir.path(), "hot", &DARWIN.replace("total: 279400", "total: 1400000"), PS_DARWIN);
        all.set_runner("box", hot);
        m.sample("box");
        assert_eq!(asked(&log), 1);
        let b = m.alerts();
        assert_eq!((b[0].level, &b[0].since, b[0].threshold), (Level::Crit, &a[0].since, Some(80.0)));
        assert_eq!(m.report().machines[1].alerts["compressor"].level, Level::Crit);
        assert!(rx.try_recv().is_ok());
        let text = crate::cli::load_text(&serde_json::to_value(m.report()).unwrap());
        assert!(text.contains("ALERT: compressor CRIT since"), "{text}");
    }
}
