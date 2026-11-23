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

impl IntentState {
    /// Done or dropped: no longer worked on.
    pub fn over(self) -> bool {
        matches!(self, IntentState::Done | IntentState::Dropped)
    }
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
    #[serde(default)]
    pub goal: String,
    pub at: String,
    /// The project's folder; empty: of no project.
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
    /// The workspace (its id) it runs in while active, and last ran in; none: never started.
    #[serde(default)]
    pub workspace: Option<String>,
    /// Why it was dropped.
    #[serde(default)]
    pub reason: Option<String>,
    /// Who decides its open findings.
    #[serde(default)]
    pub findings: FindingsPolicy,
    /// What kind of work it is; none: not said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<IntentKind>,
    /// How big it is expected to be; none: not estimated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<IntentSize>,
    /// Free words for its area or theme, as kept by `tags`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// When it was last made done or dropped; none while planned or active. When the files
    /// only it holds start to count towards the attachment rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed: Option<String>,
    /// Its versions, as far as the rules and the board need them; its history is read from
    /// its events (`intent_versions`). Never serialized: derived on replay, and the history
    /// is not what every read of the state should carry.
    #[serde(skip)]
    pub edits: Edits,
    /// The start and end of each active interval, derived in event order. The last end is
    /// unset while the intent is active.
    #[serde(skip)]
    pub(crate) active_intervals: Vec<(String, Option<String>)>,
}

impl Intent {
    fn change_state(&mut self, state: IntentState, at: &str) {
        if self.state != state {
            if state == IntentState::Active {
                self.active_intervals.push((at.to_string(), None));
            } else if self.state == IntentState::Active && let Some((_, end)) = self.active_intervals.last_mut() {
                *end = Some(at.to_string());
            }
        }
        self.state = state;
        self.closed = state.over().then(|| at.to_string());
    }
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
    #[serde(default, skip_serializing_if = "crate::records::is_zero")]
    pub kind: u32,
    #[serde(default, skip_serializing_if = "crate::records::is_zero")]
    pub size: u32,
    #[serde(default, skip_serializing_if = "crate::records::is_zero")]
    pub tags: u32,
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

    /// The manager of an intent: who holds the seat of the workspace it runs in, or last ran
    /// in. One that never ran, or whose workspace's seat is empty, has none.
    pub fn manager_of(&self, intent: &str) -> Option<&str> {
        self.intent_workspace(intent)?.manager.as_ref().map(|m| m.session.as_str())
    }

    /// Refuses an intent of a project in a workspace of another: an intent runs in a
    /// workspace of its project. One of no project passes (only the user gets here with one;
    /// it then takes the workspace's project).
    fn of_its_project(&self, i: &Intent, w: &Workspace) -> Result<(), Rejected> {
        let theirs = self.intent_project(i);
        let ours = self.workspace_project(w).map(|p| p.path.as_str()).unwrap_or_default();
        if theirs.is_empty() || theirs == ours {
            return Ok(());
        }
        let title = |p: &str| self.project_of(p).map_or(p.to_string(), |x| x.title.clone());
        no(format!("{} belongs to project {}; workspace {} ({}) is of project {}: an intent runs in a workspace of its project", i.id, title(&theirs), w.id, w.name, title(ours)))
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
            IntentCreated { title, goal, project, state, note, external, kind, size, tags: given, .. } => {
                if blank(&title) {
                    return no("an intent needs a title");
                }
                if !matches!(state, IntentState::Planned | IntentState::Active) {
                    return no("a new intent is planned (in the backlog) or active");
                }
                // Where an intent runs is set by a start the rules check, or by the user; an
                // agent's intent starts in the roadmap.
                if !user && state == IntentState::Active {
                    return no("an agent adds an intent to the roadmap, planned and in no workspace (`workriver backlog add`); it runs where it is started (`workriver intent start`)");
                }
                // A project is registered by the user, or it is none: whoever records the
                // intent, the folder it names is one a registered project holds.
                if !project.is_empty() && self.project_of(&project).is_none() {
                    return no(format!("{project} is in no registered project: name a project by its id (`workriver project list`), or ask the user to add it first (`workriver project add <folder>`, from the user's terminal)"));
                }
                let tags = tags(&given)?;
                IntentCreated { id: format!("i{}", self.intents.len() + 1), title, goal, project, state, note, external, kind, size, tags }
            }
            IntentStateChanged { intent, state, reason, mut workspace } => {
                let i = self.intent(&intent)?;
                use IntentState::*;
                // Where an intent runs is set by a start: a close that named a workspace would
                // move the intent there.
                if state != Active && workspace.is_some() {
                    return no("a state change names a workspace only when it starts an intent: leave the workspace out (an intent stays recorded where it last ran)");
                }
                match (i.state, state) {
                    (Planned, Active) => {
                        // The user: any open workspace of the intent's project. A manager: only
                        // the workspace it holds the seat of, which is filled in here; a body
                        // naming another is refused.
                        let w = match (&workspace, who) {
                            (Some(id), _) if user => self.workspace(id)?,
                            (None, _) if user => return no(format!("say where {intent} starts: `workriver intent start {intent} --workspace <ws>` (`workriver workspace list` shows them), or start it in the roadmap in the web UI")),
                            (named, _) => {
                                let Some(w) = self.seat_of(actor) else {
                                    return no(format!("only a workspace's manager, or the user, starts an intent; {SEATING}"));
                                };
                                if named.as_ref().is_some_and(|id| **id != w.id) {
                                    return no(format!("you manage workspace {} ({}): a manager starts intents there, and nowhere else", w.id, w.name));
                                }
                                w
                            }
                        };
                        if w.closed {
                            return no(format!("workspace {} is closed: open it first, or start {intent} in another", w.id));
                        }
                        // One of no project is the user's to start: it takes the workspace's.
                        if !user && self.intent_project(i).is_empty() {
                            return no(format!("{intent} has no project: ask the user to start it (in the roadmap, in a workspace of the project it belongs to)"));
                        }
                        self.of_its_project(i, w)?;
                        workspace = Some(w.id.clone());
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
                    if self.intent_project(x) != self.intent_project(i) {
                        return no(format!("{f} is in another project than {intent}"));
                    }
                    // Work, findings or chores of its own would be lost with it.
                    let held = [
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
            IntentWorkspaceSet { intent, workspace } => {
                let i = self.intent(&intent)?;
                if i.state != IntentState::Active {
                    return no(format!("intent {intent} is {}: only an active intent is moved to another workspace (a planned one runs where it is started)", name(&i.state)));
                }
                let w = self.workspace(&workspace)?;
                if w.closed {
                    return no(format!("workspace {workspace} is closed: open it first"));
                }
                if i.workspace.as_deref() == Some(&workspace) {
                    return no(format!("{intent} runs in {workspace} already"));
                }
                self.of_its_project(i, w)?;
                IntentWorkspaceSet { intent, workspace }
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
            IntentCreated { id, title, goal, project, state, note, external, kind, size, tags } => {
                self.intents.push(Intent {
                    id: id.clone(),
                    title: title.clone(),
                    goal: goal.clone(),
                    at: at.clone(),
                    project: project.clone(),
                    state: *state,
                    note: note.clone(),
                    external: external.clone(),
                    workspace: None,
                    reason: None,
                    findings: FindingsPolicy::default(),
                    by: e.actor.clone(),
                    kind: *kind,
                    size: *size,
                    tags: tags.clone(),
                    closed: state.over().then(|| at.clone()),
                    edits: Edits::default(),
                    active_intervals: if *state == IntentState::Active { vec![(at.clone(), None)] } else { vec![] },
                });
                if let Some(i) = self.intents.last_mut() {
                    // Kind, size and tags only when given: an intent without them is as before.
                    i.edits.bump(at, &e.actor, Changed { title: true, goal: true, note: true, kind: kind.is_some(), size: size.is_some(), tags: !tags.is_empty() });
                }
                if *state == IntentState::Planned {
                    self.roadmap.push(id.clone());
                }
            }
            IntentStateChanged { intent, state, reason, workspace } => {
                let Some(k) = self.intents.iter().position(|i| &i.id == intent) else { return };
                let before = self.intents[k].workspace.clone();
                {
                    let i = &mut self.intents[k];
                    i.change_state(*state, at);
                    if reason.is_some() {
                        i.reason = reason.clone();
                    }
                }
                self.roadmap.retain(|x| x != intent);
                match state {
                    IntentState::Active => {
                        if let Some(id) = workspace {
                            self.runs_in(k, id);
                        }
                    }
                    _ => {
                        // No longer worked on: no longer where new work goes.
                        if let Some(id) = before {
                            self.left(&id, intent);
                        }
                        if *state == IntentState::Planned {
                            self.roadmap.push(intent.clone());
                        }
                    }
                }
            }
            IntentWorkspaceSet { intent, workspace } => {
                let Some(k) = self.intents.iter().position(|i| &i.id == intent) else { return };
                if let Some(old) = self.intents[k].workspace.clone() {
                    self.left(&old, intent);
                }
                self.runs_in(k, workspace);
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
                        x.change_state(IntentState::Dropped, at);
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
                let Some(mine) = self.seat_of(actor) else {
                    return no(format!("{id} is planned: only the user, or the manager of a workspace of its project with --add-note, adds to its note"));
                };
                if replace {
                    return no(format!("{id} is planned: a manager adds to its note (--add-note), nothing more; the rest is the user's"));
                }
                let theirs = self.intent_project(i);
                // Of no project: no workspace's, whatever the manager's own works on.
                if theirs.is_empty() {
                    return no(format!("{id} is planned and has no project: only the user adds to its note; ask the user to add yours"));
                }
                if self.workspace_project(mine).is_some_and(|p| p.path == theirs) {
                    return Ok(());
                }
                let name = self.project_of(&theirs).map_or(theirs.clone(), |p| p.title.clone());
                no(format!("{id} belongs to project {name}; the workspace you manage is of another project, so its note is not yours to add to"))
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

    /// Intent `k` runs in workspace `id` from now on: it is where new work there goes; one
    /// of no project takes the workspace's (only the user puts such an intent anywhere).
    fn runs_in(&mut self, k: usize, id: &str) {
        let project = self.workspaces.iter().find(|w| w.id == id).and_then(|w| self.workspace_project(w)).map(|p| p.path.clone());
        let i = &mut self.intents[k];
        i.workspace = Some(id.to_string());
        if i.project.is_empty() {
            i.project = project.unwrap_or_default();
        }
        let intent = i.id.clone();
        if let Some(w) = self.workspaces.iter_mut().find(|w| w.id == id) {
            w.current = Some(intent);
        }
    }

    /// The registered project whose folder holds `path`, the deepest one.
    pub fn project_of(&self, path: &str) -> Option<&Project> {
        self.projects.iter().filter(|p| std::path::Path::new(path).starts_with(&p.path)).max_by_key(|p| p.path.len())
    }

    /// An intent's project for the rules: the registered project holding the folder recorded
    /// on it, never its units'. Empty: of no project (none recorded, or a folder no registered
    /// project holds), so that no agent picks an intent's project by adding a unit or by
    /// naming a folder.
    pub fn intent_project(&self, i: &Intent) -> String {
        if i.project.is_empty() {
            return String::new();
        }
        self.project_of(&i.project).map(|p| p.path.clone()).unwrap_or_default()
    }

    /// An intent's project folder to show (the page, a notice): the registered project
    /// holding the folder recorded on it, else the one holding its first unit's repository.
    /// Empty: of no project. The rules read `intent_project`.
    pub fn project_folder(&self, i: &Intent) -> String {
        let path = if i.project.is_empty() { self.units.iter().find(|u| u.intent == i.id).map(|u| u.repo.clone()).unwrap_or_default() } else { i.project.clone() };
        self.project_of(&path).map(|p| p.path.clone()).unwrap_or_default()
    }
}
