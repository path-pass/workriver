//! Workers that stopped without delivering: a running task whose agent has sat idle (or
//! done) in the runtime for longer than the user's threshold, or is no longer there. Live
//! state, like an agent's wait in its terminal: worked out at each poll from the runtime and
//! the records, never recorded.
//!
//! The default threshold is ten minutes. In the records of real use, a task ran 1 to 46
//! minutes from start to delivery (median about 4) with its agent working throughout, and
//! the manager handed out the next task within a minute of a delivery; an agent that ends
//! its turn on a running task is waiting for something. Ten minutes is past the pause of a
//! worker waiting for its manager's short answer, and a fifth of the longest task: a
//! stopped worker is noticed long before its work would have been done.
use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use workriver_core::runtime::{AgentState, RuntimeAgent};
use workriver_core::{self as core, IntentState, State, Task, TaskKind, TaskState};

use crate::quote;

/// A running task whose agent stopped, as the page, the board and `task show` read it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Stall {
    /// Idle (or done) since then; gone: since it was first seen gone (or idle before that).
    pub since: String,
    /// The agent is no longer in the runtime.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gone: bool,
    /// When the agent itself was reminded of its task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reminded: Option<String>,
    /// The reminder did not help (or it is gone): the user (Needs you) and the manager are told.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub escalated: bool,
}

/// A task being watched: its stall once there is one to show, when the reminder went out (to
/// measure the wait), and since when its agent is missing from the runtime's reads.
#[derive(Clone, Default)]
pub struct Watch {
    pub stall: Option<Stall>,
    pub at: Option<Instant>,
    pub missing: Option<Instant>,
}

/// What a poll calls for about a task.
pub enum Act {
    /// Tell the agent itself, once, that its task is still running.
    Remind,
    /// Tell the manager (and show it in Needs you), once.
    Escalate,
}

/// One task to act on: where, its agent (a session; gone: as recorded) and the agent's name.
pub struct Found {
    pub owner: String,
    pub task: u32,
    pub agent: String,
    pub name: String,
    pub stall: Stall,
    pub act: Act,
}

type Key = (String, u32);

/// One poll's look at the running tasks of active intents. An agent idle (or done) on its
/// task for `threshold` is reminded of it, once; still idle half that again after the
/// reminder (and idle for that long), it is escalated: the reminder is typed into its
/// terminal, so an agent that can go on does within a minute or two, and the user hears of
/// a stopped worker at one and a half times the threshold. An agent missing from the reads
/// for `grace` (a minute) is gone and escalated: there is nobody to remind. Working again a
/// while after the reminder (not just answering it), delivering or dropping the task clears
/// it; a later stop starts over.
///
/// `idle`: since when each live agent (by session) has been idle or done; `read`: where the
/// runtime was read just now and listed agents (None: this machine). Anywhere else (a read that
/// failed or came back empty, a machine that cannot be reached) its agents are unknown, never
/// stopped or gone. `before`: the last poll's.
///
/// Not stopped: an agent waiting for the user on something it (or the manager) asked after
/// the task started (a gate open to the user, a question still open); one waiting in its
/// terminal (blocked: Needs you already shows it) or in no known state; gates, which are the
/// user's; work of a closed unit or an intent that is not active.
#[allow(clippy::too_many_arguments)]
pub fn step(state: &State, live: &[RuntimeAgent], idle: &HashMap<String, (String, Instant)>, threshold: Duration, grace: Duration, read: &[Option<String>], before: &HashMap<Key, Watch>) -> (HashMap<Key, Watch>, Vec<Found>) {
    let again = threshold / 2;
    let away = |m: Option<&str>| !read.iter().any(|d| d.as_deref() == m);
    let (mut next, mut acts) = (HashMap::new(), vec![]);
    for i in state.intents.iter().filter(|i| i.state == IntentState::Active) {
        let manager = state.manager_of(&i.id);
        let units = state.units.iter().filter(|u| u.intent == i.id && u.merged.is_none() && u.done.is_none());
        // Where the work runs: the unit's machine, or the intent's workspace's.
        let owners = std::iter::once((&i.id, &i.tasks, i.workspace.as_ref().and_then(|w| w.machine.as_deref()))).chain(units.map(|u| (&u.id, &u.tasks, u.machine.as_deref())));
        for (owner, tasks, machine) in owners {
            for t in tasks.iter().filter(|t| t.state == TaskState::Running && t.kind != TaskKind::Gate) {
                let Some(a) = t.agent.as_deref() else { continue };
                let key = (owner.clone(), t.n);
                let prev = before.get(&key);
                // Seen (or not known) now: no longer missing.
                let kept = || prev.filter(|w| w.stall.is_some()).map(|w| Watch { missing: None, ..w.clone() });
                let mut found = |agent: &str, name: &str, stall: &Stall, act: Act| acts.push(Found { owner: owner.clone(), task: t.n, agent: agent.into(), name: name.into(), stall: stall.clone(), act });
                let watch = match live.iter().find(|x| x.session == a || x.name == a) {
                    // Unknown: whatever was found before stays as it was.
                    None if away(machine) => kept(),
                    None if waits_on_user(state, owner, t, &[a], manager) => None,
                    None => {
                        let mut w = prev.cloned().unwrap_or_default();
                        let missing = *w.missing.get_or_insert_with(Instant::now);
                        if missing.elapsed() >= grace && !w.stall.as_ref().is_some_and(|s| s.escalated) {
                            let had = w.stall.take();
                            let since = had.as_ref().map(|s| s.since.clone()).unwrap_or_else(core::now);
                            let stall = Stall { since, gone: true, reminded: had.and_then(|s| s.reminded), escalated: true };
                            found(a, state.name_of(a).unwrap_or(a), &stall, Act::Escalate);
                            w.stall = Some(stall);
                        }
                        Some(w)
                    }
                    Some(x) if away(x.machine.as_deref()) => kept(),
                    // Working: answering the reminder keeps it; working on past that clears it.
                    Some(x) if x.state == AgentState::Working => kept().filter(|w| w.at.is_some_and(|at| at.elapsed() < again)),
                    Some(x) if !matches!(x.state, AgentState::Idle | AgentState::Done) => kept(),
                    Some(x) => match (idle.get(&x.session), kept()) {
                        (None, _) => None,
                        _ if waits_on_user(state, owner, t, &[a, &x.session], manager) => None,
                        (Some(_), Some(w)) if w.stall.as_ref().is_some_and(|s| s.escalated) => Some(Watch { stall: w.stall.map(|s| Stall { gone: false, ..s }), ..w }),
                        (Some((_, idle_at)), Some(w)) if w.at.is_some_and(|at| at.elapsed() >= again) && idle_at.elapsed() >= again => {
                            let stall = Stall { gone: false, escalated: true, ..w.stall.clone().unwrap() };
                            found(&x.session, &x.name, &stall, Act::Escalate);
                            Some(Watch { stall: Some(stall), ..w })
                        }
                        (Some(_), Some(w)) => Some(w),
                        (Some((since, idle_at)), None) if idle_at.elapsed() >= threshold => {
                            let stall = Stall { since: since.clone(), gone: false, reminded: Some(core::now()), escalated: false };
                            found(&x.session, &x.name, &stall, Act::Remind);
                            Some(Watch { stall: Some(stall), at: Some(Instant::now()), missing: None })
                        }
                        (Some(_), None) => None,
                    },
                };
                if let Some(w) = watch {
                    next.insert(key, w);
                }
            }
        }
    }
    (next, acts)
}

/// The agent (`who`: as recorded, and its session) asked the user something after its task
/// started that is still open: a gate the user sees (asked by it or by the manager for it),
/// or a question.
fn waits_on_user(state: &State, owner: &str, t: &Task, who: &[&str], manager: Option<&str>) -> bool {
    let started = t.started.as_deref().unwrap_or_default();
    let asked = state.questions.iter().any(|q| q.open() && who.contains(&q.by.as_str()));
    let gate = state.tasks_of(owner).into_iter().flatten().any(|g| {
        g.kind == TaskKind::Gate && g.state == TaskState::Waiting && g.at.as_str() >= started && (who.contains(&g.by.as_str()) || Some(g.by.as_str()) == manager)
    });
    asked || gate
}

/// The message a step calls for: to the agent itself (a reminder), or to the manager.
pub fn message(state: &State, f: &Found) -> Option<String> {
    let t = state.task(&f.owner, f.task).ok()?;
    let what = crate::deliver::label(state, &f.owner, t);
    let r = format!("{}#{}", f.owner, f.task);
    let name = quote::name(&f.name);
    let minutes = humantime::parse_rfc3339(&f.stall.since).ok().and_then(|s| s.elapsed().ok()).map_or(0, |d| d.as_secs() / 60);
    if matches!(f.act, Act::Remind) {
        // Only its own task: nothing of other work, so a blind task is told no more than it may know.
        return Some(format!(
            "[workriver] Your task {what} is still running and nothing was delivered; you have been idle for {minutes} min. If it is finished, deliver it (`workriver task deliver {r} ...`). If something stopped you (a command was refused, e.g. a hook forbids push; a permission is missing), deliver with what you did and the refusal in `--note`, or raise it: `workriver chore add --title \"...\"` for the user, `workriver ask <task> \"...\"` about another task. Do not just wait. Details: `workriver task show {r}`."
        ));
    }
    let reminded = f.stall.reminded.as_ref().map(|at| format!(" It was reminded of its task at {at} and did not go on.")).unwrap_or_default();
    let rest = format!(
        "hand the task to another agent (it goes on with {r} as it is, without `task start`, and delivers it), or drop it (`workriver task drop {r} --reason \"...\"`). How: `workriver guide how stalled`. Details: `workriver task show {r}`."
    );
    Some(if f.stall.gone {
        format!("[workriver] {name}, the agent on {what}, is gone (no longer in the agent runtime) while the task is still running and not delivered.{reminded} Find out what it did (`workriver task show {r}`, the branch), then {rest}")
    } else {
        format!("[workriver] {name} stopped {minutes} min ago without delivering {what}: it is idle in its terminal while the task is still running.{reminded} {} {rest}", crate::herdr::stall_hint(&name))
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use workriver_core::runtime::{AgentState, RuntimeAgent};
    use workriver_core::{Principal, Store};

    use super::*;

    fn agent(session: &str, state: AgentState, machine: Option<&str>) -> RuntimeAgent {
        RuntimeAgent { session: session.into(), harness: "claude".into(), state, cwd: String::new(), group: None, name: session.into(), topic: None, machine: machine.map(str::to_string) }
    }

    /// An agent on a machine that cannot be reached is unknown, never stopped: neither when it
    /// is listed as last seen, nor when it is not listed at all.
    #[test]
    fn a_disconnected_machine_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(&dir.path().join("w.db")).unwrap();
        let mut at = |who: Principal, v: serde_json::Value| s.append(&who, serde_json::from_value(v).unwrap()).unwrap();
        at(Principal::User, json!({"kind": "intent.created", "body": {"title": "Chat"}}));
        at(Principal::User, json!({"kind": "unit.created", "body": {"intent": "i1", "title": "Far", "repo": "/r", "branch": "feat/b", "base": "main", "machine": "box"}}));
        at(Principal::User, json!({"kind": "task.created", "body": {"unit": "u1", "kind": "implement", "title": "x"}}));
        at(Principal::Agent { session: "w".into(), machine: Some("box".into()) }, json!({"kind": "task.started", "body": {"unit": "u1", "task": 1}}));
        let state = s.state().clone();
        let long_ago = Instant::now() - Duration::from_secs(3600);
        let idle: HashMap<String, (String, Instant)> = [("w".to_string(), ("2026-11-17T00:00:00Z".to_string(), long_ago))].into();
        let none = HashMap::new();
        let live = [agent("w", AgentState::Idle, Some("box"))];
        let here = agent("x", AgentState::Working, None);
        let minute = Duration::from_secs(60);
        let grace = Duration::from_millis(300);
        let read = |r: &[Option<&str>]| r.iter().map(|m| m.map(str::to_string)).collect::<Vec<_>>();
        assert!(step(&state, &live, &idle, minute, grace, &read(&[None]), &none).1.is_empty(), "listed, on a machine that is down");
        assert!(step(&state, &[], &idle, minute, grace, &read(&[None]), &none).1.is_empty(), "not listed, its unit's machine down");
        // Read: it is reminded.
        let (_, found) = step(&state, &live, &idle, minute, grace, &read(&[Some("box")]), &none);
        assert!(matches!(found[..], [Found { act: Act::Remind, .. }]));
        // Missing from a good read: nothing yet; still missing past the grace: gone, escalated once.
        let ok = read(&[None, Some("box")]);
        let (w, found) = step(&state, std::slice::from_ref(&here), &idle, minute, grace, &ok, &none);
        assert!(found.is_empty() && w.values().all(|w| w.stall.is_none()));
        // A failed read in between keeps nothing missing.
        let (w, _) = step(&state, &[], &idle, minute, grace, &[], &w);
        assert!(w.is_empty());
        let (w, found) = step(&state, std::slice::from_ref(&here), &idle, minute, grace, &ok, &w);
        assert!(found.is_empty());
        std::thread::sleep(grace);
        let (w, found) = step(&state, std::slice::from_ref(&here), &idle, minute, grace, &ok, &w);
        assert!(matches!(found[..], [Found { act: Act::Escalate, stall: Stall { gone: true, .. }, .. }]));
        let (_, found) = step(&state, std::slice::from_ref(&here), &idle, minute, grace, &ok, &w);
        assert!(found.is_empty(), "once");
    }
}
