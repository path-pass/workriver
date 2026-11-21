// Intents: the roadmap of a project, an intent's life from planned to done or dropped, and
// who decides its findings.
use serde::{Deserialize, Serialize};

use crate::*;

/// Where an intent is: in the backlog, being worked on, finished, or given up.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum IntentState {
    Planned,
    Active,
    Done,
    Dropped,
}

pub(crate) fn active() -> IntentState {
    IntentState::Active
}

/// What kind of work an intent is: the first three are the branch prefixes its units take
/// (feat/, fix/, chore/), the fourth the research unit. Closed: a filter and a comparison need
/// one meaning per value, and each is UI text, translated.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[serde(rename_all = "lowercase")]
pub enum IntentKind {
    /// New behaviour.
    Feature,
    /// Behaviour that is wrong: a bug, a hole, a flaky test.
    Fix,
    /// No new behaviour: refactor, docs, build, dependencies, release.
    Chore,
    /// The outcome is a decision or a report, not code.
    Research,
}

/// How big an intent is expected to be: an estimate made before the work exists.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[serde(rename_all = "lowercase")]
pub enum IntentSize {
    /// One unit, one round of implement and review; hours.
    S,
    /// One or two units, a few review rounds; a day or two.
    M,
    /// Three units or more, or a design first; days.
    L,
}

/// The most tags an intent carries.
pub const MAX_TAGS: usize = 8;

/// An intent's tags as they are kept: each trimmed and lower-case (other scripts as written),
/// repeats dropped, in the order given; refused, naming it, when one breaks the limits (1–32
/// characters, no space or comma) or there are more than `MAX_TAGS`. More than four times
/// `MAX_TAGS` given, repeats or not, is refused before any is read: the rules run under the
/// service's one lock on the store, and any agent may add an intent.
pub fn tags(given: &[String]) -> Result<Vec<String>, Rejected> {
    if given.len() > 4 * MAX_TAGS {
        return no(format!("an intent has at most {MAX_TAGS} tags; {} given", given.len()));
    }
    let mut out: Vec<String> = vec![];
    for t in given {
        let t = t.trim().to_lowercase();
        let n = t.chars().count();
        if n == 0 || n > 32 {
            return no(format!("a tag is 1–32 characters: «{t}» is not"));
        }
        if t.chars().any(|c| c.is_whitespace() || c == ',') {
            return no(format!("a tag has no space or comma: «{t}»; give two tags, or join the words with -"));
        }
        if !out.contains(&t) {
            out.push(t);
        }
    }
    if out.len() > MAX_TAGS {
        return no(format!("an intent has at most {MAX_TAGS} tags; {} given", out.len()));
    }
    Ok(out)
}

/// A merged intent's kind, size and tags, as one line of its block (`` fix · S · `security`, `cli` ``);
/// empty when it has none. Each tag is a code span, so that it reads as written in the note's
/// Markdown, whatever it looks like (`~~~` would open a fence, `---` underline a heading).
fn meta_line(i: &Intent) -> String {
    let size = i.size.map(|z| name(&z).to_uppercase());
    let tags = i.tags.iter().map(|t| literal(t)).collect::<Vec<_>>().join(", ");
    [i.kind.map(|k| name(&k)), size, (!tags.is_empty()).then_some(tags)].into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

/// A tag as a Markdown code span: fenced by one backtick more than its longest run of them,
/// and spaced inside when it starts or ends with one (a tag has no space to lose).
fn literal(tag: &str) -> String {
    let fence = "`".repeat(tag.split(|c| c != '`').map(str::len).max().unwrap_or(0) + 1);
    let pad = if tag.starts_with('`') || tag.ends_with('`') { " " } else { "" };
    format!("{fence}{pad}{tag}{pad}{fence}")
}

/// The same item in an outside tracker (Jira, Linear, GitHub Issues); only kept and shown.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct External {
    pub system: String,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Who decides an intent's open findings; set by the user alone.
///
/// There is no policy by severity: a finding's severity is its reviewer's own rating, so a
/// rule that kept "P1" for the user would look like a control and be none.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FindingsPolicy {
    /// The user decides every finding. (Also what a record of the withdrawn
    /// "manager-except-p1" replays as: the stricter of the two.)
    #[serde(alias = "manager-except-p1")]
    User,
    /// The intent's manager decides them: the default. The user can still decide any
    /// finding, and change what the manager decided.
    #[default]
    Manager,
}

impl FindingsPolicy {
    /// Whether the intent's manager decides its findings.
    pub fn managers(self) -> bool {
        self == FindingsPolicy::Manager
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Intent {
    pub id: String,
    pub title: String,
    /// Before 0.2: the manager registered for it. Now the manager is the workspace's.
    pub manager: Option<String>,
    #[serde(default)]
    pub goal: String,
    /// Before 0.2: the runtime group it followed.
    #[serde(default)]
    pub runtime_ref: Option<RuntimeRef>,
    pub at: String,
    /// The project folder ("" when not known: then its units' repositories say).
    #[serde(default)]
    pub project: String,
    #[serde(default = "active")]
    pub state: IntentState,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub external: Option<External>,
    /// Who added it: "user", or the agent (session) that did. Only the user merges what the
    /// user added; agents may tidy what agents added (an import split too finely).
    #[serde(default)]
    pub by: String,
    /// Where it runs while active (and last ran).
    #[serde(default)]
    pub workspace: Option<RuntimeRef>,
    /// Why it was dropped.
    #[serde(default)]
    pub reason: Option<String>,
    /// Who decides its open findings.
    #[serde(default)]
    pub findings: FindingsPolicy,
    /// Intent-level work (research, design), numbered `i3#1`…
    #[serde(default)]
    pub tasks: Vec<Task>,
    /// What kind of work it is; none: not said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<IntentKind>,
    /// How big it is expected to be; none: not estimated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<IntentSize>,
    /// Free words for its area or theme, as kept by `tags`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Its versions, as far as the rules and the board need them; its history is read from
    /// its events (`intent_versions`). Never serialized: derived on replay, and the history
    /// is not what every read of the state should carry.
    #[serde(skip)]
    pub edits: Edits,
}

/// Where an intent's text stands: its last version, who made it and when, and the version
/// that last changed each of its title, goal, note, kind, size and tags (0: never set). A
/// version is `intent.created`, and each `intent.updated` that sets one of them (or adds to
/// the note), and each merge into it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Edits {
    pub version: u32,
    pub at: String,
    /// Who made the last version: "user", or the agent's session.
    pub by: String,
    pub title: u32,
    pub goal: u32,
    pub note: u32,
    #[serde(default, skip_serializing_if = "zero")]
    pub kind: u32,
    #[serde(default, skip_serializing_if = "zero")]
    pub size: u32,
    #[serde(default, skip_serializing_if = "zero")]
    pub tags: u32,
}

fn zero(n: &u32) -> bool {
    *n == 0
}

/// Which fields of an intent a version changed.
#[derive(Clone, Copy, Default)]
struct Changed {
    title: bool,
    goal: bool,
    note: bool,
    kind: bool,
    size: bool,
    tags: bool,
}

impl Edits {
    /// A new version, by `by` at `at`, changing what is flagged.
    fn bump(&mut self, at: &str, by: &str, c: Changed) {
        self.version += 1;
        self.at = at.to_string();
        self.by = by.to_string();
        let n = self.version;
        for (changed, field) in [(c.title, &mut self.title), (c.goal, &mut self.goal), (c.note, &mut self.note), (c.kind, &mut self.kind), (c.size, &mut self.size), (c.tags, &mut self.tags)] {
            if changed {
                *field = n;
            }
        }
    }
}

/// What a merged intent said, as a block of its target's note: its title, its kind, size and
/// tags, its goal, note and external id (with its link), one per line, empty ones left out.
fn merged_note(i: &Intent) -> String {
    let external = i.external.as_ref().map(|e| format!("{}:{}{}", e.system, e.key, e.url.as_deref().map(|u| format!(" {u}")).unwrap_or_default()));
    [format!("Merged from {}: {}", i.id, i.title), meta_line(i), i.goal.trim().to_string(), i.note.trim().to_string(), external.unwrap_or_default()].into_iter().filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

impl State {
    pub fn intent(&self, id: &str) -> Result<&Intent, Rejected> {
        match self.intents.iter().find(|i| i.id == id) {
            Some(i) => Ok(i),
            None => no(format!("no intent {id}")),
        }
    }

    /// The intent a unit (or intent) belongs to.
    pub fn intent_of(&self, owner: &str) -> Option<&str> {
        self.units.iter().find(|u| u.id == owner).map(|u| u.intent.as_str()).or_else(|| self.intents.iter().find(|i| i.id == owner).map(|i| i.id.as_str()))
    }

    /// The manager of an intent: the manager of the workspace record it runs in, or last ran
    /// in. One whose workspace has no manager, or no record, has none: the manager registered
    /// for the intent itself (before 0.2) counts only for an intent with no workspace at all,
    /// or it would follow the intent into a workspace where nobody registered.
    pub fn manager_of(&self, intent: &str) -> Option<&str> {
        let i = self.intents.iter().find(|i| i.id == intent)?;
        match &i.workspace {
            Some(g) => self.workspace_of(g).and_then(|w| w.manager.as_deref()),
            None => i.manager.as_deref(),
        }
    }

    /// Whether every unit of `intent` is merged or done (and it has one).
    pub fn finished(&self, intent: &str) -> bool {
        let mut units = self.units.iter().filter(|u| u.intent == intent).peekable();
        units.peek().is_some() && units.all(|u| matches!(self.progress(u), Progress::Merged | Progress::Done))
    }

    /// The rules of an intent's events: `prepare`, for these kinds.
    pub(crate) fn prepare_intents(&self, who: &Principal, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        let actor = who.actor();
        let user = *who == Principal::User;
        Ok(match body {
            IntentCreated { title, goal, runtime_ref, project, state, note, external, kind, size, tags: given, .. } => {
                if blank(&title) {
                    return no("an intent needs a title");
                }
                if runtime_ref.as_ref().is_some_and(|r| blank(&r.id)) {
                    return no("a runtime group needs an id");
                }
                if !matches!(state, IntentState::Planned | IntentState::Active) {
                    return no("a new intent is planned (in the backlog) or active");
                }
                // Where an intent runs (a workspace's current intent, its project, the intent's
                // manager) is set by a start the rules check, or by the user; an agent's intent
                // starts in the roadmap. (Before 0.2 an intent was made in its runtime group.)
                if !user && (runtime_ref.is_some() || state == IntentState::Active) {
                    return no("an agent adds an intent to the roadmap, planned and in no workspace (`workriver backlog add`); it runs where it is started (`workriver intent start`)");
                }
                let tags = tags(&given)?;
                IntentCreated { id: format!("i{}", self.intents.len() + 1), title, goal, runtime_ref, project, state, note, external, kind, size, tags }
            }
            IntentStateChanged { intent, state, reason, workspace } => {
                let i = self.intent(&intent)?;
                use IntentState::*;
                // Where an intent runs is set by a start: a close that named a workspace would
                // move the intent there, and with it that workspace's project.
                if state != Active && workspace.is_some() {
                    return no("a state change names a workspace only when it starts an intent: leave the workspace out (an intent stays recorded where it last ran)");
                }
                match (i.state, state) {
                    (Planned, Active) => {
                        let Some(g) = &workspace else { return no(format!("start {intent} from its manager's terminal: workriver intent start {intent}")) };
                        let manager = self.workspace_of(g).and_then(|w| w.manager.as_deref());
                        if !user && manager != Some(actor) {
                            return no(format!("only the manager of workspace {} starts intents there; register with `workriver manager here` first", g.label));
                        }
                        // A manager starts its workspace's project's intents only: else it would become
                        // the manager of another project's intent (and make its workspace that
                        // project's). The user may start any, anywhere.
                        // Where its workspace's project is not known by the records, its start would
                        // make it that project's: the user starts the first one, or sets it.
                        let theirs = self.intent_project(i);
                        // One of no known project (records from before projects) is the user's to start.
                        if !user && theirs.is_empty() {
                            return no(format!("{intent} has no project: ask the user to start it (in the roadmap, in a workspace of the project it belongs to)"));
                        }
                        if !user {
                            let title = |p: &str| self.project_of(p).map_or(p.to_string(), |x| x.title.clone());
                            match self.workspace_of(g).and_then(|w| self.workspace_project(w)) {
                                Some(ours) if ours == theirs => {}
                                Some(ours) => {
                                    return no(format!(
                                        "{intent} belongs to project {}; your workspace works on {}: ask the user to start it (in the roadmap, in a workspace of its project) or to set your workspace's project (Settings, or `workriver workspace project`)",
                                        title(&theirs),
                                        title(&ours)
                                    ));
                                }
                                None => {
                                    return no(format!(
                                        "{intent} belongs to project {}; your workspace's project is not known yet: ask the user to start {intent} from the roadmap in your workspace, or to set your workspace's project (Settings, or `workriver workspace project`); then you start that project's intents yourself",
                                        title(&theirs)
                                    ));
                                }
                            }
                        }
                    }
                    // Closing or shelving an intent is its manager's or the user's, as dropping it is:
                    // who may edit an intent goes by its state.
                    (Active, Done) | (Active, Planned) => {
                        if !user && self.manager_of(&intent) != Some(actor) {
                            return no(format!("only the user or the manager of intent {intent} marks it {}", name(&state)));
                        }
                    }
                    (Planned, Dropped) | (Active, Dropped) => {
                        // In the roadmap it is the user's, whoever managed it when it ran.
                        if !user && i.state == Planned {
                            return no(format!("only the user drops planned intent {intent} (in the roadmap, or `workriver intent drop` from the user's terminal)"));
                        }
                        if !user && self.manager_of(&intent) != Some(actor) {
                            return no(format!("only the user or the manager of intent {intent} drops it"));
                        }
                        if reason.as_deref().is_none_or(blank) {
                            return no("dropping an intent needs a reason: --reason \"...\"");
                        }
                    }
                    (from, to) => return no(format!("intent {intent} is {}; it cannot become {}", name(&from), name(&to))),
                }
                IntentStateChanged { intent, state, reason, workspace }
            }
            IntentMoved { intent, before, after } => {
                let i = self.intent(&intent)?;
                if i.state != IntentState::Planned {
                    return no(format!("intent {intent} is not in the roadmap (it is {})", name(&i.state)));
                }
                let Some(other) = before.as_ref().or(after.as_ref()).filter(|_| before.is_none() || after.is_none()) else {
                    return no("move it --before or --after one other planned intent");
                };
                let o = self.intent(other)?;
                if o.state != IntentState::Planned || other == &intent {
                    return no(format!("{other} is not another planned intent"));
                }
                IntentMoved { intent, before, after }
            }
            IntentMerged { intent, from, title } => {
                let i = self.intent(&intent)?;
                if i.state != IntentState::Planned {
                    return no(format!("intent {intent} is {}; only planned intents are merged", name(&i.state)));
                }
                if title.as_deref().is_some_and(blank) {
                    return no("the new title is empty: give one, or leave --title out to keep the title");
                }
                if title.is_some() && !user {
                    return no(format!("the title is the user's: merge without --title (it keeps {intent}'s); the user renames it on the intent's page"));
                }
                if from.is_empty() {
                    return no(format!("name the planned intents to merge into {intent}"));
                }
                // The user's own goals are the user's to merge; what agents added, agents may tidy.
                if !user && let Some(mine) = std::iter::once(&intent).chain(&from).find(|x| self.intents.iter().any(|i| &i.id == *x && i.by == "user")) {
                    return no(format!("{mine} was added by the user: only the user merges it (in the roadmap, or `workriver backlog merge` from the user's terminal)"));
                }
                for (k, f) in from.iter().enumerate() {
                    if f == &intent || from[..k].contains(f) {
                        return no(format!("{f} is named twice: give each intent to merge once, and not {intent} itself"));
                    }
                    let x = self.intent(f)?;
                    if x.state != IntentState::Planned {
                        return no(format!("intent {f} is {}; only planned intents are merged", name(&x.state)));
                    }
                    if x.project != i.project {
                        return no(format!("{f} is in another project than {intent}"));
                    }
                    // Work, findings or chores of its own would be lost with it.
                    let held = [
                        ("tasks", !x.tasks.is_empty()),
                        ("units", self.units.iter().any(|u| &u.intent == f)),
                        ("backlog findings", self.backlog.iter().any(|b| b.intent.as_deref() == Some(f))),
                        ("chores", self.chores.iter().any(|c| &c.intent == f)),
                        ("conversations", self.threads.iter().any(|t| &t.unit == f)),
                    ];
                    if let Some((what, _)) = held.iter().find(|(_, has)| *has) {
                        return no(format!("{f} has {what} of its own; only an intent with nothing attached is merged (drop it with a reason instead)"));
                    }
                }
                IntentMerged { intent, from, title: title.map(|t| t.trim().to_string()) }
            }
            IntentLinked { intent, runtime_ref } => {
                self.intent(&intent)?;
                if blank(&runtime_ref.id) {
                    return no("a runtime group needs an id");
                }
                IntentLinked { intent, runtime_ref }
            }
            IntentUpdated { intent, title, goal, findings, add_note, note, base, kind, size, tags: given } => {
                let i = self.intent(&intent)?;
                if title.is_none() && goal.is_none() && findings.is_none() && add_note.is_none() && note.is_none() && kind.is_none() && size.is_none() && given.is_none() {
                    return no("nothing to change: give a title, a goal, --note, --add-note, --findings, --kind, --size or --tags");
                }
                if note.is_some() && add_note.is_some() {
                    return no("give --note or --add-note, not both: --note replaces the whole note");
                }
                if title.as_deref().is_some_and(blank) {
                    return no("an intent needs a title");
                }
                if add_note.as_deref().is_some_and(blank) {
                    return no("the note to add is empty: --add-note \"...\"");
                }
                let set = Changed { title: title.is_some(), goal: goal.is_some(), note: note.is_some(), kind: kind.is_some(), size: size.is_some(), tags: given.is_some() };
                // Who may set them before the tags are read: an agent with no right on the
                // intent is refused without the work.
                if !user {
                    self.may_edit(actor, i, set, findings.is_some())?;
                }
                let tags = given.as_deref().map(tags).transpose()?;
                self.unchanged_since(i, base, set)?;
                // The whole note as written: its indentation and blank lines are Markdown.
                IntentUpdated { intent, title, goal, findings, add_note: add_note.map(|n| n.trim().to_string()), note, base, kind, size, tags }
            }
            other => unreachable!("{} is not one of an intent's kinds", name_of_kind(&other)),
        })
    }

    /// What an intent's events change: `apply`, for these kinds.
    pub(crate) fn apply_intents(&mut self, e: &Event) {
        use Body::*;
        let at = &e.at;
        match &e.body {
            IntentCreated { id, title, goal, runtime_ref, project, state, note, external, kind, size, tags } => {
                self.intents.push(Intent {
                    id: id.clone(),
                    title: title.clone(),
                    manager: None,
                    goal: goal.clone(),
                    runtime_ref: runtime_ref.clone(),
                    at: at.clone(),
                    project: project.clone(),
                    state: *state,
                    note: note.clone(),
                    external: external.clone(),
                    workspace: runtime_ref.clone(),
                    reason: None,
                    findings: FindingsPolicy::default(),
                    tasks: vec![],
                    by: e.actor.clone(),
                    kind: *kind,
                    size: *size,
                    tags: tags.clone(),
                    edits: Edits::default(),
                });
                if let Some(i) = self.intents.last_mut() {
                    // Kind, size and tags only when given: an intent without them is as before.
                    i.edits.bump(at, &e.actor, Changed { title: true, goal: true, note: true, kind: kind.is_some(), size: size.is_some(), tags: !tags.is_empty() });
                }
                if *state == IntentState::Planned {
                    self.roadmap.push(id.clone());
                }
                // Before 0.2 an intent was its runtime group's: it is that group's current intent.
                if let Some(g) = runtime_ref
                    && *state == IntentState::Active
                {
                    self.workspace_mut(g).current = Some(id.clone());
                }
            }
            IntentStateChanged { intent, state, reason, workspace } => {
                let Some(k) = self.intents.iter().position(|i| &i.id == intent) else { return };
                let before = self.intents[k].workspace.clone();
                {
                    let i = &mut self.intents[k];
                    i.state = *state;
                    if reason.is_some() {
                        i.reason = reason.clone();
                    }
                    if let Some(g) = workspace {
                        i.workspace = Some(g.clone());
                    }
                }
                self.roadmap.retain(|x| x != intent);
                match state {
                    IntentState::Active => {
                        if let Some(g) = workspace {
                            self.workspace_mut(g).current = Some(intent.clone());
                        }
                    }
                    _ => {
                        // No longer worked on: no longer where new work goes. New work then
                        // goes to the intent started last among those still active there.
                        if let Some(g) = before {
                            let at = self.workspace_at(&g);
                            let next = self.intents.iter().rev().find(|i| i.state == IntentState::Active && &i.id != intent && i.workspace.as_ref().is_some_and(|x| self.workspace_at(x) == at && at.is_some())).map(|i| i.id.clone());
                            // Found, never renamed to what the intent was started with.
                            let w = self.workspace_found_mut(&g);
                            if w.current.as_deref() == Some(intent.as_str()) {
                                w.current = next;
                            }
                        }
                        if *state == IntentState::Planned {
                            self.roadmap.push(intent.clone());
                        }
                    }
                }
            }
            IntentMoved { intent, before, after } => {
                self.roadmap.retain(|x| x != intent);
                let at = before.as_ref().or(after.as_ref()).and_then(|o| self.roadmap.iter().position(|x| x == o));
                let k = match (at, before.is_some()) {
                    (Some(k), true) => k,
                    (Some(k), false) => k + 1,
                    (None, _) => self.roadmap.len(),
                };
                self.roadmap.insert(k, intent.clone());
            }
            IntentMerged { intent, from, title } => {
                let mut blocks = vec![];
                let mut tags = vec![];
                for f in from {
                    if let Some(x) = self.intents.iter_mut().find(|i| &i.id == f) {
                        blocks.push(merged_note(x));
                        tags.extend(x.tags.iter().cloned());
                        x.state = IntentState::Dropped;
                        x.reason = Some(format!("merged into {intent}"));
                    }
                    self.roadmap.retain(|r| r != f);
                }
                if let Some(i) = self.intents.iter_mut().find(|i| &i.id == intent) {
                    let own = i.note.trim_end().to_string();
                    i.note = std::iter::once(own).chain(blocks).filter(|b| !b.is_empty()).collect::<Vec<_>>().join("\n\n");
                    if let Some(t) = title {
                        i.title = t.clone();
                    }
                    // The tags are the user's to widen: only the user's merge unions them (its
                    // own first, cut to the most kept); an agent's leaves them in the note.
                    let mut union = i.tags.clone();
                    if e.actor == Principal::User.actor() {
                        for t in tags {
                            if !union.contains(&t) && union.len() < MAX_TAGS {
                                union.push(t);
                            }
                        }
                    }
                    let widened = union != i.tags;
                    i.tags = union;
                    i.edits.bump(at, &e.actor, Changed { title: title.is_some(), note: true, tags: widened, ..Changed::default() });
                }
            }
            IntentUpdated { intent, title, goal, findings, add_note, note, kind, size, tags, .. } => {
                if let Some(i) = self.intents.iter_mut().find(|i| &i.id == intent) {
                    if let Some(n) = note {
                        i.note = n.clone();
                    }
                    if let Some(n) = add_note {
                        let own = i.note.trim_end();
                        i.note = if own.is_empty() { n.clone() } else { format!("{own}\n\n{n}") };
                    }
                    if let Some(t) = title {
                        i.title = t.clone();
                    }
                    if let Some(g) = goal {
                        i.goal = g.clone();
                    }
                    if let Some(f) = findings {
                        i.findings = *f;
                    }
                    if let Some(k) = kind {
                        i.kind = *k;
                    }
                    if let Some(z) = size {
                        i.size = *z;
                    }
                    if let Some(t) = tags {
                        i.tags = t.clone();
                    }
                    let c = Changed { title: title.is_some(), goal: goal.is_some(), note: note.is_some() || add_note.is_some(), kind: kind.is_some(), size: size.is_some(), tags: tags.is_some() };
                    if c.title || c.goal || c.note || c.kind || c.size || c.tags {
                        i.edits.bump(at, &e.actor, c);
                    }
                }
            }
            IntentLinked { intent, runtime_ref } => {
                let mut active = false;
                if let Some(i) = self.intents.iter_mut().find(|i| &i.id == intent) {
                    i.runtime_ref = Some(runtime_ref.clone());
                    i.workspace = Some(runtime_ref.clone());
                    active = i.state == IntentState::Active;
                }
                if active {
                    self.workspace_mut(runtime_ref).current = Some(intent.clone());
                }
            }
            other => unreachable!("{} is not one of an intent's kinds", name_of_kind(other)),
        }
    }
}

impl State {
    /// Whether agent `actor` may make this change to intent `i` (the user may make any): title,
    /// goal and findings are the user's; the note, kind, size and tags are its manager's while
    /// it is active; a planned one takes additions to its note from a manager of a workspace of
    /// its project, and nothing more; a done or dropped one is the user's.
    fn may_edit(&self, actor: &str, i: &Intent, set: Changed, findings: bool) -> Result<(), Rejected> {
        let (title_or_goal, replace, meta) = (set.title || set.goal, set.note, set.kind || set.size || set.tags);
        let id = &i.id;
        // Who decides findings is the user's to say, never the one it is handed to.
        if findings {
            return no("only the user says who decides an intent's findings (workriver intent set <intent> --findings …, or in the web UI)");
        }
        if title_or_goal {
            return no(format!("the title and goal of {id} are the user's: ask the user to change them (on the intent's page); write what you know in its note"));
        }
        match i.state {
            IntentState::Active if self.manager_of(id) == Some(actor) => Ok(()),
            IntentState::Active if meta => no(format!("only the user or the manager of intent {id} changes its kind, size and tags")),
            IntentState::Active => no(format!("only the user or the manager of intent {id} changes its note")),
            IntentState::Planned if meta => no(format!("{id} is planned: its kind, size and tags are the user's (whoever adds an intent sets them; its manager, once it is started); ask the user, or write what you think in its note")),
            IntentState::Planned => {
                let mine: Vec<&Workspace> = self.workspaces.iter().filter(|w| w.manager.as_deref() == Some(actor)).collect();
                if mine.is_empty() {
                    return no(format!("{id} is planned: only the user, or a manager of a workspace of its project with --add-note, adds to its note"));
                }
                if replace {
                    return no(format!("{id} is planned: a manager adds to its note (--add-note), nothing more; the rest is the user's"));
                }
                let theirs = self.intent_project(i);
                let name = self.project_of(&theirs).map_or(theirs.clone(), |p| p.title.clone());
                let known: Vec<String> = mine.iter().filter_map(|w| self.workspace_project(w)).collect();
                if !theirs.is_empty() && known.contains(&theirs) {
                    return Ok(());
                }
                if known.is_empty() || theirs.is_empty() {
                    return no(format!("{id} belongs to project {name}; your workspace's project is not known: ask the user to set it (Settings, or `workriver workspace project`)"));
                }
                no(format!("{id} belongs to project {name}; no workspace you manage works on it, so its note is not yours to add to"))
            }
            IntentState::Done | IntentState::Dropped => no(format!("intent {id} is {}: only the user changes it now", name(&i.state))),
        }
    }

    /// Refuses a replace made from version `base` when a field it sets changed after it.
    fn unchanged_since(&self, i: &Intent, base: Option<u32>, set: Changed) -> Result<(), Rejected> {
        let Some(base) = base else { return Ok(()) };
        let id = &i.id;
        let e = &i.edits;
        if base > e.version || base == 0 {
            return no(format!("{id} has no version {base}; it is at v{}", e.version));
        }
        for (set, changed, what) in [(set.title, e.title, "title"), (set.goal, e.goal, "goal"), (set.note, e.note, "note"), (set.kind, e.kind, "kind"), (set.size, e.size, "size"), (set.tags, e.tags, "tags")] {
            if set && changed > base {
                let by = self.agents.iter().find(|a| a.session == e.by).map_or(e.by.as_str(), |a| a.name.as_str());
                let by = if by == "user" { "the user" } else { by };
                let n = e.version;
                return no(format!("{id}'s {what} changed since v{base} (in v{changed}; it is at v{n}, last by {by} at {}); read it (`workriver intent history {id} --version {changed}`) and run again with `--base {n}`", e.at));
            }
        }
        Ok(())
    }

    /// The project a workspace works on, from the records: what the user set, else the
    /// project of its current intent, else the one project of the intents that ran in it
    /// (none, or several: not known). Never what an agent could steer: a manager starts only
    /// its workspace's project's intents, and none of a known project while this is not known.
    pub fn workspace_project(&self, w: &Workspace) -> Option<String> {
        if let Some(p) = &w.project {
            return Some(p.clone());
        }
        let current = w.current.as_deref().and_then(|c| self.intents.iter().find(|i| i.id == c)).map(|i| self.intent_project(i)).filter(|p| !p.is_empty());
        current.or_else(|| {
            let ran: std::collections::BTreeSet<String> = self
                .intents
                .iter()
                .filter(|i| i.workspace.as_ref().and_then(|g| self.workspace_of(g)).is_some_and(|x| x.group == w.group))
                .map(|i| self.intent_project(i))
                .filter(|p| !p.is_empty())
                .collect();
            (ran.len() == 1).then(|| ran.into_iter().next()).flatten()
        })
    }

    /// The registered project whose folder holds `path`, the deepest one.
    pub fn project_of(&self, path: &str) -> Option<&Project> {
        self.projects.iter().filter(|p| std::path::Path::new(path).starts_with(&p.path)).max_by_key(|p| p.path.len())
    }

    /// An intent's project for the rules: the one recorded on it (the registered project
    /// holding that folder when there is one), never its units'. Empty: not known, whatever
    /// its units are, so that no agent picks an intent's project by adding a unit.
    pub fn intent_project(&self, i: &Intent) -> String {
        if i.project.is_empty() {
            return String::new();
        }
        self.project_of(&i.project).map_or_else(|| i.project.clone(), |p| p.path.clone())
    }

    /// An intent's project folder to show (the page, a notice, the start warning): as
    /// recorded, else where its first unit's repository is; the registered project holding it
    /// when there is one. Empty: not known. The rules read `intent_project`.
    pub fn project_folder(&self, i: &Intent) -> String {
        let path = if i.project.is_empty() { self.units.iter().find(|u| u.intent == i.id).map(|u| u.repo.clone()).unwrap_or_default() } else { i.project.clone() };
        if path.is_empty() {
            return path;
        }
        self.project_of(&path).map(|p| p.path.clone()).unwrap_or(path)
    }
}
