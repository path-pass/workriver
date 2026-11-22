// `new check`: the new store's state, as the product replays it, against the old state as
// the old code replayed the copy. Everything that is not a workspace is equal; where a
// workspace is, it is the one the mapping says. A difference nothing explains fails it.
use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};
use workriver_core::State;

/// The new state as the product's store replays a copy of `db` (the file itself is not opened).
pub fn replayed(db: &Path) -> Result<State, String> {
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let copy = tmp.path().join("workriver.db");
    std::fs::copy(db, &copy).map_err(|e| format!("{}: {e}", db.display()))?;
    let s = workriver_data::Store::open(&copy).map_err(|e| e.to_string())?;
    Ok(s.state().clone())
}

fn without(v: &Value, keys: &[&str]) -> Value {
    let mut v = v.clone();
    if let Some(o) = v.as_object_mut() {
        for k in keys {
            o.remove(*k);
        }
    }
    v
}

/// The manager's words in a message the user has not read, under read mark `read`.
fn unread(m: &Value, read: Option<&Value>) -> usize {
    let r = read.and_then(Value::as_str);
    let after = |at: &Value| r.is_none_or(|r| at.as_str().unwrap_or_default() > r);
    let own = usize::from(m["by_manager"].as_bool().unwrap_or(false) && after(&m["at"]));
    own + m["replies"].as_array().map_or(0, |rs| rs.iter().filter(|x| after(&x["at"])).count())
}

pub fn check(dir: &Path, db: &Path) -> Result<String, String> {
    let new_state = replayed(db)?;
    let n = serde_json::to_value(&new_state).unwrap();
    let text = std::fs::read_to_string(dir.join("old-state.json")).map_err(|e| format!("old-state.json: {e}"))?;
    let mut o: Value = serde_json::from_str(&text).map_err(|e| format!("old-state.json: {e}"))?;
    let x = o.as_object_mut().unwrap().remove("expected").ok_or("old-state.json has no expected part")?;
    let mut diff: Vec<String> = vec![];
    let mut seen: Vec<String> = vec![];
    let arr = |v: &Value, k: &str| v[k].as_array().cloned().unwrap_or_default();

    // Intents: equal but where they run and the project a folder line or a workspace gave.
    let (oi, ni) = (arr(&o, "intents"), arr(&n, "intents"));
    if oi.len() != ni.len() {
        diff.push(format!("intents: {} before, {} after", oi.len(), ni.len()));
    }
    let mut moved = 0;
    for (a, b) in oi.iter().zip(&ni) {
        let id = a["id"].as_str().unwrap_or_default();
        if without(a, &["manager", "runtime_ref", "workspace", "project"]) != without(b, &["workspace", "project"]) {
            diff.push(format!("intent {id} differs:\n  before: {}\n  after:  {}", without(a, &["note"]), without(b, &["note"])));
        }
        if b["workspace"] != x["intents"][id] {
            diff.push(format!("intent {id} runs in {} after, the mapping says {}", b["workspace"], x["intents"][id]));
        }
        if b["project"] != x["intent_projects"][id] {
            diff.push(format!("intent {id} is of project {} after, the mapping says {}", b["project"], x["intent_projects"][id]));
        }
        moved += usize::from(b["project"] != a["project"]);
    }
    seen.push(format!("{} intents: equal but for their workspace, now a workspace id; {moved} took a project (a folder line, or the workspace an intent of no project ran in)", ni.len()));

    // Projects: the old ones (an id may move up when the mapping registered one before it),
    // and those the mapping registered.
    let ren = &x["renumbered"];
    let mut want: Vec<Value> = arr(&o, "projects").into_iter().map(|mut p| {
        if let Some(to) = ren[p["id"].as_str().unwrap_or_default()].as_str() {
            p["id"] = json!(to);
        }
        p
    }).collect();
    let got = arr(&n, "projects");
    let registered = arr(&x, "registered");
    for f in &registered {
        if let Some(p) = got.iter().find(|p| &p["path"] == f) {
            want.push(p.clone());
        }
    }
    let key = |v: &Vec<Value>| v.iter().map(|p| (p["id"].to_string(), p.clone())).collect::<BTreeMap<_, _>>();
    if key(&want) != key(&got) {
        diff.push(format!("projects:\n  expected: {}\n  after:    {}", json!(want), json!(got)));
    }
    seen.push(format!("{} projects, {} registered by the mapping", got.len(), registered.len()));

    // Messages: equal but for the conversation they are in and the appointment of the form.
    let (om, nm) = (arr(&o, "messages"), arr(&n, "messages"));
    if om.len() != nm.len() {
        diff.push(format!("messages: {} before, {} after", om.len(), nm.len()));
    }
    let (oread, nread) = (&o["read"], &n["read"]);
    let mut unread_before: BTreeMap<String, usize> = BTreeMap::new();
    let mut unread_after: BTreeMap<String, usize> = BTreeMap::new();
    for (a, b) in om.iter().zip(&nm) {
        let id = a["id"].as_str().unwrap_or_default();
        let strip = ["workspace", "since", "notice", "to"];
        if without(a, &strip) != without(b, &strip) {
            diff.push(format!("message {id} differs:\n  before: {}\n  after:  {}", without(a, &strip), without(b, &strip)));
        }
        if b["workspace"] != x["messages"][id] {
            diff.push(format!("message {id} is in {} after, the mapping says {}", b["workspace"], x["messages"][id]));
        }
        let (at, bt) = (arr(a, "to"), arr(b, "to"));
        if at.len() != bt.len() {
            diff.push(format!("message {id}: {} copies before, {} after", at.len(), bt.len()));
        }
        for (c, d) in at.iter().zip(&bt) {
            let cut = ["workspace", "since", "machine"];
            if without(c, &cut) != without(d, &cut) {
                diff.push(format!("message {id}'s copy differs:\n  before: {c}\n  after:  {d}"));
            }
            let manager = c["manager"].as_str().unwrap_or_default();
            if d["workspace"] != x["copies"][id][manager] {
                diff.push(format!("message {id}'s copy to {manager} is in {} after, the mapping says {}", d["workspace"], x["copies"][id][manager]));
            }
        }
        // Unread, per old conversation (its label) and per new one.
        let label = a["workspace"].as_str().unwrap_or_default().to_string();
        let ws = b["workspace"].as_str().unwrap_or_default().to_string();
        *unread_before.entry(label.clone()).or_default() += unread(a, oread.get(&label));
        *unread_after.entry(label).or_default() += unread(b, nread.get(&ws));
    }
    if unread_before != unread_after {
        diff.push(format!("unread, by old conversation: before {unread_before:?}, after {unread_after:?}"));
    }
    seen.push(format!("{} messages: equal but for their conversation, now a workspace id; unread by old conversation {unread_after:?}", nm.len()));

    // Read marks: only of workspaces that exist (or the conversation with every manager).
    let ids: Vec<Value> = arr(&n, "workspaces").iter().map(|w| w["id"].clone()).collect();
    for k in nread.as_object().into_iter().flat_map(|m| m.keys()) {
        if !k.is_empty() && !ids.contains(&json!(k)) {
            diff.push(format!("a read mark of {k}, which is no workspace"));
        }
    }

    // Workspaces: those the mapping makes, no seat.
    let path_of = |id: &Value| got.iter().find(|p| &p["id"] == id).map(|p| p["path"].clone()).unwrap_or(Value::Null);
    let shown: Vec<Value> = arr(&n, "workspaces").iter().map(|w| json!({ "id": w["id"], "name": w["name"], "project": path_of(&w["project"]), "closed": w["closed"].as_bool().unwrap_or(false) })).collect();
    if json!(shown) != x["workspaces"] {
        diff.push(format!("workspaces:\n  expected: {}\n  after:    {}", x["workspaces"], json!(shown)));
    }
    if let Some(w) = arr(&n, "workspaces").iter().find(|w| !w["manager"].is_null()) {
        diff.push(format!("{} has a seat: the seats start empty", w["id"]));
    }
    for w in arr(&n, "workspaces") {
        if let Some(c) = w["current"].as_str()
            && !ni.iter().any(|i| i["id"] == json!(c) && i["workspace"] == w["id"] && i["state"] == json!("active"))
        {
            diff.push(format!("{}'s current intent {c} is not active in it", w["id"]));
        }
    }
    seen.push(format!("{} workspaces, seats empty", shown.len()));

    // Choices: none open after; those open before were listed by `plan`.
    for m in &new_state.messages {
        for k in 0..=m.replies.len() {
            if new_state.choice_open(&m.id, k as u32) {
                diff.push(format!("{}'s choice {k} is still open", m.id));
            }
        }
    }
    seen.push(format!("choices open before, closed now: {}; sends not delivered before: {}", x["choices"], x["sends"]));

    // Everything else: equal.
    let rest = |v: &Value| without(v, &["intents", "projects", "messages", "read", "workspaces", "links"]);
    let (ro, rn) = (rest(&o), rest(&n));
    for k in ro.as_object().unwrap().keys().chain(rn.as_object().unwrap().keys()).collect::<std::collections::BTreeSet<_>>() {
        if ro.get(k) != rn.get(k) {
            diff.push(format!("{k} differs"));
        } else {
            seen.push(format!("{k}: equal"));
        }
    }
    let mut report: String = seen.iter().map(|l| format!("ok: {l}\n")).collect();
    if diff.is_empty() {
        report += "the check passes: every difference is one the mapping explains\n";
        Ok(report)
    } else {
        report += &diff.iter().map(|l| format!("DIFFERENT: {l}\n")).collect::<String>();
        Err(report + &format!("the check fails: {} differences no line of the mapping explains\n", diff.len()))
    }
}
