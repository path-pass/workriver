# Adding a project and starting its manager from the page

Status: design, nothing implemented. Before the branch is merged its rules go into
`docs/security.md`, `docs/design/ui/navigation.md`, `docs/runtimes/herdr.md`,
`docs/design/layout.md` and `CLAUDE.md` / `AGENTS.md` (section 3 lists the sentences), and
this file is removed.

Today a project is added only with `workriver project add` in the user's terminal, and a
manager is an agent the user started by hand in the runtime and then seated. After this the
user does both on the page: add a project, then one click on a workspace starts a coding
agent in the project's folder and seats it.

## 1. Add a project

- **The same record, the same route.** The page records `project.added` through the route
  the CLI's `project add` uses. Nothing new decides who may: the handler finds the caller
  (`App::who`), the event is `Body::user_only`, an agent is refused, a machine's listener
  refuses it (`complete`: "register project folders on the machine workriver runs on").
  Its first workspace is recorded with it under one lock, as today.
- **What the page asks.** Two fields: the folder (typed, an absolute path on the machine
  workriver runs on; a leading `~/` is the service's home) and a title (optional; the
  folder's name when empty). No folder browser: a browser gives a page no real path, and a
  route that lists the user's folders would be a new thing to read.
- **Any other path is refused.** `complete` refuses, before it makes the path canonical, one
  that neither starts with `/` nor with `~/` ("give the folder's whole path, from / or
  ~/"): `.`, `foo`, `../x`, a bare `~`, `~name`. Today such a path is resolved against the
  directory the service was started from, so a typo would register that folder. `~/` is
  expanded there too, to the home of the account the service runs as. The rule is the
  service's and holds for every entrance; the CLI already sends a whole path. The page
  refuses the same before it sends, as a convenience only.
- **Where.** "Add project" at the foot of the rail's projects, and in the two places that
  today print the CLI line (the empty page, Start's "no project" hint). The dialog shows a
  refusal in the service's words. On success the project shows with its workspace, whose
  line offers "Start manager".
- **Checks.** Today: the folder exists and is made canonical (`complete`), it is not a
  project already, folder and title are not blank (`prepare`). Two are missing today and
  are added for both entrances, the CLI and the page:
  - *It is a directory* (`complete`; a file passes today).
  - *It is not inside a registered project and holds none* (`prepare`, on the recorded
    paths). Today nesting is accepted and `project_of` takes the deepest project, so a
    project added inside another changes the project of intents already recorded there,
    which the rules say never changes. Projects already nested in a store stay as they
    replay.

## 2. Start a manager

**The request.** `POST /api/workspaces`, a new action `start`: the workspace's id, a model
key, and `replaces` (the seat the user was shown, as for seating). It answers as soon as
the start is recorded; it does not wait for the agent.

**Where on the page.** The manager select gets a second part, "Start a new one": the
harness (Claude Code, the only one), the model, and "Start and seat". A workspace line with
no manager offers it directly. Where a manager runs, the select asks once before starting
over it, as it does before seating over it.

**What the service does**, in order:

1. Looks before it opens anything: the workspace exists and is open, its seat is the one
   `replaces` names, no start is pending for it, the model key is in the table, the
   project's folder is still a directory, the runtime answers. A refusal here is only
   early; step 3 is what decides.
2. Opens a place, through a new method of the runtime interface (a shell in a folder, in a
   named group, without taking focus), and reads its shell (process id and start time, as
   for a pin). Nothing runs in it yet.
   - *group*: the runtime's group labelled with the workspace's name; the first with that
     label is reused, else one is made. It is a hint and decides nothing, as before.
   - *folder*: the project's folder, as recorded. Gone by now: the start fails.
   - A place whose shell the runtime does not know cannot be told from another later: it is
     closed and the start fails.
3. Records `workspace.manager_started`, the user's alone (`Body::user_only`): the
   workspace, the harness, the model key, the agent's name, **the place just made and its
   shell**, and `replaces`. The rules (core's `prepare`, under the store's lock, so two
   clicks cannot both pass):
   - the workspace exists and is open;
   - its seat is the one `replaces` names (session and appointment, as for a seating; 409);
   - no start is pending for it (409);
   - the folder is not taken from the request: the rules fill in the folder of the
     workspace's project.

   A refusal closes the place of step 2 (it is still a shell with nothing in it;
   `close_shell` checks that) and is the request's answer. Like `command.ran`: what acts as
   the user is in the record before it happens, here before the harness is started.
4. Starts the harness in that place, through a second new method (a harness with arguments,
   under a name, in a place):
   - *harness and arguments*: from a table in `common/config/`, one entry now: Claude
     Code, with models "your default" (no argument) and the aliases the harness resolves
     itself (`fable`, `opus`, `sonnet`: `--model <alias>`). The request names a key of the
     table; an unknown key is refused. Nothing else is passed: permission mode, MCP
     servers, hooks, settings are the user's own configuration, exactly as when they type
     the harness's name in that folder. No environment is added.
   - *name*: `<workspace id>-manager`.
   - It does not close the place because the harness is not ready (below, "A trust
     prompt").
5. Waits, off the request. When the runtime shows, at the recorded place and with the
   recorded shell, an agent with a session of the harness's own (never the name the
   adapter gives an agent without one), the service records what it sees of that agent
   (`agent.seen`) and then `workspace.manager_start_seated`, workriver's own
   (`Body::service_only`): the workspace, the start it ends (the sequence of that
   `workspace.manager_started`), the session, the folder the runtime shows for it, the
   place and shell it was read at. The service reads these as `App::seating` does (the
   session at exactly one root, the pin). The rules accept it only as the end of that
   start (below, "The seating is the start's").
6. The seating tells the agent what every seating tells (`tell_of_seat`): which workspace
   and project, what is active, and `workriver guide manager`. That is how the guide is
   sent: by its command, so it matches the installed version. A dialog on the agent's
   screen holds the message, as for every message.

**The click is the seating act**; the record of the seat follows when there is a session to
seat, and it is not the user's event: the records say who chose what. The user chose the
workspace, the harness and the model, and that what this start brings up is seated
(`workspace.manager_started`). The service says which session that was
(`workspace.manager_start_seated`).

**The seating is the start's.** A workspace holds its start (`Workspace.start`: the
sequence of its `workspace.manager_started`, when, harness, model, name, folder, place,
shell, and how it ended if it did). Pending: recorded and not ended. `apply` drops it,
pending or ended, on every event that touches that workspace's seat after it: a seating
or an emptying (`workspace.manager_set`), the workspace closed or opened, a seating
elsewhere that takes this seat's session away, and `workspace.manager_start_seated`
itself. `workspace.manager_start_seated` is accepted only when all of these hold, read
from the records:

- It names the workspace's pending start by its sequence. So nothing touched the seat
  since the click: the comparison is "no seat event after the start", not "the seat has
  the same value". A seat emptied and filled and emptied again, or a workspace closed and
  opened again, has dropped the start, and the late seating is refused. A second seating
  of the same start is refused too: the first dropped it. An ended start seats nobody.
- Its place and shell are the ones recorded on the start.
- Its folder is the start's (the project's).
- Its session is new to the records: either nothing is recorded of it, or its first
  `agent.seen` has a sequence after the start's (`Agent` keeps the sequence it was first
  seen at; one replayed without it counts as old). So a session workriver knew before the
  click (a worker's, a stopped manager's, one that holds a seat or has tasks) is never
  seated by a start, whatever shows it in the new place; and no other workspace's seat is
  emptied by one.
- Its harness, as seen, is the start's.

What it changes is what a seating changes: the seat, with a new appointment (`since`: its
own sequence), the session pinned to that place. `workspace.manager_back` does not touch a
seat and drops no start: the user chose to start over that manager when its terminal was
gone, and its coming back is not a choice of theirs.

What the rules cannot check and the service holds: that the place and shell it reports are
what the runtime showed (as for the machine and folder of every seating), and the time
(the rules have no clock). The 10 minutes are the service's, and their end is recorded
(below), after which the rules refuse.

**One pending start, and its end.** Pending is in the records, not in memory, and the
rules see it. A start ends by a seat event (above) or by `workspace.manager_start_ended`,
workriver's own (`Body::service_only`): the workspace, the start by its sequence, a reason
(a key, never UI text: `runtime`, `no_agent`, `no_session`, `refused`, `restarted`) and,
for the first, the runtime's words. It is accepted only for the workspace's pending start.
The service records it on each failure below, when its wait is over for any reason (the
wait's one exit records it), and **at start-up for every start still pending**, before it
serves: its wait died with the service that made it. So a crash leaves no workspace that
can never be started again, and no late seating comes back after a restart: only the
running service records one, and only for a start it waits on.

**While it starts** the workspace's line says "starting a manager…", and after a failure
why, from the workspace's start as recorded (`/api/workspace`, a field `start`: since
when, pending or the reason). The reason stays until the next start or seating.

**A trust prompt or any other dialog.** workriver never answers it. Measured (the runtime
at 0.9.5, Claude Code in a folder it has never seen, the question never answered): the
start call fails after about 3.5 s with `agent_not_ready` ("blocked during startup and is
not ready for prompts"); the agent is there, `blocked`, its launch pending, with no
session, so it goes by the name of its place. The screen asks "Is this a project you
created or one you trust?". Today's `launch` closes the place on any error of the start
call, which would close the user's dialog and fail every start in a new folder. So the
second method (step 4) does not use it as it is:

- `agent_not_ready` (`RuntimeError::NotReady`), or an agent that shows blocked or still
  launching: not a failure. The place is kept and the wait goes on.
- The place is closed only when no agent shows in it after the start call (the harness is
  not installed, the shell refused the line), and only while it is still a shell with
  nothing in it.

The page shows it as it shows every agent waiting in its terminal: a Blocking row under
Needs you and the gate of "Answering a stuck agent" (`docs/design/ui/navigation.md`),
found by the agent's name and machine since it has no session yet; the workspace's line
says "starting a manager… waiting for you in its terminal" and opens the same gate. The
user answers there or in the terminal; the session shows and the seating follows.

**Failures.** Each ends the pending start with a reason on the workspace's line, in the
user's language; none seats anyone.

| What | Reason | Result |
|---|---|---|
| Runtime not connected, folder gone, no shell known for the place | (none recorded: step 1 or 2) | Nothing started; the place, if made, is closed. |
| The rules refuse the start (seat changed, one pending, closed) | (none recorded: step 3) | The place is closed. 409 or 422, the rules' words. |
| The runtime refuses the start call, other than not ready | `runtime` | The place is closed if still an empty shell. |
| No agent shows in the place (harness not installed) | `no_agent` | The place is closed if still an empty shell. |
| No session to seat within 10 minutes | `no_session` | Not seated. The agent and its place are left: the user may be in its dialog. It is in the manager select like any running agent, one click. |
| The rules refuse the seating (a session known before, another folder or harness) | `refused` | Not seated. The agent is left running. |
| The seat changed meanwhile, or the workspace was closed | (the event that did it dropped the start) | The service finds its start gone and stops waiting. The agent is left running, unseated. |
| The service restarted while pending | `restarted` | Recorded at start-up. The agent is left running, unseated. |

**A second click.** One pending start per workspace: a second is refused by the rules
(409, "a manager is being started for ws1"), a retried request too. After a failure the
user may start again. Starting where a seated manager does not run is a new session and a
new appointment.

**Another machine.** Not in this version. A workspace has no machine and a project is a
folder of the machine workriver runs on, so no record says where to start elsewhere, and
the folder must not come from the request. See Not done.

## 3. The rules this changes

Two stated rules change. Both are rewritten where they stand, with the implementation.

### "workriver starts no agent"

It is already not literally true: the service starts a temporary agent to answer a question
and to review a command. What holds, and is now written:

> workriver starts a working agent (one that takes a seat) only on the user's own act,
> never on an agent's, and schedules none: which worker gets which task stays the
> manager's. The temporary agents that answer a question or review a command are as before.

Where the old wording is, to change with the implementation: `README.md` ("it does not
start or schedule agents") and `docs/README.zh-CN.md`; `CLAUDE.md` (the first paragraph);
`docs/design/ui/navigation.md` ("workriver never starts an agent");
`docs/design/caller-identity.md` (Not done: "making or arranging the runtime's groups …,
or starting agents"); `docs/runtimes/herdr.md` ("start a coding agent in a pane, then pick
it") and `docs/getting-started.md`, both languages; `docs/security.md` ("What only you can
do", and a section of its own); the comment in `frontend/web/src/planning/Start.tsx`; the
`seat` recipe and `CHANGES.md` (what a manager tells a user who asks for a seat).

### "Only the user's event sets a seat"

Today: a seat is set by one event, the user's (`workspace.manager_set`), and the service
adds to it only what it reads of the runtime, "never a seat". After this the service fills
one seat itself: that of a start the user recorded. What holds, and is written:

> A manager's seat is set by the user's event (`workspace.manager_set`) or by the end of
> the user's own start: `workspace.manager_started` is the user's alone and names the
> place the service made; `workspace.manager_start_seated` is workriver's alone and core's
> `prepare` accepts it only for that workspace's pending start, when no event touched the
> seat since (by sequence, never by the seat's value), at the recorded place and shell, in
> the project's folder, for a session the records did not know before the start. No agent
> event sets a seat, a workspace or a project; a new kind that does is `Body::user_only`,
> or `Body::service_only` under a rule of `prepare` that ties it to a user's event.

Where the old wording is:

- `CLAUDE.md` and `AGENTS.md`, the security rule, three sentences: "A manager's seat is
  set by one event, the user's only (`workspace.manager_set`): no agent event sets a seat,
  a workspace or a project, and a new kind that does one of these is `Body::user_only`"
  (as above); "The service adds to a seating only what it reads of the runtime, for the
  user's own event (`App::seating`: …), never a seat" (add: "but the seat of a start the
  user recorded, which it names and the rules check"); "its manager are set only by the
  user or by a start the rules checked" (that start is an intent's; say "an intent's
  start" and add the manager's).
- `docs/security.md`, both languages: "What only you can do" (starting a manager is the
  user's); "Each is set only by you, or by a start the rules checked" (the manager: by you,
  or by the end of a start you made, as above); "Bait for your pick of a manager" (a
  started manager is not picked from what programs report: the user saw the workspace, the
  project's folder, the harness and the model, and the session is one the records never
  knew, at the place the service made; what is not held is in section 4).
- `docs/design/caller-identity.md` (the seat's pin: also set by a start's seating).
- The comments of `Body::user_only` and `Body::service_only`
  (`crates/workriver-core/src/access/mod.rs`) and of `App::record`
  (`crates/workriver/src/server/app/record.rs`), and `docs/design/layout.md`.

**ACP later.** The start sits behind two methods of the runtime interface, in workriver's
words (a place in a folder; a harness, arguments and a name in a place). An adapter that
speaks ACP opens the session itself and has its id from the protocol instead of reading a
terminal. The route, the records, who may, and the seating's rules do not change; only the
adapter and what "place" means do. Starting workers from workriver stays with that work.

## 4. Security

- **Who.** The user only: `App::user` in the handler, `workspace.manager_started` is
  `Body::user_only`, the route is not on a machine's listener, and it is a POST from the
  page's own origin like every POST. No agent starts an agent through workriver. The two
  events that end a start are `Body::service_only`: no request records them, the user's
  neither.
- **Built from records.** The request carries a workspace id, a model key and `replaces`,
  nothing else. The folder is the recorded project's; the harness and its arguments are
  literals of workriver's own table; the name comes from the id; the place is the one the
  service just made. No path, command, argument, environment, machine, group or place is
  taken from the request. The arguments go to
  the runtime as a list, never as a line of shell.
- **The late seating.** One click stays, and its checks are core's (section 2, "The
  seating is the start's"), not a task's in the service's memory. It is recorded as
  workriver's, naming the user's start, so the records tell what the user chose from what
  the service found. It is refused when anything touched the seat since the click (by
  sequence: an emptied and refilled seat, a closed and reopened workspace), for a second
  seating of one start, for an ended start (10 minutes, a restart), at another place or
  shell, in another folder, and for any session the records knew before the click. A
  session it seats holds no other seat and has no task, so no other workspace loses its
  manager by a start. Anything refused is not seated and waits for a click in the select.
- **What stays the service's word.** That the session was read at the recorded place and
  shell, and the 10 minutes: the rules have no clock and do not see the runtime. Both are
  in the records (the place on both events, the end of an expired start), so a wrong one
  shows.
- **Before it is seated** the agent is an agent with no seat and no task: it reads what
  every agent reads (no workspace's conversation), may add a planned intent, and is refused
  what a manager does. workriver types nothing to it until it is seated.
- **What it runs as.** The user's account, the user's harness configuration and whatever
  the project's folder holds for the harness (instructions, settings, hooks), as when the
  user starts it there by hand. Which folders can be started in is the user's: only they
  add projects. The harness's own trust question stays the user's to answer.
- **Not held** (as today, `docs/security.md`, "What this does not protect against"): a
  program with the runtime's socket can type into the new place before the seating, as it
  can into a seated manager's after it. It can so bring up, in that place, a session the
  records never knew (one it made itself), and that one is seated: it is new to workriver
  as the started one would be, and it is that program's either way. A session workriver
  knows is refused.

## 5. Tests

- Core, projects: a project inside another, or holding one, refused.
- Core, the start: `workspace.manager_started` refused from an agent and from the service;
  refused on a closed workspace, on a changed seat, while one is pending; its folder is
  the project's whatever the body says. `workspace.manager_start_seated` and
  `workspace.manager_start_ended` refused from the user and from an agent.
- Core, the seating of a start, each refused: the seat emptied and refilled since the
  start (and filled, emptied: the same value as at the click); the workspace closed and
  reopened; a second seating of one start; an ended start; a start that is not the
  workspace's latest; another place; the same place with another shell; another folder;
  another harness; a session seen before the start (a worker's, a stopped manager's, one
  seated elsewhere, whose seat stays). Accepted: a session never recorded, and one first
  seen after the start; the appointment is new; `workspace.manager_back` in between
  changes nothing. Replay: a store without starts, and agents without a first-seen
  sequence, read as before.
- Service, with the fake runtime: the route in `route-access.txt` (nobody and an agent
  refused, absent on a machine's listener); the arguments given to the runtime are the
  table's and the recorded folder whatever else the request holds; an unknown model key
  refused; a second click 409, two at once: one place left, one start recorded; a refused
  start closes its place; not ready, blocked, then a session: the place kept, seated once,
  told once; no agent shows: nothing seated, the place closed, the reason recorded; the
  seat changed meanwhile: not seated, the agent left; 10 minutes: ended, not seated, the
  place left; start-up ends every pending start and seats nobody after; `start` in
  `page-json.txt`.
- Service, projects: `.`, `foo`, `../x`, `~` and `~name` refused by `complete`, and nothing
  registered; `~/x` is the service's home's.
- Adapter: the place is opened without taking focus; the start call carries the arguments
  as a list, in that place; the runtime's `agent_not_ready` answer leaves the place open
  and is `NotReady`; an agent shown blocked leaves it open; no agent in the place closes
  it.
- Page: the two dialogs, the refusals shown (a path not from `/` or `~/` before sending),
  the reasons of an ended start, the three languages in step.
- Once by hand, in a runtime session of its own with a temporary home: a real start in a
  folder the harness has never seen: the Blocking row and the gate open for the agent
  while it has no session, the trust prompt answered from the gate, to the seated message.

## Not done

- Starting on another machine. The way in: a project's folder on a machine becomes a
  record of the user's; until then only "where the last manager was" (the seat's machine
  and folder) could be offered.
- Resuming a seated manager's own session after its terminal closed (it would take its
  seat back by itself, with no seating).
- Harnesses other than Claude Code; choosing a permission mode or other arguments.
- Closing the started agent from the page; starting workers; removing a project.
- Cancelling a pending start from the page: it ends by itself (a failure, 10 minutes) or
  when the user seats someone.
