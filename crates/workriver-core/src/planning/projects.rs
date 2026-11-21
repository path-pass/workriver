// Projects (a folder of repositories), the runtime workspaces that work on them, and the
// links that make a workspace of another machine one with a local one.
use serde::{Deserialize, Serialize};

use crate::*;

/// The runtime's group an intent follows (a workspace): its id and label.
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

/// A runtime group (a workspace): its manager and the intent new work goes to.
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
    /// The record of the workspace an intent runs in (a linked group of another machine: the
    /// workspace here it is linked to); none for an intent that runs nowhere, or in a group
    /// with no record.
    pub fn intent_workspace(&self, intent: &str) -> Option<&Workspace> {
        self.intents.iter().find(|i| i.id == intent)?.workspace.as_ref().and_then(|g| self.workspace_of(g))
    }

    /// A runtime group's record: by its id, or else by its label, since a runtime may give
    /// its groups new ids when it restarts.
    pub fn workspace_of(&self, r: &RuntimeRef) -> Option<&Workspace> {
        self.workspace_at(r).map(|k| &self.workspaces[k])
    }

    /// Whether two runtime groups are one workspace: the same group, or the same record.
    pub(crate) fn same_group(&self, a: &RuntimeRef, b: &RuntimeRef) -> bool {
        (a.id == b.id && a.machine == b.machine) || self.workspace_at(a).is_some_and(|k| self.workspace_at(b) == Some(k))
    }

    /// The link of a group of another machine, if it has one.
    pub fn link_of(&self, r: &RuntimeRef) -> Option<&Link> {
        let m = r.machine.as_ref()?;
        self.links.iter().find(|l| &l.machine == m && l.workspace == r.label)
    }

    /// Where a group's record is. A linked group of another machine is the workspace of this
    /// machine it is linked to; nothing else is ever matched across machines.
    pub(super) fn workspace_at(&self, r: &RuntimeRef) -> Option<usize> {
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
    pub(super) fn workspace_mut(&mut self, g: &RuntimeRef) -> &mut Workspace {
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

    /// A group's record, found and never renamed: for an event that does not name a workspace
    /// itself (an intent closed, a manager registered for an intent), which must not set a
    /// record's id and label back to the ones its intent was started with. Made when there
    /// is none.
    pub(super) fn workspace_found_mut(&mut self, g: &RuntimeRef) -> &mut Workspace {
        match self.workspace_at(g) {
            Some(k) => &mut self.workspaces[k],
            None => self.workspace_mut(g),
        }
    }

    /// The rules of a project's or a workspace's events: `prepare`, for these kinds.
    pub(crate) fn prepare_projects(&self, who: &Principal, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        Ok(match body {
            ProjectAdded { path, title, .. } => {
                if blank(&path) || blank(&title) {
                    return no("a project needs its folder and a title");
                }
                if self.projects.iter().any(|p| p.path == path) {
                    return no(format!("{path} is already a project"));
                }
                ProjectAdded { id: format!("p{}", self.projects.len() + 1), path, title }
            }
            ProjectRenamed { project, title } => {
                let Some(p) = self.projects.iter().find(|p| p.id == project || p.path == project) else {
                    return no(format!("no project {project}; a repository outside every project is registered with `workriver project add <folder> --title ...`"));
                };
                if blank(&title) {
                    return no("a project needs a title");
                }
                ProjectRenamed { project: p.id.clone(), title }
            }
            WorkspaceLinked(l) => {
                if blank(&l.machine) || blank(&l.workspace) || blank(&l.local) {
                    return no("a link names a machine, its workspace and a workspace of this machine");
                }
                if let Some(old) = self.links.iter().find(|x| x.machine == l.machine && x.workspace == l.workspace) {
                    return no(format!("{}:{} is already linked to {}; unlink it first", l.machine, l.workspace, old.local));
                }
                WorkspaceLinked(l)
            }
            WorkspaceUnlinked { machine, workspace } => {
                if !self.links.iter().any(|x| x.machine == machine && x.workspace == workspace) {
                    return no(format!("{machine}:{workspace} is not linked"));
                }
                WorkspaceUnlinked { machine, workspace }
            }
            WorkspaceProjectSet { workspace, project } => {
                if blank(&workspace.id) || blank(&workspace.label) {
                    return no("name the workspace by its id and label, as `workriver workspace list` shows it");
                }
                let project = match project {
                    None => None,
                    // A registered project (by id or folder), or a folder an intent or unit names.
                    Some(p) => match self.projects.iter().find(|x| x.id == p || x.path == p) {
                        Some(x) => Some(x.path.clone()),
                        None if !blank(&p) && (self.intents.iter().any(|i| i.project == p) || self.units.iter().any(|u| u.repo == p)) => Some(p),
                        None => return no(format!("no project {p}; name a project as `workriver project list` shows it")),
                    },
                };
                WorkspaceProjectSet { workspace, project }
            }
            ManagerRegistered { intent, agent, workspace } => {
                if workspace.as_ref().is_some_and(|g| blank(&g.id)) || (workspace.is_none() && intent.is_none()) {
                    return no("a manager manages a runtime workspace: run `workriver manager here` in its terminal");
                }
                // Naming an intent (as before 0.2) is managing the workspace it runs in, and only that.
                if let Some(i) = &intent {
                    let Some(at) = &self.intent(i)?.workspace else {
                        return no(format!("{i} runs in no workspace: run `workriver manager here` in your workspace's terminal, without --intent, then `workriver intent start {i}`"));
                    };
                    if let Some(g) = workspace.as_ref().filter(|g| !self.same_group(g, at)) {
                        return no(format!("{i} runs in workspace {}, not {}: a manager manages the workspace it runs in; run `workriver manager here` there, without --intent", at.label, g.label));
                    }
                }
                // An agent registers itself; nobody is made a manager by someone else.
                let agent = match who {
                    Principal::Agent { session, .. } if blank(&agent) || &agent == session => session.clone(),
                    Principal::Agent { .. } => return no("an agent registers itself as manager, not another agent"),
                    _ => return no("a manager is an agent: run `workriver manager here` in the manager agent's terminal"),
                };
                ManagerRegistered { intent, agent, workspace }
            }
            other => unreachable!("{} is not one of a project's or a workspace's kinds", name_of_kind(&other)),
        })
    }

    /// What a project's or a workspace's events change: `apply`, for these kinds.
    pub(crate) fn apply_projects(&mut self, e: &Event) {
        use Body::*;
        match &e.body {
            ProjectAdded { id, path, title } => self.projects.push(Project { id: id.clone(), path: path.clone(), title: title.clone() }),
            ProjectRenamed { project, title } => {
                if let Some(p) = self.projects.iter_mut().find(|p| &p.id == project || &p.path == project) {
                    p.title = title.clone();
                }
            }
            WorkspaceLinked(l) => self.links.push(l.clone()),
            WorkspaceUnlinked { machine, workspace } => self.links.retain(|x| !(&x.machine == machine && &x.workspace == workspace)),
            WorkspaceProjectSet { workspace, project } => self.workspace_mut(workspace).project = project.clone(),
            ManagerRegistered { intent, agent, workspace } => {
                // Before 0.2: the intent's manager, which is the manager of its runtime group.
                let mut ran = None;
                if let Some(i) = intent.as_ref().and_then(|id| self.intents.iter_mut().find(|i| &i.id == id)) {
                    i.manager = Some(agent.clone());
                    ran = i.workspace.clone();
                }
                // Only a workspace the event names is followed (its id and label); the
                // intent's is found as it is recorded now.
                if let Some(g) = workspace {
                    self.workspace_mut(g).manager = Some(agent.clone());
                } else if let Some(g) = ran {
                    self.workspace_found_mut(&g).manager = Some(agent.clone());
                }
            }
            other => unreachable!("{} is not one of a project's or a workspace's kinds", name_of_kind(other)),
        }
    }
}
