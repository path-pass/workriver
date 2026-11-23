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
    aging: std::sync::Arc<Aging>,
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
    let (d, aging) = aged_service(&home.join("w.db"), jobs, waits);
    wait_until("the service", || http(d.port, "GET", "/api/status", &[], None).0 == 200);
    Setup { _tmp: tmp, home, files, d, aging }
}

impl Setup {
    /// Stores a file of this name and content as the user, linked to nothing: its id.
    fn store(&self, name: &str, bytes: &[u8]) -> String {
        let at = self.files.join(name);
        std::fs::write(&at, bytes).unwrap();
        self.d.upload(&at, "", None).unwrap()["id"].as_str().unwrap().to_string()
    }

    fn stored(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(crate::common::home::attachments_dir(&self.home)).into_iter().flatten().flatten().filter(|e| e.file_type().unwrap().is_file()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
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
        self.aging.days.store(days, Ordering::SeqCst);
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

impl Setup {
    /// An active intent with a research unit and its task, `u1#1`, whose text is `text`.
    fn work(&self, text: &str) {
        self.d.append("", "intent.created", json!({ "title": "Work" })).unwrap();
        self.d.append("", "unit.created", json!({ "intent": "i1", "title": "Look", "kind": "research" })).unwrap();
        self.d.append("", "task.created", json!({ "unit": "u1", "kind": "research", "title": "Look", "text": text })).unwrap();
    }

    /// Stores a file as the user, linked to `u1#1`: what the service answers.
    fn store_for_task(&self, name: &str, bytes: &[u8]) -> Value {
        let at = self.files.join(name);
        std::fs::write(&at, bytes).unwrap();
        self.d.upload(&at, "", Some(("u1", 1, None))).unwrap()
    }

    /// `GET /api/attachments/{id}` as the user.
    fn read(&self, id: &str) -> (u16, String) {
        as_user(&self.d, "GET", &format!("/api/attachments/{id}"), None)
    }

    /// The kinds of the events stored after the last removal, in order.
    fn after_removal(&self) -> Vec<String> {
        let events = stored(&self.home.join("w.db"));
        let kinds: Vec<String> = events.iter().map(|e| serde_json::to_value(e).unwrap()["kind"].as_str().unwrap().to_string()).collect();
        let from = kinds.iter().rposition(|k| k == "attachment.removed").expect("a removal");
        kinds[from + 1..].to_vec()
    }
}

/// One file per content: the same content under another name writes nothing and is answered
/// with the file already there; a double made before that rule goes with the removal; once
/// removed, the content uploaded again is stored under its recorded name, the service's
/// restore recorded before the caller's own record, and read again.
#[test]
fn a_file_is_kept_once_under_its_recorded_name_and_stored_again_after_it_went() {
    let s = setup(vec![lifecycle::JOB]);
    let id = s.store("out.txt", b"the same log");
    std::fs::write(s.files.join("out.log"), b"the same log").unwrap();
    let again = s.d.upload(&s.files.join("out.log"), "", None).unwrap();
    assert_eq!(again["id"], json!(id));
    assert!(again["path"].as_str().unwrap().ends_with(&format!("{id}.txt")), "{again}");
    assert_eq!(s.stored(), [format!("{id}.txt")]);
    let dir = crate::common::home::attachments_dir(&s.home);
    std::fs::write(dir.join(format!("{id}.log")), b"the same log").unwrap();
    assert_eq!(s.rule(json!({ "after": "delete", "days": 1 })).0, 200);
    s.age(2);
    let (_, ran) = s.run();
    assert_eq!(ran["detail"]["files"], 1, "{ran}");
    assert!(s.stored().is_empty(), "every file of the id goes");
    let (code, why) = s.read(&id);
    assert_eq!(code, 410, "{why}");
    assert!(why.contains(&format!("attachment {id} was removed on ")) && why.ends_with("(the attachment rule)"), "{why}");
    let back = s.d.upload(&s.files.join("out.log"), "", None).unwrap();
    assert!(back["path"].as_str().unwrap().ends_with(&format!("{id}.txt")), "{back}");
    assert_eq!(s.stored(), [format!("{id}.txt")]);
    assert_eq!(s.read(&id), (200, "the same log".to_string()));
    assert_eq!(s.after_removal(), ["attachment.restored", "attachment.added"]);
}

/// An upload is refused when the records cannot be read to look its id up (and writes no
/// file), or when the restore of a file that went cannot be recorded: then nothing says
/// "stored", and the next run's sweep takes what was written.
#[test]
fn an_upload_that_cannot_look_up_or_restore_fails() {
    let s = setup(vec![lifecycle::JOB]);
    s.aging.down.store(true, Ordering::SeqCst);
    std::fs::write(s.files.join("x.txt"), b"never stored").unwrap();
    assert!(s.d.upload(&s.files.join("x.txt"), "", None).is_err());
    s.aging.down.store(false, Ordering::SeqCst);
    assert!(s.stored().is_empty(), "no file written");
    let id = s.store("y.txt", b"gone soon");
    assert_eq!(s.rule(json!({ "after": "delete", "days": 1 })).0, 200);
    s.age(2);
    s.run();
    assert!(s.stored().is_empty());
    s.aging.writes_down.store(true, Ordering::SeqCst);
    let e = s.d.upload(&s.files.join("y.txt"), "", None).unwrap_err().to_string();
    s.aging.writes_down.store(false, Ordering::SeqCst);
    assert!(e.contains("the disk is gone"), "{e}");
    assert!(s.after_removal().is_empty(), "nothing recorded");
    assert_eq!(s.run().1["ok"], true);
    assert!(s.stored().is_empty(), "the sweep took it");
    assert_eq!(s.read(&id).0, 410);
}

/// An upload of a due file's content between the run's look at what is due and its record
/// (the run holds its first read; the upload waits for the store, then links the file to
/// open work): the run takes what is due again under the upload's lock and leaves the file.
#[test]
fn an_upload_between_what_is_due_and_its_removal_is_kept() {
    let s = setup(vec![held_job()]);
    s.work("");
    let id = s.store("r.txt", b"12 passed");
    assert_eq!(s.rule(json!({ "after": "delete", "days": 1 })).0, 200);
    s.age(2);
    s.aging.pause_at.store(1, Ordering::SeqCst);
    let (ran, up) = std::thread::scope(|sc| {
        let ran = sc.spawn(|| s.run());
        wait_until("the run's first read", || s.aging.reached.load(Ordering::SeqCst));
        let up = sc.spawn(|| s.store_for_task("r.txt", b"12 passed"));
        std::thread::sleep(Duration::from_millis(300));
        s.aging.open.store(true, Ordering::SeqCst);
        (ran.join().unwrap(), up.join().unwrap())
    });
    assert_eq!((ran.0, ran.1["ok"].clone(), ran.1["words"].clone()), (200, json!(true), json!("nothing due")), "{ran:?}");
    assert_eq!(up["id"], json!(id));
    assert!(s.removals().is_empty());
    assert_eq!(s.stored(), [format!("{id}.txt")]);
    assert_eq!(s.read(&id), (200, "12 passed".to_string()));
}

/// An upload of the same content while the run records its removal (the run holds its read
/// under the upload's lock): the upload comes after, finds it gone, stores it again and
/// says so before its own record.
#[test]
fn an_upload_during_the_removal_stores_the_file_again() {
    let s = setup(vec![held_job()]);
    let id = s.store("r.txt", b"12 passed");
    assert_eq!(s.rule(json!({ "after": "delete", "days": 1 })).0, 200);
    s.age(2);
    s.aging.pause_at.store(2, Ordering::SeqCst);
    let (ran, up) = std::thread::scope(|sc| {
        let ran = sc.spawn(|| s.run());
        wait_until("the run's read under the lock", || s.aging.reached.load(Ordering::SeqCst));
        let up = sc.spawn(|| s.store("r.txt", b"12 passed"));
        std::thread::sleep(Duration::from_millis(300));
        s.aging.open.store(true, Ordering::SeqCst);
        (ran.join().unwrap(), up.join().unwrap())
    });
    assert_eq!((ran.0, ran.1["detail"]["files"].clone()), (200, json!(1)), "{ran:?}");
    assert_eq!(up, id);
    assert_eq!(s.after_removal(), ["attachment.restored", "attachment.added"]);
    assert_eq!(s.stored(), [format!("{id}.txt")]);
    assert_eq!(s.read(&id), (200, "12 passed".to_string()));
}

/// A moved file is read from where it went, checked; one changed there is not served.
#[test]
fn a_moved_file_is_served_from_its_folder_only_as_it_was() {
    let s = setup(vec![lifecycle::JOB]);
    let out = tempfile::tempdir().unwrap();
    let id = s.store("a.png", b"picture a");
    assert_eq!(s.rule(json!({ "after": "move", "days": 1, "target": folder(out.path()) })).0, 200);
    s.age(2);
    assert_eq!(s.run().1["ok"], true);
    assert!(s.stored().is_empty());
    assert_eq!(s.read(&id), (200, "picture a".to_string()));
    let copy = out.path().canonicalize().unwrap().join(format!("{id}.png"));
    std::fs::write(&copy, b"picture b").unwrap();
    let (code, why) = s.read(&id);
    assert_eq!(code, 404, "{why}");
    assert!(why.contains(&format!("attachment {id} was moved to {} on ", out.path().canonicalize().unwrap().display())) && why.ends_with("it cannot be read there now"), "{why}");
}

/// What an agent reads of files that went: a moved one by the command that fetches it (never
/// a path into its folder), a deleted one by its name and date, and a file only named in
/// the task's text by a line after it.
#[test]
fn task_show_names_gone_files_without_a_path_into_their_folder() {
    let s = setup(vec![lifecycle::JOB]);
    let out = tempfile::tempdir().unwrap();
    let named = s.store("c.png", b"picture c");
    let dir = crate::common::home::attachments_dir(&s.home);
    s.work(&format!("See {}", dir.join(format!("{named}.png")).display()));
    let moved = s.store_for_task("a.png", b"picture a")["id"].as_str().unwrap().to_string();
    let deleted = s.store_for_task("b.txt", b"a log")["id"].as_str().unwrap().to_string();
    s.d.append("", "intent.state_changed", json!({ "intent": "i1", "state": "done" })).unwrap();
    assert_eq!(s.rule(json!({ "after": "move", "days": 1, "only": "pictures", "target": folder(out.path()) })).0, 200);
    s.age(2);
    assert_eq!(s.run().1["detail"]["files"], 2);
    assert_eq!(s.rule(json!({ "after": "delete", "days": 1 })).0, 200);
    assert_eq!(s.run().1["detail"]["files"], 1);
    assert_eq!(bodies(&s.home.join("w.db"), "attachment.removed")[1]["ids"], json!([deleted]));
    // Read through the aged store: removed two days "ago".
    let today = humantime::format_rfc3339_seconds(std::time::SystemTime::now() - Duration::from_secs(2 * 86_400)).to_string()[..10].to_string();
    let out_dir = out.path().canonicalize().unwrap().display().to_string();
    let shown: crate::common::wire::TaskShow = serde_json::from_value(s.d.get("/api/task?task=u1%231").unwrap()).unwrap();
    assert!(!serde_json::to_string(&shown).unwrap().contains(&out_dir), "{shown:?}");
    assert_eq!(shown.no_longer_stored, [format!("c.png (moved: `workriver attach fetch {named}.png`)")]);
    // The task's own files, as its delivery lists them.
    s.d.append("", "task.delivered", json!({ "unit": "u1", "task": 1, "note": "Seen" })).unwrap();
    let shown: crate::common::wire::TaskShow = serde_json::from_value(s.d.get("/api/task?task=u1%231").unwrap()).unwrap();
    assert!(!serde_json::to_string(&shown).unwrap().contains(&out_dir), "{shown:?}");
    let mut files = shown.output.unwrap().files;
    files.sort();
    let mut want = vec![format!("`workriver attach fetch {moved}.png`"), format!("b.txt (removed {today})")];
    want.sort();
    assert_eq!(files, want);
}

/// `attachment.added` for a file that went, recorded by an agent here or on another machine,
/// links it and leaves it gone: nothing but the service's own restore says it is stored.
#[test]
fn an_agents_record_of_a_gone_file_leaves_it_gone() {
    use workriver_core::runtime::AgentState::*;
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let local = std::sync::Arc::new(Fake::default());
    *local.agents.lock().unwrap() = vec![("w1:p1".into(), live("s-here", "here", "w1", Idle))];
    let far = std::sync::Arc::new(Fake::default());
    *far.agents.lock().unwrap() = vec![("w1:p1".into(), live("s-far", "far", "w1", Idle))];
    let all = crate::runtime::follow::Machines::with(local, vec![("box", far)]);
    let (port, kernel, aging) = aged_over(all.clone(), Some(all.clone()), &home.join("w.db"), vec![lifecycle::JOB], Default::default());
    wait_until("the service", || http(port, "GET", "/api/status", &[], None).0 == 200);
    wait_until("the machine's listener", || all.local_port("box").is_some());
    let user = kernel.client(port, &home);
    let file = tmp.path().join("a.png");
    std::fs::write(&file, b"picture a").unwrap();
    let id = user.upload(&file, "", None).unwrap()["id"].as_str().unwrap().to_string();
    as_user(&user, "POST", "/api/settings/attachments", Some(&json!({ "rule": { "after": "delete", "days": 1 } })));
    aging.days.store(2, Ordering::SeqCst);
    assert_eq!(as_user(&user, "POST", "/api/settings/attachments/run", Some(&json!({}))).0, 200);
    let added = json!({ "id": id, "name": "a.png", "type": "image/png", "size": 9 });
    let here = kernel.daemon(port, Some("w1:p1".into()), None);
    let there = kernel.daemon(all.local_port("box").unwrap(), Some("w1:p1".into()), None);
    for agent in [here, there] {
        wait_until("the agent is known", || agent.append("", "attachment.added", added.clone()).is_ok());
        assert_eq!(as_user(&user, "GET", &format!("/api/attachments/{id}"), None).0, 410);
    }
    assert!(bodies(&home.join("w.db"), "attachment.restored").is_empty());
}
