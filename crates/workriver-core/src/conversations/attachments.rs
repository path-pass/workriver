// Attachments: the files put in the store, where they are linked, who holds them, and when
// the attachment rule may take them out of the store (deleted, or moved to a target).
use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::*;

/// The largest file the store takes, and so the largest size a record may say a file has.
pub const MAX_ATTACHMENT: usize = 50 * 1024 * 1024;

/// A stored file. What is recorded of it stays for good; its bytes may leave the store under
/// the user's attachment rule (`gone`).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub mime: String,
    pub size: u64,
    pub by: String,
    pub at: String,
    /// The user stored it (first, or the same content again): a question of theirs may point at it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub user: bool,
    /// Its bytes left the store: when, how and where to. Cleared only when the service
    /// stores them again (`attachment.restored`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gone: Option<Gone>,
    /// When it was last stored or named again (an `attachment.added` for the same content,
    /// after the first): its clock under the attachment rule starts again from it. None: only
    /// once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<String>,
}

/// Where a moved file went: the target as the settings gave it, its kind and the kind's own
/// fields (a folder's `path`). Core keeps it as it is and reads nothing of it but that it has
/// a kind; what a kind means is the service's.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Target {
    pub kind: String,
    #[serde(flatten)]
    pub rest: serde_json::Map<String, Value>,
}

/// How a file left the store.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Removed {
    Deleted,
    Moved,
}

/// A file whose bytes left the store.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Gone {
    pub at: String,
    pub how: Removed,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<Target>,
}

/// The conditions of the attachment rule, as recorded with a removal: `days` after the work
/// a file belongs to is closed, all files or pictures only, only those over `minMb` (0: any
/// size). What happens then (kept, deleted, moved) and where to are the service's settings.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Conditions {
    pub days: u32,
    pub only: Only,
    #[serde(rename = "minMb")]
    pub min_mb: u32,
}

/// Which files a rule takes.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Only {
    All,
    /// `image/*` by the recorded type.
    Pictures,
}

impl Conditions {
    /// Whether the rule takes this file, by its type and size (not by time).
    pub fn takes(&self, a: &Attachment) -> bool {
        (self.only == Only::All || a.mime.starts_with("image/")) && a.size > u64::from(self.min_mb) * 1024 * 1024
    }
}

/// A record naming a stored file: which file, how, who holds it through that record and the
/// record's own time.
#[derive(Clone, PartialEq, Debug)]
pub struct Held<'a> {
    pub id: &'a str,
    pub how: Named,
    pub by: Holder,
    pub at: &'a str,
}

/// How a record names a file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Named {
    /// As an attachment: a task's files, a check's report, a finding's, a note's, what a
    /// question or a message points at. What the blind view goes by.
    Attached,
    /// Its id written in any text of the record (a path, the fetch command, the page's link).
    InText,
}

/// Who holds a file through a record.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Holder {
    /// Closed when the intent is done or dropped.
    Intent(String),
    /// Closed when no intent is active in the workspace.
    Workspace(String),
    /// A record that belongs to no intent: closed when it is itself. It has no closing time:
    /// once closed, the file's clock is its own time.
    Record { open: bool },
}

/// A record that can hold a file: who holds through it, its own time, what it carries as
/// attachments, and the record as the state serializes it (made only when texts are read).
struct Holding<'a> {
    by: Vec<Holder>,
    at: &'a str,
    attached: Vec<&'a String>,
    text: Box<dyn Fn() -> Value + 'a>,
}

fn serialized<T: Serialize>(t: &T) -> Box<dyn Fn() -> Value + '_> {
    Box::new(move || serde_json::to_value(t).unwrap_or(Value::Null))
}

/// Every run of exactly 16 lowercase hex characters in the strings (and keys) of `v`.
fn hex_ids<'v>(v: &'v Value, out: &mut Vec<&'v str>) {
    fn runs<'s>(s: &'s str, out: &mut Vec<&'s str>) {
        let hex = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
        let bytes = s.as_bytes();
        let mut k = 0;
        while k < bytes.len() {
            if !hex(bytes[k]) {
                k += 1;
                continue;
            }
            let start = k;
            while k < bytes.len() && hex(bytes[k]) {
                k += 1;
            }
            if k - start == 16 {
                out.push(&s[start..k]);
            }
        }
    }
    match v {
        Value::String(s) => runs(s, out),
        Value::Array(a) => a.iter().for_each(|x| hex_ids(x, out)),
        Value::Object(o) => o.iter().for_each(|(k, x)| {
            runs(k, out);
            hex_ids(x, out)
        }),
        _ => {}
    }
}

fn parsed(at: &str) -> Option<SystemTime> {
    humantime::parse_rfc3339_weak(at).ok()
}

/// Whether `id` is the store's own form of an attachment id: 16 lowercase hex characters, a
/// short hash of the content (the service makes it). Its file's path is built from it, so
/// nothing else is ever recorded as one.
pub fn attachment_id(id: &str) -> bool {
    id.len() == 16 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl State {
    /// The one walk of the records that can hold a file (docs: the attachment rule's table).
    /// A file is held by a task, its findings, notes, checks and deliveries, and its unit, for
    /// the unit's intent; by an intent's own texts; by a question for its unit's intent, or
    /// (no unit) until it is answered or failed and its discussion ended; by a discussion's
    /// conclusion for its unit's intent; by a message for the intent it names, else for its
    /// workspace (one to every manager: each workspace it went to); by a chore for its intent,
    /// or (none) until it is done; by a backlog finding for the intent it was batched for, or
    /// (none) until it is dropped.
    fn holding(&self) -> Vec<Holding<'_>> {
        fn finding(f: &Finding) -> impl Iterator<Item = &String> {
            f.found.attachments.iter().chain(f.notes.iter().flat_map(|n| &n.attachments))
        }
        fn pointed(c: &[AskContext]) -> Vec<&String> {
            c.iter().filter(|c| c.kind == "attachment").filter_map(|c| c.id.as_ref()).collect()
        }
        let unit_intent = |unit: &str| self.units.iter().find(|u| u.id == unit).map(|u| Holder::Intent(u.intent.clone()));
        let mut out = vec![];
        for i in &self.intents {
            out.push(Holding { by: vec![Holder::Intent(i.id.clone())], at: &i.at, attached: vec![], text: serialized(i) });
        }
        for u in &self.units {
            let attached = u.tasks.iter().flat_map(|t| t.attachments.iter().chain(t.evidence.iter().filter_map(|e| e.attachment.as_ref())).chain(t.notes.iter().flat_map(|n| &n.attachments)).chain(t.findings.iter().flat_map(finding))).collect();
            out.push(Holding { by: vec![Holder::Intent(u.intent.clone())], at: &u.at, attached, text: serialized(u) });
        }
        for q in &self.questions {
            let discussing = q.discuss && self.threads.iter().any(|t| t.id == q.thread && !t.ended);
            let by = unit_intent(&q.unit).unwrap_or(Holder::Record { open: q.open() || discussing });
            out.push(Holding { by: vec![by], at: &q.at, attached: pointed(&q.context), text: serialized(q) });
        }
        for t in &self.threads {
            let by = unit_intent(&t.unit).unwrap_or(Holder::Record { open: !t.ended });
            out.push(Holding { by: vec![by], at: "", attached: vec![], text: serialized(t) });
        }
        for m in &self.messages {
            let by = if !m.intent.is_empty() {
                vec![Holder::Intent(m.intent.clone())]
            } else if !m.to.is_empty() {
                m.to.iter().map(|r| Holder::Workspace(r.workspace.clone())).collect()
            } else if !m.workspace.is_empty() {
                vec![Holder::Workspace(m.workspace.clone())]
            } else {
                vec![Holder::Record { open: true }]
            };
            out.push(Holding { by, at: &m.at, attached: pointed(&m.context), text: serialized(m) });
        }
        for c in &self.chores {
            let by = if c.intent.is_empty() { Holder::Record { open: !c.done } } else { Holder::Intent(c.intent.clone()) };
            out.push(Holding { by: vec![by], at: &c.at, attached: vec![], text: serialized(c) });
        }
        for b in &self.backlog {
            let by = match &b.intent {
                Some(i) => Holder::Intent(i.clone()),
                None => Holder::Record { open: b.finding.state != FindingState::Dropped },
            };
            out.push(Holding { by: vec![by], at: &b.at, attached: finding(&b.finding).collect(), text: serialized(b) });
        }
        out
    }

    /// Who holds which stored file, and how: each record that names a file as an attachment
    /// or by its id in any of its texts (every string of the record as serialized, so a field
    /// added later is read too), once per holder. An id in a text counts only when it is a
    /// stored attachment's, as a run of exactly 16 lowercase hex characters (a commit hash
    /// never matches). A chance match only keeps a file longer.
    pub fn attachment_holders(&self) -> Vec<Held<'_>> {
        let stored: HashSet<&str> = self.attachments.iter().map(|a| a.id.as_str()).collect();
        let mut out = vec![];
        for h in self.holding() {
            let mut named: Vec<(&str, Named)> = vec![];
            for id in &h.attached {
                if !named.iter().any(|(n, _)| n == id) {
                    named.push((id.as_str(), Named::Attached));
                }
            }
            let text = (h.text)();
            let mut found = vec![];
            hex_ids(&text, &mut found);
            for id in found {
                if let Some(id) = stored.get(id).copied()
                    && !named.iter().any(|(n, _)| *n == id)
                {
                    named.push((id, Named::InText));
                }
            }
            for (id, how) in named {
                out.extend(h.by.iter().map(|by| Held { id, how, by: by.clone(), at: h.at }));
            }
        }
        out
    }

    /// The files the records hold as attachments (`Named::Attached` of `attachment_holders`):
    /// what the blind view keeps. The same walk, without reading any text: it serializes
    /// nothing, since every agent's read asks for it.
    pub fn held_attachments(&self) -> HashSet<String> {
        self.holding().into_iter().flat_map(|h| h.attached).cloned().collect()
    }

    /// Since when `a` is free (every holder closed): the latest of the holding intents' close
    /// (a workspace's: the last close of an intent that ran there), the holding records' own
    /// times and the file's own (its first store, or the last time it was added again). None while any holder is open, or a time cannot be read.
    fn free_since(&self, a: &Attachment, held: &[&Held]) -> Option<SystemTime> {
        let mut times: Vec<&str> = std::iter::once(a.at.as_str()).chain(a.last.as_deref()).collect();
        for h in held {
            if !h.at.is_empty() {
                times.push(h.at);
            }
            match &h.by {
                Holder::Intent(id) => times.push(self.intents.iter().find(|i| &i.id == id)?.closed.as_deref()?),
                Holder::Workspace(w) => {
                    let ran = self.intents.iter().filter(|i| i.workspace.as_deref() == Some(w.as_str()));
                    if ran.clone().any(|i| i.state == IntentState::Active) {
                        return None;
                    }
                    times.extend(ran.filter_map(|i| i.closed.as_deref()));
                }
                Holder::Record { open: true } => return None,
                Holder::Record { open: false } => {}
            }
        }
        times.into_iter().map(parsed).collect::<Option<Vec<_>>>()?.into_iter().max()
    }

    /// The stored files the rule takes at `now`: not gone, of its type and size, free, and
    /// free for its `days`. A file an active intent holds is never among them, however old.
    pub fn attachments_due(&self, rule: &Conditions, now: SystemTime) -> Vec<&Attachment> {
        let holders = self.attachment_holders();
        let wait = Duration::from_secs(u64::from(rule.days) * 24 * 3600);
        self.attachments
            .iter()
            .filter(|a| a.gone.is_none() && rule.takes(a))
            .filter(|a| {
                let held: Vec<&Held> = holders.iter().filter(|h| h.id == a.id).collect();
                self.free_since(a, &held).is_some_and(|t| t + wait <= now)
            })
            .collect()
    }

    /// The stored attachments shown with a task: its output's, its checks', its findings' and
    /// those added to them later, those added to what it asks (its own, blind or not), then
    /// those of the findings it answers, unless it is blind and not delivered (its agent is
    /// told nothing of other work). Empty for no such task.
    pub fn attachments_of(&self, unit: &str, n: u32) -> Vec<&str> {
        let Ok(t) = self.task(unit, n) else { return vec![] };
        fn of<'a>(fs: &'a [Finding], ids: Option<&[String]>) -> Vec<&'a String> {
            let fs = fs.iter().filter(|f| ids.is_none_or(|ids| ids.contains(&f.found.id)));
            fs.flat_map(|f| f.found.attachments.iter().chain(f.notes.iter().flat_map(|n| &n.attachments))).collect()
        }
        let mut out: Vec<&String> = t.attachments.iter().chain(t.evidence.iter().filter_map(|e| e.attachment.as_ref())).chain(of(&t.findings, None)).chain(t.notes.iter().flat_map(|n| &n.attachments)).collect();
        if !(t.blind && !t.state.over()) {
            for c in &t.causes {
                if let Ok(by) = self.task(c.unit.as_deref().unwrap_or(unit), c.task) {
                    out.extend(of(&by.findings, Some(&c.findings)));
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        out.into_iter().map(String::as_str).filter(|id| seen.insert(*id)).collect()
    }

    /// Whether a question or a message may point at stored attachment `id`: one shown with
    /// task `n` of `unit` (n 0: with any of its tasks), or, for the user, one the user stored
    /// (attached in the page), which needs no unit. Its path is given to an agent, so
    /// nothing else gets out.
    pub fn pointable(&self, unit: &str, n: u32, id: &str, user: bool) -> bool {
        if user && self.attachments.iter().any(|a| a.id == id && a.user) {
            return true;
        }
        match n {
            _ if unit.is_empty() => false,
            0 => self.tasks_of(unit).is_some_and(|ts| ts.iter().any(|t| self.attachments_of(unit, t.n).contains(&id))),
            n => self.attachments_of(unit, n).contains(&id),
        }
    }

    /// Every id names a stored attachment.
    pub(crate) fn stored<'a>(&self, ids: impl Iterator<Item = &'a String>) -> Result<(), Rejected> {
        for id in ids {
            if !self.attachments.iter().any(|a| &a.id == id) {
                return no(format!("no stored attachment {id}; store the file first (workriver attach)"));
            }
        }
        Ok(())
    }

    /// The rules of an attachment's events: `prepare`, for these kinds.
    pub(crate) fn prepare_attachments(&self, _who: &Principal, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        Ok(match body {
            AttachmentAdded { id, name, mime, size, unit, task, finding } => {
                if blank(&id) || blank(&name) {
                    return no("an attachment needs its id and name");
                }
                if !attachment_id(&id) {
                    return no(format!("{id:?} is not an attachment id: the store names a file by 16 lowercase hex characters; store it with `workriver attach <file>` and record the id it gives"));
                }
                if size > MAX_ATTACHMENT as u64 {
                    return no(format!("{name} is larger than {} MB, the most workriver stores", MAX_ATTACHMENT / 1024 / 1024));
                }
                match (&unit, task, &finding) {
                    (None, None, None) => {}
                    (Some(u), Some(n), f) => {
                        let t = self.task(u, n)?;
                        if let Some(f) = f
                            && !t.findings.iter().any(|x| &x.found.id == f)
                        {
                            return no(format!("{u}#{n} has no finding {f}"));
                        }
                    }
                    _ => return no("an attachment is linked to a task (unit and number), or to nothing"),
                }
                AttachmentAdded { id, name, mime, size, unit, task, finding }
            }
            AttachmentRemoved { ids, how, to, rule } => {
                if ids.is_empty() {
                    return no("a removal names the files it removes");
                }
                if rule.days == 0 {
                    return no("the attachment rule waits at least one day after the work is closed");
                }
                match (how, &to) {
                    (Removed::Moved, None) => return no("a move says where the files went (to)"),
                    (Removed::Moved, Some(t)) if blank(&t.kind) => return no("a move's target has a kind"),
                    (Removed::Deleted, Some(_)) => return no("a deletion goes nowhere: it has no target"),
                    _ => {}
                }
                let holders = self.attachment_holders();
                for (k, id) in ids.iter().enumerate() {
                    if ids[..k].contains(id) {
                        return no(format!("attachment {id} is named twice"));
                    }
                    let Some(a) = self.attachments.iter().find(|a| &a.id == id) else { return no(format!("no stored attachment {id}")) };
                    if a.gone.is_some() {
                        return no(format!("attachment {id} is already removed"));
                    }
                    let held: Vec<&Held> = holders.iter().filter(|h| h.id == id).collect();
                    if self.free_since(a, &held).is_none() {
                        return no(format!("attachment {id} is still held by open work"));
                    }
                }
                AttachmentRemoved { ids, how, to, rule }
            }
            AttachmentRestored { id } => {
                match self.attachments.iter().find(|a| a.id == id) {
                    None => return no(format!("no stored attachment {id}")),
                    Some(a) if a.gone.is_none() => return no(format!("attachment {id} is not gone")),
                    Some(_) => {}
                }
                AttachmentRestored { id }
            }
            other => unreachable!("{} is not one of an attachment's kinds", name_of_kind(&other)),
        })
    }

    /// What an attachment's events change: `apply`, for these kinds.
    pub(crate) fn apply_attachments(&mut self, e: &Event) {
        use Body::*;
        let at = &e.at;
        match &e.body {
            AttachmentAdded { id, name, mime, size, unit, task, finding } => {
                let user = e.actor == "user";
                match self.attachments.iter_mut().find(|a| &a.id == id) {
                    Some(a) => {
                        a.user |= user;
                        a.last = Some(at.clone());
                    }
                    None => self.attachments.push(Attachment { id: id.clone(), name: name.clone(), mime: mime.clone(), size: *size, by: e.actor.clone(), at: at.clone(), user, gone: None, last: None }),
                }
                if let (Some(u), Some(n)) = (unit, task)
                    && let Some(t) = self.task_mut(u, *n)
                {
                    let list = match finding {
                        Some(f) => t.findings.iter_mut().find(|x| &x.found.id == f).map(|x| &mut x.found.attachments),
                        None => Some(&mut t.attachments),
                    };
                    if let Some(list) = list.filter(|l| !l.contains(id)) {
                        list.push(id.clone());
                    }
                }
            }
            AttachmentRemoved { ids, how, to, .. } => {
                for a in self.attachments.iter_mut().filter(|a| ids.contains(&a.id)) {
                    a.gone = Some(Gone { at: at.clone(), how: *how, to: to.clone() });
                }
            }
            // Stored again: its clock starts again.
            AttachmentRestored { id } => {
                if let Some(a) = self.attachments.iter_mut().find(|a| &a.id == id) {
                    a.gone = None;
                    a.at = at.clone();
                }
            }
            other => unreachable!("{} is not one of an attachment's kinds", name_of_kind(other)),
        }
    }
}
