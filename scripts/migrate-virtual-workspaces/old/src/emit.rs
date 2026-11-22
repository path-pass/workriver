// From the analysis and the mapping: the new events in order (`events.jsonl`), the old
// registrations as seats (`seats.jsonl`, for the check only) and what the check compares
// (`old-state.json`).
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use workriver_core::{DeliveryStep, IntentState, State};

use crate::analyse::{Analysis, Place, Step};
use crate::mapping::{Folder, Key, Mapping, Ws};

/// A workspace to be: an old record or a label that found none, and a project's folder.
pub type WsKey = (Key, String);

/// What the old records and the mapping say, read in one place: projects as folders.
pub struct Resolver<'a> {
    pub a: &'a Analysis,
    pub folders: BTreeMap<String, Folder>,
    pub workspaces: BTreeMap<WsKey, Ws>,
    pub settled: BTreeMap<String, String>,
}

impl<'a> Resolver<'a> {
    /// The resolver of a mapping (projects named by id or folder made folders).
    pub fn new(a: &'a Analysis, m: &Mapping) -> Result<Resolver<'a>, String> {
        let mut r = Resolver { a, folders: BTreeMap::new(), workspaces: BTreeMap::new(), settled: BTreeMap::new() };
        for (f, d) in &m.folders {
            if !a.folders.contains_key(f) {
                return Err(format!("mapping: folder {f} is not one that needs a line"));
            }
            r.folders.insert(f.clone(), d.clone());
        }
        for (f, d) in &m.folders {
            if let Folder::Into(p) = d {
                let to = r.project(p)?;
                r.folders.insert(f.clone(), Folder::Into(to));
            }
        }
        for ((k, p), ws) in &m.workspaces {
            r.workspaces.insert((k.clone(), r.project(p)?), ws.clone());
        }
        for (k, p) in &m.settled {
            r.settled.insert(k.clone(), r.project(p)?);
        }
        Ok(r)
    }

    /// A project as the mapping names it: a registered one's id or folder, or a folder the
    /// mapping registers.
    pub fn project(&self, p: &str) -> Result<String, String> {
        let s = &self.a.state;
        if let Some(x) = s.projects.iter().find(|x| x.id == p || x.path == p) {
            return Ok(x.path.clone());
        }
        if self.folders.get(p) == Some(&Folder::Register) {
            return Ok(p.to_string());
        }
        Err(format!("mapping: {p} is no registered project (nor a folder the mapping registers)"))
    }

    /// The project an intent's folder (or a project_set's) is of, after the folder lines.
    pub fn folder_project(&self, f: &str) -> Result<String, String> {
        if f.is_empty() {
            return Ok(String::new());
        }
        let s = &self.a.state;
        if let Some(p) = s.projects.iter().find(|p| p.id == f) {
            return Ok(p.path.clone());
        }
        match self.folders.get(f) {
            Some(Folder::Register) => Ok(f.to_string()),
            Some(Folder::Into(p)) => Ok(p.clone()),
            Some(Folder::None) => Ok(String::new()),
            None => s.project_of(f).map(|p| p.path.clone()).ok_or_else(|| format!("folder {} = ?", crate::mapping::quoted(f))),
        }
    }

    pub fn intent_project(&self, intent: &str) -> Result<String, String> {
        let i = self.a.state.intents.iter().find(|i| i.id == intent).ok_or_else(|| format!("no intent {intent}"))?;
        self.folder_project(&i.project)
    }

    /// The projects of a record's or label's workspaces: its intents' (a record of several
    /// is split); else its project_set's; else the mapping's lines for it.
    pub fn projects(&self, k: &Key) -> Result<Vec<String>, String> {
        let intents: Vec<String> = match k {
            Key::Record(r) => self.a.records[*r].intents.clone(),
            Key::Label(l) => self.a.orphans.get(l).into_iter().flatten().filter_map(|m| self.a.state.messages.iter().find(|x| &x.id == m)).map(|m| m.intent.clone()).filter(|i| !i.is_empty()).collect(),
        };
        let mut out: Vec<String> = vec![];
        for i in &intents {
            let p = self.intent_project(i)?;
            if !p.is_empty() && !out.contains(&p) {
                out.push(p);
            }
        }
        if out.is_empty()
            && let Key::Record(r) = k
            && let Some(f) = &self.a.records[*r].project_set
        {
            let p = self.folder_project(f)?;
            if !p.is_empty() {
                out.push(p);
            }
        }
        if out.is_empty() {
            out = self.workspaces.keys().filter(|(x, _)| x == k).map(|(_, p)| p.clone()).collect();
        }
        Ok(out)
    }

    /// The workspace something of `place` about `intent` (may be empty) goes to; `settle` is
    /// the mapping's key when the place's projects do not tell.
    pub fn ws_for(&self, place: &Place, intent: &str, settle: &str) -> Result<Option<WsKey>, String> {
        let k = match place {
            Place::Every => return Ok(None),
            Place::Record(r) => Key::Record(*r),
            Place::Label(l) => Key::Label(l.clone()),
        };
        if !intent.is_empty() {
            let p = self.intent_project(intent)?;
            if !p.is_empty() {
                return Ok(Some((k, p)));
            }
        }
        let ps = self.projects(&k)?;
        if ps.len() == 1 {
            return Ok(Some((k, ps[0].clone())));
        }
        if let Some(p) = self.settled.get(settle) {
            return Ok(Some((k, p.clone())));
        }
        Err(match (ps.is_empty(), &k) {
            (true, Key::Record(r)) => format!("record {r} ? = name {}, open", crate::mapping::quoted(&self.name(&k))),
            (true, Key::Label(l)) => format!("label {} ? = name {}, closed", crate::mapping::quoted(l), crate::mapping::quoted(l)),
            (false, _) => format!("{settle} = ?"),
        })
    }

    /// A record's or label's name by default: the last label it carried.
    pub fn name(&self, k: &Key) -> String {
        match k {
            Key::Record(r) => self.a.state.workspaces[*r].group.label.clone(),
            Key::Label(l) => l.clone(),
        }
    }
}

/// A line of `events.jsonl`. `body` is JSON text: as stored for an event kept byte for byte.
/// `check`: written or changed by the script, so it passes the new rules in `new write`.
fn line(at: &str, actor: &str, kind: &str, body: &str, from: Option<i64>, check: bool) -> String {
    let s = |x: &str| serde_json::to_string(x).unwrap();
    let mut l = format!("{{\"at\":{},\"actor\":{},\"kind\":{},\"body\":{}", s(at), s(actor), s(kind), body);
    if let Some(f) = from {
        l += &format!(",\"from\":{f}");
    }
    if check {
        l += ",\"check\":true";
    }
    l + "}"
}

pub struct Out {
    pub events: Vec<String>,
    pub seats: Vec<String>,
    pub expected: Value,
    /// What happened, for the user: workspaces made, events dropped by reason.
    pub report: Vec<String>,
}

struct Emit<'r, 'a> {
    r: &'r Resolver<'a>,
    lines: Vec<String>,
    /// The folders registered so far in the output, in order: `p<k+1>`.
    projects: Vec<String>,
    ws: Vec<(WsKey, String)>,
    /// Each workspace's name as made.
    names: BTreeMap<String, String>,
    /// Seats of workspaces made for a label: the manager its messages went to.
    label_seats: Vec<(usize, String, String)>,
    pending: Vec<(WsKey, String, String)>,
    placed: BTreeMap<String, String>,
    seat_now: BTreeMap<usize, (String, Option<String>, usize)>,
}

impl Emit<'_, '_> {
    fn pid(&self, folder: &str) -> String {
        format!("p{}", self.projects.iter().position(|p| p == folder).unwrap() + 1)
    }

    fn id_of(&self, k: &WsKey) -> Option<String> {
        self.ws.iter().find(|(x, _)| x == k).map(|(_, id)| id.clone())
    }

    fn meta(&self, k: &WsKey) -> Ws {
        self.r.workspaces.get(k).cloned().unwrap_or_else(|| Ws { name: self.r.name(&k.0), open: self.active_in(k) })
    }

    /// Whether an intent put in this workspace is active at the end.
    fn active_in(&self, k: &WsKey) -> bool {
        let Key::Record(r) = &k.0 else { return false };
        self.r.a.records[*r].intents.iter().any(|i| {
            let st = &self.r.a.state;
            st.intents.iter().any(|x| &x.id == i && x.state == IntentState::Active && x.workspace.as_ref().and_then(|g| st.workspace_of(g)).is_some_and(|w| std::ptr::eq(w, &st.workspaces[*r])))
                && self.r.intent_project(i).is_ok_and(|p| p == k.1 || (p.is_empty() && self.r.projects(&k.0).is_ok_and(|ps| ps.len() == 1)))
        })
    }

    /// The workspace's id, made now when its project is registered by now.
    fn ensure(&mut self, k: &WsKey, at: &str) -> Option<String> {
        if let Some(id) = self.id_of(k) {
            return Some(id);
        }
        if !self.projects.contains(&k.1) {
            return None;
        }
        let id = format!("ws{}", self.ws.len() + 1);
        let name = self.meta(k).name;
        let body = json!({ "id": id, "name": name, "project": self.pid(&k.1) });
        self.names.insert(id.clone(), name);
        self.lines.push(line(at, "user", "workspace.created", &body.to_string(), None, true));
        self.ws.push((k.clone(), id.clone()));
        Some(id)
    }

    fn place(&mut self, intent: &str, id: &str, at: &str) {
        if self.placed.get(intent).map(String::as_str) != Some(id) {
            let body = json!({ "intent": intent, "workspace": id });
            self.lines.push(line(at, "user", "intent.workspace_set", &body.to_string(), None, true));
            self.placed.insert(intent.to_string(), id.to_string());
        }
    }

    /// After a project is registered: the placements that waited for it.
    fn flush(&mut self, at: &str) {
        let ready: Vec<_> = self.pending.iter().filter(|(k, _, _)| self.projects.contains(&k.1)).cloned().collect();
        self.pending.retain(|(k, _, _)| !self.projects.contains(&k.1));
        for (k, intent, _) in ready {
            let id = self.ensure(&k, at).unwrap();
            self.place(&intent, &id, at);
        }
    }

    /// Workspace ids for a place: a record of several projects is one per project.
    fn all_of(&mut self, p: &Place, at: &str) -> Result<Vec<String>, String> {
        let k = match p {
            Place::Record(r) => Key::Record(*r),
            Place::Label(l) => Key::Label(l.clone()),
            Place::Every => return Ok(vec![String::new()]),
        };
        let mut out = vec![];
        for proj in self.r.projects(&k)? {
            if let Some(id) = self.ensure(&(k.clone(), proj), at) {
                out.push(id);
            }
        }
        Ok(out)
    }
}

pub fn emit(r: &Resolver) -> Result<Out, String> {
    let a = r.a;
    let mut e = Emit { r, lines: vec![], projects: vec![], ws: vec![], names: BTreeMap::new(), label_seats: vec![], pending: vec![], placed: BTreeMap::new(), seat_now: BTreeMap::new() };
    let mut errors: Vec<String> = vec![];
    let mut dropped: BTreeMap<String, usize> = BTreeMap::new();
    let mut seat_log: Vec<(usize, usize, String, Option<String>)> = vec![];
    let mut renumbered: BTreeMap<String, String> = BTreeMap::new();
    let mut registered: Vec<String> = vec![];
    let first_naming: BTreeMap<usize, Vec<String>> = r.folders.iter().filter(|(_, d)| **d == Folder::Register).fold(BTreeMap::new(), |mut m, (f, _)| {
        m.entry(a.folders[f][0].event).or_insert_with(Vec::new).push(f.clone());
        m
    });
    let mut last_at = String::new();
    for (k, (o, step)) in a.events.iter().zip(&a.steps).enumerate() {
        let (at, actor, seq) = (o.event.at.as_str(), o.event.actor.as_str(), Some(o.event.seq));
        last_at = at.to_string();
        let mut fail = |what: String| errors.push(format!("event {} ({} at {at}): {what}", o.event.seq, o.kind));
        // A folder the mapping registers: the user's act, before the first event naming it.
        for f in first_naming.get(&k).into_iter().flatten() {
            let id = format!("p{}", e.projects.len() + 1);
            let title = std::path::Path::new(f).file_name().map_or(f.clone(), |n| n.to_string_lossy().to_string());
            e.lines.push(line(at, "user", "project.added", &json!({ "id": id, "path": f, "title": title }).to_string(), None, true));
            e.projects.push(f.clone());
            registered.push(f.clone());
            e.flush(at);
        }
        let mut body: Value = serde_json::from_str(&o.body).unwrap();
        let intent = body["intent"].as_str().or(body["id"].as_str()).unwrap_or_default().to_string();
        match step {
            Step::Keep => match o.kind.as_str() {
                "project.added" => {
                    let path = body["path"].as_str().unwrap_or_default().to_string();
                    let id = format!("p{}", e.projects.len() + 1);
                    let changed = body["id"] != json!(id);
                    if changed {
                        renumbered.insert(body["id"].as_str().unwrap_or_default().to_string(), id.clone());
                        body["id"] = json!(id);
                    }
                    let text = if changed { body.to_string() } else { o.body.clone() };
                    e.lines.push(line(at, actor, &o.kind, &text, seq, changed));
                    e.projects.push(path);
                    e.flush(at);
                }
                "project.renamed" if body["project"].as_str().is_some_and(|p| renumbered.contains_key(p)) => {
                    body["project"] = json!(renumbered[body["project"].as_str().unwrap()]);
                    e.lines.push(line(at, actor, &o.kind, &body.to_string(), seq, true));
                }
                _ => {
                    if o.kind == "intent.state_changed" && e.pending.iter().any(|(_, i, _)| *i == intent) {
                        fail(format!("{intent} ends before the project of its workspace is registered; it cannot be placed there"));
                    }
                    e.lines.push(line(at, actor, &o.kind, &o.body, seq, false));
                }
            },
            Step::Drop(why) => *dropped.entry(format!("{} ({why})", o.kind)).or_default() += 1,
            Step::Created { record } => {
                let mut changed = false;
                let folder = body["project"].as_str().unwrap_or_default().to_string();
                match r.folders.get(&folder) {
                    Some(Folder::Into(p)) => {
                        body["project"] = json!(p);
                        changed = true;
                    }
                    Some(Folder::None) => {
                        body.as_object_mut().unwrap().remove("project");
                        changed = true;
                    }
                    _ => {}
                }
                // The new form has no runtime group on an intent: the field goes, nothing else.
                let had = body.as_object_mut().unwrap().remove("runtime_ref").is_some();
                let text = if changed || had { body.to_string() } else { o.body.clone() };
                e.lines.push(line(at, actor, &o.kind, &text, seq, changed));
                if let Some(rec) = record {
                    match r.ws_for(&Place::Record(*rec), &intent, &format!("intent {intent}")) {
                        Ok(Some(key)) => match e.ensure(&key, at) {
                            Some(id) => e.place(&intent, &id, at),
                            None => e.pending.push((key, intent.clone(), at.to_string())),
                        },
                        Ok(None) => {}
                        Err(x) => fail(x),
                    }
                }
            }
            Step::Start { record } => match r.ws_for(&Place::Record(*record), &intent, &format!("intent {intent}")) {
                Ok(Some(key)) => match e.ensure(&key, at) {
                    Some(id) => {
                        e.pending.retain(|(_, i, _)| *i != intent);
                        body["workspace"] = json!(id);
                        e.lines.push(line(at, actor, &o.kind, &body.to_string(), seq, true));
                        e.placed.insert(intent.clone(), id);
                    }
                    None => fail(format!("{intent} starts in a workspace of project {}, which is registered only later", key.1)),
                },
                Ok(None) => {}
                Err(x) => fail(x),
            },
            Step::Unplace => {
                body.as_object_mut().unwrap().remove("workspace");
                e.lines.push(line(at, actor, &o.kind, &body.to_string(), seq, true));
            }
            Step::Linked { active: false, .. } => *dropped.entry(format!("{} (of an intent not active)", o.kind)).or_default() += 1,
            Step::Linked { record, active: true } => match r.ws_for(&Place::Record(*record), &intent, &format!("intent {intent}")) {
                Ok(Some(key)) => match e.ensure(&key, at) {
                    Some(id) if e.placed.get(&intent) == Some(&id) => *dropped.entry(format!("{} (to the workspace it is in)", o.kind)).or_default() += 1,
                    Some(id) => e.place(&intent, &id, at),
                    None => {
                        e.pending.retain(|(_, i, _)| *i != intent);
                        e.pending.push((key, intent.clone(), at.to_string()));
                    }
                },
                Ok(None) => {}
                Err(x) => fail(x),
            },
            Step::Sent { place, copies } => {
                let id = body["id"].as_str().unwrap_or_default().to_string();
                match r.ws_for(place, body["intent"].as_str().unwrap_or_default(), &format!("message {id}")) {
                    Ok(None) => body["workspace"] = json!(""),
                    Ok(Some(key)) => match e.ensure(&key, at) {
                        Some(w) => {
                            // A conversation no record carried has no registration: for the
                            // check, the manager the message went to holds its seat.
                            if let (Key::Label(_), Some(m)) = (&key.0, body["manager"].as_str()) {
                                e.label_seats.push((e.lines.len(), w.clone(), m.to_string()));
                            }
                            body["workspace"] = json!(w);
                        }
                        None => fail(format!("message {id} goes to a workspace of project {}, which is registered only later", key.1)),
                    },
                    Err(x) => fail(x),
                }
                for (n, (session, p)) in copies.iter().enumerate() {
                    let Place::Record(rec) = p else {
                        fail(format!("message {id}'s copy to {session} went to a group with no record"));
                        continue;
                    };
                    let w = match r.ws_for(p, "", &format!("message {id}")) {
                        Ok(Some(key)) => e.ensure(&key, at),
                        Ok(None) => None,
                        Err(x) => {
                            fail(x);
                            continue;
                        }
                    };
                    let (Some(w), Some((s, machine, since))) = (w, e.seat_now.get(rec).cloned()) else {
                        fail(format!("message {id}'s copy to {session}: its workspace has no seat or no project registered then"));
                        continue;
                    };
                    if s != *session {
                        fail(format!("message {id}'s copy went to {session}, but the seat was {s}'s"));
                    }
                    let c = &mut body["to"][n];
                    c["workspace"] = json!(w);
                    c["since"] = json!(since);
                    match machine {
                        Some(m) => c["machine"] = json!(m),
                        None => {
                            c.as_object_mut().unwrap().remove("machine");
                        }
                    }
                }
                e.lines.push(line(at, actor, &o.kind, &body.to_string(), seq, true));
            }
            Step::Said { place } => {
                let id = body["id"].as_str().unwrap_or_default().to_string();
                match r.ws_for(place, body["intent"].as_str().unwrap_or_default(), &format!("message {id}")).map(|k| k.and_then(|k| e.ensure(&k, at))) {
                    Ok(Some(w)) => {
                        body["workspace"] = json!(w);
                        e.lines.push(line(at, actor, &o.kind, &body.to_string(), seq, true));
                    }
                    Ok(None) => fail(format!("message {id}'s workspace is not registered by then")),
                    Err(x) => fail(x),
                }
            }
            Step::Read { places } => {
                let mut ids: Vec<String> = vec![];
                for p in places {
                    match e.all_of(p, at) {
                        Ok(x) => ids.extend(x),
                        Err(x) => fail(x),
                    }
                }
                let mut seen = BTreeSet::new();
                ids.retain(|i| seen.insert(i.clone()));
                if ids.is_empty() {
                    *dropped.entry(format!("{} (of no conversation)", o.kind)).or_default() += 1;
                }
                for w in ids {
                    body["workspace"] = json!(w);
                    e.lines.push(line(at, actor, &o.kind, &body.to_string(), seq, true));
                }
            }
        }
        for s in a.seats.iter().filter(|s| s.after == k) {
            e.seat_now.insert(s.record, (s.session.clone(), s.machine.clone(), e.lines.len()));
            seat_log.push((e.lines.len(), s.record, s.session.clone(), s.machine.clone()));
        }
    }
    for (k, intent, at) in &e.pending {
        errors.push(format!("{intent}, placed at {at}: the project of its workspace ({}) is never registered", k.1));
    }
    // A registered project with no workspace: one, named as the project.
    let st = &a.state;
    for p in e.projects.clone() {
        if !e.ws.iter().any(|((_, x), _)| *x == p) {
            let title = st.projects.iter().find(|x| x.path == p).map_or_else(|| std::path::Path::new(&p).file_name().map_or(p.clone(), |n| n.to_string_lossy().to_string()), |x| x.title.clone());
            let id = format!("ws{}", e.ws.len() + 1);
            e.lines.push(line(&last_at, "user", "workspace.created", &json!({ "id": id, "name": title, "project": e.pid(&p) }).to_string(), None, true));
            e.names.insert(id.clone(), title);
            e.ws.push(((Key::Label(String::new()), p), id));
        }
    }
    // The mapping's lines all name a workspace that is made.
    for k in r.workspaces.keys() {
        if !e.ws.iter().any(|(x, _)| x == k) {
            errors.push(format!("the mapping's line for {:?} of project {} names no workspace the old records make", k.0, k.1));
        }
    }
    let mut closed = vec![];
    for (k, id) in e.ws.clone() {
        if k.0 == Key::Label(String::new()) || e.meta(&k).open {
            continue;
        }
        if let Some((i, _)) = e.placed.iter().find(|(i, w)| **w == id && st.intents.iter().any(|x| &x.id == *i && x.state == IntentState::Active)) {
            errors.push(format!("{id} ({:?}) is to be closed, but {i} is active in it: write it open", k.0));
            continue;
        }
        e.lines.push(line(&last_at, "user", "workspace.closed", &json!({ "workspace": id }).to_string(), None, true));
        closed.push(id);
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    // The seats, for the check only: each record's registered session, on each of its workspaces.
    let mut seats = vec![];
    for (at, rec, session, machine) in seat_log {
        for ((k, _), id) in &e.ws {
            if *k == Key::Record(rec) {
                let folder = st.agents.iter().find(|x| x.session == session).map(|x| x.cwd.clone()).unwrap_or_default();
                seats.push(json!({ "line": at, "workspace": id, "session": session, "machine": machine, "folder": folder }).to_string());
            }
        }
    }
    for (at, id, session) in &e.label_seats {
        seats.push(json!({ "line": at, "workspace": id, "session": session, "machine": null, "folder": "" }).to_string());
    }
    let expected = expect(r, &e, &registered, &renumbered)?;
    let mut report = vec![format!("{} events in, {} out", a.events.len(), e.lines.len())];
    for (k, id) in &e.ws {
        let from = match &k.0 {
            Key::Record(n) => format!("record {n} ({})", describe(st, *n)),
            Key::Label(l) if l.is_empty() => "a project with no workspace".to_string(),
            Key::Label(l) => format!("label {l:?}, which no record carried"),
        };
        report.push(format!("{id} {:?} in {} from {from}{}", e.names[id], k.1, if closed.contains(id) { ", closed" } else { "" }));
    }
    for f in &registered {
        report.push(format!("registered by the mapping: {f}"));
    }
    for (old, new) in &renumbered {
        report.push(format!("project {old} is {new} now"));
    }
    for (what, n) in dropped {
        report.push(format!("dropped: {n} {what}"));
    }
    Ok(Out { events: e.lines, seats, expected, report })
}

/// A record as the plan shows it: machine, the runtime's id, its labels.
pub fn describe(s: &State, r: usize) -> String {
    let g = &s.workspaces[r].group;
    format!("{}, runtime id {}, last label {:?}", g.machine.as_deref().map_or("this machine".to_string(), |m| format!("machine {m}")), g.id, g.label)
}

/// What the check compares the new store's state with.
fn expect(r: &Resolver, e: &Emit, registered: &[String], renumbered: &BTreeMap<String, String>) -> Result<Value, String> {
    let a = r.a;
    let st = &a.state;
    let id_of = |k: Option<WsKey>| k.and_then(|k| e.id_of(&k));
    let ws_project = |id: &str| e.ws.iter().find(|(_, x)| x == id).map(|((_, p), _)| p.clone());
    let mut intents = serde_json::Map::new();
    let mut projects = serde_json::Map::new();
    for i in &st.intents {
        let ws = match (i.state, a.placed.get(&i.id).copied()) {
            (IntentState::Planned, _) | (_, None) => None,
            (_, Some(n)) => id_of(r.ws_for(&Place::Record(n), &i.id, &format!("intent {}", i.id))?),
        };
        let mut p = r.intent_project(&i.id)?;
        if p.is_empty()
            && let Some(w) = &ws
        {
            p = ws_project(w).unwrap_or_default();
        }
        intents.insert(i.id.clone(), json!(ws));
        projects.insert(i.id.clone(), json!(p));
    }
    let mut messages = serde_json::Map::new();
    let mut copies = serde_json::Map::new();
    for m in &st.messages {
        let place = a.messages.get(&m.id).cloned().unwrap_or(Place::Every);
        messages.insert(m.id.clone(), json!(id_of(r.ws_for(&place, &m.intent, &format!("message {}", m.id))?).unwrap_or_default()));
        if !m.to.is_empty() {
            let mut c = serde_json::Map::new();
            for x in &m.to {
                let rec = st.workspace_of(&x.workspace).and_then(|w| st.workspaces.iter().position(|y| std::ptr::eq(y, w)));
                let w = match rec {
                    Some(n) => id_of(r.ws_for(&Place::Record(n), "", &format!("message {}", m.id))?),
                    None => None,
                };
                c.insert(x.manager.clone(), json!(w));
            }
            copies.insert(m.id.clone(), Value::Object(c));
        }
    }
    let mut choices = vec![];
    let mut sends = vec![];
    for m in &st.messages {
        for (k, c) in std::iter::once(m.choice.as_ref()).chain(m.replies.iter().map(|x| x.choice.as_ref())).enumerate() {
            if c.is_some() && st.choice_open(&m.id, k as u32) {
                choices.push(json!({ "message": m.id, "reply": k }));
            }
        }
        let waiting = |d: &Option<workriver_core::Delivery>| d.as_ref().is_none_or(|d| d.step == DeliveryStep::Held);
        if !m.by_manager && m.to.is_empty() && waiting(&m.delivery) {
            sends.push(json!({ "message": m.id, "manager": m.manager }));
        }
        for x in m.to.iter().filter(|x| waiting(&x.delivery)) {
            sends.push(json!({ "message": m.id, "manager": x.manager }));
        }
    }
    let workspaces: Vec<Value> = e.ws.iter().map(|(k, id)| json!({ "id": id, "name": e.names[id], "project": k.1, "closed": k.0 != Key::Label(String::new()) && !e.meta(k).open })).collect();
    Ok(json!({
        "workspaces": workspaces,
        "intents": intents,
        "intent_projects": projects,
        "messages": messages,
        "copies": copies,
        "registered": registered,
        "renumbered": renumbered,
        "choices": choices,
        "sends": sends,
    }))
}
