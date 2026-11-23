// Jobs by the clock, in a running service: each test gives the service jobs of its own
// (functions that count, wait on a gate, fail or panic, each with statics of its own) and
// short waits. The user's "Run now" is a job's own route; here a job of the test's
// (`TRIGGER`) calls `App::run_job` for it, as that route does.
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::SeqCst};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Value, json};

use super::support::*;
use crate::cli::Daemon;
use crate::server::app::App;
use crate::server::schedule::{By, Due, Job, Report, STOPPED};

fn ok(words: &str) -> Report {
    Report { ok: true, words: words.into(), detail: Value::Null }
}

/// A service with these jobs, its timers looking every 20 ms; its client (the user's), its
/// data directory and the thread it runs on (which ends when the service has stopped and
/// every blocking task it started has returned).
fn serve(jobs: Vec<Job>, fake: std::sync::Arc<Fake>) -> (Daemon, tempfile::TempDir, std::thread::JoinHandle<()>) {
    let tmp = tempfile::tempdir().unwrap();
    let store = workriver_data::Store::open(&tmp.path().join("w.db")).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let d = kernel.client(listener.local_addr().unwrap().port(), tmp.path());
    let waits = crate::server::Waits { look: Duration::from_millis(20), tick: Duration::from_millis(50), ..Default::default() };
    let options = crate::server::Options { waits, jobs, allow_as: true, ..kernel.options() };
    let running = std::thread::spawn(move || {
        let _ = rt.block_on(crate::server::serve_with(listener, store, fake, options));
    });
    wait_until("the service", || http(d.port, "GET", "/api/status", &[], None).0 == 200);
    (d, tmp, running)
}

/// The job `name` as `workriver status` reads it.
fn job(d: &Daemon, name: &str) -> Value {
    let status = d.get("/api/status").unwrap();
    status["jobs"].as_array().unwrap().iter().find(|j| j["name"] == name).cloned().unwrap_or_else(|| panic!("no job {name}: {status}"))
}

/// The user's "Run now" of a job, asked of the service on `port`, and what came of it.
static ASKS: Mutex<Vec<(u16, &str, Option<String>)>> = Mutex::new(vec![]);

fn trigger(app: &App) -> Report {
    loop {
        let asked = ASKS.lock().unwrap().iter().find(|(p, _, r)| *p == app.port && r.is_none()).map(|(_, n, _)| *n);
        let Some(name) = asked else { return ok("") };
        let came = match app.run_job(name, By::User) {
            Ok(r) => format!("{:?}: {}", r.by, r.report.words),
            Err(e) => format!("{e:?}"),
        };
        if let Some(a) = ASKS.lock().unwrap().iter_mut().find(|(p, n, r)| *p == app.port && *n == name && r.is_none()) {
            a.2 = Some(came);
        }
    }
}

const TRIGGER: Job = Job { name: "trigger", due: Due::Every { first: Duration::ZERO, every: Duration::from_millis(20) }, run: trigger };

fn ask(d: &Daemon, name: &'static str) {
    ASKS.lock().unwrap().push((d.port, name, None));
}

fn answer(d: &Daemon, name: &str) -> String {
    let mut came = None;
    wait_until(&format!("the run of {name}"), || {
        came = ASKS.lock().unwrap().iter().find(|(p, n, r)| *p == d.port && *n == name && r.is_some()).and_then(|a| a.2.clone());
        came.is_some()
    });
    ASKS.lock().unwrap().retain(|(p, n, _)| !(*p == d.port && *n == name));
    came.unwrap()
}

/// Waits (up to 20 s) for a gate a test opens.
fn wait_for(gate: &AtomicBool) {
    let until = Instant::now() + Duration::from_secs(20);
    while !gate.load(SeqCst) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(5));
    }
}

static COUNTED: AtomicU32 = AtomicU32::new(0);

fn counted(_: &App) -> Report {
    COUNTED.fetch_add(1, SeqCst);
    ok("counted")
}

/// An interval job runs after its first delay, not before, then again at each interval.
#[test]
fn an_interval_job_waits_its_first_delay_then_runs_at_each_interval() {
    let before = Instant::now();
    let (d, _tmp, _) = serve(vec![Job { name: "count", due: Due::Every { first: Duration::from_millis(600), every: Duration::from_millis(100) }, run: counted }], Default::default());
    if before.elapsed() < Duration::from_millis(500) {
        assert_eq!(COUNTED.load(SeqCst), 0, "not before its first delay");
        assert!(job(&d, "count")["next"].is_string(), "{}", job(&d, "count"));
    }
    wait_until("the first run", || COUNTED.load(SeqCst) >= 1);
    assert!(before.elapsed() >= Duration::from_millis(600));
    wait_until("the runs at each interval", || COUNTED.load(SeqCst) >= 4);
    let shown = job(&d, "count");
    assert_eq!((&shown["last"]["by"], &shown["last"]["ok"], &shown["last"]["words"], &shown["running"]), (&json!("timer"), &json!(true), &json!("counted"), &json!(false)), "{shown}");
    assert!(shown["last"]["at"].is_string() && shown["last"]["ms"].is_u64() && shown["last"]["detail"].is_null(), "{shown}");
}

static AT_ANSWER: Mutex<Result<Option<SystemTime>, String>> = Mutex::new(Ok(None));
static AT_RAN: AtomicU32 = AtomicU32::new(0);

fn at_answer(_: &App) -> Result<Option<SystemTime>, String> {
    AT_ANSWER.lock().unwrap().clone()
}

fn at_ran(_: &App) -> Report {
    AT_RAN.fetch_add(1, SeqCst);
    ok("ran at its time")
}

/// A job that says when it is next due: nothing while it says none; at the time it answers
/// and not before (shown as `next`); never twice for one answer (kept as a failure); a past
/// time at once; an answer it could not give kept as a failure, running nothing; a good
/// answer after it runs.
#[test]
fn a_job_runs_at_the_time_it_answers() {
    let set = |a: Result<Option<SystemTime>, String>| *AT_ANSWER.lock().unwrap() = a;
    let (d, _tmp, _) = serve(vec![Job { name: "at", due: Due::At(at_answer), run: at_ran }], Default::default());
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(AT_RAN.load(SeqCst), 0, "nothing due");
    assert_eq!((&job(&d, "at")["next"], &job(&d, "at")["last"]), (&Value::Null, &Value::Null));

    let then = SystemTime::now() + Duration::from_millis(2000);
    set(Ok(Some(then)));
    let shown = humantime::format_rfc3339_seconds(then).to_string();
    wait_until("the time answered, shown", || job(&d, "at")["next"] == shown.as_str());
    assert_eq!(AT_RAN.load(SeqCst), 0, "not before its time");
    wait_until("the run at its time", || AT_RAN.load(SeqCst) == 1);
    assert!(SystemTime::now() >= then);

    // The same answer again: the job's error, and no second run.
    wait_until("the same answer, refused", || job(&d, "at")["last"]["words"].as_str().is_some_and(|w| w.contains("which it already ran for")));
    assert_eq!(job(&d, "at")["last"]["ok"], false);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(AT_RAN.load(SeqCst), 1);

    // A time already past, and later than the last: at once.
    set(Ok(Some(SystemTime::now() - Duration::from_millis(10))));
    wait_until("a past time", || AT_RAN.load(SeqCst) == 2);

    // It cannot tell: a failure with its words, nothing runs.
    set(Err("cannot read the records: the disk is gone".into()));
    wait_until("its words", || job(&d, "at")["last"]["words"] == "cannot read the records: the disk is gone");
    assert_eq!((&job(&d, "at")["last"]["ok"], &job(&d, "at")["next"]), (&json!(false), &Value::Null));
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(AT_RAN.load(SeqCst), 2);
    set(Ok(Some(SystemTime::now())));
    wait_until("a good answer after it", || AT_RAN.load(SeqCst) == 3);
}

static ONE_GATE: AtomicBool = AtomicBool::new(false);
static ONE_RAN: AtomicU32 = AtomicU32::new(0);
static ONE_ANSWER: Mutex<Option<SystemTime>> = Mutex::new(None);

fn one_answer(_: &App) -> Result<Option<SystemTime>, String> {
    Ok(*ONE_ANSWER.lock().unwrap())
}

fn one_gated(_: &App) -> Report {
    ONE_RAN.fetch_add(1, SeqCst);
    wait_for(&ONE_GATE);
    ok("gated run")
}

/// One run of a job at a time, both ways: the user's try during the timer's run is `Busy`
/// and nothing is kept of it; the timer's during the user's runs nothing and keeps nothing,
/// and the job runs when it is next due once that run has ended.
#[test]
fn one_run_of_a_job_at_a_time() {
    let set = |t: Option<SystemTime>| *ONE_ANSWER.lock().unwrap() = t;
    let (d, _tmp, _) = serve(vec![TRIGGER, Job { name: "gated", due: Due::At(one_answer), run: one_gated }], Default::default());

    // The timer's run is on: the user's is refused.
    set(Some(SystemTime::now()));
    wait_until("the timer's run", || ONE_RAN.load(SeqCst) == 1);
    assert_eq!((&job(&d, "gated")["running"], &job(&d, "gated")["last"]), (&json!(true), &Value::Null));
    ask(&d, "gated");
    assert_eq!(answer(&d, "gated"), "Busy");
    assert_eq!((ONE_RAN.load(SeqCst), &job(&d, "gated")["last"]), (1, &Value::Null), "nothing kept of the user's try");
    set(None);
    ONE_GATE.store(true, SeqCst);
    wait_until("the timer's result", || job(&d, "gated")["last"]["by"] == "timer");

    // The user's run is on, and the job's time comes and passes: the timer runs nothing.
    ONE_GATE.store(false, SeqCst);
    ask(&d, "gated");
    wait_until("the user's run", || ONE_RAN.load(SeqCst) == 2);
    set(Some(SystemTime::now()));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(ONE_RAN.load(SeqCst), 2, "no second run beside it");
    let shown = job(&d, "gated");
    assert_eq!((&shown["running"], &shown["last"]["by"]), (&json!(true), &json!("timer")), "nothing kept of the timer's try: {shown}");
    ONE_GATE.store(true, SeqCst);
    assert_eq!(answer(&d, "gated"), "User: gated run");
    // Then it runs when it is next due: its time has come already.
    wait_until("the timer's run after it", || ONE_RAN.load(SeqCst) == 3);
    wait_until("kept", || job(&d, "gated")["last"]["by"] == "timer" && job(&d, "gated")["running"] == false);
}

static NOW_RAN: AtomicU32 = AtomicU32::new(0);

fn now_ran(_: &App) -> Report {
    NOW_RAN.fetch_add(1, SeqCst);
    ok("ran now")
}

/// Run now: the job runs at once, said to be the user's; a name no job has runs nothing. A
/// service with no jobs shows none.
#[test]
fn a_job_runs_now_by_its_name() {
    let (d, _tmp, _) = serve(vec![TRIGGER, Job { name: "later", due: Due::Every { first: Duration::from_secs(3600), every: Duration::from_secs(3600) }, run: now_ran }], Default::default());
    ask(&d, "later");
    assert_eq!(answer(&d, "later"), "User: ran now");
    assert_eq!((NOW_RAN.load(SeqCst), &job(&d, "later")["last"]["by"]), (1, &json!("user")));
    ask(&d, "missing");
    assert_eq!(answer(&d, "missing"), "NoJob");

    let (none, _tmp, _) = serve(vec![], Default::default());
    assert_eq!(none.get("/api/status").unwrap()["jobs"], json!([]));
}

static LONG_GATE: AtomicBool = AtomicBool::new(false);
static LONG_ON: AtomicBool = AtomicBool::new(false);
static TICKS: AtomicU32 = AtomicU32::new(0);

fn long(_: &App) -> Report {
    LONG_ON.store(true, SeqCst);
    wait_for(&LONG_GATE);
    ok("long")
}

fn tick(_: &App) -> Report {
    TICKS.fetch_add(1, SeqCst);
    ok("tick")
}

/// A job that runs long holds nobody: another job goes on, a request answers, and the poll
/// still reads the runtime.
#[test]
fn a_long_job_holds_nobody() {
    use workriver_core::runtime::AgentState::Idle;
    let fake = std::sync::Arc::new(Fake::default());
    let hour = Duration::from_secs(3600);
    let jobs = vec![Job { name: "long", due: Due::Every { first: Duration::ZERO, every: hour }, run: long }, Job { name: "tick", due: Due::Every { first: Duration::ZERO, every: Duration::from_millis(20) }, run: tick }];
    let (d, _tmp, _) = serve(jobs, fake.clone());
    wait_until("the long run", || LONG_ON.load(SeqCst));
    let ticks = TICKS.load(SeqCst);
    wait_until("the other job", || TICKS.load(SeqCst) >= ticks + 5);
    assert_eq!(http(d.port, "GET", "/api/status", &[], None).0, 200);
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-new", "new", "w1", Idle))];
    wait_until("the poll", || d.get("/api/workspace").unwrap()["sessions"].as_array().is_some_and(|s| s.iter().any(|x| x["agent"] == "new")));
    assert_eq!(job(&d, "long")["running"], true);
    LONG_GATE.store(true, SeqCst);
    wait_until("its end", || job(&d, "long")["last"]["words"] == "long");
}

static FAILING: AtomicBool = AtomicBool::new(true);

fn failing(_: &App) -> Report {
    match FAILING.load(SeqCst) {
        true => Report { ok: false, words: "the folder /Volumes/x is not there; nothing was deleted".into(), detail: json!({ "files": 0 }) },
        false => Report { ok: true, words: "3 files, 0.1 MB deleted".into(), detail: json!({ "files": 3 }) },
    }
}

/// A failed run is the job's last result, in `/api/status`, and the page hears of it; the
/// next good run replaces it.
#[test]
fn a_failure_shows_until_the_next_good_run() {
    let (d, tmp, _) = serve(vec![Job { name: "fails", due: Due::Every { first: Duration::ZERO, every: Duration::from_millis(50) }, run: failing }], Default::default());
    // The page's stream of changes, as the user.
    let mut s = std::net::TcpStream::connect(("127.0.0.1", d.port)).unwrap();
    let token = crate::common::home::read_token(tmp.path()).unwrap();
    let proof = d.process(false).0.unwrap();
    use std::io::{Read, Write};
    write!(s, "GET /api/events HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {token}\r\nX-Workriver-Caller: {proof}\r\n\r\n", d.port).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut heard = String::new();
    let mut buf = [0u8; 4096];
    while !heard.contains("event: changed") {
        let n = s.read(&mut buf).expect("the page hears of a run");
        assert!(n > 0, "{heard}");
        heard += &String::from_utf8_lossy(&buf[..n]);
    }
    wait_until("the failure", || job(&d, "fails")["last"]["ok"] == false);
    let shown = job(&d, "fails");
    assert_eq!((&shown["last"]["words"], &shown["last"]["detail"]), (&json!("the folder /Volumes/x is not there; nothing was deleted"), &json!({ "files": 0 })), "{shown}");
    FAILING.store(false, SeqCst);
    wait_until("the next good run", || job(&d, "fails")["last"]["ok"] == true);
    assert_eq!(job(&d, "fails")["last"]["words"], "3 files, 0.1 MB deleted");
}

static PANICS: AtomicU32 = AtomicU32::new(0);

fn panics(_: &App) -> Report {
    if PANICS.fetch_add(1, SeqCst) == 0 {
        panic!("a job's own bug");
    }
    ok("fine again")
}

/// A job that panics: its run failed in the service's words, the service answers, and the
/// next tick runs it again.
#[test]
fn a_job_that_panics_fails_and_runs_again() {
    let (d, _tmp, _) = serve(vec![Job { name: "panics", due: Due::Every { first: Duration::ZERO, every: Duration::from_millis(300) }, run: panics }], Default::default());
    wait_until("the panic, kept", || job(&d, "panics")["last"]["words"] == STOPPED);
    assert_eq!((&job(&d, "panics")["last"]["ok"], &job(&d, "panics")["running"]), (&json!(false), &json!(false)));
    wait_until("the next run", || job(&d, "panics")["last"]["words"] == "fine again");
    assert!(PANICS.load(SeqCst) >= 2);
}

static STOP_RAN: AtomicU32 = AtomicU32::new(0);
static STOP_NOW: AtomicBool = AtomicBool::new(false);
static STOP_SEEN: AtomicBool = AtomicBool::new(false);

/// A job that runs in steps and looks at `stopping` between two; it gives the stop signal
/// itself when the test says so (the signal's handler does nothing else of note here).
fn steps(app: &App) -> Report {
    STOP_RAN.fetch_add(1, SeqCst);
    let until = Instant::now() + Duration::from_secs(20);
    while Instant::now() < until {
        if STOP_NOW.load(SeqCst) {
            let _ = app.stopping.send(true);
        }
        if *app.stopping.borrow() {
            STOP_SEEN.store(true, SeqCst);
            return ok("stopped between two steps");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    ok("all steps")
}

/// The service is stopped while a job runs: the job sees it between two steps and ends, the
/// service ends within its grace (the process's exit waits for the job's step), and no run
/// starts after the signal.
#[test]
fn the_service_ends_while_a_job_runs_and_starts_no_run_after() {
    let (_d, _tmp, running) = serve(vec![Job { name: "steps", due: Due::Every { first: Duration::ZERO, every: Duration::from_millis(20) }, run: steps }], Default::default());
    wait_until("the run", || STOP_RAN.load(SeqCst) == 1);
    let stopped = Instant::now();
    STOP_NOW.store(true, SeqCst);
    while !running.is_finished() && stopped.elapsed() < Duration::from_secs(15) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(running.is_finished(), "the service still runs after {:?}", stopped.elapsed());
    assert!(STOP_SEEN.load(SeqCst));
    assert_eq!(STOP_RAN.load(SeqCst), 1, "no run after the stop");
}
