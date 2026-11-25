// Answering a dialog from the page: one request, one agent, one place, resolved once; keys
// only after the whole screen read there is the one the user saw; arrows one step and a
// read each; never a key sent again by itself; and what came of it recorded.
use std::time::{Duration, Instant};

use workriver_core::screens::{self, KEPT_WORDS, Key, KeptOption, Outcome};
use workriver_core::{Body, Principal, TaskState};

use super::*;

/// How often the screen is read after a key, and for how long at most; a screen that holds
/// still this long is taken as what the key did.
const READ_EVERY: Duration = Duration::from_millis(100);
const READ_FOR: Duration = Duration::from_secs(2);
const STEADY: Duration = Duration::from_millis(400);

/// What a press came to, for the page: what went, what came of it, and the screen now when
/// the dialog is still there or another one is.
#[derive(Serialize)]
pub(in crate::server) struct Answered {
    outcome: Outcome,
    keys: Vec<Key>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screen: Option<ScreenView>,
}

/// The options' words and the line above them: what tells one dialog from another.
fn same(a: &screens::Dialog, b: &screens::Dialog) -> bool {
    a.above == b.above && a.options.len() == b.options.len() && a.options.iter().zip(&b.options).all(|(x, y)| x.words == y.words && x.number == y.number)
}

impl App {
    /// Sends what the user pressed to the agent with this session on `machine`, if the
    /// screen there is still the one whose digest the page was given (`shown`).
    pub(in crate::server) fn answer(&self, session: &str, machine: Option<&str>, shown: &str, press: Press) -> Result<Answered, Refusal> {
        // 1. Only to an agent that waits, as last polled.
        if self.waiting(session, machine).is_none() {
            return Err(Refusal::new(StatusCode::CONFLICT, "not-waiting", "It is no longer waiting"));
        }
        // 2. The one place, once: every read and key of this request goes there. A seated
        // manager's is where its seat is held to, its machine read now (`place_of`); found
        // before the writer is taken, which orders workriver's own writes and never held the
        // terminal still: what binds the keys to the place is the screen read there below.
        let place = self.place_of(session, machine)?;
        // 3. One writer per agent: a second press, or a message being typed, comes first.
        let writer = self.terminals.writer(session, machine);
        let Ok(_one) = writer.try_lock() else { return Err(Refusal::new(StatusCode::CONFLICT, "busy", "Something is being sent to it now; look again")) };
        // 4. The screen the user saw, or nothing is sent.
        let before = self.runtime.screen(machine, &place)?;
        let count = self.terminals.count(session, machine);
        if self.terminals.digest(session, machine, count, &before.text) != shown {
            let view = self.screen_view(session, machine, &place, count, before, false);
            return Err(Refusal { screen: Some(Box::new(view)), ..Refusal::new(StatusCode::CONFLICT, "changed", "The screen changed; look again") });
        }
        let dialog = screens::parse(&before.text);
        // 5. Send.
        let mut keys = vec![];
        let mut kept = None;
        let sent = match press {
            Press::Key(k) => self.send_keys(session, machine, &place, &[k], &mut keys),
            Press::Option(n) => {
                let d = dialog.as_ref().ok_or_else(|| Refusal::new(StatusCode::CONFLICT, "no-dialog", "No options on its screen; use the keys"))?;
                let o = d.options.get(n - 1).ok_or_else(|| Refusal::new(StatusCode::BAD_REQUEST, "invalid", format!("the dialog has {} options", d.options.len())))?;
                kept = Some(KeptOption { number: o.number, words: screens::kept(&o.words, KEPT_WORDS) });
                match o.number {
                    // A numbered option: its digit, whatever is selected.
                    Some(digit) => self.send_keys(session, machine, &place, &[Key::Digit(digit)], &mut keys),
                    None => self.walk(session, machine, &place, d, n - 1, before.text.clone(), &mut keys),
                }
            }
        };
        // 6. What came of it.
        let (outcome, now) = match sent {
            Err(Step::Unsure) => (Outcome::Unsure, self.runtime.screen(machine, &place).ok()),
            Err(Step::Changed(now)) => (Outcome::Changed, Some(now)),
            Ok(()) => self.after(machine, &place, &before.text, dialog.as_ref()),
        };
        // 7. Recorded, with the task the agent holds; a record that fails is logged: the keys went.
        let task = self.held_task(session);
        let body = Body::AgentKeys {
            agent: session.into(),
            machine: machine.map(str::to_string),
            unit: task.as_ref().map(|t| t.0.clone()),
            task: task.map(|t| t.1),
            keys: keys.clone(),
            outcome,
            question: dialog.as_ref().and_then(|d| d.above.clone()),
            option: kept,
        };
        if let Err(e) = self.record(&Principal::User, body) {
            tracing::warn!("keys went to {session}, but the answer is not recorded: {}", e.1);
        }
        let _ = self.changed.send(());
        let count = self.terminals.count(session, machine);
        let screen = now.filter(|_| outcome != Outcome::Answered).map(|s| self.screen_view(session, machine, &place, count, s, false));
        Ok(Answered { outcome, keys, screen })
    }

    /// Presses `k` at the place; a failed or lost send counts as one that may have gone.
    fn send_keys(&self, session: &str, machine: Option<&str>, place: &str, k: &[Key], keys: &mut Vec<Key>) -> Result<(), Step> {
        self.terminals.pressed(session, machine);
        keys.extend_from_slice(k);
        self.runtime.press(machine, place, k).map_err(|e| {
            tracing::warn!("keys to {session} may not have gone: {e}");
            Step::Unsure
        })
    }

    /// A marked list: one arrow toward option `to`, then a read; on only while the marker sits
    /// on the next line expected (the list wraps: steps are never counted ahead) and nothing
    /// else on the screen changed; Enter only when the marked line's words are the option's.
    #[allow(clippy::too_many_arguments)]
    fn walk(&self, session: &str, machine: Option<&str>, place: &str, d: &screens::Dialog, to: usize, mut text: String, keys: &mut Vec<Key>) -> Result<(), Step> {
        let mut at = d.selected().ok_or_else(|| Step::Changed(Screen::of(text.clone())))?;
        for _ in 0..d.options.len() {
            if at == to {
                break;
            }
            let (key, next) = if to > at { (Key::Down, at + 1) } else { (Key::Up, at - 1) };
            self.send_keys(session, machine, place, &[key], keys)?;
            let now = self.settled(machine, place, &text).ok_or(Step::Unsure)?;
            if screens::marked_row(&now.text) != Some(d.options[next].row) || screens::unmarked(&now.text) != screens::unmarked(&text) {
                return Err(Step::Changed(now));
            }
            at = next;
            text = now.text;
        }
        let words = screens::parse(&text).and_then(|n| n.selected().map(|k| n.options[k].words.clone()));
        if at != to || words.as_deref() != Some(d.options[to].words.as_str()) {
            return Err(Step::Changed(Screen::of(text)));
        }
        self.send_keys(session, machine, place, &[Key::Enter], keys)
    }

    /// The screen once it shows something other than `was` (an arrow's effect), read every
    /// 100 ms for up to 2 s; the last one read when it never does; none when it cannot be read.
    fn settled(&self, machine: Option<&str>, place: &str, was: &str) -> Option<Screen> {
        let start = Instant::now();
        loop {
            let now = self.runtime.screen(machine, place).ok()?;
            if now.text != was || start.elapsed() >= READ_FOR {
                return Some(now);
            }
            std::thread::sleep(READ_EVERY);
        }
    }

    /// Reads after the keys until the dialog is gone and the runtime, read successfully, no
    /// longer reports the agent blocked (answered), another dialog holds still there (a new
    /// question), or the same one holds still after a change or for the whole wait (still
    /// open); with the dialog gone and the runtime still reporting blocked at the end, or its
    /// state not read: sent.
    fn after(&self, machine: Option<&str>, place: &str, before: &str, was: Option<&screens::Dialog>) -> (Outcome, Option<Screen>) {
        let start = Instant::now();
        let mut last: Option<(String, Instant)> = None;
        let mut seen = None;
        loop {
            let now = match self.runtime.screen(machine, place) {
                Ok(s) => s,
                Err(_) => return (Outcome::Unsure, seen),
            };
            let steady = match &last {
                Some((t, since)) if *t == now.text => since.elapsed() >= STEADY,
                _ => {
                    last = Some((now.text.clone(), Instant::now()));
                    false
                }
            };
            let d = screens::parse(&now.text);
            let over = start.elapsed() >= READ_FOR;
            match (&d, was) {
                (None, _) => {
                    // Answered only when the runtime says, in a read that worked, that the agent
                    // no longer waits; a failed read says nothing, and is read again.
                    let goes_on = self.runtime.locate(machine, place).ok().and_then(|p| p.agent).is_some_and(|a| a.state != AgentState::Blocked);
                    if goes_on {
                        return (Outcome::Answered, Some(now));
                    }
                    if over {
                        return (Outcome::Sent, Some(now));
                    }
                }
                (Some(d), Some(w)) if same(d, w) => {
                    if (steady && now.text != before) || over {
                        return (Outcome::Open, Some(now));
                    }
                }
                (Some(_), _) => {
                    if steady || over {
                        return (Outcome::New, Some(now));
                    }
                }
            }
            seen = Some(now);
            std::thread::sleep(READ_EVERY);
        }
    }

    /// The task the agent holds now (started, not delivered), if one.
    fn held_task(&self, session: &str) -> Option<(String, u32)> {
        let state = got(self.store.lock().unwrap().recorded()).ok().flatten()?;
        let on = |owner: &str, ts: &[workriver_core::Task]| ts.iter().filter(|t| t.agent.as_deref() == Some(session) && t.state == TaskState::Running).map(|t| (owner.to_string(), t.n)).collect::<Vec<_>>();
        let mine: Vec<(String, u32)> = state.units.iter().flat_map(|u| on(&u.id, &u.tasks)).collect();
        (mine.len() == 1).then(|| mine[0].clone())
    }
}

/// Why sending stopped short.
enum Step {
    /// A key may have gone without its answer; nothing more is sent.
    Unsure,
    /// Something else changed between steps: Enter was not sent. The screen now.
    Changed(Screen),
}
