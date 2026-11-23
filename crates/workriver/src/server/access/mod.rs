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
        // Found lost before, back now: where it is, the next look reads.
        self.seats.lock().unwrap().remove(session);
        match recorded {
            Ok(_) => {
                let _ = self.changed.send(());
            }
            Err(e) => tracing::warn!("cannot record that {session} has the seat of {workspace} back: {e}"),
        }
    }

    /// The regular read of rule 5: this machine's roots, and those of each other machine a
    /// manager is seated on (`see_roots_on`), against `agents`, the runtime's list the poll
    /// read just before. Not read (that list, or the records): no seat's place is known.
    pub(super) fn see_roots(&self, agents: Option<&[RuntimeAgent]>) {
        let read = got(self.store.lock().unwrap().recorded());
        let (Some(agents), Ok(Some(all))) = (agents, read) else {
            self.seats.lock().unwrap().clear();
            return;
        };
        let mut machines: Vec<Option<String>> = vec![None];
        for m in all.workspaces.iter().filter(|w| !w.closed).filter_map(|w| w.manager.as_ref()?.machine.clone()) {
            if !machines.contains(&Some(m.clone())) {
                machines.push(Some(m));
            }
        }
        drop(all);
        // A machine no seat is on any more is not read: what was found there goes.
        self.seats.lock().unwrap().retain(|_, (m, _)| machines.contains(m));
        for m in machines {
            if let Err(e) = self.see_roots_on(m.as_deref(), agents).map(drop) {
                tracing::debug!("cannot read the roots{}: {e}", on(m.as_deref()));
            }
        }
    }

    /// The regular read of rule 5 on one machine. On this one: a session shown at one place
    /// with a known shell, and pinned to no root that runs, is pinned there; pins whose root
    /// is gone are released. So a session is its place's before its agent ever calls. On
    /// another machine only seated sessions are looked at (a worker's pin there goes to the
    /// first place that calls). A session that holds a seat keeps its pin while its root is
    /// gone, and is pinned again only where the old root was (the seat's machine and folder):
    /// it then has its seat back (`back`). Where each seat on that machine is (`App::seats`)
    /// and what the page proposes for one that is lost (`proposals`) are worked out from the
    /// same read. `agents`: the runtime's list, read now by the caller, for whether another
    /// machine lists a seated session. Everything is read before anything changes: roots,
    /// records or the machine's runner not read, that machine's seats are not known
    /// (`seat_place` answers so) and its pins and proposals stay as they were.
    fn see_roots_on(&self, machine: Option<&str>, agents: &[RuntimeAgent]) -> Result<Vec<(String, Option<String>)>, RuntimeError> {
        let read = (|| {
            let roots = self.runtime.agent_roots(machine)?;
            let read = got(self.store.lock().unwrap().recorded());
            let all = read.map_err(|Unread(why)| RuntimeError::Unreachable(why))?.ok_or_else(|| RuntimeError::Unreachable("the records cannot be read now".into()))?;
            let runner = machine.map(|m| self.runner(Some(m))).transpose().map_err(|e| RuntimeError::Unreachable(e.1))?;
            Ok((roots, all, runner))
        })();
        let (roots, all, runner) = match read {
            Ok(r) => r,
            Err(e) => {
                self.forget_seats(machine);
                return Err(e);
            }
        };
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
        // Where each open seat on this machine is, from this read alone. Pinned: at the root it
        // is pinned to, while that root runs, shows the session and no other root here shows
        // it. With no pin (no shell was known at its seating): at the one root here that shows
        // the session, in the seat's folder. Either way only while no other machine lists the
        // session (rule 5 refuses a caller from either). Anywhere else it is lost: its manager
        // does not run, whatever terminal reports its session. With whom to propose for a lost
        // seat: the seated session where it shows now (another folder, or beside its old root:
        // the user confirms), else the agent now in the terminal it was pinned to.
        let mut places = vec![];
        let mut found = std::collections::HashMap::new();
        for w in all.workspaces.iter().filter(|w| !w.closed) {
            let Some(m) = w.manager.as_ref().filter(|m| m.machine.as_deref() == machine) else { continue };
            let at: Vec<&(&Root, &RuntimeAgent)> = shown.iter().filter(|(_, a)| a.session == m.session).collect();
            let elsewhere = agents.iter().any(|a| a.session == m.session && a.machine.as_deref() != machine);
            let pin = self.pins.get(&m.session);
            let place = match (&pin, at.as_slice()) {
                _ if elsewhere => None,
                (Some(pin), [(r, _)]) => (pin.machine.as_deref() == machine && r.place == pin.place && self.runs(pin)).then(|| pin.place.clone()),
                (None, [(r, a)]) => (a.cwd == m.folder).then(|| r.place.clone()),
                _ => None,
            };
            if place.is_none() {
                let moved = at.first().map(|(_, a)| a.session.clone());
                let successor = || pin.as_ref().and_then(|p| shown.iter().find(|(r, _)| r.place == p.place)).map(|(_, a)| a.session.clone());
                found.insert(w.id.clone(), moved.or_else(successor));
            }
            places.push((m.session.clone(), place));
        }
        {
            let mut seats = self.seats.lock().unwrap();
            seats.retain(|_, (m, _)| m.as_deref() != machine);
            seats.extend(places.iter().map(|(s, p)| (s.clone(), (machine.map(str::to_string), p.clone()))));
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
        Ok(places)
    }

    /// Where the seats on `machine` are is not known any more: its read failed.
    fn forget_seats(&self, machine: Option<&str>) {
        self.seats.lock().unwrap().retain(|_, (m, _)| m.as_deref() != machine);
    }

    /// The machine of the seat `session` holds (none: this one), if it holds one, as recorded
    /// now. Records that cannot be read: nothing can be typed or read for it now.
    pub(in crate::server) fn seat_machine(&self, session: &str) -> Result<Option<Option<String>>, RuntimeError> {
        let read = got(self.store.lock().unwrap().seat_of(session));
        let w = read.map_err(|Unread(why)| RuntimeError::Unreachable(why))?.flatten().filter(|w| !w.closed);
        Ok(w.and_then(|w| w.manager).map(|m| m.machine))
    }

    /// Whether `to` is a session as recorded (`delivery::known_session`): then it is found by
    /// that session alone, never by an agent's name. Records that cannot be read: taken to be
    /// one, so no name stands in for it.
    pub(in crate::server) fn names_a_session(&self, to: &str) -> bool {
        let read = got(self.store.lock().unwrap().recorded());
        read.ok().flatten().is_none_or(|all| crate::server::delivery::known_session(&all, to))
    }

    /// The place on `machine` (none: this one) that holds `session` now: the one answer for
    /// whatever types into or presses at a session's terminal. A session that holds a seat is
    /// at the place it is held to and nowhere else (`see_roots_on`: its pin, while that root
    /// runs and alone shows the session; with no pin, the one root of the seat's machine that
    /// shows it, in the seat's folder). Anything else is `NoAgent`, whatever terminal reports
    /// the session; so is a request that names another machine than the seat's. The seat's
    /// machine is read now (the agents and its roots), and the answer is that read's own,
    /// never one another read wrote meanwhile; not read, it is that error, never an older
    /// answer. A session with no seat is at the one place its machine's runtime shows it
    /// (`Runtime::resolve`). It reads, never under an agent's writer: a message's send holds
    /// `appointing` (or `discussing`) around it, a press nothing.
    pub(in crate::server) fn place_of(&self, session: &str, machine: Option<&str>) -> Result<String, RuntimeError> {
        let Some(seat) = self.seat_machine(session)? else { return self.runtime.resolve(machine, session) };
        let agents = self.runtime.agents().inspect_err(|_| self.forget_seats(seat.as_deref()))?;
        let found = self.see_roots_on(seat.as_deref(), &agents)?.into_iter().find(|(s, _)| s == session).map(|(_, place)| (seat.clone(), place));
        seat_answer(session, seat.as_deref(), machine, found.as_ref())
    }

    /// The same from what the last read of the seat's machine found (`App::seats`), with no
    /// read of its own: for what only reads a screen. None: the records say the session holds
    /// no seat, and nothing else is ever None.
    pub(in crate::server) fn seat_place(&self, session: &str, machine: Option<&str>) -> Option<Result<String, RuntimeError>> {
        match self.seat_machine(session) {
            Err(e) => Some(Err(e)),
            Ok(None) => None,
            Ok(Some(seat)) => Some(self.place_as_read(session, seat.as_deref(), machine)),
        }
    }

    /// Where `session`, seated on machine `seat`, is for a request on `machine`, as the last
    /// read of the seat's machine found it (`App::seats`): `seat_answer`. For a caller that
    /// already has the seat from the records it read.
    pub(in crate::server) fn place_as_read(&self, session: &str, seat: Option<&str>, machine: Option<&str>) -> Result<String, RuntimeError> {
        seat_answer(session, seat, machine, self.seats.lock().unwrap().get(session))
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
        let named = named(&live, agent);
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
        Ok(Seating { session: a.session.clone(), machine: a.machine.clone(), folder, harness: Some(a.harness.clone()), pin })
    }

    /// A seating was recorded: its session is its root's from now on (a place where the
    /// runtime knows no shell has no root that stays, and is not pinned until a read knows
    /// its shell, `see_roots_on`).
    pub(in crate::server) fn seated(&self, seat: Seating) {
        // Where it was found before is not where it is held now: known at the next read.
        self.seats.lock().unwrap().remove(&seat.session);
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
            return Ok(json!({ "actor": null, "name": null, "workspace": null, "manager": false, "intent": null }));
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
            "workspace": workspace.as_ref().map(|(id, name, ..)| json!({ "id": id, "name": name })),
            "manager": workspace.as_ref().is_some_and(|w| w.3),
            "intent": workspace.and_then(|w| w.2),
        }))
    }
}

/// Where `session`, seated on machine `seat`, is for a request on `machine`, from what a read
/// of the seat's machine found (`found`: that machine, and the place or none where the seat
/// is lost; none: not read). The one rule: a request for another machine than the seat's is
/// `NoAgent`; a lost seat is `NoAgent`; a seat whose machine was not read, or whose last read
/// failed, is `Unreachable`.
fn seat_answer(session: &str, seat: Option<&str>, machine: Option<&str>, found: Option<&SeatFound>) -> Result<String, RuntimeError> {
    if seat != machine {
        return Err(RuntimeError::NoAgent(session.into()));
    }
    match found {
        Some((m, Some(place))) if m.as_deref() == seat => Ok(place.clone()),
        Some((m, None)) if m.as_deref() == seat => Err(RuntimeError::NoAgent(session.into())),
        _ => Err(RuntimeError::Unreachable(format!("where the manager {session} runs{} cannot be read now", on(seat)))),
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
    /// The harness the runtime shows it runs.
    pub(in crate::server) harness: Option<String>,
    pin: Option<Pin>,
}

impl Seating {
    /// The root it is pinned to once seated: its place, its shell's process id and start; none
    /// where the runtime knows no shell.
    pub(in crate::server) fn root(&self) -> Option<(String, u32, u64)> {
        self.pin.as_ref().map(|p| (p.place.clone(), p.pid, p.start))
    }
}

/// The CLI asks who and where it is: see `App::here`. Nothing it sends is read for that.
pub(super) async fn here(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Result<Json<serde_json::Value>, ApiError> {
    blocking(move || app.here(&caller).map(Json)).await
}

// ---- the web UI, built into the binary (read from disk in debug builds) ----
