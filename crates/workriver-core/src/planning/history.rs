// An intent's history: its versions, rebuilt from its events (and those of the intents
// merged into it) by the same `apply` that keeps the state, so what is a version and what
// each one held are said once.
use serde::{Deserialize, Serialize};

use crate::*;

/// How a version came to be.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum How {
    Created,
    Edited,
    Merged,
}

/// One version of an intent: who made it and when, and what its event set (an addition is
/// the paragraph added, not the whole note).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Version {
    pub n: u32,
    pub at: String,
    /// "user", or the agent's session.
    pub by: String,
    pub how: How,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    /// The whole note, when the version set it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The paragraph it added to the note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<String>,
    /// The intents merged into it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub merged: Vec<String>,
    /// The kind it set: absent, not set by it; null, cleared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<Option<IntentKind>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<Option<IntentSize>>,
    /// The tags it left, when it changed them (`[]`: cleared).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// An intent's text as one version left it, with its kind, size and tags.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Text {
    pub title: String,
    pub goal: String,
    pub note: String,
    pub kind: Option<IntentKind>,
    pub size: Option<IntentSize>,
    pub tags: Vec<String>,
}

/// Replays `events` (oldest first) and calls `each` after every event that made a version of
/// intent `id`, with that event and the intent as it left it; stops when `each` says so.
fn replay(events: &[Event], id: &str, mut each: impl FnMut(&Event, &Intent) -> bool) {
    let mut s = State::default();
    for e in events {
        let before = s.intents.iter().find(|i| i.id == id).map_or(0, |i| i.edits.version);
        s.apply(e);
        if let Some(i) = s.intents.iter().find(|i| i.id == id)
            && i.edits.version > before
            && !each(e, i)
        {
            return;
        }
    }
}

/// The versions of intent `id`, oldest first, from its events and those of every intent
/// merged into it (any other event is passed over).
pub fn intent_versions(events: &[Event], id: &str) -> Vec<Version> {
    let mut out = vec![];
    replay(events, id, |e, i| {
        let (n, at, by) = (i.edits.version, e.at.clone(), e.actor.clone());
        // Kind, size and tags as this version left them, where it changed them (a merge
        // changes the tags as the rules make it, not as its event says).
        let ed = &i.edits;
        let (kind, size, tags) = ((ed.kind == n).then_some(i.kind), (ed.size == n).then_some(i.size), (ed.tags == n).then(|| i.tags.clone()));
        let v = |how, title: Option<&String>, goal: Option<&String>, note: Option<&String>| Version { n, at: at.clone(), by: by.clone(), how, title: title.cloned(), goal: goal.cloned(), note: note.cloned(), added: None, merged: vec![], kind, size, tags: tags.clone() };
        out.push(match &e.body {
            Body::IntentCreated { title, goal, note, .. } => v(How::Created, Some(title), Some(goal), Some(note)),
            Body::IntentUpdated { title, goal, note, add_note, .. } => Version { added: add_note.clone(), ..v(How::Edited, title.as_ref(), goal.as_ref(), note.as_ref()) },
            Body::IntentMerged { from, title, .. } => Version { merged: from.clone(), ..v(How::Merged, title.as_ref(), None, None) },
            _ => v(How::Edited, None, None, None),
        });
        true
    });
    out
}

/// Intent `id`'s title, goal and note as version `n` left them; None when it has no such
/// version.
pub fn intent_text(events: &[Event], id: &str, n: u32) -> Option<Text> {
    let mut found = None;
    replay(events, id, |_, i| {
        if i.edits.version == n {
            found = Some(Text { title: i.title.clone(), goal: i.goal.clone(), note: i.note.clone(), kind: i.kind, size: i.size, tags: i.tags.clone() });
        }
        found.is_none()
    });
    found
}
