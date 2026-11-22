# Adding a project and starting its manager from the page

Status: design, nothing implemented. Before the branch is merged its rules go into
`docs/security.md`, `docs/design/ui/navigation.md`, `docs/runtimes/herdr.md` and
`docs/design/layout.md`, and this file is removed.

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

1. Checks, by the event rules: the workspace exists and is open, its seat is still the one
   `replaces` names (409 otherwise), no start is pending for it.
2. Records `workspace.manager_started`, the user's alone: the workspace, the harness, the
   model key, the folder, the agent's name. Like `command.ran`: what acts as the user is in
   the record before it happens.
3. Asks the runtime, through one new method of the runtime interface (start an agent of a
   harness with arguments, in a folder, in a place of its own inside a named group):
   - *group*: the runtime's group labelled with the workspace's name; the first with that
     label is reused, else one is made. It is a hint and decides nothing, as before.
   - *folder*: the project's folder, as recorded. Gone by now: the start fails.
   - *harness and arguments*: from a table in `common/config/`, one entry now: Claude
     Code, with models "your default" (no argument) and the aliases the harness resolves
     itself (`fable`, `opus`, `sonnet`: `--model <alias>`). The request names a key of the
     table; an unknown key is refused. Nothing else is passed: permission mode, MCP
     servers, hooks, settings are the user's own configuration, exactly as when they type
     the harness's name in that folder. No environment is added.
   - *name*: `<workspace id>-manager`.
4. Waits, off the request. When the place it made shows an agent with a session of the
   harness's own, it records the seating (`workspace.manager_set`) as the user's, with the
   `replaces` of the click, through `App::seating` (one root, the pin) and only for the
   session at that place (its shell's pid and start time). **The click is the seating
   act**; the record follows when there is a session to seat.
5. The seating tells the agent what every seating tells (`tell_of_seat`): which workspace
   and project, what is active, and `workriver guide manager`. That is how the guide is
   sent: by its command, so it matches the installed version. A dialog on the agent's
   screen holds the message, as for every message.

**While it starts** the workspace's line says "starting a manager…", from a field of
`/api/workspace` kept in memory (`starting`: since when, state, reason).

**A trust prompt or any other dialog.** workriver never answers it. The agent shows under
Needs you, Blocking, and the user answers in the gate or the terminal; the seating follows.
To measure first, with a real Claude Code in a folder it has never seen: what the runtime
reports at "trust this folder?" (the code says it may report no state and no session), and
whether the gate opens for such an agent. If not, the pending start gives the gate its own
place.

**Failures.** Each ends the pending start with a reason on the workspace's line, in the
user's language; none seats anyone.

| What | Result |
|---|---|
| Runtime not connected, folder gone, the runtime refuses | Nothing started. |
| No agent shows in the new place (harness not installed) | Its place is closed (as `launch` does today). |
| No session to seat within 10 minutes | Not seated. The agent and its place are left: the user may be in its dialog. It is in the manager select like any running agent, one click. |
| The seat changed meanwhile, or the workspace was closed | The rules refuse the late seating. The agent is left running, unseated. |
| The service restarted while pending | The start is forgotten (it lived in memory). As the row above. |

**A second click.** One pending start per workspace: a second is refused (409, "a manager
is being started for ws1"), a retried request too. After a failure the user may start
again. Starting where a seated manager does not run is a new session and a new appointment.

**Another machine.** Not in this version. A workspace has no machine and a project is a
folder of the machine workriver runs on, so no record says where to start elsewhere, and
the folder must not come from the request. See Not done.

## 3. The rule "workriver starts no agent"

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

**ACP later.** The start sits behind one method of the runtime interface, in workriver's
words (harness, arguments, folder, machine). An adapter that speaks ACP opens the session
itself and has its id from the protocol instead of reading a terminal. The route, the
record, who may, and the seating do not change; only the adapter and what "place" means do.
Starting workers from workriver stays with that work.

## 4. Security

- **Who.** The user only: `App::user` in the handler, `workspace.manager_started` is
  `Body::user_only`, the route is not on a machine's listener, and it is a POST from the
  page's own origin like every POST. No agent starts an agent through workriver.
- **Built from records.** The request carries a workspace id, a model key and `replaces`,
  nothing else. The folder is the recorded project's; the harness and its arguments are
  literals of workriver's own table; the name comes from the id. No path, command,
  argument, environment, machine or group is taken from the request. The arguments go to
  the runtime as a list, never as a line of shell.
- **The late seating.** It is recorded as the user's after the click, so it is held to
  that click: that workspace, the seat still as `replaces` says, the session at the place
  the service made (pinned by its shell), within 10 minutes, while the same service runs.
  Anything else is not seated and waits for a click.
- **Before it is seated** the agent is an agent with no seat and no task: it reads what
  every agent reads (no workspace's conversation), may add a planned intent, and is refused
  what a manager does. workriver types nothing to it until it is seated.
- **What it runs as.** The user's account, the user's harness configuration and whatever
  the project's folder holds for the harness (instructions, settings, hooks), as when the
  user starts it there by hand. Which folders can be started in is the user's: only they
  add projects. The harness's own trust question stays the user's to answer.
- **Not held** (as today, `docs/security.md`, "What this does not protect against"): a
  program with the runtime's socket can type into the new place before the seating, as it
  can into a seated manager's after it.

## 5. Tests

- Core: a project inside another, or holding one, refused; `workspace.manager_started`
  refused from an agent and from the service; a start refused on a closed workspace, on a
  changed seat.
- Service, with the fake runtime: the route in `route-access.txt` (nobody and an agent
  refused, absent on a machine's listener); the arguments given to the runtime are the
  table's and the recorded folder whatever else the request holds; an unknown model key
  refused; a second click 409; blocked, then ready: seated once, and told once; no agent
  shows: nothing seated, the reason shown; the seat changed meanwhile: not seated; 10
  minutes: not seated, the place left; a restart forgets the start; `starting` in
  `page-json.txt`.
- Adapter: the start call carries the arguments as a list, in a place of its own, without
  taking focus.
- Page: the two dialogs, the refusals shown, the three languages in step.
- Once by hand, in a runtime session of its own with a temporary home: a real start in a
  folder the harness has never seen, through the trust prompt, to the seated message.

## Not done

- Starting on another machine. The way in: a project's folder on a machine becomes a
  record of the user's; until then only "where the last manager was" (the seat's machine
  and folder) could be offered.
- Resuming a seated manager's own session after its terminal closed (it would take its
  seat back by itself, with no seating).
- Harnesses other than Claude Code; choosing a permission mode or other arguments.
- Closing the started agent from the page; starting workers; removing a project.
