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

use tokio::sync::broadcast;
use workriver_core::runtime::{Runtime, RuntimeAgent};
use workriver_scan as scan;

use crate::common::config::{DiskLimit, Levels, Limit, Thresholds};
use crate::common::wire::{Alert, Detail, Disk, Level, MachineMetrics, Metric, Os, Pressure, Proc, Raise, Report, Sample};
use crate::runtime::follow::Machines;

mod alerts;
mod history;
mod probe;

pub use alerts::*;
pub use history::*;
pub use probe::*;

/// The name of the machine workriver runs on (a followed machine may not take it).
pub const HERE: &str = "local";

/// How often each machine is read.
pub const STEP: Duration = Duration::from_secs(5);

/// How many samples are kept per machine: ten minutes at `STEP`, the longest window.
pub(super) const KEEP: usize = 120;

#[derive(Default)]
pub(super) struct Track {
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
pub(super) struct Raised {
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
    pub(super) fn wanted(&self) -> Vec<String> {
        std::iter::once(HERE.to_string()).chain(self.machines.iter().flat_map(|m| m.states()).map(|s| s.label)).collect()
    }

    /// One machine's loop, until it is no longer followed.
    pub(super) fn follow(self: Arc<Self>, label: &str) {
        while !self.stop.load(Ordering::Relaxed) && self.wanted().iter().any(|l| l == label) {
            let started = Instant::now();
            self.sample(label);
            std::thread::sleep(self.step.saturating_sub(started.elapsed()));
        }
        self.tracks.lock().unwrap().remove(label);
        self.running.lock().unwrap().remove(label);
    }

    /// Where the probe runs for a machine: None while it cannot be reached.
    pub(super) fn runner(&self, label: &str) -> Option<scan::Runner> {
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
    pub(super) fn top(&self, label: &str) -> Vec<Proc> {
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
    pub(super) fn judge(track: &mut Track, t: &Thresholds, step: Duration) -> bool {
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
mod tests;
