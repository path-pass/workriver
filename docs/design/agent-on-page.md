# Answering a stuck agent from the page

Design of this branch. Before it is merged its rules go into `docs/design/ui/navigation.md`
(Needs you, Agents), `docs/design/ui/operations.md` (Opening a terminal),
`docs/design/layout.md` and `docs/security.md`, and this file is removed.

What it builds on was measured with Herdr 0.9.5 (protocol 22), Claude Code 2.1.330 and
codex-cli 0.171.0: Herdr works headless; `pane.agent_status_changed` must be subscribed per
pane; `terminal session observe` reaches a pane through `herdr-client.sock`, not the API
socket; a watched idle pane costs nothing, a watched working pane about 3 % of one core in
Herdr's server and 8.7 MB per observe process; a page keeps about 16 WebGL contexts; Herdr
saw all six Claude dialogs as blocked and missed three of six Codex ones ("Hooks need
review" reported idle, "Implement this plan?" done); a list under `❯`/`›` wraps around;
a digit submits in most dialogs, only moves the selection in one (an option that is a text
field) and in another submits its own option whatever is selected.

## Decisions

1. The terminal is not shown by default. A stuck agent becomes a **gate** in Needs you: the
   dialog's text and buttons to answer it. A **terminal view** opens only when the user asks,
   one shown at a time, read-only.
2. The service, never the page, reads screens, parses them and sends keys. The page sends
   "press this" with the text it showed; the service checks the screen still says that.
3. Sending keys is the user's act: `App::user`, never on a machine's listener, recorded.
4. The runtime keeps being read every 2 s (the poll stays the source of every state); the
   new subscription only wakes the poll early. A missed event costs at most one tick.
5. Where Herdr misses a dialog, the service still sees it on the screen of an agent that has
   just gone idle or done, with the same parser, and shows it as waiting.

## 1. Noticing a stuck agent

- **Herdr says blocked.** The poll already turns that into a Blocking row (`blocked` in the
  service, how long it has waited). The subscription (section 6) makes the next poll run at
  once, so the row comes within about half a second instead of up to two.
- **Herdr misses it.** When an agent's state becomes idle or done (or is first seen so), the
  service reads its visible screen once then and once a tick later (a dialog may be drawn
  after the state changed), and parses it (section 2). A dialog found makes the agent
  **waiting on its screen**: shown and counted exactly as blocked, its deliveries held as
  for a blocked agent (so a prompt is never typed into a dialog), with one quiet line on the
  gate: "The runtime reports it idle; its screen shows a question." While it is so marked
  its screen is read every tick; the mark goes when the dialog is gone or the runtime
  reports another state. An agent idle or done for longer is not read again. In memory
  only, like `blocked` and `idle_since`. An agent the service has just started (a question
  agent) is read the same way before its first message is typed, since a start can report
  success over such a dialog.
- The runtime's reason goes with the gate: `agent.explain`'s rule and evidence, read once
  when the agent becomes blocked, shown quietly ("bash_permission_prompt"); none for one
  waiting on its screen.

## 2. Reading a dialog off a screen

One parser, in core (`workriver-core/src/screens/`: pure, no I/O, no UI text, no harness
names), from a screen's visible text to a `Dialog` or none:

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
  waiting-on-its-screen check it also needs a hint that names a key: an idle screen must
  look like a dialog in both ways to be called one.
- **Keys** the service will ever send: Up, Down, Enter, Escape, Tab, digits 1–9. Nothing
  else, no text: a screen can say anything, and the page has no other way to type.

What the parser cannot know stays visible instead of guessed: an option that is a text
field ("Type something.", "Tell … what to change") is a button like any other; after it
the dialog is still there, with the selection on that field, and the gate says so.

## 3. Sending keys

`POST /api/agent/keys` `{ agent, machine, shown, press }`: `shown` is the digest of the
region the page showed (the whole visible text when no dialog was parsed); `press` is an
option (`{ option: 2 }`) or one key (`{ key: "escape" }`). The handler:

1. `App::user`; the agent found by session and machine in a fresh read (its place is
   never kept from earlier); not blocked and not waiting on its screen: refused, nothing
   sent ("It is no longer waiting").
2. Reads the screen. Its digest differs from `shown`: refused, nothing sent, the new
   screen returned ("The screen changed; look again").
3. Sends:
   - a numbered option: its digit;
   - a marked list: one Up or Down toward the option, then reads again, and goes on only
     when the marker sits on the next line expected (the list wraps, so it never counts
     steps ahead), at most as many steps as there are options; on the option, Enter.
     The marker anywhere else stops it: what was sent is said, Enter is not sent;
   - one key: that key.
4. Reads again (every 100 ms, up to 2 s) until the dialog is gone or the runtime reports
   the agent no longer blocked: "Answered". Still there: "Sent; the dialog is still open",
   with the new screen and its new buttons (a digit that only moved the selection).
5. Records `agent.keys` (`Body::user_only`): the agent, its task if it holds one, the
   region shown (quoted text), what was pressed and the keys sent, and the outcome. A
   failed record after keys went is logged, and the answer says the keys went.

One press at a time per agent: a second while one runs is refused.

## 4. The gate

- **Where.** The Blocking row in Needs you ("ja-impl is waiting for you in its terminal")
  now shows the dialog's first line (its question, e.g. "Do you want to proceed?") under the
  agent's name, its topic after it quietly. A click opens the gate (no longer the terminal
  app). The row's terminal button opens the terminal view. The same gate opens from a
  running task's detail ("Waiting for you in its terminal · Answer"), from a blocked line in
  Agents, and from a browser notification of a newly waiting agent.
- **What.** A dialog (Radix, modal, up to 960 px wide so 120 columns fit), top to bottom:
  the agent (mark, name), its task as a link or its workspace, how long it has waited, the
  runtime's reason quietly (or the waiting-on-its-screen line); the region as quoted agent
  text (mono, plain text, never Markdown, links inert), "Whole screen" showing every
  visible line; the options as buttons in their order, each its own words and, as a small
  key cap, what will be sent ("2", or "↓ ↵"); the hint keys ("Esc · cancel"); last, quieter
  and always there, the generic keys ↑ ↓ ↵ Esc Tab 1–9. No dialog parsed: only the screen
  and the generic keys, and "Open terminal view" stands out.
- **After a press.** Buttons wait while it runs. Answered: the gate closes, the row leaves
  by itself. Still open or changed: the gate shows the new screen and its new buttons with
  one line saying which. Refused or failed: the reason in the user's language, the
  service's words in a tooltip.
- **In the record.** An answered dialog shows in its task's detail with the other things
  the user put in: "You answered in its terminal: 2. Yes, and don't ask again", the time,
  the region on demand.
- **On the rail.** A stuck agent counts in the filled violet mark of its unit when it holds
  a task, else of its workspace line in the rail of workspaces of workriver's own (project →
  workspace → intent → unit), as Needs you's Blocking already counts.

## 5. The terminal view

- **Opening.** The terminal button in Agents, in a Blocking row, in a running task's
  header, and "Open terminal view" in the gate. Today these bring the terminal app to the
  front; that stays as "Open in terminal app" in the view's header.
- **Where.** A dock at the bottom of the main area, absent until a view is opened: a row of
  tabs (the agent's mark and name, its state, ×), "Open in terminal app", fold; its height
  dragged with `useResize` (remembered). At most four tabs; a fifth replaces the one looked
  at least recently. On a narrow screen it is a sheet.
- **One stream, the shown one.** Only the tab shown streams. Another tab, the dock folded,
  the page hidden (`visibilitychange`) or the tab closed ends its stream; showing it again
  opens a new one, whose first frame is the whole screen (5–10 KB). So the page never holds
  more than one stream per browser tab, and Herdr pays 3 % of a core only for what is
  looked at.
- **Drawing.** xterm.js (`@xterm/xterm`), loaded only when a view first opens, DOM renderer
  (one terminal shown: 1.4 % of the main thread in the measurement; no WebGL addon, no
  context limit to meet). Columns and rows are the pane's (from the frames); the font scales
  so the width fits, down to 9 px, then the view scrolls sideways. The pane is never
  resized. Read-only: `disableStdin`, no key reaches the pane; while its agent waits, the
  gate's buttons sit under the view (the same component, the same request), for when only
  the terminal shows what to press.
- **Gone.** An agent that leaves, or a machine that cannot be reached: the view keeps its
  last frame, greyed, with one line why.

## 6. What the service gives, and the runtime under it

Routes, all `App::user`, none in `machine_router`; agents read no screen of another agent
through workriver (a blind reviewer could otherwise read the work it must not see):

| Route | Gives |
|---|---|
| `GET /api/agent/screen?agent=&machine=` | the visible text, its size, its digest, the parsed `Dialog`, the runtime's reason |
| `POST /api/agent/keys` | section 3 |
| `GET /api/agent/view?agent=&machine=` | Server-Sent Events, one per frame: `{ full, cols, rows, seq, bytes }` (bytes base64, as Herdr gives them); ends when the pane goes |

The state of agents keeps reaching the page as today (`/api/events` says "changed", the page
reads the snapshot); blocked and waiting-on-its-screen agents carry their dialog's first
line in it, the gate reads the rest with `/screen`.

Open views are capped in the service (4 in `config/defaults`, across every page); one more
is refused with the reason. One observe child per open view, not shared: the first frame
must be whole for each viewer, and the cap keeps it to 35 MB.

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
/// What a place shows now, as plain text, one line per row, with its size.
fn screen(&self, machine: Option<&str>, place: &str) -> Result<Screen, RuntimeError>;
/// Presses keys at a place, in order; no text.
fn press(&self, machine: Option<&str>, place: &str, keys: &[Key]) -> Result<(), RuntimeError>;
/// The place's terminal as frames, the first one whole; dropping it stops it.
fn observe(&self, machine: Option<&str>, place: &str) -> Result<Box<dyn Iterator<Item = Result<Frame, RuntimeError>> + Send>, RuntimeError>;
/// Why the runtime calls the agent at a place blocked, in its words (a rule and its evidence).
fn why_blocked(&self, machine: Option<&str>, place: &str) -> Result<Option<String>, RuntimeError>;
```

`Key` (Up, Down, Enter, Escape, Tab, Digit(1..=9)), `Screen` and `Frame` are core's.
`read_place` stays as it is (the last lines of a run's shell).

The service runs one `watch` per machine on a thread of its own (this machine's from
`App`, the others' in `follow.rs` next to their reads), calls it again 5 s after it ends,
and on each change wakes the poll for that machine (coalesced: one poll per tick at most
early). The trait's first method that pushes; it is reviewed on its own.

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
  (`terminal.frame`); dropping the iterator kills and reaps the child.
- **Other machines:** observe derives `<socket without .sock>-client.sock`. `ssh.rs` adds a
  second `-L` in the same ssh: `herdr-<label>-client.sock` here to the remote API socket's
  sibling `-client.sock` there. Both paths stay under the 104-byte limit in the data
  directory.

**Another runtime** (one speaking ACP, for harnesses that are not run in a terminal): its
client receives the agent's permission requests itself, so `watch` reports Blocked when one
arrives and stops reporting when it is answered; it has no screen, keys or frames
(`screen`, `press`, `observe` stay `None`). Its gate would come from the request's own
options, not from the parser: a sixth method (answer an option by its id) is added then, not
now. The gate's page part does not change: it shows a `Dialog`, whoever made it.

## 7. Security

- The three routes are the user's only (`App::user`); none in `machine_router`; an agent's
  proof is refused there whatever it names.
- What may be sent is the fixed key set; the request names an option by its number or a key
  by its name, never text. The service sends only after the screen it reads matches what the
  user saw, and reads again after each arrow.
- The screen's text and the options' words are another's writing: the page shows them as
  plain text (quoted agent text, as everywhere); they never go into a message to an agent;
  in the record they are quoted.
- Observe children: one per open view, capped, killed with their stream; the child gets no
  more environment than it needs.

## 8. Tests (the rules as tests)

Service, with the fake runtime (`server/tests/`):

1. `/api/agent/keys`, `/screen`, `/view` refuse an agent's proof and a caller with none;
   they are not in `machine_router` (the guard that lists its routes).
2. A screen that differs from `shown`: refused, `press` never called.
3. A marked list that wraps: one key per step, a read after each; the marker moving
   anywhere but the next line stops it without Enter; no more steps than options.
4. A digit after which the dialog is still there: "still open", with the new screen.
5. An agent not blocked: refused; two presses at once: the second refused.
6. A key outside the set, or text: refused (400), nothing sent.
7. An idle agent whose screen shows a dialog with hints: waiting, held deliveries; an idle
   screen with the same dialog text above an input box: not; the mark goes when the screen
   clears.
8. `watch` reporting a change: the poll runs before the next tick; `watch` failing: the
   poll goes on at its pace and `watch` is called again.
9. A view whose stream is dropped: its child is gone; the cap's next view refused.
10. `agent.keys` is `user_only` (the event rules' table test).

Core: the parser against each dialog the measurement saw (Claude: trust, Bash, Write,
question, plan; Codex: hooks review, command, edit, plan, question) as fixtures in
`workriver-core/src/screens/__fixtures__/` (the screens as captured, paths of the machine
they came from replaced), and against idle screens with a dialog quoted in the transcript.

Adapter: the `-L` pair in `ssh.rs`'s arguments; `observe`'s child environment.

Page: the screen shown as text (Markdown, a link, an escape sequence stay inert); a press
disabled while one runs; the dock streams only the shown tab and closes it on
`visibilitychange`; every string in en, zh-CN and ja.

## Not done

- Typing text into an option that is a text field; such an answer is given in the terminal.
- More than one terminal shown at once, and typing into the terminal view.
- Telling Herdr of the two Codex dialogs it misses, and starting workers so they do not
  meet them (hooks trusted, no plan mode for workers): changes of their own.
- Not measured: a real second machine over a network (latency, how observe ends when ssh
  drops), a Herdr version that differs between the two ends, Claude's MCP and network
  dialogs, multi-question asks, how often Herdr misses a dialog.
- An ACP runtime's answers (section 6).
