// The data port: what the service asks of whatever keeps the work, in workriver's own
// words. The one implementation is workriver-data; nothing here names it, and nothing of a
// database crosses: no SQL, connection, transaction, table or row id.
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use crate::access::{Principal, Rejected};
use crate::records::{Body, Event, State};
use crate::{Agent, CommandRun, Delivery, DeliveryStep, Project, Question, Task, Thread, Unit, Workspace};

/// How far the record has got: the number of its last committed change (that event's
/// `seq`), 0 before the first. History is read by it too: it is the cursor.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct Revision(pub u64);

/// Everything recorded, as it was at one revision. It is the reader's own: what is recorded
/// afterwards does not change it, and it borrows nothing of the store that gave it.
#[derive(Clone, PartialEq, Debug)]
pub struct Recorded {
    revision: Revision,
    state: Arc<State>,
}

impl Recorded {
    /// For a store: its state at `revision`. Sharing it costs nothing; the store copies its
    /// own before changing it while a reader still holds this one.
    pub fn new(revision: Revision, state: Arc<State>) -> Recorded {
        Recorded { revision, state }
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    /// The state itself, to hand on or change (a blind view, `/api/state`).
    pub fn into_state(self) -> State {
        Arc::unwrap_or_clone(self.state)
    }
}

impl Deref for Recorded {
    type Target = State;

    fn deref(&self) -> &State {
        &self.state
    }
}

/// A request that was recorded: the event as it was committed (ids and numbers given by the
/// rules, the time, who it was recorded as), the revision it made, and everything recorded
/// at that revision: what follows the record is worked out on the state it made, not on
/// whatever was recorded after it by the time it is read again.
#[derive(Clone, PartialEq, Debug)]
pub struct Committed {
    pub event: Event,
    pub revision: Revision,
    pub recorded: Recorded,
}

/// A stretch of the history, oldest first, and where to read on from.
#[derive(Clone, PartialEq, Debug)]
pub struct History {
    pub events: Vec<Event>,
    /// The revision of the last event given (`after` itself when there was none).
    pub next: Revision,
}

/// What the storage says of itself, for `workriver status`.
#[derive(Clone, PartialEq, Debug)]
pub struct DataStatus {
    /// Where the record is kept, as its storage names it (a file).
    pub location: PathBuf,
    /// How many events are stored.
    pub events: u64,
    pub revision: Revision,
}

/// A message that was given to its receiver and not yet seen picked up: what it is about
/// (a task, one of its findings, or a chore, then with no unit or task) and how far it got.
#[derive(Clone, PartialEq, Debug)]
pub struct Delivered {
    pub unit: String,
    pub task: u32,
    pub finding: Option<String>,
    pub chore: Option<String>,
    pub delivery: Delivery,
}

/// A question of a discussion neither answered nor failed: with the agent of its task (its
/// session; none when the task has none). Which machine that agent runs on is the runtime's
/// to say, not the unit's.
#[derive(Clone, PartialEq, Debug)]
pub struct Discussed {
    pub question: Question,
    pub agent: Option<String>,
}

/// Why the store did not do what was asked. The texts are shown as they are.
#[derive(Debug, thiserror::Error)]
pub enum DataError {
    /// The request breaks a rule: the caller's doing. Nothing was written.
    #[error(transparent)]
    Rejected(#[from] Rejected),
    /// The storage could not be read or written; in the storage's words.
    #[error("{0}")]
    Unavailable(String),
    /// What is stored cannot be read as what was recorded.
    #[error("{0}")]
    Corrupt(String),
}

/// Keeps the work: reads of it at one revision, the one way to record, its history and what
/// the storage says of itself.
///
/// The reads are the service's own. Who asks and what they may see is settled before a read
/// (the service's `App::who` and `App::view`, which cuts a blind agent's view with core's
/// rule); no read takes a caller's word, and none is answered to a request directly.
///
/// Every read may fail as a write may: `Unavailable` or `Corrupt` when the storage could not
/// be read, which is never the caller's fault. What is asked for by id and is not there is
/// `Rejected` in the rules' words where the rules have words for it (`unit`, `task`,
/// `question`), else `None`.
pub trait WorkStore: Send {
    /// Everything recorded, at the revision of this call: for what calls a function of the
    /// whole state (the page, a message's plan, a prompt, the reminders, a blind view) or
    /// needs several records of one revision. One record, or what a previous run left, has
    /// a read of its own below: a store need not put the whole work together for those.
    fn recorded(&self) -> Result<Recorded, DataError>;

    /// Records `request` as `who`: checked against the rules on what is recorded now
    /// (`State::prepare`), then committed and added. `who` is who the caller was found to
    /// be, never who it says it is. A refused or failed request changes nothing: not the
    /// record, not what is read, not the next id. There is no other way to write.
    fn record(&mut self, who: &Principal, request: Body) -> Result<Committed, DataError>;

    /// Up to `limit` events recorded after `after`, oldest first. For the service itself and
    /// for tests only: there is no blind cut for events (core's blind rule cuts a `State`),
    /// so nothing of this may be answered to a reader until there is one.
    fn history(&self, after: Revision, limit: usize) -> Result<History, DataError>;

    fn status(&self) -> Result<DataStatus, DataError>;

    /// The events that make intent `id`'s versions, oldest first: its `intent.created`,
    /// `intent.updated` and `intent.merged` into it, and the same of every intent merged into
    /// it, and into those (`intent_versions` and `intent_text` rebuild its history from them).
    /// None for an intent that is not recorded. Not to be answered to a reader as they are:
    /// whoever asks is checked first (`App::view`).
    fn intent_events(&self, id: &str) -> Result<Vec<Event>, DataError>;

    // ---- one record, by its id ----

    fn unit(&self, id: &str) -> Result<Unit, DataError> {
        Ok(self.recorded()?.unit(id)?.clone())
    }

    fn task(&self, unit: &str, n: u32) -> Result<Task, DataError> {
        Ok(self.recorded()?.task(unit, n)?.clone())
    }

    fn question(&self, id: &str) -> Result<Question, DataError> {
        Ok(self.recorded()?.question(id)?.clone())
    }

    /// A question's thread, or any thread, by its id.
    fn thread(&self, id: &str) -> Result<Option<Thread>, DataError> {
        Ok(self.recorded()?.threads.iter().find(|t| t.id == id).cloned())
    }

    /// The agent of `session` as it was last seen (its name, its harness), if ever.
    fn agent(&self, session: &str) -> Result<Option<Agent>, DataError> {
        Ok(self.recorded()?.agents.iter().find(|a| a.session == session).cloned())
    }

    /// The name the agent of `session` goes by, if it was ever seen.
    fn agent_name(&self, session: &str) -> Result<Option<String>, DataError> {
        Ok(self.agent(session)?.map(|a| a.name))
    }

    /// A command the user ran, as recorded.
    fn run(&self, id: &str) -> Result<Option<CommandRun>, DataError> {
        Ok(self.recorded()?.runs.iter().find(|r| r.id == id).cloned())
    }

    /// A registered project by its id (`p1`).
    fn project(&self, id: &str) -> Result<Option<Project>, DataError> {
        Ok(self.recorded()?.projects.iter().find(|p| p.id == id).cloned())
    }

    /// A workspace by its id (`ws1`).
    fn workspace(&self, id: &str) -> Result<Workspace, DataError> {
        Ok(self.recorded()?.workspace(id)?.clone())
    }

    /// The workspace whose seat `session` holds, if any.
    fn seat_of(&self, session: &str) -> Result<Option<Workspace>, DataError> {
        Ok(self.recorded()?.seat_of(session).cloned())
    }

    /// Whether `session` holds the seat of any workspace.
    fn manages(&self, session: &str) -> Result<bool, DataError> {
        Ok(self.seat_of(session)?.is_some())
    }

    /// Every unit, in the order they were made.
    fn units(&self) -> Result<Vec<Unit>, DataError> {
        Ok(self.recorded()?.units.clone())
    }

    // ---- what a previous run left, for the service's start ----

    /// Messages given to their receiver and not seen picked up: tasks' and their findings'
    /// in the units' order, then chores'.
    fn delivered(&self) -> Result<Vec<Delivered>, DataError> {
        let all = self.recorded()?;
        let delivered = |d: &Option<Delivery>| d.clone().filter(|d| d.step == DeliveryStep::Delivered);
        let mut out = vec![];
        for u in &all.units {
            for t in &u.tasks {
                if let Some(d) = delivered(&t.delivery) {
                    out.push(Delivered { unit: u.id.clone(), task: t.n, finding: None, chore: None, delivery: d });
                }
                for f in &t.findings {
                    if let Some(d) = delivered(&f.delivery) {
                        out.push(Delivered { unit: u.id.clone(), task: t.n, finding: Some(f.found.id.clone()), chore: None, delivery: d });
                    }
                }
            }
        }
        for c in &all.chores {
            if let Some(d) = delivered(&c.delivery) {
                out.push(Delivered { unit: String::new(), task: 0, finding: None, chore: Some(c.id.clone()), delivery: d });
            }
        }
        Ok(out)
    }

    /// Deliveries held for a receiver on a blind task (`DeliveryStep::Held`): tasks' and their
    /// findings', of units and of intents' own work, then chores', then backlog findings'.
    fn held_deliveries(&self) -> Result<Vec<Delivered>, DataError> {
        let all = self.recorded()?;
        let held = |d: &Option<Delivery>| d.clone().filter(|d| d.step == DeliveryStep::Held);
        let mut out = vec![];
        let owners = all.units.iter().map(|u| (&u.id, &u.tasks)).chain(all.intents.iter().map(|i| (&i.id, &i.tasks)));
        for (owner, tasks) in owners {
            for t in tasks {
                if let Some(d) = held(&t.delivery) {
                    out.push(Delivered { unit: owner.clone(), task: t.n, finding: None, chore: None, delivery: d });
                }
                for f in &t.findings {
                    if let Some(d) = held(&f.delivery) {
                        out.push(Delivered { unit: owner.clone(), task: t.n, finding: Some(f.found.id.clone()), chore: None, delivery: d });
                    }
                }
            }
        }
        for c in &all.chores {
            if let Some(d) = held(&c.delivery) {
                out.push(Delivered { unit: String::new(), task: 0, finding: None, chore: Some(c.id.clone()), delivery: d });
            }
        }
        for b in &all.backlog {
            if let Some(d) = held(&b.finding.delivery) {
                out.push(Delivered { unit: String::new(), task: 0, finding: Some(b.finding.found.id.clone()), chore: None, delivery: d });
            }
        }
        Ok(out)
    }

    /// The ids of the questions neither answered nor failed, but those of a discussion (their
    /// answerer is the task's own agent, not one a run of the service started).
    fn open_questions(&self) -> Result<Vec<String>, DataError> {
        Ok(self.recorded()?.questions.iter().filter(|q| q.open() && !q.discuss).map(|q| q.id.clone()).collect())
    }

    /// The questions of a discussion neither answered nor failed.
    fn open_discussions(&self) -> Result<Vec<Discussed>, DataError> {
        let s = self.recorded()?;
        let of = |q: &Question| Discussed { question: q.clone(), agent: s.task(&q.unit, q.task).ok().and_then(|t| t.agent.clone()) };
        Ok(s.questions.iter().filter(|q| q.open() && q.discuss).map(of).collect())
    }

    /// The user's messages to a manager that got no delivery yet: each message's id and, for
    /// one to every manager, the manager of each copy not given yet (else empty).
    fn held_messages(&self) -> Result<Vec<(String, String)>, DataError> {
        let all = self.recorded()?;
        let mut out = vec![];
        for m in all.messages.iter().filter(|m| !m.by_manager) {
            match m.to.is_empty() {
                true if m.delivery.is_none() => out.push((m.id.clone(), String::new())),
                true => {}
                false => out.extend(m.to.iter().filter(|r| r.delivery.is_none()).map(|r| (m.id.clone(), r.manager.clone()))),
            }
        }
        Ok(out)
    }

    /// The starts of a manager still pending, by workspace: its id and the start's sequence.
    fn pending_starts(&self) -> Result<Vec<(String, u64)>, DataError> {
        Ok(self.recorded()?.workspaces.iter().filter_map(|w| Some((w.id.clone(), w.start.as_ref().filter(|s| s.ended.is_none())?.seq))).collect())
    }

    /// The names of the temporary agents threads were last answered by.
    fn thread_agents(&self) -> Result<Vec<String>, DataError> {
        Ok(self.recorded()?.threads.iter().filter_map(|t| t.name.clone()).collect())
    }
}
