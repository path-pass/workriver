# Virtual workspaces: workriver's own, not bound to the runtime

Design, not built yet. A workspace stops being the record of a runtime group (machine, the
runtime's id, label) and becomes a record workriver makes at the user's word. It replaces
`workspace_at` / `workspace_mut` (a record found by the runtime's id, else by its label),
`not_another_open`, the links, and every use of a label as a key. Before the branch is
merged its rules go into `docs/security.md`, `caller-identity.md`, `layout.md`, `guards.md`,
`docs/design/ui/navigation.md`, `conversations.md`, `docs/runtimes/herdr.md` and the
recipes, and this file is removed.

## 0. Why not identify by the runtime (probed, Herdr 0.9.5, an isolated server)

| Fact | Consequence |
| --- | --- |
| After a restart the next new workspace takes highest id + 1: a closed newest workspace's id is given again. With the session file lost, ids start again at the first. | The runtime's id is no identity. |
| Two open workspaces may share a label; any program renames, creates and closes workspaces through the socket. Workspaces of other machines share names on purpose. | A label proves nothing. |
| Workspace metadata is display-only and written by any program. | A key of workriver's cannot be kept in the runtime. |

So nothing the runtime says about its workspaces decides anything. What it still gives:
the agents (session, harness, state, name, machine, working directory), the roots of their
terminals (who a caller is, `caller-identity.md`, unchanged), sending, focusing, and where
it opens a terminal of its own. An agent's runtime group is kept as a text to show ("in
Herdr: chat") and to open a temporary agent beside the one it serves; no record holds it
and no rule reads it.

## 1. The workspace

- **What.** `Workspace { id, name, project, manager, current, closed }`.
  - `id`: `ws<N>`, counting in the order they are made; never changes, never given again.
  - `name`: shown, nothing else. It may repeat; the page tells same names apart by project,
    then id.
  - `project`: the project folder it works on, held as an intent holds its project and
    compared the same way (`intent_project`). Set when it is made and never changed: other
    work gets another workspace. `workspace_project` (worked out from the manager's folder,
    the current intent, the intents that ran) and `workspace.project_set` go.
  - `manager`: the seat, section 2. `current`: the intent new work goes to, as today.
- **Who makes and closes one: the user only** (`Body::user_only`; the page, or the CLI
  from the user's own terminal).

  | Event | Rule |
  | --- | --- |
  | `workspace.created {id, name, project}` | the project is a registered one, or a folder an intent names |
  | `workspace.renamed {workspace, name}` | any time |
  | `workspace.closed {workspace}` | it has no active intent; the seat is emptied. Its intents and conversation stay, read-only |
  | `workspace.opened {workspace}` | a closed one, again |

- **One per project by default.** Adding a project (page or `workriver project add`)
  records `project.added`, then `workspace.created` named as the project, under one lock
  of the store. The user adds more (the rail's "+" on a project,
  `workriver workspace add <project> --name …`), e.g. one for a big intent. A project with
  no open workspace offers only "New workspace".
- **An agent's workspace**, wherever a command leaves the intent or workspace out, comes
  from the records alone: the workspace whose seat its session holds; else the workspace of
  the intent of the task it has (started and not delivered; the newest); else none, and the
  command asks for the intent. The user names the intent or the workspace.

## 2. The manager's seat

- **What is kept:** `manager: {session, machine}`, only to know who the manager is and to
  reach its terminal (`Runtime::send_to(machine, session)`, `focus`). Not its pane, not its
  runtime group. A manager on another machine holds the same seat the same way: there are
  no links.
- **Seated by the user's act, never by an agent alone.** One event, the user's only:
  `workspace.manager_set {workspace, manager: session | none, machine}`.
  - On the page: the workspace's line in the rail, Start's dialog, or Settings →
    Workspaces: "Manager" opens a select of the agents running now. Each line: name,
    harness, machine, the folder it was started in, its runtime group as a quiet hint,
    and what the records know of it (the tasks it has; "manager of ws2").
  - In the user's own terminal: `workriver workspace manager ws3 <session>` (a name only
    when exactly one running agent has it; else refused with the list), `--none` to empty.
  - The session must be one the runtime lists on that machine at the request (a read made
    then; it cannot be asked: 503).
  - A session holds one seat: seating it in another workspace empties its old one.
  - The user may seat over a manager that still runs (the page says it runs and asks once);
    that one is told it no longer manages.
  - The service then types into the new manager's terminal that the user made it the
    manager of `ws3` «name» (project, active intents), with `workriver guide manager`.
    **`workriver manager here` and `manager.registered` go**; the command is refused with
    how the user seats a manager.
- **When the manager's session ends** (the runtime no longer lists it on its machine): the
  seat stays. The workspace shows "manager not running"; what goes to it is "not delivered"
  as today; its open questions stay open. The same session running again (resumed, in any
  terminal the session rules allow) is the manager again with nothing to do.
- **A new session takes the seat only by the user's act again.** Needs you shows one row
  per open workspace that has an active intent and no running manager; it opens the same
  select, preset to the agent now running in the terminal the old session was first seen
  in (`session-roots`), when there is one. One click.
  *Open choice for the user's gate:* the alternative is that this successor in the same
  terminal takes the seat by itself. Recommended: the click; a seat is then never a side
  effect of what a terminal is reported to hold.

## 3. Everything goes by the workspace id

| Today (label or runtime group) | With virtual workspaces |
| --- | --- |
| `Intent.workspace: RuntimeRef`, rewritten by `workspace_mut`; `Intent.manager`, `runtime_ref` | `Intent.workspace: ws id`, set by a start or by the user (`intent.workspace_set {intent, workspace}`, same project; replaces `intent.linked`). `manager_of(intent)` = its workspace's seat. |
| A start names a runtime group; the service makes it the caller's and asks the runtime what is open | `intent.state_changed {active, workspace: ws id}`. The user: any open workspace of the intent's project (an intent of no project, from old records: any). A manager: only the workspace it holds the seat of, filled in by the rules from the seat, a body naming another refused; only an intent of that workspace's project. All of it in core's `prepare`: no runtime read, no two steps. |
| `message.sent` / `said` / `messages.read` name a label | the ws id (`""`: the conversation with every manager). `say`: the workspace the caller holds the seat of. |
| A copy to every manager names its runtime group | `Recipient {workspace: ws id, manager, machine}`: the seats of open workspaces whose session runs when it is sent. |
| `manages(label, session)`, `choice_open`, routing an answer | open while not answered and its asker holds the seat of the message's workspace. A rename changes nothing. |
| An agent reads a message through its group's record | the holder of the seat of the message's workspace, or a manager it went to. |
| `may_place`: a unit on another machine needs a link | the intent's manager (or the user) puts a unit on any machine workriver follows; another agent on its own machine only. |
| `/api/whoami`: the caller's group and its current intent | the caller's workspace by section 1 (id, name) and its current intent. |
| `App::who_in` gives the caller's group; `callers_workspace`, `not_another_open`, `groups_on` in `App::record_in` | `App::who` gives the principal; nothing else. A workspace an event names is checked by the rules against the store (it exists, is open, is the caller's by the seat). |
| The blind view shows which machine a workspace is on | a workspace has no machine; the seat's is an agent's machine and is cleared like one. |

## 4. The page

- **The rail: project → workspace → intent → unit.** Under the project switch ("All
  projects": a heading per project), each open workspace of the project is a group that
  folds: its name with its id quietly in mono, and one status line, the manager (crown,
  name, working / idle / not running, or "no manager" in amber when it has an active
  intent). That line moves here from every intent's. Under it its active intents with their
  units, as today. The project's **Roadmap** (planned intents are in no workspace) and
  **Done** (folded; each done intent names the workspace it ran in) follow its workspaces.
  A project's "+" adds a workspace; a workspace's menu: rename, manager, close. A closed
  workspace is not in the rail; Settings → Workspaces lists them (open again there).
- **Start** picks an open workspace of the intent's project (the only one, or the one last
  started in, preselected), each with its manager's name and state. One without a running
  manager shows the manager select in the dialog. "Other projects' workspaces" and its
  confirmation go: an intent runs in a workspace of its project.
- **The Manager select**: "All managers", then one entry per open workspace by id (name,
  and the project where names repeat or all projects are shown): the open unit's first,
  then the rail's project's, then the rest; closed workspaces that have messages last,
  read-only. No folding of equal names. A conversation follows the workspace, not the
  manager: a new manager sees it; an earlier manager's question says it can no longer be
  answered, as today.
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
  format mark (workriver-data's own); the service refuses a store with events and no mark,
  naming the script. A new store is marked when made.
- **Where:** `scripts/migrate-virtual-workspaces/`, a Cargo package of its own, not a
  member of the workspace: never built, tested or shipped with the product. It is built
  against workriver-core and workriver-data as of the last commit before this change (a
  work tree under `.local/`), so old events are replayed by the very code that wrote them,
  not by a copy of its matching. It is removed from the repository once the user's store
  is migrated.
- **Three steps, the source never written** (opened read-only; the output is a new file,
  refused when it exists or is the live store's path):
  1. `plan <copy.db>` prints, and writes as a file the user may edit, the mapping: each
     old record → a workspace (name, project, open or closed), and what could not be told.
  2. `apply <copy.db> <mapping> <out.db>` writes the new events.
  3. `check <copy.db> <state.json>` compares, below.
- **What it maps**, replaying the old events one by one with the old code and taking, for
  each event, the record it wrote or resolved to then:

  | Old | New |
  | --- | --- |
  | A workspace record | `workspace.created` (as the user), just before the first event that made the record: name = its last label; project = today's `workspace_project`, else the user's in the mapping. Proposed closed when it has no active intent and no manager. |
  | Records merged by label (a new id after a restart) | stay one workspace. |
  | A linked pair | one workspace, the local one: a linked group's events already resolve to it. The link events are dropped. A record of the other machine from before the link, which nothing resolves to any more, is dropped and listed. |
  | Same label, not linked (another machine, or two here) | each its own workspace. |
  | `manager.registered` | `workspace.manager_set`, in place, same actor and time, for the record it wrote, with the agent's machine. |
  | A start, `intent.created` with a group, `intent.linked` | the start with the ws id; `intent.workspace_set`. Where today's last answer differs from the record at the event (a link made later), one `intent.workspace_set` at the end makes it today's. |
  | `message.sent`, `message.said`, each copy to every manager | the ws id of the record the event resolved to. A label that found no record: a closed workspace made for it. |
  | `messages.read {label}` | one mark per workspace that carried the label then (they were read together). |
  | `workspace.project_set` | folded into the workspace's project; dropped. |
  | A project with no workspace after all this | `workspace.created`, named as the project, at the end. |
  | Every other event | unchanged, byte for byte, in order. |

  It stops, listing each, instead of guessing: a record whose intents are of several
  projects, or whose project nothing says (the user writes it in the mapping; a second
  workspace takes the other project's intents); an intent of no project in a workspace (it
  takes the workspace's project in `intent.created`, once the mapping says so).
- **How it is checked**, all on the copy:
  1. The product's copied-database replay (`replays_a_copied_database`) of `out.db` gives
     the new state: it replays at all, and the service starts on it.
  2. `check` compares that state with the old one: everything that is not a workspace is
     equal (intents but their workspace fields, units, tasks, findings, gates, chores,
     questions, threads, the backlog, agents, projects; every message's text, replies,
     deliveries and answers); every intent's workspace and `manager_of` are the mapping of
     the old ones; every message is in the conversation the mapping says; unread counts per
     conversation add up to the old ones per label; each open question is open (one that a
     rename had closed opens again: listed). A difference no line of the mapping explains
     fails the check.
  3. The user opens the page on the copy (its own home and port): the rail, each
     conversation, Needs you.
  4. Then the service is stopped, the old store kept, `out.db` put in its place and the new
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

**An agent cannot**

- make, rename, close or open a workspace, or seat anyone, itself included;
- become a workspace's manager, or get its intents, conversation or read marks, by
  anything it does in the runtime: making, naming, renaming or closing a group, or
  running in one;
- start an intent anywhere but in the workspace it was seated in, or one of another
  project; read another workspace's conversation; close the user's question or block
  another manager by a name.

**Limits that stay** (the runtime's interface and the files are the user's account's):

- **A session whose terminal is gone can be taken** (`caller-identity.md`, unchanged), and
  the seat is a session: a program that closes the manager's terminal through the runtime
  and reports its session is the manager. The agent's terminal disappears; nothing is
  taken unseen. This is now the only way to a seat without the user.
- **Bait for the user's pick.** An agent's name and runtime group are what any program
  reports; the select also shows the machine, the folder and what the records know. A
  wrong pick is the user's to seat again.
- A manager on another machine is found as any caller there is (weaker, as today); that
  machine's listener serves no seating.
- A command typed into a plain terminal is the user's (as today), seating included.
- Closing the manager's terminal stops its workspace's deliveries; nobody gains them.

## 7. Not done

- An agent asking for a seat for the user to accept; a join code; the successor in the same
  terminal taking the seat by itself (section 2).
- Changing a workspace's project; merging two workspaces; moving a conversation.
- Two managers in one workspace; one session managing two.
- Any reading of old-form events in the product.
- workriver making or arranging the runtime's groups for a workspace, or starting agents.
- Splitting what old events merged by label. Machines known by anything but their label.

## 8. Tests the implementation must pass

The three review reproductions, as they now read: two workspaces of one name have two
conversations and two sets of read marks, and neither manager reads the other's; a rename
keeps a question open. The ownership probes: no agent event sets a seat, a workspace or a
project; a start names only the seated workspace; a runtime group made, renamed or closed
changes no record. The seat: one per session, the user seats over a running one, a session
ended and resumed is the manager again, a new session is not until seated, a session not
listed on that machine is refused. A manager and units on another machine without a link.
Closing refused with an active intent. The route-access guard for reading a message. The
store without a format mark refused. The script: each row of section 5 on a fixture of old
events, and `check` failing on an unexplained difference.
