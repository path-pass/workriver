// `new write`: each line decoded as the new form; what the script wrote or changed put
// through the new rules on the state so far (with the old registrations as seats, in memory
// only); then every line stored as a row of a new, marked store.
use std::path::Path;

use serde_json::{Value, json};
use workriver_core::{Body, Event, Principal, Seat, State};

pub struct Line {
    pub at: String,
    pub actor: String,
    pub kind: String,
    pub body: Value,
    pub from: Option<i64>,
    pub check: bool,
}

pub fn lines(path: &Path) -> Result<Vec<Value>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    text.lines().filter(|l| !l.trim().is_empty()).enumerate().map(|(k, l)| serde_json::from_str(l).map_err(|e| format!("{} line {}: {e}", path.display(), k + 1))).collect()
}

pub fn events(dir: &Path) -> Result<Vec<Line>, String> {
    Ok(lines(&dir.join("events.jsonl"))?
        .into_iter()
        .map(|v| Line {
            at: v["at"].as_str().unwrap_or_default().into(),
            actor: v["actor"].as_str().unwrap_or_default().into(),
            kind: v["kind"].as_str().unwrap_or_default().into(),
            body: v["body"].clone(),
            from: v["from"].as_i64(),
            check: v["check"].as_bool().unwrap_or(false),
        })
        .collect())
}

/// Who an event's actor is, for the rules: the user, workriver, or the agent of that session
/// on the machine its `agent.seen` gives.
fn principal(actor: &str, s: &State) -> Principal {
    match actor {
        "user" => Principal::User,
        "workriver" => Principal::Service,
        a => Principal::Agent { session: a.into(), machine: s.agents.iter().find(|x| x.session == a).and_then(|x| x.machine.clone()) },
    }
}

/// The seats the old registrations gave, by the line they hold from: in the order they
/// were given, a later one over an earlier one.
fn seats_by_line(seats: &[Value]) -> Vec<(usize, Seat, String)> {
    let mut out: Vec<(usize, Seat, String)> = seats
        .iter()
        .map(|x| {
            let line = x["line"].as_u64().unwrap_or(0);
            let seat = Seat { session: x["session"].as_str().unwrap_or_default().into(), machine: x["machine"].as_str().map(String::from), folder: x["folder"].as_str().unwrap_or_default().into(), since: line };
            (line as usize, seat, x["workspace"].as_str().unwrap_or_default().to_string())
        })
        .collect();
    out.sort_by_key(|(line, ..)| *line);
    out
}

/// The validation state `v` for line `l` (from 1): the seats given before it put in (kept
/// there, so that what was asked under one appointment is answered under it), and a session
/// seated in several workspaces (a registration covered every workspace its record became)
/// holding, for `l`, the seat of the workspace `l` names.
fn seated(v: &mut State, seats: &[(usize, Seat, String)], n: usize, l: &Line) -> State {
    // The last seat given before `l` of each workspace (one made later takes it once made).
    let mut last: std::collections::BTreeMap<&str, &Seat> = std::collections::BTreeMap::new();
    for (_, seat, ws) in seats.iter().take_while(|(line, ..)| *line < n) {
        last.insert(ws, seat);
    }
    for w in v.workspaces.iter_mut() {
        if let Some(seat) = last.get(w.id.as_str()) {
            w.manager = Some((*seat).clone());
        }
    }
    let mut s = v.clone();
    let mine = |w: &workriver_core::Workspace| w.manager.as_ref().is_some_and(|m| m.session == l.actor);
    if let Some(named) = l.body["workspace"].as_str().filter(|w| s.workspaces.iter().any(|x| x.id == *w && mine(x))) {
        for w in s.workspaces.iter_mut().filter(|w| w.id != named && mine(w)) {
            w.manager = None;
        }
    }
    s
}

pub fn decode(l: &Line) -> Result<Body, String> {
    serde_json::from_value(json!({ "kind": l.kind, "body": l.body })).map_err(|e| e.to_string())
}

/// The lines with their bodies as the new form decodes them; the state they add up to; how
/// many were put through the new rules.
pub type Checked = (Vec<(Line, Body)>, State, usize);

/// Checks every line. Every failure is listed.
pub fn checked(dir: &Path) -> Result<Checked, String> {
    let all = events(dir)?;
    let seats = seats_by_line(&lines(&dir.join("seats.jsonl"))?);
    // `s`: what the product replays (no seats: they start empty). `v`: the same with the
    // old registrations as seats, given at their lines and kept, which the rules are asked
    // on; a message said or sent then belongs to that appointment there, as it did.
    let mut s = State::default();
    let mut v = State::default();
    let mut out = vec![];
    let mut failed = vec![];
    let mut n_checked = 0;
    for (k, l) in all.into_iter().enumerate() {
        let n = k + 1;
        let what = || format!("line {n} ({} by {} at {}{})", l.kind, l.actor, l.at, l.from.map_or(String::new(), |f| format!(", old event {f}")));
        let body = match decode(&l) {
            Ok(b) => b,
            Err(e) => {
                failed.push(format!("{}: does not decode in the new form: {e}", what()));
                continue;
            }
        };
        let asked = seated(&mut v, &seats, n, &l);
        if l.check {
            n_checked += 1;
            match asked.prepare(&principal(&l.actor, &s), body.clone()) {
                Err(r) => failed.push(format!("{}: refused: {r}", what())),
                Ok(b) if b != body => failed.push(format!("{}: comes out different:\n  written:  {}\n  prepared: {}", what(), body_json(&body), body_json(&b))),
                Ok(_) => {}
            }
        }
        let e = Event { seq: n as i64, at: l.at.clone(), actor: l.actor.clone(), body: body.clone() };
        s.apply(&e);
        v.apply(&e);
        out.push((l, body));
    }
    if !failed.is_empty() {
        return Err(failed.join("\n"));
    }
    Ok((out, s, n_checked))
}

fn body_json(b: &Body) -> String {
    serde_json::to_value(b).map(|v| v["body"].to_string()).unwrap_or_default()
}

pub fn write(dir: &Path, out: &Path) -> Result<String, String> {
    crate::new_path(out)?;
    let (rows, state, n_checked) = checked(dir).map_err(|e| format!("nothing was written:\n{e}"))?;
    let done = store(out, &rows).and_then(|_| {
        // The product's own replay of what was written.
        let s = workriver_data::Store::open(out).map_err(|e| e.to_string())?;
        if serde_json::to_value(s.state()).unwrap() != serde_json::to_value(&state).unwrap() {
            return Err("the store replays to another state than the lines did".into());
        }
        Ok(())
    });
    if let Err(e) = done {
        for f in [out.to_path_buf(), out.with_extension("db-wal"), out.with_extension("db-shm")] {
            let _ = std::fs::remove_file(f);
        }
        return Err(format!("{}: {e}; removed", out.display()));
    }
    Ok(format!("{} events written to {} ({} put through the new rules, {} kept as they were); it replays\n", rows.len(), out.display(), n_checked, rows.len() - n_checked))
}

/// A new store, marked by `Store::open`, and the rows as workriver-data writes them, in one
/// transaction. The one write of the events table outside workriver-data; it goes with the
/// script.
fn store(out: &Path, rows: &[(Line, Body)]) -> Result<(), String> {
    drop(workriver_data::Store::open(out).map_err(|e| e.to_string())?);
    let mut conn = rusqlite::Connection::open(out).map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for (l, body) in rows {
        let (intent, unit, task) = body.refs();
        let json = serde_json::to_value(body).expect("events serialize");
        tx.execute(
            "INSERT INTO events (at, kind, actor, intent, unit, task, body) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            (&l.at, json["kind"].as_str(), &l.actor, intent, unit, task, json["body"].to_string()),
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}
