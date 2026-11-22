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
    /// The user's last start of a manager for it (`workspace.manager_started`), pending or
    /// how it ended; dropped by every event that touches the seat after it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<ManagerStart>,
}

/// A place's shell as the service read it when it opened the place: its process id and when
/// it started, in its machine's own measure (as for a pin).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shell {
    pub pid: u32,
    pub start: u64,
}

/// The user's start of a manager: what they chose, and the place the service opened for it.
/// Pending until it ends: by a seat event (it is then dropped), or by `ManagerStartEnded`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ManagerStart {
    /// The sequence of its `workspace.manager_started`: what its seating and end name.
    pub seq: u64,
    pub at: String,
    pub harness: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    pub name: String,
    pub folder: String,
    pub place: String,
    pub shell: Shell,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<StartOver>,
}

/// How a start ended with nobody seated.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct StartOver {
    pub reason: StartEnd,
    /// The runtime's words, when it said any.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub words: String,
    pub at: String,
}

/// Why a start ended with nobody seated: a key, never text for a person.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StartEnd {
    /// The runtime refused to start the harness.
    Runtime,
    /// Nothing showed up at the place: the harness is not there, or the shell refused it.
    NoAgent,
    /// What showed up had no session to seat in time.
    NoSession,
    /// The rules refused its seating.
    Refused,
    /// The service restarted while it was pending.
    Restarted,
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

/// How a refused start begins when one is pending for the workspace already.
pub const START_PENDING: &str = "a manager is being started";

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

    /// A workspace's start that is still pending, if any.
    pub fn pending_start(&self, id: &str) -> Option<&ManagerStart> {
        self.workspaces.iter().find(|w| w.id == id)?.start.as_ref().filter(|s| s.ended.is_none())
    }

    /// The workspace's pending start named by `seq`: an end or a seating names it, and only
    /// that one, while it is pending, is ended or seated.
    fn the_start(&self, workspace: &str, seq: u64) -> Result<&ManagerStart, Rejected> {
        let w = self.workspace(workspace)?;
        match &w.start {
            Some(s) if s.seq != seq && s.ended.is_none() => no(format!("{seq} is not the start of a manager for {workspace}; {} is", s.seq)),
            Some(s) if s.seq == seq && s.ended.is_none() => Ok(s),
            _ => no(format!("the start of a manager for {workspace} ({seq}) is over: what it brought up is not seated")),
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
                // One project per folder of the tree: an intent's project is the one its folder
                // is in, and a project added inside another would change it for those recorded.
                let inside = |outer: &str, inner: &str| inner.strip_prefix(outer).is_some_and(|rest| rest.starts_with('/') || outer.ends_with('/'));
                if let Some(p) = self.projects.iter().find(|p| inside(&p.path, &path)) {
                    return no(format!("{path} is inside project {} ({}): a project holds no other", p.id, p.path));
                }
                if let Some(p) = self.projects.iter().find(|p| inside(&path, &p.path)) {
                    return no(format!("{path} holds project {} ({}): a project holds no other", p.id, p.path));
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
            // The user's start: as a seating over the seat they were shown, one at a time, in
            // the folder of the workspace's project whatever the body says.
            WorkspaceManagerStarted { workspace, harness, model, place, shell, replaces, .. } => {
                let w = self.workspace(&workspace)?;
                if w.closed {
                    return no(format!("{workspace} is closed: open it first (Settings → Workspaces, or `workriver workspace open {workspace}`)"));
                }
                let now = w.manager.as_ref().map(Seat::shown);
                if now != replaces {
                    let who = |s: &Option<SeatRef>| s.as_ref().map_or("nobody".to_string(), |s| self.name_of(&s.session).unwrap_or(&s.session).to_string());
                    return no(format!("{SEAT_CHANGED}: {workspace}'s manager is {} now, not {}; look again and choose", who(&now), who(&replaces)));
                }
                if self.pending_start(&workspace).is_some() {
                    return no(format!("{START_PENDING} for {workspace}: wait for it, or seat one yourself"));
                }
                if blank(&harness) || blank(&place) {
                    return no("a start names its harness and the place it was opened at");
                }
                let Some(p) = self.workspace_project(w) else { return no(format!("{workspace}'s project is not registered")) };
                WorkspaceManagerStarted { name: format!("{workspace}-manager"), folder: p.path.clone(), workspace, harness, model, place, shell, replaces }
            }
            // What the start brought up, at its place: only for the start pending now (so no seat
            // event came after it, by sequence), and only a session the records did not know
            // before it, which holds no seat and has no task.
            ManagerStartSeated { workspace, start, session, harness, folder, place, shell } => {
                let s = self.the_start(&workspace, start)?;
                if place != s.place {
                    return no(format!("{session} is at another place ({place}) than the start opened ({})", s.place));
                }
                if shell != s.shell {
                    return no(format!("{place} has another shell than the one the start opened it with"));
                }
                if folder != s.folder {
                    return no(format!("{session} runs in another folder ({folder}) than the project's ({})", s.folder));
                }
                if harness != s.harness {
                    return no(format!("{session} is another harness ({harness}) than the one started ({})", s.harness));
                }
                if blank(&session) || matches!(session.as_str(), "user" | "workriver") {
                    return no("a manager is an agent: name its session");
                }
                if let Some(w) = self.seat_of(&session) {
                    return no(format!("{session} holds the seat of {}: a start seats only what it brought up", w.id));
                }
                if self.agents.iter().any(|a| a.session == session && a.first <= start) {
                    return no(format!("{session} was known before the start: a start seats only what it brought up"));
                }
                let name = self.name_of(&session);
                let theirs = |t: &Task| t.agent.as_deref().is_some_and(|a| a == session || Some(a) == name);
                if self.units.iter().flat_map(|u| &u.tasks).chain(self.intents.iter().flat_map(|i| &i.tasks)).any(theirs) {
                    return no(format!("{session} has tasks: a start seats only what it brought up"));
                }
                ManagerStartSeated { workspace, start, session, harness, folder, place, shell }
            }
            ManagerStartEnded { workspace, start, reason, words } => {
                self.the_start(&workspace, start)?;
                ManagerStartEnded { workspace, start, reason, words }
            }
            other => unreachable!("{} is not one of a project's or a workspace's kinds", name_of_kind(&other)),
        })
    }

    /// A seat is filled (`session`) or emptied, by the event at `seq`: the user's seating, or
    /// a start's. Every seat it touches drops its workspace's start.
    fn seat(&mut self, workspace: &str, session: Option<&str>, machine: &Option<String>, folder: &str, seq: u64) {
        let Some(k) = self.workspaces.iter().position(|w| w.id == workspace) else { return };
        self.workspaces[k].start = None;
        let Some(session) = session else {
            self.workspaces[k].manager = None;
            return;
        };
        // A session holds one seat.
        for w in self.workspaces.iter_mut().filter(|w| w.id != workspace && w.manager.as_ref().is_some_and(|m| m.session == session)) {
            w.manager = None;
            w.start = None;
        }
        // The same session where it was keeps its appointment; anything else is a new one.
        let kept = self.workspaces[k].manager.as_ref().filter(|m| m.session == session && &m.machine == machine && m.folder == folder).map(|m| m.since);
        self.workspaces[k].manager = Some(Seat { session: session.to_string(), machine: machine.clone(), folder: folder.to_string(), since: kept.unwrap_or(seq) });
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
            WorkspaceCreated { id, name, project } => self.workspaces.push(Workspace { id: id.clone(), name: name.clone(), project: project.clone(), manager: None, current: None, closed: false, start: None }),
            WorkspaceRenamed { workspace, name } => {
                if let Some(k) = found(&mut self.workspaces, workspace) {
                    self.workspaces[k].name = name.clone();
                }
            }
            // Every event that touches a seat drops that workspace's start, pending or ended:
            // its late seating is then refused, by sequence.
            WorkspaceClosed { workspace } => {
                if let Some(k) = found(&mut self.workspaces, workspace) {
                    self.workspaces[k].closed = true;
                    self.workspaces[k].manager = None;
                    self.workspaces[k].start = None;
                }
            }
            WorkspaceOpened { workspace } => {
                if let Some(k) = found(&mut self.workspaces, workspace) {
                    self.workspaces[k].closed = false;
                    self.workspaces[k].start = None;
                }
            }
            WorkspaceManagerSet { workspace, manager, machine, folder, .. } => self.seat(workspace, manager.as_deref(), machine, folder, e.seq as u64),
            ManagerStartSeated { workspace, session, folder, .. } => self.seat(workspace, Some(session), &None, folder, e.seq as u64),
            ManagerBack { id, workspace, session } => self.messages.push(Message::notice(id, workspace, session, &e.at, Notice::ManagerBack)),
            WorkspaceManagerStarted { workspace, harness, model, name, folder, place, shell, .. } => {
                if let Some(k) = found(&mut self.workspaces, workspace) {
                    let (harness, model, name, folder, place) = (harness.clone(), model.clone(), name.clone(), folder.clone(), place.clone());
                    self.workspaces[k].start = Some(ManagerStart { seq: e.seq as u64, at: e.at.clone(), harness, model, name, folder, place, shell: *shell, ended: None });
                }
            }
            ManagerStartEnded { workspace, reason, words, .. } => {
                if let Some(s) = found(&mut self.workspaces, workspace).and_then(|k| self.workspaces[k].start.as_mut()) {
                    s.ended = Some(StartOver { reason: *reason, words: words.clone(), at: e.at.clone() });
                }
            }
            other => unreachable!("{} is not one of a project's or a workspace's kinds", name_of_kind(other)),
        }
    }
}
