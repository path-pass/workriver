// Who calls, and what they may do and see. Here: App::who (the agent at the place the
// calling process runs at, or the user), App::user, App::view (what a caller may read), and
// the checks under them. Beside it: the user's token and sessions (auth.rs), the process
// behind a request (caller.rs), where each session was first seen (pins.rs).
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::json;
use workriver_core::runtime::{Root, RuntimeAgent, RuntimeError};
use workriver_core::{self as core, Principal};

use crate::runtime::words;
use crate::common::process::{self, Chain};
use crate::server::app::*;
use crate::server::http::*;

pub(super) mod auth;
pub(super) mod caller;
pub(super) mod pins;

use pins::Pin;

#[cfg(test)]
mod tests;

/// What a request was found to be, before any handler sees it.
#[derive(Clone)]
pub(super) struct Caller {
    /// The followed machine whose listener it came in on; none: this machine's.
    pub(super) machine: Option<String>,
    /// It carries the user token. Never on a machine's listener: a caller on another machine
    /// is never the user. The token alone makes nobody the user: its process does (`chain`).
    pub(super) token: bool,
    /// It carries a session of the page from our own origin: the user in a browser, which
    /// has no process to check and needs none.
    pub(super) session: bool,
    /// The place the CLI says it runs at (`x-workriver-place`, the one carrier of it): only
    /// cross-checked against where its process is.
    pub(super) place: Option<String>,
    /// The process that sends it, with its parents, as read when it connected to the caller
    /// socket (this machine's listener only).
    pub(super) chain: Option<Chain>,
    /// On a machine's listener: the process id the CLI gives (its word; checked over there).
    pub(super) pid: Option<u32>,
}

/// Who reads is found as for what is recorded (`App::who`); a reader names nothing but,
/// where the service allows it, `--as`.
#[derive(Deserialize)]
pub(super) struct Reader {
    #[serde(default, rename = "as")]
    pub(super) named: Option<String>,
    /// Without file texts, uncommitted work or trees.
    #[serde(default)]
    pub(super) light: bool,
    /// A unit's code: the commits (short hashes, comma-separated) whose texts the reader
    /// has already; they never change.
    #[serde(default)]
    pub(super) have: String,
}

impl App {
    /// Who a request acts as (docs/design/caller-identity.md). Never from what it says: the
    /// agent is the one at the place the calling process really runs at; the user is a caller
    /// with the token at a place with no agent, or at no place at a terminal, or the page's session.
    /// Every failure is a refusal; none falls through to the user or to another agent.
    /// The one exception, where the daemon was started to allow it: an agent named with `--as`.
    pub(super) fn who(&self, caller: &Caller, named: Option<&str>) -> Result<Principal, ApiError> {
        let machine = caller.machine.as_deref();
        if let Some(name) = named.filter(|n| !n.is_empty()) {
            if !self.allow_as {
                return Err(ApiError(StatusCode::FORBIDDEN, words::AS_OFF.into()));
            }
            if matches!(name, "user" | "workriver") {
                return Err(ApiError(StatusCode::FORBIDDEN, format!("nobody records as {name:?} by saying so")));
            }
            if machine.is_none() && caller.chain.is_none() {
                return Err(self.no_process(caller));
            }
            return Ok(Principal::Agent { session: self.session_of(name, None), machine: machine.map(str::to_string) });
        }
        if caller.session {
            return Ok(Principal::User);
        }
        let chain = self.chain_of(caller)?;
        match self.place(caller, &chain)? {
            Some(root) => match &root.at.agent {
                Some(a) => {
                    self.one_place(machine, &root, a, &chain)?;
                    self.seen(a)?;
                    self.tell_of_update(&a.session, false);
                    Ok(Principal::Agent { session: a.session.clone(), machine: a.machine.clone() })
                }
                None if machine.is_none() && caller.token => Ok(Principal::User),
                None => Err(ApiError(StatusCode::UNAUTHORIZED, words::no_agent_at(&root.place))),
            },
            None => match machine {
                Some(m) => Err(ApiError(StatusCode::UNAUTHORIZED, words::in_no_place_on(m, &process::show(&chain)))),
                None if !caller.token => Err(ApiError(StatusCode::UNAUTHORIZED, words::NOT_THE_USER.into())),
                // A helper of a harness, or a background job, has no terminal; the user at
                // a terminal window has one.
                None if self.allow_as || chain.first().is_some_and(|p| p.tty.is_some()) => Ok(Principal::User),
                None => Err(ApiError(StatusCode::FORBIDDEN, words::no_terminal(&process::show(&chain)))),
            },
        }
    }

    /// What a request on this machine that shows no process is told.
    fn no_process(&self, caller: &Caller) -> ApiError {
        ApiError(StatusCode::UNAUTHORIZED, if caller.token { words::NO_PROCESS.into() } else { words::NOT_THE_USER.into() })
    }

    /// The calling process and its parents. This machine: as read when it connected to the
    /// caller socket. Another machine: read there now, at every request, for the process id
    /// the CLI gives; that is the caller's word, so the process must be running and be the CLI.
    fn chain_of(&self, caller: &Caller) -> Result<Chain, ApiError> {
        let Some(m) = caller.machine.as_deref() else {
            return caller.chain.clone().ok_or_else(|| self.no_process(caller));
        };
        let refuse = |pid: Option<u32>, why: &str| ApiError(StatusCode::UNAUTHORIZED, words::not_the_cli(m, pid, why));
        let pid = caller.pid.ok_or_else(|| refuse(None, "is not given"))?;
        let chain = self.procs.chain(Some(&self.runner(Some(m))?), pid).map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, format!("cannot read process {pid} on machine {m}: {e}")))?;
        match chain.first() {
            None => Err(refuse(Some(pid), "is gone")),
            Some(p) if p.name != "workriver" => Err(refuse(Some(pid), "is not the `workriver` CLI")),
            Some(_) => Ok(chain),
        }
    }

    /// The place the calling process is at, if any (rule 1): a request that names another
    /// place than the one its process runs at is refused, with both said.
    fn place(&self, caller: &Caller, chain: &Chain) -> Result<Option<Root>, ApiError> {
        let machine = caller.machine.as_deref();
        let named = caller.place.as_deref();
        let cannot = |e: RuntimeError| ApiError(StatusCode::SERVICE_UNAVAILABLE, words::cannot_ask(named.unwrap_or("?"), &e.to_string()));
        // The usual case costs one look: the process descends from the root of the place it names.
        if let Some(p) = named {
            let there = self.runtime.roots(machine, Some(p)).map_err(cannot)?;
            if let Some(r) = caller::place_of(chain, &there, &|_| None) {
                return Ok(Some(r.clone()));
            }
        }
        let all = match self.runtime.roots(machine, None) {
            Ok(all) => all,
            // No runtime at all: there are no places to be at.
            Err(RuntimeError::None) if named.is_none() => vec![],
            Err(e) => return Err(cannot(e)),
        };
        // A job left behind keeps its place's terminal: looked for on this machine only.
        // Another machine has rules 1, 2 and 5 (each root's terminal would be one run there).
        let tty_of = |pid: u32| if machine.is_some() { None } else { self.procs.chain(None, pid).ok()?.first()?.tty.clone() };
        let found = caller::place_of(chain, &all, &tty_of).cloned();
        match named {
            Some(p) if found.as_ref().is_none_or(|r| r.place != p) => {
                let gone = !all.iter().any(|r| r.place == p);
                Err(ApiError(StatusCode::FORBIDDEN, words::elsewhere(p, gone, &process::show(chain), found.as_ref().map(|r| r.place.as_str()), machine)))
            }
            _ => Ok(found),
        }
    }

    /// Whether a pinned root still runs: its process is there and started when it did. One
    /// that cannot be asked (its machine is not reached) is taken to run.
    fn runs(&self, pin: &Pin) -> bool {
        let Ok(runner) = pin.machine.as_deref().map(|m| self.runner(Some(m))).transpose() else { return true };
        match self.procs.chain(runner.as_ref(), pin.pid) {
            Ok(c) => c.first().is_some_and(|p| p.start == pin.start),
            Err(_) => true,
        }
    }

    /// Rule 5, one session, one place: which agent a place holds is what the runtime was told,
    /// and anyone may tell it. So a session is its first place's while that place's root runs:
    /// the same session from another root is refused. `chain`: the caller's, which holds the
    /// root's own line.
    fn one_place(&self, machine: Option<&str>, root: &Root, a: &RuntimeAgent, chain: &Chain) -> Result<(), ApiError> {
        // A place where the runtime knows no shell has no root that stays: it is not pinned.
        let mine = root.shell.and_then(|pid| chain.iter().find(|p| p.pid == pid).cloned().or_else(|| {
            let runner = machine.map(|m| self.runner(Some(m))).transpose().ok()?;
            self.procs.chain(runner.as_ref(), pid).ok()?.into_iter().next()
        }));
        let pin = self.pins.get(&a.session);
        if let (Some(pin), Some(m)) = (&pin, &mine)
            && (pin.machine.as_deref(), pin.pid, pin.start) == (machine, m.pid, m.start)
        {
            return Ok(());
        }
        if let Some(pin) = pin.filter(|p| self.runs(p)) {
            return Err(ApiError(StatusCode::FORBIDDEN, words::held(&a.session, &pin.place, pin.machine.as_deref())));
        }
        // First sight. On this machine every place is looked at: a session nobody was seen
        // with, shown at two places at once, is neither's; a runtime that cannot say is
        // refused too, and nothing is pinned. (On another machine the pin goes to the first
        // place that calls.)
        if machine.is_none() {
            let all = self.runtime.agent_roots(None).map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, words::cannot_ask(&root.place, &e.to_string())))?;
            if let Some(other) = all.iter().find(|r| r.place != root.place && r.at.agent.as_ref().is_some_and(|x| x.session == a.session)) {
                return Err(ApiError(StatusCode::FORBIDDEN, words::twice(&a.session, &root.place, &other.place)));
            }
        }
        if let Some(m) = mine {
            self.pins.change(vec![(a.session.clone(), Some(Pin { machine: machine.map(str::to_string), place: root.place.clone(), pid: m.pid, start: m.start }))]);
        }
        Ok(())
    }

    /// The regular read of rule 5, on this machine: a session shown at one place with a known
    /// shell, and pinned to no root that runs, is pinned there; pins whose root is gone are
    /// released. So a session is its place's before its agent ever calls.
    pub(super) fn see_roots(&self) {
        let Ok(roots) = self.runtime.agent_roots(None) else { return };
        let mut changes: Vec<(String, Option<Pin>)> = self.pins.all().into_iter().filter(|(_, p)| p.machine.is_none() && !self.runs(p)).map(|(s, _)| (s, None)).collect();
        let shown: Vec<(&Root, &str)> = roots.iter().filter_map(|r| Some((r, r.at.agent.as_ref()?.session.as_str()))).collect();
        for (root, session) in &shown {
            let free = self.pins.get(session).is_none_or(|p| changes.iter().any(|(s, _)| s == session) || !self.runs(&p));
            if !free || shown.iter().filter(|(_, s)| s == session).count() != 1 {
                continue;
            }
            if let Some(p) = root.shell.and_then(|pid| self.procs.chain(None, pid).ok()?.into_iter().next()) {
                changes.retain(|(s, _)| s != session);
                changes.push((session.to_string(), Some(Pin { machine: None, place: root.place.clone(), pid: p.pid, start: p.start })));
            }
        }
        self.pins.change(changes);
    }

    /// The user, or a refusal that says how to be the user.
    pub(super) fn user(&self, caller: &Caller) -> Result<Principal, ApiError> {
        if let Some(m) = &caller.machine {
            return Err(ApiError(StatusCode::FORBIDDEN, format!("a caller on machine {m} is an agent of that machine, never the user")));
        }
        match self.who(caller, None)? {
            Principal::User => Ok(Principal::User),
            // The token file can be read at an agent's place on this machine; what runs at
            // an agent's place is the agent's doing.
            _ => Err(ApiError(StatusCode::FORBIDDEN, words::ONLY_THE_USER.into())),
        }
    }

    /// The records as the caller may see them: all of them for the user and for an agent,
    /// except an agent on a blind task, which gets `State::blind_view` until it delivers.
    /// Nobody else reads: a request that is neither the user's nor from an agent's place is
    /// refused.
    pub(super) fn view(&self, caller: &Caller, r: &Reader) -> Result<core::State, ApiError> {
        let who = self.who(caller, r.named.as_deref())?;
        let store = self.store.lock().unwrap();
        Ok(match &who {
            Principal::Agent { session, .. } => store.state().blind_view(session).unwrap_or_else(|| store.state().clone()),
            _ => store.state().clone(),
        })
    }

    /// Who and where the CLI is: the agent at the place its process runs at (recorded by name
    /// the first time), that place's group, and the group's current intent, if any. At no
    /// place: nobody, in no group. Answered without the token, but not without a process.
    pub(super) fn here(&self, caller: &Caller) -> Result<serde_json::Value, ApiError> {
        if caller.machine.is_none() && caller.chain.is_none() {
            return Err(ApiError(StatusCode::UNAUTHORIZED, words::NO_PROCESS.into()));
        }
        let chain = self.chain_of(caller)?;
        let Some(root) = self.place(caller, &chain)? else {
            return Ok(json!({ "actor": null, "name": null, "group": null, "intent": null }));
        };
        let at = &root.at;
        if let Some(a) = &at.agent {
            self.one_place(caller.machine.as_deref(), &root, a, &chain)?;
            self.seen(a)?;
        }
        let intent = at.group.as_ref().and_then(|g| self.store.lock().unwrap().state().intent_for(g).map(|i| i.id.clone()));
        Ok(json!({ "actor": at.agent.as_ref().map(|a| &a.session), "name": at.agent.as_ref().map(|a| &a.name), "group": at.group, "intent": intent }))
    }
}
