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
            FindingsBatched { .. } | FindingAdded { .. } | FindingDropped { .. } | FindingNoted { .. } | FindingDecided { .. } => return self.prepare_findings(who, body),
            AgentSeen { session, name, harness } => {
                if blank(&session) || blank(&name) {
                    return no("an agent needs a session and a name");
                }
                AgentSeen { session, name, harness }
            }
            UnitCreated { .. } | UnitRelocated { .. } | UnitMerged { .. } | UnitDone { .. } | CommitLinked { .. } | CommitUnlinked { .. } => return self.prepare_units(who, body),
            TaskCreated(..) | TaskStarted { .. } | TaskDelivered { .. } | TaskDropped { .. } | TaskAmended { .. } => return self.prepare_tasks(who, body),
            GateSubmitted { .. } => return self.prepare_gates(who, body),
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
            CommandRan { .. } | CommandEnded { .. } | CommandReviewed { .. } => return self.prepare_commands(who, body),
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
            ChoreCreated { .. } | ChoreDone { .. } => return self.prepare_chores(who, body),
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
        match &e.body {
            IntentCreated { .. } | IntentStateChanged { .. } | IntentMoved { .. } | IntentMerged { .. } | IntentUpdated { .. } | IntentLinked { .. } => self.apply_intents(e),
            ProjectAdded { .. } | ProjectRenamed { .. } | WorkspaceLinked(..) | WorkspaceUnlinked { .. } | WorkspaceProjectSet { .. } | ManagerRegistered { .. } => self.apply_projects(e),
            FindingsBatched { .. } | FindingAdded { .. } | FindingDropped { .. } | FindingNoted { .. } | FindingDecided { .. } => self.apply_findings(e),
            UnitCreated { .. } | UnitRelocated { .. } | UnitMerged { .. } | UnitDone { .. } | CommitLinked { .. } | CommitUnlinked { .. } => self.apply_units(e),
            AgentSeen { session, name, harness } => {
                let seen = Agent { session: session.clone(), name: name.clone(), harness: harness.clone() };
                match self.agents.iter_mut().find(|a| &a.session == session) {
                    Some(a) => *a = seen,
                    None => self.agents.push(seen),
                }
            }
            TaskCreated(..) | TaskStarted { .. } | TaskDelivered { .. } | TaskDropped { .. } | TaskAmended { .. } => self.apply_tasks(e),
            GateSubmitted { .. } => self.apply_gates(e),
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
            CommandRan { .. } | CommandEnded { .. } | CommandReviewed { .. } => self.apply_commands(e),
            DeliveryUpdated { chore: Some(c), delivery, .. } => {
                if let Some(x) = self.chores.iter_mut().find(|x| &x.id == c) {
                    x.delivery = Some(delivery.clone());
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
            ChoreCreated { .. } | ChoreDone { .. } => self.apply_chores(e),
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

