// The agents workriver holds itself: each runs under a host process of workriver's own,
// started only by the user. The records keep the receipts (the reservation, the start or its
// end, the user's answers, the close); the conversation stays in the host's journal.
use serde::{Deserialize, Serialize};

use crate::access::{Principal, Rejected, no};
use crate::planning::Shell;
use crate::records::*;
use crate::work::Task;

#[cfg(test)]
mod tests;

/// A hosted agent as recorded.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct HostedAgent {
    /// `a<N>`, in the order starts are reserved; never given again.
    pub id: String,
    pub workspace: String,
    pub name: String,
    pub harness: String,
    /// What the user asked for; empty: the harness's default.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    pub cwd: String,
    pub auth: String,
    /// When the start was reserved.
    pub at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started: Option<Started>,
    /// Why the start ended without starting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed: Option<Closed>,
    /// The keys of the requests whose answers are recorded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answered: Vec<u64>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Started {
    pub session: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    pub host: Shell,
    pub generation: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub account: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub plan: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub approvals: String,
    pub at: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Closed {
    pub outcome: Outcome,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pids: Vec<u32>,
    pub at: String,
}

/// What the user answered an approval request.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Answer {
    Allow,
    Reject,
}

/// How a close ended, as the host wrote it.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    /// No process of the agent's group was left.
    Ended,
    /// Processes of its group were left at the deadline.
    Stuck,
}

impl HostedAgent {
    /// Reserved or started, and neither ended nor closed: its name is taken.
    pub fn open(&self) -> bool {
        self.ended.is_none() && self.closed.is_none()
    }

    /// Started and not closed.
    pub fn running(&self) -> bool {
        self.started.is_some() && self.closed.is_none()
    }

    pub fn session(&self) -> Option<&str> {
        self.started.as_ref().map(|s| s.session.as_str())
    }

    /// Its place in the runtime for a generation: `acp:a7/1`.
    pub fn place(&self, generation: u64) -> String {
        format!("acp:{}/{generation}", self.id)
    }
}

impl State {
    pub fn hosted_agent(&self, id: &str) -> Result<&HostedAgent, Rejected> {
        match self.hosted.iter().find(|a| a.id == id) {
            Some(a) => Ok(a),
            None => no(format!("no hosted agent {id}; `workriver agent list` shows them")),
        }
    }

    /// A hosted agent by its id, or by the name an open one has.
    pub fn hosted_named(&self, agent: &str) -> Result<&HostedAgent, Rejected> {
        match self.hosted.iter().rev().find(|a| a.id == agent || (a.name == agent && a.open())) {
            Some(a) => Ok(a),
            None => no(format!("no hosted agent {agent}; `workriver agent list` shows them")),
        }
    }

    /// The hosted agent a session was started as, if any: once recorded hosted, a session is
    /// that agent only at its own host, closed or not.
    pub fn hosted_by_session(&self, session: &str) -> Option<&HostedAgent> {
        self.hosted.iter().find(|a| a.session() == Some(session))
    }

    /// The workspace a hosted agent works for, as a worker: started there, not closed, and
    /// holding no manager's seat of any workspace now (a manager is nobody's worker).
    pub fn worker_of<'a>(&'a self, agent: &'a HostedAgent) -> Option<&'a str> {
        let session = agent.session().filter(|_| agent.running())?;
        self.seat_of(session).is_none().then_some(agent.workspace.as_str())
    }

    /// Whether `who` may prompt, read or cancel a hosted agent: the user any; a workspace's
    /// seated manager its own workers; nobody else.
    pub fn drives(&self, who: &Principal, agent: &HostedAgent) -> bool {
        match who {
            Principal::User => true,
            Principal::Agent { session, .. } => self.worker_of(agent).is_some_and(|w| self.seat_of(session).is_some_and(|s| s.id == w)),
            Principal::Service => false,
        }
    }

    fn reservation(&self, id: &str) -> Result<&HostedAgent, Rejected> {
        let a = self.hosted_agent(id)?;
        match (&a.started, &a.ended) {
            (None, None) => Ok(a),
            (Some(_), _) => no(format!("the start of {id} is recorded already")),
            (_, Some(_)) => no(format!("the start of {id} has ended")),
        }
    }

    fn running_hosted(&self, id: &str) -> Result<&HostedAgent, Rejected> {
        let a = self.hosted_agent(id)?;
        match (&a.started, &a.closed) {
            (Some(_), None) => Ok(a),
            (None, _) => no(format!("{id} has not started")),
            (_, Some(_)) => no(format!("{id} is closed")),
        }
    }

    pub(crate) fn prepare_hosted(&self, _who: &Principal, body: Body) -> Result<Body, Rejected> {
        use Body::*;
        Ok(match body {
            AgentStartReserved { workspace, name, harness, model, cwd, auth, .. } => {
                if self.workspace(&workspace)?.closed {
                    return no(format!("{workspace} is closed: open it before starting an agent there"));
                }
                if blank(&name) || matches!(name.as_str(), "user" | "workriver") || name.starts_with('a') && name[1..].parse::<u64>().is_ok() {
                    return no(format!("{name:?} cannot name an agent: give it a name that is not empty, `user`, `workriver` or an id like `a1`"));
                }
                if let Some(a) = self.hosted.iter().find(|a| a.name == name && a.open()) {
                    return no(format!("{name} is {} already, which is not closed: close it, or give the new one another name", a.id));
                }
                if blank(&harness) {
                    return no("name the harness it runs");
                }
                if !cwd.starts_with('/') {
                    return no(format!("{cwd:?} is not an absolute folder"));
                }
                if !matches!(auth.as_str(), "chatgpt" | "api_key") {
                    return no(format!("{auth:?} is not a way to sign in: chatgpt or api_key"));
                }
                AgentStartReserved { id: format!("a{}", self.hosted.len() + 1), workspace, name, harness, model, cwd, auth }
            }
            AgentStarted { agent, session, model, host, generation, version, account, plan, approvals } => {
                self.reservation(&agent)?;
                if blank(&session) || matches!(session.as_str(), "user" | "workriver") {
                    return no("a hosted agent is started with its session");
                }
                if generation == 0 {
                    return no("a host's first generation is 1");
                }
                let name = self.name_of(&session);
                let theirs = |t: &Task| t.agent.as_deref().is_some_and(|a| a == session || Some(a) == name);
                if self.agents.iter().any(|a| a.session == session) || self.hosted_by_session(&session).is_some() || self.seat_of(&session).is_some() || self.units.iter().flat_map(|u| &u.tasks).any(theirs) {
                    return no(format!("{session} was known before {agent} started: a start records only the session its host brought up"));
                }
                AgentStarted { agent, session, model, host, generation, version, account, plan, approvals }
            }
            AgentStartEnded { agent, why } => {
                self.reservation(&agent)?;
                AgentStartEnded { agent, why }
            }
            AgentAnswered { agent, request, decision } => {
                if self.running_hosted(&agent)?.answered.contains(&request) {
                    return no(format!("the answer to request {request} of {agent} is recorded already"));
                }
                AgentAnswered { agent, request, decision }
            }
            AgentClosed { agent, outcome, pids } => {
                self.running_hosted(&agent)?;
                AgentClosed { agent, outcome, pids }
            }
            other => unreachable!("{} is not one of a hosted agent's kinds", name_of_kind(&other)),
        })
    }

    pub(crate) fn apply_hosted(&mut self, e: &Event) {
        use Body::*;
        let at = e.at.clone();
        let find = |s: &mut Self, id: &str| s.hosted.iter().position(|a| a.id == id);
        match &e.body {
            AgentStartReserved { id, workspace, name, harness, model, cwd, auth } => self.hosted.push(HostedAgent {
                id: id.clone(), workspace: workspace.clone(), name: name.clone(), harness: harness.clone(), model: model.clone(),
                cwd: cwd.clone(), auth: auth.clone(), at, started: None, ended: None, closed: None, answered: vec![],
            }),
            AgentStarted { agent, session, model, host, generation, version, account, plan, approvals } => {
                if let Some(k) = find(self, agent) {
                    self.hosted[k].started = Some(Started {
                        session: session.clone(), model: model.clone(), host: *host, generation: *generation, version: version.clone(),
                        account: account.clone(), plan: plan.clone(), approvals: approvals.clone(), at,
                    });
                }
            }
            AgentStartEnded { agent, why } => {
                if let Some(k) = find(self, agent) {
                    self.hosted[k].ended = Some(why.clone());
                }
            }
            AgentAnswered { agent, request, .. } => {
                if let Some(k) = find(self, agent) {
                    self.hosted[k].answered.push(*request);
                }
            }
            AgentClosed { agent, outcome, pids } => {
                if let Some(k) = find(self, agent) {
                    self.hosted[k].closed = Some(Closed { outcome: *outcome, pids: pids.clone(), at });
                }
            }
            other => unreachable!("{} is not one of a hosted agent's kinds", name_of_kind(other)),
        }
    }
}
