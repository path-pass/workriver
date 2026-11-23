// Jobs by the clock: the service's one place for work that runs by itself. A job has a name,
// says when it is due (a first delay and an interval, or the next time by the wall clock),
// runs off every request's path and off the poll, one run at a time; the user can run it
// now (its domain's own route); its last result is kept for the page and `workriver status`.
// This module names no domain: each domain exports its job as a constant and the service's
// list (`server::jobs()`) gathers them. No job is added by a request, an event, a setting or
// an agent: the list is compiled in, and `Schedule` has no `add`.
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex, TryLockError};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

use crate::server::app::App;

/// One piece of timed work: a constant of the domain it belongs to.
///
/// What a job owes (a second job is reviewed by these):
/// - it is off every request's path, so its reads follow the rule for those: through `got`;
///   a failed read ends the run as failed (`Report { ok: false, … }`), never a value in its
///   place;
/// - it acts as the service: what it records, it records as `Principal::Service`; it never
///   acts as the user and never takes an actor from anywhere;
/// - `words` and `detail` are shown by `GET /api/status`, which answers anyone on this
///   machine and anyone on every followed machine's listener, without asking who: they hold
///   no secret and no text of any record (a conversation, an instruction, a task's text, the
///   user's own words too), only counts, times and the job's own fixed words. A job that has
///   content to show the user shows it through its own route (`App::user`, `App::view`);
/// - every step of it is bounded, and between two steps it looks at `App.stopping` and ends:
///   the process's exit waits for the step a job is in (tokio's runtime, when dropped, waits
///   for a blocking task), so a job that never looks holds Ctrl-C up until a second one. It
///   is safe to be killed at any point (its order makes it so);
/// - its own locks are its own: the schedule's lock only says "one run of this job".
///
/// (Reachable from `main.rs`, which hands the service its list; built only in the service.)
#[derive(Clone, Copy)]
pub(crate) struct Job {
    /// Its key: in the list, in `run_job`, in `workriver status`.
    pub(in crate::server) name: &'static str,
    pub(in crate::server) due: Due,
    /// Blocking; called on a blocking thread.
    pub(in crate::server) run: fn(&App) -> Report,
}

/// When a job is due. The module only ever compares two `SystemTime`s: a calendar, a time
/// zone, a weekday are the job's own arithmetic.
#[derive(Clone, Copy)]
pub(in crate::server) enum Due {
    /// `first` after the service started, then `every` after its last run started (the
    /// user's or the timer's).
    // The attachment run, the service's first job, is one.
    #[cfg_attr(not(test), allow(dead_code))]
    Every { first: Duration, every: Duration },
    /// The job says: the next time it is due, by the wall clock; `None`: nothing now (asked
    /// again at the next look). A time already past runs at once. Blocking, it may read the
    /// store and its settings; `Err`: it could not tell (kept as a failure, asked again at the
    /// next look; never taken as "nothing due" or "due now").
    ///
    /// The answer must move forward: after each run the timer started, an answer not later
    /// than the time that run was for is refused as the job's error ("due at …, which it
    /// already ran for") and nothing runs. To try again, answer a later time, never the same
    /// one. So a clock set forward is followed within a look, and one set back is not: until
    /// the wall clock passes the time last run for again, a job that answers "the next 08:00
    /// after now" shows that error, and an `Every` job waits out its interval from its last
    /// run.
    // Ready for the first job that runs at a time of day (the user's timed instructions).
    #[cfg_attr(not(test), allow(dead_code))]
    At(fn(&App) -> Result<Option<SystemTime>, String>),
}

/// What a run says of itself.
#[derive(Clone, Debug, Serialize)]
pub(in crate::server) struct Report {
    pub(in crate::server) ok: bool,
    /// One English line, for `workriver status` and the log.
    pub(in crate::server) words: String,
    /// What the job's own part of the page reads; `Null` if nothing.
    pub(in crate::server) detail: serde_json::Value,
}

/// Who started a run.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::server) enum By {
    Timer,
    // A job's own "Run now" route: the attachment run's is the first.
    #[cfg_attr(not(test), allow(dead_code))]
    User,
}

/// A run as kept and as answered: `{ at, by, ms, ok, words, detail }`.
#[derive(Clone, Debug, Serialize)]
pub(in crate::server) struct Ran {
    pub(in crate::server) at: String,
    pub(in crate::server) by: By,
    pub(in crate::server) ms: u64,
    #[serde(flatten)]
    pub(in crate::server) report: Report,
}

/// Why `run_job` ran nothing.
#[derive(Debug, PartialEq)]
pub(in crate::server) enum NotRun {
    /// A run of it is on.
    Busy,
    /// No job of that name.
    NoJob,
}

/// A job as `last` shows it.
#[derive(Serialize)]
pub(in crate::server) struct JobOut {
    name: &'static str,
    /// When it is due, as last worked out; none: nothing due now, or not worked out yet.
    next: Option<String>,
    running: bool,
    /// Its last run (or the last failure to tell when it is due); none since the start.
    last: Option<Ran>,
}

/// The jobs of this service, fixed for its life.
pub(in crate::server) struct Schedule {
    /// When the service started: an interval's first delay counts from it.
    start: SystemTime,
    jobs: Vec<Kept>,
}

struct Kept {
    job: Job,
    /// One run of this job at a time, for a whole run.
    one: Mutex<()>,
    now: Mutex<Now>,
}

#[derive(Default)]
struct Now {
    /// When its last run began: an interval counts from it.
    began: Option<SystemTime>,
    next: Option<SystemTime>,
    running: bool,
    last: Option<Ran>,
}

/// How often a timer looks at the wall clock at least: a machine that slept over a job's
/// time, or a clock set forward, is followed within it.
pub(in crate::server) const LOOK: Duration = Duration::from_secs(30);

/// What a job that panicked is said to have done.
pub(in crate::server) const STOPPED: &str = "the job stopped on an internal error; see the log";

impl Schedule {
    /// The jobs of this service, fixed for its life. Two of one name: a panic at start.
    pub(in crate::server) fn of(jobs: Vec<Job>) -> Schedule {
        for (k, j) in jobs.iter().enumerate() {
            assert!(!jobs[..k].iter().any(|o| o.name == j.name), "two scheduled jobs are named {}", j.name);
        }
        Schedule { start: SystemTime::now(), jobs: jobs.into_iter().map(|job| Kept { job, one: Mutex::default(), now: Mutex::default() }).collect() }
    }

    /// Every job: `{ name, next, running, last }`. For status and a job's own `GET`.
    pub(in crate::server) fn last(&self) -> Vec<JobOut> {
        self.jobs
            .iter()
            .map(|k| {
                let now = k.now.lock().unwrap();
                JobOut { name: k.job.name, next: now.next.map(time), running: now.running, last: now.last.clone() }
            })
            .collect()
    }

    fn kept(&self, name: &str) -> Option<&Kept> {
        self.jobs.iter().find(|k| k.job.name == name)
    }
}

/// A time as the service writes times.
fn time(t: SystemTime) -> String {
    humantime::format_rfc3339_seconds(t).to_string()
}

impl App {
    /// Runs a job now, on the calling (blocking) thread: the timer's tick and the user's act
    /// both come here. `Busy`: a run of it is on (nothing is kept of this try: the run that is
    /// on writes the result). `NoJob`: no job of that name.
    pub(in crate::server) fn run_job(&self, name: &str, by: By) -> Result<Ran, NotRun> {
        let kept = self.schedule.kept(name).ok_or(NotRun::NoJob)?;
        let _one = match kept.one.try_lock() {
            Ok(g) => g,
            Err(TryLockError::WouldBlock) => return Err(NotRun::Busy),
            Err(TryLockError::Poisoned(p)) => p.into_inner(),
        };
        let began = SystemTime::now();
        {
            let mut now = kept.now.lock().unwrap();
            now.began = Some(began);
            now.running = true;
        }
        let clock = Instant::now();
        // A job that panics is a run that failed; the lock's guard is dropped the ordinary
        // way, so the next run takes it.
        let report = std::panic::catch_unwind(AssertUnwindSafe(|| (kept.job.run)(self))).unwrap_or_else(|panic| {
            let why = panic.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| panic.downcast_ref::<String>().cloned()).unwrap_or_default();
            tracing::warn!("job {name} panicked: {why}");
            Report { ok: false, words: STOPPED.into(), detail: serde_json::Value::Null }
        });
        let ran = Ran { at: time(began), by, ms: clock.elapsed().as_millis() as u64, report };
        if ran.report.ok {
            tracing::info!(job = name, by = ?by, ms = ran.ms, "{}", ran.report.words);
        } else {
            tracing::warn!(job = name, by = ?by, ms = ran.ms, "FAILED: {}", ran.report.words);
        }
        {
            let mut now = kept.now.lock().unwrap();
            now.running = false;
            now.last = Some(ran.clone());
        }
        let _ = self.changed.send(());
        Ok(ran)
    }

    /// Keeps what the timer found wrong with a job's answer as its last result, as a failed
    /// run that did not start; logged and told only when its words change.
    fn not_told(&self, kept: &Kept, words: String) {
        let mut now = kept.now.lock().unwrap();
        now.next = None;
        if now.last.as_ref().is_some_and(|r| !r.report.ok && r.report.words == words) {
            return;
        }
        tracing::warn!(job = kept.job.name, "FAILED: {words}");
        now.last = Some(Ran { at: time(SystemTime::now()), by: By::Timer, ms: 0, report: Report { ok: false, words, detail: serde_json::Value::Null } });
        drop(now);
        let _ = self.changed.send(());
    }
}

/// Starts the timers (called once, when the service starts): one small task per job, so a
/// job that runs long delays its own next run and nobody else's (not the poll, not another
/// job, not a request). Each looks at the wall clock again at least every `Waits.look`, so a
/// machine that slept over a job's time runs it within a look of waking.
pub(in crate::server) fn start(app: &Arc<App>) {
    for k in 0..app.schedule.jobs.len() {
        let app = app.clone();
        tokio::spawn(async move { timer(app, k).await });
    }
}

async fn timer(app: Arc<App>, k: usize) {
    let look = app.waits.look;
    let mut stopping = app.stopping.subscribe();
    let job = app.schedule.jobs[k].job;
    // The time the timer's last run was for: an `At` answer must be later.
    let mut ran_for: Option<SystemTime> = None;
    loop {
        if *stopping.borrow() {
            return;
        }
        // 1. When it is next due.
        let due = match job.due {
            Due::Every { first, every } => {
                let began = app.schedule.jobs[k].now.lock().unwrap().began;
                Ok(Some(began.map_or(app.schedule.start + first, |b| b + every)))
            }
            Due::At(when) => {
                let asked = app.clone();
                tokio::task::spawn_blocking(move || when(&asked)).await.unwrap_or_else(|_| Err(STOPPED.into()))
            }
        };
        let kept = &app.schedule.jobs[k];
        let wait = match due {
            Err(words) => {
                app.not_told(kept, words);
                look
            }
            Ok(Some(t)) if matches!(job.due, Due::At(_)) && ran_for.is_some_and(|r| t <= r) => {
                app.not_told(kept, format!("due at {}, which it already ran for", time(t)));
                look
            }
            Ok(None) => {
                kept.now.lock().unwrap().next = None;
                look
            }
            Ok(Some(t)) => {
                kept.now.lock().unwrap().next = Some(t);
                match t.duration_since(SystemTime::now()) {
                    // 2. Not yet: sleep until then, a look at most, and look again.
                    Ok(left) if !left.is_zero() => left.min(look),
                    // 4. Its time has come.
                    _ => {
                        // 3. Not once the service is stopping.
                        if *stopping.borrow() {
                            return;
                        }
                        let runner = app.clone();
                        match tokio::task::spawn_blocking(move || runner.run_job(job.name, By::Timer)).await {
                            Ok(Ok(_)) => {
                                ran_for = Some(t);
                                Duration::ZERO
                            }
                            // Another run of it is on (the user's): look again a while later.
                            _ => look,
                        }
                    }
                }
            }
        };
        if !wait.is_zero() {
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = stopping.wait_for(|s| *s) => return,
            }
        }
    }
}

#[cfg(test)]
mod tests;
