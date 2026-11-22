# Security model

workriver runs on your machine, for you. It records what agents do and types your
decisions into their terminals, so the question it must answer is: **was that really you?**

Agents run in the terminals of an agent runtime (today Herdr; what is Herdr's own here is
in [runtimes/herdr.md](runtimes/herdr.md#security-under-herdr)). "An agent's
terminal" below is the one the runtime runs that agent in.

## Who is who

- **You** are whoever holds the *user token*: a file only your account can read,
  `~/workriver-data/user-token` (made at the first start, mode 600).
  - The CLI sends it with every command, and the token alone makes nobody you: the service
    also looks at **the process that sends the command** (next section). You are a command
    with the token that runs in a terminal of the runtime with no agent, or in a terminal
    window outside the runtime. In an agent's terminal a command is that agent's, token or not: a manager that runs
    `workriver machine add` is refused. A request with the token that shows no process (a
    `curl`, a script) is nobody. (See the limits below for a program that sets out to get
    around it.)
  - The web UI gets a session for it: `workriver open` hands the browser a link that
    works once and for five minutes, without showing it (nothing that signs in is ever
    printed by the service either: agents can read the runtime's terminals; run `workriver open`,
    and above all `workriver open --print`, whose link lasts a minute, in a terminal agents
    do not read); the page exchanges it for a cookie that scripts cannot
    read (HttpOnly) and that is not sent with requests from other sites (SameSite=Strict).
    The cookie is a random value the service hands out for the code and nothing else makes:
    the service keeps only its hash (`~/workriver-data/user-sessions`, mode 600), so
    neither the token nor that file yields a session. Requests that change something must
    also come from workriver's own page (Origin check). Without a session the page shows
    only how to open it.
- **An agent** is whoever runs in an agent's terminal: the service finds which terminal the
  command's process really runs in, and takes the agent the runtime shows there. What the
  command says about its terminal is only cross-checked. In an agent's terminal a command is
  always that agent's, even if it can read the token. There is no `--as user`.
- **Another machine** you follow can report and ask, never act as you. Its agents reach the
  service through a listener of their own that serves an allow-list: who am I, record my
  work, ask and answer, attach and fetch files, read the board and an intent's history. No session, no token, no
  machine settings, no decisions there, whatever the request carries.

## Which terminal a command comes from

The service does not believe the terminal a command names. It reads it from the process:

1. The CLI first connects to a Unix socket of the service, `~/workriver-data/caller.sock`
   (mode 600). The kernel tells the service which process connected; the caller cannot
   choose that. The service reads that process and its parents, up to the first process of
   the system, and answers with a proof that stands for them for 30 seconds; the CLI sends
   it with its requests (a command that lasts longer fetches another).
2. The runtime tells the service the shell at the root of each of its terminals. The command
   is **in the terminal whose shell is one of its parents** (or, for a job left running in
   the background, the one whose tty it still has).
3. Then:
   - the command names a terminal it is not in: **refused**, with both said;
   - it is in an agent's terminal: it is **that agent's**, token or not;
   - it is in a terminal of the runtime with no agent: **yours** with the token, else
     refused;
   - it is in none of the runtime's terminals: **yours** with the token and a tty, else
     refused. A helper process of an agent's program, or a background job, has no tty; you
     at a terminal window have one.
4. **One session, one terminal.** Which agent a terminal holds is what the runtime was told,
   and any program of your account may tell it (Herdr lets it:
   [runtimes/herdr.md](runtimes/herdr.md#security-under-herdr)).
   So the service remembers the terminal each session was first seen in
   (`~/workriver-data/session-roots.json`): while that terminal's shell runs, the same
   session shown by another terminal is refused. Two terminals that show a session nobody
   was seen with before are both refused.

Every failure is a refusal that says why; none makes the caller you or another agent.

What this means for you:

- **An agent's `workriver` commands must run inside its own terminal.** Some agent programs
  hand their commands to a shared background service that was started somewhere else; such
  a command is refused, and the refusal points at the recipe (`workriver guide how
  hand-off`): start that worker so that it runs its commands itself.
- **Your own commands without a terminal, outside the runtime** (cron, launchd, an editor's task
  runner) are refused. Use a terminal or the page.
- **A session resumed in a second terminal** while its first terminal is still open is refused
  until you close the first.
- **`curl` with the token** no longer works; use the CLI or the page.
- The data directory's path must be short enough for a socket (about 100 bytes); the
  service says so at start if it is not.

On another machine there is no service to connect to. The CLI there sends its process id
and the service reads that process's parents on that machine, through the same ssh
connection, against that machine's runtime. That is weaker: the id is the caller's word (the
process must be running and be the `workriver` CLI). It stops a stale or mistaken terminal, not
a program that sets out to cheat; and a caller there is never you in any case.

`WORKRIVER_ALLOW_AS=1` (tests and development only; the service warns at start) does two
things: `--as <agent>` names an agent, never the user, for any local command; and a command
in none of the runtime's terminals needs no tty to be yours. Everything else stays on.

## What only you can do

Decide findings as the user, submit gates, finish chores, do deferred findings together or
drop them, end a conversation, discuss a delivered research or design result with the agent
that did it (`/api/discuss`; only that agent may answer), focus a terminal from the page, read, answer or watch an agent's terminal from the page (below), start a planned intent in a
workspace from the page (`/api/intent/start`), run a command (below), add projects, make,
rename, close and open workspaces, seat a workspace's manager or empty its seat, start a
manager for a workspace (below, "Starting a manager"), move an intent to another workspace
(`/api/workspaces`), add or enable machines, set when the machines alert
(`/api/metrics/thresholds`), set the default harness and model of a start and try them
(`/api/settings/start`).
Reading the machines' CPU, memory and their history (`/api/metrics`, `/api/metrics/history`,
`workriver machine load`) the user and an agent of this machine may; an agent on another
machine reads its own machine's only, never this one's or the others'. The service asks for the token or session on those routes, and
the rules of the event store refuse those events from anyone else as a last line of
defence.

An intent's text is yours, with one owner beside you: its title and goal only you change; its
note its manager also changes while the intent is active; while it is planned, the manager of
a workspace of the intent's project (the project the workspace was made in: what is
recorded, never the folder its terminal is in)
may only add to its note; no other agent changes any of it, and a done or dropped intent, or
dropping a planned one, is yours alone. These are the rules of the event store
(`intent.updated`, recorded through the same route as other records, `App::who`), not of a
route. Every change is kept as a version, recorded as whoever the service found the caller
to be, and you can restore any of them on the intent's page; when you change an active
intent, its manager is told, your words quoted. Its history is read like the rest
(`App::view`); an agent on a blind task cannot read it, since it holds the note.

One of your messages to a manager can be read whole from a terminal (`workriver message
show`), for when the quote typed to the manager was cut. It is read through `App::view` too,
and as narrow: an agent reads a message of the conversation of the workspace whose seat it
holds (by the workspace's id, never its name), or its own copy of one to every manager (with
only its own replies there). Anything else is refused the same way as a message that does
not exist: another workspace's, of the same name or not, and any message for an agent that
manages no workspace. You read every message.

An intent's kind, size and tags follow the same rules, with less for agents: whoever adds an
intent sets them in the same record (an agent's `backlog add`, an import); after that you
change them on any intent, its manager while it is active, and nobody else: a planned, done
or dropped intent's are yours alone (a manager that may add to a planned one's note sets
none of them). They are versions like the title (a change from an older version is refused
when that field changed since). A manager is asked by its instructions, not stopped by a
rule, to leave what you set as it is; the history shows who set what. Merging intents never
widens what an agent may change: an agent's merge of agents' intents keeps the target's
tags as they are (the merged ones' are written into its note); only your merge adds them,
as a new version of its tags. The check of the tags a request gives is bounded: more than
32 are refused before any is read, and an edit is refused for who sends it before its tags
are read, so that no request holds workriver up while it is checked.

Since these rules go by the intent's state and by its workspace, three
facts are held as well: the **project** an intent belongs to, the **workspace** it runs in,
and its **manager**. Each is set only by you, or by a start the rules checked (an intent's
start; for the manager, the end of a start of one that you made). No other
event sets them, no name a request carries, and nothing the agent runtime says: all three
are checked by the event rules against what is recorded, when the event is recorded.

- **A workspace is workriver's own record, made by you.** It has an id workriver gives
  (`ws1`, never given again), a name that is only shown (two may share one), and the project
  it was made in, which never changes. You make, rename, close and open workspaces; adding a
  project makes its first. Nothing the runtime says of its own workspaces (their ids, their
  names, which terminal is in which) identifies one, makes one or changes one: any program
  can rename, create and close those, and the runtime gives a closed one's id again after a
  restart. A terminal's place in the runtime is shown beside an agent, as where its
  terminal is, and decides nothing.
- **Its project** is the registered project that holds the folder recorded when the intent
  was added; nothing changes it afterwards. An intent names a folder that a registered
  project holds, or none, whoever adds it: registering a project is yours, so no agent
  makes a folder a project by naming it (a repository outside every registered project is
  part of none: the agent is told to name a project by its id, or to ask you to add it).
  Its units never make it either: an intent with no project has none for the rules,
  whatever repositories its units are in, so only you start it (it then takes the project
  of the workspace you start it in), and no manager adds to its note while it is planned.
  A unit is added by the intent's manager or by you, by no other agent.
- **Where it runs** is set by a start, or by you when you move an active intent to another
  workspace of its project. An agent adds intents to the roadmap only (planned, in no
  workspace). An intent runs only in a workspace of its project. An active intent is marked
  done, put back in the roadmap or dropped by you or its manager only, and none of the
  three names a workspace: an intent stays recorded where it last ran.
- **A manager starts intents only in the workspace it holds the seat of**, and only intents
  of that workspace's project. Its start names no workspace: the rules fill in the one it
  is seated in, and refuse a start that names another. So no agent starts an intent in
  another workspace, or becomes the manager of another project's intent, by starting it.
- **Its manager** is whoever holds the seat of the workspace it runs in, or last ran in. An
  intent whose workspace's seat is empty has no manager, and you act for it.
- **A manager is seated by you, never by an agent.** A seat is set by your seating, in the
  page (the workspace's line, Manager) or `workriver workspace manager <ws> <agent>` from
  your own terminal, or by the end of a start of a manager that you made on the page (below,
  "Starting a manager"): workriver seats what that start brought up, and the event rules
  accept that only as the end of your start. There is no `workriver manager here`: nobody becomes a
  manager by saying so, by running in some group of the runtime, or by naming a workspace.
  What is kept of a seat is the session, its machine and the folder it was started in (to
  reach its terminal), and the *appointment*: the seating itself. A session holds one seat.
  You may seat over a manager that still runs (the page asks once; that one is told it no
  longer manages). A change of the seat names the seat you were shown, and is refused when
  the seat is no longer that one: a change made meanwhile is never overwritten unseen.
- **The session you seat is checked where it runs.** At the seating the service asks the
  runtime of that session's machine (this one or another alike) which terminal shows it:
  shown in none, or in several, or in another terminal than the one it is known in while
  that one still runs, the seating is refused. The session is then held to that terminal
  ([Which terminal a command comes from](#which-terminal-a-command-comes-from): one session, one terminal).
- **A seat does not move by itself to another session, folder or machine.** While a seated
  session's terminal is gone, the workspace shows "manager not running", and what is sent
  to it is "not delivered". One thing comes back by itself: the same session, on the same
  machine, in the same folder, showing again (a resume, a restart of the runtime) is the
  manager again, with its appointment, and the workspace's conversation gets a line saying
  so. Anything else needs you: another session in the old terminal, or the seated session
  reported from another folder or machine, is not the manager until you seat it (Needs you
  shows the workspace, with that agent proposed), and that seating is a new appointment.
  The same while the seated session is shown in its own terminal and in another one at
  once, on this machine or another: neither is the manager, and nothing is typed to it
  until only its own terminal shows it again or you seat it. What goes to a manager is
  typed into the terminal it is held to, never into whichever terminal reports its session,
  nor into an agent that takes the manager's session id as its name; its screen is read
  there first, as for every message (below, "A dialog the runtime misses"), and a message
  you sent holds the order of seat changes, then the agent's one writer, never the reverse.
- **What belongs to an appointment never reaches the next.** A message you sent, a question
  a manager asked you with options, a reply: each belongs to the appointment it was sent to
  or asked under. A message still waiting when the seat is given anew is recorded as not
  delivered, never typed to the new manager; an earlier appointment's question can no
  longer be answered (the new manager reads the conversation and asks again); only the
  appointment a message went to replies to it.
- **Units on another machine** are put there by the intent's manager or by you, on any
  machine you follow (following a machine is your act); another agent puts a unit only on
  its own machine. There are no links between workspaces: an agent on another machine works
  in a workspace through the tasks it is given, and a manager on another machine holds a
  seat like any other.

### Starting a manager

workriver starts a working agent (one that takes a seat) only on your own act, never on an
agent's, and schedules none: which worker gets which task stays the manager's. The temporary
agents that answer a question or review a command are workriver's own, as before.

- **Who.** You only: the route asks for you, the start event (`workspace.manager_started`)
  is yours alone in the event rules, it is not on a machine's listener, and it is recorded
  only by the start action (recording it any other way, `/api/append` included, is
  refused). The two events that end a start, its seating and its end with why, are
  workriver's alone: no request records them, yours neither.
- **Built from records.** The request names a workspace, a harness, a model and the seat you
  were shown, nothing else. The folder is the recorded project's (filled in by the rules);
  the harness and how a model is given to it are workriver's own table (Claude Code and
  Codex; a model is your default, the harness's own alias, or one name of letters, digits
  and `. _ : / @ [ ] -`, never starting with `-`); the agent's name comes from the
  workspace's id; the place is the one the service just opened. No path, command, argument,
  environment, machine or place is taken from the request, and the arguments go to the
  runtime as a list, never as a line of shell. A harness is offered only when its command is
  on the machine and the runtime's integration for it is current.
- **The late seating.** The start is recorded with the place the service opened and that
  place's shell, before the harness starts. What shows up there is seated by workriver, and
  the event rules accept that seating only for that workspace's start still pending: when no
  event touched the seat since your click (by sequence, never by the seat's value: a seat
  emptied and filled and emptied again, or a workspace closed and opened again, ends the
  start), at the recorded place and shell, in the project's folder, with the harness
  started, and for a session the records did not know before the start (one seen before,
  one that holds a seat, one that has a task is refused). So no other workspace loses its
  manager by a start. One start per workspace is pending at a time (a second is refused,
  409). A start ends by a seat event, by a failure, after ten minutes with no session, or
  when the service restarts (recorded before it serves): an ended start seats nobody.
  Anything refused is not seated and waits for a click in the select. The seating takes the
  same order as every seat change: the seat before is read, the one replaced is told it no
  longer manages, and what was queued for its appointment is not delivered.
- **What stays the service's word.** That the session was read at the recorded place and
  shell, and the ten minutes: the rules have no clock and do not see the runtime. Both are
  in the records (the place on both events, the end of an expired start).
- **Before it is seated** the agent is an agent with no seat and no task: it reads what every
  agent reads (no workspace's conversation) and is refused what a manager does. workriver
  types nothing to it until it is seated.
- **What it runs as.** Your account, your harness configuration and whatever the project's
  folder holds for the harness (instructions, settings, hooks), as when you start it there
  by hand. Which folders can be started in is yours: only you add projects. The harness's
  own trust question in a new folder is yours to answer (in its terminal, or from the page
  like any agent waiting in its terminal).
- **Projects** you add on the page are checked as from the CLI: a whole path (from `/` or
  `~/`, the service's home; anything else would be read against the folder the service was
  started in), a folder (not a file), not a project already, not inside a project and
  holding none (a project added inside another would change the project of intents already
  recorded there). Projects already nested in an older store stay as they were.
- **Trying a harness** (Settings, Try it) starts it once in a temporary folder of its own,
  with no prompt, seats nobody, records nothing, then closes its terminal and removes the
  folder. Yours only.

What these do not hold, since the runtime's own interface is your account's, is under
[What this does not protect against](#what-this-does-not-protect-against): the bait for
your pick of a manager, and what a program can do to a terminal.

Findings are the exception by design: by default the intent's **manager decides them**, so
that work does not wait for you. The trade-off, plainly: a manager that is compromised or
simply wrong can accept or reject findings. What limits it: only the manager of that intent
can (no other agent ever), its decisions are recorded as an agent's and shown with a robot
mark, never told to anyone as yours, and you can change any of them, which the manager is
told. To decide everything yourself for an intent, set "Findings: Manual"
(`workriver intent set <intent> --findings user`). There is no rule by severity: a
finding's severity is its reviewer's own rating, not a control.

## Running a command as you

Some steps an agent may not take itself (on a work machine a hook blocks agents from
pushing); the manager makes them a gate step with the command, and the page can run it for
you. That is the most powerful thing the page does, so:

- **Only you.** The routes (`/api/run/prepare`, `/api/run`, reading a run and focusing its
  terminal) ask for you (your session, or your token from a process that is yours: see
  above), refuse a command that runs in an agent's terminal, and
  are not served on another machine's listener. An agent cannot run anything this way, even
  with the token in reach.
- **Only what you confirmed.** Preparing a run runs nothing: the service works out the
  directory (the unit's work tree, or a project's folder; never a path the page sends) and
  the machine, and who wrote the command, and the page shows them with the exact text. The
  run then goes only with that preparation's id **and** the same text, once, within ten
  minutes; any other text is refused and nothing runs. A command is one line without control
  characters. Why the confirmation: the text was written by an agent, which may have been
  misled by what it read (prompt injection) or be simply wrong, and it runs with your
  credentials; you are the check. The dialog says "written by agent X: read it before you run
  it" and flags obviously destructive commands (`rm -rf`, `push --force`, `reset --hard`,
  `clean -fd`…) in red – a heuristic, not a guard.
- **Your shell, nothing more.** It is typed into a new plain shell of yours (no agent) in a
  workspace of the runtime named "workriver-commands" on the unit's machine, so your credentials, hooks and
  checks apply exactly as when you type it there; workriver adds none of its own and removes
  none. A remote machine's command runs in that machine's runtime, as you are there. workriver
  closes that shell once the command has ended and nothing runs in it any more (at once after
  exit 0, at your next run after a failure); a shell with a background job, or one you went on
  working in, is left alone, and so is any terminal that is not a run's own shell.
- **In the record.** Each run is an event, `command.ran` (id, command, directory, machine,
  unit, who wrote it), recorded as yours, and `command.ended` (its exit status) once the
  service sees it end; never the output, which may hold secrets. The output is shown to you
  live and kept only in memory.
- **Reviewed, never blocked.** When the dialog opens, workriver starts a temporary agent in
  the same "workriver-commands" workspace, in the folder the command would run in, to tell
  you what the command will do (the scripts it calls, what it writes, deletes, downloads or
  installs) with a verdict: ok, caution or danger. Its prompt says read-only: run nothing of
  it, change nothing. The command and the step's text reach it quoted, as content. Only that
  agent may record the review (the service checks its terminal, also on another machine's
  listener, where this is the one route added); not you, not another agent. It is shown to you
  as text and never typed into any terminal; "Run" never waits for it. It is recorded, as
  workriver's, as `command.reviewed` with the run; the agent is closed once it answered, or
  after ten minutes.

## Answering an agent's terminal from the page

When an agent waits for you in its terminal (a permission prompt, a question, a trust
dialog), the page shows its dialog and lets you answer it, and opens its terminal read-only
when you ask. Typing into an agent's terminal is acting as you, so:

- **Only you.** The three routes (`/api/agent/screen`, `/api/agent/keys`,
  `/api/agent/view`) ask for you (`App::user`) and are not served on another machine's
  listener; an agent's proof is refused there whatever it names, so no agent reads another
  agent's screen through workriver (a blind reviewer could otherwise read the work it must
  not see). A POST must come from the page's own origin (as every POST). The two GETs do
  something (`/view` starts a stream and takes one of four slots, `/screen` reads the
  terminal), and a page on another port of this host is the same site, so its requests
  carry the session cookie: a request with the cookie is refused unless the browser says it
  comes from the page itself (`Sec-Fetch-Site: same-origin`; a missing header is refused
  too). The CLI's token is not a page's to add. No CORS headers: no other origin reads the
  answers. Every answer is `Cache-Control: no-store`.
- **Only keys, only these.** What may be sent is Up, Down, Enter, Escape, Tab or a digit 1–9:
  the page names an option by its number or one key by its name, never text, a sequence, a
  size or a paste. A screen can say anything; the page has no other way to type.
- **Only to the screen you saw, at one place, once.** The service, never the page, reads the
  screen, parses it and sends the keys. The page sends what you pressed with the digest the
  service gave for the screen it showed: SHA-256 of a key drawn at random when the service
  starts, the agent, the number of presses sent to it before the screen was read, and the
  whole visible screen; the count and the read are taken together under the agent's one
  writer, so a press under way ends (its keys taken effect) before the screen is read, and a
  screen read before a press never signs the count after it. One request resolves the agent's session to exactly one place (two places that
  show it: refused, "answer it in its terminal") and reads and presses only there; it sends
  only when the whole screen it reads there matches the digest, so a second click, a retried
  request or a stale dialog that looks the same never goes; a list is walked one arrow at a
  time with the whole screen read again after each, and Enter goes only when the marked line
  is the option you chose. A send that failed or whose answer was lost is never repeated by
  itself. One writer per agent: a press and a message workriver types never interleave. A
  few milliseconds between the last read and the key remain and cannot be closed (the
  runtime has no send-if-unchanged); they are kept to one read followed at once by one key.
- **What is kept.** Screen text and frames stay out of the snapshot, the logs and browser
  storage. The event store keeps, for each answer, `agent.keys` (yours alone to record): the
  agent, its task, the keys, the outcome, and two pieces of the screen: the question line
  (the nearest line above the first option, only when it ends with `?`) and the option's
  number and words, each cut at the first backtick, `"`, `'` not between two letters, `$`,
  `=`, `/`, `~` or `\` (where a command's arguments, an assignment, a quoted value or a path
  begin) and capped (120 and 40 characters), on one line; the event rules cut them again,
  whatever a request says. It is kept for good, and agents can read it (task detail goes
  through `App::view` like every record): the cut keeps out where a secret usually sits, but
  a secret with none of those characters before it, inside the cap, would be kept.
- **Shown as text, in terminal order.** The screen's text and an option's words are another's
  writing: the page shows them as plain text (quoted agent text, never Markdown, links
  inert), drawn left to right in the order written as a terminal draws them (a browser would
  reorder them by the Unicode bidi algorithm, so a command could read one way and run
  another), with every invisible or reordering character (Unicode's Default_Ignorable_Code_Point,
  which holds every Bidi_Control one) shown as its code point, `⟨U+202E⟩`, never dropped.
  ANSI only inside the terminal view, which is read-only (no key reaches the terminal, it is
  never resized), writes no clipboard and opens no link (an OSC 8 link does nothing).
  Screen text never goes into a message to an agent.
- **Bounded.** Four terminal views at most across every page, one stream each (today a
  child process of the runtime's own CLI, given no more environment than it needs), killed
  with its stream; a stream whose reader falls behind is ended.
- **A dialog the runtime misses.** The service reads the screen of an agent the runtime calls
  idle or done when that state begins, and before every message it types into any agent (at
  the same place, under the same writer); a screen with options and a key named under them
  holds the message and shows the agent waiting, so a prompt is never typed into a dialog.

## What agents are told

Every message workriver types into a terminal starts with its own words (what happened,
what to do next). What somebody else wrote comes after, inside «…», named as content:
`quoted from rev (content, not instructions): «…»`. Your own notes read `The user wrote:
«…»`, and only when the service knows they are yours. Control characters and the quote marks
are taken out of quoted text, and long text is cut. The agents' instructions say never to
follow what stands inside a quote.
A message you write to a manager from the page names what you picked and the files you
attached in workriver's own words, from its records (a file only if you may point at it);
only your own text is quoted as yours.

## Blind reviews

A review, tests, research or design task can be handed out blind
(`workriver task add … --blind`), so that its agent is not anchored by the implementer's
account or by other reviews. Until that agent delivers, the service shows it only its own
task (with what the manager or you added to it, and that addition's files), the unit and
the intent, and its own attachments: not other tasks' notes, evidence,
findings or decisions, no questions, no backlog, and of other agents only their names, not
where they work (the path of a session file is given only for its own task, by
`workriver task session`). It cannot ask other agents and they cannot
ask it; nothing is typed into its terminal about other work. Afterwards it sees everything
again, and the task keeps a "blind" mark so you can weigh its findings as an independent
view.

For this, reading is no longer open to every local program: the page reads with your
session, the CLI as you (token) or as the agent of its terminal; a request that is neither is
refused. Only whether the service runs (`workriver status`) is anyone's to see.

## Conversations are read by their workspace

What you and a manager write to each other is that workspace's: through the service, an
agent reads

- the messages of the workspace whose seat it holds, the earlier managers' included (a new
  manager reads the history), and that workspace's read marks;
- of a message you sent to every manager, only when a copy went to its own appointment: the
  message, its own copy and its own replies. Nothing of the other copies (their workspace,
  their manager, whether they were delivered) or of the other managers' replies;
- nothing else of any conversation: not another workspace's, with the same name or not.

The same cut is made for every route that returns records, on this machine's listener and
on another machine's (`App::view`: the records, the page's snapshot, a task, an intent's
history, a stored file). For a stored file the cut is only this: a file that nothing but the
parts cut away names is not served to that agent; a file another record holds as a file too
(a task's or a finding's files, a check's report, what a question or a message it does read
points at), or that the agent stored itself, is served as before. A file's id written in
words (a title, a note) holds nothing. You read everything.

What stays readable by every agent, by design: everything that is not a conversation with
you. Intents (planned and active, every workspace's and project's), units, tasks, findings,
gates, chores, the discussions of tasks and their threads, the agents. A review reads
another unit's work, `workriver ask` reaches any task, a manager plans against the whole
roadmap. The blind view (above) narrows that for an agent on a blind task.

What this scope cannot hold is the same as for blind reviews, and no more is promised: it
binds the service's routes, so it holds completely for agents on other machines, which
reach the records only through their machine's listener. An agent on this machine runs as
you: it can read the database and the stored files from disk, and where the runtime lets
any agent read another's terminal (Herdr does), it can read the manager's terminal, where
your messages are typed.

Blindness guards against bias, not against an agent that sets out to look. What remains:

- **Commit messages and the repository.** The reviewer reads the code from git, and git
  also holds commit messages, other branches and whatever was committed. workriver's own
  views give only hashes; it cannot hide git. Reviewers are told to read the diff.
- **Being another agent.** Naming another agent's terminal is refused. What is left takes
  more: see "A session whose terminal is gone" below.
- **Files.** Agents run as you: the database and the attachments in `~/workriver-data`
  can be read from disk, and so can every harness's session files (workriver only says
  where they are, when asked; it never reads them).
- **Who manages.** A workspace has no machine of its own; its seat's machine and folder are
  an agent's, and are cleared for a blind agent like any other agent's. That a workspace
  has a manager, and its name, stays visible.
- **Other terminals.** Where the runtime lets any agent read another's terminal (Herdr does:
  [runtimes/herdr.md](runtimes/herdr.md#security-under-herdr)), what another
  agent's terminal shows is not workriver's to hide.
- **The manager's message.** What the manager writes into the hand-off, or adds to the task
  later (`workriver task note`, which the blind agent is told and shown: it is its own
  task), is up to it; its instructions say what to leave out. workriver's own words in that
  notice name the task only as the blind agent's view shows it; an addition to another task
  of the same agent waits, like any other notice, until it delivers.

Who is blind and who is told are one question, answered once: the agent a task is with is
found by its session among the agents running now (a task given by a name, as before
sessions were recorded, belongs to the one running agent that goes by it), and that same
agent is the one checked for blind work and the one typed to. What waits for a blind agent
is for that session on its machine alone: if it closes meanwhile, nothing is given to an
agent that came since, whatever it is called; and an addition whose task was dropped or
handed to someone else meanwhile is not given at all.

## What this does not protect against

Agents run under your account, and the check above does not change that. It stops the
accident (a command that carries a stale or wrong terminal) and the one-variable claim
(naming a terminal, or none, to be someone else); it stops a request that merely claims to be you (a
prompt-injected `curl`, a page in your browser, another machine). It does not stop a
determined program on your machine:

- **A program that sets out to be you still can.** Read the token, start a process that
  leaves its terminal, give it a terminal of its own, call the socket: one line of shell. What
  changed is that it takes that line.
- **The runtime's own interface is your account's.** Any program may tell the runtime which
  agent a terminal holds, or close a terminal (under Herdr, any program can:
  [runtimes/herdr.md](runtimes/herdr.md#security-under-herdr)).
  - **A session whose terminal is gone can be taken**, unless it holds a manager's seat:
    once the terminal a session was first seen in no longer runs, whoever reports that
    session for its own terminal is that agent. A program does not have to wait: it can
    close that terminal through the runtime and then report its session. It cannot do that
    unseen: the agent's terminal disappears. A session that holds a seat is not taken this
    way from another folder or machine (that needs your seating); from the same folder on
    the same machine it is: a program started there that reports the seated session's id
    after its terminal is gone is the manager again, with its appointment, as a resumed
    manager is. You see it: the workspace's conversation says the manager is back.
  - **An agent can be given another name** by a report for its terminal: its next commands
    are a new agent's and its own tasks refuse it. Nobody is impersonated; work stops.
  - **Bait for your pick of a manager.** An agent's name, its session id and the group of
    the runtime its terminal is in are what any program reports. The manager select shows
    them with what workriver itself knows: the machine, the folder, the tasks the records
    give that session. This holds for the agent Needs you proposes too: a program that
    reports the seated session's id from another folder is offered as "a new terminal
    reports this session", with both folders shown, and your confirming it is a new
    appointment. A wrong pick is yours to seat again. A manager you start from the page is
    not picked from what programs report: you saw the workspace, the project's folder, the
    harness and the model, and the session seated is one the records never knew, at the
    place the service opened. Not held: a program with the runtime's socket can type into
    that new terminal before the seating, as into a seated manager's after it, and so bring
    up there a session the records never knew (one it made itself); that one is seated. It
    is new to workriver as the started one would be, and it is that program's either way.
    A session workriver knew is refused.
  - **Closing the manager's terminal** through the runtime stops that workspace's
    deliveries. Nobody gains them.
  - **Renaming, creating or closing the runtime's own workspaces changes nothing recorded.**
  - **A terminal the runtime knows as an agent's only by report can be released** and is
    then a plain terminal: yours, with the token. (Under Herdr, which agents are
    not such terminals: [runtimes/herdr.md](runtimes/herdr.md#security-under-herdr).)
  - On another machine a worker's session goes to the first terminal that *calls* with it;
    a manager's is held, from its seating, to the one terminal that machine's runtime
    shows it in: while that terminal is gone or holds another agent, or another terminal
    (there or on any other machine) shows the session too, nothing is typed to the manager,
    as on this machine, and the same session showing again in the seat's
    folder there has its seat back by itself. What that machine's processes and runtime say comes through that machine's
    account, so it holds as far as that account does. A
    terminal where the runtime knows no shell is not remembered: its session shown by
    another terminal gets both refused.
- **Typing into another terminal.** Where the runtime lets any agent type into any terminal
  (Herdr does): a command typed into another agent's terminal is that agent's, one typed
  into a plain terminal is yours.
- **A background service that gives its commands a terminal**, started outside the runtime
  or with none of its terminals in its environment, is taken for you with the token: the
  terminal rule catches the plain case only.
- **An agent that runs outside the runtime at a terminal** (another terminal window) is you
  as far as the service can tell.
- **The files**: the token, the database, the attachments, `session-roots.json` and
  `user-sessions` can be read and written from disk without the service (`user-sessions`
  holds hashes: reading it gives no session; a line written into it counts after the
  service restarts).

If agents work on input you do not trust, run them in a sandbox or under another account,
where they cannot read `~/workriver-data` (they then cannot reach the caller socket either
and cannot record; that is not supported yet).

Other machines: workriver's own ssh connections never ask for anything, never forward your
ssh agent and never accept an unknown host key (`BatchMode`, `ForwardAgent=no`,
`StrictHostKeyChecking=yes`), whatever your ssh config says; connect once by hand to accept
a new host's key. The port that leads here from another machine is picked anew for every
connection and written there to `~/.config/workriver/port` (mode 600 under a 700
directory); a machine's listener therefore takes any port of `127.0.0.1` or `localhost` as
its Host, still refusing other names (a page served elsewhere cannot pose as it).

Reading through the service needs an identity too (you, or an agent in its terminal), but the
files themselves are readable by every program running as you.
