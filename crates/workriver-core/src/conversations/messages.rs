// Messages between the user and a workspace's manager, their replies, and the agents seen.
use serde::{Deserialize, Serialize};

use crate::*;

/// An agent session as last seen: the name shown for it, and its harness.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Agent {
    pub session: String,
    pub name: String,
    pub harness: String,
}

/// The user's message to a workspace's manager, and what it answered.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Message {
    pub id: String,
    pub intent: String,
    pub workspace: String,
    /// The manager's session it went to: only that one replies.
    pub manager: String,
    pub text: String,
    pub at: String,
    pub delivery: Option<Delivery>,
    #[serde(default)]
    pub replies: Vec<Reply>,
    /// Started by the manager (`workriver say`): its text is the manager's, and it went nowhere.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_manager: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Reply {
    pub by: String,
    pub text: String,
    pub at: String,
}

impl State {
    /// The name last seen for an agent session.
    pub fn name_of(&self, session: &str) -> Option<&str> {
        self.agents.iter().find(|a| a.session == session).map(|a| a.name.as_str())
    }

    pub fn message(&self, id: &str) -> Result<&Message, Rejected> {
        match self.messages.iter().find(|m| m.id == id) {
            Some(m) => Ok(m),
            None => no(format!("no message {id}")),
        }
    }

    /// The rules of a message's events: `prepare`, for these kinds.
    pub(crate) fn prepare_messages(&self, who: &Principal, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        let actor = who.actor();
        let user = *who == Principal::User;
        Ok(match body {
            AgentSeen { session, name, harness } => {
                if blank(&session) || blank(&name) {
                    return no("an agent needs a session and a name");
                }
                AgentSeen { session, name, harness }
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
            other => unreachable!("{} is not one of a message's kinds", name_of_kind(&other)),
        })
    }

    /// What a message's events change: `apply`, for these kinds.
    pub(crate) fn apply_messages(&mut self, e: &Event) {
        use Body::*;
        let at = &e.at;
        match &e.body {
            AgentSeen { session, name, harness } => {
                let seen = Agent { session: session.clone(), name: name.clone(), harness: harness.clone() };
                match self.agents.iter_mut().find(|a| &a.session == session) {
                    Some(a) => *a = seen,
                    None => self.agents.push(seen),
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
            other => unreachable!("{} is not one of a message's kinds", name_of_kind(other)),
        }
    }
}
