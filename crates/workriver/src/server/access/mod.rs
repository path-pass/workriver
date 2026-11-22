// Who calls, and what they may do and see. Here: App::who (the agent at the place the
// calling process runs at, or the user), App::user, App::view (what a caller may read), and
// the checks under them. Beside it: the user's token and sessions (auth.rs), the process
// behind a request (caller.rs), where each session was first seen (pins.rs).
use axum::extract::State;
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::json;
use workriver_core::runtime::{Root, RuntimeAgent, RuntimeError};
use workriver_core::{self as core, Principal};

use crate::common::process::{self, Chain};
use crate::runtime::words;
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
    /// Only who: which workspace an agent is in, or manages, is in the records (its seat, its
    /// tasks), never in where the runtime shows its terminal.
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
            return Ok(Principal::Agent { session: self.session_of(name), machine: machine.map(str::to_string) });
        }
        let user = Principal::User;
        if caller.session {
            return Ok(user);
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
                None if machine.is_none() && caller.token => Ok(user),
                None => Err(ApiError(StatusCode::UNAUTHORIZED, words::no_agent_at(&root.place))),
            },
            None => match machine {
                Some(m) => Err(ApiError(StatusCode::UNAUTHORIZED, words::in_no_place_on(m, &process::show(&chain)))),
                None if !caller.token => Err(ApiError(StatusCode::UNAUTHORIZED, words::NOT_THE_USER.into())),
                // A helper of a harness, or a background job, has no terminal; the user at
                // a terminal window has one.
                None if self.allow_as || chain.first().is_some_and(|p| p.tty.is_some()) => Ok(user),
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
    ///
    /// A session that holds a manager's seat keeps its root past that: once the root is gone,
    /// another root that shows the session is the manager only where the old one was (the
    /// seat's machine and folder: it then has the seat back by itself, `back`); anywhere
    /// else it is refused until the user confirms the manager's new terminal.
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
        if let Some(pin) = pin.as_ref().filter(|p| self.runs(p)) {
            return Err(ApiError(StatusCode::FORBIDDEN, words::held(&a.session, &pin.place, pin.machine.as_deref())));
        }
        let seat = self.store.lock().unwrap().seat_of(&a.session)?;
        if let Some(m) = seat.as_ref().and_then(|w| w.manager.as_ref())
            && (m.machine.as_deref(), m.folder.as_str()) != (machine, a.cwd.as_str())
        {
            return Err(ApiError(StatusCode::FORBIDDEN, words::seat_elsewhere(&a.session, &root.place, machine, &a.cwd, m.machine.as_deref(), &m.folder)));
        }
        // First sight, or a seat taken back. On this machine every place is looked at: a
        // session nobody was seen with, shown at two places at once, is neither's; a runtime
        // that cannot say is refused too, and nothing is pinned. (On another machine a
        // worker's pin goes to the first place that calls; a seat is taken back only where
        // that machine's runtime shows the session at this one place.)
        if machine.is_none() || seat.is_some() {
            let all = self.runtime.agent_roots(machine).map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, words::cannot_ask(&root.place, &e.to_string())))?;
            if let Some(other) = all.iter().find(|r| r.place != root.place && r.at.agent.as_ref().is_some_and(|x| x.session == a.session)) {
                return Err(ApiError(StatusCode::FORBIDDEN, words::twice(&a.session, &root.place, &other.place)));
            }
        }
        if let Some(m) = mine {
            self.pins.change(vec![(a.session.clone(), Some(Pin { machine: machine.map(str::to_string), place: root.place.clone(), pid: m.pid, start: m.start }))]);
            // Pinned before, at a root that is gone: its seat is back with it.
            if let (Some(w), Some(_)) = (&seat, &pin) {
                self.back(&w.id, &a.session);
            }
        }
        Ok(())
    }

    /// A seated session has its seat back by itself: said as a line of its workspace's
    /// conversation, which is all that is recorded (the seat and its appointment did not
    /// change). The pin is already set; a record that fails is logged.
    fn back(&self, workspace: &str, session: &str) {
        let body = core::Body::ManagerBack { id: String::new(), workspace: workspace.into(), session: session.into() };
        let recorded = self.store.lock().unwrap().record(&Principal::Service, body);
        self.proposals.lock().unwrap().remove(workspace);
        match recorded {
            Ok(_) => {
                let _ = self.changed.send(());
            }
            Err(e) => tracing::warn!("cannot record that {session} has the seat of {workspace} back: {e}"),
        }
    }

    /// The regular read of rule 5: this machine's roots, and those of each other machine a
    /// manager is seated on (`see_roots_on`).
    pub(super) fn see_roots(&self) {
        let Ok(Some(all)) = got(self.store.lock().unwrap().recorded()) else { return };
        let mut machines: Vec<Option<String>> = vec![None];
        for m in all.workspaces.iter().filter(|w| !w.closed).filter_map(|w| w.manager.as_ref()?.machine.clone()) {
            if !machines.contains(&Some(m.clone())) {
                machines.push(Some(m));
            }
        }
        for m in machines {
            self.see_roots_on(m.as_deref());
        }
    }

    /// The regular read of rule 5 on one machine. On this one: a session shown at one place
    /// with a known shell, and pinned to no root that runs, is pinned there; pins whose root
    /// is gone are released. So a session is its place's before its agent ever calls. On
    /// another machine only seated sessions are looked at (a worker's pin there goes to the
    /// first place that calls). A session that holds a seat keeps its pin while its root is
    /// gone, and is pinned again only where the old root was (the seat's machine and folder):
    /// it then has its seat back (`back`). What the page proposes for a workspace whose
    /// manager does not run is worked out from the same read (`proposals`), for the seats on
    /// that machine; a machine that cannot be read leaves its seats' proposals as they were.
    fn see_roots_on(&self, machine: Option<&str>) {
        let Ok(roots) = self.runtime.agent_roots(machine) else { return };
        let Ok(Some(all)) = got(self.store.lock().unwrap().recorded()) else { return };
        let Ok(runner) = machine.map(|m| self.runner(Some(m))).transpose() else { return };
        let seat = |session: &str| all.seat_of(session).and_then(|w| Some((w.id.clone(), w.manager.clone()?)));
        let pins = self.pins.all();
        let gone: Vec<&String> = pins.iter().filter(|(_, p)| p.machine.as_deref() == machine && !self.runs(p)).map(|(s, _)| s).collect();
        let mut changes: Vec<(String, Option<Pin>)> = gone.iter().filter(|s| machine.is_none() && seat(s).is_none()).map(|s| ((*s).clone(), None)).collect();
        let mut back = vec![];
        let shown: Vec<(&Root, &RuntimeAgent)> = roots.iter().filter_map(|r| Some((r, r.at.agent.as_ref()?))).collect();
        for (root, a) in &shown {
            let session = a.session.as_str();
            let pinned = pins.iter().find(|(s, _)| s == session).map(|(_, p)| p);
            let free = pinned.is_none_or(|p| gone.contains(&&a.session) || !self.runs(p));
            if !free || shown.iter().filter(|(_, x)| x.session == session).count() != 1 {
                continue;
            }
            let seated = seat(session);
            if machine.is_some() && seated.is_none() || seated.as_ref().is_some_and(|(_, m)| m.machine.as_deref() != machine || m.folder != a.cwd) {
                continue;
            }
            if let Some(p) = root.shell.and_then(|pid| self.procs.chain(runner.as_ref(), pid).ok()?.into_iter().next()) {
                changes.retain(|(s, _)| s != session);
                changes.push((session.to_string(), Some(Pin { machine: machine.map(str::to_string), place: root.place.clone(), pid: p.pid, start: p.start })));
                if let (Some((w, _)), Some(_)) = (seated, pinned) {
                    back.push((w, session.to_string()));
                }
            }
        }
        self.pins.change(changes);
        for (w, session) in back {
            self.back(&w, &session);
        }
        // Each open workspace seated on this machine whose seated session is not at the root
        // it is pinned to (the root is gone, or it runs and shows another agent now), or is
        // shown at another root as well, on this machine or another (rule 5 refuses a caller
        // from either): its manager does not run, whatever terminal reports its session.
        // With whom to propose for the seat: the seated session where it shows now (another
        // folder, or beside its old root: the user confirms), else the agent now in the
        // terminal the old session was pinned to. A session never pinned (a place with no
        // known shell) has no root to have lost. The other machines' agents as last seen: for
        // this machine's seats, the service's last read of every machine; for another's,
        // read now (this machine's fresh). Not read, the proposals stay as they were.
        let everywhere = match machine {
            None => self.live(),
            Some(_) => self.runtime.agents().ok(),
        };
        let Some(everywhere) = everywhere else { return };
        let mut found = std::collections::HashMap::new();
        for w in all.workspaces.iter().filter(|w| !w.closed) {
            let Some(m) = w.manager.as_ref().filter(|m| m.machine.as_deref() == machine) else { continue };
            let Some(pin) = self.pins.get(&m.session) else { continue };
            let there = shown.iter().find(|(r, _)| r.place == pin.place).filter(|_| pin.machine.as_deref() == machine && self.runs(&pin));
            let beside = shown.iter().any(|(r, a)| a.session == m.session && r.place != pin.place);
            let elsewhere = everywhere.iter().any(|a| a.session == m.session && a.machine.as_deref() != machine);
            if there.is_some_and(|(_, a)| a.session == m.session) && !beside && !elsewhere {
                continue;
            }
            let moved = shown.iter().find(|(_, a)| a.session == m.session);
            let successor = || shown.iter().find(|(r, _)| r.place == pin.place);
            found.insert(w.id.clone(), moved.or_else(successor).map(|(_, a)| a.session.clone()));
        }
        // The proposals of the seats on other machines stay as their own read left them.
        let elsewhere = |id: &String| all.workspace(id).ok().filter(|w| !w.closed).and_then(|w| w.manager.as_ref()).is_some_and(|m| m.machine.as_deref() != machine);
        let changed = {
            let mut proposals = self.proposals.lock().unwrap();
            let before = proposals.clone();
            proposals.retain(|id, _| elsewhere(id));
            proposals.extend(found);
            *proposals != before
        };
        if changed {
            let _ = self.changed.send(());
        }
    }

    /// Whether `session` holds a seat and is not at the root it is pinned to alone: its
    /// terminal is gone (or holds another agent now), or another terminal reports the
    /// session too, here or on another machine; whichever terminal reports the session is
    /// not the manager until it has the seat back (`one_place`, `see_roots`) or the user
    /// confirms it. Nothing is typed into it meanwhile: what goes to the manager is not
    /// delivered, as to one that does not run. The seat's machine is looked at now, every
    /// time: the session may show again where it was, or somewhere else too since the
    /// service's tick, and another machine is not read on that tick alone.
    pub(in crate::server) fn seat_lost(&self, session: &str) -> bool {
        let Some(_) = self.pins.get(session) else { return false };
        let Ok(Some(Some(w))) = got(self.store.lock().unwrap().seat_of(session)) else { return false };
        let machine = w.manager.as_ref().and_then(|m| m.machine.clone());
        self.see_roots_on(machine.as_deref());
        self.pins.get(session).is_some_and(|pin| !self.runs(&pin)) || self.proposals.lock().unwrap().contains_key(&w.id)
    }

    /// Where a seated session is typed into: the root it is pinned to, on that root's
    /// machine, and nowhere else (`seat_lost` says whether it is still there alone). None:
    /// it holds no seat, or was never pinned (a place with no known shell), and goes by its
    /// session. Records that cannot be read say neither.
    pub(in crate::server) fn held_at(&self, session: &str) -> Result<Option<(Option<String>, String)>, Unread> {
        let seated = got(self.store.lock().unwrap().seat_of(session))?.flatten().is_some();
        Ok(self.pins.get(session).filter(|_| seated).map(|p| (p.machine, p.place)))
    }

    /// Where a session the user seats is, read now on its own machine (this one or another
    /// alike): the one live agent it names (by session, or by a name only one has), at the
    /// one root that machine's runtime shows it at. Refused, naming the places, when it is
    /// shown at none or at several, or at another root than the one it is pinned to while
    /// that one runs; 503 while that machine's runtime cannot be asked. The seat's folder is
    /// the working directory the runtime shows for it there.
    pub(in crate::server) fn seating(&self, agent: &str) -> Result<Seating, ApiError> {
        let cannot = |e: RuntimeError| ApiError(StatusCode::SERVICE_UNAVAILABLE, format!("cannot read the agents now: {e}"));
        let live = self.runtime.agents().map_err(cannot)?;
        let named: Vec<&RuntimeAgent> = match live.iter().filter(|a| a.session == agent).collect::<Vec<_>>() {
            exact if !exact.is_empty() => exact,
            _ => live.iter().filter(|a| a.name == agent).collect(),
        };
        let a = match named[..] {
            [one] => one,
            [] => return Err(bad(format!("no running agent {agent}: a manager is seated while it runs (`workriver agents` lists them)"))),
            _ => return Err(several(agent, &named)),
        };
        let machine = a.machine.as_deref();
        let roots = self.runtime.agent_roots(machine).map_err(cannot)?;
        let at: Vec<&Root> = roots.iter().filter(|r| r.at.agent.as_ref().is_some_and(|x| x.session == a.session)).collect();
        let root = match at[..] {
            [one] => one,
            [] => return Err(bad(format!("{} ({}) is shown in no terminal{} now: seat it once it runs in one", a.name, a.session, on(machine)))),
            _ => return Err(bad(format!("{} ({}) is shown in several terminals{} at once ({}): one session is one terminal; close the others first", a.name, a.session, on(machine), at.iter().map(|r| r.place.as_str()).collect::<Vec<_>>().join(", ")))),
        };
        let runner = machine.map(|m| self.runner(Some(m))).transpose()?;
        let shell = root.shell.and_then(|pid| self.procs.chain(runner.as_ref(), pid).ok()?.into_iter().next());
        let pin = self.pins.get(&a.session).filter(|p| self.runs(p));
        if let Some(p) = pin.filter(|p| shell.as_ref().is_none_or(|s| (p.machine.as_deref(), p.pid, p.start) != (machine, s.pid, s.start))) {
            return Err(bad(format!(
                "{} ({}) is shown in terminal {}{}, but its session was first seen in terminal {}{}, whose shell still runs: one session is one terminal; close one of them first",
                a.name,
                a.session,
                root.place,
                on(machine),
                p.place,
                on(p.machine.as_deref())
            )));
        }
        let folder = root.at.agent.as_ref().map(|x| x.cwd.clone()).unwrap_or_default();
        let pin = shell.map(|s| Pin { machine: machine.map(str::to_string), place: root.place.clone(), pid: s.pid, start: s.start });
        Ok(Seating { session: a.session.clone(), machine: a.machine.clone(), folder, pin })
    }

    /// A seating was recorded: its session is its root's from now on (a place where the
    /// runtime knows no shell has no root that stays, and is not pinned).
    pub(in crate::server) fn seated(&self, seat: Seating) {
        if let Some(pin) = seat.pin {
            self.pins.change(vec![(seat.session, Some(pin))]);
        }
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

    /// The records as the caller may see them: all of them for the user. An agent sees all
    /// but the conversations of workspaces it does not hold the seat of (`conversations_of`),
    /// except an agent on a blind task, which gets `State::blind_view` until it delivers.
    /// Nobody else reads: a request that is neither the user's nor from an agent's place is
    /// refused.
    pub(super) fn view(&self, caller: &Caller, r: &Reader) -> Result<core::State, ApiError> {
        let who = self.who(caller, r.named.as_deref())?;
        let all = self.store.lock().unwrap().recorded()?;
        Ok(match &who {
            Principal::Agent { session, .. } => all.blind_view(session).unwrap_or_else(|| all.into_state().conversations_of(session)),
            _ => all.into_state(),
        })
    }

    /// Who and where the CLI is: the agent at the place its process runs at (recorded by name
    /// the first time), its workspace by the records (the seat it holds, else the one
    /// workspace of the tasks it has) with that workspace's current intent, and the runtime's
    /// group of its place, which is only shown. At no place: nobody. Answered without the
    /// token, but not without a process.
    fn here(&self, caller: &Caller) -> Result<serde_json::Value, ApiError> {
        if caller.machine.is_none() && caller.chain.is_none() {
            return Err(ApiError(StatusCode::UNAUTHORIZED, words::NO_PROCESS.into()));
        }
        let chain = self.chain_of(caller)?;
        let Some(root) = self.place(caller, &chain)? else {
            return Ok(json!({ "actor": null, "name": null, "group": null, "workspace": null, "manager": false, "intent": null }));
        };
        let at = &root.at;
        let mut workspace = None;
        if let Some(a) = &at.agent {
            self.one_place(caller.machine.as_deref(), &root, a, &chain)?;
            self.seen(a)?;
            let all = self.store.lock().unwrap().recorded()?;
            workspace = all.workspace_of_agent(&a.session).map(|w| (w.id.clone(), w.name.clone(), w.current.clone(), w.manager.as_ref().is_some_and(|m| m.session == a.session)));
        }
        Ok(json!({
            "actor": at.agent.as_ref().map(|a| &a.session),
            "name": at.agent.as_ref().map(|a| &a.name),
            "group": at.group,
            "workspace": workspace.as_ref().map(|(id, name, ..)| json!({ "id": id, "name": name })),
            "manager": workspace.as_ref().is_some_and(|w| w.3),
            "intent": workspace.and_then(|w| w.2),
        }))
    }
}

/// " on machine m", or nothing for this one.
fn on(machine: Option<&str>) -> String {
    machine.map(|m| format!(" on machine {m}")).unwrap_or_default()
}

/// A session the user seats, as its machine's runtime shows it: its machine, the folder it
/// was started in, and the root it is pinned to once the seating is recorded.
pub(in crate::server) struct Seating {
    pub(in crate::server) session: String,
    pub(in crate::server) machine: Option<String>,
    pub(in crate::server) folder: String,
    pin: Option<Pin>,
}

/// The CLI asks who and where it is: see `App::here`. Nothing it sends is read for that.
pub(super) async fn here(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Json<serde_json::Value>, ApiError> {
    blocking(move || app.here(&caller).map(Json)).await
}

// ---- the web UI, built into the binary (read from disk in debug builds) ----
