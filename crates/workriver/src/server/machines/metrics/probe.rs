// The probe: what is run on a machine, and its output read (pure parsers).
use super::*;

/// A probe that has not answered in this long is given up; the machine counts as unreachable.
pub(super) const TIMEOUT: Duration = Duration::from_secs(15);

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
pub(super) fn disks(text: &str) -> Vec<Disk> {
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
pub(super) fn parse_ps(text: &str) -> Vec<(u32, u32, u64, String)> {
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
pub(super) fn top_of(ps: &[(u32, u32, u64, String)], agents: &[(RuntimeAgent, Vec<u32>)]) -> Vec<Proc> {
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
