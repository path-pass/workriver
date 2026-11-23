// An agent's terminal from the page (server/terminals/): who may read and press, that a press
// goes only to the screen the user saw, at one place, one writer at a time, never text and
// never twice by itself; what is kept of it; a dialog the runtime missed; the wake-ups; the
// terminal view's stream and its bounds.
use std::io::{Read, Write};
use std::time::Duration;

use serde_json::{Value, json};
use workriver_core::runtime::AgentState::{self, *};
use workriver_core::screens::Key;

use super::support::*;
use crate::cli::Daemon;

/// A harness's own rule: from column 0, across the screen (wider than any line drawn here).
const RULE: &str = "────────────────────────────────────────────────────────────────────────────────";

/// A numbered dialog, as Claude draws one (the command above it, a long option).
fn bash(first: &str) -> String {
    format!("{first}\n{RULE}\n Bash command\n touch a.txt\n Do you want to proceed?\n ❯ 1. Yes\n   2. Yes, and don't ask again for `curl -H x` commands in /p\n   3. No\n\n Esc to cancel · Tab to amend\n")
}

/// A marked list of three with the marker on `at` (0 to 2).
fn list(at: usize) -> String {
    let words = ["Keep it", "Throw it away", "Ask me later"];
    let rows: Vec<String> = words.iter().enumerate().map(|(k, w)| if k == at { format!(" ❯ {w}") } else { format!("   {w}") }).collect();
    format!("{RULE}\n What now?\n\n{}\n\n Enter to confirm · Esc to cancel\n", rows.join("\n"))
}

/// An idle screen that quotes a dialog in its transcript, above its input box.
fn quoting() -> String {
    format!("⏺ It asked:\n   Do you want to proceed?\n   ❯ 1. Yes\n     2. No\n   Esc to cancel\n\n{RULE}\n❯ \n{RULE}\n  ? for shortcuts\n")
}

struct T {
    _tmp: tempfile::TempDir,
    db: std::path::PathBuf,
    fake: std::sync::Arc<Fake>,
    d: Daemon,
    kernel: Kernel,
    /// The machines followed, read again by the test (`poll_all`); none: this one alone.
    machines: Option<std::sync::Arc<crate::runtime::follow::Machines>>,
}

/// A service whose agents are `boss` (manager, idle, p1) and `alice` (blocked on a dialog, p2,
/// on task u1#1), with this tick and wait before the change stream is opened again.
fn service(tick: Duration, rewatch: Duration) -> T {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-a", "alice", "w1", Blocked))];
    fake.shown.lock().unwrap().insert("p2".into(), bash("⏺ Bash(touch a.txt)"));
    let db = tmp.path().join("w.db");
    let waits = crate::server::Waits { tick, rewatch, ..Default::default() };
    let (d, kernel) = daemon_and(fake.clone(), &db, waits);
    manage(&d, "p1", "s-boss");
    d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "M", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "implement", "title": "Do it", "agent": "s-a" })).unwrap();
    d.append("s-a", "task.started", json!({ "unit": "u1", "task": 1 })).unwrap();
    let t = T { _tmp: tmp, db, fake, d, kernel, machines: None };
    wait_until("the poll sees alice waiting", || waits_on(&t, "s-a").is_some());
    t
}

/// Since when the page says the agent waits, and whether only its screen says so.
fn waits_on(t: &T, session: &str) -> Option<(String, bool)> {
    let ws = t.d.get("/api/workspace").unwrap();
    let s = ws["sessions"].as_array().unwrap().iter().find(|s| s["session"] == session)?.clone();
    Some((s["blockedSince"].as_str()?.to_string(), s["onScreen"] == true))
}

/// One request as the user's CLI makes it (the token and its process's proof), with `extra`
/// headers: the status and the JSON answer.
fn ask(t: &T, method: &str, path: &str, body: Option<Value>, extra: &[(&str, &str)]) -> (u16, Value) {
    let token = format!("Bearer {}", t.d.token().unwrap());
    let (proof, _) = t.d.process(false);
    let proof = proof.unwrap();
    let mut headers = vec![("authorization", token.as_str()), ("x-workriver-caller", proof.as_str())];
    headers.extend_from_slice(extra);
    let body = body.map(|b| b.to_string());
    let (code, text) = http(t.d.port, method, path, &headers, body.as_deref());
    (code, serde_json::from_str(text.split("\r\n\r\n").nth(1).unwrap_or_default()).unwrap_or(Value::Null))
}

fn screen(t: &T, agent: &str) -> (u16, Value) {
    ask(t, "GET", &format!("/api/agent/screen?agent={agent}"), None, &[])
}

fn press(t: &T, agent: &str, shown: &Value, what: Value) -> (u16, Value) {
    ask(t, "POST", "/api/agent/keys", Some(json!({ "agent": agent, "shown": shown, "press": what })), &[])
}

fn pressed(t: &T) -> Vec<(String, Vec<Key>)> {
    t.fake.pressed.lock().unwrap().clone()
}

/// What a press at `place` brings, in order.
fn then(t: &T, place: &str, next: &[(String, Option<AgentState>)]) {
    t.fake.then.lock().unwrap().insert(place.into(), next.iter().cloned().collect());
}

/// The page's session cookie, made with a one-time code as the page does.
fn cookie(t: &T) -> String {
    let ours = format!("http://127.0.0.1:{}", t.d.port);
    let url = t.kernel.client(t.d.port, t.db.parent().unwrap()).call("/api/session/code", Some(json!({}))).unwrap()["url"].as_str().unwrap().to_string();
    let code = url.split_once("#k=").unwrap().1.to_string();
    let (_, answer) = http(t.d.port, "POST", "/api/session", &[("Origin", &ours)], Some(&json!({ "code": code }).to_string()));
    let set = answer.lines().find(|l| l.to_lowercase().starts_with("set-cookie:")).unwrap().to_string();
    set.split_once(':').unwrap().1.split(';').next().unwrap().trim().to_string()
}

/// Test 1. The user's alone, from the user's own page or CLI: an agent's proof is refused,
/// and so is a caller with none; a cookie request to the two GETs must come from our page.
#[test]
fn only_the_user_reads_and_presses() {
    let t = service(Duration::from_secs(2), Duration::from_secs(5));
    let (code, s) = screen(&t, "s-a");
    assert_eq!(code, 200, "{s}");
    let keys = json!({ "agent": "s-a", "shown": s["digest"], "press": { "key": "escape" } }).to_string();
    for (method, path, body) in [("GET", "/api/agent/screen?agent=s-a", None), ("GET", "/api/agent/view?agent=s-a", None), ("POST", "/api/agent/keys", Some(keys.as_str()))] {
        let (code, _) = http(t.d.port, method, path, &[], body);
        assert_eq!(code, 401, "nobody: {method} {path}");
        let (code, _) = http_at(&t.d, t.d.port, "p1", method, path, &[], body);
        assert_eq!(code, 403, "an agent: {method} {path}");
    }
    // A POST from another origin is refused by the guard.
    let (code, _) = ask(&t, "POST", "/api/agent/keys", Some(serde_json::from_str(&keys).unwrap()), &[("Origin", "http://127.0.0.1:3000")]);
    assert_eq!(code, 403);
    // The page's cookie: only from our own origin, which its browser says.
    let c = cookie(&t);
    for fetch in [Some("same-site"), Some("cross-site"), None] {
        let mut h = vec![("Cookie", c.as_str())];
        h.extend(fetch.map(|f| ("Sec-Fetch-Site", f)));
        assert_eq!(http(t.d.port, "GET", "/api/agent/screen?agent=s-a", &h, None).0, 403, "{fetch:?}");
        assert_eq!(http(t.d.port, "GET", "/api/agent/view?agent=s-a", &h, None).0, 403, "{fetch:?}");
    }
    assert_eq!(t.fake.observing.load(std::sync::atomic::Ordering::SeqCst), 0, "no view was started");
    let (code, text) = http(t.d.port, "GET", "/api/agent/screen?agent=s-a", &[("Cookie", &c), ("Sec-Fetch-Site", "same-origin")], None);
    assert_eq!(code, 200, "{text}");
    assert!(text.to_lowercase().contains("cache-control: no-store"), "{text}");
    assert!(pressed(&t).is_empty());
}

/// Tests 2 and 13. Nothing is sent unless the whole screen is the one shown (a change above
/// the region counts); a request goes once; the same dialog again takes a fresh look. What is
/// recorded of the screen: the question line, and the option cut where its command begins.
#[test]
fn a_press_goes_only_to_the_screen_shown() {
    let t = service(Duration::from_secs(2), Duration::from_secs(5));
    let (_, s) = screen(&t, "s-a");
    assert_eq!(s["dialog"]["options"].as_array().unwrap().len(), 3);
    assert_eq!(s["waiting"], "blocked");
    // Above the region only: still another screen.
    t.fake.shown.lock().unwrap().insert("p2".into(), bash("⏺ Bash(touch b.txt)"));
    let (code, r) = press(&t, "s-a", &s["digest"], json!({ "option": 2 }));
    assert_eq!((code, r["code"].as_str()), (409, Some("changed")), "{r}");
    assert!(r["screen"]["text"].as_str().unwrap().contains("touch b.txt"));
    assert!(pressed(&t).is_empty(), "nothing sent");
    // The screen now: its digit, whatever is selected; the same request again goes nowhere.
    then(&t, "p2", &[(String::new(), Some(Idle))]);
    let (_, s) = screen(&t, "s-a");
    let (code, r) = press(&t, "s-a", &s["digest"], json!({ "option": 2 }));
    assert_eq!((code, r["outcome"].as_str()), (200, Some("answered")), "{r}");
    assert_eq!(pressed(&t), [("p2".to_string(), vec![Key::Digit(2)])]);
    let (code, _) = press(&t, "s-a", &s["digest"], json!({ "option": 2 }));
    assert_ne!(code, 200);
    assert_eq!(pressed(&t).len(), 1, "once");
    let kept = bodies(&t.db, "agent.keys");
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0], json!({ "agent": "s-a", "unit": "u1", "task": 1, "keys": ["2"], "outcome": "answered", "question": "Do you want to proceed?", "option": { "number": 2, "words": "Yes, and don't ask again for" } }));
    let task = t.d.get("/api/workspace").unwrap()["intents"][0]["units"][0]["tasks"][0]["input"]["answers"].clone();
    assert_eq!(task[0]["option"]["words"], "Yes, and don't ask again for");

    // The same command asked again: the same dialog. The earlier digest no longer matches.
    t.fake.shown.lock().unwrap().insert("p2".into(), bash("⏺ Bash(touch b.txt)"));
    t.fake.agents.lock().unwrap()[1].1.state = Blocked;
    wait_until("waiting again", || waits_on(&t, "s-a").is_some());
    let (_, again) = screen(&t, "s-a");
    let (code, r) = press(&t, "s-a", &s["digest"], json!({ "option": 1 }));
    assert_eq!((code, r["code"].as_str()), (409, Some("changed")), "{r}");
    then(&t, "p2", &[(String::new(), Some(Idle))]);
    let (code, r) = press(&t, "s-a", &again["digest"], json!({ "option": 1 }));
    assert_eq!((code, r["outcome"].as_str()), (200, Some("answered")), "{r}");
    assert_eq!(pressed(&t).len(), 2);
}

/// Test 3. A marked list: one arrow, a read, on only when the marker is on the next line and
/// nothing else changed; Enter only then. A numbered option: its digit alone.
#[test]
fn a_marked_list_one_step_at_a_time() {
    let t = service(Duration::from_secs(2), Duration::from_secs(5));
    t.fake.shown.lock().unwrap().insert("p2".into(), list(0));
    then(&t, "p2", &[(list(1), None), (list(2), None), (String::new(), Some(Idle))]);
    let (_, s) = screen(&t, "s-a");
    assert_eq!(s["dialog"]["numbered"], false);
    let (code, r) = press(&t, "s-a", &s["digest"], json!({ "option": 3 }));
    assert_eq!((code, r["outcome"].as_str()), (200, Some("answered")), "{r}");
    assert_eq!(pressed(&t).into_iter().map(|(_, k)| k).collect::<Vec<_>>(), [vec![Key::Down], vec![Key::Down], vec![Key::Enter]]);
    let reads = t.fake.reads.lock().unwrap().len();
    assert!(reads >= 4, "a read after each step: {reads}");

    // The list wraps: the marker lands somewhere else than the next line. No Enter.
    for (after, why) in [(list(2), "skipped a line"), (list(0).replace("What now?", "What then?"), "another line changed")] {
        t.fake.pressed.lock().unwrap().clear();
        t.fake.shown.lock().unwrap().insert("p2".into(), list(0));
        t.fake.agents.lock().unwrap()[1].1.state = Blocked;
        wait_until("waiting again", || waits_on(&t, "s-a").is_some());
        then(&t, "p2", &[(after, None)]);
        let (_, s) = screen(&t, "s-a");
        let (code, r) = press(&t, "s-a", &s["digest"], json!({ "option": 2 }));
        assert_eq!((code, r["outcome"].as_str()), (200, Some("changed")), "{why}: {r}");
        assert_eq!(pressed(&t).into_iter().map(|(_, k)| k).collect::<Vec<_>>(), [vec![Key::Down]], "{why}");
        assert!(r["screen"]["text"].is_string());
    }
    // A marker that does not move: as many steps as options at most, and no Enter.
    t.fake.pressed.lock().unwrap().clear();
    t.fake.shown.lock().unwrap().insert("p2".into(), list(0));
    then(&t, "p2", &[]);
    let (_, s) = screen(&t, "s-a");
    let (_, r) = press(&t, "s-a", &s["digest"], json!({ "option": 3 }));
    assert_eq!(r["outcome"], "changed");
    assert_eq!(pressed(&t).len(), 1);

    // Numbered: the digit, though the marker is on another option.
    t.fake.pressed.lock().unwrap().clear();
    t.fake.shown.lock().unwrap().insert("p2".into(), bash(""));
    then(&t, "p2", &[(String::new(), Some(Idle))]);
    let (_, s) = screen(&t, "s-a");
    press(&t, "s-a", &s["digest"], json!({ "option": 3 }));
    assert_eq!(pressed(&t).into_iter().map(|(_, k)| k).collect::<Vec<_>>(), [vec![Key::Digit(3)]]);
}

/// Test 4. What the screen after the key says: the dialog still there (a digit that only
/// moved the selection), another dialog, or gone while the runtime still reports blocked.
#[test]
fn what_came_of_a_press() {
    let t = service(Duration::from_secs(2), Duration::from_secs(5));
    let moved = bash("⏺ Bash(touch a.txt)").replace(" ❯ 1. Yes", "   1. Yes").replace("   3. No", " ❯ 3. No");
    then(&t, "p2", &[(moved.clone(), None)]);
    let (_, s) = screen(&t, "s-a");
    let (_, r) = press(&t, "s-a", &s["digest"], json!({ "option": 3 }));
    assert_eq!(r["outcome"], "open", "{r}");
    assert!(r["screen"]["text"].as_str().unwrap().contains("❯ 3. No"));
    assert!(r["screen"]["digest"].is_string() && r["screen"]["digest"] != s["digest"]);
    // A second dialog after the first.
    then(&t, "p2", &[(list(0), None)]);
    let (_, r) = press(&t, "s-a", &r["screen"]["digest"], json!({ "option": 1 }));
    assert_eq!(r["outcome"], "new", "{r}");
    assert_eq!(r["screen"]["dialog"]["options"][1]["words"], "Throw it away");
    // Gone, but the runtime still says blocked: not answered.
    then(&t, "p2", &[(String::new(), None)]);
    let (_, r) = press(&t, "s-a", &r["screen"]["digest"], json!({ "key": "escape" }));
    assert_eq!(r["outcome"], "sent", "{r}");
    assert_eq!(pressed(&t).len(), 3);
}

/// Tests 5 and 6. Only to an agent that waits; one writer at a time; keys of the set only.
#[test]
fn one_writer_and_only_keys() {
    let t = service(Duration::from_millis(200), Duration::from_secs(5));
    let (_, s) = screen(&t, "s-a");
    for bad in [json!({ "key": "x" }), json!({ "key": "enter\n" }), json!({ "text": "rm -rf /" }), json!({ "option": 12 }), json!({ "keys": ["1", "2"] }), json!({ "option": 1, "key": "enter" })] {
        let (code, _) = press(&t, "s-a", &s["digest"], bad.clone());
        assert_eq!(code, 400, "{bad}");
    }
    // Nobody waits there; a name is not a session; a session nobody has.
    for agent in ["s-boss", "alice", "s-gone"] {
        let (code, _) = press(&t, agent, &s["digest"], json!({ "key": "escape" }));
        assert_ne!(code, 200, "{agent}");
    }
    assert!(pressed(&t).is_empty());
    // Two at once: the second is refused while the first reads.
    *t.fake.on_read.lock().unwrap() = Some(Box::new(|_| std::thread::sleep(Duration::from_millis(600))));
    let first = std::thread::spawn({
        let (port, token, proof) = (t.d.port, t.d.token().unwrap(), t.d.process(false).0.unwrap());
        let body = json!({ "agent": "s-a", "shown": s["digest"], "press": { "key": "escape" } }).to_string();
        move || http(port, "POST", "/api/agent/keys", &[("authorization", &format!("Bearer {token}")), ("x-workriver-caller", &proof)], Some(&body)).0
    });
    std::thread::sleep(Duration::from_millis(200));
    *t.fake.on_read.lock().unwrap() = None;
    let (code, r) = press(&t, "s-a", &s["digest"], json!({ "key": "escape" }));
    assert_eq!((code, r["code"].as_str()), (409, Some("busy")), "{r}");
    assert_eq!(first.join().unwrap(), 200);
    assert_eq!(pressed(&t).len(), 1);

    // A press while a message is being typed into the same agent: refused, never between.
    // (A requirement added to its task is told it whatever its state; its screen is read
    // first, under the agent's writer: held here for a while.)
    let (_, s) = screen(&t, "s-a");
    let reading = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
    *t.fake.on_read.lock().unwrap() = Some(Box::new({
        let reading = reading.clone();
        move |_| {
            if reading.compare_exchange(1, 2, std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst).is_ok() {
                std::thread::sleep(Duration::from_millis(1500));
            }
        }
    }));
    reading.store(1, std::sync::atomic::Ordering::SeqCst);
    // The note is told within its request: that request waits on the read.
    let noting = std::thread::spawn({
        let d = t.d.clone();
        move || d.append("s-boss", "task.noted", json!({ "unit": "u1", "task": 1, "text": "Also the queue" })).unwrap()
    });
    wait_until("the message's read is under way", || reading.load(std::sync::atomic::Ordering::SeqCst) == 2);
    let (code, r) = press(&t, "s-a", &s["digest"], json!({ "key": "escape" }));
    assert_eq!((code, r["code"].as_str()), (409, Some("busy")), "{r}");
    noting.join().unwrap();
    assert_eq!(pressed(&t).len(), 1);
}

/// Test 7. A send that failed or whose answer was lost: said so, never sent again.
#[test]
fn a_lost_press_is_not_sent_again() {
    let t = service(Duration::from_secs(2), Duration::from_secs(5));
    t.fake.press_fails.store(true, std::sync::atomic::Ordering::SeqCst);
    let (_, s) = screen(&t, "s-a");
    let (code, r) = press(&t, "s-a", &s["digest"], json!({ "option": 1 }));
    assert_eq!((code, r["outcome"].as_str()), (200, Some("unsure")), "{r}");
    assert_eq!(pressed(&t).len(), 1);
    // And the digest it was given no longer matches: the press may have gone.
    let (code, _) = press(&t, "s-a", &s["digest"], json!({ "option": 1 }));
    assert_eq!(code, 409);
    assert_eq!(pressed(&t).len(), 1);
}

/// Test 8. A dialog the runtime missed: an idle agent whose screen shows options and a key is
/// waiting on its screen, counted as blocked, its messages held; one whose transcript only
/// quotes a dialog is not; the mark goes when the screen clears.
#[test]
fn a_dialog_the_runtime_missed() {
    let t = service(Duration::from_millis(300), Duration::from_secs(5));
    // The manager quotes one: not waiting.
    t.fake.shown.lock().unwrap().insert("p1".into(), quoting());
    t.fake.agents.lock().unwrap()[0].1.state = Working;
    std::thread::sleep(Duration::from_millis(700));
    t.fake.agents.lock().unwrap()[0].1.state = Idle;
    std::thread::sleep(Duration::from_millis(1000));
    assert_eq!(waits_on(&t, "s-boss"), None);
    // It shows one now: waiting on its screen, from the next look.
    t.fake.shown.lock().unwrap().insert("p1".into(), bash(""));
    t.fake.agents.lock().unwrap()[0].1.state = Done;
    wait_until("boss waits on its screen", || waits_on(&t, "s-boss").is_some_and(|(_, screen)| screen));
    let ws = t.d.get("/api/workspace").unwrap().to_string();
    assert!(!ws.contains("Do you want to proceed") && !ws.contains("Bash command"), "no screen text in the snapshot");
    let (_, s) = screen(&t, "s-boss");
    assert_eq!(s["waiting"], "screen");
    assert!(s["why"].is_null());
    // A message to it is held: nothing typed.
    let before = t.fake.typed_at.lock().unwrap().len();
    t.d.call("/api/message", Some(json!({ "intent": "i1", "text": "Still there?" }))).unwrap();
    std::thread::sleep(Duration::from_millis(1000));
    assert_eq!(t.fake.typed_at.lock().unwrap().len(), before, "not typed into a dialog");
    assert!(!t.fake.sent.lock().unwrap().iter().any(|(_, m)| m.contains("Still there?")));
    // The screen clears: the mark goes, and the message is given.
    t.fake.shown.lock().unwrap().insert("p1".into(), String::new());
    wait_until("the mark goes", || waits_on(&t, "s-boss").is_none());
    wait_until("the held message is given", || t.fake.sent.lock().unwrap().iter().any(|(to, m)| to == "s-boss" && m.contains("Still there?")));
}

/// Test 8, the read before a message: a message to an idle agent whose screen shows a dialog
/// (drawn with no change of state since the last look) is held, nothing typed. Test 12, its
/// half: the message goes to the one place read, by `send_at`. The manager here is seated (at
/// p1): its pin is that place, read first and typed into, never another that shows it.
#[test]
fn the_read_before_a_message() {
    let t = service(Duration::from_secs(2), Duration::from_secs(5));
    t.fake.shown.lock().unwrap().insert("p1".into(), String::new());
    t.d.call("/api/message", Some(json!({ "intent": "i1", "text": "First" }))).unwrap();
    wait_until("typed at its place", || t.fake.typed_at.lock().unwrap().iter().any(|(p, m)| p == "p1" && m.contains("First")));
    // Another place starts showing the session as its screen is read: the message still goes
    // to the place read, and no other.
    *t.fake.on_read.lock().unwrap() = Some(Box::new({
        let fake = std::sync::Arc::downgrade(&t.fake);
        move |place| {
            if place == "p1"
                && let Some(f) = fake.upgrade()
            {
                f.also.lock().unwrap().push(("p9".into(), "s-boss".into()));
            }
        }
    }));
    t.d.call("/api/message", Some(json!({ "intent": "i1", "text": "Second" }))).unwrap();
    wait_until("typed", || t.fake.typed_at.lock().unwrap().iter().any(|(_, m)| m.contains("Second")));
    assert!(t.fake.typed_at.lock().unwrap().iter().all(|(p, _)| p == "p1"));
    assert!(!t.fake.reads.lock().unwrap().iter().any(|p| p == "p9"), "only its pin is read");
    *t.fake.on_read.lock().unwrap() = None;
    t.fake.also.lock().unwrap().clear();
    // A dialog drawn since: held.
    t.fake.shown.lock().unwrap().insert("p1".into(), bash(""));
    t.d.call("/api/message", Some(json!({ "intent": "i1", "text": "Third" }))).unwrap();
    std::thread::sleep(Duration::from_millis(1000));
    assert!(!t.fake.typed_at.lock().unwrap().iter().any(|(_, m)| m.contains("Third")));
    wait_until("boss waits on its screen", || waits_on(&t, "s-boss").is_some_and(|(_, screen)| screen));
}

/// The read before a message to an agent with no seat (a discussion with a task's agent): its
/// one place is resolved, its screen read there, and the message typed there; a dialog on it
/// holds the message, and two places that show it get nothing.
#[test]
fn the_read_before_a_message_to_an_agent_with_no_seat() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle)), ("p2".into(), live("s-d", "designer", "w1", Idle))];
    fake.shown.lock().unwrap().insert("p2".into(), bash(""));
    let d = daemon_with(fake.clone(), &tmp.path().join("w.db"), crate::server::Waits::default());
    manage(&d, "p1", "s-boss");
    d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "M", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "design", "title": "Design the index", "agent": "s-d" })).unwrap();
    d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "gate", "gate": "decide", "title": "Confirm", "inputs": [{ "from": "1" }] })).unwrap();
    d.append("s-d", "task.started", json!({ "unit": "u1", "task": 1 })).unwrap();
    d.append("s-d", "task.delivered", json!({ "unit": "u1", "task": 1, "note": "A trie." })).unwrap();
    std::thread::sleep(Duration::from_millis(2500)); // a poll sees the agents
    let typed = |what: &str| fake.typed_at.lock().unwrap().iter().filter(|(_, m)| m.contains(what)).map(|(p, _)| p.clone()).collect::<Vec<_>>();
    d.call("/api/discuss", Some(json!({ "unit": "u1", "task": 1, "text": "Why a trie?" }))).unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    assert!(typed("Why a trie?").is_empty(), "held while a dialog is on its screen");
    assert!(fake.reads.lock().unwrap().iter().any(|p| p == "p2"), "read at its place");
    fake.shown.lock().unwrap().insert("p2".into(), String::new());
    wait_until("given once the dialog is gone", || typed("Why a trie?") == ["p2"]);
    // Two places show it: nothing is typed at either.
    fake.also.lock().unwrap().push(("p9".into(), "s-d".into()));
    d.call("/api/discuss", Some(json!({ "unit": "u1", "task": 1, "text": "And memory?" }))).unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    assert!(typed("And memory?").is_empty());
}

/// Test 12. One place per request: two that show the session at the start, refused with
/// nothing sent; one that starts showing it between the read and the key gets nothing.
#[test]
fn one_place_per_request() {
    let t = service(Duration::from_secs(2), Duration::from_secs(5));
    let (_, s) = screen(&t, "s-a");
    t.fake.also.lock().unwrap().push(("p9".into(), "s-a".into()));
    let (code, r) = screen(&t, "s-a");
    assert_eq!((code, r["code"].as_str()), (409, Some("two")), "{r}");
    let (code, r) = press(&t, "s-a", &s["digest"], json!({ "option": 1 }));
    assert_eq!((code, r["code"].as_str()), (409, Some("two")), "{r}");
    assert!(pressed(&t).is_empty());
    t.fake.also.lock().unwrap().clear();
    t.fake.reads.lock().unwrap().clear();
    *t.fake.on_read.lock().unwrap() = Some(Box::new({
        let fake = std::sync::Arc::downgrade(&t.fake);
        move |_| {
            if let Some(f) = fake.upgrade()
                && f.also.lock().unwrap().is_empty()
            {
                f.also.lock().unwrap().push(("p9".into(), "s-a".into()));
            }
        }
    }));
    then(&t, "p2", &[(String::new(), Some(Idle))]);
    let (code, r) = press(&t, "s-a", &s["digest"], json!({ "option": 1 }));
    assert_eq!((code, r["outcome"].as_str()), (200, Some("answered")), "{r}");
    assert_eq!(pressed(&t), [("p2".to_string(), vec![Key::Digit(1)])]);
    assert!(t.fake.reads.lock().unwrap().iter().all(|p| p == "p2"), "{:?}", t.fake.reads.lock().unwrap());
}

/// Test 9. A change the runtime tells wakes the poll before its tick; a change stream that
/// ends is opened again.
#[test]
fn the_runtimes_changes_wake_the_poll() {
    let (tx, rx) = std::sync::mpsc::channel();
    let t = {
        let tmp = tempfile::tempdir().unwrap();
        let fake = std::sync::Arc::new(Fake::default());
        *fake.agents.lock().unwrap() = vec![("p1".into(), live("s-boss", "boss", "w1", Idle))];
        *fake.changes.lock().unwrap() = Some(rx);
        let db = tmp.path().join("w.db");
        let waits = crate::server::Waits { tick: Duration::from_secs(60), rewatch: Duration::from_millis(200), ..Default::default() };
        let (d, kernel) = daemon_and(fake.clone(), &db, waits);
        T { _tmp: tmp, db, fake, d, kernel, machines: None }
    };
    wait_until("the first poll", || t.d.get("/api/workspace").unwrap()["sessions"].as_array().is_some_and(|s| !s.is_empty()));
    wait_until("the change stream is followed", || t.fake.watched.load(std::sync::atomic::Ordering::SeqCst) == 1);
    t.fake.agents.lock().unwrap()[0].1.state = Blocked;
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(waits_on(&t, "s-boss"), None, "not before the change is told (the tick is a minute)");
    tx.send(workriver_core::runtime::Change { place: "p1".into(), state: Blocked }).unwrap();
    wait_until("the poll ran early", || waits_on(&t, "s-boss").is_some());
    drop(tx);
    wait_until("opened again after it ended", || t.fake.watched.load(std::sync::atomic::Ordering::SeqCst) >= 3);
}

/// Reads one Server-Sent Event stream's start: the status, and the text read before `until`
/// shows (or 2 s pass).
fn open_view(port: u16, token: &str, proof: &str, until: &str) -> (std::net::TcpStream, u16, String) {
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
    let req = format!("GET /api/agent/view?agent=s-a HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nauthorization: Bearer {token}\r\nx-workriver-caller: {proof}\r\n\r\n");
    s.write_all(req.as_bytes()).unwrap();
    let mut got = String::new();
    let end = std::time::Instant::now() + Duration::from_secs(2);
    let mut buf = [0u8; 4096];
    while std::time::Instant::now() < end && !got.contains(until) {
        if let Ok(n) = s.read(&mut buf) {
            if n == 0 {
                break;
            }
            got.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
    }
    let code = got.split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    (s, code, got)
}

/// Test 10. A view's stream: whole frame first; its runtime stream ends with its reader; at
/// most `VIEWS_MAX` open; a reader that falls behind has its stream ended.
#[test]
fn a_view_ends_with_its_reader() {
    use std::sync::atomic::Ordering::SeqCst;
    let t = service(Duration::from_secs(2), Duration::from_secs(5));
    *t.fake.frames.lock().unwrap() = vec![workriver_core::screens::Frame { full: true, cols: 120, rows: 40, seq: 1, bytes: "aGVsbG8=".into() }];
    let (token, proof) = (t.d.token().unwrap(), t.d.process(false).0.unwrap());
    let (s, code, got) = open_view(t.d.port, &token, &proof, "aGVsbG8=");
    assert_eq!(code, 200, "{got}");
    assert!(got.contains("event: frame") && got.contains("\"full\":true") && got.to_lowercase().contains("cache-control: no-store"), "{got}");
    assert_eq!(t.fake.observing.load(SeqCst), 1);
    drop(s);
    wait_until("its runtime stream ends with its reader", || t.fake.observing.load(SeqCst) == 0);
    // The cap.
    let open: Vec<_> = (0..crate::common::config::defaults::VIEWS_MAX).map(|_| open_view(t.d.port, &token, &proof, "aGVsbG8=")).collect();
    assert!(open.iter().all(|(_, c, _)| *c == 200));
    let (_, code, got) = open_view(t.d.port, &token, &proof, "views");
    assert_eq!(code, 503, "{got}");
    assert!(got.contains("\"code\":\"views\""), "{got}");
    drop(open);
    wait_until("all closed", || t.fake.observing.load(SeqCst) == 0);
    // A reader that falls behind: frames come faster than it reads.
    let big = "A".repeat(64 * 1024);
    *t.fake.frames.lock().unwrap() = (0..800).map(|k| workriver_core::screens::Frame { full: k == 0, cols: 120, rows: 40, seq: k, bytes: big.clone() }).collect();
    let (slow, code, _) = open_view(t.d.port, &token, &proof, "event: frame");
    assert_eq!(code, 200);
    wait_until("the slow reader's stream is ended", || t.fake.observing.load(SeqCst) == 0);
    drop(slow);
}

/// The code review's reproduction: a screen read just before a press is answered must not
/// be signed with the count after it. The GET pauses after its read (in the runtime's
/// reason); meanwhile a press answers the dialog and the same dialog comes back. The GET's
/// digest then authorizes nothing.
#[test]
fn a_screen_read_is_signed_with_the_presses_before_it() {
    use std::sync::atomic::{AtomicU8, Ordering::SeqCst};
    let t = service(Duration::from_secs(2), Duration::from_secs(5));
    let (_, first) = screen(&t, "s-a");
    let paused = std::sync::Arc::new(AtomicU8::new(0));
    *t.fake.on_why.lock().unwrap() = Some(Box::new({
        let paused = paused.clone();
        move |_| {
            if paused.compare_exchange(0, 1, SeqCst, SeqCst).is_ok() {
                std::thread::sleep(Duration::from_millis(1500));
            }
        }
    }));
    let late = std::thread::spawn({
        let (port, token, proof) = (t.d.port, t.d.token().unwrap(), t.d.process(false).0.unwrap());
        move || {
            let (_, text) = http(port, "GET", "/api/agent/screen?agent=s-a", &[("authorization", &format!("Bearer {token}")), ("x-workriver-caller", &proof)], None);
            serde_json::from_str::<Value>(text.split("\r\n\r\n").nth(1).unwrap_or_default()).unwrap()
        }
    });
    wait_until("the late read has read the screen", || paused.load(SeqCst) == 1);
    // The first dialog is answered; the same one is drawn again.
    then(&t, "p2", &[(bash("⏺ Bash(touch a.txt)"), None)]);
    let (code, r) = press(&t, "s-a", &first["digest"], json!({ "option": 1 }));
    assert_eq!(code, 200, "{r}");
    let late = late.join().unwrap();
    assert_eq!(late["text"], first["text"], "it read the first dialog");
    let (code, r) = press(&t, "s-a", &late["digest"], json!({ "option": 1 }));
    assert_eq!((code, r["code"].as_str()), (409, Some("changed")), "{r}");
    assert_eq!(pressed(&t).len(), 1, "the second dialog took no key");
}

/// A runtime that cannot answer the read before a message (its socket fails) is not one
/// that cannot read screens: nothing is typed, and the message waits.
#[test]
fn a_failed_read_before_a_message_types_nothing() {
    use std::sync::atomic::Ordering::SeqCst;
    let t = service(Duration::from_millis(300), Duration::from_secs(5));
    t.fake.shown.lock().unwrap().insert("p1".into(), String::new());
    for (down, what) in [(&t.fake.screen_down, "the screen"), (&t.fake.resolve_down, "the place")] {
        down.store(true, SeqCst);
        let text = format!("While {what} cannot be read");
        t.d.call("/api/message", Some(json!({ "intent": "i1", "text": text }))).unwrap();
        std::thread::sleep(Duration::from_millis(1000));
        assert!(!t.fake.sent.lock().unwrap().iter().any(|(_, m)| m.contains(&text)), "{what}: typed");
        assert!(!t.fake.typed_at.lock().unwrap().iter().any(|(_, m)| m.contains(&text)), "{what}: typed at");
        down.store(false, SeqCst);
        wait_until("given once it can be read", || t.fake.typed_at.lock().unwrap().iter().any(|(p, m)| p == "p1" && m.contains(&text)));
    }
}

/// After a key, the dialog gone but the runtime's state cannot be read: not answered.
#[test]
fn a_failed_status_read_is_not_answered() {
    use std::sync::atomic::Ordering::SeqCst;
    let t = service(Duration::from_secs(2), Duration::from_secs(5));
    then(&t, "p2", &[(String::new(), None)]);
    t.fake.locate_down.store(true, SeqCst);
    let (_, s) = screen(&t, "s-a");
    let (code, r) = press(&t, "s-a", &s["digest"], json!({ "option": 1 }));
    assert_eq!(code, 200, "{r}");
    assert_ne!(r["outcome"], "answered", "{r}");
    assert!(r["screen"]["text"].is_string(), "the screen comes with it: {r}");
    assert_ne!(bodies(&t.db, "agent.keys")[0]["outcome"], "answered");
}

/// A service whose agents are on this machine (`here`) and on the machine `box` (`far`), with
/// this tick; `boss` manages i1 here.
fn two_machines(here: Vec<(&str, workriver_core::runtime::RuntimeAgent)>, far: Vec<(&str, workriver_core::runtime::RuntimeAgent)>, tick: Duration) -> (T, std::sync::Arc<Fake>) {
    let tmp = tempfile::tempdir().unwrap();
    let fake = std::sync::Arc::new(Fake::default());
    *fake.agents.lock().unwrap() = here.into_iter().map(|(p, a)| (p.to_string(), a)).collect();
    let other = std::sync::Arc::new(Fake::default());
    *other.agents.lock().unwrap() = far.into_iter().map(|(p, a)| (p.to_string(), a)).collect();
    let all = crate::runtime::follow::Machines::with(fake.clone(), vec![("box", other.clone())]);
    let db = tmp.path().join("w.db");
    let store = workriver_data::Store::open(&db).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(crate::server::bind(0)).unwrap();
    let kernel = Kernel::default();
    let d = kernel.client(listener.local_addr().unwrap().port(), tmp.path());
    let waits = crate::server::Waits { tick, ..Default::default() };
    let (runtime, machines) = (all.clone(), all.clone());
    let options = crate::server::Options { waits, allow_as: true, machines: Some(machines), ..kernel.options() };
    std::thread::spawn(move || rt.block_on(crate::server::serve_with(listener, store, runtime, options)));
    (T { _tmp: tmp, db, fake, d, kernel, machines: Some(all) }, other)
}

/// The session as the page gets it on `machine` (none: here).
fn session_on(t: &T, session: &str, machine: Option<&str>) -> Option<Value> {
    let ws = t.d.get("/api/workspace").unwrap();
    ws["sessions"].as_array().unwrap().iter().find(|s| s["session"] == session && s["machine"].as_str() == machine).cloned()
}

/// A resumed session seen on two machines: each is its own agent. The one here waits on its
/// screen; the one over there, with a clear screen, does not take that mark away.
#[test]
fn the_same_session_on_two_machines() {
    let (t, far) = two_machines(vec![("p1", live("s-x", "x", "w1", Idle))], vec![("w9:p1", live("s-x", "x", "w9", Idle))], Duration::from_millis(300));
    t.fake.shown.lock().unwrap().insert("p1".into(), bash(""));
    far.shown.lock().unwrap().insert("w9:p1".into(), String::new());
    wait_until("the one here waits on its screen", || session_on(&t, "s-x", None).is_some_and(|s| s["onScreen"] == true));
    // Several looks later, still.
    std::thread::sleep(Duration::from_millis(1500));
    let here = session_on(&t, "s-x", None).unwrap();
    assert_eq!(here["onScreen"], true, "{here}");
    let there = session_on(&t, "s-x", Some("box")).unwrap();
    assert!(there["blockedSince"].is_null() && there["onScreen"].is_null(), "{there}");
    // The screen route asks the machine named.
    let (_, s) = ask(&t, "GET", "/api/agent/screen?agent=s-x&machine=box", None, &[]);
    assert!(s["waiting"].is_null(), "{s}");
    let (_, s) = screen(&t, "s-x");
    assert_eq!(s["waiting"], "screen", "{s}");
}

/// A machine whose screens are slow to read never holds this machine's states back: its
/// reads are made beside the poll, and what they find is shown when they end.
#[test]
fn a_slow_machine_never_holds_the_poll() {
    let (t, far) = two_machines(vec![("p1", live("s-here", "here", "w1", Blocked))], vec![("w9:p1", live("s-far", "far", "w9", Idle))], Duration::from_millis(300));
    far.shown.lock().unwrap().insert("w9:p1".into(), bash(""));
    *far.on_read.lock().unwrap() = Some(Box::new(|_| std::thread::sleep(Duration::from_secs(3))));
    wait_until("here waits", || session_on(&t, "s-here", None).is_some_and(|s| s["blockedSince"].is_string()));
    t.fake.agents.lock().unwrap()[0].1.state = Working;
    let start = std::time::Instant::now();
    while session_on(&t, "s-here", None).is_some_and(|s| s["blockedSince"].is_string()) {
        assert!(start.elapsed() < Duration::from_millis(1500), "here is still shown blocked after {:?}", start.elapsed());
        std::thread::sleep(Duration::from_millis(100));
    }
    // And the slow read's answer is shown when it comes.
    *far.on_read.lock().unwrap() = None;
    wait_until("far waits on its screen", || session_on(&t, "s-far", Some("box")).is_some_and(|s| s["onScreen"] == true));
}

/// The code review's reproduction: another machine's agent first seen idle with a clear
/// screen draws its dialog 500 ms later. The read a tick later still comes (the first read's
/// end does not use it up at once) and finds the dialog.
#[test]
fn a_dialog_drawn_after_the_first_read_on_another_machine() {
    let (t, far) = two_machines(vec![], vec![("w9:p1", live("s-far", "far", "w9", Idle))], Duration::from_secs(2));
    let reads = std::sync::Arc::new(std::sync::Mutex::new(Vec::<std::time::Instant>::new()));
    let seen = reads.clone();
    *far.on_read.lock().unwrap() = Some(Box::new(move |_| seen.lock().unwrap().push(std::time::Instant::now())));
    wait_until("the first read", || !reads.lock().unwrap().is_empty());
    let first = reads.lock().unwrap()[0];
    std::thread::sleep(Duration::from_millis(500));
    far.shown.lock().unwrap().insert("w9:p1".into(), bash(""));
    wait_until("far waits on its screen", || session_on(&t, "s-far", Some("box")).is_some_and(|s| s["onScreen"] == true));
    let reads = reads.lock().unwrap();
    assert!(reads[1] - first >= Duration::from_millis(800), "the second read came {:?} after the first", reads[1] - first);
}

/// The code review's reproduction: a whole frame too large to pass on is not followed by the
/// changes after it, drawn on nothing: the stream ends there.
#[test]
fn a_view_ends_at_a_frame_it_cannot_pass_on() {
    let t = service(Duration::from_secs(2), Duration::from_secs(5));
    let big = "A".repeat(crate::common::config::defaults::VIEW_FRAME_MAX + 1);
    *t.fake.frames.lock().unwrap() = vec![
        workriver_core::screens::Frame { full: true, cols: 120, rows: 40, seq: 1, bytes: big },
        workriver_core::screens::Frame { full: false, cols: 120, rows: 40, seq: 2, bytes: "aGVsbG8=".into() },
    ];
    let (token, proof) = (t.d.token().unwrap(), t.d.process(false).0.unwrap());
    let (_s, code, got) = open_view(t.d.port, &token, &proof, "event: end");
    assert_eq!(code, 200, "{got}");
    assert!(got.contains("event: end"), "{got}");
    assert!(!got.contains("event: frame"), "no change drawn on nothing: {got}");
}

/// The status a terminal view of `query` (`agent=…&machine=…`) is answered with.
fn view_status(t: &T, query: &str) -> u16 {
    let (token, proof) = (t.d.token().unwrap(), t.d.process(false).0.unwrap());
    let mut s = std::net::TcpStream::connect(("127.0.0.1", t.d.port)).unwrap();
    s.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
    let req = format!("GET /api/agent/view?{query} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nauthorization: Bearer {token}\r\nx-workriver-caller: {proof}\r\n\r\n", t.d.port);
    s.write_all(req.as_bytes()).unwrap();
    let mut got = String::new();
    let end = std::time::Instant::now() + Duration::from_secs(5);
    let mut buf = [0u8; 4096];
    while std::time::Instant::now() < end && !got.contains("\r\n") {
        if let Ok(n) = s.read(&mut buf) {
            if n == 0 {
                break;
            }
            got.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
    }
    got.split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0)
}

/// What the page proposes for workspace ws1's seat (null: the seat is not lost).
fn proposal(t: &T) -> Value {
    let ws = t.d.get("/api/workspace").unwrap();
    ws["workspaces"].as_array().unwrap().iter().find(|w| w["id"] == "ws1").map(|w| w["proposal"].clone()).unwrap_or(Value::Null)
}

/// Nothing of a seated manager's that is lost is read or pressed elsewhere: its screen and
/// its view are "gone", a key is refused with nothing pressed, and its row offers no dialog.
fn nothing_of_the_lost_seat(t: &T, query: &str) {
    let (code, r) = ask(t, "GET", &format!("/api/agent/screen?{query}"), None, &[]);
    assert_eq!((code, r["code"].as_str()), (404, Some("gone")), "the screen: {r}");
    assert_eq!(view_status(t, query), 404, "the view");
    let machine = query.split("machine=").nth(1);
    let (code, r) = ask(t, "POST", "/api/agent/keys", Some(json!({ "agent": "s-boss", "machine": machine, "shown": "any", "press": { "option": 1 } })), &[]);
    assert!(code == 404 || r["code"] == "not-waiting", "the keys: {code} {r}");
    assert!(pressed(t).is_empty(), "nothing pressed: {:?}", pressed(t));
}

/// The security check's two probe cases: the seated manager's terminal is gone, or it runs
/// another agent now; either way another terminal (another folder) reports the manager's
/// session and waits on a dialog. The seat is lost: the screen and the view are "gone", a key
/// is refused and pressed nowhere, and the session's row has no wait and no dialog.
#[test]
fn a_lost_seat_takes_no_key_and_shows_no_screen() {
    for root_runs in [false, true] {
        let t = service(Duration::from_millis(300), Duration::from_secs(5));
        if !root_runs {
            t.kernel.procs.kill(root("p1"));
        }
        {
            let mut agents = t.fake.agents.lock().unwrap();
            agents.retain(|(p, _)| p != "p1");
            if root_runs {
                agents.push(("p1".into(), live("s-evil", "evil", "w1", Idle)));
            }
            agents.push(("p5".into(), workriver_core::runtime::RuntimeAgent { cwd: "/elsewhere".into(), ..live("s-boss", "boss", "w1", Blocked) }));
        }
        t.fake.shown.lock().unwrap().insert("p5".into(), bash("⏺ Bash(rm -rf x)"));
        wait_until("the seat is lost", || proposal(&t) == "s-boss");
        std::thread::sleep(Duration::from_millis(900)); // a few regular reads
        assert_eq!(waits_on(&t, "s-boss"), None, "no wait offered on the lost seat's row");
        nothing_of_the_lost_seat(&t, "agent=s-boss");
    }
}

/// The seated manager waits on a dialog at its pinned terminal, and the runtime's lookup of
/// its session would also find another place: the screen, the poll's mark and the keys go
/// to the pin, which alone shows the session among the roots.
#[test]
fn a_seated_managers_dialog_is_read_and_answered_at_its_pin() {
    let t = service(Duration::from_millis(300), Duration::from_secs(5));
    t.fake.shown.lock().unwrap().insert("p1".into(), bash("⏺ Bash(touch m.txt)"));
    t.fake.also.lock().unwrap().push(("p9".into(), "s-boss".into()));
    t.fake.agents.lock().unwrap()[0].1.state = Done; // its screen is read as it changes
    wait_until("boss waits on its screen", || waits_on(&t, "s-boss").is_some_and(|(_, screen)| screen));
    t.fake.reads.lock().unwrap().clear();
    let (code, s) = screen(&t, "s-boss");
    assert_eq!(code, 200, "{s}");
    assert_eq!(s["waiting"], "screen");
    then(&t, "p1", &[(String::new(), None)]);
    let (code, r) = press(&t, "s-boss", &s["digest"], json!({ "option": 1 }));
    assert_eq!(code, 200, "{r}");
    assert_eq!(pressed(&t), [("p1".to_string(), vec![Key::Digit(1)])]);
    assert!(t.fake.reads.lock().unwrap().iter().all(|p| p == "p1"), "{:?}", t.fake.reads.lock().unwrap());
}

/// A manager seated here whose session another machine reports too: a request that names
/// that machine is not the manager's (and the seat is lost). Nothing is read or pressed there.
#[test]
fn a_request_for_another_machine_than_the_seats_is_gone() {
    let (t, far) = two_machines(vec![("p1", live("s-boss", "boss", "w1", Idle))], vec![], Duration::from_millis(300));
    manage(&t.d, "p1", "s-boss");
    *far.agents.lock().unwrap() = vec![("w9:p4".into(), workriver_core::runtime::RuntimeAgent { cwd: "/srv/x".into(), ..live("s-boss", "boss", "w9", Blocked) })];
    far.shown.lock().unwrap().insert("w9:p4".into(), bash(""));
    t.machines.as_ref().unwrap().poll_all();
    wait_until("the seat is lost", || proposal(&t) == "s-boss");
    far.reads.lock().unwrap().clear();
    nothing_of_the_lost_seat(&t, "agent=s-boss&machine=box");
    assert!(far.pressed.lock().unwrap().is_empty() && far.reads.lock().unwrap().is_empty(), "nothing read or pressed over there");
}

/// A press reads the seat's machine itself, not the tick's read: a seat lost since the last
/// tick takes no key; a roots read that fails answers 503 and presses nothing.
#[test]
fn a_press_reads_the_seats_machine_now() {
    let t = service(Duration::from_secs(2), Duration::from_secs(5));
    let dialog = bash("⏺ Bash(touch q.txt)");
    t.fake.agents.lock().unwrap()[0].1.state = Blocked;
    t.fake.shown.lock().unwrap().insert("p1".into(), dialog.clone());
    wait_until("boss waits", || waits_on(&t, "s-boss").is_some());
    let (code, s) = screen(&t, "s-boss");
    assert_eq!(code, 200, "{s}");
    // Its roots cannot be read now.
    t.fake.agent_roots_down.store(true, std::sync::atomic::Ordering::SeqCst);
    let (code, r) = press(&t, "s-boss", &s["digest"], json!({ "option": 1 }));
    assert_eq!(code, 503, "{r}");
    assert!(pressed(&t).is_empty());
    t.fake.agent_roots_down.store(false, std::sync::atomic::Ordering::SeqCst);
    // Lost since the tick: its terminal gone, the session (and the same dialog) shown in
    // another folder.
    t.kernel.procs.kill(root("p1"));
    {
        let mut agents = t.fake.agents.lock().unwrap();
        agents.retain(|(p, _)| p != "p1");
        agents.push(("p5".into(), workriver_core::runtime::RuntimeAgent { cwd: "/elsewhere".into(), ..live("s-boss", "boss", "w1", Blocked) }));
    }
    t.fake.shown.lock().unwrap().insert("p5".into(), dialog);
    let (code, r) = press(&t, "s-boss", &s["digest"], json!({ "option": 1 }));
    assert!(code == 404 || r["code"] == "not-waiting", "{code} {r}");
    assert!(pressed(&t).is_empty(), "{:?}", pressed(&t));
}

/// The tick's read of the seat's machine failed (the agents or the roots cannot be read),
/// and the pinned shell runs another agent now: the screen and the view answer 503 (never
/// an older read), and nothing is typed or pressed.
#[test]
fn a_seat_not_read_takes_nothing() {
    for agents in [true, false] {
        let t = service(Duration::from_millis(300), Duration::from_secs(5));
        {
            let mut all = t.fake.agents.lock().unwrap();
            all.retain(|(p, _)| p != "p1");
            all.push(("p1".into(), live("s-boss", "evil", "w1", Blocked)));
        }
        t.fake.shown.lock().unwrap().insert("p1".into(), bash(""));
        if agents {
            t.fake.agents_down.store(true, std::sync::atomic::Ordering::SeqCst);
        } else {
            t.fake.agent_roots_down.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        std::thread::sleep(Duration::from_millis(900)); // a few regular reads
        let (code, r) = screen(&t, "s-boss");
        assert_eq!(code, 503, "the screen: {r}");
        assert_eq!(view_status(&t, "agent=s-boss"), 503, "the view");
        let (code, r) = press(&t, "s-boss", &json!("any"), json!({ "option": 1 }));
        assert!(code == 503 || r["code"] == "not-waiting", "{code} {r}");
        assert!(pressed(&t).is_empty());
        t.d.call("/api/message", Some(json!({ "intent": "i1", "text": "UNREAD_SEAT" }))).unwrap();
        std::thread::sleep(Duration::from_millis(900));
        assert!(t.fake.sent.lock().unwrap().iter().all(|(_, m)| !m.contains("UNREAD_SEAT")), "typed nothing");
    }
}

/// A session with no seat that two machines list takes nothing by its session: which of the
/// two is meant cannot be told (a discussion's question to a task's agent here).
#[test]
fn a_session_two_machines_list_takes_no_message_by_its_session() {
    let (t, far) = two_machines(vec![("p1", live("s-boss", "boss", "w1", Idle)), ("p2", live("s-d", "designer", "w1", Idle))], vec![], Duration::from_millis(300));
    let repo = t._tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    manage(&t.d, "p1", "s-boss");
    t.d.append("s-boss", "unit.created", json!({ "intent": "i1", "title": "M", "repo": repo.to_string_lossy(), "branch": "feat/f", "base": "main" })).unwrap();
    t.d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "design", "title": "Design the index", "agent": "s-d" })).unwrap();
    t.d.append("s-boss", "task.created", json!({ "unit": "u1", "kind": "gate", "gate": "decide", "title": "Confirm", "inputs": [{ "from": "1" }] })).unwrap();
    t.d.append("s-d", "task.started", json!({ "unit": "u1", "task": 1 })).unwrap();
    t.d.append("s-d", "task.delivered", json!({ "unit": "u1", "task": 1, "note": "A trie." })).unwrap();
    *far.agents.lock().unwrap() = vec![("w9:p2".into(), live("s-d", "designer", "w9", Idle))];
    t.machines.as_ref().unwrap().poll_all();
    wait_until("both machines list it", || session_on(&t, "s-d", Some("box")).is_some());
    t.d.call("/api/discuss", Some(json!({ "unit": "u1", "task": 1, "text": "Why a trie?" }))).unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    for f in [&t.fake, &far] {
        assert!(f.sent.lock().unwrap().iter().all(|(_, m)| !m.contains("Why a trie?")), "typed nothing");
    }
}
