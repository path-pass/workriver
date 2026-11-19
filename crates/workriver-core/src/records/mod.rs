// The events workriver records, the state they add up to, and the rules a new event must
// pass. Ids and numbers are assigned here, so replaying the stored events gives the same
// state every time.
use serde::{Deserialize, Serialize};

use crate::access::{Principal, Rejected, no};
use crate::conversations::*;
use crate::planning::*;
use crate::work::*;

mod body;
pub use body::*;

pub(crate) fn yes() -> bool {
    true
}

pub(crate) fn is_true(b: &bool) -> bool {
    *b
}

pub(crate) fn user() -> String {
    "user".into()
}

/// A recorded event.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Event {
    pub seq: i64,
    pub at: String,
    pub actor: String,
    #[serde(flatten)]
    pub body: Body,
}

// ---- the state the events add up to ----

#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
pub struct State {
    pub intents: Vec<Intent>,
    pub units: Vec<Unit>,
    pub chores: Vec<Chore>,
    #[serde(default)]
    pub agents: Vec<Agent>,
    #[serde(default)]
    pub questions: Vec<Question>,
    #[serde(default)]
    pub threads: Vec<Thread>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
    /// Planned intents in roadmap order (each project's roadmap is its part of this list).
    #[serde(default)]
    pub roadmap: Vec<String>,
    #[serde(default)]
    pub backlog: Vec<BacklogFinding>,
    #[serde(default)]
    pub links: Vec<Link>,
    /// The user's messages to managers, with their replies.
    #[serde(default)]
    pub messages: Vec<Message>,
    /// Up to when the user has read each workspace's conversation with its manager, by label.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub read: std::collections::BTreeMap<String, String>,
    /// Not the records but a part of them: what an agent on a blind task may see.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reduced: bool,
    /// The newest commands the user ran from the page (at most `RUNS_KEPT`), oldest first.
    /// Never serialized: only the user sees them, and the service adds them to the user's
    /// view itself.
    #[serde(skip)]
    pub runs: Vec<CommandRun>,
}

/// An event's kind, as recorded ("gate.submitted").
pub(crate) fn name_of_kind(b: &Body) -> String {
    serde_json::to_value(b).ok().and_then(|v| v["kind"].as_str().map(str::to_string)).unwrap_or_default()
}

pub fn name<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

pub(crate) fn blank(s: &str) -> bool {
    s.trim().is_empty()
}

impl State {

    /// Checks a new event against the rules and fills in what is assigned here (ids and
    /// numbers). The result is what gets stored.
    pub fn prepare(&self, who: &Principal, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        let actor = who.actor();
        let user = *who == Principal::User;
        // An agent is never the user or workriver by calling itself so.
        if blank(actor) || (matches!(who, Principal::Agent { .. }) && matches!(actor, "user" | "workriver")) {
            return no("the actor is empty or not an agent's");
        }
        // The last line of defence: whatever let the request in, these are the user's alone.
        if body.user_only() && !user {
            return no(format!("only the user does this ({}): in the web UI, or with the CLI from a terminal that is not an agent's", name_of_kind(&body)));
        }
        Ok(match body {
            IntentCreated { .. } | IntentStateChanged { .. } | IntentMoved { .. } | IntentMerged { .. } | IntentUpdated { .. } | IntentLinked { .. } => return self.prepare_intents(who, body),
            ProjectAdded { .. } | ProjectRenamed { .. } | WorkspaceLinked(..) | WorkspaceUnlinked { .. } | WorkspaceProjectSet { .. } | ManagerRegistered { .. } => return self.prepare_projects(who, body),
            FindingsBatched { intent, findings } => {
                let i = self.intent(&intent)?;
                if i.state != IntentState::Active {
                    return no(format!("intent {intent} is {}; start it first (workriver intent start {intent} in its manager's pane)", name(&i.state)));
                }
                if findings.is_empty() {
                    return no("a batch needs at least one finding");
                }
                for r in &findings {
                    let f = self.finding(r)?;
                    // A task's finding belongs to its intent; a backlog one to its intent, if any, or the project.
                    let fits = if r.task == 0 {
                        let b = self.backlog.iter().find(|b| b.finding.found.id == r.finding);
                        b.is_some_and(|b| b.intent.as_deref().is_none_or(|x| x == intent))
                    } else {
                        self.intent_of(&r.unit) == Some(intent.as_str())
                    };
                    if !fits {
                        return no(format!("finding {} is not part of intent {intent}", r.finding));
                    }
                    if f.state != FindingState::Deferred {
                        return no(format!("finding {} is {}, not deferred", r.finding, name(&f.state)));
                    }
                }
                FindingsBatched { intent, findings }
            }
            FindingAdded { project, intent, finding, source, note } => {
                if blank(&project) {
                    return no("a backlog finding needs its project (--project)");
                }
                if blank(&finding.problem) {
                    return no("a finding needs its problem");
                }
                if let Some(i) = &intent {
                    self.intent(i)?;
                }
                let mut finding = finding;
                finding.id = format!("f{}", self.backlog.len() + 1);
                FindingAdded { project, intent, finding, source, note }
            }
            FindingDropped { unit, task, finding, reason } => {
                let f = self.finding(&FindingRef { unit: unit.clone(), task, finding: finding.clone() })?;
                if !matches!(f.state, FindingState::Deferred | FindingState::Scheduled) {
                    return no(format!("finding {finding} is {}; only deferred or scheduled ones are dropped", name(&f.state)));
                }
                if blank(&reason) {
                    return no("dropping a finding needs a reason: --reason \"...\"");
                }
                FindingDropped { unit, task, finding, reason }
            }
            AgentSeen { session, name, harness } => {
                if blank(&session) || blank(&name) {
                    return no("an agent needs a session and a name");
                }
                AgentSeen { session, name, harness }
            }
            UnitCreated { .. } | UnitRelocated { .. } | UnitMerged { .. } | UnitDone { .. } | CommitLinked { .. } | CommitUnlinked { .. } => return self.prepare_units(who, body),
            TaskCreated(t) => {
                // A blocker is put by the agent stuck on that task, its manager or the user.
                if let Some(b) = t.blocks
                    && !user
                    && let Ok(stuck) = self.task(&t.unit, b)
                    && !self.is_agent_of(stuck, actor)
                    && self.intent_of(&t.unit).and_then(|i| self.manager_of(i)) != Some(actor)
                {
                    let r = format!("{}#{b}", t.unit);
                    return no(format!("a blocker before {r} is put by the agent of {r}, the manager of its intent, or the user; to tell the manager: workriver manager tell {r} --text \"...\""));
                }
                TaskCreated(self.prepare_task(t)?)
            }
            TaskStarted { unit, task, agent } => {
                let t = self.task(&unit, task)?;
                not_dropped(&unit, t)?;
                if t.kind == TaskKind::Gate {
                    return no(format!("{unit}#{task} is a gate: only the user does it, with gate submit"));
                }
                if t.state != TaskState::Todo {
                    return no(format!("{unit}#{task} is {}, not todo", name(&t.state)));
                }
                TaskStarted { unit, task, agent }
            }
            TaskDelivered { unit, task, note, evidence, findings, attachments } => {
                let t = self.task(&unit, task)?;
                not_dropped(&unit, t)?;
                if t.kind == TaskKind::Gate {
                    return no(format!("{unit}#{task} is a gate: the user submits it with gate submit"));
                }
                if t.state == TaskState::Done {
                    return no(format!("{unit}#{task} is already delivered; to correct it, run `workriver task deliver {unit}#{task} --amend` with what replaces it"));
                }
                check_output(t, &evidence, &findings)?;
                self.stored(evidence.iter().filter_map(|e| e.attachment.as_ref()).chain(findings.iter().flat_map(|f| &f.attachments)).chain(&attachments))?;
                if t.kind == TaskKind::Design && blank(&note) {
                    return no("a design task delivers its design as the note: --note \"...\"");
                }
                if t.kind == TaskKind::Research && blank(&note) && attachments.is_empty() {
                    return no("a research task delivers what it found: a --note, and its report attached (workriver attach --dir)");
                }
                TaskDelivered { unit, task, note, evidence, findings: numbered(findings), attachments }
            }
            TaskDropped { unit, task, reason } => {
                let t = self.task(&unit, task)?;
                not_dropped(&unit, t)?;
                if t.state == TaskState::Done {
                    return no(format!("{unit}#{task} is done; only work not done yet is dropped"));
                }
                if blank(&reason) {
                    return no("dropping a task needs a reason: --reason \"...\" (e.g. superseded by u1#17)");
                }
                // The manager re-plans; a worker takes back only what it asked for itself.
                let manager = self.intent_of(&unit).and_then(|i| self.manager_of(i)) == Some(actor);
                if !user && !manager && t.by != actor {
                    return no(format!("{unit}#{task} is dropped by the manager of its intent or the user (or the agent that recorded it); tell your manager it is no longer needed: workriver manager tell {unit}#{task} --text \"...\""));
                }
                TaskDropped { unit, task, reason }
            }
            TaskAmended { unit, task, note, evidence, findings, attachments, add } => {
                let t = self.task(&unit, task)?;
                if t.kind == TaskKind::Gate || t.state != TaskState::Done {
                    return no(format!("{unit}#{task} is not a delivered task; only a delivered task can be amended"));
                }
                if note.is_none() && evidence.is_none() && findings.is_none() && attachments.is_empty() && add.is_empty() {
                    return no("nothing to amend: give a note, evidence or findings");
                }
                if findings.is_some() && !add.is_empty() {
                    return no("either replace the findings (--findings) or add to them (--add-findings), not both");
                }
                if findings.is_some() && t.findings.iter().any(|f| f.state != FindingState::Open) {
                    return no(format!(
                        "some findings of {unit}#{task} are already decided, so they cannot be replaced; add new ones with `workriver task deliver {unit}#{task} --amend --add-findings FILE`, or add to a finding with `workriver finding note {unit}#{task} <id> --text \"...\"`"
                    ));
                }
                // New findings are the reviewer's to report (or the user's), as the first were.
                if !add.is_empty() && !user && !self.is_agent_of(t, actor) {
                    return no(format!("findings of {unit}#{task} are added by the agent that delivered it"));
                }
                check_output(t, evidence.as_deref().unwrap_or_default(), findings.as_deref().unwrap_or(&add))?;
                self.stored(
                    evidence.iter().flatten().filter_map(|e| e.attachment.as_ref()).chain(findings.iter().flatten().chain(&add).flat_map(|f| &f.attachments)).chain(&attachments),
                )?;
                let add = numbered_after(add, &t.findings);
                TaskAmended { unit, task, note, evidence, findings: findings.map(numbered), attachments, add }
            }
            FindingNoted { unit, task, finding, text, attachments } => {
                let t = self.task(&unit, task)?;
                if !t.findings.iter().any(|f| f.found.id == finding) {
                    return no(format!("{unit}#{task} has no finding {finding}"));
                }
                // By whoever reported it, the intent's manager, or the user.
                let manager = self.intent_of(&unit).and_then(|i| self.manager_of(i)) == Some(actor);
                if !user && !manager && !self.is_agent_of(t, actor) {
                    return no(format!("a finding of {unit}#{task} is added to by the agent that delivered it, the manager of its intent, or the user"));
                }
                if blank(&text) && attachments.is_empty() {
                    return no("a note needs its text: --text \"...\" (or a file: --attach FILE)");
                }
                self.stored(attachments.iter())?;
                FindingNoted { unit, task, finding, text, attachments }
            }
            FindingDecided { unit, task, finding, decision, note, .. } => {
                let t = self.task(&unit, task)?;
                let Some(f) = t.findings.iter().find(|f| f.found.id == finding) else { return no(format!("{unit}#{task} has no finding {finding}")) };
                let to = match decision {
                    Decision::Accept => FindingState::Accepted,
                    Decision::Revise => FindingState::Revised,
                    Decision::Reject => FindingState::Rejected,
                    Decision::Defer => FindingState::Deferred,
                };
                // A decision is made once; only the user changes one afterwards (the manager's,
                // or their own), and only while nothing was built on its being put off.
                let was = match f.state {
                    FindingState::Open => None,
                    FindingState::Accepted | FindingState::Revised | FindingState::Rejected | FindingState::Deferred if user && f.state != to => Some(f.state),
                    _ if user && f.state == to && decision != Decision::Revise => return no(format!("finding {finding} of {unit}#{task} is already {}", name(&f.state))),
                    _ if user && f.state == to => Some(f.state),
                    _ if user => return no(format!("finding {finding} of {unit}#{task} is {}; that is no longer changed here", name(&f.state))),
                    _ => return no(format!("finding {finding} of {unit}#{task} is already {}; only the user changes a decision", name(&f.state))),
                };
                if decision == Decision::Revise && blank(&note) {
                    return no("revise needs a note saying how it is to be fixed: --note \"...\"");
                }
                // An agent decides only what is the intent's manager's to decide, and only as
                // that manager; whose decision it is follows from who records it.
                if !user {
                    let intent = self.intent_of(&unit).unwrap_or_default();
                    let policy = self.intents.iter().find(|i| i.id == intent).map(|i| i.findings).unwrap_or_default();
                    if !policy.managers() {
                        return no(format!(
                            "finding {finding} of {unit}#{task} is the user's to decide (intent {intent}: findings {}); the user decides it in the web UI",
                            name(&policy)
                        ));
                    }
                    if self.manager_of(intent) != Some(actor) {
                        return no(format!("finding {finding} of {unit}#{task} is for the manager of intent {intent} to decide (or the user), not for other agents"));
                    }
                }
                FindingDecided { unit, task, finding, decision, note, by: if user { Who::User } else { Who::Agent }, was }
            }
            GateSubmitted { unit, task, result, note, findings, attachments } => {
                self.stored(attachments.iter().chain(findings.iter().flat_map(|f| &f.attachments)))?;
                let t = self.task(&unit, task)?;
                not_dropped(&unit, t)?;
                if t.kind == TaskKind::Gate && t.state == TaskState::Todo {
                    return no(format!("gate {unit}#{task} is not open yet: it follows {}, not done yet", self.unfinished_inputs(&unit, t).join(", ")));
                }
                if t.kind != TaskKind::Gate || t.state != TaskState::Waiting {
                    return no(format!("{unit}#{task} is not a waiting gate"));
                }
                match result {
                    GateResult::Problem if findings.is_empty() => return no("a gate with a problem needs at least one finding"),
                    GateResult::Done if !findings.is_empty() => return no("a gate that is done carries no findings; submit it as problem"),
                    _ => {}
                }
                // The user at a gate may not know where the problem sits; a problem is enough.
                if findings.iter().any(|f| blank(&f.problem)) {
                    return no("a finding needs a problem");
                }
                GateSubmitted { unit, task, result, note, findings: numbered(findings), attachments }
            }
            AttachmentAdded { id, name, mime, size, unit, task, finding } => {
                if blank(&id) || blank(&name) {
                    return no("an attachment needs its id and name");
                }
                match (&unit, task, &finding) {
                    (None, None, None) => {}
                    (Some(u), Some(n), f) => {
                        let t = self.task(u, n)?;
                        if let Some(f) = f
                            && !t.findings.iter().any(|x| &x.found.id == f)
                        {
                            return no(format!("{u}#{n} has no finding {f}"));
                        }
                    }
                    _ => return no("an attachment is linked to a task (unit and number), or to nothing"),
                }
                AttachmentAdded { id, name, mime, size, unit, task, finding }
            }
            CommandRan { id, command, dir, machine, unit, by, from, step } => {
                if blank(&id) || blank(&command) || blank(&dir) {
                    return no("a command run needs its id, its text and its directory");
                }
                if let Some(u) = &unit {
                    self.unit(u)?;
                }
                CommandRan { id, command, dir, machine, unit, by, from, step }
            }
            CommandEnded { id, exit } => {
                if *who != Principal::Service {
                    return no("only workriver records how a command ended");
                }
                CommandEnded { id, exit }
            }
            CommandReviewed { id, verdict, text } => {
                if *who != Principal::Service {
                    return no("only workriver records a command's review, from the agent it started for it");
                }
                if blank(&text) {
                    return no("a review says what the command does");
                }
                CommandReviewed { id, verdict, text }
            }
            DeliveryUpdated { unit, task, finding, chore, delivery } => {
                if let Some(c) = &chore {
                    if !self.chores.iter().any(|x| &x.id == c) {
                        return no(format!("no chore {c}"));
                    }
                } else if task == 0 {
                    // A backlog finding's (a batch's message is recorded on its first finding).
                    self.finding(&FindingRef { unit: String::new(), task: 0, finding: finding.clone().unwrap_or_default() })?;
                } else {
                    let t = self.task(&unit, task)?;
                    if let Some(id) = &finding
                        && !t.findings.iter().any(|f| &f.found.id == id)
                    {
                        return no(format!("{unit}#{task} has no finding {id}"));
                    }
                }
                DeliveryUpdated { unit, task, finding, chore, delivery }
            }
            ChoreCreated { intent, project, unit, title, steps, .. } => {
                // Of an intent, or (no intent) of a project, or of nothing at all.
                if !intent.is_empty() {
                    self.intent(&intent)?;
                }
                if let Some(u) = &unit
                    && (intent.is_empty() || self.unit(u)?.intent != intent)
                {
                    return no(format!("unit {u} is not part of intent {intent:?}"));
                }
                if blank(&title) {
                    return no("a chore needs a title");
                }
                let project = if intent.is_empty() { project } else { String::new() };
                ChoreCreated { id: format!("c{}", self.chores.len() + 1), intent, project, unit, title, steps }
            }
            QuestionAsked { thread, unit, task, text, context, conclude, .. } => {
                // Task 0 is the unit itself: the user's to ask about; an agent asks about a task.
                let about = match task {
                    0 if !user => return no("a question is about a task: workriver ask <unit>#<n> \"...\""),
                    0 => {
                        self.unit(&unit)?;
                        None
                    }
                    n => Some(self.task(&unit, n)?),
                };
                // Blind work neither asks nor is asked by other agents: both would tell it things.
                if !self.blind_tasks(actor).is_empty() {
                    return no("you are on a blind task: other work is hidden until you deliver, and so are questions. If you cannot go on without one, tell your manager instead: workriver manager tell <your task> --text \"...\"");
                }
                if about.is_some_and(|t| t.blind && !t.state.over()) && !user {
                    return no(format!("{unit}#{task} is a blind task in progress: its agent is not to be told about other work. Ask once it is delivered"));
                }
                if blank(&text) {
                    return no("a question needs its text");
                }
                // An attachment it points at is one shown with that task (with the unit: with one
                // of its tasks): the temporary agent is given its path, so nothing else gets out.
                // The user's question also points at a file the user stored (attached in the page).
                let shown: Vec<&str> = match about {
                    Some(_) => self.attachments_of(&unit, task),
                    None => self.unit(&unit)?.tasks.iter().flat_map(|t| self.attachments_of(&unit, t.n)).collect(),
                };
                for c in context.iter().filter(|c| c.kind == "attachment") {
                    let a = c.id.as_deref().unwrap_or_default();
                    if !shown.contains(&a) && !(user && self.attachments.iter().any(|x| x.id == a && x.user)) {
                        return no(format!("attachment {a:?} is not one of {unit}#{task}'s; a question points only at what is shown with its task, or at a file the user attached to it"));
                    }
                }
                let id = format!("q{}", self.questions.len() + 1);
                let thread = if thread.is_empty() {
                    if conclude {
                        return no("a conclusion is written for a thread; name it");
                    }
                    id.clone()
                } else {
                    match self.threads.iter().find(|t| t.id == thread) {
                        None => return no(format!("no thread {thread}")),
                        Some(t) if t.unit != unit || t.task != task => return no(format!("thread {thread} is about another task")),
                        Some(t) if t.ended => return no(format!("thread {thread} has ended; start a new one")),
                        Some(_) => thread,
                    }
                };
                QuestionAsked { id, thread, unit, task, text, context, conclude }
            }
            QuestionStarted { question, agent, name, temporary } => {
                let q = self.question(&question)?;
                if !q.open() || q.agent.is_some() {
                    return no(format!("question {question} is not waiting for an agent"));
                }
                QuestionStarted { question, agent, name, temporary }
            }
            QuestionAnswered { question, text, asked, conclusion } => {
                let q = self.question(&question)?;
                if !q.open() {
                    return no(format!("question {question} is already {}", if q.answer.is_some() { "answered" } else { "closed" }));
                }
                // Only the thread's temporary agent (known by session or by name), or the user.
                let t = self.threads.iter().find(|t| t.id == q.thread);
                let name = t.and_then(|t| t.name.as_deref()).or(q.name.as_deref());
                // The task's own agent may always answer about its work.
                let own = self.task(&q.unit, q.task).ok().and_then(|x| x.agent.as_deref()) == Some(actor);
                let mine = own
                    || [q.agent.as_deref(), t.and_then(|t| t.agent.as_deref())].contains(&Some(actor))
                    || [name, q.name.as_deref()].iter().any(|n| n.is_some() && self.name_of(actor) == *n);
                if !user && !mine {
                    return no(format!("question {question} is answered by the agent started for it ({}), or by the user", name.unwrap_or("not started yet")));
                }
                if blank(&text) {
                    return no("an answer needs its text");
                }
                if conclusion != q.conclude {
                    return no(if q.conclude { format!("{question} asks for the conclusion: answer with --conclusion") } else { format!("{question} is a question, not a request for a conclusion: answer without --conclusion") });
                }
                QuestionAnswered { question, text, asked, conclusion }
            }
            ThreadEnded { thread } => match self.threads.iter().find(|t| t.id == thread) {
                None => return no(format!("no thread {thread}")),
                Some(t) if t.ended => return no(format!("thread {thread} has already ended")),
                Some(_) => ThreadEnded { thread },
            },
            QuestionFailed { question, reason_code, reason } => {
                if !self.question(&question)?.open() {
                    return no(format!("question {question} is already closed"));
                }
                QuestionFailed { question, reason_code, reason }
            }
            ChoreDone { chore } => match self.chores.iter().find(|c| c.id == chore) {
                None => return no(format!("no chore {chore}")),
                Some(c) if c.done => return no(format!("chore {chore} is already done")),
                Some(_) => ChoreDone { chore },
            },
            MessageSent { intent, workspace, text, .. } => {
                if blank(&text) {
                    return no("a message needs its text");
                }
                // To the manager of the intent's workspace, or of the workspace named (here).
                let (workspace, manager) = if intent.is_empty() {
                    let w = self.workspaces.iter().find(|w| w.group.machine.is_none() && w.group.label == workspace);
                    (workspace, w.and_then(|w| w.manager.clone()))
                } else {
                    let i = self.intent(&intent)?;
                    let label = i.workspace.as_ref().map(|g| self.workspace_of(g).map_or(&g.label, |w| &w.group.label).clone()).unwrap_or_default();
                    (label, self.manager_of(&intent).map(str::to_string))
                };
                let Some(manager) = manager else {
                    return no(format!("{} has no manager; a manager registers with `workriver manager here`", if intent.is_empty() { format!("workspace {workspace:?}") } else { format!("intent {intent}") }));
                };
                MessageSent { id: format!("m{}", self.messages.len() + 1), intent, workspace, manager, text }
            }
            MessageDelivery { id, delivery } => {
                if *who != Principal::Service {
                    return no("only workriver records where a message is");
                }
                self.message(&id)?;
                MessageDelivery { id, delivery }
            }
            ManagerTold { unit, task, text } => {
                if user {
                    return no("the user writes to a manager in workriver's page (Ask, Manager)");
                }
                self.task(&unit, task)?;
                if self.intent_of(&unit).and_then(|i| self.manager_of(i)) == Some(actor) {
                    return no("you are the manager of this intent");
                }
                if blank(&text) {
                    return no("say what: --text \"...\"");
                }
                ManagerTold { unit, task, text }
            }
            MessageReplied { message, text } => {
                let m = self.message(&message)?;
                // Only the manager it was typed to: nobody else answers for it, the user included.
                if m.manager != actor {
                    return no(format!("message {message} went to {}; only that manager replies to it", self.name_of(&m.manager).unwrap_or(&m.manager)));
                }
                if blank(&text) {
                    return no("a reply needs its text");
                }
                MessageReplied { message, text }
            }
            MessageSaid { text, .. } => {
                if user {
                    return no("the user writes to a manager in workriver's page (Ask, Manager)");
                }
                let Some(w) = self.workspaces.iter().find(|w| w.manager.as_deref() == Some(actor)) else {
                    return no("only a workspace's manager says something to the user; a worker tells its manager: `workriver manager tell <task> --text \"...\"`");
                };
                if blank(&text) {
                    return no("say what: --text \"...\"");
                }
                MessageSaid { id: format!("m{}", self.messages.len() + 1), intent: w.current.clone().unwrap_or_default(), workspace: w.group.label.clone(), text }
            }
            MessagesRead { workspace, at } => {
                if blank(&workspace) {
                    return no("which workspace's conversation was read");
                }
                // Stored as the service writes times, so that they compare as text.
                let Ok(t) = humantime::parse_rfc3339_weak(&at) else { return no(format!("{at:?} is not a time")) };
                MessagesRead { workspace, at: humantime::format_rfc3339_seconds(t).to_string() }
            }
        })
    }

    /// Adds a recorded event. It passed `prepare` when it was recorded, so it is trusted.
    pub fn apply(&mut self, e: &Event) {
        use Body::*;
        let at = &e.at;
        let agent_of = |actor: &str| (actor != "user").then(|| actor.to_string());
        match &e.body {
            IntentCreated { .. } | IntentStateChanged { .. } | IntentMoved { .. } | IntentMerged { .. } | IntentUpdated { .. } | IntentLinked { .. } => self.apply_intents(e),
            ProjectAdded { .. } | ProjectRenamed { .. } | WorkspaceLinked(..) | WorkspaceUnlinked { .. } | WorkspaceProjectSet { .. } | ManagerRegistered { .. } => self.apply_projects(e),
            FindingsBatched { intent, findings } => {
                for r in findings {
                    if let Some(f) = self.finding_mut(r) {
                        f.state = FindingState::Scheduled;
                    }
                    if r.task == 0
                        && let Some(b) = self.backlog.iter_mut().find(|b| b.finding.found.id == r.finding)
                    {
                        b.intent = Some(intent.clone());
                    }
                }
            }
            FindingAdded { project, intent, finding, source, note } => {
                let found = finding.clone();
                self.backlog.push(BacklogFinding {
                    finding: Finding { found, state: FindingState::Deferred, decided_by: Some(Who::User), note: (!note.is_empty()).then(|| note.clone()), delivery: None, reason: None, notes: vec![], decisions: vec![] },
                    project: project.clone(),
                    intent: intent.clone(),
                    source: source.clone(),
                    by: e.actor.clone(),
                    at: at.clone(),
                });
            }
            FindingDropped { unit, task, finding, reason } => {
                if let Some(f) = self.finding_mut(&FindingRef { unit: unit.clone(), task: *task, finding: finding.clone() }) {
                    f.state = FindingState::Dropped;
                    f.reason = Some(reason.clone());
                }
            }
            UnitCreated { .. } | UnitRelocated { .. } | UnitMerged { .. } | UnitDone { .. } | CommitLinked { .. } | CommitUnlinked { .. } => self.apply_units(e),
            AgentSeen { session, name, harness } => {
                let seen = Agent { session: session.clone(), name: name.clone(), harness: harness.clone() };
                match self.agents.iter_mut().find(|a| &a.session == session) {
                    Some(a) => *a = seen,
                    None => self.agents.push(seen),
                }
            }
            TaskCreated(t) => {
                // Before 0.2 a task named no intent: its unit's.
                let intent = if t.intent.is_empty() { self.intent_of(&t.unit).unwrap_or_default().to_string() } else { t.intent.clone() };
                let tasks = match self.units.iter_mut().find(|u| u.id == t.unit) {
                    Some(u) => Some(&mut u.tasks),
                    None => self.intents.iter_mut().find(|i| i.id == t.unit).map(|i| &mut i.tasks),
                };
                if let Some(tasks) = tasks {
                    tasks.push(Task {
                        n: t.n,
                        intent,
                        kind: t.kind,
                        title: t.title.clone(),
                        text: t.text.clone(),
                        inputs: t.inputs.clone(),
                        causes: t.causes.clone(),
                        agent: t.agent.clone(),
                        reviewer: t.reviewer,
                        gate: t.gate,
                        steps: t.steps.clone(),
                        plans: t.plans.clone(),
                        blind: t.blind,
                        blocks: t.blocks,
                        // A gate opens (waits for the user) once what it follows is done.
                        state: TaskState::Todo,
                        by: e.actor.clone(),
                        at: at.clone(),
                        started: None,
                        done: None,
                        note: String::new(),
                        evidence: vec![],
                        findings: vec![],
                        result: None,
                        delivery: None,
                        amended: None,
                        waiting_since: None,
                        attachments: vec![],
                        dropped: None,
                    })
                }
                // More work for a unit closed as done: it is open again, its state from its tasks.
                if let Some(u) = self.units.iter_mut().find(|u| u.id == t.unit)
                    && let Some(was) = u.done.take()
                {
                    u.reopened = Some(was);
                }
                // The task it blocks follows it from now on.
                if let Some(b) = t.blocks
                    && let Some(stuck) = self.task_mut(&t.unit, b)
                {
                    stuck.inputs.push(Input { from: t.n.to_string(), primary: false });
                }
            }
            TaskStarted { unit, task, agent } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.state = TaskState::Running;
                    t.started = Some(at.clone());
                    t.agent = agent.clone().or_else(|| agent_of(&e.actor)).or(t.agent.take());
                }
            }
            TaskDelivered { unit, task, note, evidence, findings, attachments } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.attachments.extend(attachments.iter().cloned());
                    t.state = TaskState::Done;
                    t.done = Some(at.clone());
                    t.note = note.clone();
                    t.evidence = evidence.clone();
                    t.findings = findings.iter().cloned().map(open).collect();
                    if t.agent.is_none() {
                        t.agent = agent_of(&e.actor);
                    }
                }
                // Scheduled findings this work answers are done with.
                let causes: Vec<FindingRef> = self
                    .task(unit, *task)
                    .map(|t| {
                        t.causes
                            .iter()
                            .flat_map(|c| {
                                let at = if c.task == 0 { String::new() } else { c.unit.clone().unwrap_or_else(|| unit.clone()) };
                                c.findings.iter().map(move |f| FindingRef { unit: at.clone(), task: c.task, finding: f.clone() })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                for r in causes {
                    if let Some(f) = self.finding_mut(&r).filter(|f| f.state == FindingState::Scheduled) {
                        f.state = FindingState::Done;
                    }
                }
            }
            FindingDecided { unit, task, finding, decision, note, by, .. } => {
                if let Some(f) = self.task_mut(unit, *task).and_then(|t| t.findings.iter_mut().find(|f| &f.found.id == finding)) {
                    f.state = match decision {
                        Decision::Accept => FindingState::Accepted,
                        Decision::Revise => FindingState::Revised,
                        Decision::Reject => FindingState::Rejected,
                        Decision::Defer => FindingState::Deferred,
                    };
                    f.decided_by = Some(*by);
                    f.note = (!note.is_empty()).then(|| note.clone());
                    f.decisions.push(Decided { state: f.state, by: *by, agent: agent_of(&e.actor), note: note.clone(), at: at.clone() });
                }
            }
            TaskDropped { unit, task, reason } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.state = TaskState::Dropped;
                    t.dropped = Some(Closed { at: at.clone(), by: e.actor.clone(), reason: reason.clone() });
                }
            }
            GateSubmitted { unit, task, result, note, findings, attachments } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.state = TaskState::Done;
                    t.done = Some(at.clone());
                    t.result = Some(*result);
                    t.note = note.clone();
                    t.findings = findings.iter().cloned().map(open).collect();
                    t.attachments.extend(attachments.iter().cloned());
                }
            }
            AttachmentAdded { id, name, mime, size, unit, task, finding } => {
                let user = e.actor == "user";
                match self.attachments.iter_mut().find(|a| &a.id == id) {
                    Some(a) => a.user |= user,
                    None => self.attachments.push(Attachment { id: id.clone(), name: name.clone(), mime: mime.clone(), size: *size, by: e.actor.clone(), at: at.clone(), user }),
                }
                if let (Some(u), Some(n)) = (unit, task)
                    && let Some(t) = self.task_mut(u, *n)
                {
                    let list = match finding {
                        Some(f) => t.findings.iter_mut().find(|x| &x.found.id == f).map(|x| &mut x.found.attachments),
                        None => Some(&mut t.attachments),
                    };
                    if let Some(list) = list.filter(|l| !l.contains(id)) {
                        list.push(id.clone());
                    }
                }
            }
            // The latest few only (the user's recent runs); never their output.
            CommandRan { id, command, dir, machine, unit, by, from, step } => {
                self.runs.push(CommandRun { id: id.clone(), command: command.clone(), dir: dir.clone(), machine: machine.clone(), unit: unit.clone(), by: by.clone(), from: from.clone(), step: *step, at: at.clone(), exit: None, review: None });
                let over = self.runs.len().saturating_sub(RUNS_KEPT);
                self.runs.drain(..over);
            }
            CommandEnded { id, exit } => {
                if let Some(r) = self.runs.iter_mut().find(|r| &r.id == id) {
                    r.exit = Some(*exit);
                }
            }
            CommandReviewed { id, verdict, text } => {
                if let Some(r) = self.runs.iter_mut().find(|r| &r.id == id) {
                    r.review = Some((*verdict, text.clone()));
                }
            }
            DeliveryUpdated { chore: Some(c), delivery, .. } => {
                if let Some(x) = self.chores.iter_mut().find(|x| &x.id == c) {
                    x.delivery = Some(delivery.clone());
                }
            }
            FindingNoted { unit, task, finding, text, attachments } => {
                if let Some(f) = self.task_mut(unit, *task).and_then(|t| t.findings.iter_mut().find(|f| &f.found.id == finding)) {
                    f.notes.push(FindingNote { by: e.actor.clone(), at: at.clone(), text: text.clone(), attachments: attachments.clone() });
                }
            }
            TaskAmended { unit, task, note, evidence, findings, attachments, add } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    t.findings.extend(add.iter().cloned().map(open));
                    t.attachments.extend(attachments.iter().cloned());
                    if let Some(n) = note {
                        t.note = n.clone();
                    }
                    if let Some(e) = evidence {
                        t.evidence = e.clone();
                    }
                    if let Some(f) = findings {
                        t.findings = f.iter().cloned().map(open).collect();
                    }
                    t.amended = Some(at.clone());
                }
            }
            DeliveryUpdated { task: 0, finding: Some(f), delivery, .. } => {
                if let Some(x) = self.finding_mut(&FindingRef { unit: String::new(), task: 0, finding: f.clone() }) {
                    x.delivery = Some(delivery.clone());
                }
            }
            DeliveryUpdated { unit, task, finding, delivery, .. } => {
                if let Some(t) = self.task_mut(unit, *task) {
                    match finding {
                        None => t.delivery = Some(delivery.clone()),
                        Some(id) => {
                            if let Some(f) = t.findings.iter_mut().find(|f| &f.found.id == id) {
                                f.delivery = Some(delivery.clone());
                            }
                        }
                    }
                }
            }
            ChoreCreated { id, intent, project, unit, title, steps } => self.chores.push(Chore {
                id: id.clone(),
                intent: intent.clone(),
                project: project.clone(),
                unit: unit.clone(),
                title: title.clone(),
                by: e.actor.clone(),
                steps: steps.clone(),
                done: false,
                at: at.clone(),
                delivery: None,
            }),
            QuestionAsked { id, thread, unit, task, text, context, conclude } => {
                if !self.threads.iter().any(|t| &t.id == thread) {
                    self.threads.push(Thread { id: thread.clone(), unit: unit.clone(), task: *task, agent: None, name: None, ended: false, conclusion: None });
                }
                self.questions.push(Question {
                id: id.clone(),
                thread: thread.clone(),
                conclude: *conclude,
                unit: unit.clone(),
                task: *task,
                text: text.clone(),
                context: context.clone(),
                at: at.clone(),
                agent: None,
                name: None,
                answer: None,
                answered_at: None,
                asked: None,
                answered_by: None,
                failed: None,
                by: e.actor.clone(),
                temporary: true,
                })
            }
            QuestionStarted { question, agent, name, temporary } => {
                if let Some(q) = self.questions.iter_mut().find(|q| &q.id == question) {
                    q.agent = Some(agent.clone());
                    q.name = Some(name.clone());
                    q.temporary = *temporary;
                    // Only a temporary agent belongs to the thread (and is closed with it).
                    if *temporary && let Some(t) = self.threads.iter_mut().find(|t| t.id == q.thread) {
                        t.agent = Some(agent.clone());
                        t.name = Some(name.clone());
                    }
                }
            }
            QuestionAnswered { question, text, asked, conclusion } => {
                if let Some(q) = self.questions.iter_mut().find(|q| &q.id == question) {
                    q.answer = Some(text.clone());
                    q.answered_at = Some(at.clone());
                    q.asked = asked.clone();
                    q.answered_by = Some(e.actor.clone());
                    if *conclusion && let Some(t) = self.threads.iter_mut().find(|t| t.id == q.thread) {
                        t.conclusion = Some(text.clone());
                    }
                }
            }
            ThreadEnded { thread } => {
                if let Some(t) = self.threads.iter_mut().find(|t| &t.id == thread) {
                    t.ended = true;
                }
            }
            QuestionFailed { question, reason_code, reason } => {
                if let Some(q) = self.questions.iter_mut().find(|q| &q.id == question) {
                    q.failed = Some((*reason_code, reason.clone()));
                }
            }
            ChoreDone { chore } => {
                if let Some(c) = self.chores.iter_mut().find(|c| &c.id == chore) {
                    c.done = true;
                }
            }
            MessageSent { id, intent, workspace, manager, text } => self.messages.push(Message {
                id: id.clone(),
                intent: intent.clone(),
                workspace: workspace.clone(),
                manager: manager.clone(),
                text: text.clone(),
                at: at.clone(),
                delivery: None,
                replies: vec![],
                by_manager: false,
            }),
            MessageSaid { id, intent, workspace, text } => self.messages.push(Message {
                id: id.clone(),
                intent: intent.clone(),
                workspace: workspace.clone(),
                manager: e.actor.clone(),
                text: text.clone(),
                at: at.clone(),
                delivery: None,
                replies: vec![],
                by_manager: true,
            }),
            MessagesRead { workspace, at: upto } => {
                let r = self.read.entry(workspace.clone()).or_default();
                if *upto > *r {
                    *r = upto.clone();
                }
            }
            MessageDelivery { id, delivery } => {
                if let Some(m) = self.messages.iter_mut().find(|m| &m.id == id) {
                    m.delivery = Some(delivery.clone());
                }
            }
            MessageReplied { message, text } => {
                if let Some(m) = self.messages.iter_mut().find(|m| &m.id == message) {
                    m.replies.push(Reply { by: e.actor.clone(), text: text.clone(), at: at.clone() });
                }
            }
            // Only told: the record is the event itself.
            ManagerTold { .. } => {}
        }
        self.open_gates(at);
    }
}

