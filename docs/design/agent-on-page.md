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
   "press this" with the digest the service gave for the screen it showed (the whole visible
   screen, bound to the presses already sent); the service checks the screen still says
   that, and sends nothing when it does not. Every read and key of one request goes to one
   place, resolved once.
3. Sending keys is the user's act: `App::user`, never on a machine's listener, recorded.
4. The runtime keeps being read every 2 s (the poll stays the source of every state); the
   new subscription only wakes the poll early. A missed event costs at most one tick.
5. Where the runtime misses a dialog, the service still sees it on the screen, with the same
   parser and for every harness alike: of an agent that has just gone idle or done, and of
   any agent workriver is about to type a message into.
6. No screen text leaves the gate's own routes: not in the snapshot, the change stream, the
   logs or browser storage. The one exception is the event store: an answer keeps its
   question line and its option's words, cut as section 3 step 7 says, and agents can read
   them.

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
- **The read and the message reach one place.** The read before a message and the message
  are one step under the agent's write lock (section 3): the recipient is resolved once
  (`resolve`, section 6: exactly one place shows that session on its machine, else not
  delivered, with the reason), its screen is read at that place, and the message is typed at
  that same place (`send_at`). Such messages go by session only: a recipient known by a name
  is turned into its session in the same fresh read, exactly one agent by that name, and
  never through the adapter's cached session-to-place map (`pane_of`). Today `App::send`
  goes by session or name through that cache (progress, record and questions), so its
  callers move to this step.
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
  most twelve. "Whole screen" shows the rest. When the twelve cut it (the rule was not
  reached), the `Dialog` says so, and the gate opens with the whole screen shown and a line
  saying the dialog is longer than its region: a long command must be seen whole before it
  is approved.
- **A dialog** needs options (numbered, or two or more with one marked). For the
  waiting-on-its-screen check (section 1) it also needs a hint that names a key: a screen
  the runtime does not call blocked must look like a dialog in both ways to be called one.
- **Keys** the service will ever send: Up, Down, Enter, Escape, Tab, digits 1–9. Nothing
  else, no text: a screen can say anything, and the page has no other way to type.
- **Marks that reorder or hide text.** Every piece of screen text the service gives the
  page (the screen, the region, the options' words, the hints) or records goes through one
  core function, `screens::marked`: U+061C, U+200B–U+200F, U+202A–U+202E, U+2060–U+2064,
  U+2066–U+2069 and U+FEFF each become a visible `⟨U+202E⟩` (its code point). They are never
  dropped: dropping also changes what the user approves. The parser and the digest work on
  the screen as read, not as marked.

What the parser cannot know stays visible instead of guessed: an option that is a text
field ("Type something.", "Tell … what to change") is a button like any other; after it
the dialog is still there, with the selection on that field, and the gate says so.

## 3. Sending keys

`POST /api/agent/keys` `{ agent, machine, shown, press }`: `agent` is the agent's session;
`shown` is the digest `/api/agent/screen` gave with the screen the page showed; `press` is
an option (`{ option: 2 }`) or one key (`{ key: "escape" }`).

**The digest** is SHA-256 over three things: a key the service draws at random when it
starts, the number of presses sent to that agent since then, and the whole visible screen as
read (not the region: a long command cut at twelve lines differs above them). The count goes
up by one for every request that sent a key, or may have (a failed or lost send). So a
request made before a press (a second click, a retried request, a stale gate) never matches
after it, even when the next dialog looks exactly the same (the same command asked twice);
a fresh `/screen` of that next dialog gives the new digest. The key and the counts are in
memory only; after a restart no earlier digest matches. A line that changes by itself while
a dialog waits (a clock in a status line) would refuse every press with "The screen changed";
that fails safe. Whether the measured dialogs hold still while they wait is checked by
hand (section 8) before this is built on.

The handler:

1. `App::user`; the request names the agent by session and machine, never by a name. Not
   blocked and not waiting on its screen: refused, nothing sent ("It is no longer waiting").
2. Takes the agent's write lock: one writer per agent at a time, shared with message
   delivery (section 1), so a press and a message never interleave. A second press while one
   runs: refused.
3. Resolves the session to its place once (`resolve`, section 6): exactly one place on that
   machine shows that session, else refused, nothing sent (none: "It is gone"; two: "It shows
   in two places; answer it in its terminal"). Every read and every key of this request goes
   to that place and no other; the request never resolves again, so a second place that
   starts showing the session meanwhile gets nothing.
4. Reads the screen there. Its digest differs from `shown`: refused, nothing sent, the new
   screen returned ("The screen changed; look again").
5. Sends:
   - a numbered option: its digit, whatever is selected;
   - a marked list: one Up or Down toward the option, then reads again, and goes on only
     when the marker sits on the next line expected (the list wraps, so it never counts
     steps ahead) **and** nothing else changed: the screen with the marker characters at the
     start of lines blanked is the same as before the step. At most as many steps as there
     are options; Enter only when the marked line's words are the chosen option's words. Any
     other change stops it (a dialog replaced by another with options at the same rows): what
     was sent is said, Enter is not sent;
   - one key: that key.
6. Reads again (every 100 ms, up to 2 s) until the dialog is gone **and** the runtime does
   not report the agent blocked: "Answered". Another dialog there: "A new question", with
   its screen and buttons. The same dialog still there: "Sent; the dialog is still open",
   with the new screen and its new buttons (a digit that only moved the selection). A send
   that failed or whose answer was lost: "It may have been sent; look again", with a fresh
   screen when one can be read. Nothing is ever sent again by itself.
7. Records `agent.keys` (`Body::user_only`): the agent, its task if it holds one, the keys
   sent, the outcome, and two pieces of the screen, exactly:
   - **the question line**: the nearest non-empty line above the first option, kept only
     when it ends with `?` (a question, not a command or a path that some dialogs put
     there); else none;
   - **the option**: its number and its words.

   Each piece is cut at the first backtick, `"`, `'` that does not stand between two
   letters, `$`, `=`, `/`, `~` or `\` (where a command's arguments, an assignment, a quoted
   header or a path begin), capped (the line at 120 characters, the words at 40), on one
   line with control characters as spaces, and marked (section 2). Nothing else of the
   screen, not the region. The record is kept for good and agents can read it (task detail
   goes through `App::view` like every record). The cut keeps out where a secret usually
   sits, but a secret with none of those characters before it, inside the cap, would be
   kept. Whether to keep even this, or only the option's number and the outcome, is the
   user's to confirm (below). A failed record after keys went is logged, and the answer says
   the keys went.

**What stays open.** Between the last read and the key there are a few milliseconds (about
8 ms measured from a key to the screen showing it) in which the dialog can change and the
key still go. workriver cannot close that: Herdr has no send-if-unchanged (`pane.send_keys`
takes no condition on what the pane shows). It is kept to one read followed at once by one
key, under the write lock, and the read after the key says what the key did.

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
  every visible line (shown from the start, with a line why, when the region was cut,
  section 2); the options as buttons in their order, each its own words and, as a
  small key cap, what will be sent ("2", or "↓ ↵"); the hint keys ("Esc · cancel"); last,
  quieter and always there, the generic keys ↑ ↓ ↵ Esc Tab 1–9, one key each, for doing it
  by hand. No dialog parsed: only the screen and the generic keys, and "Open terminal view"
  stands out.
- **In the order a terminal draws it.** A browser reorders text by the Unicode bidi
  algorithm and a terminal does not, so a command could read one way in the gate and run
  another. The region, the whole screen, the option buttons and the recorded line are drawn
  with `direction: ltr; unicode-bidi: bidi-override` (characters left to right in the order
  they were written, as a terminal draws them), and the marks that reorder or hide text are
  visible (section 2). One shared component in `content/` does this for all four.
- **After a press.** Buttons wait while it runs. Answered: the gate closes, the row leaves
  by itself. Still open, a new question, or changed: the gate shows the new screen and its
  new buttons with one line saying which. Refused or failed: the reason in the user's
  language, the service's words in a tooltip.
- **In the record.** An answered dialog shows in its task's detail with the other things
  the user put in: "You answered in its terminal: Do you want to proceed? — 2. Yes, and
  don't ask again for", the time (the option's words cut at its backtick, section 3 step 7).
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
  resized. Read-only: `disableStdin`, no key reaches the pane; no clipboard write from the
  terminal (OSC 52 ignored). No links: no link addon, and since xterm.js makes OSC 8
  hyperlinks in its core (6.0.0 `OscLinkProvider`: with no handler a click calls `confirm()`
  and then `window.open()` to the agent's URI), the Terminal is created with
  `linkHandler: { activate: () => {}, allowNonHttpProtocols: false }`, so a click on one does
  nothing and shows nothing. Nothing it draws acts outside it. While its
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

The two GETs do something (`/view` starts an observe child and takes one of the capped
slots; `/screen` reads the pane), and the guard lets a GET through on the session cookie
alone, without an Origin. SameSite=Strict does not stop a page on another port of the same
host (`http://127.0.0.1:3000` is the same site as the service): its requests carry the
cookie. So both refuse a request that came with the session cookie unless its
`Sec-Fetch-Site` is `same-origin` (a missing header is refused too; every browser the page
supports sends it, on `fetch` and on `EventSource`). Such a page could not read the answers
(no CORS), but it could hold all four views. A request with the user's bearer token (the
CLI) sends no such header and is not affected: no page can add that token.

The state of agents keeps reaching the page as today (`/api/events` says "changed", the page
reads the snapshot). The snapshot says an agent is blocked or waiting on its screen, and
since when; never what its screen says. The gate reads that with `/screen`.

Open views are capped in the service (4 in `config/defaults`, across every page); one more
is refused with the reason. One observe child per open view, not shared: the first frame
must be whole for each viewer, and the cap keeps it to 35 MB. Each stream has a bounded
queue (in `config/defaults`) and a frame larger than a bound is not passed on; a reader that
falls behind has its stream ended (the page reopens from a whole frame).

**The `Runtime` trait** grows by seven methods, each with a default that cannot
(`RuntimeError::None`), so a runtime without them still runs. Everything that reads or
presses takes a place, never a session: a request resolves its session once and then works
on that one place (section 3 step 3), so its reads and its keys cannot reach two places.

```rust
/// A change of an agent's state, as the runtime tells it when it happens.
pub struct Change { pub place: String, pub state: AgentState }

/// Blocks, telling `to` each change of an agent's state on `machine` as it happens, until
/// `to` returns false (Ok) or the connection ends (the error). Only a wake-up: states are
/// still read with `agents`; a change missed before it started or while it reconnects is
/// seen by the next read.
fn watch(&self, machine: Option<&str>, to: &mut dyn FnMut(Change) -> bool) -> Result<(), RuntimeError>;
/// The one place on `machine` that shows this session now, read fresh: `NoAgent` when none
/// does, `Ambiguous` when more than one does; never an agent by its name, never a place
/// kept from an earlier read.
fn resolve(&self, machine: Option<&str>, session: &str) -> Result<String, RuntimeError>;
/// What that place shows now, as plain text, one line per row, with its size.
fn screen(&self, machine: Option<&str>, place: &str) -> Result<Screen, RuntimeError>;
/// Presses keys at that place, in order; no text.
fn press(&self, machine: Option<&str>, place: &str, keys: &[Key]) -> Result<(), RuntimeError>;
/// Types a message at that place and submits it, as `send_to` does for a session.
fn send_at(&self, machine: Option<&str>, place: &str, text: &str) -> Result<(), RuntimeError>;
/// That place's terminal as frames, the first one whole; dropping it stops it.
fn observe(&self, machine: Option<&str>, place: &str) -> Result<Box<dyn Iterator<Item = Result<Frame, RuntimeError>> + Send>, RuntimeError>;
/// Why the runtime calls the agent at that place blocked, in its words (a rule and its
/// evidence).
fn why_blocked(&self, machine: Option<&str>, place: &str) -> Result<Option<String>, RuntimeError>;
```

`resolve` is where a session becomes a place, and the only one for these requests: today
`send_to` takes the first pane that shows the session (`find`) and does not refuse two, and
any program may report a session for its pane (a resumed session shows in two). `Ambiguous`
is a new `RuntimeError` (refused, nothing sent). A place is the runtime's opaque word, as in
`locate` and `read_place`. `/screen` and `/view` resolve at their call too, and `/view`
streams the place it resolved until it closes. `Key` (Up, Down, Enter, Escape, Tab,
Digit(1..=9)), `Screen` and `Frame` are core's. `read_place` stays as it is (the last lines
of a run's shell, not the visible screen).

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
- `resolve`: one `agent.list` read, the panes whose agent has that session; not the
  `pane_of` cache. `screen`: `pane.read`, source `visible`, format `text`. `press`:
  `pane.send_keys`, the keys in Herdr's names. `send_at`: `agent.prompt` at that pane.
  `why_blocked`: `agent.explain`.
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
(`screen`, `press`, `send_at`, `observe` stay `None`). Its gate would come from the request's own
options, not from the parser: a sixth method (answer an option by its id) is added then, not
now. The gate's page part does not change: it shows a `Dialog`, whoever made it.

## 7. Security

- The three routes are the user's only (`App::user`); none in `machine_router`; an agent's
  proof is refused there whatever it names. The guard's Host and Origin checks apply as to
  every route (a POST only from our origin); the service sends no CORS headers, so another
  origin cannot read `/screen` or `/view`; and those two GETs, which do something, refuse a
  cookie request whose `Sec-Fetch-Site` is not `same-origin` (section 6), so a page on
  another local port cannot start views.
- What may be sent is the fixed key set; the request names an option by its number or a key
  by its name, never text, a sequence of keys, a size or a paste. One request resolves its
  agent to one place, once, and reads and presses only there. The service sends only after
  the whole screen it reads there matches what the user saw (the digest, which also counts
  the presses already sent, so no request goes twice), checks the whole screen again after
  each arrow, and never repeats a send by itself. A few milliseconds between the last read
  and the key remain and cannot be closed (section 3, "What stays open").
- The read before a message and the message reach the same place, under the same lock
  (section 1).
- The screen's text and the options' words are another's writing: the page shows them as
  plain text (quoted agent text, as everywhere), in the order a terminal draws them, with
  the marks that reorder or hide text made visible (sections 2 and 4), and ANSI only inside
  the terminal view, where links do nothing; they never go into a message to an agent.
  Screen text and frames are kept out of the snapshot, the logs and browser storage. The
  event store keeps, for each answer, the question line and the option's words, cut and
  capped (section 3 step 7); agents can read them. When this is built, `docs/security.md`
  says exactly that, under what the event store holds.
- Observe children: one per open view, capped, killed with their stream; the child gets no
  more environment than it needs.

## 8. Tests (the rules as tests)

Service, with the fake runtime (`server/tests/`):

1. `/api/agent/keys`, `/screen`, `/view` refuse an agent's proof (a blind one too) and a
   caller with none; they are not in `machine_router` (the guard that lists its routes); a
   POST from another origin is refused; `/screen` and `/view` with the session cookie and
   `Sec-Fetch-Site: same-site` (or none) are refused and start no observe; with the bearer
   token they are not.
2. A screen that differs from `shown`: refused, `press` never called; a change above the
   region (a long command's first lines) counts. The same request sent twice: the second
   refused, one press. The same dialog drawn again after an answer (the same command asked
   twice): the earlier request's digest refused, a fresh `/screen`'s accepted.
3. A marked list that wraps: one key per step, a read after each; the marker moving
   anywhere but the next line, landing on a line whose words are not the option's, or any
   other line of the screen changing between steps, stops it without Enter; no more steps
   than options. A numbered option: its digit alone, whatever is selected.
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
12. A second place that starts showing the session between the screen read and the press
    gets no key, and every read of that request is of the first place; two places showing
    it when the request starts: refused, nothing sent. The same for the read before a
    message and the message: one place, `send_at` there.
13. `agent.keys` records the question line only when it ends with `?`, and both pieces cut
    at a backtick, a quote, `$`, `=`, `/`, `~` or `\`, and capped; a dialog whose option
    reads ``Yes, and don't ask again for `curl -H …` commands in /path`` records
    "Yes, and don't ask again for".

Core: the parser against each dialog the measurement saw (Claude: trust, Bash, Write,
question, plan; Codex: hooks review, command, edit, plan, question) as fixtures in
`workriver-core/src/screens/__fixtures__/` (the screens as captured, paths of the machine
they came from replaced), and against idle screens with a dialog quoted in the transcript;
a dialog whose region the twelve lines cut says so; `screens::marked` turns each listed mark
into its visible form and keeps the order of everything else.

Adapter: the `-L` pair in `ssh.rs`'s arguments; `observe`'s child environment. By hand, on
an isolated headless Herdr: a whole frame then deltas, the child gone after a close, a
missing client socket, both sockets through ssh; each measured dialog's whole screen read
every second for 30 s while it waits, unchanged.

Page: the screen shown as text (Markdown, a link, an escape sequence stay inert); a screen
with U+202E (and one with right-to-left letters) shows the mark and keeps its written order,
in the region, the whole screen, the buttons and the record; a cut region opens with the
whole screen; a press disabled while one runs; a failed screen read disables the keys; the
dock streams only while shown and ends it on `visibilitychange`; the terminal view writes no
clipboard, and a frame with an OSC 8 link opens nothing and shows no dialog when the link is
clicked; every string in en, zh-CN and ja.

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

- Each answer is recorded as an event (`agent.keys`), for traceability: the keys, the
  outcome, the question line (only one that ends with `?`) and the option's number and words,
  both cut where a command, a value or a path begins and capped (section 3 step 7); no
  screen. It is kept for good and agents can read it; a secret with none of the cut
  characters before it would be kept too. The other choices: keep only the option's number
  and the outcome, or (the other design) no record at all.
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
| What binds a press to the screen | the screen's digest, one press at a time | a single-use grant per gate episode, a gate ID | A, made single-use (after the security review) | the digest is of the whole screen and counts the presses already sent, so it is B's single use without a grant or a gate ID: a replay, or a request for a dialog that looks the same as one answered, never matches |
| Serializing writes | presses per agent | every write per terminal, deliveries too | B | a message typed into a half-answered dialog is the same harm |
| Agent identity | session and machine, fresh | session, machine and a terminal generation | A, resolved once per request (after the security review) | session and machine are workriver's identity of a live agent; `resolve` turns it into exactly one place once, and every read and key of the request (or the read before a message and the message) goes there; resolving at each call, as `send_to` does, could read one pane and press another |
| Marked list | one click; the service steps, reads after each, then Enter | one step per click; the user presses Enter | A, with B's check of the option's words before Enter | the service makes the same check the user would, against the same race; an option stays one button; the generic keys stay for doing it by hand |
| "Answered" | dialog gone or not blocked | dialog gone and not blocked | B | "or" would call a missed dialog answered while it is still there |
| Lost or failed send | not said | "unverified", never retried | B | a retry could answer twice |
| Failed screen read in the gate | not said | keys disabled | B | a failed read is never replaced by a value |
| Recording an answer | `agent.keys` with the region | nothing in the store or logs | A, smaller (to confirm) | traceability is workriver's purpose; the region can carry a secret, so only a question line ending with `?` and the option's number and words, cut where a command, a value or a path begins (after the security review) |
| Question line in the Blocking row | in the snapshot | no screen text in the snapshot | B | agents read the snapshot too; the gate reads the screen when opened |
| Terminal views | a dock of up to 4 tabs, one streams | one per page | B | one view is simpler; reopening costs one 5–10 KB frame |
| Stream bounds | a cap of open views | also frame size, queue, a slow reader ended, restart from a whole frame | both | a stalled page must not hold Herdr's output in the service |
| Missing client socket | not said | error in the view, gate still works | B | the gate needs only reads and keys |
| xterm side effects | read-only | also no links, no clipboard | B | a screen can contain anything; xterm makes OSC 8 links in its core, so a `linkHandler` that does nothing (after the security review) |
| Origin, caching | not said | page origin for writes, no cross-origin reads, no-store | B, mostly existing | the guard already checks Host and Origin and sends no CORS; `no-store` is added; the two GETs that do something also need `Sec-Fetch-Site: same-origin` (after the security review) |
| Rail | the unit's mark, else a workspace line of a new rail | rolled up through every ancestor, workspace when no intent | where Blocking counts today | the rail of workspaces is not on main; this design does not add it |
| Watch reopening | on a pane detected or closed | reconciled with the 2 s inventory | A | the poll stays the source of state, so a missed subscription costs one tick |
| Key set | ↑ ↓ ↵ Esc Tab 1–9 | ↑ ↓ ↵ Esc 1–9 | A | Claude's dialogs advertise Tab |
