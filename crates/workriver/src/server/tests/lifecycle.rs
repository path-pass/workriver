// The attachment rule as the service runs it: what a run removes or moves, what it leaves,
// how it fails, and the user's routes. The days a rule waits are seen through a store whose
// reads are aged (`Aged`): nothing in the service is told the time is other than it is.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use serde_json::{Value, json};

use super::support::*;
use crate::cli::Daemon;
use crate::server::files::lifecycle;
use crate::server::schedule::{Due, Job};

struct Setup {
    _tmp: tempfile::TempDir,
    home: PathBuf,
    files: PathBuf,
    d: Daemon,
    days: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

fn setup(jobs: Vec<Job>) -> Setup {
    setup_with(jobs, Default::default())
}

fn setup_with(jobs: Vec<Job>, waits: crate::server::Waits) -> Setup {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let files = tmp.path().join("files");
    std::fs::create_dir(&home).unwrap();
    std::fs::create_dir(&files).unwrap();
    let (d, days) = aged_service(&home.join("w.db"), jobs, waits);
    wait_until("the service", || http(d.port, "GET", "/api/status", &[], None).0 == 200);
    Setup { _tmp: tmp, home, files, d, days }
}

impl Setup {
    /// Stores a file of this name and content as the user, linked to nothing: its id.
    fn store(&self, name: &str, bytes: &[u8]) -> String {
        let at = self.files.join(name);
        std::fs::write(&at, bytes).unwrap();
        self.d.upload(&at, "", None).unwrap()["id"].as_str().unwrap().to_string()
    }

    fn stored(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(crate::common::home::attachments_dir(&self.home)).unwrap().flatten().filter(|e| e.file_type().unwrap().is_file()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    fn rule(&self, rule: Value) -> (u16, Value) {
        let (code, body) = as_user(&self.d, "POST", "/api/settings/attachments", Some(&json!({ "rule": rule })));
        (code, serde_json::from_str(&body).unwrap_or(Value::String(body)))
    }

    fn run(&self) -> (u16, Value) {
        let (code, body) = as_user(&self.d, "POST", "/api/settings/attachments/run", Some(&json!({})));
        (code, serde_json::from_str(&body).unwrap_or(Value::String(body)))
    }

    fn removals(&self) -> Vec<Value> {
        bodies(&self.home.join("w.db"), "attachment.removed")
    }

    fn age(&self, days: u64) {
        self.days.store(days, Ordering::SeqCst);
    }
}

fn folder(path: &Path) -> Value {
    json!({ "kind": "folder", "path": path })
}

#[test]
fn the_service_runs_the_attachment_rule_hourly() {
    let jobs = crate::server::jobs();
    let ours: Vec<&Job> = jobs.iter().filter(|j| j.name == "attachments").collect();
    assert_eq!(ours.len(), 1);
    match ours[0].due {
        Due::Every { first, every } => assert_eq!((first, every), (Duration::from_secs(60), Duration::from_secs(3600))),
        _ => panic!("the attachment run is due by an interval"),
    }
}

#[test]
fn a_run_removes_exactly_the_due_files_once() {
    let s = setup(vec![lifecycle::JOB]);
    let a = s.store("a.png", b"picture a");
    let b = s.store("b.png", b"picture b");
    let c = s.store("c.txt", b"a report");
    // Named in the note of an intent that is still active: held, however old.
    let held = s.store("d.png", b"picture d");
    s.d.append("", "intent.created", json!({ "title": "Work", "note": format!("see attachments/{held}.png") })).unwrap();
    assert_eq!(s.rule(json!({ "after": "delete", "days": 30, "only": "pictures" })).0, 200);
    // Not old enough yet.
    let (code, ran) = s.run();
    assert_eq!((code, ran["ok"].clone(), ran["by"].clone()), (200, json!(true), json!("user")), "{ran}");
    assert_eq!(ran["words"], "nothing due");
    assert!(s.removals().is_empty());
    s.age(31);
    let (_, ran) = s.run();
    assert_eq!(ran["ok"], true, "{ran}");
    assert_eq!(ran["detail"]["how"], "deleted");
    assert_eq!(ran["detail"]["files"], 2);
    assert_eq!(ran["detail"]["bytes"], 18);
    assert_eq!(ran["words"], "2 files, 0.0 MB deleted");
    let mut gone = vec![a.clone(), b.clone()];
    gone.sort();
    let removals = s.removals();
    assert_eq!(removals.len(), 1, "one event for the batch");
    assert_eq!(removals[0]["how"], "deleted");
    assert_eq!(removals[0]["rule"], json!({ "days": 30, "only": "pictures", "minMb": 0 }));
    let mut ids: Vec<String> = serde_json::from_value(removals[0]["ids"].clone()).unwrap();
    ids.sort();
    assert_eq!(ids, gone);
    let mut kept = vec![format!("{c}.txt"), format!("{held}.png")];
    kept.sort();
    assert_eq!(s.stored(), kept);
    // Nothing more is due: a second run records nothing.
    let (_, ran) = s.run();
    assert_eq!((ran["ok"].clone(), ran["words"].clone()), (json!(true), json!("nothing due")), "{ran}");
    assert_eq!(s.removals().len(), 1);
}

#[test]
fn a_move_copies_verified_files_and_a_folder_not_there_stops_the_run() {
    let s = setup(vec![lifecycle::JOB]);
    let out = tempfile::tempdir().unwrap();
    let a = s.store("a.png", b"picture a");
    let t = s.store("t.txt", b"text");
    // The folder is saved canonical: through a link, the folder it leads to.
    let links = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(out.path(), links.path().join("out")).unwrap();
    let (code, saved) = s.rule(json!({ "after": "move", "days": 1, "target": folder(&links.path().join("out")) }));
    assert_eq!(code, 200, "{saved}");
    let canonical = out.path().canonicalize().unwrap();
    assert_eq!(saved["rule"]["target"], folder(&canonical));
    s.age(2);
    let (_, ran) = s.run();
    assert_eq!((ran["ok"].clone(), ran["detail"]["how"].clone(), ran["detail"]["files"].clone()), (json!(true), json!("moved"), json!(2)), "{ran}");
    assert_eq!(std::fs::read(canonical.join(format!("{a}.png"))).unwrap(), b"picture a");
    assert_eq!(std::fs::read(canonical.join(format!("{t}.txt"))).unwrap(), b"text");
    assert!(s.stored().is_empty());
    assert_eq!(s.removals()[0]["to"], folder(&canonical));

    // The folder renamed away (a drive not mounted): the run fails, nothing goes.
    s.age(0);
    let b = s.store("b.png", b"picture b");
    s.age(2);
    std::fs::rename(&canonical, canonical.with_extension("away")).unwrap();
    let (_, ran) = s.run();
    assert_eq!(ran["ok"], false, "{ran}");
    assert!(ran["words"].as_str().unwrap().contains("is not there"), "{ran}");
    assert!(ran["words"].as_str().unwrap().contains("nothing was moved"), "{ran}");
    assert_eq!(s.stored(), [format!("{b}.png")]);
    assert_eq!(s.removals().len(), 1);
}

#[test]
fn a_file_that_cannot_be_copied_stays_and_the_run_says_so() {
    let s = setup(vec![lifecycle::JOB]);
    let out = tempfile::tempdir().unwrap();
    let a = s.store("a.png", b"picture a");
    let b = s.store("b.png", b"picture b");
    assert_eq!(s.rule(json!({ "after": "move", "days": 1, "target": folder(out.path()) })).0, 200);
    // Something that is not a file holds b's name there: its copy cannot be put in place.
    std::fs::create_dir(out.path().join(format!("{b}.png"))).unwrap();
    std::fs::write(out.path().join(format!("{b}.png")).join("x"), b"x").unwrap();
    s.age(2);
    let (_, ran) = s.run();
    assert_eq!(ran["ok"], false, "{ran}");
    assert_eq!(ran["detail"]["failed"], 1);
    assert_eq!(ran["detail"]["files"], 1);
    assert!(ran["words"].as_str().unwrap().starts_with("1 of 2 moved; 1 failed: "), "{ran}");
    assert_eq!(s.stored(), [format!("{b}.png")], "b stays");
    let ids: Vec<String> = serde_json::from_value(s.removals()[0]["ids"].clone()).unwrap();
    assert_eq!(ids, [a]);
}

#[test]
fn the_sweep_finishes_what_a_run_left() {
    let s = setup(vec![lifecycle::JOB]);
    let out = tempfile::tempdir().unwrap();
    let a = s.store("a.png", b"picture a");
    assert_eq!(s.rule(json!({ "after": "delete", "days": 1 })).0, 200);
    s.age(2);
    assert_eq!(s.run().1["ok"], true);
    // A run that ended after its record (a crash) left a's file, and a double of it made
    // before one file per id.
    let dir = crate::common::home::attachments_dir(&s.home);
    std::fs::write(dir.join(format!("{a}.png")), b"picture a").unwrap();
    std::fs::write(dir.join(format!("{a}.log")), b"picture a").unwrap();
    std::fs::create_dir_all(dir.join("inbox").join("u1-1")).unwrap();
    let (_, ran) = s.run();
    assert_eq!((ran["ok"].clone(), ran["words"].clone()), (json!(true), json!("nothing due")), "{ran}");
    assert!(s.stored().is_empty(), "{:?}", s.stored());
    assert!(dir.join("inbox").join("u1-1").is_dir(), "the inbox is not the sweep's");

    // A moved file whose copy is missing in the folder: the leftover is copied again, then
    // deleted.
    s.age(0);
    let m = s.store("m.png", b"picture m");
    assert_eq!(s.rule(json!({ "after": "move", "days": 1, "target": folder(out.path()) })).0, 200);
    s.age(2);
    assert_eq!(s.run().1["ok"], true);
    let copy = out.path().canonicalize().unwrap().join(format!("{m}.png"));
    std::fs::remove_file(&copy).unwrap();
    std::fs::write(dir.join(format!("{m}.png")), b"picture m").unwrap();
    assert_eq!(s.run().1["ok"], true);
    assert_eq!(std::fs::read(&copy).unwrap(), b"picture m");
    assert!(s.stored().is_empty());
    // A leftover whose copy cannot be made again stays, and the run says so.
    std::fs::remove_file(&copy).unwrap();
    std::fs::create_dir(&copy).unwrap();
    std::fs::write(copy.join("x"), b"x").unwrap();
    std::fs::write(dir.join(format!("{m}.png")), b"picture m").unwrap();
    let (_, ran) = s.run();
    assert_eq!(ran["ok"], false, "{ran}");
    assert_eq!(s.stored(), [format!("{m}.png")]);
}

#[test]
fn a_rule_out_of_range_runs_nothing_and_the_tab_says_why() {
    let s = setup(vec![lifecycle::JOB]);
    let a = s.store("a.png", b"picture a");
    std::fs::write(s.home.join(crate::common::home::SETTINGS), r#"{ "attachments": { "after": "delete", "days": 0 } }"#).unwrap();
    s.age(40);
    let (_, ran) = s.run();
    assert_eq!(ran["ok"], false, "{ran}");
    assert!(ran["words"].as_str().unwrap().starts_with("days is 0"), "{ran}");
    assert_eq!(s.stored(), [format!("{a}.png")]);
    let tab = s.d.get("/api/settings/attachments").unwrap();
    assert!(tab["error"].as_str().unwrap().starts_with("days is 0"), "{tab}");
    assert_eq!(tab["rule"], Value::Null);
}

#[test]
fn the_settings_tab_reads_checks_and_saves_the_rule() {
    let s = setup(vec![lifecycle::JOB]);
    let tab = s.d.get("/api/settings/attachments").unwrap();
    assert_eq!(tab["rule"]["after"], "keep", "{tab}");
    assert_eq!(tab["lastRun"], Value::Null);
    assert_eq!(tab["running"], false);
    assert_eq!(tab["usage"], json!({ "files": 0, "bytes": 0 }));
    s.store("a.png", b"picture a");
    s.age(2);
    // A dry run counts, and saves nothing.
    let (code, dry) = as_user(&s.d, "POST", "/api/settings/attachments", Some(&json!({ "rule": { "after": "delete", "days": 1 }, "dryRun": true })));
    assert_eq!(code, 200, "{dry}");
    let dry: Value = serde_json::from_str(&dry).unwrap();
    assert_eq!(dry["due"], json!({ "files": 1, "bytes": 9 }));
    assert_eq!(s.d.get("/api/settings/attachments").unwrap()["rule"]["after"], "keep");
    // Refused in words: a range, a folder that will not do.
    let (code, why) = s.rule(json!({ "after": "delete", "days": 0 }));
    assert_eq!(code, 422);
    assert!(why.as_str().unwrap().contains("days is 0"), "{why}");
    let (code, why) = s.rule(json!({ "after": "move", "days": 1, "target": { "kind": "folder", "path": "relative" } }));
    assert_eq!(code, 422);
    assert!(why.as_str().unwrap().contains("absolute"), "{why}");
    let (code, why) = s.rule(json!({ "after": "move", "days": 1, "target": folder(&s.home) }));
    assert_eq!(code, 422);
    assert!(why.as_str().unwrap().contains("data directory"), "{why}");
    let (code, saved) = s.rule(json!({ "after": "delete", "days": 1 }));
    assert_eq!((code, saved["due"].clone()), (200, json!({ "files": 1, "bytes": 9 })), "{saved}");
    assert_eq!(s.d.get("/api/settings/attachments").unwrap()["rule"]["after"], "delete");
    // After a run, its result.
    s.run();
    let tab = s.d.get("/api/settings/attachments").unwrap();
    assert_eq!((tab["lastRun"]["by"].clone(), tab["lastRun"]["ok"].clone()), (json!("user"), json!(true)), "{tab}");
    assert_eq!(tab["lastRun"]["detail"]["files"], 1);
}

static GATE: AtomicBool = AtomicBool::new(false);
static RUNS: AtomicU32 = AtomicU32::new(0);

/// The attachment run, held at its start until the test opens the gate.
fn gated(app: &crate::server::app::App) -> crate::server::schedule::Report {
    RUNS.fetch_add(1, Ordering::SeqCst);
    while !GATE.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(5));
    }
    (lifecycle::JOB.run)(app)
}

#[test]
fn run_now_during_a_run_is_refused_and_the_timer_keeps_nothing() {
    // Its timer looks often and its interval is short: during the user's run it comes due.
    let job = Job { run: gated, due: Due::Every { first: Duration::from_secs(3600), every: Duration::from_millis(30) }, ..lifecycle::JOB };
    let s = setup_with(vec![job], crate::server::Waits { look: Duration::from_millis(20), ..Default::default() });
    let d = s.d.clone();
    let first = std::thread::spawn(move || as_user(&d, "POST", "/api/settings/attachments/run", Some(&json!({}))));
    wait_until("the first run", || RUNS.load(Ordering::SeqCst) == 1);
    let (code, why) = s.run();
    assert_eq!(code, 409, "{why}");
    assert!(why.as_str().unwrap().contains("a run is in progress"), "{why}");
    let tab = s.d.get("/api/settings/attachments").unwrap();
    assert_eq!((tab["running"].clone(), tab["lastRun"].clone()), (json!(true), Value::Null), "{tab}");
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(RUNS.load(Ordering::SeqCst), 1, "no second run beside it");
    GATE.store(true, Ordering::SeqCst);
    let (code, ran) = first.join().unwrap();
    assert_eq!(code, 200, "{ran}");
    let ran: Value = serde_json::from_str(&ran).unwrap();
    assert_eq!(ran["by"], "user");
}
