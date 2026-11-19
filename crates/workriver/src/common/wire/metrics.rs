// What the service reports of a machine's load: the samples, the alerts and the report the
// page and `workriver machine load` read.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::common::config::Thresholds;

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

/// How bad an alert is.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Warn,
    Crit,
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
