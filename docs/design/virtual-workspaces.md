# Virtual workspaces: workriver's own, not bound to the runtime

Design, not built yet. Merged from two designs written independently; section 9 says where
they differed and which way this one takes. A workspace stops being the record of a runtime
group (machine, the runtime's id, label) and becomes a record workriver makes at the user's
word. It replaces `workspace_at` / `workspace_mut` (a record found by the runtime's id, else
by its label), `not_another_open`, the links, and every use of a label as a key. Before the
branch is merged its rules go into `docs/security.md`, `caller-identity.md`, `layout.md`,
`guards.md`, `docs/design/ui/navigation.md`, `conversations.md`, `docs/runtimes/herdr.md`
and the recipes, and this file is removed.

## 0. Why not identify by the runtime (probed, Herdr 0.9.5, an isolated server)

| Fact | Consequence |
| --- | --- |
| After a restart the next new workspace takes highest id + 1: a closed newest workspace's id is given again. With the session file lost, ids start again at the first. | The runtime's id is no identity. |
| Two open workspaces may share a label; any program renames, creates and closes workspaces through the socket. Workspaces of other machines share names on purpose. | A label proves nothing. |
| Workspace metadata is display-only and written by any program. | A key of workriver's cannot be kept in the runtime. |

So nothing the runtime says about its workspaces decides anything. What it still gives:
the agents (session, harness, state, name, machine, working directory), the roots of their
terminals (who a caller is, `caller-identity.md`), sending, focusing, and where it opens a
terminal of its own. An agent's runtime group is kept as a text to show ("in Herdr: chat")
and to open a temporary agent beside the one it serves; no record holds it and no rule
reads it.

## 1. The workspace

- **What.** `Workspace { id, name, project, manager, current, closed }`.
  - `id`: `ws<N>`, counting in the order they are made; never changes, never given again.
  - `name`: shown, nothing else. It may repeat; the page shows the id beside it.
  - `project`: the id of a registered project (`Project.id`). Set when the workspace is
    made, never changed: other work gets another workspace. An intent is of the workspace's
    project when `intent_project` is that project's folder. `workspace_project` (worked out
    from the manager's folder, the current intent, the intents that ran) and
    `workspace.project_set` go.
  - An intent's project, for the rules, is the registered project holding the folder it
    names, else none: `intent_project` no longer returns a folder no registered project
    holds. And `intent.created` names such a folder or none, whoever records it (refused
    otherwise, saying to add the project first): today an agent's planned intent may name
    any folder, and nothing checks it.
  - `manager`: the seat, section 2. `current`: the intent new work goes to, as today.
- **Who makes and closes one: the user only** (`Body::user_only`; the page, or the CLI
  from the user's own terminal).

  | Event | Rule |
  | --- | --- |
  | `workspace.created {id, name, project}` | the project is registered |
  | `workspace.renamed {workspace, name}` | any time |
  | `workspace.closed {workspace}` | it has no active intent; the seat is emptied. Its intents and conversation stay, read-only |
  | `workspace.opened {workspace}` | a closed one, again |

- **One per project by default.** Adding a project (page or `workriver project add`)
  records `project.added`, then `workspace.created` named as the project, under one lock
  of the store. The user adds more (the rail's "+" on a project,
  `workriver workspace add <project> --name …`), e.g. one for a big intent or a separate
  conversation. A project with no open workspace offers only "New workspace". A workspace
  needs no running terminal.
- **An intent's workspace.** A planned intent is in none: it is in its project's roadmap,
  one ordered list per project. A planned intent of no project (old records, section 5) is
  in the roadmap of "No project", shown after the projects; only the user starts it.
  A start puts it in a workspace of its project (section 3);
  the user may move an active intent to another workspace of the same project
  (`intent.workspace_set`). Messages sent before a move stay in the conversation they were
  in. A done or dropped intent keeps the workspace it ran in.
- **An agent's workspace**, wherever a command leaves the intent or workspace out, comes
  from the records alone: the workspace whose seat its session holds; else the workspace of
  the intents of the tasks it has (started and not delivered) when that is one workspace;
  else none, and the command asks for the task or intent. Nothing picks the newest.

## 2. The manager's seat

- **What is kept:** `manager: {session, machine, folder, since}`, to know who the manager
  is and to reach its terminal (`Runtime::send_to(machine, session)`, `focus`). `folder`
  is the working directory the runtime showed for it when it was seated (the service fills
  it in from the same read as the machine); it is shown, and compared once (below). `since`
  is the revision of the event that seated this session: the **appointment**. Not its pane,
  not its runtime group. A manager on another machine holds a seat the same way: there are
  no links.
- **Seated by the user's act, never by an agent alone.** One event, the user's only:
  `workspace.manager_set {workspace, manager: session | none, machine, folder, replaces}`
  (machine and folder filled in by the service from its read of the runtime, below).
  - `replaces` is the seat the user was shown (its session and `since`, or none). The rules
    refuse the event when the seat is no longer that one (409, the page shows the seat
    again): a change made meanwhile is never overwritten unseen. Checked and recorded under
    one lock, in core's `prepare`.
  - On the page: the workspace's line in the rail, Start's dialog, or Settings →
    Workspaces: "Manager" opens a select of the agents running now. Before the click the
    page shows the project, the workspace's name and id, and for each agent: name, harness,
    machine, the folder it was started in, its runtime group as a quiet hint, and what the
    records know of it (the tasks it has; "manager of ws2").
  - In the user's own terminal: `workriver workspace manager ws3 <session>` (a name only
    when exactly one running agent has it; else refused with the list), `--none` to empty.
    It sends the seat it read as `replaces`.
  - **The session's root is read at the seating, on its own machine.** The service reads
    that machine's agent roots then (`Runtime::agent_roots(machine)`, this machine's or
    another's alike; one that cannot be asked: 503). The seat is refused when the session
    is shown at no root there, at more than one, or at a root other than the one it is
    pinned to while that root runs (`caller-identity.md` rule 5); the refusal names the
    places. A session with no pin, or whose pinned root is gone, is pinned at the seating to
    the one root it is shown at. So on another machine too the seat's root is the one the
    runtime shows for that session when the user seats it, not whichever root called first
    with it (today a remote pin goes to the first caller, and only this machine is looked
    at for a session shown twice, `App::one_place`).
  - A session holds one seat: seating it in another workspace empties its old one.
  - The user may seat over a manager that still runs (the page says it runs and asks once);
    that one is told it no longer manages.
  - The service then types into the new manager's terminal that the user made it the
    manager of `ws3` «name» (project, active intents), with `workriver guide manager`.
    **`workriver manager here` and `manager.registered` go**; the command is refused with
    how the user seats a manager.
- **A seated session keeps its root.** Rule 5's pin of a session that holds a seat is not
  released when its root stops running, and no other root takes it by reporting the
  session: such a caller is refused, the refusal saying the user confirms the manager's new
  terminal. The workspace then shows "manager not running"; what goes to it is "not
  delivered" as today. The same root running again (nothing was closed) is the manager
  again with nothing to do.
- **Any other terminal or session takes the seat only by the user's act.** Needs you shows
  one row per open workspace that has an active intent and no running manager. It opens the
  same select, preset to the seated session when it now runs at another root (a resume, a
  runtime restart), else to the agent now running in the terminal the old session was
  pinned to, when there is one. One click; the seating is checked as above.
  - **A seated session at a new root is shown as a new terminal**, never as "the manager,
    resumed": the row says that a new terminal reports this session, and shows the old
    root's machine and folder (the seat's, "gone") beside the new root's (and when it
    started). The session id is what any program may report (section 0), so the preset is
    a proposal like any other.
  - **The appointment is kept only where the new root is where the old one was.**
    Confirming the same session keeps `since` (open choices stay answerable, queued sends
    are delivered) only when the new root's machine and folder are the seat's. Otherwise
    the confirmation is a new appointment, as for another session: the earlier
    appointment's choices close and its queued sends are "not delivered". The page says
    which of the two the click does before it is made.
  - Another session always gets a new `since`.
- Emptying the seat or closing the workspace releases the pin to rule 5 as it is for any
  agent.

## 3. Everything goes by the workspace id

| Today (label or runtime group) | With virtual workspaces |
| --- | --- |
| `Intent.workspace: RuntimeRef`, rewritten by `workspace_mut`; `Intent.manager`, `runtime_ref` | `Intent.workspace: ws id`, set by a start or by the user (`intent.workspace_set {intent, workspace}`, same project; replaces `intent.linked`). `manager_of(intent)` = its workspace's seat. |
| A start names a runtime group; the service makes it the caller's and asks the runtime what is open | `intent.state_changed {active, workspace: ws id}`. The user: any open workspace of the intent's project (an intent of no project, from old records: any, and the intent takes that project). A manager: only the workspace it holds the seat of, filled in by the rules from the seat, a body naming another refused; only an intent of that workspace's project. All of it in core's `prepare`: no runtime read, no two steps. |
| `message.sent` / `said` / `messages.read` name a label | the ws id (`""`: the conversation with every manager). `say`: the workspace the caller holds the seat of. The page's URLs name the ws id. |
| A copy to every manager names its runtime group | `Recipient {workspace: ws id, manager, machine, since}`: the seats of open workspaces whose session runs when it is sent. |
| Queued delivery goes to whoever the group's manager is | delivered only while the seat still has the recipient's session and `since`; else "not delivered", never re-routed. A reply is taken only from that appointment. |
| `manages(label, session)`, `choice_open`, routing an answer | a choice belongs to the workspace and the appointment that asked it: open while not answered and the seat's `since` is the one it was asked under. A rename changes nothing; a new manager reads the conversation and asks anew. |
| An agent reads a message through its group's record | the holder of the seat of the message's workspace, or the appointment a copy went to. |
| `may_place`: a unit on another machine needs a link | the intent's manager (or the user) puts a unit on any machine workriver follows (following one is the user's act); another agent on its own machine only. Workers join through the tasks they are given; there is no membership list. |
| `/api/whoami`: the caller's group and its current intent | the caller's workspace by section 1 (id, name) and its current intent. |
| `App::who_in` gives the caller's group; `callers_workspace`, `not_another_open`, `groups_on` in `App::record_in` | `App::who` gives the principal; nothing else. A workspace an event names is checked by the rules against the store (it exists, is open, is the caller's by the seat), never taken from the request. |
| The blind view shows which machine a workspace is on | a workspace has no machine; the seat's is an agent's machine and is cleared like one. |

**Conversations are read by their workspace only.** Today `App::view` gives an agent every
record, every workspace's messages included. Now the view, for an agent, keeps:

- the messages of the workspace whose seat it holds;
- of a message to every manager, only when a copy of it went to its current appointment:
  the message itself (the user's text, what it points at, the user's attachments), its
  own copy and its own replies, and the user's answers to those replies. Nothing of the
  other copies: not their workspace, manager or delivery, not their replies (text,
  choices, answers), not the attachments those replies or answers carry. Such a message
  is one record (`Message.to` holds every copy, `Message.replies` every manager's
  replies), so the view cuts it down; keeping or dropping it whole would not do;
- the read marks of the workspace whose seat it holds; none other.

The same view serves every route that returns records, on both listeners (on main:
`/api/state`, the snapshot `/api/workspace`, `/api/task` and the attachment route;
`message show` is a command of the unmerged choices branch and goes through it once that
is merged). An attachment is served to an agent only when a record the view kept grants
it. The user sees all; the blind view stays
as it is within that.

**What stays readable by every agent:** everything that is not a conversation with the
user: intents (planned and active, every workspace's and project's), units, tasks,
findings, gates, chores, the task discussions (questions) and their threads, the agents.
That is intended: a review reads another unit's work, `ask` reaches any task, a manager
plans against the whole roadmap. The blind view still narrows it for an agent on a blind
task. Scoping more than the conversations is not part of this (section 7). What the
scope binds, and what it cannot, is under the limits (section 6).

## 4. The page

- **The rail: project → workspace → intent → unit.** Under the project switch ("All
  projects": a heading per project), each open workspace of the project is a group that
  folds, the only one included: its name with its id quietly in mono, and one status line,
  the manager (crown, name, working / idle / not running, or "no manager" in amber when it
  has an active intent). That line moves here from every intent's. Under it its active
  intents with their units, as today. The project's **Roadmap** (planned intents are in no
  workspace; the order is the project's) and **Done** (folded; each done intent names the
  workspace it ran in) follow its workspaces. After the projects, "No project" holds the
  planned intents of no project (old records only: section 1), with Start for the user
  only; it is not shown when it is empty. A project's "+" adds a workspace; a
  workspace's menu: rename, manager, close. A closed workspace is not in the rail; Settings
  → Workspaces lists them (open again there). A link to an intent or unit opens its
  workspace's group.
- **Start** picks an open workspace of the intent's project (the only one, or the one last
  started in, preselected), each with its manager's name and state. One without a running
  manager shows the manager select in the dialog. "Other projects' workspaces" and its
  confirmation go: an intent runs in a workspace of its project.
- **The Manager select**: "All managers", then one entry per open workspace by id (name,
  and the project where names repeat or all projects are shown): the open unit's first,
  then the rail's project's, then the rest; closed workspaces that have messages last,
  read-only. No folding of equal names. A conversation follows the workspace, not the
  manager, and shows its history with the seat empty or the manager not running: a new
  manager sees it; an earlier appointment's question says it can no longer be answered.
- **Needs you**: the rows of managers' messages are per workspace id; an agent waiting in
  its terminal names its workspace by section 1 (else its runtime group as a hint); new:
  the row of section 2 for a workspace whose manager does not run.
- **Agents** are grouped by workspace (section 1), the rest under "In no workspace"; an
  agent's line may show its runtime group quietly, as where its terminal is.
- **Settings**: "Workspaces' projects" and the links go; "Workspaces" lists each with its
  name, project, manager and state.

## 5. Old data: one script, run once, outside the product

- **The product reads only the new form.** `Body` loses `workspace.linked`,
  `workspace.unlinked`, `workspace.project_set`, `manager.registered`, `intent.linked` and
  `intent.created`'s `runtime_ref`; `RuntimeRef` leaves the records. The store carries a
  format mark: a schema step of workriver-data's (`MIGRATIONS`, counted by
  `user_version`) that `Store::open` runs only on a store with no events, so a new store is
  marked when made. A store with events and without that step is refused, naming the
  script; the step is never run on it.
- **Where:** `scripts/migrate-virtual-workspaces/`, two Cargo packages, `old/` and `new/`,
  neither a member of the workspace: never built, tested or shipped with the product. Each
  has an empty `[workspace]` table of its own (without one, Cargo takes a package under
  the root for a part of the root's workspace and refuses to build it; the root's
  `members = ["crates/*"]` stays as it is). Two packages because each is built against
  one version of workriver-core and workriver-data, and reads only the form that version
  knows:
  - `old/` against the last commit before this change (a work tree under `.local/`), so
    old events are replayed by the very code that wrote them, not by a copy of its
    matching;
  - `new/` against this branch, so the new events are decoded, checked and stored by the
    product's own code.

  Both are removed from the repository once the user's store is migrated.
- **Four steps, on a copy taken with the service stopped, the copy never written**
  (opened read-only; every output is a new file or folder, refused when it exists or is
  the live store's path):
  1. `old plan <copy.db> <mapping>` prints, and writes as a file the user may edit, the
     mapping: each old record → its workspaces (name, project, open or closed), and every
     point it could not tell, each a line the user fills in.
  2. `old apply <copy.db> <mapping> <dir>` writes, refused while a line of the mapping is
     unfilled: `<dir>/events.jsonl`, the new events in order, one `{at, actor, kind,
     body}` per line, the new kinds' bodies written by the script in the new form's JSON
     (old kinds' bodies as stored, byte for byte); `<dir>/seats.jsonl`, the old
     registrations as seats (below); `<dir>/old-state.json`, the old state as the old code
     replays the copy, with each old record's and message's mapped workspace.
  3. `new write <dir> <out.db>`: `Store::open` makes `out.db` (empty, so it is marked);
     then each line is decoded as the new `Body` (one that does not decode fails the run),
     checked against the new rules (below), and inserted as a row with its own `at` and
     `actor`, the way workriver-data's `events::insert` writes one (the same columns; kind,
     body and refs from the new `Body`), all in one transaction. This insert is the one
     write of the events table outside workriver-data, and it goes with the script.
     `Store::open` of the result then replays it with the product's code.
  4. `new check <dir> <out.db>` compares, below.
- **What it maps**, replaying the old events one by one with the old code and taking, for
  each event, the record it wrote or resolved to then (event-time attribution: what an
  event meant when it was recorded, not today's answer):

  | Old | New |
  | --- | --- |
  | A workspace record | `workspace.created` (as the user), just before the first event that made the record: name = its last label; project = the registered project of its intents' folders (after the folder lines below). Proposed closed when it has no active intent. |
  | A record whose intents are of several projects | one workspace per project, each taking that project's intents; a message goes with the intent it names. A message naming none, of such a record: a line of the mapping. |
  | A record whose project nothing says (no intent with a project) | a line of the mapping; an intent of no project in it takes the project written there, in its `intent.created`. |
  | A folder an intent or `workspace.project_set` names that no registered project holds | never registered by the script on its own: a line of the mapping, which `plan` shows with every event that named the folder, its kind, time and who wrote it (the user, or which agent). The user writes one of three: `register` (a `project.added` as the user, before the first event that names it: the user's act, written in the mapping by the user); `into <project>` (its intents' `intent.created` name that registered project's folder); `none` (their `intent.created` names no project). An intent's project is decided by its folder's line before the record rows below read it. |
  | A planned intent of no project, or whose folder became `none` | stays planned, in no workspace: the roadmap of "No project" (section 4), in its old order; only the user starts it. An active one of no project is in the workspace of its record's line. |
  | Records merged by label (a new id after a restart) | stay one workspace. |
  | A link | creates no workspace and no permission; the link events are dropped. While it held, the linked group's events go to the workspace it was linked into (they resolved to that record); before it and after the unlink, to the remote record's own workspace (closed unless it has an active intent). |
  | Same label, not linked (another machine, or two here) | each its own workspace. |
  | `manager.registered` | dropped: the seats start empty (an agent's registration is not the user's seating); written only to `seats.jsonl`, for the check. Needs you then offers each workspace with an active intent its last registered session, when it runs: one click. |
  | A start, `intent.created` with a group, `intent.linked` | the start with the ws id; `intent.workspace_set`. |
  | `message.sent`, `message.said`, each copy to every manager | the ws id of the record the event resolved to then. A label that found no record: a closed workspace named as the label, its project from the intent the messages name, else a line of the mapping. |
  | `messages.read {label}` | one mark per workspace that carried the label then: the old mark was by label, so it read them all. |
  | An open choice, an undelivered or queued send | closed / "not delivered", never re-routed: the appointment it belonged to does not exist in the new form. `plan` lists them, so that the user answers them before migrating. |
  | `workspace.project_set` | folded into the workspace's project; dropped. |
  | A registered project with no workspace after all this | `workspace.created`, named as the project, at the end. |
  | Every other event | unchanged, byte for byte, in order: ids, authors, times, attachment ids. |

- **How it is checked**, all on the copy:
  1. **Every new event passes the new rules** (in `new write`, before it is stored): the
     state of the events before it, as the new code applies them, `prepare`s it under the
     principal its actor names (`user`: the user; `workriver`: the service; else the agent
     of that session, on the machine its `agent.seen` gives), and the prepared body must be
     the line's (the ids and numbers `prepare` gives come out as written). The old
     registrations are not records of the new form, yet an old manager's starts, says and
     replies were allowed by them; so for this check only, the seats of `seats.jsonl` (each
     workspace's registered session from where it registered, `since` the revision of the
     last new event before that point) are set on that state in memory before each event, and written
     nowhere. A refusal, or a body that comes out different, fails the run, listed with the
     event and the rule's words; it is mended in the script or by a line of the mapping,
     never by a looser rule in the product.
  2. The product's copied-database replay (`replays_a_copied_database`) of `out.db` gives
     the new state: it replays at all, and the service starts on it.
  3. `check` compares that state with `old-state.json`: everything that is not a workspace
     is equal (intents but their workspace fields and the projects their folders' lines
     changed, units, tasks, findings, gates, chores, questions, threads, the backlog,
     agents, projects but the ones `register` added; every message's text, replies,
     deliveries, answers and attachments); every intent's workspace is the mapping of the
     old one; every message is in the conversation the mapping says; unread counts per
     conversation add up to the old ones per label; the choices and sends of the table's
     row are the ones `plan` listed. A difference no line of the mapping explains fails
     the check, listed.
  4. The user opens the page on the copy (its own home and port): the rail, each
     conversation, Needs you.
  5. Then the service is stopped, the old store kept, `out.db` put in its place and the new
     version started. Going back is the old store with the old binary.

## 6. What goes, security, limits

**Deleted or simpler.**

- Core: `Link`, `links`, `link_of`, `workspace_at` / `workspace_of` / `workspace_mut` /
  `workspace_found_mut`, `same_group`, `workspace_project`'s guessing, `manages` by label,
  `Intent.manager` and `runtime_ref`. A workspace is found by its id, as an intent is.
- Service: `callers_workspace`, `not_another_open` (its 409 and 503), `may_manage`'s look
  at who runs, the runtime read before the lock in `record_in`, `who_in`'s group, the
  `--as` fallback that lists the agent's group, `Runtime::groups` / `groups_on`.
- The ownership rules: "a workspace is what the runtime says it is", the rule that a label
  finds only a closed workspace's record and its same-label refusal, "an intent stays with
  its workspace, not with a name", and their limits (a workspace renamed or closed in the
  runtime; a label a link names) have nothing left to hold. What stays: an intent's
  project, where it runs and its manager are set only by the user or by a start the rules
  checked; now all three are checked in core.
- CLI: `workspace link`, `unlink`, `project`, `manager here`. Recipes: `machines`,
  `units-other-machine`, `start-intent`, `updating`, the manager skill, `CHANGES.md`.
- Guards: the tests of the same-label refusal, of links and of `workspace_mut` go; the
  replay guard's fixtures and expected state are written again in the new form; fixtures
  of old events live with the script only.

**What acts as the user.** Making, renaming, closing and opening a workspace, seating and
moving an intent are `App::user` in their handlers and `Body::user_only` in the rules, and
none is on `machine_router`.

**An agent cannot**

- make, rename, close or open a workspace, or seat anyone, itself included;
- become a workspace's manager, or get its intents, conversation or read marks, by
  anything it does in the runtime: making, naming, renaming or closing a group, running in
  one, or reporting a seated session from another terminal;
- start an intent anywhere but in the workspace it was seated in, or one of another
  project; through any of the service's routes, read another workspace's conversation or
  read marks, or, in a message to every manager, another manager's copy, replies, the
  user's answers to them or their attachments (what this does not cover: the limits
  below); close the user's question or block another manager by a name;
- receive what was queued for an earlier appointment;
- name, as an intent's project, a folder no registered project holds; or have a folder it
  wrote registered as a project, the migration included (section 5).

**Limits that stay** (the runtime's interface and the files are the user's account's):

- **A seated manager's new terminal needs the user.** After a runtime restart every root
  is new: each workspace with a manager waits for one click (section 2). That is the price
  of a seat never moving by itself.
- **A session whose terminal is gone can still be taken** when it holds no seat
  (`caller-identity.md`, unchanged): a worker's session, never a manager's.
- **The read scope binds the service, not the machine.** It holds for every route of the
  service, and so completely for agents on other machines, which reach the records only
  through their machine's listener. An agent on this machine runs as the user: it can read
  the store and the attachments in the data home from disk (as `docs/security.md` already
  says for blind reviews), and under Herdr it can read any terminal's screen, the
  manager's included, where the user's messages are typed. And by design every agent reads
  every workspace's intents, units, tasks, questions and threads (section 3). The
  security document written from this design says all of this, and promises no more.
- **Bait for the user's pick.** An agent's name, session id and runtime group are what any
  program reports; the select also shows the machine, the folder and what the records
  know. This holds for the Needs you row's preset too: a program that reports the seated
  session id at a root of its own before the real manager resumes is offered as that
  session. The row says the terminal is new and shows both roots (section 2), and keeps
  the appointment only when machine and folder are the old root's; a program in the same
  folder on the same machine still gets it by the user's click. A wrong pick is the user's
  to seat again.
- **A manager on another machine** is pinned at its seating to the one root that
  machine's runtime shows for the session, and is refused where it is shown at two
  (section 2); the first root to call no longer decides. What stays weaker than here: the
  process facts there come through the runner and the listing from that machine's
  runtime, so they hold as far as that machine's account does; and between seatings a
  remote worker's session is pinned to the first root that calls, as today. That
  machine's listener serves no seating.
- A command typed into a plain terminal is the user's (as today), seating included.
- Closing the manager's terminal stops its workspace's deliveries; nobody gains them.
- `session-roots.json` can be written from disk without the service (as today).

## 7. Not done

- An agent asking for a seat for the user to accept; a join code; a successor in the same
  terminal taking the seat by itself.
- Changing a workspace's project; merging two workspaces; moving a conversation.
- Two managers in one workspace; one session managing two.
- Scoping by workspace, for agents, anything but the conversations: intents, units, tasks,
  questions, threads (section 3). If the user wants the threads scoped, it is a change of
  its own.
- Any reading of old-form events in the product.
- workriver making or arranging the runtime's groups for a workspace, or starting agents.
- Splitting what old events merged by label; recovering what the old log did not record.
  Machines known by anything but their label. Agents outside the supported runtime.

## 8. Tests the implementation must pass

The three review reproductions, as they now read: two workspaces of one name have two
conversations and two sets of read marks, and neither manager reads the other's, through
`/api/state` or the snapshot (and `message show` once the choices branch that adds it is
merged); a rename keeps a question open. A message to every manager: manager B's reply,
the user's answer to it and an attachment of that reply are not in manager A's
`/api/state`, snapshot or attachment route, nor is B's copy (its workspace, manager,
delivery); A sees the message, its own copy and its own replies. The ownership probes: no
agent event sets a seat, a workspace or a project; a start names only the seated
workspace; a runtime group made, renamed or closed changes no record; an `intent.created`
naming a folder no registered project holds is refused (the review's probe, an agent's
planned intent naming `/Users/someone/.ssh`, now refused). The seat: one per session; a
change against a seat other than the one shown is refused; the user seats over a running
one; a seated session reported from another root is refused until the user confirms;
confirming keeps the appointment when the new root's machine and folder are the seat's,
and is a new appointment (earlier choices closed, queued sends not delivered) when they
are not; a new session is not the manager until seated, and its seating closes the
earlier appointment's choices; a session not listed on that machine, or shown at two
roots there, is refused; on another machine (a fake runtime with two machines), a session
first called with from one root and shown by that machine's runtime at another is refused
at seating, one shown at one root with no pin is pinned there, and a caller from another
root is then refused; a send queued for an appointment that was replaced is not
delivered. A manager and units on another machine without a link; a worker with tasks in
two workspaces is asked for the task. Closing refused with an active intent. The
route-access guard for reading a message and an attachment granted by one. The store
without a format mark refused, and never marked by opening it. The script: each row of
section 5 on a fixture of old events, among them an old link, a conversation the mapping
must settle, and a folder an agent named (left unregistered unless the mapping says
`register`); `write` failing on an event the new rules refuse and on a body that comes
out different; `check` failing on an unexplained difference.

## 9. Where the two designs differed

Both designs agree on: an id of workriver's that is never given again; a name that is
shown only; a project fixed when the workspace is made; one workspace made with each
project, more at the user's word; the seat (session and machine) set by the user only, one
per workspace and one per session; no links, a manager on any followed machine; messages,
read marks, choices and the current intent by workspace id; the rail project → workspace →
intent → unit with the manager on the workspace's line; a one-off script outside the
product, on a copy, by a mapping the user can edit, checked against the old state, the old
store kept; the product reading only the new form.

| Point | First design | Second design | Taken, and why |
| --- | --- | --- | --- |
| A workspace's project | a folder, as an intent's | a registered project's id; unregistered folders registered by the script | Second for the id: the rail is by registered project, so no workspace hangs under a folder that is none. Not the registering: an agent may have written the folder, so each is a line of the mapping for the user (section 5, after the security review). |
| Closing a workspace | `closed` / `opened` | not said | First: the rail needs to drop finished workspaces without losing their conversation. |
| A seat change racing another | the page asks once | the event names the seat the user saw; checked atomically | Second, with the first's question: a seat is never overwritten unseen. |
| A seated session in a new terminal | the manager again by itself; "a session whose terminal is gone can be taken" left as a limit | the pin stays; a new root needs the user | Second: it closes the only way to a seat without the user, on another machine too once the seating reads that machine's roots (section 2); the click is the Needs you row the first design already had for a new session, and keeps the appointment only where the new root is where the old one was. Cost: one click per workspace after a runtime restart. |
| Who owns an open choice | the session holding the seat | the appointment (the seating event) | Second: seated, replaced and seated again is a new appointment; re-confirming the same session after a restart is not. |
| Queued delivery after the seat changed | not said | checked against the appointment | Second: old work never reaches a new manager. |
| Agents reading conversations | by route (`message show`, a command of the unmerged choices branch) | the snapshot, direct reads and attachments filtered | Second: `App::view` gives every agent every workspace's messages today; the first design missed that. A message to every manager is cut down to the agent's own copy and replies (section 3). |
| An agent's workspace when a command omits it | the newest task's | only when unique, else asked | Second: no guessing. |
| Planned intents | in no workspace; Roadmap and Done per project | each in a workspace (the default); Active, Roadmap, Done per workspace | First: the roadmap is one ordered list per project and an intent is planned there until started; a closed workspace's done intents stay findable. |
| A unit on another machine | the intent's manager places it | the user picks its destination | First: following a machine is already the user's act; a click per unit would be the link again in another shape. |
| Old seats | `manager.registered` rewritten as the user's seating | empty, for the user to confirm | Second: an agent's registration written as the user's act would be a forged record; Needs you makes it one click. |
| Old open choices and queued sends | kept open (one a rename closed opens again) | closed, never re-routed | Second: their appointment does not exist in the new form; `plan` lists them so the user answers first. |
| A record of several projects, or of none | the script stops and asks | split by the intents' projects; ambiguities to the user | Both: split where the intents say, a line of the mapping where they do not. |
| A message no record received | a closed workspace made for it | a user-only archive | First: a closed workspace is already read by the user only; no new kind of record. |
| Old read marks | every workspace that carried the label | only where the scope is provable | First, which is the second's rule: the old mark's scope was the label. |
| Old links | a linked pair is one workspace; the remote record before it dropped | explains attribution only while in force | Second: nothing an event was attributed to is dropped. |
