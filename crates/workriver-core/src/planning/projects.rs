// Projects (a folder of repositories) and the workspaces that work on them: workriver's own
// records, made at the user's word, each with the seat of its manager.
use serde::{Deserialize, Serialize};

use crate::*;

/// A registered project folder.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Project {
    pub id: String,
    pub path: String,
    pub title: String,
}

/// A workspace: where a project's intents run, one conversation with the user, one manager.
/// Nothing of the agent runtime identifies it: its id is workriver's, and its name is shown
/// only (it may repeat).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Workspace {
    /// `ws<N>`, in the order they are made; never changes, never given again.
    pub id: String,
    pub name: String,
    /// The registered project's id; set when the workspace is made, never changed.
    pub project: String,
    /// The seat: who its manager is, and where to reach it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manager: Option<Seat>,
    /// The intent new work goes to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    /// Closed by the user: its intents and conversation stay, read-only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub closed: bool,
}

/// A manager's seat: the session the user seated, with what reaches its terminal. `since` is
/// the appointment: the revision of the event that seated it. What was asked of or queued
/// for one appointment is never another's.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Seat {
    pub session: String,
    /// The machine it runs on; none: this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    /// The working directory the runtime showed for it when it was seated.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
    pub since: u64,
}

/// A seat as someone was shown it: its session and appointment.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct SeatRef {
    pub session: String,
    pub since: u64,
}

impl Seat {
    pub fn shown(&self) -> SeatRef {
        SeatRef { session: self.session.clone(), since: self.since }
    }
}

/// How a refused seating starts when the seat is no longer the one the user was shown.
pub const SEAT_CHANGED: &str = "the seat changed";

/// How the user seats a manager, for a refusal.
pub const SEATING: &str = "the user seats a workspace's manager: in the web UI (the workspace's line, Manager), or `workriver workspace manager <ws> <agent>` from the user's own terminal";

impl State {
    pub fn workspace(&self, id: &str) -> Result<&Workspace, Rejected> {
        match self.workspaces.iter().find(|w| w.id == id) {
            Some(w) => Ok(w),
            None => no(format!("no workspace {id}; `workriver workspace list` shows them")),
        }
    }

    /// The workspace an intent runs in, or last ran in; none for one that never ran.
    pub fn intent_workspace(&self, intent: &str) -> Option<&Workspace> {
        let id = self.intents.iter().find(|i| i.id == intent)?.workspace.as_deref()?;
        self.workspaces.iter().find(|w| w.id == id)
    }

    /// The workspace whose seat `session` holds (a session holds one).
    pub fn seat_of(&self, session: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.manager.as_ref().is_some_and(|m| m.session == session))
    }

    /// An agent's workspace, from the records alone: the one whose seat its session holds;
    /// else the workspace of the intents of the tasks it has (started, not over) when that
    /// is one workspace; else none. Nothing picks the newest.
    pub fn workspace_of_agent(&self, session: &str) -> Option<&Workspace> {
        if let Some(w) = self.seat_of(session) {
            return Some(w);
        }
        let name = self.name_of(session);
        let mine = |t: &&Task| t.state == TaskState::Running && t.agent.as_deref().is_some_and(|a| a == session || Some(a) == name);
        let of_units = self.units.iter().filter(|u| u.tasks.iter().any(|t| mine(&t))).map(|u| u.intent.as_str());
        let of_intents = self.intents.iter().filter(|i| i.tasks.iter().any(|t| mine(&t))).map(|i| i.id.as_str());
        let mut found: Vec<&str> = of_units.chain(of_intents).filter_map(|i| self.intent_workspace(i)).map(|w| w.id.as_str()).collect();
        found.sort_unstable();
        found.dedup();
        match found[..] {
            [one] => self.workspaces.iter().find(|w| w.id == one),
            _ => None,
        }
    }

    /// A workspace's project, as registered.
    pub fn workspace_project(&self, w: &Workspace) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == w.project)
    }

    /// The intent started last among those still active in workspace `id`, but `not`.
    fn next_current(&self, id: &str, not: &str) -> Option<String> {
        self.intents.iter().rev().find(|i| i.state == IntentState::Active && i.id != not && i.workspace.as_deref() == Some(id)).map(|i| i.id.clone())
    }

    /// An intent leaves workspace `id` (closed, shelved, moved): no longer where new work
    /// goes there; that is then the intent started last among those still active in it.
    pub(super) fn left(&mut self, id: &str, intent: &str) {
        let next = self.next_current(id, intent);
        if let Some(w) = self.workspaces.iter_mut().find(|w| w.id == id)
            && w.current.as_deref() == Some(intent)
        {
            w.current = next;
        }
    }

    /// The rules of a project's or a workspace's events: `prepare`, for these kinds.
    pub(crate) fn prepare_projects(&self, _who: &Principal, body: Body) -> Result<Body, Rejected> {
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
            WorkspaceCreated { name, project, .. } => {
                let Some(p) = self.projects.iter().find(|p| p.id == project || p.path == project) else {
                    return no(format!("no project {project}: a workspace is made in a registered project (`workriver project list`)"));
                };
                if blank(&name) {
                    return no("a workspace needs a name");
                }
                WorkspaceCreated { id: format!("ws{}", self.workspaces.len() + 1), name: name.trim().to_string(), project: p.id.clone() }
            }
            WorkspaceRenamed { workspace, name } => {
                self.workspace(&workspace)?;
                if blank(&name) {
                    return no("a workspace needs a name");
                }
                WorkspaceRenamed { workspace, name: name.trim().to_string() }
            }
            WorkspaceClosed { workspace } => {
                let w = self.workspace(&workspace)?;
                if w.closed {
                    return no(format!("{workspace} is closed already"));
                }
                if let Some(i) = self.intents.iter().find(|i| i.state == IntentState::Active && i.workspace.as_deref() == Some(&workspace)) {
                    return no(format!("{workspace} has an active intent ({}): finish it, put it back in the roadmap or move it to another workspace first", i.id));
                }
                WorkspaceClosed { workspace }
            }
            WorkspaceOpened { workspace } => {
                if !self.workspace(&workspace)?.closed {
                    return no(format!("{workspace} is open"));
                }
                WorkspaceOpened { workspace }
            }
            WorkspaceManagerSet { workspace, manager, machine, folder, replaces } => {
                let w = self.workspace(&workspace)?;
                if w.closed {
                    return no(format!("{workspace} is closed: open it first (Settings → Workspaces, or `workriver workspace open {workspace}`)"));
                }
                // Never over a seat the user was not shown: a change made meanwhile stays.
                let now = w.manager.as_ref().map(Seat::shown);
                if now != replaces {
                    let who = |s: &Option<SeatRef>| s.as_ref().map_or("nobody".to_string(), |s| self.name_of(&s.session).unwrap_or(&s.session).to_string());
                    return no(format!("{SEAT_CHANGED}: {workspace}'s manager is {} now, not {}; look again and choose", who(&now), who(&replaces)));
                }
                match &manager {
                    None if now.is_none() => return no(format!("{workspace} has no manager")),
                    Some(s) if blank(s) || matches!(s.as_str(), "user" | "workriver") => return no("a manager is an agent: name its session"),
                    _ => {}
                }
                // An empty seat has no machine or folder.
                let (machine, folder) = if manager.is_some() { (machine, folder) } else { (None, String::new()) };
                WorkspaceManagerSet { workspace, manager, machine, folder, replaces }
            }
            ManagerBack { workspace, session, .. } => {
                if self.workspace(&workspace)?.manager.as_ref().is_none_or(|m| m.session != session) {
                    return no(format!("{session} does not hold the seat of {workspace}"));
                }
                ManagerBack { id: format!("m{}", self.messages.len() + 1), workspace, session }
            }
            other => unreachable!("{} is not one of a project's or a workspace's kinds", name_of_kind(&other)),
        })
    }

    /// What a project's or a workspace's events change: `apply`, for these kinds.
    pub(crate) fn apply_projects(&mut self, e: &Event) {
        use Body::*;
        let found = |all: &mut Vec<Workspace>, id: &str| all.iter().position(|w| w.id == id);
        match &e.body {
            ProjectAdded { id, path, title } => self.projects.push(Project { id: id.clone(), path: path.clone(), title: title.clone() }),
            ProjectRenamed { project, title } => {
                if let Some(p) = self.projects.iter_mut().find(|p| &p.id == project || &p.path == project) {
                    p.title = title.clone();
                }
            }
            WorkspaceCreated { id, name, project } => self.workspaces.push(Workspace { id: id.clone(), name: name.clone(), project: project.clone(), manager: None, current: None, closed: false }),
            WorkspaceRenamed { workspace, name } => {
                if let Some(k) = found(&mut self.workspaces, workspace) {
                    self.workspaces[k].name = name.clone();
                }
            }
            WorkspaceClosed { workspace } => {
                if let Some(k) = found(&mut self.workspaces, workspace) {
                    self.workspaces[k].closed = true;
                    self.workspaces[k].manager = None;
                }
            }
            WorkspaceOpened { workspace } => {
                if let Some(k) = found(&mut self.workspaces, workspace) {
                    self.workspaces[k].closed = false;
                }
            }
            WorkspaceManagerSet { workspace, manager, machine, folder, .. } => {
                let Some(k) = found(&mut self.workspaces, workspace) else { return };
                let Some(session) = manager else {
                    self.workspaces[k].manager = None;
                    return;
                };
                // A session holds one seat.
                for w in self.workspaces.iter_mut().filter(|w| &w.id != workspace && w.manager.as_ref().is_some_and(|m| &m.session == session)) {
                    w.manager = None;
                }
                // The same session where it was keeps its appointment; anything else is a new one.
                let kept = self.workspaces[k].manager.as_ref().filter(|m| &m.session == session && &m.machine == machine && &m.folder == folder).map(|m| m.since);
                self.workspaces[k].manager = Some(Seat { session: session.clone(), machine: machine.clone(), folder: folder.clone(), since: kept.unwrap_or(e.seq as u64) });
            }
            ManagerBack { id, workspace, session } => self.messages.push(Message::notice(id, workspace, session, &e.at, Notice::ManagerBack)),
            other => unreachable!("{} is not one of a project's or a workspace's kinds", name_of_kind(other)),
        }
    }
}
