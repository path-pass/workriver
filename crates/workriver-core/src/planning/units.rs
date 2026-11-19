// Units: a piece of an intent done on its own branch (or, for research, with no branch), where
// it is, how far it got, and the commits linked to it by hand.
use serde::{Deserialize, Serialize};

use crate::access::{Principal, Rejected, no};
use crate::records::*;

/// What a unit holds: code on a branch of its own, or research and design with no branch.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum UnitKind {
    #[default]
    Code,
    /// Research and design for the intent: no repository of its own, no branch, never merged;
    /// closed with `unit.done`.
    Research,
    /// An experiment (measure, compare, try an approach): code on a `spike/` branch of its
    /// own, scanned and shown like a code unit's, but never merged; closed with `unit.done`.
    Spike,
}

impl UnitKind {
    /// Code on a branch of its own: a code unit or a spike.
    pub fn is_code(&self) -> bool {
        matches!(self, UnitKind::Code | UnitKind::Spike)
    }

    /// Merged when done: a code unit. A spike and a research unit are closed with `unit.done`.
    pub fn merges(&self) -> bool {
        *self == UnitKind::Code
    }

    /// The default kind, left out when written.
    pub(crate) fn is_default(&self) -> bool {
        *self == UnitKind::Code
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Unit {
    pub id: String,
    pub intent: String,
    pub title: String,
    pub purpose: String,
    pub repo: String,
    pub branch: String,
    pub base: String,
    /// The machine its repository is on; none: this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    pub at: String,
    /// When it was recorded as merged.
    pub merged: Option<String>,
    pub merged_commits: Vec<String>,
    pub tasks: Vec<Task>,
    /// Where its work was before, oldest first, with the commits seen there.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub earlier: Vec<Earlier>,
    /// Commits linked to its tasks by hand.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<CommitLink>,
    /// Finished without a merge (research, abandoned, superseded): when, by whom, why.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done: Option<Closed>,
    /// Closed as done before, then given a new task, which opened it again: that closing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reopened: Option<Closed>,
    #[serde(default, skip_serializing_if = "UnitKind::is_default")]
    pub kind: UnitKind,
}

impl Unit {
    /// Code on a branch: what git is read for. A research unit and an intent's own work
    /// (seen as a unit) have none.
    pub fn has_code(&self) -> bool {
        self.kind.is_code() && !self.branch.is_empty()
    }
}

/// A unit closed by hand, or a task dropped.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Closed {
    pub at: String,
    pub by: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

/// Where a unit stands, from its tasks and its branch; `Done` and `Merged` are recorded.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Progress {
    /// A task is to be done or being done.
    Working,
    /// Something waits on a decision or on the user: an open finding, a waiting gate, or a
    /// finding accepted with no work handed out for it.
    Waiting,
    /// Everything delivered and decided: it can be merged.
    Ready,
    Merged,
    Done,
}

/// A commit as it was seen on a branch a unit has left.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct SeenCommit {
    pub hash: String,
    #[serde(default)]
    pub subject: String,
    /// The task it carried out there (by its trailer or a link), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<u32>,
    /// Its author date, which a rebase keeps: with the subject, it is found again.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub date: String,
    /// Its patch id (`git patch-id --stable`), taken when the unit moved: a rebased copy of
    /// it has the same one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub patch_id: String,
}

/// Where a unit's work was before it was moved (rebased onto a new branch, say).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Earlier {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    pub repo: String,
    pub branch: String,
    pub base: String,
    /// When it was left.
    pub until: String,
    pub commits: Vec<SeenCommit>,
}

/// A commit said to belong to a task (`workriver commit link`).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct CommitLink {
    pub hash: String,
    pub patch_id: String,
    pub subject: String,
    pub date: String,
    pub task: u32,
    pub by: String,
}

/// The branch prefixes a unit's branch takes: what kind of work it holds.
const BRANCH_PREFIXES: [&str; 3] = ["feat/", "chore/", "fix/"];

/// A unit's branch is its own: never its base (that would count everyone's commits on it as
/// the unit's), and, when newly named, one of `BRANCH_PREFIXES` and a name (a spike's:
/// `spike/` and a name, which no other unit takes). Checked when an event is written, never
/// on replay: units named before the rule keep loading.
pub(crate) fn unit_branch(branch: &str, base: &str, named: bool, kind: UnitKind) -> Result<(), Rejected> {
    if branch == base || base.strip_prefix("origin/") == Some(branch) {
        return no(format!("a unit's branch must not be its base ({base}): create the unit's own branch from {base} (git switch -c feat/<name> {base}) and give that: --branch feat/<name>"));
    }
    let named_with = |p: &str| branch.strip_prefix(p).is_some_and(|rest| !rest.is_empty());
    let name = branch.rsplit('/').next().unwrap_or(branch);
    if named && kind == UnitKind::Spike && !named_with("spike/") {
        return no(format!("a spike's branch starts with spike/ and a name; {branch} does not: name it so, e.g. --branch spike/{name}"));
    }
    if named && kind != UnitKind::Spike && !BRANCH_PREFIXES.iter().any(|p| named_with(p)) {
        return no(format!("a unit's branch starts with feat/, chore/ or fix/ and a name (feat/<name> for new work, fix/<name> for a fix, chore/<name> for the rest; spike/<name> is for --kind spike); {branch} does not: name it so, e.g. --branch feat/{name}"));
    }
    Ok(())
}

impl State {
    pub fn unit(&self, id: &str) -> Result<&Unit, Rejected> {
        match self.units.iter().find(|u| u.id == id) {
            Some(u) => Ok(u),
            None => no(format!("no unit {id}")),
        }
    }

    /// A unit, or an intent seen as a unit for its intent-level tasks (no branch; its
    /// project folder as the repository).
    pub fn unit_or_intent(&self, id: &str) -> Option<Unit> {
        if let Ok(u) = self.unit(id) {
            return Some(u.clone());
        }
        let i = self.intents.iter().find(|i| i.id == id)?;
        Some(Unit {
            id: i.id.clone(),
            intent: i.id.clone(),
            title: i.title.clone(),
            purpose: i.goal.clone(),
            repo: i.project.clone(),
            branch: String::new(),
            base: String::new(),
            machine: None,
            at: i.at.clone(),
            merged: None,
            merged_commits: vec![],
            tasks: i.tasks.clone(),
            earlier: vec![],
            links: vec![],
            done: None,
            reopened: None,
            kind: UnitKind::Code,
        })
    }

    /// Where unit `u` stands. A waiting gate or an open finding comes before work going on,
    /// since it asks for someone; deferred, rejected and scheduled findings hold nothing up.
    pub fn progress(&self, u: &Unit) -> Progress {
        if u.done.is_some() {
            return Progress::Done;
        }
        if u.merged.is_some() {
            return Progress::Merged;
        }
        // Dropped work answers nothing: the finding waits for work again.
        let answered = |t: &Task, f: &Finding| self.follow_ups(&u.id, t.n, &f.found.id).iter().any(|(_, x)| x.state != TaskState::Dropped);
        let waits = u.tasks.iter().any(|t| {
            t.state == TaskState::Waiting
                || t.findings.iter().any(|f| f.state == FindingState::Open || (matches!(f.state, FindingState::Accepted | FindingState::Revised) && !answered(t, f)))
        });
        if waits {
            Progress::Waiting
        } else if u.tasks.is_empty() || u.tasks.iter().any(|t| matches!(t.state, TaskState::Todo | TaskState::Running)) {
            Progress::Working
        } else {
            Progress::Ready
        }
    }

    /// Since when a ready unit has been ready: the last time one of its tasks was delivered,
    /// submitted, corrected or dropped, or one of their findings decided. None unless ready.
    pub fn ready_since(&self, u: &Unit) -> Option<String> {
        if self.progress(u) != Progress::Ready {
            return None;
        }
        let times = u.tasks.iter().flat_map(|t| t.done.iter().chain(&t.amended).chain(t.dropped.as_ref().map(|d| &d.at)).chain(t.findings.iter().flat_map(|f| f.decisions.iter().map(|d| &d.at))));
        times.max().cloned()
    }

    /// Whether `who` may put a unit of `intent` on `machine` (none: the machine workriver
    /// runs on). The user may anywhere. An agent may on its own machine; on another only as
    /// the intent's manager, and only where a workspace of that machine is linked with the
    /// intent's workspace: the work must be reachable by the agents it is for.
    pub(crate) fn may_place(&self, who: &Principal, intent: &str, machine: Option<&str>) -> Result<(), Rejected> {
        let Principal::Agent { session, machine: own } = who else { return Ok(()) };
        if own.as_deref() == machine {
            return Ok(());
        }
        if self.manager_of(intent) != Some(session.as_str()) {
            return no(format!("only the user or the manager of intent {intent} puts its units on another machine"));
        }
        let other = machine.or(own.as_deref()).unwrap_or_default();
        let workspace = self.intents.iter().find(|i| i.id == intent).and_then(|i| i.workspace.as_ref()).and_then(|g| self.workspace_of(g)).map(|w| w.group.label.clone());
        let linked = workspace.as_ref().is_some_and(|label| self.links.iter().any(|l| l.machine == other && &l.local == label));
        if !linked {
            return no(format!(
                "no workspace on {other} is linked with this intent's workspace{}; the user links one first: workriver workspace link {other}:<workspace there> {}",
                workspace.as_ref().map(|l| format!(" ({l})")).unwrap_or_default(),
                workspace.as_deref().unwrap_or("<workspace here>")
            ));
        }
        Ok(())
    }
}
