// When a metric is in alert: each sample's level against the thresholds, and the windows.
use super::*;

/// A threshold's window may be 1 to this many minutes (what the samples kept cover).
pub const MAX_MINUTES: u8 = 10;

/// An alert clears once every sample of this long is below its threshold.
pub(super) const CLEAR_MS: i64 = 60_000;

/// One GB as the page counts it (steps of 1024).
pub(super) const GIB: f32 = 1_073_741_824.0;

impl Disk {
    /// The share of it in use, in percent.
    pub(super) fn used(&self) -> f32 {
        (self.total - self.free.min(self.total)) as f32 * 100.0 / self.total as f32
    }

    pub(super) fn level(&self, d: &DiskLimit) -> Option<Level> {
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

pub(super) fn above(v: f32, l: &Levels) -> Option<Level> {
    if v > l.crit { Some(Level::Crit) } else if v > l.warn { Some(Level::Warn) } else { None }
}

pub(super) fn below(v: f32, l: &Levels) -> Option<Level> {
    if v < l.crit { Some(Level::Crit) } else if v < l.warn { Some(Level::Warn) } else { None }
}

impl Sample {
    /// The filesystem nearest its limits: the highest level, then the fullest. One of no
    /// size (an automount map) is no filesystem to judge.
    pub(super) fn worst_disk(&self, d: &DiskLimit) -> Option<&Disk> {
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

pub(super) const METRICS: [Metric; 10] = [Metric::Cpu, Metric::Memory, Metric::Compressor, Metric::Swap, Metric::Pressure, Metric::SystemFree, Metric::Available, Metric::Psi, Metric::Oom, Metric::Disk];

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
    pub(super) fn threshold(&self, metric: Metric, level: Level) -> (Option<f32>, Option<f32>) {
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
