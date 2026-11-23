// The user's settings (`settings.json` in the data directory): the machines followed, the
// alert thresholds, how long a worker may stop. Keys and defaults as they always were.
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub mod defaults;
pub mod harness;

use defaults::{IDLE_MINUTES, READY_HOURS, STALL_MAX, STALL_MINUTES};

/// A machine as the user added it. `O` is what the runtime adapter keeps for it (where its
/// socket is over there), under the adapter's own keys and with the adapter's own checks: this
/// file names no adapter and none of its keys; who puts the settings together picks `O`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Setting<O> {
    /// What ssh takes as the destination: a Host of the user's config, `user@host`.
    pub ssh: String,
    #[serde(flatten)]
    pub options: O,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Settings<O> {
    /// By the machine's name.
    #[serde(default = "BTreeMap::new")]
    pub machines: BTreeMap<String, Setting<O>>,
    /// When a machine's CPU or memory alerts: for this machine and every followed one.
    #[serde(default)]
    alerts: Thresholds,
    /// Minutes an agent may sit idle on a running task before it counts as stopped.
    #[serde(default = "stall_default", rename = "stallMinutes")]
    stall_minutes: u32,
    /// Hours a unit may stay ready to merge before its manager is asked to check it.
    #[serde(default = "ready_default", rename = "readyHours")]
    ready_hours: u32,
    /// Minutes a worker may sit idle with no task before its manager is told.
    #[serde(default = "idle_default", rename = "idleMinutes")]
    idle_minutes: u32,
    /// What a manager started from the page runs, unless the start dialog changes it. Not
    /// written while it is the default, so a file the user never set it in stays as it was.
    #[serde(default, skip_serializing_if = "StartDefault::is_default")]
    start: StartDefault,
}

/// The harness and model a start takes unless the user changes them in its dialog.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StartDefault {
    /// A kind of the harness table.
    pub harness: String,
    /// Empty: the harness's own default.
    #[serde(default)]
    pub model: String,
}

impl StartDefault {
    fn is_default(&self) -> bool {
        *self == StartDefault::default()
    }
}

impl Default for StartDefault {
    fn default() -> Self {
        StartDefault { harness: harness::HARNESSES[0].kind.to_string(), model: String::new() }
    }
}

impl<O> Default for Settings<O> {
    fn default() -> Self {
        Settings { machines: BTreeMap::new(), alerts: Thresholds::default(), stall_minutes: STALL_MINUTES, ready_hours: READY_HOURS, idle_minutes: IDLE_MINUTES, start: StartDefault::default() }
    }
}

fn ready_default() -> u32 {
    READY_HOURS
}

fn idle_default() -> u32 {
    IDLE_MINUTES
}

fn stall_default() -> u32 {
    STALL_MINUTES
}

/// Held while `settings.json` is read and written back: machines and thresholds share it.
pub static SETTINGS: Mutex<()> = Mutex::new(());

impl<O: Serialize + DeserializeOwned> Settings<O> {
    /// After how many hours a unit ready to merge and not merged has its manager asked to check
    /// it (at least one). Set by hand in settings.json; read when the service starts.
    pub fn ready_hours(home: &Path) -> u32 {
        Self::read(home).ready_hours.max(1)
    }

    /// After how many minutes idle with no task a worker's manager is told to close it (at least
    /// one). Set by hand in settings.json; read when the service starts.
    pub fn idle_minutes(home: &Path) -> u32 {
        Self::read(home).idle_minutes.max(1)
    }

    pub fn read(home: &Path) -> Self {
        std::fs::read(home.join(crate::common::home::SETTINGS)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn write(&self, home: &Path) -> Result<(), String> {
        let path = home.join(crate::common::home::SETTINGS);
        let text = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
    }

    /// The alert thresholds as the user set them (the defaults until then).
    pub fn thresholds(home: &Path) -> Thresholds {
        Self::read(home).alerts
    }

    /// Saves the alert thresholds; the machines in the same file stay as they are.
    pub fn set_thresholds(home: &Path, alerts: Thresholds) -> Result<(), String> {
        let _held = SETTINGS.lock().unwrap();
        let mut all = Self::read(home);
        all.alerts = alerts;
        all.write(home)
    }

    /// The harness and model a start takes by default.
    pub fn start_default(home: &Path) -> StartDefault {
        Self::read(home).start
    }

    /// Saves them, once the caller has checked that the harness is offered; the model's form
    /// is checked here. The rest of the file stays.
    pub fn set_start_default(home: &Path, start: StartDefault) -> Result<(), String> {
        let h = harness::harness(&start.harness).ok_or_else(|| format!("no harness {:?}", start.harness))?;
        h.args(&start.model)?;
        let _held = SETTINGS.lock().unwrap();
        let mut all = Self::read(home);
        all.start = start;
        all.write(home)
    }

    /// After how many idle minutes a running task's agent counts as stopped.
    pub fn stall_minutes(home: &Path) -> u32 {
        Self::read(home).stall_minutes
    }

    /// Saves the stall threshold (1 to `STALL_MAX` minutes); the rest of the file stays.
    pub fn set_stall_minutes(home: &Path, minutes: u32) -> Result<(), String> {
        if !(1..=STALL_MAX).contains(&minutes) {
            return Err(format!("the stall threshold is 1 to {STALL_MAX} minutes, not {minutes}"));
        }
        let _held = SETTINGS.lock().unwrap();
        let mut all = Self::read(home);
        all.stall_minutes = minutes;
        all.write(home)
    }
}

/// CPU and memory: at or above `percent` for `minutes` without a break (one level, warn).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Limit {
    pub on: bool,
    pub percent: u8,
    pub minutes: u8,
}

/// Two levels of one value: past `warn`, past `crit` (over them, or under them for a share
/// that is free), held for `minutes`; none: on the sample that sees it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Levels {
    pub on: bool,
    pub warn: f32,
    pub crit: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minutes: Option<u8>,
}

/// An alert whose levels are fixed: the pressure level (held for `minutes`), and OOM kills
/// (critical at once, held `minutes` after the last one).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Timed {
    pub on: bool,
    pub minutes: u8,
}

/// A filesystem: used over `warn` % or under `warn_free` GB free warns; the same with `crit`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiskLimit {
    pub on: bool,
    pub warn: f32,
    pub crit: f32,
    pub warn_free: f32,
    pub crit_free: f32,
    pub minutes: u8,
}

/// The thresholds, the same for every machine; each applies where the machine's system has
/// the value. A key missing from an older settings file takes its default.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Thresholds {
    pub cpu: Limit,
    pub memory: Limit,
    pub compressor: Levels,
    pub swap: Levels,
    pub pressure: Timed,
    pub system_free: Levels,
    pub available: Levels,
    pub psi: Levels,
    pub oom: Timed,
    pub disk: DiskLimit,
}

/// How long the memory and disk alerts hold before they rise, by default: a minute. One
/// sample over is never enough (a spike), and a leak can fill a Mac's compressor within
/// minutes, so longer would come late; PSI's avg10 is a 10 s mean already.
const HOLD: Option<u8> = Some(1);

impl Default for Thresholds {
    fn default() -> Self {
        let limit = Limit { on: true, percent: 90, minutes: 5 };
        let levels = |warn, crit, minutes| Levels { on: true, warn, crit, minutes };
        Thresholds {
            cpu: limit,
            memory: limit,
            compressor: levels(60.0, 80.0, None),
            swap: levels(4.0, 10.0, HOLD),
            pressure: Timed { on: true, minutes: 1 },
            system_free: levels(25.0, 20.0, HOLD),
            available: levels(25.0, 12.0, HOLD),
            psi: levels(5.0, 20.0, HOLD),
            oom: Timed { on: true, minutes: 5 },
            disk: DiskLimit { on: true, warn: 80.0, crit: 90.0, warn_free: 50.0, crit_free: 20.0, minutes: 1 },
        }
    }
}
