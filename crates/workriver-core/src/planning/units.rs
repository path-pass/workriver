// Units: a piece of an intent done on its own branch (or, for research, with no branch), where
// it is, how far it got, and the commits linked to it by hand.
use serde::{Deserialize, Serialize};

use crate::*;

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
    #[serde(default, skip_serializing_if = "UnitKind::is_default")]
    pub kind: UnitKind,
}

impl Unit {
    /// Code on a branch: what git is read for. A research unit has none.
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
fn unit_branch(branch: &str, base: &str, named: bool, kind: UnitKind) -> Result<(), Rejected> {
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
    /// runs on). The user may anywhere, and so may the intent's manager: on any machine
    /// workriver follows (following one is the user's act; the service checks the repository
    /// there). Another agent only on its own machine.
    fn may_place(&self, who: &Principal, intent: &str, machine: Option<&str>) -> Result<(), Rejected> {
        let Principal::Agent { session, machine: own } = who else { return Ok(()) };
        if own.as_deref() == machine || self.manager_of(intent) == Some(session.as_str()) {
            return Ok(());
        }
        no(format!("only the user or the manager of intent {intent} puts its units on another machine"))
    }

    /// The rules of a unit's events: `prepare`, for these kinds.
    pub(crate) fn prepare_units(&self, who: &Principal, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        let actor = who.actor();
        let user = *who == Principal::User;
        Ok(match body {
            UnitRelocated { unit, machine, repo, branch, base, previous } => {
                let u = self.unit(&unit)?;
                if !u.kind.is_code() {
                    return no(format!("unit {unit} is a research unit: it has no repository or branch to move"));
                }
                if u.merged.is_some() {
                    return no(format!("unit {unit} is merged; its place no longer matters"));
                }
                if !user && self.manager_of(&u.intent) != Some(actor) {
                    let manager = self.manager_of(&u.intent).map(|m| format!(" ({})", self.name_of(m).unwrap_or(m))).unwrap_or_default();
                    return no(format!(
                        "only the manager of intent {}{manager} or the user moves its units; tell the manager the new branch (workriver manager tell <your task> --text \"...\"), who runs: workriver unit move {unit} --branch <new> [--base <new base>]",
                        u.intent
                    ));
                }
                self.may_place(who, &u.intent, machine.as_deref())?;
                for (field, v) in [("repo", &repo), ("branch", &branch), ("base", &base)] {
                    if blank(v) {
                        return no(format!("a unit needs a {field}"));
                    }
                }
                // A branch named before the rule keeps its name while only the rest moves.
                unit_branch(&branch, &base, branch != u.branch, u.kind)?;
                UnitRelocated { unit, machine, repo, branch, base, previous }
            }
            CommitLinked { unit, hash, patch_id, subject, date, task } => {
                let u = self.unit(&unit)?;
                if !u.kind.is_code() {
                    return no(format!("unit {unit} is a research unit: it has no branch, so no commits to link"));
                }
                let t = match u.tasks.iter().find(|t| t.n == task) {
                    Some(t) => t,
                    None => return no(format!("no task {unit}#{task}")),
                };
                if blank(&hash) {
                    return no("which commit? give its hash");
                }
                // By whoever did the task, the intent's manager, or the user.
                if !user && self.manager_of(&u.intent) != Some(actor) && !self.is_agent_of(t, actor) {
                    return no(format!("a commit is linked to {unit}#{task} by the agent of that task, the manager of its intent, or the user"));
                }
                CommitLinked { unit, hash, patch_id, subject, date, task }
            }
            CommitUnlinked { unit, hash } => {
                if blank(&hash) {
                    return no("which commit? give its hash");
                }
                let found: Vec<&Unit> = self.units.iter().filter(|u| (unit.is_empty() || u.id == unit) && u.links.iter().any(|l| l.hash.starts_with(&hash))).collect();
                let [u] = found[..] else {
                    return no(if found.is_empty() { format!("no commit {hash} is linked to a task") } else { format!("commit {hash} is linked in several units; name one: workriver commit unlink {hash} <unit>") });
                };
                let l = u.links.iter().find(|l| l.hash.starts_with(&hash)).expect("found above");
                let own = u.tasks.iter().find(|t| t.n == l.task).is_some_and(|t| self.is_agent_of(t, actor));
                if !user && self.manager_of(&u.intent) != Some(actor) && !own {
                    return no("a link is taken back by the agent of its task, the manager of its intent, or the user");
                }
                CommitUnlinked { unit: u.id.clone(), hash: l.hash.clone() }
            }
            UnitCreated { intent, title, purpose, repo, branch, base, machine, kind, .. } => {
                let i = self.intent(&intent)?;
                if matches!(i.state, IntentState::Done | IntentState::Dropped) {
                    return no(format!("intent {intent} is {}; it takes no new units", name(&i.state)));
                }
                // A unit is part of the intent's plan: its manager's, or the user's.
                match self.manager_of(&intent) {
                    _ if user => {}
                    Some(m) if m == actor => {}
                    Some(m) => {
                        return no(format!(
                            "only the user or the manager of intent {intent} ({}) adds its units; tell the manager (`workriver manager tell <your task> --text \"…\"`)",
                            self.name_of(m).unwrap_or(m)
                        ));
                    }
                    None => return no(format!("{intent} has no manager: units are added by the manager of the workspace it runs in, once it is started, or by the user")),
                }
                let id = format!("u{}", self.units.len() + 1);
                // Research and design: no repository, branch or machine of its own; it reads
                // in its intent's project folder.
                if kind == UnitKind::Research {
                    if blank(&title) {
                        return no("a unit needs a title");
                    }
                    if !blank(&branch) || !blank(&base) || !blank(&repo) || machine.is_some() {
                        return no("a research unit has no repository, branch, base or machine: leave out --repo, --branch, --base and --machine");
                    }
                    let folder = self.projects.iter().find(|p| p.id == i.project).map_or(&i.project, |p| &p.path).clone();
                    return Ok(UnitCreated { id, intent, title, purpose, repo: folder, branch, base, machine, kind });
                }
                for (field, v) in [("title", &title), ("repo", &repo), ("branch", &branch), ("base", &base)] {
                    if blank(v) {
                        return no(format!("a unit needs a {field}"));
                    }
                }
                unit_branch(&branch, &base, true, kind)?;
                self.may_place(who, &intent, machine.as_deref())?;
                UnitCreated { id, intent, title, purpose, repo, branch, base, machine, kind }
            }
            UnitMerged { unit, commits } => {
                let u = self.unit(&unit)?;
                if u.kind == UnitKind::Spike {
                    return no(format!("unit {unit} is a spike: it is never merged; close it with `workriver unit done {unit} --reason \"<what was learned>\"`"));
                }
                if !u.kind.merges() {
                    return no(format!("unit {unit} is a research unit: it is never merged; close it with `workriver unit done {unit} --reason \"...\"`"));
                }
                if u.merged.is_some() {
                    return no(format!("unit {unit} is already merged"));
                }
                UnitMerged { unit, commits }
            }
            UnitDone { unit, reason } => {
                let u = self.unit(&unit)?;
                if u.merged.is_some() || u.done.is_some() {
                    return no(format!("unit {unit} is already {}", if u.merged.is_some() { "merged" } else { "done" }));
                }
                if !user && self.manager_of(&u.intent) != Some(actor) {
                    let manager = self.manager_of(&u.intent).map(|m| format!(" ({})", self.name_of(m).unwrap_or(m))).unwrap_or_default();
                    return no(format!("only the manager of intent {}{manager} or the user closes its units", u.intent));
                }
                UnitDone { unit, reason }
            }
            other => unreachable!("{} is not one of a unit's kinds", name_of_kind(&other)),
        })
    }

    /// What a unit's events change: `apply`, for these kinds.
    pub(crate) fn apply_units(&mut self, e: &Event) {
        use Body::*;
        let at = &e.at;
        match &e.body {
            CommitLinked { unit, hash, patch_id, subject, date, task } => {
                if let Some(u) = self.units.iter_mut().find(|u| &u.id == unit) {
                    // One link per commit: the later one stands.
                    u.links.retain(|l| &l.hash != hash);
                    u.links.push(CommitLink { hash: hash.clone(), patch_id: patch_id.clone(), subject: subject.clone(), date: date.clone(), task: *task, by: e.actor.clone() });
                }
            }
            CommitUnlinked { unit, hash } => {
                if let Some(u) = self.units.iter_mut().find(|u| &u.id == unit) {
                    u.links.retain(|l| &l.hash != hash);
                }
            }
            UnitRelocated { unit, machine, repo, branch, base, previous } => {
                if let Some(u) = self.units.iter_mut().find(|u| &u.id == unit) {
                    // Where it was is kept, with what was seen there.
                    if (&u.machine, &u.repo, &u.branch, &u.base) != (machine, repo, branch, base) {
                        u.earlier.push(Earlier { machine: u.machine.clone(), repo: u.repo.clone(), branch: u.branch.clone(), base: u.base.clone(), until: at.clone(), commits: previous.clone() });
                    }
                    u.machine = machine.clone();
                    u.repo = repo.clone();
                    u.branch = branch.clone();
                    u.base = base.clone();
                }
            }
            UnitCreated { id, intent, title, purpose, repo, branch, base, machine, kind } => self.units.push(Unit {
                kind: *kind,
                machine: machine.clone(),
                id: id.clone(),
                intent: intent.clone(),
                title: title.clone(),
                purpose: purpose.clone(),
                repo: repo.clone(),
                branch: branch.clone(),
                base: base.clone(),
                at: at.clone(),
                merged: None,
                merged_commits: vec![],
                tasks: vec![],
                earlier: vec![],
                links: vec![],
                done: None,
            }),
            UnitMerged { unit, commits } => {
                if let Some(u) = self.units.iter_mut().find(|u| &u.id == unit) {
                    u.merged = Some(at.clone());
                    u.merged_commits = commits.clone();
                }
            }
            UnitDone { unit, reason } => {
                if let Some(u) = self.units.iter_mut().find(|u| &u.id == unit) {
                    u.done = Some(Closed { at: at.clone(), by: e.actor.clone(), reason: reason.clone() });
                }
            }
            other => unreachable!("{} is not one of a unit's kinds", name_of_kind(other)),
        }
    }
}
