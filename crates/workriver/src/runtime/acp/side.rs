// The hosts as a runtime: each published host's root is its agent's place (`acp:a7/1`), a
// host not published yet is a place that is starting, and a message is a prompt with its
// delivery key. Only this machine has hosts.
use std::time::Duration;

use serde_json::json;
use workriver_core::runtime::{AgentState, Place, Root, Runtime, RuntimeAgent, RuntimeError};

use super::service::{HostError, Hosts, Published, runs};

/// `acp:a7/2` as its instance and generation.
fn parse(place: &str) -> Option<(&str, u64)> {
    let (id, generation) = place.strip_prefix("acp:")?.split_once('/')?;
    Some((id, generation.parse().ok()?))
}

fn state(word: &str) -> AgentState {
    match word {
        "idle" => AgentState::Idle,
        "working" | "closing" => AgentState::Working,
        "blocked" => AgentState::Blocked,
        _ => AgentState::Unknown,
    }
}

/// The agent a published host holds, its state not read (identity does not need it).
fn quiet(p: &Published) -> RuntimeAgent {
    RuntimeAgent { session: p.session.clone(), harness: p.harness.clone(), state: AgentState::Unknown, cwd: p.cwd.clone(), group: None, name: p.name.clone(), topic: None, machine: None }
}

impl Hosts {
    fn agent(&self, id: &str, p: &Published) -> RuntimeAgent {
        let word = match self.call(id, "status", json!({}), Duration::from_secs(2)) {
            Ok(s) => s["state"].as_str().unwrap_or_default().to_string(),
            Err(_) => String::new(),
        };
        RuntimeAgent { state: if self.alive(id) { state(&word) } else { AgentState::Unknown }, ..quiet(p) }
    }

    /// The roots of the hosts that run, each an agent's place once published, else starting.
    fn live_roots(&self) -> Vec<Root> {
        let mut out = vec![];
        for (id, pid, start, agent) in self.all() {
            if !runs(pid, start) {
                continue;
            }
            let (place, at) = match &agent {
                Some(p) => (format!("acp:{id}/{}", p.generation), Place { agent: Some(quiet(p)), starting: false }),
                None => (format!("acp:{id}/1"), Place { agent: None, starting: true }),
            };
            out.push(Root { place, shell: Some(pid), front: vec![], at });
        }
        out
    }

    /// The published instance at `place`, while its generation is the one published.
    fn at(&self, place: &str) -> Option<(String, Published)> {
        let (id, generation) = parse(place)?;
        self.all().into_iter().find(|(i, ..)| i == id).and_then(|(i, _, _, a)| Some((i, a.filter(|a| a.generation == generation)?)))
    }
}

impl Runtime for Hosts {
    fn agents(&self) -> Result<Vec<RuntimeAgent>, RuntimeError> {
        Ok(self.all().into_iter().filter_map(|(id, _, _, a)| Some(self.agent(&id, a.as_ref()?))).collect())
    }

    fn locate(&self, machine: Option<&str>, place: &str) -> Result<Place, RuntimeError> {
        if machine.is_some() {
            return Ok(Place::default());
        }
        Ok(Place { agent: self.at(place).map(|(id, p)| self.agent(&id, &p)), starting: false })
    }

    fn focus(&self, _: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unsupported)
    }

    /// A prompt with the message's key, aimed at the place's generation: the host takes a key
    /// once, so a try after a lost answer is answered as a repeat and sends nothing.
    fn send_at(&self, machine: Option<&str>, place: &str, session: &str, key: &str, text: &str) -> Result<(), RuntimeError> {
        let (id, generation) = parse(place).filter(|_| machine.is_none()).ok_or_else(|| RuntimeError::NoAgent(session.into()))?;
        let params = json!({"generation": generation, "session": session, "key": key, "text": text});
        match self.call(id, "prompt", params, Duration::from_secs(5)) {
            Ok(_) => Ok(()),
            Err(HostError::Refused(why)) => Err(match why.as_str() {
                "busy" => RuntimeError::Busy(format!("{session} is in a turn")),
                "blocked" => RuntimeError::Blocked(format!("{session} waits for an answer to an approval")),
                "billing" => RuntimeError::Blocked(format!("{session}'s account is being checked again")),
                "moved" => RuntimeError::Moved(format!("{session} runs in a newer generation than {place}")),
                "gone" => RuntimeError::NoAgent(session.into()),
                other => RuntimeError::Failed(format!("{id} refused the message: {other}")),
            }),
            Err(HostError::Unreachable(why)) => Err(RuntimeError::Unreachable(why)),
        }
    }

    fn start_temporary(&self, _: &str, _: &str, _: &str, _: &str) -> Result<RuntimeAgent, RuntimeError> {
        Err(RuntimeError::Unsupported)
    }

    fn close(&self, _: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unsupported)
    }

    fn processes(&self, machine: Option<&str>) -> Result<Vec<(RuntimeAgent, Vec<u32>)>, RuntimeError> {
        if machine.is_some() {
            return Ok(vec![]);
        }
        Ok(self.live_roots().into_iter().filter_map(|r| Some((r.at.agent?, vec![r.shell?]))).collect())
    }

    fn roots(&self, machine: Option<&str>, place: Option<&str>) -> Result<Vec<Root>, RuntimeError> {
        if machine.is_some() {
            return Ok(vec![]);
        }
        Ok(self.live_roots().into_iter().filter(|r| place.is_none_or(|p| p == r.place)).collect())
    }

    fn resolve(&self, machine: Option<&str>, session: &str) -> Result<String, RuntimeError> {
        if machine.is_some() {
            return Err(RuntimeError::NoAgent(session.into()));
        }
        let found = self.all().into_iter().find(|(_, pid, start, a)| a.as_ref().is_some_and(|a| a.session == session) && runs(*pid, *start));
        match found {
            Some((id, _, _, Some(a))) => Ok(format!("acp:{id}/{}", a.generation)),
            _ => Err(RuntimeError::NoAgent(session.into())),
        }
    }
}
