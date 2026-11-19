// Projects (a folder of repositories), the runtime workspaces that work on them, and the
// links that make a workspace of another machine one with a local one.
use serde::{Deserialize, Serialize};

use crate::records::*;

use super::*;

/// The runtime's group an intent follows (under Herdr, a workspace): its id and label.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct RuntimeRef {
    /// Unique across machines: another machine's carries its name.
    pub id: String,
    pub label: String,
    /// The machine it is on; none: this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
}

/// A registered project folder.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Project {
    pub id: String,
    pub path: String,
    pub title: String,
}

/// A runtime group (a Herdr workspace): its manager and the intent new work goes to.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Workspace {
    pub group: RuntimeRef,
    pub manager: Option<String>,
    pub current: Option<String>,
    /// The project (its folder) the user says it works on; none: workriver works it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

/// A workspace of another machine made one with a workspace of this machine: one manager,
/// the same intents. Workspaces are named by their labels, which outlive their ids.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Link {
    pub machine: String,
    pub workspace: String,
    /// The label of the workspace on this machine.
    pub local: String,
}

impl State {
    /// A runtime group's record: by its id, or else by its label, since a runtime may give
    /// its groups new ids when it restarts.
    pub fn workspace_of(&self, r: &RuntimeRef) -> Option<&Workspace> {
        self.workspace_at(r).map(|k| &self.workspaces[k])
    }

    /// The link of a group of another machine, if it has one.
    pub fn link_of(&self, r: &RuntimeRef) -> Option<&Link> {
        let m = r.machine.as_ref()?;
        self.links.iter().find(|l| &l.machine == m && l.workspace == r.label)
    }

    /// Where a group's record is. A linked group of another machine is the workspace of this
    /// machine it is linked to; nothing else is ever matched across machines.
    pub(crate) fn workspace_at(&self, r: &RuntimeRef) -> Option<usize> {
        if let Some(l) = self.link_of(r) {
            return self.workspaces.iter().position(|w| w.group.machine.is_none() && w.group.label == l.local);
        }
        let here = |w: &&Workspace| w.group.machine == r.machine;
        let at = |f: &dyn Fn(&Workspace) -> bool| self.workspaces.iter().position(|w| here(&w) && f(w));
        at(&|w| w.group.id == r.id).or_else(|| at(&|w| w.group.label == r.label))
    }

    /// The intent new work in a runtime group goes to: the group's current intent.
    pub fn intent_for(&self, r: &RuntimeRef) -> Option<&Intent> {
        let id = self.workspace_of(r)?.current.as_deref()?;
        self.intents.iter().find(|i| i.id == id)
    }

    /// A runtime group's record, made when first needed; it follows the group's latest id and label.
    pub(crate) fn workspace_mut(&mut self, g: &RuntimeRef) -> &mut Workspace {
        // A linked group's record is the local workspace's, which keeps its own id and label.
        let local = self.link_of(g).map(|l| RuntimeRef { id: l.local.clone(), label: l.local.clone(), machine: None });
        match self.workspace_at(g) {
            Some(k) => {
                if local.is_none() {
                    self.workspaces[k].group = g.clone();
                }
                &mut self.workspaces[k]
            }
            None => {
                self.workspaces.push(Workspace { group: local.unwrap_or_else(|| g.clone()), manager: None, current: None, project: None });
                self.workspaces.last_mut().unwrap()
            }
        }
    }
}
