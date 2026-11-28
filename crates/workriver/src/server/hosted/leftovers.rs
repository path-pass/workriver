// Processes a hosted agent left behind: this account's processes whose marker names one of
// this service's instances (by its tag) that no running launch holds: its agent closed, its
// start ended, an older generation, or a host that is gone. Listed after initial attachment;
// ended only on the user's request, one or all, each read again before every signal. Nothing
// ends them by itself, after a restart neither. Only the marker is read of an environment.
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use workriver_core::hosted::{HostedAgent, ProcessOutcome};

use crate::common::process;
use crate::server::app::*;
use crate::server::http::*;

/// The identity taken from the list when the user selects one leftover.
#[derive(PartialEq, Eq)]
pub(super) struct Selection {
    pub pid: u32,
    pub start: u64,
    pub marker: String,
}

struct Leftover {
    selection: Selection,
    agent: String,
    generation: u64,
}

impl Leftover {
    fn shown(&self, why: &str) -> Value {
        let Selection { pid, start, marker } = &self.selection;
        let name = process::one(*pid).filter(|p| p.start == *start).map(|p| p.name).unwrap_or_default();
        json!({"pid": pid, "start": start, "marker": marker, "command": name, "started": process::started_at(*start), "agent": self.agent, "generation": self.generation, "why": why})
    }

    fn state(&self) -> ProcessOutcome {
        let Selection { pid, start, marker } = &self.selection;
        process::marked_state(*pid, *start, marker)
    }
}

impl App {
    /// The leftovers now, each with why it is one.
    fn leftovers(&self) -> Result<Vec<(Leftover, &'static str)>, ApiError> {
        Ok(self.hosted_processes()?.into_iter().filter_map(|(l, why)| Some((l, why?))).collect())
    }

    /// Every process of this service's hosted agents now: each with why it is left over, or
    /// none while its launch runs. A scan that cannot list the processes is that error, never
    /// an empty list.
    fn hosted_processes(&self) -> Result<Vec<(Leftover, Option<&'static str>)>, ApiError> {
        if !self.hosted_reconciled() {
            return Err(ApiError(axum::http::StatusCode::SERVICE_UNAVAILABLE, "the service is still re-attaching hosted agents".into()));
        }
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
            let why = why_left(all.hosted_agent(agent).ok(), generation);
            out.push((Leftover { agent: agent.into(), generation, selection: Selection { pid, start: owned.start, marker } }, why));
        }
        Ok(out)
    }

    /// Ends one selected leftover or all of them on the user's request. Checks the identity
    /// and whether its launch still runs before every signal, within one TERM/KILL deadline.
    pub(super) fn end_leftovers(&self, selected: Option<Selection>) -> Result<Value, ApiError> {
        let deadline = Instant::now() + Duration::from_secs(7);
        let chosen: Vec<Leftover> = self.leftovers()?.into_iter().map(|(l, _)| l).filter(|l| selected.as_ref().is_none_or(|p| *p == l.selection)).collect();
        if let (Some(p), true) = (selected, chosen.is_empty()) {
            return Err(bad(format!("process {} no longer matches the listed process (`workriver agent processes` lists leftovers)", p.pid)));
        }
        for (sig, wait) in [(libc::SIGTERM, Duration::from_secs(5)), (libc::SIGKILL, Duration::from_secs(2))] {
            for l in &chosen {
                if Instant::now() >= deadline {
                    break;
                }
                let store = self.store.lock().unwrap();
                let current = store.hosted_agent(&l.agent)?;
                let Selection { pid, start, marker } = &l.selection;
                if why_left(current.as_ref(), l.generation).is_some()
                    && let Err(e) = process::signal_marked(*pid, *start, marker, sig)
                {
                    tracing::warn!("signalling leftover process {pid} ({sig}): {e}");
                }
            }
            let until = (Instant::now() + wait).min(deadline);
            // One dying may show no environment for a moment: waited for as one alive.
            while Instant::now() < until && chosen.iter().any(|l| l.state() != ProcessOutcome::Exited) {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        Ok(json!({"processes": chosen.iter().map(|l| json!({"pid": l.selection.pid, "agent": l.agent, "outcome": l.state()})).collect::<Vec<_>>()}))
    }
}

/// Why this launch is left over. The recorded host identity is trusted even while its
/// transport is unavailable or not connected yet.
fn why_left(a: Option<&HostedAgent>, generation: u64) -> Option<&'static str> {
    let Some(a) = a else { return Some("no start reserved it") };
    match &a.started {
        _ if a.closed.is_some() => Some("its agent is closed"),
        _ if a.ended.is_some() => Some("its agent's start ended"),
        None => None,
        Some(s) if s.generation != generation => Some("an older launch of its agent"),
        Some(s) if process::owned_running(s.host.pid, s.host.start) == Some(false) => Some("its agent's host is gone"),
        Some(_) => None,
    }
}

/// `a7/2/<tag>` as its agent, generation and tag.
fn parse(marker: &str) -> Option<(&str, u64, &str)> {
    let mut parts = marker.splitn(3, '/');
    let (agent, generation, tag) = (parts.next()?, parts.next()?.parse().ok()?, parts.next()?);
    let number = agent.strip_prefix('a')?;
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
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
