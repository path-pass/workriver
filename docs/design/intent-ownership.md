# An intent's project, workspace and manager: who decides

Status: design, not confirmed. Its rules go into `docs/security.md` (both languages) and
`docs/design/caller-identity.md` before the branch is merged; this file is then removed.

Every rule about who may change an intent reads three facts: the **project** the intent
belongs to, the **workspace** it runs in, and its **manager**. The rule this design holds:
**each of the three is set only by the user, or by a start the rules checked. No other
event, and no name in a request, sets them.** The reviews of intent editing found four
holes in that rule, and the security review of this design two more. Each was in a
different event, so this design lists every event that writes one of the facts and gives
the rule for each.

## 1. What is open on main today

Each hole was checked by its reviewers' probes, rerun on main at 0374dd4. Two new probes
were added for what was left of them.

| Hole | Today |
|---|---|
| `unit.created` gives a project-less intent its project (the first unit's repository), so any agent picks it, and through it the project of the intent's workspace | **open**: core and the service |
| `manager.registered` naming an intent makes the caller that intent's manager when the intent's workspace has no manager | **closed** by the change that made the service record a manager's workspace as the runtime gives it: core refuses an intent that runs in a workspace other than the caller's. **Left**: `manager_of` still falls back to the per-intent manager. An agent registered "for" an intent in its own workspace stays its manager after the user starts it again in a manager-less workspace. New probe, core |
| A manager relabels its own workspace record onto the label a remote workspace is linked to, and so takes over that workspace's intents | **closed for `manager.registered`**: the same change. **Open through an agent's start**: `intent.state_changed` records the workspace as the body names it, so a start with the caller's own id and another label relabels the record the same way. New probe, through the service |
| A state change other than a start (done, back to the roadmap, dropped) that names a workspace sets where the intent ran, and so another workspace's project | **open**: core and the service |
| An agent in another runtime workspace that has the same label is matched to the first workspace's record (by label: no rename, no link). Once that record's manager is not running, its `manager here` makes it the manager of every intent that runs or ran there | **open**: through the service (the security review's probe) |
| Done, back to the roadmap, dropped, and `manager.registered` naming only an intent, write the record's id and label back to the ones the intent was started with. After a rename, closing an old intent relabels the record to the old label, and a link to that label moves to it, with the manager of the linked workspace's intents | **open**: core (the security review's probe) |

Rounds 1 and 2 of the probes still pass: 13 in core and 4 through the service.

## 2. The rules

### 2.1 An intent's project

- **For the rules, an intent's project is the one recorded on it**: the `project` of its
  `intent.created`, as the service completed it (a registered project's folder, or the
  repository's top folder). The registered project that holds that folder is the one
  meant. `State::intent_project(i)` returns it; empty means not known.
- **Its units never make it.** The first unit's repository (`project_folder`'s fallback)
  stays for display only: the page, a notice's "of project «…»", the start warning. The
  start check, `may_edit` and `workspace_project` read `intent_project` only.
- An intent with no recorded project is not known to the rules, whatever its units are.
  The user starts it (refused for agents, as today), and a manager adds nothing to its
  note while it is planned (as today, when it has no units).
- Who sets it: whoever adds the intent (the user; an agent's `backlog add` only into the
  roadmap). Nobody changes it afterwards: there is no event for that, and this design
  adds none (section 6).

### 2.2 Where an intent runs

- An intent's workspace (`Intent.workspace`), and with it a workspace's current intent and
  the intents that "ran in it", is set only by:
  - **a start** (`intent.state_changed` from planned to active), by the user in any
    workspace, or by an agent that passes the start check (section 2.5);
  - **the user's** `intent.created` with `runtime_ref` or `intent.linked` (as today).
- **A state change names a workspace only when it starts an intent.** Done, back to the
  roadmap and dropped carry none, from anyone. Nothing sends one today: the CLI's `done`
  and `drop` and the page send none. Refused: "a state change names a workspace only when
  it starts an intent: leave the workspace out (an intent stays recorded where it last
  ran)".

### 2.3 A workspace's record and its name

- **What an agent's event names as a workspace is the runtime's.** For every event an
  agent sends that names a workspace, the service takes the group the runtime gives the
  caller. The body must name that group's id (and machine). The event is recorded with the
  runtime's id and label, never the body's. Today this holds for `manager.registered`
  (`may_manage`). It is extended to an agent's start. One helper serves both: "the
  caller's workspace", which refuses a caller in no workspace and a body naming another
  id. Refused (403): "an agent starts intents in the workspace it runs in: run `workriver
  intent start <i>` from your own terminal". No other agent event names a workspace:
  `intent.created` with one and `intent.linked`, `workspace.project_set` and
  `workspace.linked`/`unlinked` are refused for agents, as today.
- The user's events name a workspace as the user's client gives it. The page's start takes
  the runtime's group by id (as today). The CLI's `workspace project` takes the recorded
  id and label. The user is not checked against the runtime, except that the page's
  start gets the same-label refusal below (a check of the group, not of who asks).
- How a record is found stays as it is (`workspace_at`): by id, else by label, and a
  linked remote group through the local label of its link. It runs inside `apply`, so
  changing it would change the State that old events replay to. What changes is that no
  label an agent's request carries reaches a record, and the two rules below.
- **A runtime group is matched by label only to a record whose own id the runtime no
  longer lists.** The label fallback is there for one case: the same workspace under a new
  id (the runtime restarted). A record whose id is still listed is another open workspace
  that has the same label, not this one under a new id. Core cannot tell the two apart (it
  has no runtime, and replay must not depend on one), so the service checks it wherever it
  takes a group from the runtime to record it: in "the caller's workspace" helper (an
  agent's `manager.registered` and its start) and in the page's start. The check: the
  record the group resolves to (`workspace_of`) is on the same machine, has another id, and
  that id is in `runtime.groups()` (every workspace of this machine, with or without
  agents, and those of followed machines). Then the request is refused (409): "workspace
  {label} is already recorded for another open workspace ({id}); two open workspaces have
  one name, and workriver tells workspaces apart by name: ask the user to rename one of
  them". A runtime that cannot be read refuses the request (503), as `may_manage` does
  today. For a registration naming only an intent, the record checked is the one it
  writes: the record of the intent's workspace. A group of another machine matched through
  a link is not checked: the link is the user's, made by label.
  - This is the security review's tried fix, kept as the rule, with one addition: the
    page's start gets the same refusal. Without it, the user starting an intent from the
    page in the second of two open workspaces labelled alike renames the first one's record
    to the second's id. The first one's manager then manages the new intent, and the second
    one's agent cannot register (prototyped: the start is accepted, the first workspace's
    manager replaces the new intent's note, the second's registration gets "has a manager
    running"). That is the accident the review names, done by the user's hand. A
    stronger rule, never matching by label while two records could share one, needs a key
    of workriver's own for a workspace (section 6).
  - What it leaves: once a workspace is closed in the runtime, the next group with its
    label is its record, on purpose (the restart case; prototyped: a manager registered
    again under the new id is accepted and keeps its intent). An agent can close a
    workspace, or rename its own, through the runtime's socket and then match a closed
    workspace's record. That is the limit below.
- **Only an event that names a workspace writes a record's id and label.** That is a start,
  a `manager.registered` naming a workspace, and the user's `intent.created` with
  `runtime_ref`, `intent.linked` and `workspace.project_set` (`workspace_mut`, which makes
  the record follow the group it is given, as today). Done, back to the roadmap and
  dropped, and `manager.registered` naming only an intent, find the record of the intent's
  workspace (`workspace_at`) and change only its current intent or its manager. They make
  the record when there is none, as today, but never set an existing one's id and label to
  the ones the intent was started with. This changes `apply` for those two (section 4).
- **Where this stops (a limit, written into `caller-identity.md`):** the runtime's socket
  belongs to the account. An agent can rename any workspace through it (Herdr's
  `workspace rename`), as it can type into another terminal or close it. It can give its
  own workspace the label a link names. When the label is next recorded, from the runtime,
  workriver records it, and a link then resolves to the first local record that has that
  label. It can also close a workspace and give its own the closed one's label: a group
  that has no record of its own yet is then matched to the closed one's record. Any local
  record is open to this, not only one a link names. This is the same kind of limit as the
  ones already listed for the runtime's socket. workriver does not see it happen, and no
  request to workriver does it. Two open workspaces that share a label without any of this
  are refused above.

### 2.4 A workspace's project

`workspace_project` keeps its order and reads only facts set as above. First what the user
set (`workspace.project_set`, the user's only). Then the recorded project of its current
intent. Then the one recorded project of the intents that ran in it (none, or several: not
known). Every input is now the user's, or comes from a checked start. A checked start by an
agent changes nothing here: it starts only an intent whose project is already the
workspace's.

### 2.5 The start check (unchanged, on the facts above)

An agent starts a planned intent only:

- in the workspace the runtime gives it, never matched to another open workspace's record
  (2.3),
- when it is that workspace's recorded manager,
- when the intent's recorded project (2.1) is known and equal to the workspace's project
  (2.4).

Its refusals stay word for word ("only the manager of workspace … starts intents there",
"… has no project: ask the user to start it", "… belongs to project Q; your workspace works
on P …", "… your workspace's project is not known yet …"). The user starts any intent
anywhere. The page asks the user to confirm another project's.

### 2.6 An intent's manager

- **A workspace's manager** is set only by `manager.registered` from an agent running in
  that workspace (by the runtime), and only to the record of that workspace: never to the
  record of another open workspace that has the same label (2.3). It takes over only from
  a manager that is no longer running (as today).
- **An intent's manager** (`manager_of`) is the manager of the workspace record it runs in,
  or last ran in. **The manager registered for the intent itself (`Intent.manager`, before
  0.2) counts only for an intent that has no workspace at all**, which only records from
  before 0.2 have. An intent with a workspace whose record has no manager, or no record,
  has no manager. The user acts for it, or a manager registers there.
- `manager.registered` with `intent` stays accepted when that intent runs in the caller's
  workspace (as today). It still sets `Intent.manager`, and since an intent with a
  workspace never reads that field, the field no longer decides anything. It sets the
  manager of the record of the intent's workspace without renaming that record (2.3).
  The hidden `--intent` keeps working for old prompts.

### 2.7 Units

- **`unit.created` is the user's or the intent's manager's** (`manager_of`), as
  `unit.relocated` already is. A unit is part of the intent's plan, and the manager plans.
  This is not what keeps the project safe (2.1 does that). It means no other agent adds
  work to an intent it does not manage. Refused: "only the user or the manager of intent
  {i} ({name}) adds its units; tell the manager (`workriver manager tell <your task> --text
  "…"`)". An intent with no manager (planned and never run, or in a workspace without one)
  is refused with: "{i} has no manager: units are added by the manager of the workspace it
  runs in, once it is started, or by the user".
- `may_place` (another machine) is unchanged.

## 3. Every event, at a glance

| Event | Sets | Who may send it | Check |
|---|---|---|---|
| `intent.created` | project; with `runtime_ref`, where it runs | the user; an agent planned, no `runtime_ref` | as today |
| `intent.state_changed` → active | where it runs, current intent; the record's id and label | the user; an agent passing 2.5 | new: workspace = the caller's, by the runtime (service); a group matched to another open workspace's record is refused, for an agent's start and the page's (service) |
| `intent.state_changed` → done / planned / dropped | state, and the current intent of the record it ran in (found, never renamed) | the user or `manager_of`; planned → dropped the user | new: a workspace is refused (core, everybody); the record is no longer renamed to the intent's workspace (apply) |
| `intent.linked` | where it runs | the user | as today |
| `unit.created` | a unit (never the project, for the rules) | the user or `manager_of` | new (core) |
| `unit.relocated` | the unit's place | the user or `manager_of` | as today |
| `manager.registered` | a workspace's manager; naming a workspace, the record's id and label | the agent itself, in that workspace | as today; new: a group matched to another open workspace's record is refused (service); naming only an intent, the record is found, never renamed (apply); `Intent.manager` read only without a workspace (`manager_of`) |
| `workspace.project_set`, `workspace.linked`, `workspace.unlinked`, `project.added`, `project.renamed` | the workspace's project, links, projects | the user | as today (`Body::user_only`) |
| `intent.merged` | nothing of the three (same project required) | as today | as today |

## 4. Old events

`apply` changes for two cases only (2.3): a state change to done, planned or dropped, and
`manager.registered` naming only an intent, find the record of the intent's workspace
(`workspace_at`) and no longer write its id and label. Each still makes the record when
there is none, with the intent's workspace, as before. Every other kind applies as before.
Replayed, old events give the same State except in this:

- **A workspace record's id and label** (`Workspace.group`), and only where one of those
  two events came after the record had taken an id or label other than the intent's
  workspace's: a later registration, start or user event that named the workspace under a
  new id or label (a rename, or a runtime restart). Before, the record went back to the
  intent's id and label; now it keeps the latest. Where the record already had the
  intent's id and label, or was found through a link (a linked record was never renamed),
  nothing differs.
- **What is found through that record from then on.** Records are found by id, else by
  label, so a later event naming the old label no longer finds this record (and may find
  another, or none, and make one), and one naming the latest label still does. Which record
  holds a later registration's manager, a start's current intent or the user's project,
  and with it `manager_of` and `workspace_project` of the intents that ran there, can then
  differ from that event on. The security review's core probe is this case: after the
  change, the link stays on the other workspace's record and its manager stays the manager
  of the linked workspace's intent.

Whether the guards accept it:

- The synthetic replay (`records/tests/replay.rs`, and its stored half in workriver-data)
  has no such sequence. Prototyped (outside the tree), both pass with `expected-state.json`
  unchanged: no re-bless. If the implementation's fixture ever changes there, a re-bless
  is reviewed for this kind of difference only.
- The copy of the user's own database (`replays_a_copied_database`, `guards.md`) is run
  before merging, with the user's say-so, since this changes replay. Its result must equal
  its baseline, or differ only in `workspaces[].group` and in what follows from it as
  above. The implementer lists each record that differs, with the event that renamed it
  before. Any other difference is a defect.

What also differs is read from that State, with `apply` as before:

- new events that the rules now refuse (2.2, 2.3, 2.7);
- `manager_of` for an intent that ran in a workspace after a per-intent registration and now
  runs where nobody is registered: no manager, where it used to be the old registrant;
- for the rules only, the project of an intent recorded with none but with units: not
  known, where it used to be its first unit's repository. Its workspace's project follows
  (`workspace_project`). A manager whose workspace's project was known only that way is
  refused the next start and asked to have the user set it. Nothing the page shows
  changes.

## 5. What agents are told

- `CHANGES.md`, one entry (managers): units of an intent are added by its manager or the
  user; `intent done/drop` never name a workspace (nothing changes for the CLI); a project
  is the one recorded on the intent, never its units' (if your start or `--add-note` is now
  refused with "project not known", ask the user to set your workspace's project); `intent
  start` runs from your own terminal only; `manager here` and `intent start` are refused
  in a workspace that has the label of another open workspace (ask the user to rename
  one).
- `start-intent`: "by the records" now means the intent's recorded project. The refusals
  list gains the start-from-your-own-terminal refusal and the same-label refusal (2.3).
  The recipe that has `manager here` lists the same-label refusal too.
- `units`: who adds a unit, and the two refusals of 2.7. `deferred-batches` already has
  the manager adding the unit.
- `docs/security.md` (and `.zh-CN.md`): the paragraph "Since these rules go by …" names the
  three facts and their writers as in section 3. `caller-identity.md`, section 6: the
  limit of 2.3 as written there (a rename onto a link's label, and a closed workspace's
  record matched by a group given its label: any local record, not only a link), and that
  two open workspaces sharing a label are refused, not matched.
- The page shows the 409 of its start as it shows the other refusals of the start; its
  words come from the service, so no new UI text.

## 6. Not done

- No event for the user to set or change an intent's project after it is added. A
  project-less intent is started by the user. Its workspace's project is the user's to set.
- Records stay matched by label, and links stay resolved by label (2.3). A key of
  workriver's own for a workspace record would end the rename limit and make the same-label
  refusal unneeded, but it changes replay and the link format.
- The user's `workspace project` and `workspace link` from the CLI name a record as
  recorded (its id and label; a link, its local label), not a runtime group, so the
  same-label check does not apply to them.
- The per-intent manager field and the `--intent` flag are not removed.

## 7. Tests

The rounds' probes become tests: the unit, close-naming and back-to-roadmap probes in core,
and their service forms. Add the two new probes (the stale per-intent manager, and a start
with a forged label onto a linked workspace through the service) and one test per refusal
above. The security review's two probes (`probes5.rs`) become tests too: in the service,
an agent in a second open workspace with the same label is refused `manager here` and
cannot touch the first one's intent; in core, closing an old intent after a rename leaves
the link and the linked intent's manager where they were. Add with them: the restart case
(a workspace whose id is no longer listed is matched by label, and its manager, registered
again under the new id, keeps its intent); the page's start in the second of two open
workspaces with the same label, refused; a `manager.registered` naming only an intent after
a rename, which leaves the record's label as it was; and a runtime that cannot list its
groups, which refuses the registration (503). The core probe of a forged label in
`manager.registered` stays failing by design:
core takes a workspace as given, and the service makes it the runtime's. Its service form
is the test. Rounds 1 and 2 must stay green, along with the guards
(`docs/design/guards.md`).
