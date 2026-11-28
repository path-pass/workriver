// Processes a hosted agent left behind: this account's processes whose marker names one of
// this service's instances (by its tag) that no running launch holds: its agent closed, its
// start ended, an older generation, or a host that is gone. Listed for the user at any time;
// ended only on the user's request, one or all, each read again before every signal. Nothing
// ends them by itself, after a restart neither. Only the marker is read of an environment.
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use workriver_core as core;

use crate::common::process::{self, Owned};
use crate::server::app::*;
use crate::server::http::*;

/// One leftover process: its id, start, the agent and generation its marker names.
#[derive(Clone, Debug)]
pub(super) struct Leftover {
    pid: u32,
    start: u64,
    agent: String,
    generation: u64,
    marker: String,
}

impl Leftover {
    fn shown(&self, why: &str) -> Value {
        let name = process::one(self.pid).map(|p| p.name).unwrap_or_default();
        json!({"pid": self.pid, "command": name, "started": process::started_at(self.start), "agent": self.agent, "generation": self.generation, "why": why})
    }
}

impl App {
    /// The leftovers now, each with why it is one.
    pub(super) fn leftovers(&self) -> Result<Vec<(Leftover, &'static str)>, ApiError> {
        Ok(self.hosted_processes()?.into_iter().filter_map(|(l, why)| Some((l, why?))).collect())
    }

    /// Every process of this service's hosted agents now: each with why it is left over, or
    /// none while its launch runs. A scan that cannot list the processes is that error, never
    /// an empty list.
    fn hosted_processes(&self) -> Result<Vec<(Leftover, Option<&'static str>)>, ApiError> {
        let found = process::marked().ok_or_else(|| ApiError(axum::http::StatusCode::SERVICE_UNAVAILABLE, "this account's processes cannot be listed now".into()))?;
        let all = self.store.lock().unwrap().recorded()?;
        let mut tags = std::collections::HashMap::new();
        let mut out = vec![];
        for (pid, owned, marker) in found {
            let Some((agent, generation, tag)) = parse(&marker) else { continue };
            let ours = tags.entry(agent.to_string()).or_insert_with(|| self.hosts.tag(agent)).as_deref() == Some(tag);
            if !ours {
                // Another data directory's, or nobody's this service can prove: not listed.
                continue;
            }
            let why = self.why_left(&all, agent, generation);
            out.push((Leftover { pid, start: owned.start, agent: agent.into(), generation, marker }, why));
        }
        Ok(out)
    }

    /// Why a process of `agent`'s launch `generation` is left over, or none while that launch
    /// runs under its live host.
    fn why_left(&self, all: &core::State, agent: &str, generation: u64) -> Option<&'static str> {
        let Ok(a) = all.hosted_agent(agent) else { return Some("no start reserved it") };
        match &a.started {
            _ if a.closed.is_some() => Some("its agent is closed"),
            _ if a.ended.is_some() => Some("its agent's start ended"),
            None => None,
            Some(s) if s.generation != generation => Some("an older launch of its agent"),
            Some(_) if !self.hosts.alive(agent) => Some("its agent's host is gone"),
            Some(_) => None,
        }
    }

    /// Ends leftovers on the user's request: `pid`, or every one. Each is read again before
    /// every signal (the same start, this account's, the marker still there, still left
    /// over); TERM, then KILL for what is left, within one deadline. How each ended.
    pub(super) fn end_leftovers(&self, pid: Option<u32>) -> Result<Value, ApiError> {
        let chosen: Vec<Leftover> = self.leftovers()?.into_iter().map(|(l, _)| l).filter(|l| pid.is_none_or(|p| p == l.pid)).collect();
        if let (Some(p), true) = (pid, chosen.is_empty()) {
            return Err(bad(format!("process {p} is not a leftover of a hosted agent (`workriver agent leftovers` lists them)")));
        }
        let deadline = Instant::now() + Duration::from_secs(7);
        for (sig, wait) in [(libc::SIGTERM, Duration::from_secs(5)), (libc::SIGKILL, Duration::from_secs(2))] {
            for l in chosen.iter().filter(|l| state(l) == "alive") {
                // SAFETY: a positive pid addresses one process, read again just above.
                unsafe { libc::kill(l.pid as libc::pid_t, sig) };
            }
            let until = (Instant::now() + wait).min(deadline);
            // One dying may show no environment for a moment: waited for as one alive.
            while Instant::now() < until && chosen.iter().any(|l| state(l) != "exited") {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        Ok(json!({"processes": chosen.iter().map(|l| json!({"pid": l.pid, "agent": l.agent, "outcome": state(l)})).collect::<Vec<_>>()}))
    }
}

/// `exited`, `alive` (its start and marker as found) or `unverifiable`.
fn state(l: &Leftover) -> &'static str {
    match process::owned(l.pid) {
        Some(Owned { start, zombie: false }) if start == l.start => match process::marker(l.pid) {
            Ok(Some(m)) if m == l.marker => "alive",
            Ok(_) => "exited",
            Err(()) => "unverifiable",
        },
        _ => "exited",
    }
}

/// `a7/2/<tag>` as its agent, generation and tag.
fn parse(marker: &str) -> Option<(&str, u64, &str)> {
    let mut parts = marker.splitn(3, '/');
    let (agent, generation, tag) = (parts.next()?, parts.next()?.parse().ok()?, parts.next()?);
    Some((agent, generation, tag))
}

/// What the user is shown: each running agent's processes, and the leftovers with why.
pub(super) fn list_shown(app: &App) -> Result<Value, ApiError> {
    let all = app.hosted_processes()?;
    let (left, running): (Vec<_>, Vec<_>) = all.iter().partition(|(_, why)| why.is_some());
    Ok(json!({
        "leftover": left.iter().map(|(l, why)| l.shown(why.unwrap_or_default())).collect::<Vec<_>>(),
        "running": running.iter().map(|(l, _)| { let mut v = l.shown(""); v.as_object_mut().unwrap().remove("why"); v }).collect::<Vec<_>>(),
    }))
}
