// The old events replayed one by one with the old code: for each, what it meant when it was
// recorded (the workspace record it wrote or resolved to then), and the seats the old
// registrations gave.
use std::collections::BTreeMap;

use workriver_core::{Body, IntentState, RuntimeRef, State};

use crate::read::Old;

/// Where an event went: an old workspace record (by its place in the old state's list, which
/// only grows), a label that found no record, or the conversation with every manager.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Place {
    Record(usize),
    Label(String),
    Every,
}

/// What an old event becomes, as far as the old code tells.
pub enum Step {
    /// Byte for byte.
    Keep,
    /// Not in the new form.
    Drop(&'static str),
    /// `intent.created`; with the record it was put in when it named a group and is active.
    Created { record: Option<usize> },
    /// A start in a group: the record it ran in from then.
    Start { record: usize },
    /// A state change that is not a start but names a group: the group goes.
    Unplace,
    /// `intent.linked`: the record it follows from then, and whether it is active.
    Linked { record: usize, active: bool },
    /// The user's message: its conversation, and each copy's (to every manager) by session.
    Sent { place: Place, copies: Vec<(String, Place)> },
    Said { place: Place },
    /// A read mark by label: every place that carried the label then.
    Read { places: Vec<Place> },
}

/// A record's manager as the old registrations set it, after event `after` (an index into
/// the events).
pub struct Seat {
    pub after: usize,
    pub record: usize,
    pub session: String,
    pub machine: Option<String>,
}

/// An event that named a folder no registered project holds.
pub struct Naming {
    pub event: usize,
}

#[derive(Default)]
pub struct Record {
    /// The intents put in it (created in it, started in it, linked to it), in order.
    pub intents: Vec<String>,
    /// The folder `workspace.project_set` last gave it, if any.
    pub project_set: Option<String>,
    /// Whether a message, copy or read mark went to it.
    pub talked: bool,
    /// The labels it carried, in order.
    pub labels: Vec<String>,
}

pub struct Analysis {
    pub events: Vec<Old>,
    pub steps: Vec<Step>,
    pub state: State,
    pub records: Vec<Record>,
    pub seats: Vec<Seat>,
    pub folders: BTreeMap<String, Vec<Naming>>,
    /// Each message's conversation, as it resolved when it was recorded.
    pub messages: BTreeMap<String, Place>,
    /// Labels that found no record: the messages that went to them.
    pub orphans: BTreeMap<String, Vec<String>>,
    /// The record each intent was last put in, as it resolved then (a start in a linked
    /// group: the record it was linked into, whatever the link is later).
    pub placed: BTreeMap<String, usize>,
}

fn record_of(s: &State, g: &RuntimeRef) -> Option<usize> {
    let w = s.workspace_of(g)?;
    s.workspaces.iter().position(|x| std::ptr::eq(x, w))
}

fn record_of_intent(s: &State, intent: &str) -> Option<usize> {
    let w = s.intent_workspace(intent)?;
    s.workspaces.iter().position(|x| std::ptr::eq(x, w))
}

pub fn analyse(events: Vec<Old>) -> Analysis {
    let mut s = State::default();
    let mut steps = vec![];
    let mut records: Vec<Record> = vec![];
    let mut seats = vec![];
    let mut named: Vec<(String, usize)> = vec![];
    let mut messages: BTreeMap<String, Place> = BTreeMap::new();
    let mut placed: BTreeMap<String, usize> = BTreeMap::new();
    for (k, o) in events.iter().enumerate() {
        let pre = s.clone();
        let label_place = |label: &str, pre: &State| {
            let local = pre.workspaces.iter().position(|w| w.group.machine.is_none() && w.group.label == label);
            local.or_else(|| pre.workspaces.iter().position(|w| w.group.label == label)).map_or(Place::Label(label.to_string()), Place::Record)
        };
        // Before it is applied: a message resolved against the records as they were.
        let step = match &o.event.body {
            Body::MessageSent { id, intent, workspace, to, answer, .. } => {
                let place = if !to.is_empty() {
                    Place::Every
                } else if let Some(a) = answer {
                    messages.get(&a.message).cloned().unwrap_or_else(|| label_place(workspace, &pre))
                } else if !intent.is_empty() {
                    record_of_intent(&pre, intent).map_or_else(|| Place::Label(workspace.clone()), Place::Record)
                } else {
                    label_place(workspace, &pre)
                };
                let copies = to.iter().map(|r| (r.manager.clone(), record_of(&pre, &r.workspace).map_or_else(|| Place::Label(r.workspace.label.clone()), Place::Record))).collect();
                messages.insert(id.clone(), place.clone());
                Step::Sent { place, copies }
            }
            Body::MessageSaid { id, workspace, .. } => {
                let mine = |w: &workriver_core::Workspace| w.manager.as_deref() == Some(o.event.actor.as_str());
                let place = pre
                    .workspaces
                    .iter()
                    .position(|w| mine(w) && &w.group.label == workspace)
                    .or_else(|| pre.workspaces.iter().position(mine))
                    .map_or_else(|| label_place(workspace, &pre), Place::Record);
                messages.insert(id.clone(), place.clone());
                Step::Said { place }
            }
            Body::MessagesRead { workspace, .. } if workspace.is_empty() => Step::Keep,
            Body::MessagesRead { workspace, .. } => {
                // The old mark was by label: it read every conversation stored under it, and
                // every record that carried it then.
                let mut places: Vec<Place> = pre.messages.iter().filter(|m| &m.workspace == workspace && m.to.is_empty()).filter_map(|m| messages.get(&m.id).cloned()).collect();
                places.extend(pre.workspaces.iter().enumerate().filter(|(_, w)| &w.group.label == workspace).map(|(r, _)| Place::Record(r)));
                places.sort();
                places.dedup();
                Step::Read { places }
            }
            Body::ManagerRegistered { .. } => Step::Drop("a registration: the seats start empty"),
            Body::WorkspaceLinked(_) | Body::WorkspaceUnlinked { .. } => Step::Drop("a link"),
            Body::WorkspaceProjectSet { .. } => Step::Drop("folded into the workspace's project"),
            _ => Step::Keep,
        };
        s.apply(&o.event);
        while records.len() < s.workspaces.len() {
            records.push(Record::default());
        }
        // After: where an intent went, and what a registration or project_set touched.
        let step = match (&o.event.body, step) {
            (Body::IntentCreated { id, runtime_ref, state, .. }, _) => {
                let record = runtime_ref.as_ref().filter(|_| *state == IntentState::Active).and_then(|_| record_of_intent(&s, id));
                Step::Created { record }
            }
            (Body::IntentStateChanged { intent, state: IntentState::Active, workspace: Some(_), .. }, _) => match record_of_intent(&s, intent) {
                Some(record) => Step::Start { record },
                None => Step::Unplace,
            },
            (Body::IntentStateChanged { workspace: Some(_), .. }, _) => Step::Unplace,
            (Body::IntentLinked { intent, .. }, _) => match record_of_intent(&s, intent) {
                Some(record) => Step::Linked { record, active: s.intents.iter().any(|i| &i.id == intent && i.state == IntentState::Active) },
                None => Step::Drop("a link of an intent to no record"),
            },
            (Body::WorkspaceProjectSet { workspace, project }, step) => {
                if let Some(r) = record_of(&s, workspace) {
                    records[r].project_set = project.clone();
                }
                step
            }
            (_, step) => step,
        };
        match &step {
            Step::Created { record: Some(r) } | Step::Start { record: r } | Step::Linked { record: r, .. } => {
                let id = o.event.body.refs().0.unwrap_or_default();
                placed.insert(id.clone(), *r);
                if !records[*r].intents.contains(&id) {
                    records[*r].intents.push(id);
                }
            }
            Step::Sent { place, copies } => {
                for p in std::iter::once(place).chain(copies.iter().map(|(_, p)| p)) {
                    if let Place::Record(r) = p {
                        records[*r].talked = true;
                    }
                }
            }
            Step::Said { place: Place::Record(r) } => records[*r].talked = true,
            _ => {}
        }
        // The folders events named, to find those no registered project holds at the end.
        match &o.event.body {
            Body::IntentCreated { project, .. } if !project.is_empty() => named.push((project.clone(), k)),
            Body::WorkspaceProjectSet { project: Some(p), .. } if !p.is_empty() => named.push((p.clone(), k)),
            _ => {}
        }
        // A manager set by a registration (only those set the old records' managers).
        for (r, w) in s.workspaces.iter().enumerate() {
            let before = pre.workspaces.get(r).and_then(|x| x.manager.clone());
            if let Some(m) = &w.manager
                && before.as_ref() != Some(m)
            {
                // The machine the agent runs on (a manager registered through a link runs on the
                // linked group's machine, not the record's).
                let machine = s.agents.iter().find(|a| &a.session == m).and_then(|a| a.machine.clone()).or_else(|| w.group.machine.clone());
                seats.push(Seat { after: k, record: r, session: m.clone(), machine });
            }
            if records[r].labels.last() != Some(&w.group.label) {
                records[r].labels.push(w.group.label.clone());
            }
        }
        steps.push(step);
    }
    // The messages of each label that found no record.
    let mut orphans: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (m, p) in &messages {
        if let Place::Label(l) = p {
            orphans.entry(l.clone()).or_default().push(m.clone());
        }
    }
    let mut folders: BTreeMap<String, Vec<Naming>> = BTreeMap::new();
    for (f, event) in named {
        // A registered project's id, or a folder a registered project holds, is not a line.
        if s.projects.iter().any(|p| p.id == f) || s.project_of(&f).is_some() {
            continue;
        }
        folders.entry(f).or_default().push(Naming { event });
    }
    Analysis { events, steps, state: s, records, seats, folders, messages, orphans, placed }
}
