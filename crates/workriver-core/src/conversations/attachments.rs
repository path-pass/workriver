// Attachments: the files put in the store, and where they are linked.
use serde::{Deserialize, Serialize};

use crate::*;

/// A stored file. Never deleted, like the records.
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
}

impl State {
    /// The stored attachments shown with a task: its output's, its checks', its findings' and
    /// those added to them later, then those of the findings it answers, unless it is blind
    /// and not delivered (its agent is told nothing of other work). Empty for no such task.
    pub fn attachments_of(&self, unit: &str, n: u32) -> Vec<&str> {
        let Ok(t) = self.task(unit, n) else { return vec![] };
        fn of<'a>(fs: &'a [Finding], ids: Option<&[String]>) -> Vec<&'a String> {
            let fs = fs.iter().filter(|f| ids.is_none_or(|ids| ids.contains(&f.found.id)));
            fs.flat_map(|f| f.found.attachments.iter().chain(f.notes.iter().flat_map(|n| &n.attachments))).collect()
        }
        let mut out: Vec<&String> = t.attachments.iter().chain(t.evidence.iter().filter_map(|e| e.attachment.as_ref())).chain(of(&t.findings, None)).collect();
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
                    Some(a) => a.user |= user,
                    None => self.attachments.push(Attachment { id: id.clone(), name: name.clone(), mime: mime.clone(), size: *size, by: e.actor.clone(), at: at.clone(), user }),
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
            other => unreachable!("{} is not one of an attachment's kinds", name_of_kind(other)),
        }
    }
}
