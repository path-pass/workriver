# Answering a stuck agent from the page

Design of this branch: two designs written independently, merged into one (how they were
merged is the last section). Before it is merged its rules go into
`docs/design/ui/navigation.md` (Needs you, Agents), `docs/design/ui/operations.md` (Opening a
terminal), `docs/design/layout.md` and `docs/security.md`, and this file is removed.

What it builds on was measured with Herdr 0.9.5 (protocol 22), Claude Code 2.1.330 and
codex-cli 0.171.0: Herdr works headless; `pane.agent_status_changed` must be subscribed per
pane; `terminal session observe` reaches a pane through `herdr-client.sock`, not the API
socket; a watched idle pane costs nothing, a watched working pane about 3 % of one core in
Herdr's server and 8.7 MB per observe process; a page keeps about 16 WebGL contexts; Herdr
saw all six Claude dialogs as blocked and missed three of six Codex ones ("Hooks need
review" reported idle, "Implement this plan?" done); a list under `❯`/`›` wraps around;
a digit submits in most dialogs, only moves the selection in one (an option that is a text
field) and in another submits its own option whatever is selected. Only a loopback ssh was
measured, not a second machine.

## Decisions

1. The terminal is not shown by default. A stuck agent becomes a **gate** in Needs you: the
   dialog's text and buttons to answer it. A **terminal view** opens only when the user asks,
   one per page, read-only.
2. The service, never the page, reads screens, parses them and sends keys. The page sends
   "press this" with the digest of the screen it showed; the service checks the screen still
   says that, and sends nothing when it does not.
3. Sending keys is the user's act: `App::user`, never on a machine's listener, recorded.
4. The runtime keeps being read every 2 s (the poll stays the source of every state); the
   new subscription only wakes the poll early. A missed event costs at most one tick.
5. Where the runtime misses a dialog, the service still sees it on the screen, with the same
   parser and for every harness alike: of an agent that has just gone idle or done, and of
   any agent workriver is about to type a message into.
6. No screen text leaves the gate's own routes: not in the snapshot, the change stream, the
   event store, the logs or browser storage.

## 1. Noticing a stuck agent

- **The runtime says blocked.** The poll already turns that into a Blocking row (`blocked`
  in the service, how long it has waited). The subscription (section 6) makes the next poll
  run at once, so the row comes within about half a second instead of up to two.
- **The runtime misses it.** When an agent's state becomes idle or done (or is first seen
  so, which also rebuilds the marks after a restart), the service reads its visible screen
  once then and once a tick later (a dialog may be drawn after the state changed), and
  parses it (section 2). A dialog found makes the agent **waiting on its screen**: shown and
  counted exactly as blocked, its deliveries held as for a blocked agent, with one quiet line
  on the gate: "The runtime reports it idle; its screen shows a question." While it is so
  marked its screen is read every tick; the mark goes when the dialog is gone or the runtime
  reports another state. An agent idle or done for longer is not read again by the poll. In
  memory only, like `blocked` and `idle_since`.
- **Before typing a message.** Every message workriver types into an idle or done agent (a
  delivery, a reminder, a question agent's first message after its start, which can report
  success over such a dialog) is preceded by one read of its screen; a dialog there marks the
  agent waiting on its screen and the message is held as for a blocked agent (`Blocked`), so a
  prompt is never typed into a dialog. A dialog that appears later without any change of
  state and before any message is not seen; the terminal view shows it.
- The runtime's reason goes with the gate: `agent.explain`'s rule and evidence, read when
  the gate opens, shown quietly ("bash_permission_prompt"); none for one waiting on its
  screen.

## 2. Reading a dialog off a screen

One parser, in core (`workriver-core/src/screens/`: pure, no I/O, no UI text, no harness
names), from a screen's visible text to a `Dialog` or none.

It reads from the bottom of the screen up, so text above the dialog (a transcript that
quotes a dialog, a plan's own numbered steps) is never reached:

- **Hints.** The lowest non-empty lines, while they are not options: `Esc to cancel · Tab
  to amend`, `enter confirm · esc skip`, `Press enter to confirm or esc to cancel`. Each
  part (split at `·`, `|`, ` or `) that names a key of the allowed set (below) becomes a
  button with that part's words; any other (`ctrl+g to edit`, a path) stays text.
- **Options.** Going up from there: numbered lines `[marker] N. words` (marker `❯` or `›`
  before the selected one) counting down to 1, each with the lines indented under it (its
  wrapped text, a sub-line, a column of notes); blank lines and full-width rules between
  them are passed over (a question's last option sits under a rule). Reaching `1.` ends the
  options. With no numbers: the run of lines just above the hints at one indent, one of them
  marked. Numbered options are answered by their digit, a marked list by arrows. Letter
  shortcuts (`(y)`, `(p)`) are left as text: the digit does the same.
- **Shown region.** The options, the hints, and above the first option the lines up to the
  nearest solid rule (`─`; dashed `╌` rules are inside a dialog and frame its command), at
  most twelve. "Whole screen" shows the rest.
- **A dialog** needs options (numbered, or two or more with one marked). For the
  waiting-on-its-screen check (section 1) it also needs a hint that names a key: a screen
  the runtime does not call blocked must look like a dialog in both ways to be called one.
- **Keys** the service will ever send: Up, Down, Enter, Escape, Tab, digits 1–9. Nothing
  else, no text: a screen can say anything, and the page has no other way to type.

What the parser cannot know stays visible instead of guessed: an option that is a text
field ("Type something.", "Tell … what to change") is a button like any other; after it
the dialog is still there, with the selection on that field, and the gate says so.

## 3. Sending keys

`POST /api/agent/keys` `{ agent, machine, shown, press }`: `agent` is the agent's session;
`shown` is the digest of the region the page showed (the whole visible text when no dialog
was parsed), as `/api/agent/screen` gave it; `press` is an option (`{ option: 2 }`) or one
key (`{ key: "escape" }`). The handler:

1. `App::user`; the agent found by session and machine in a fresh read, never by a name and
   never from a place kept from earlier; none, or more than one: refused, nothing sent. Not
   blocked and not waiting on its screen: refused, nothing sent ("It is no longer waiting").
2. Takes the agent's write lock: one writer per agent at a time, shared with message
   delivery (section 1), so a press and a message never interleave. A second press while one
   runs: refused.
3. Reads the screen. Its digest differs from `shown`: refused, nothing sent, the new screen
   returned ("The screen changed; look again"). A press that already went therefore cannot
   go again by a second click or a retried request: the screen has changed.
4. Sends:
   - a numbered option: its digit, whatever is selected;
   - a marked list: one Up or Down toward the option, then reads again, and goes on only
     when the marker sits on the next line expected (the list wraps, so it never counts
     steps ahead), at most as many steps as there are options; Enter only when the marked
     line's words are the chosen option's words. The marker anywhere else stops it: what was
     sent is said, Enter is not sent;
   - one key: that key.
5. Reads again (every 100 ms, up to 2 s) until the dialog is gone **and** the runtime does
   not report the agent blocked: "Answered". Another dialog there: "A new question", with
   its screen and buttons. The same dialog still there: "Sent; the dialog is still open",
   with the new screen and its new buttons (a digit that only moved the selection). A send
   that failed or whose answer was lost: "It may have been sent; look again", with a fresh
   screen when one can be read. Nothing is ever sent again by itself.
6. Records `agent.keys` (`Body::user_only`): the agent, its task if it holds one, the
   dialog's first line and the chosen option's words (quoted text), the keys sent, and the
   outcome; not the region, not the screen (a command in a dialog can carry anything). A
   failed record after keys went is logged, and the answer says the keys went.

## 4. The gate

- **Where.** The Blocking row in Needs you ("ja-impl is waiting for you in its terminal",
  its topic, as now; one waiting on its screen says the same) opens the gate on a click (no
  longer the terminal app). The row's terminal button opens the terminal view. The same gate
  opens from a running task's detail ("Waiting for you in its terminal · Answer"), from a
  blocked line in Agents, and from a browser notification of a newly waiting agent.
- **What.** A dialog (Radix, modal, up to 960 px wide so 120 columns fit). Opening it reads
  `/api/agent/screen`; a failed read says why, with the keys disabled and "Try again". Top
  to bottom: the agent (mark, name), its task as a link or its workspace, how long it has
  waited, the runtime's reason quietly (or the waiting-on-its-screen line); the region as
  quoted agent text (mono, plain text, never Markdown, links inert), "Whole screen" showing
  every visible line; the options as buttons in their order, each its own words and, as a
  small key cap, what will be sent ("2", or "↓ ↵"); the hint keys ("Esc · cancel"); last,
  quieter and always there, the generic keys ↑ ↓ ↵ Esc Tab 1–9, one key each, for doing it
  by hand. No dialog parsed: only the screen and the generic keys, and "Open terminal view"
  stands out.
- **After a press.** Buttons wait while it runs. Answered: the gate closes, the row leaves
  by itself. Still open, a new question, or changed: the gate shows the new screen and its
  new buttons with one line saying which. Refused or failed: the reason in the user's
  language, the service's words in a tooltip.
- **In the record.** An answered dialog shows in its task's detail with the other things
  the user put in: "You answered in its terminal: Do you want to proceed? — 2. Yes, and
  don't ask again", the time.
- **On the rail.** A stuck agent counts where Blocking counts today: in the filled violet
  mark of the unit whose task it holds (and so in its folded intent's sum); one that holds no
  task (a manager) in its intent's, or, when its workspace runs none, in Needs you and the
  tab's title only.

## 5. The terminal view

- **Opening.** The terminal button in Agents, in a Blocking row, in a running task's
  header, and "Open terminal view" in the gate. Today these bring the terminal app to the
  front; that stays as "Open in terminal app" in the view's header.
- **Where.** A dock at the bottom of the main area, absent until a view is opened: the
  agent's mark and name, its state, "Open in terminal app", fold, close; its height dragged
  with `useResize` (remembered). One view per page: opening another agent's replaces it. On
  a narrow screen it is a sheet.
- **One stream, while shown.** The dock folded, the page hidden (`visibilitychange`) or the
  view closed ends its stream; showing it again opens a new one, whose first frame is the
  whole screen (5–10 KB). So a page holds at most one stream, and Herdr pays 3 % of a core
  only for what is looked at. A stream that ends for another reason (the connection, a
  reader too slow, section 6) is reopened by the page once, from a whole frame.
- **Drawing.** xterm.js (`@xterm/xterm`), loaded only when a view first opens, DOM renderer
  (one terminal shown: 1.4 % of the main thread in the measurement; no WebGL addon, no
  context limit to meet). Columns and rows are the pane's (from the frames); the font scales
  so the width fits, down to 9 px, then the view scrolls sideways. The pane is never
  resized. Read-only: `disableStdin`, no key reaches the pane; no link addon, no clipboard
  write from the terminal (OSC 52 ignored), nothing it draws acts outside it. While its
  agent waits, the gate's buttons sit under the view (the same component, the same request),
  for when only the terminal shows what to press.
- **Gone.** An agent that leaves, a machine that cannot be reached, or a view that cannot be
  had (another machine without the client socket forwarded, an observe that fails): the view
  keeps its last frame, greyed, or shows nothing, with one line why. The gate still works:
  it needs only reads and keys.

## 6. What the service gives, and the runtime under it

Routes, all `App::user`, none in `machine_router`, every answer `Cache-Control: no-store`;
agents read no screen of another agent through workriver (a blind reviewer could otherwise
read the work it must not see):

| Route | Gives |
|---|---|
| `GET /api/agent/screen?agent=&machine=` | the visible text, its size, its digest, the parsed `Dialog`, the runtime's reason |
| `POST /api/agent/keys` | section 3 |
| `GET /api/agent/view?agent=&machine=` | Server-Sent Events, one per frame: `{ full, cols, rows, seq, bytes }` (bytes base64, as Herdr gives them); ends when the pane goes |

The state of agents keeps reaching the page as today (`/api/events` says "changed", the page
reads the snapshot). The snapshot says an agent is blocked or waiting on its screen, and
since when; never what its screen says. The gate reads that with `/screen`.

Open views are capped in the service (4 in `config/defaults`, across every page); one more
is refused with the reason. One observe child per open view, not shared: the first frame
must be whole for each viewer, and the cap keeps it to 35 MB. Each stream has a bounded
queue (in `config/defaults`) and a frame larger than a bound is not passed on; a reader that
falls behind has its stream ended (the page reopens from a whole frame).

**The `Runtime` trait** grows by five methods, each with a default that cannot
(`RuntimeError::None`), so a runtime without them still runs:

```rust
/// A change of an agent's state, as the runtime tells it when it happens.
pub struct Change { pub place: String, pub state: AgentState }

/// Blocks, telling `to` each change of an agent's state on `machine` as it happens, until
/// `to` returns false (Ok) or the connection ends (the error). Only a wake-up: states are
/// still read with `agents`; a change missed before it started or while it reconnects is
/// seen by the next read.
fn watch(&self, machine: Option<&str>, to: &mut dyn FnMut(Change) -> bool) -> Result<(), RuntimeError>;
/// What the agent with this session on `machine` shows now, as plain text, one line per
/// row, with its size.
fn screen(&self, machine: Option<&str>, session: &str) -> Result<Screen, RuntimeError>;
/// Presses keys at that agent, in order; no text.
fn press(&self, machine: Option<&str>, session: &str, keys: &[Key]) -> Result<(), RuntimeError>;
/// That agent's terminal as frames, the first one whole; dropping it stops it.
fn observe(&self, machine: Option<&str>, session: &str) -> Result<Box<dyn Iterator<Item = Result<Frame, RuntimeError>> + Send>, RuntimeError>;
/// Why the runtime calls that agent blocked, in its words (a rule and its evidence).
fn why_blocked(&self, machine: Option<&str>, session: &str) -> Result<Option<String>, RuntimeError>;
```

Each resolves the session to the agent's place itself, at the call, as `send_to` does:
`NoAgent` when no live agent has it on that machine, never another agent by its name.
`Key` (Up, Down, Enter, Escape, Tab, Digit(1..=9)), `Screen` and `Frame` are core's.
`read_place` stays as it is (the last lines of a run's shell, not the visible screen).

The service runs one `watch` per machine on a thread of its own (this machine's from
`App`, the others' in `follow.rs` next to their reads), calls it again 5 s after it ends,
and on each change wakes the poll for that machine (coalesced: one poll per tick at most
early). A failed poll stays a failed read, as now: never an empty list of agents. The
trait's first method that pushes; it is reviewed on its own.

**The Herdr adapter** (`runtime/herdr/`, a file `watch.rs` and one `screens.rs`):

- `watch`: one long connection, `events.subscribe` with `pane.agent_detected` and
  `pane.closed` (these take no pane) and `pane.agent_status_changed` for each agent pane of
  an `agent.list` read at the start. A pane detected or closed ends that connection and
  opens a new one with the new list (re-subscribing on a connection is not documented).
- `screen`: `pane.read`, source `visible`, format `text`. `press`: `pane.send_keys`, the
  keys in Herdr's names. `why_blocked`: `agent.explain`.
- `observe`: a child `herdr terminal session observe <pane>` (Herdr's own client protocol is
  private; the CLI must match the server's version), its environment cleared of
  `HERDR_PANE_ID`, `HERDR_WORKSPACE_ID`, `HERDR_TAB_ID`, `HERDR_ENV`, with
  `HERDR_SOCKET_PATH` set to the socket used for that machine; its stdout read line by line
  (`terminal.frame`); dropping the iterator kills and reaps the child, as does the service's
  shutdown.
- **Other machines:** observe derives `<socket without .sock>-client.sock`. `ssh.rs` adds a
  second `-L` in the same ssh: `herdr-<label>-client.sock` here to the remote API socket's
  sibling `-client.sock` there. Both paths stay under the 104-byte limit in the data
  directory. A client socket that is not there fails `observe` alone, with its reason.

**Another runtime** (one speaking ACP, for harnesses that are not run in a terminal): its
client receives the agent's permission requests itself, so `watch` reports Blocked when one
arrives and stops reporting when it is answered; it has no screen, keys or frames
(`screen`, `press`, `observe` stay `None`). Its gate would come from the request's own
options, not from the parser: a sixth method (answer an option by its id) is added then, not
now. The gate's page part does not change: it shows a `Dialog`, whoever made it.

## 7. Security

- The three routes are the user's only (`App::user`); none in `machine_router`; an agent's
  proof is refused there whatever it names. The guard's Host and Origin checks apply as to
  every route (a POST only from our origin); the service sends no CORS headers, so another
  origin cannot read `/screen` or `/view`.
- What may be sent is the fixed key set; the request names an option by its number or a key
  by its name, never text, a sequence of keys, a size or a paste. The service sends only
  after the screen it reads matches what the user saw, reads again after each arrow, and
  never repeats a send by itself.
- The screen's text and the options' words are another's writing: the page shows them as
  plain text (quoted agent text, as everywhere) and ANSI only inside the terminal view; they
  never go into a message to an agent; in the record only the dialog's first line and the
  option's words, quoted. Screen text and frames are kept out of the snapshot, the event
  store (beyond that), the logs and browser storage.
- Observe children: one per open view, capped, killed with their stream; the child gets no
  more environment than it needs.

## 8. Tests (the rules as tests)

Service, with the fake runtime (`server/tests/`):

1. `/api/agent/keys`, `/screen`, `/view` refuse an agent's proof (a blind one too) and a
   caller with none; they are not in `machine_router` (the guard that lists its routes); a
   POST from another origin is refused.
2. A screen that differs from `shown`: refused, `press` never called. The same request sent
   twice: the second refused, one press.
3. A marked list that wraps: one key per step, a read after each; the marker moving
   anywhere but the next line, or landing on a line whose words are not the option's, stops
   it without Enter; no more steps than options. A numbered option: its digit alone,
   whatever is selected.
4. A digit after which the dialog is still there: "still open", with the new screen; a
   second dialog after the first: "a new question"; the dialog gone while the runtime still
   reports blocked: not "Answered" before it stops.
5. An agent not blocked: refused; two presses at once: the second refused; a press while a
   message is being typed into the same agent waits or is refused, never interleaves.
6. A key outside the set, or text: refused (400), nothing sent. A session no live agent has
   on that machine, or a name: refused, nothing sent.
7. `press` failing or its answer lost: "may have been sent", `press` called once.
8. An idle agent whose screen shows a dialog with hints: waiting, held deliveries; an idle
   screen with the same dialog text above an input box: not; the mark goes when the screen
   clears. A delivery to an idle agent whose screen shows a dialog: held, nothing typed.
9. `watch` reporting a change: the poll runs before the next tick; `watch` failing: the
   poll goes on at its pace and `watch` is called again.
10. A view whose stream is dropped: its child is gone; the cap's next view refused; a
    reader that falls behind: its stream ended.
11. `agent.keys` is `user_only` (the event rules' table test); the snapshot of a blocked
    agent carries no screen text.

Core: the parser against each dialog the measurement saw (Claude: trust, Bash, Write,
question, plan; Codex: hooks review, command, edit, plan, question) as fixtures in
`workriver-core/src/screens/__fixtures__/` (the screens as captured, paths of the machine
they came from replaced), and against idle screens with a dialog quoted in the transcript.

Adapter: the `-L` pair in `ssh.rs`'s arguments; `observe`'s child environment. By hand, on
an isolated headless Herdr: a whole frame then deltas, the child gone after a close, a
missing client socket, both sockets through ssh.

Page: the screen shown as text (Markdown, a link, an escape sequence stay inert); a press
disabled while one runs; a failed screen read disables the keys; the dock streams only
while shown and ends it on `visibilitychange`; the terminal view opens no link and writes
no clipboard; every string in en, zh-CN and ja.

## Not done

- Typing text into an option that is a text field; such an answer is given in the terminal.
- More than one terminal view at once, and typing into the terminal view.
- A dialog that appears with no change of state and before any message (section 1).
- Telling Herdr of the two Codex dialogs it misses, and starting workers so they do not
  meet them (hooks trusted, no plan mode for workers): changes of their own.
- Not measured: a real second machine over a network (latency, how observe ends when ssh
  drops), a Herdr version that differs between the two ends, Claude's MCP and network
  dialogs, multi-question asks, how often Herdr misses a dialog.
- An ACP runtime's answers (section 6).

## For the user to confirm

- Each answer is recorded as an event (`agent.keys`), with the dialog's first line and the
  option's words but no screen, for traceability; the other design kept no record at all.
- An agent waiting on its screen (the runtime missed the dialog) is counted, notified and
  held exactly as a blocked one; the other design showed it as "may be waiting", without the
  mark or a notification.
- One terminal view per page, no tabs; the existing terminal buttons open it, and the
  terminal app moves to "Open in terminal app".

## How the two designs were merged

Design A (the earlier one on this branch) and design B (written independently) agree on: no
terminal by default; the gate in Needs you's Blocking, opened from the row, the task, Agents
and a notification; the screen beside the buttons as plain text; a numbered option by its
digit whatever is selected; only advertised supported keys as hint buttons, the generic keys
always there; no text entry; a fresh screen compared before any key; a read after each key
and the new screen when the dialog is still there; the runtime's blocked state first, its
rule and evidence on the gate; the poll kept; a status subscription per pane, reviewed on
its own; `read_place` not reused; the observe child on demand, killed on close; the second
`-L` for the client socket; xterm.js with the DOM renderer, read-only, never resized;
`App::user` for all three routes, none in `machine_router`; screens hidden from blind views.

| Point | A | B | Taken | Why |
|---|---|---|---|---|
| Dialogs the runtime misses | one parser for every harness, on idle/done | match the two known Codex dialogs only, Codex only | A | no harness is named in code; the parser's "options and a key hint at the bottom" is B's narrow match without the names |
| Check before typing a message | the first message of a started agent | every message to an idle/done agent | B | the same read closes the gap for every delivery, not just the first |
| How a missed one is shown | counted and notified as blocked | labelled, no mark, no notification | A (to confirm) | the agent is as stuck as a blocked one; i33 is about attention |
| What binds a press to the screen | the screen's digest, one press at a time | a single-use grant per gate episode, a gate ID | A | the digest already binds the press to what was read and refuses a replay (the screen has changed); a grant adds state and checks nothing more |
| Serializing writes | presses per agent | every write per terminal, deliveries too | B | a message typed into a half-answered dialog is the same harm |
| Agent identity | session and machine, fresh | session, machine and a terminal generation | A, resolved in the runtime | session and machine are workriver's identity of a live agent; the runtime resolves it at the call, as `send_to` does; the digest catches a replaced screen |
| Marked list | one click; the service steps, reads after each, then Enter | one step per click; the user presses Enter | A, with B's check of the option's words before Enter | the service makes the same check the user would, against the same race; an option stays one button; the generic keys stay for doing it by hand |
| "Answered" | dialog gone or not blocked | dialog gone and not blocked | B | "or" would call a missed dialog answered while it is still there |
| Lost or failed send | not said | "unverified", never retried | B | a retry could answer twice |
| Failed screen read in the gate | not said | keys disabled | B | a failed read is never replaced by a value |
| Recording an answer | `agent.keys` with the region | nothing in the store or logs | A, smaller (to confirm) | traceability is workriver's purpose; the region can carry a secret, so only the question line and the option's words |
| Question line in the Blocking row | in the snapshot | no screen text in the snapshot | B | agents read the snapshot too; the gate reads the screen when opened |
| Terminal views | a dock of up to 4 tabs, one streams | one per page | B | one view is simpler; reopening costs one 5–10 KB frame |
| Stream bounds | a cap of open views | also frame size, queue, a slow reader ended, restart from a whole frame | both | a stalled page must not hold Herdr's output in the service |
| Missing client socket | not said | error in the view, gate still works | B | the gate needs only reads and keys |
| xterm side effects | read-only | also no links, no clipboard | B | a screen can contain anything |
| Origin, caching | not said | page origin for writes, no cross-origin reads, no-store | B, mostly existing | the guard already checks Host and Origin and sends no CORS; `no-store` is added |
| Rail | the unit's mark, else a workspace line of a new rail | rolled up through every ancestor, workspace when no intent | where Blocking counts today | the rail of workspaces is not on main; this design does not add it |
| Watch reopening | on a pane detected or closed | reconciled with the 2 s inventory | A | the poll stays the source of state, so a missed subscription costs one tick |
| Key set | ↑ ↓ ↵ Esc Tab 1–9 | ↑ ↓ ↵ Esc 1–9 | A | Claude's dialogs advertise Tab |
