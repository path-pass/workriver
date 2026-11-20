// A machine's minute history on disk: folded, kept a week, read back.
use super::*;

/// The minute history kept on disk, per machine: this many days.
pub(super) const HISTORY_MS: i64 = 7 * 24 * 3_600_000;

/// A machine's history file is rewritten without its lines older than `HISTORY_MS` at
/// most this often (and at the first minute after the service starts).
pub(super) const TRIM_EVERY: Duration = Duration::from_secs(3600);

/// The longest look back into the history, in hours (what it keeps).
pub(super) const MAX_HOURS: u32 = 168;

pub(super) fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub(super) fn rfc3339(ms: i64) -> String {
    humantime::format_rfc3339_seconds(UNIX_EPOCH + Duration::from_millis(ms.max(0) as u64)).to_string()
}

// ---- the minute history on disk ----

/// Adds `s` to the minute under way; when it starts another minute, gives the one before as
/// its history line.
pub(super) fn next_minute(minute: &mut Vec<Sample>, s: &Sample) -> Option<Sample> {
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
pub(super) fn history_path(home: &std::path::Path, label: &str) -> PathBuf {
    let name: String = label.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' }).collect();
    home.join(crate::common::home::METRICS).join(format!("{name}.jsonl"))
}

/// Appends a minute to a machine's history (0600, in a 0700 folder); with `drop_before`,
/// first rewrites the file without the lines older than that. One rewrite of a week of
/// minutes is about 3 MB read and written, hence at most once an hour.
pub(super) fn keep_minute(home: &std::path::Path, label: &str, line: &Sample, drop_before: Option<i64>) -> std::io::Result<()> {
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

pub(super) fn parse_line(l: &str) -> Option<Sample> {
    serde_json::from_str(l).ok()
}

/// A machine's history lines from `since` (ms) on; none when it has none yet. A line that
/// cannot be read (cut off by a crash) is skipped.
pub(super) fn read_history(home: &std::path::Path, label: &str, since: i64) -> std::io::Result<Vec<Sample>> {
    match std::fs::read_to_string(history_path(home, label)) {
        Ok(text) => Ok(text.lines().filter_map(parse_line).filter(|s| s.at >= since).collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e),
    }
}
