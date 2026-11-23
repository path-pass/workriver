// The attachment rule's run: a job of the schedule (`JOB`), started by its timer or by the
// user's "Run now". It holds nothing of timing. Order is the safety: a copy is made and
// verified before anything is recorded, the record comes before a local file is deleted,
// and what a run leaves (a crash between two steps) the next run's sweep finishes. So the
// records never say "stored" where the bytes are nowhere.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::json;
use workriver_core::ports::Recorded;
use workriver_core::{Attachment, Body, Principal, Removed, State};

use crate::common::config::attachments::{After, Rule};
use crate::server::app::{App, Unread, got};
use crate::server::files::{store, target};
use crate::server::schedule::{Due, Job, Report};

/// The attachment rule's run: a minute after the service starts, then every hour.
pub(in crate::server) const JOB: Job = Job { name: "attachments", due: Due::Every { first: Duration::from_secs(60), every: Duration::from_secs(3600) }, run };

/// What a run did, for its report.
#[derive(Default)]
struct Done {
    how: Option<Removed>,
    /// Files removed from the store by the rule this run, and their bytes.
    files: u64,
    bytes: u64,
    /// Files the rule took that could not be moved, and the sweep's that could not be
    /// copied again: each with why (no text of a record: ids, paths, the system's words).
    failed: Vec<String>,
    /// Files the rule took this run.
    due: u64,
    /// Files a move took that had no bytes in the store: recorded as deleted.
    empty: u64,
    stopped: bool,
}

fn failed(words: String) -> Report {
    Report { ok: false, words, detail: serde_json::Value::Null }
}

/// One run of the rule (the job's `run`).
pub(in crate::server) fn run(app: &App) -> Report {
    let rule = match crate::runtime::Settings::attachment_rule(&app.home) {
        Ok(r) => r,
        Err(why) => return failed(why),
    };
    // A move goes only to the folder the user saved, as it is now.
    if rule.after == After::Move {
        let saved = rule.target.clone().expect("a move's rule has its target");
        match target::check(&saved, &app.home) {
            Ok(now) if now == saved => {}
            Ok(_) => return failed(format!("the folder {} is not the one saved; nothing was moved", target::shown(&saved))),
            Err(why) => return failed(format!("{why}; nothing was moved")),
        }
    }
    let mut done = Done::default();
    let state = match read(app) {
        Ok(s) => s,
        Err(why) => return failed(why),
    };
    let mut local = local_files(&app.home);
    sweep(app, &state, &mut local, &mut done);
    if rule.after != After::Keep && !done.stopped {
        take(app, &rule, &state, &mut local, &mut done);
    }
    report(done)
}

fn read(app: &App) -> Result<Recorded, String> {
    let answer = app.store.lock().unwrap().recorded();
    match got(answer) {
        Ok(Some(r)) => Ok(r),
        Ok(None) => Err("cannot read the records".into()),
        Err(Unread(why)) => Err(why),
    }
}

fn stopping(app: &App) -> bool {
    *app.stopping.borrow()
}

/// The store's files by id: every regular file named `<id>` or `<id>.<anything>` (one look
/// at the folder per run; the inbox is a folder and is passed over).
fn local_files(home: &Path) -> HashMap<String, Vec<PathBuf>> {
    let mut out: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for e in std::fs::read_dir(crate::common::home::attachments_dir(home)).into_iter().flatten().flatten() {
        if !e.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        let id = name.get(..16).unwrap_or_default();
        if workriver_core::attachment_id(id) && (name.len() == 16 || name.as_bytes()[16] == b'.') {
            out.entry(id.to_string()).or_default().push(e.path());
        }
    }
    out
}

/// The bytes of a stored file: its file of the recorded name, else any of its id's.
fn bytes_of(home: &Path, a: &Attachment, files: &[PathBuf]) -> std::io::Result<Vec<u8>> {
    let named = store::path(home, &a.id, &a.name);
    match named.filter(|p| files.contains(p)).or_else(|| files.first().cloned()) {
        Some(p) => std::fs::read(p),
        None => Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no file in the store")),
    }
}

/// Deletes a gone file's local files, under the upload's lock, once a fresh read still says
/// it is gone (an upload of the same content may have stored it again meanwhile).
fn delete_gone(app: &App, id: &str, local: &mut HashMap<String, Vec<PathBuf>>) -> Result<(), String> {
    let _held = app.attachments.lock().unwrap();
    let answer = app.store.lock().unwrap().attachment(id);
    match got(answer) {
        Ok(Some(Some(a))) if a.gone.is_some() => {}
        Ok(_) => return Ok(()),
        Err(Unread(why)) => return Err(why),
    }
    for f in local.remove(id).unwrap_or_default() {
        std::fs::remove_file(&f).map_err(|e| format!("cannot delete {}: {e}", f.display()))?;
    }
    Ok(())
}

/// What an earlier run left: the local files of attachments the records say are gone. A
/// moved one's copy in its target is checked first; if it is missing or wrong, the local
/// file is the only good one: copied again, and deleted only when that copy passes.
fn sweep(app: &App, state: &State, local: &mut HashMap<String, Vec<PathBuf>>, done: &mut Done) {
    let left: Vec<&Attachment> = state.attachments.iter().filter(|a| a.gone.is_some() && local.contains_key(&a.id)).collect();
    for a in left {
        if stopping(app) {
            done.stopped = true;
            return;
        }
        let gone = a.gone.as_ref().expect("filtered");
        if let (Removed::Moved, Some(to)) = (gone.how, &gone.to)
            && target::get(to, &a.id, &a.name, a.size).is_err()
        {
            let again = bytes_of(&app.home, a, &local[&a.id]).map_err(|e| e.to_string()).and_then(|b| target::put(to, &a.id, &a.name, &b));
            if let Err(why) = again {
                done.failed.push(format!("{}: kept here, its copy is not in the folder ({why})", a.id));
                continue;
            }
        }
        if let Err(why) = delete_gone(app, &a.id, local) {
            done.failed.push(format!("{}: {why}", a.id));
        }
    }
}

/// The rule's own step: what is due, copied first when moving, then recorded as one removal
/// under the upload's lock (what is due is taken again there: a file stored or named
/// meanwhile is left), then deleted here.
fn take(app: &App, rule: &Rule, state: &State, local: &mut HashMap<String, Vec<PathBuf>>, done: &mut Done) {
    let conditions = rule.conditions();
    let due: Vec<&Attachment> = state.attachments_due(&conditions, SystemTime::now());
    done.due = due.len() as u64;
    let how = if rule.after == After::Move { Removed::Moved } else { Removed::Deleted };
    let to = (how == Removed::Moved).then(|| rule.target.clone()).flatten();
    let mut ready: Vec<&Attachment> = vec![];
    // A move's files that have no bytes in the store (recorded with no upload, deleted by
    // hand): there is nothing to move, so they are recorded as a delete rule records them.
    let mut empty: Vec<&Attachment> = vec![];
    for a in due {
        if stopping(app) {
            done.stopped = true;
            break;
        }
        if to.is_some() && local.get(&a.id).is_none_or(Vec::is_empty) {
            empty.push(a);
            continue;
        }
        if let Some(to) = &to {
            let copied = bytes_of(&app.home, a, local.get(&a.id).map_or(&[], Vec::as_slice)).map_err(|e| e.to_string()).and_then(|b| target::put(to, &a.id, &a.name, &b));
            if let Err(why) = copied {
                done.failed.push(format!("{}: {why}", a.id));
                continue;
            }
        }
        ready.push(a);
    }
    if ready.is_empty() && empty.is_empty() {
        return;
    }
    let _held = app.attachments.lock().unwrap();
    let now = match read(app) {
        Ok(s) => s,
        Err(why) => {
            done.failed.push(why);
            return;
        }
    };
    let still: Vec<&str> = now.attachments_due(&conditions, SystemTime::now()).iter().map(|a| a.id.as_str()).collect();
    let still = |list: &[&Attachment]| -> Vec<String> { list.iter().filter(|a| still.contains(&a.id.as_str())).map(|a| a.id.clone()).collect() };
    // What has no bytes is taken only while it still has none (an upload under this lock
    // has stored it again, and named it, so it is not due any more anyway).
    let gone_empty: Vec<String> = still(&empty).into_iter().filter(|id| local.get(id).is_none_or(Vec::is_empty)).collect();
    if !gone_empty.is_empty() {
        let body = Body::AttachmentRemoved { ids: gone_empty.clone(), how: Removed::Deleted, to: None, rule: conditions };
        match app.record(&Principal::Service, body) {
            Ok(_) => done.empty = gone_empty.len() as u64,
            Err(e) => done.failed.push(format!("the removal could not be recorded: {}", e.1)),
        }
    }
    let ids = still(&ready);
    if ids.is_empty() {
        return;
    }
    let body = Body::AttachmentRemoved { ids: ids.clone(), how, to, rule: conditions };
    if let Err(e) = app.record(&Principal::Service, body) {
        done.failed.push(format!("the removal could not be recorded: {}", e.1));
        return;
    }
    done.how = Some(how);
    for a in ready.iter().filter(|a| ids.contains(&a.id)) {
        done.files += 1;
        done.bytes += a.size;
        for f in local.remove(&a.id).unwrap_or_default() {
            if let Err(e) = std::fs::remove_file(&f) {
                done.failed.push(format!("{}: cannot delete {}: {e}", a.id, f.display()));
            }
        }
    }
}

fn report(done: Done) -> Report {
    let how = match done.how {
        Some(Removed::Moved) => "moved",
        Some(Removed::Deleted) => "deleted",
        None => "",
    };
    let mb = done.bytes as f64 / 1_048_576.0;
    let mut words = match (done.files, done.failed.len()) {
        (0, 0) if done.empty > 0 => String::new(),
        (0, 0) => "nothing due".to_string(),
        (n, 0) => format!("{n} files, {mb:.1} MB {how}"),
        (0, f) => format!("{f} failed: {}", done.failed.join("; ")),
        (n, f) => format!("{n} of {} {}; {f} failed: {}", (done.due - done.empty).max(n + f as u64), if how.is_empty() { "removed" } else { how }, done.failed.join("; ")),
    };
    if done.empty > 0 {
        let empty = format!("{} with no file in the store, recorded as removed", done.empty);
        words = if words.is_empty() { empty } else { format!("{words}; {empty}") };
    }
    if done.stopped {
        words += "; stopped with the service, the rest at the next run";
    }
    let detail = json!({ "how": done.how, "files": done.files, "bytes": done.bytes, "failed": done.failed.len() });
    Report { ok: done.failed.is_empty(), words, detail }
}
