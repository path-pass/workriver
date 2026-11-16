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
//! Kept in memory only, the last few minutes per machine; never in the event store, since
//! samples are not records of work. Alerts are derived as samples come, like the agents
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

/// A probe that has not answered in this long is given up; the machine counts as unreachable.
const TIMEOUT: Duration = Duration::from_secs(15);

/// One sample of a machine, as raw counters. Linux has CPU counters, so the CPU share is the
/// difference between two samples; macOS has none a shell can read, so iostat measures one
/// second itself. `LC_ALL=C`: numbers with a dot whatever the user's locale.
pub const PROBE: &str = r#"export LC_ALL=C
if [ -r /proc/stat ]; then
  echo "os linux"
  head -n 1 /proc/stat
  grep -E '^(MemTotal|MemAvailable):' /proc/meminfo
else
  echo "os darwin"
  echo "memsize $(sysctl -n hw.memsize)"
  vm_stat
  echo "iostat $(iostat -n 0 -c 2 -w 1 | tail -n 1)"
fi
"#;

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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub cpu: Cpu,
    pub mem_used: u64,
    pub mem_total: u64,
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
    let available = kib("MemAvailable:").ok_or("no MemAvailable in /proc/meminfo")?;
    Ok(Reading { cpu: Cpu::Ticks { busy: total - idle, total }, mem_used: total_mem.saturating_sub(available), mem_total: total_mem })
}

fn parse_darwin(text: &str) -> Result<Reading, String> {
    let total = text.lines().find_map(|l| l.strip_prefix("memsize ")).and_then(number).ok_or("no hw.memsize")?;
    let page = text
        .lines()
        .find_map(|l| l.split_once("page size of ").and_then(|(_, r)| number(r.split_whitespace().next()?)))
        .ok_or("no page size in vm_stat")?;
    let pages = |key: &str| text.lines().find_map(|l| l.strip_prefix(key)).and_then(number).unwrap_or(0);
    // What could be handed out without swapping: as Activity Monitor and sysinfo count it.
    let available = (pages("Pages free:") + pages("Pages inactive:") + pages("Pages speculative:") + pages("Pages purgeable:")) * page;
    // iostat: us sy id, then the three load averages; the idle share is fourth from the end.
    let fields: Vec<&str> = text.lines().find_map(|l| l.strip_prefix("iostat ")).ok_or("no iostat line")?.split_whitespace().collect();
    let idle: f32 = fields.len().checked_sub(4).and_then(|k| fields[k].parse().ok()).ok_or("cannot read iostat's idle share")?;
    Ok(Reading { cpu: Cpu::Percent((100.0 - idle).clamp(0.0, 100.0)), mem_used: total.saturating_sub(available), mem_total: total })
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
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    /// When, in milliseconds since 1970.
    pub at: i64,
    /// Percent of the whole machine; none until two readings of counters are there.
    pub cpu: Option<f32>,
    pub mem_used: u64,
    pub mem_total: u64,
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
        match read {
            Some(Ok(reading)) => {
                let cpu = cpu_share(track.before.as_ref(), &reading);
                track.before = Some(reading);
                track.samples.push_back(Sample { at: now_ms(), cpu, mem_used: reading.mem_used, mem_total: reading.mem_total });
                while track.samples.len() > KEEP {
                    track.samples.pop_front();
                }
                (track.reachable, track.error) = (true, None);
            }
            Some(Err(why)) => (track.reachable, track.error) = (false, Some(why)),
            None => (track.reachable, track.error) = (false, None),
        }
        if Self::judge(track, &thresholds, self.step) {
            let _ = self.changed.send(());
        }
    }

    /// Raises and clears a machine's alerts; says whether any changed.
    fn judge(track: &mut Track, thresholds: &Thresholds, step: Duration) -> bool {
        let samples: Vec<Sample> = track.samples.iter().copied().collect();
        let mut changed = false;
        for metric in METRICS {
            let was = track.raised.get(&metric).copied();
            let now = evaluate(&samples, metric, thresholds.of(metric), was, track.reachable, step.as_millis() as i64);
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
                    samples: t.map(|t| t.samples.iter().copied().collect()).unwrap_or_default(),
                    alerts: t.map(|t| t.raised.iter().map(|(m, since)| (serde_json::to_value(m).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default(), rfc3339(*since))).collect()).unwrap_or_default(),
                    label,
                }
            })
            .collect();
        Report { thresholds: self.thresholds(), step_seconds: self.step.as_secs().max(1), machines }
    }

    /// The machines in alert now, for Needs you.
    pub fn alerts(&self) -> Vec<Alert> {
        let thresholds = self.thresholds();
        let tracks = self.tracks.lock().unwrap();
        let mut out = vec![];
        for (label, t) in tracks.iter() {
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

    /// What the probe printed on a Mac (2026-11-16), shortened to the lines it reads and a few it skips.
    const DARWIN: &str = "os darwin
memsize 34359738368
Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                                     4387.
Pages active:                                 454876.
Pages inactive:                               450892.
Pages speculative:                              2957.
Pages throttled:                                   0.
Pages wired down:                             238149.
Pages purgeable:                                5027.
\"Translation faults\":                     1369201202.
Pages occupied by compressor:                 906526.
iostat    46 22 32  12.67 11.09 9.39
";

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
        assert_eq!(r.mem_used, 34_359_738_368 - (4387 + 450892 + 2957 + 5027) * 16384);
        assert_eq!(r.cpu, Cpu::Percent(68.0));
        assert_eq!(cpu_share(None, &r), Some(68.0), "a Mac measures its own second");
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
    }

    #[test]
    fn says_what_it_cannot_read() {
        assert!(parse("os plan9\n").unwrap_err().contains("plan9"));
        assert!(parse("os linux\nMemTotal: 1 kB\n").is_err());
        assert!(parse("os darwin\nmemsize 1\n").is_err());
    }

    const STEP_MS: i64 = 5_000;
    const CPU: Limit = Limit { on: true, percent: 90, minutes: 1 };

    /// Samples every 5 s, CPU as given, from t = 0.
    fn run(cpu: &[f32]) -> Vec<Sample> {
        cpu.iter().enumerate().map(|(k, &c)| Sample { at: k as i64 * STEP_MS, cpu: Some(c), mem_used: 1, mem_total: 2 }).collect()
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
        let full: Vec<Sample> = (0..12).map(|k| Sample { at: k * STEP_MS, cpu: None, mem_used: 95, mem_total: 100 }).collect();
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
        assert!(here.here && here.reachable && here.samples.len() == 1 && here.samples[0].cpu == Some(68.0));
        let b = get("box");
        assert_eq!(b.samples.iter().map(|s| s.cpu).collect::<Vec<_>>(), [None, Some(60.0)]);
        let g = get("gone");
        assert!(!g.reachable && g.samples.is_empty() && g.down_since.is_some(), "{g:?}");
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
            t.samples.extend((0..12).map(|k| Sample { at: now - (11 - k) * 5_000, cpu: Some(10.0), mem_used: 77, mem_total: 100 }));
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
