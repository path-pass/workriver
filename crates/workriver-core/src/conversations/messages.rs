// Messages between the user and a workspace's manager, their replies, and the agents seen.
use serde::{Deserialize, Serialize};

use crate::*;

/// An agent session as last seen: the name shown for it, its harness, the working
/// directory it was first seen in and its machine.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Agent {
    pub session: String,
    pub name: String,
    pub harness: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    /// The sequence of the event it was first seen at: whether the records knew it before a
    /// manager's start. Rebuilt on every replay, never stored (0: known before anything).
    #[serde(skip)]
    pub first: u64,
}

/// The user's message to a workspace's manager (or to every manager), and what it answered.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Message {
    pub id: String,
    pub intent: String,
    /// The workspace whose conversation it is in, by id; empty: the conversation with
    /// every manager.
    pub workspace: String,
    /// The manager's session it went to: only that one replies. Empty for one to every manager.
    pub manager: String,
    /// The appointment it belongs to: the `since` of the workspace's seat when it was sent
    /// (to that manager) or said (by it). 0: the seat was not that manager's then. Only that
    /// appointment replies, is given it, and has its choice answered.
    #[serde(default, skip_serializing_if = "is_none_yet")]
    pub since: u64,
    pub text: String,
    /// What the user had picked: a unit (empty: none), one of its tasks (0: none), and the
    /// rest (findings, files, lines, attachments, other tasks).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    #[serde(default, skip_serializing_if = "crate::records::is_zero")]
    pub task: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<AskContext>,
    /// To every manager: each manager it went to, with where its copy is (then `delivery`
    /// stays empty). Each of them replies for its own workspace.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub to: Vec<Recipient>,
    pub at: String,
    pub delivery: Option<Delivery>,
    #[serde(default)]
    pub replies: Vec<Reply>,
    /// Started by the manager (`workriver say`): its text is the manager's, and it went nowhere.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_manager: bool,
    /// A manager's message with options to pick from, and the user's answer to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<Choice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered: Option<Answered>,
    /// The user's message that answers a manager's choice: which one, and the value picked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<Answer>,
    /// Not a message of anyone's: a line workriver puts in the conversation. Then `manager`
    /// is the session it is about, and there is no text (the page words it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<Notice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<Rating>,
}

fn is_none_yet(n: &u64) -> bool {
    *n == 0
}

/// What a line of workriver's own in a conversation says.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Notice {
    /// The seated session's terminal was gone; it shows again where it was and is the
    /// manager again, by itself.
    ManagerBack,
}

impl Message {
    /// The manager who wrote the addressed reply, and the appointment it was written under.
    /// Reply zero addresses a message the manager started.
    pub fn rating_author(&self, reply: u32) -> Result<(&str, u64), Rejected> {
        if reply == 0 {
            return if self.by_manager { Ok((&self.manager, self.since)) } else { no("only a manager's words can be rated") };
        }
        let Some(r) = self.replies.get(reply as usize - 1) else { return no(format!("{} has no reply {reply}", self.id)) };
        let since = self.to.iter().find(|copy| copy.manager == r.by).map_or(self.since, |copy| copy.since);
        Ok((&r.by, since))
    }

    /// A line of workriver's own in workspace `workspace`'s conversation, about `session`.
    pub(crate) fn notice(id: &str, workspace: &str, session: &str, at: &str, notice: Notice) -> Message {
        Message { id: id.into(), workspace: workspace.into(), manager: session.into(), at: at.into(), notice: Some(notice), ..Message::default() }
    }
}

/// A manager a message to every manager goes to: the seat of an open workspace when it is
/// sent (the workspace's id, the session, its machine and its appointment), and where its
/// copy is (`None`: not given yet; never in the event that sends it).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Recipient {
    pub workspace: String,
    pub manager: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    pub since: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<Delivery>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Reply {
    pub by: String,
    pub text: String,
    pub at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<Choice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered: Option<Answered>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<Rating>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Vote {
    Up,
    Down,
}

/// Session metadata available at the time of the rating. Context use is the last input's
/// tokens, including cached input. The window is kept when the log reports it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct SessionUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compactions: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Rating {
    pub vote: Vote,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub at: String,
    pub session: String,
    pub usage: SessionUsage,
}

/// What the user's message answers: a manager's message (`m7`) or its `reply`-th reply (from
/// 1; 0: the message itself), and the value picked.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Answer {
    pub message: String,
    #[serde(default, skip_serializing_if = "crate::records::is_zero")]
    pub reply: u32,
    pub selected: Vec<String>,
}

/// A choice answered: by which message of the user's, and the value picked.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Answered {
    pub by: String,
    pub selected: Vec<String>,
}

impl State {
    /// The name last seen for an agent session.
    pub fn name_of(&self, session: &str) -> Option<&str> {
        self.agents.iter().find(|a| a.session == session).map(|a| a.name.as_str())
    }

    /// The managers running now, here and on other machines: whom a message to every manager
    /// goes to. The seat of each open workspace whose session is in `live` on the seat's
    /// machine, in the workspaces' order.
    pub fn managers_among(&self, live: &[crate::runtime::RuntimeAgent]) -> Vec<Recipient> {
        let runs = |m: &Seat| live.iter().any(|a| a.session == m.session && a.machine == m.machine);
        let seats = self.workspaces.iter().filter(|w| !w.closed).filter_map(|w| Some((w, w.manager.as_ref()?)));
        seats.filter(|(_, m)| runs(m)).map(|(w, m)| Recipient { workspace: w.id.clone(), manager: m.session.clone(), machine: m.machine.clone(), since: m.since, delivery: None }).collect()
    }

    /// Whether `session` holds the seat of workspace `id` under appointment `since`.
    pub fn appointed(&self, id: &str, session: &str, since: u64) -> bool {
        self.workspaces.iter().any(|w| w.id == id && w.manager.as_ref().is_some_and(|m| m.session == session && m.since == since))
    }

    /// A manager's choice in a conversation: its message's `reply`-th reply (0: the message),
    /// with who asked it and its answer so far.
    fn asked(&self, message: &str, reply: u32) -> Result<(&str, &Choice, Option<&Answered>), Rejected> {
        let m = self.message(message)?;
        let (by, choice, answered) = match reply {
            0 => (m.by_manager.then_some(m.manager.as_str()), m.choice.as_ref(), m.answered.as_ref()),
            k => {
                let Some(r) = m.replies.get(k as usize - 1) else { return no(format!("{message} has no reply {k}")) };
                (Some(r.by.as_str()), r.choice.as_ref(), r.answered.as_ref())
            }
        };
        match (by, choice) {
            (Some(by), Some(c)) => Ok((by, c, answered)),
            _ => no(format!("{message}{} asks nothing to choose", if reply == 0 { String::new() } else { format!("'s reply {reply}") })),
        }
    }

    /// Whether the user can still answer a manager's choice in a conversation: not answered,
    /// and its asker holds the workspace's seat under the appointment it asked under (an
    /// answer to an earlier appointment would go nowhere). The same rule routes the answer
    /// (`prepare` of `message.sent`). A rename changes nothing: the workspace is its id.
    pub fn choice_open(&self, message: &str, reply: u32) -> bool {
        let Ok(m) = self.message(message) else { return false };
        self.asked(message, reply).is_ok_and(|(by, _, answered)| answered.is_none() && self.appointed(&m.workspace, by, m.since))
    }

    /// The appointment `session` holds the seat of workspace `id` under; 0: it does not.
    fn appointment(&self, id: &str, session: &str) -> u64 {
        self.workspaces.iter().find(|w| w.id == id).and_then(|w| w.manager.as_ref()).filter(|m| m.session == session).map_or(0, |m| m.since)
    }

    pub fn message(&self, id: &str) -> Result<&Message, Rejected> {
        match self.messages.iter().find(|m| m.id == id) {
            Some(m) => Ok(m),
            None => no(format!("no message {id}")),
        }
    }

    /// What the user picked for a message (`MessageSent`): a unit of the message's intent
    /// (with none named, of an intent that runs in `target`, the workspace whose manager it
    /// goes to), a task of it, and context that needs that unit, but for files the user
    /// stored, which may go to any manager.
    fn picked(&self, intent: &str, target: Option<&Workspace>, unit: &str, task: u32, context: &[AskContext]) -> Result<(), Rejected> {
        let needs = |what: &str| no(format!("{what} needs the unit it is about; pick it (or take {what} out)"));
        if unit.is_empty() {
            if task != 0 {
                return needs(&format!("task {task}"));
            }
        } else {
            let Some(of) = self.intent_of(unit) else { return no(format!("no unit {unit}")) };
            if !intent.is_empty() && of != intent {
                return no(format!("{unit} is not of intent {intent}"));
            }
            if let Some(w) = target.filter(|_| intent.is_empty())
                && self.intent_workspace(of).is_none_or(|x| x.id != w.id)
            {
                return no(format!("{unit} is not of an intent of workspace {} ({})", w.id, w.name));
            }
            if task != 0 {
                self.task(unit, task)?;
            }
        }
        for c in context {
            match c.kind.as_str() {
                "attachment" => {
                    let a = c.id.as_deref().unwrap_or_default();
                    if self.pointable(unit, task, a, true) {
                        continue;
                    }
                    if unit.is_empty() {
                        return needs(&format!("attachment {a:?}"));
                    }
                    return no(format!("attachment {a:?} is not one of {unit}#{task}'s; a message points only at what is shown with its task, or at a file the user attached"));
                }
                "task" | "finding" | "file" | "lines" if unit.is_empty() => return needs(&format!("{} {:?}", c.kind, c.label)),
                "task" => match c.task {
                    Some(n) => {
                        self.task(unit, n)?;
                    }
                    None => return no(format!("a task pointed at names its task by number ({:?} has none)", c.label)),
                },
                "finding" | "file" | "lines" => {}
                k => return no(format!("a message points at tasks, findings, files, lines and attachments, not {k:?}")),
            }
        }
        Ok(())
    }

    /// The rules of a message's events: `prepare`, for these kinds.
    pub(crate) fn prepare_messages(&self, who: &Principal, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        let actor = who.actor();
        let user = *who == Principal::User;
        Ok(match body {
            AgentSeen { session, name, harness, cwd, machine } => {
                if blank(&session) || blank(&name) {
                    return no("an agent needs a session and a name");
                }
                AgentSeen { session, name, harness, cwd, machine }
            }
            MessageSent { intent, text, unit, task, context, to, answer, .. } if !to.is_empty() => {
                if blank(&text) {
                    return no("a message needs its text");
                }
                if answer.is_some() {
                    return no("an answer goes to the manager that asked, not to every manager");
                }
                if !intent.is_empty() || !unit.is_empty() || task != 0 {
                    return no("a message to every manager is about no intent or unit; only the user's own files go with it");
                }
                // Each the seat of an open workspace now, as it is (its session, machine and
                // appointment), once; a blind one is not refused: its copy waits for that task
                // (the service holds it).
                let mut seen: Vec<&str> = vec![];
                for r in &to {
                    let w = self.workspace(&r.workspace)?;
                    if w.closed || w.manager.as_ref().is_none_or(|m| m.session != r.manager || m.since != r.since || m.machine != r.machine) {
                        return no(format!("{} does not hold the seat of workspace {} as given", self.name_of(&r.manager).unwrap_or(&r.manager), r.workspace));
                    }
                    if seen.contains(&r.manager.as_str()) {
                        return no(format!("a message to every manager goes to {} twice", self.name_of(&r.manager).unwrap_or(&r.manager)));
                    }
                    seen.push(&r.manager);
                }
                self.picked("", None, "", 0, &context)?;
                let to = to.into_iter().map(|r| Recipient { delivery: None, ..r }).collect();
                MessageSent { id: format!("m{}", self.messages.len() + 1), intent, workspace: String::new(), manager: String::new(), text, unit, task, context, to, answer: None }
            }
            MessageSent { intent, workspace, text, unit, task, context, answer, .. } => {
                if blank(&text) && answer.is_none() {
                    return no("a message needs its text");
                }
                // An answer goes to whoever asked, in the question's workspace and about its
                // intent, while it holds that workspace's seat under the appointment it asked
                // under. Otherwise to the seat of the intent's workspace, or of the workspace
                // named. (Resolved once: the unit picked must be of an intent that runs in it.)
                let (intent, workspace, manager, target) = if let Some(a) = &answer {
                    let (by, choice, answered) = self.asked(&a.message, a.reply)?;
                    if answered.is_some() {
                        return no(format!("{} is answered already; write to the manager instead", a.message));
                    }
                    let q = self.message(&a.message)?;
                    if !self.appointed(&q.workspace, by, q.since) {
                        return no(format!("{} was asked by an earlier manager of this workspace; write to the manager instead", a.message));
                    }
                    choice.check_answer(&a.selected, &text)?;
                    (q.intent.clone(), q.workspace.clone(), Some(by.to_string()), None)
                } else if intent.is_empty() {
                    let w = self.workspace(&workspace)?;
                    (intent, workspace, w.manager.as_ref().map(|m| m.session.clone()), Some(w))
                } else {
                    self.intent(&intent)?;
                    let Some(w) = self.intent_workspace(&intent) else {
                        return no(format!("intent {intent} runs in no workspace: start it first, or write to a workspace's manager"));
                    };
                    (intent, w.id.clone(), w.manager.as_ref().map(|m| m.session.clone()), None)
                };
                let Some(manager) = manager else {
                    return no(format!("{} has no manager; {SEATING}", if intent.is_empty() { format!("workspace {workspace}") } else { format!("intent {intent}'s workspace {workspace}") }));
                };
                // Nothing about other work is typed into a blind agent.
                if !self.blind_tasks(&manager).is_empty() {
                    return no(format!("the manager {} is on a blind task: write once it is delivered", self.name_of(&manager).unwrap_or(&manager)));
                }
                self.picked(&intent, target, &unit, task, &context)?;
                MessageSent { id: format!("m{}", self.messages.len() + 1), intent, workspace, manager, text, unit, task, context, to: vec![], answer }
            }
            MessageDelivery { id, manager, delivery } => {
                if *who != Principal::Service {
                    return no("only workriver records where a message is");
                }
                let m = self.message(&id)?;
                // A message to every manager: one copy's, of a manager it went to.
                match (m.to.is_empty(), manager.is_empty()) {
                    (true, false) => return no(format!("message {id} went to one manager: its delivery names no copy")),
                    (false, true) => return no(format!("message {id} went to every manager: say whose copy")),
                    (false, false) if !m.to.iter().any(|r| r.manager == manager) => return no(format!("message {id} did not go to {manager}")),
                    _ => {}
                }
                MessageDelivery { id, manager, delivery }
            }
            ManagerTold { unit, task, text } => {
                if user {
                    return no("the user writes to a manager in workriver's page (Manager)");
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
            MessageReplied { message, text, choice } => {
                let m = self.message(&message)?;
                if m.notice.is_some() {
                    return no(format!("{message} is a line of workriver's own, not a message: there is nothing to reply to"));
                }
                // To every manager: each it went to, for its own workspace.
                let copy = m.to.iter().find(|r| r.manager == actor);
                if !m.to.is_empty() && copy.is_none() {
                    return no(format!("message {message} went to every manager; only the managers it went to reply to it"));
                }
                // Only the manager it was typed to: nobody else answers for it, the user included.
                if m.to.is_empty() && m.manager != actor {
                    return no(format!("message {message} went to {}; only that manager replies to it", self.name_of(&m.manager).unwrap_or(&m.manager)));
                }
                // And only as the appointment it went to: seated again, it is another.
                let (workspace, since) = copy.map_or((&m.workspace, m.since), |r| (&r.workspace, r.since));
                if !self.appointed(workspace, actor, since) {
                    return no(format!("message {message} went to an earlier manager of workspace {workspace}; it takes no reply now: say what you have to say with `workriver say --text \"...\"`"));
                }
                if blank(&text) {
                    return no("a reply needs its text");
                }
                if let Some(c) = &choice {
                    // An answer is routed to one workspace's conversation: ask there (`say`).
                    if !m.to.is_empty() {
                        return no("a reply to a message to every manager asks nothing to choose; ask in your own workspace: `workriver say --text \"...\" --option ...`");
                    }
                    c.check()?;
                }
                MessageReplied { message, text, choice }
            }
            MessageSaid { text, choice, .. } => {
                if user {
                    return no("the user writes to a manager in workriver's page (Manager)");
                }
                let Some(w) = self.seat_of(actor) else {
                    return no("only a workspace's manager says something to the user; a worker tells its manager: `workriver manager tell <task> --text \"...\"`");
                };
                if blank(&text) {
                    return no("say what: --text \"...\"");
                }
                if let Some(c) = &choice {
                    c.check()?;
                }
                MessageSaid { id: format!("m{}", self.messages.len() + 1), intent: w.current.clone().unwrap_or_default(), workspace: w.id.clone(), text, choice }
            }
            MessageRated { message, reply, vote, note, usage, .. } => {
                let (session, _) = self.message(&message)?.rating_author(reply)?;
                let note = note.map(|s| s.trim().to_string()).filter(|s| !shows_nothing(s));
                if let Some(s) = &note
                    && (vote != Some(Vote::Down) || s.contains(['\n', '\r', '\u{2028}', '\u{2029}']))
                {
                    return no("only a thumbs down takes a note, on one line");
                }
                MessageRated { message, reply, vote, note, session: session.to_string(), usage }
            }
            MessagesRead { workspace, at } => {
                // Empty: the conversation with every manager.
                if !workspace.is_empty() {
                    self.workspace(&workspace)?;
                }
                MessagesRead { workspace, at: crate::records::stored_time(&at)? }
            }
            other => unreachable!("{} is not one of a message's kinds", name_of_kind(&other)),
        })
    }

    /// What a message's events change: `apply`, for these kinds.
    pub(crate) fn apply_messages(&mut self, e: &Event) {
        use Body::*;
        let at = &e.at;
        match &e.body {
            AgentSeen { session, name, harness, cwd, machine } => {
                let mut seen = Agent { session: session.clone(), name: name.clone(), harness: harness.clone(), cwd: cwd.clone(), machine: machine.clone(), first: e.seq as u64 };
                match self.agents.iter_mut().find(|a| &a.session == session) {
                    Some(a) => {
                        seen.first = a.first;
                        *a = seen
                    }
                    None => self.agents.push(seen),
                }
            }
            MessageSent { id, intent, workspace, manager, text, unit, task, context, to, answer } => {
                if let Some(a) = answer {
                    let answered = Some(Answered { by: id.clone(), selected: a.selected.clone() });
                    if let Some(m) = self.messages.iter_mut().find(|m| m.id == a.message) {
                        match a.reply {
                            0 => m.answered = answered,
                            k => {
                                if let Some(r) = m.replies.get_mut(k as usize - 1) {
                                    r.answered = answered;
                                }
                            }
                        }
                    }
                }
                let since = self.appointment(workspace, manager);
                self.messages.push(Message {
                    id: id.clone(),
                    intent: intent.clone(),
                    workspace: workspace.clone(),
                    manager: manager.clone(),
                    since,
                    text: text.clone(),
                    unit: unit.clone(),
                    task: *task,
                    context: context.clone(),
                    to: to.clone(),
                    at: at.clone(),
                    answer: answer.clone(),
                    ..Message::default()
                })
            }
            MessageSaid { id, intent, workspace, text, choice } => self.messages.push(Message {
                id: id.clone(),
                intent: intent.clone(),
                workspace: workspace.clone(),
                manager: e.actor.clone(),
                since: self.appointment(workspace, &e.actor),
                text: text.clone(),
                at: at.clone(),
                by_manager: true,
                choice: choice.clone(),
                ..Message::default()
            }),
            MessagesRead { workspace, at: upto } => {
                let r = self.read.entry(workspace.clone()).or_default();
                if *upto > *r {
                    *r = upto.clone();
                }
            }
            MessageDelivery { id, manager, delivery } => {
                if let Some(m) = self.messages.iter_mut().find(|m| &m.id == id) {
                    match m.to.iter_mut().find(|r| &r.manager == manager) {
                        Some(r) => r.delivery = Some(delivery.clone()),
                        None => m.delivery = Some(delivery.clone()),
                    }
                }
            }
            MessageReplied { message, text, choice } => {
                if let Some(m) = self.messages.iter_mut().find(|m| &m.id == message) {
                    m.replies.push(Reply { by: e.actor.clone(), text: text.clone(), at: at.clone(), choice: choice.clone(), answered: None, rating: None });
                }
            }
            MessageRated { message, reply, vote, note, session, usage } => {
                if let Some(m) = self.messages.iter_mut().find(|m| &m.id == message) {
                    let rating = vote.map(|vote| Rating { vote, note: note.clone(), at: at.clone(), session: session.clone(), usage: usage.clone() });
                    match *reply {
                        0 => m.rating = rating,
                        k => {
                            if let Some(r) = m.replies.get_mut(k as usize - 1) {
                                r.rating = rating;
                            }
                        }
                    }
                }
            }
            // Only told: the record is the event itself.
            ManagerTold { .. } => {}
            other => unreachable!("{} is not one of a message's kinds", name_of_kind(other)),
        }
    }
}
