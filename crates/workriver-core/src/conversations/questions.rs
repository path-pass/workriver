// Questions the user asks about a task or a unit, and the threads they form.
use serde::{Deserialize, Serialize};

use crate::*;

/// What a question or a message to a manager is about besides its task: a finding, a file,
/// lines of a file, one of the task's attachments (or a file the user stored), another task.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct AskContext {
    /// "task", "finding", "file", "lines" or "attachment" ("unit": a question's own, old).
    pub kind: String,
    pub label: String,
    /// attachment: its id in the store (one shown with the task, see `State::attachments_of`,
    /// or one the user stored for the question).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// task: its number in the unit (the label is only the page's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<u32>,
}

/// A question about a task (task 0: its unit) and how it fared.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Question {
    pub id: String,
    pub thread: String,
    pub conclude: bool,
    pub unit: String,
    pub task: u32,
    pub text: String,
    pub context: Vec<AskContext>,
    pub at: String,
    /// Who asked: "user", or an agent's session.
    #[serde(default = "user")]
    pub by: String,
    /// The agent answering it: its session and name, once it has the question.
    pub agent: Option<String>,
    pub name: Option<String>,
    /// That agent was started for it (and is closed later), not the task's own.
    #[serde(default = "yes")]
    pub temporary: bool,
    pub answer: Option<String>,
    #[serde(default)]
    pub answered_at: Option<String>,
    pub asked: Option<String>,
    pub answered_by: Option<String>,
    pub failed: Option<(ReasonCode, String)>,
    /// Part of the user's discussion of a delivered design or research with its own agent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub discuss: bool,
}

impl Question {
    pub fn open(&self) -> bool {
        self.answer.is_none() && self.failed.is_none()
    }
}

/// A conversation about a task (or, task 0, its unit): its questions share one temporary
/// agent while it lives.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Thread {
    pub id: String,
    pub unit: String,
    pub task: u32,
    /// The temporary agent last started for it: session and name.
    pub agent: Option<String>,
    pub name: Option<String>,
    pub ended: bool,
    pub conclusion: Option<String>,
    /// The user's discussion of its task (one per task): see `Question::discuss`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub discuss: bool,
    /// A discussion: its task was amended after it began, in event order. Worked out as the
    /// events are applied; never stored.
    #[serde(skip)]
    pub amended: bool,
}

impl State {
    pub fn question(&self, id: &str) -> Result<&Question, Rejected> {
        match self.questions.iter().find(|q| q.id == id) {
            Some(q) => Ok(q),
            None => no(format!("no question {id}")),
        }
    }

    /// The task's discussion thread, if it has been discussed.
    pub fn discussion(&self, unit: &str, task: u32) -> Option<&Thread> {
        self.threads.iter().find(|t| t.discuss && t.unit == unit && t.task == task)
    }

    /// Refused once the gate that asks about a design or research (`result_gate`) is
    /// submitted: what the user confirmed there is what later work follows.
    fn gate_open(&self, unit: &str, task: u32) -> Result<(), Rejected> {
        match self.result_gate(unit, task).filter(|g| g.state == TaskState::Done) {
            Some(g) => no(format!("its gate {unit}#{} was submitted: the discussion of {unit}#{task} is over; a change now is a new design or research task (write to the manager)", g.n)),
            None => Ok(()),
        }
    }

    /// The user's question in the discussion of a delivered design or research: it joins the
    /// task's one discussion thread (or opens it), and its agent alone answers.
    fn prepare_discussion(&self, user: bool, asked: Body) -> Result<Body, Rejected> {
        let Body::QuestionAsked { thread, unit, task, text, context, conclude, .. } = asked else { unreachable!("a discussion is asked") };
        if !user {
            return no("only the user discusses a delivered design or research, in workriver's page; to ask about a task: workriver ask <unit>#<n> \"...\"");
        }
        let t = self.task(&unit, task)?;
        if !matches!(t.kind, TaskKind::Design | TaskKind::Research) {
            return no(format!("{unit}#{task} is not a design or research task: only a delivered research or design result is discussed with its agent; write to the manager about other work"));
        }
        if t.state != TaskState::Done {
            return no(format!("{unit}#{task} is not delivered yet: a result is discussed once it is delivered"));
        }
        if t.agent.is_none() {
            return no(format!("{unit}#{task} has no agent to discuss it with; write to the manager"));
        }
        self.gate_open(&unit, task)?;
        if blank(&text) && !conclude {
            return no("a question needs its text");
        }
        for c in context.iter().filter(|c| c.kind == "attachment") {
            let a = c.id.as_deref().unwrap_or_default();
            if !self.pointable(&unit, task, a, true) {
                return no(format!("attachment {a:?} is not one of {unit}#{task}'s; a question points only at what is shown with its task, or at a file the user attached to it"));
            }
        }
        let id = format!("q{}", self.questions.len() + 1);
        let thread = match (thread.is_empty(), self.discussion(&unit, task).map(|d| d.id.clone())) {
            (true, Some(d)) => d,
            (true, None) if conclude => return no(format!("nothing to conclude: {unit}#{task} has not been discussed yet")),
            (true, None) => id.clone(),
            (false, Some(d)) if d == thread => d,
            (false, _) => return no(format!("thread {thread} is not the discussion of {unit}#{task}; leave the thread out to write in it")),
        };
        Ok(Body::QuestionAsked { id, thread, unit, task, text, context, conclude, discuss: true })
    }

    /// The rules of a question's events: `prepare`, for these kinds.
    pub(crate) fn prepare_questions(&self, who: &Principal, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        let actor = who.actor();
        let user = *who == Principal::User;
        Ok(match body {
            asked @ QuestionAsked { discuss: true, .. } => return self.prepare_discussion(user, asked),
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
                for c in context.iter().filter(|c| c.kind == "attachment") {
                    let a = c.id.as_deref().unwrap_or_default();
                    if !self.pointable(&unit, task, a, user) {
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
                        Some(t) if t.discuss => return no(format!("thread {thread} is a discussion of {}#{}: only the user writes in it, from workriver's page", t.unit, t.task)),
                        Some(t) if t.unit != unit || t.task != task => return no(format!("thread {thread} is about another task")),
                        Some(t) if t.ended => return no(format!("thread {thread} has ended; start a new one")),
                        Some(_) => thread,
                    }
                };
                QuestionAsked { id, thread, unit, task, text, context, conclude, discuss: false }
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
                // A discussion is answered by the task's own agent alone, while its gate waits.
                if q.discuss {
                    let t = self.task(&q.unit, q.task)?;
                    if !self.is_agent_of(t, actor) {
                        return no(format!("question {question} is part of the discussion of {}#{}: it is answered by the agent of {}#{} alone", q.unit, q.task, q.unit, q.task));
                    }
                    self.gate_open(&q.unit, q.task)?;
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
                Some(t) if t.discuss => return no(format!("thread {thread} is a discussion: it is not ended; it is over once its task's gate is submitted")),
                Some(t) if t.ended => return no(format!("thread {thread} has already ended")),
                Some(_) => ThreadEnded { thread },
            },
            QuestionsRead { unit, task, at } => {
                self.task(&unit, task)?;
                // Stored as the service writes times, so that they compare as text.
                let Ok(t) = humantime::parse_rfc3339_weak(&at) else { return no(format!("{at:?} is not a time")) };
                QuestionsRead { unit, task, at: humantime::format_rfc3339_seconds(t).to_string() }
            }
            QuestionFailed { question, reason_code, reason } => {
                if !self.question(&question)?.open() {
                    return no(format!("question {question} is already closed"));
                }
                QuestionFailed { question, reason_code, reason }
            }
            other => unreachable!("{} is not one of a question's kinds", name_of_kind(&other)),
        })
    }

    /// What a question's events change: `apply`, for these kinds.
    pub(crate) fn apply_questions(&mut self, e: &Event) {
        use Body::*;
        let at = &e.at;
        match &e.body {
            QuestionAsked { id, thread, unit, task, text, context, conclude, discuss } => {
                if !self.threads.iter().any(|t| &t.id == thread) {
                    self.threads.push(Thread { id: thread.clone(), unit: unit.clone(), task: *task, agent: None, name: None, ended: false, conclusion: None, discuss: *discuss, amended: false });
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
                discuss: *discuss,
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
            QuestionsRead { unit, task, at: upto } => {
                let r = self.questions_read.entry(format!("{unit}#{task}")).or_default();
                if *upto > *r {
                    *r = upto.clone();
                }
            }
            QuestionFailed { question, reason_code, reason } => {
                if let Some(q) = self.questions.iter_mut().find(|q| &q.id == question) {
                    q.failed = Some((*reason_code, reason.clone()));
                }
            }
            other => unreachable!("{} is not one of a question's kinds", name_of_kind(other)),
        }
    }
}
