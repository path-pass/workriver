# An intent's project, workspace and manager: who decides

Status: design, not confirmed. Its rules go into `docs/security.md` (both languages) and
`docs/design/caller-identity.md` before the branch is merged; this file is then removed.

Every rule about who may change an intent reads three facts: the **project** the intent
belongs to, the **workspace** it runs in, and its **manager**. The rule this design holds:
**each of the three is set only by the user, or by a start the rules checked. No other
event, and no name in a request, sets them.** The reviews of intent editing found four
holes in that rule. Each was in a different event, so this design lists every event that
writes one of the facts and gives the rule for each.

## 1. What is open on main today

Each hole was checked by its reviewers' probes, rerun on main at 0374dd4. Two new probes
were added for what was left of them.

| Hole | Today |
|---|---|
| `unit.created` gives a project-less intent its project (the first unit's repository), so any agent picks it, and through it the project of the intent's workspace | **open**: core and the service |
| `manager.registered` naming an intent makes the caller that intent's manager when the intent's workspace has no manager | **closed** by the change that made the service record a manager's workspace as the runtime gives it: core refuses an intent that runs in a workspace other than the caller's. **Left**: `manager_of` still falls back to the per-intent manager. An agent registered "for" an intent in its own workspace stays its manager after the user starts it again in a manager-less workspace. New probe, core |
| A manager relabels its own workspace record onto the label a remote workspace is linked to, and so takes over that workspace's intents | **closed for `manager.registered`**: the same change. **Open through an agent's start**: `intent.state_changed` records the workspace as the body names it, so a start with the caller's own id and another label relabels the record the same way. New probe, through the service |
| A state change other than a start (done, back to the roadmap, dropped) that names a workspace sets where the intent ran, and so another workspace's project | **open**: core and the service |

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
  id and label. The user is not checked against the runtime.
- How a record is found stays as it is (`workspace_at`): by id, else by label, and a
  linked remote group through the local label of its link. It runs inside `apply`, so
  changing it would change the State that old events replay to. What changes is that no
  label an agent's request carries reaches a record.
- **Where this stops (a limit, written into `caller-identity.md`):** the runtime's socket
  belongs to the account. An agent can rename any workspace through it (Herdr's
  `workspace rename`), as it can type into another terminal or close it. It can give its
  own workspace the label a link names. When the label is next recorded, from the runtime,
  workriver records it, and a link then resolves to the first local record that has that
  label. This is the same kind of limit as the ones already listed for the runtime's
  socket. workriver does not see it happen, and no request to workriver does it.

### 2.4 A workspace's project

`workspace_project` keeps its order and reads only facts set as above. First what the user
set (`workspace.project_set`, the user's only). Then the recorded project of its current
intent. Then the one recorded project of the intents that ran in it (none, or several: not
known). Every input is now the user's, or comes from a checked start. A checked start by an
agent changes nothing here: it starts only an intent whose project is already the
workspace's.

### 2.5 The start check (unchanged, on the facts above)

An agent starts a planned intent only:

- in the workspace the runtime gives it (2.3),
- when it is that workspace's recorded manager,
- when the intent's recorded project (2.1) is known and equal to the workspace's project
  (2.4).

Its refusals stay word for word ("only the manager of workspace … starts intents there",
"… has no project: ask the user to start it", "… belongs to project Q; your workspace works
on P …", "… your workspace's project is not known yet …"). The user starts any intent
anywhere. The page asks the user to confirm another project's.

### 2.6 An intent's manager

- **A workspace's manager** is set only by `manager.registered` from an agent running in
  that workspace (by the runtime). It takes over only from a manager that is no longer
  running (as today).
- **An intent's manager** (`manager_of`) is the manager of the workspace record it runs in,
  or last ran in. **The manager registered for the intent itself (`Intent.manager`, before
  0.2) counts only for an intent that has no workspace at all**, which only records from
  before 0.2 have. An intent with a workspace whose record has no manager, or no record,
  has no manager. The user acts for it, or a manager registers there.
- `manager.registered` with `intent` stays accepted when that intent runs in the caller's
  workspace (as today). It still sets `Intent.manager` (apply is unchanged), and since an
  intent with a workspace never reads that field, the field no longer decides anything.
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
| `intent.state_changed` → active | where it runs, current intent | the user; an agent passing 2.5 | new: workspace = the caller's, by the runtime (service) |
| `intent.state_changed` → done / planned / dropped | state only | the user or `manager_of`; planned → dropped the user | new: a workspace is refused (core, everybody) |
| `intent.linked` | where it runs | the user | as today |
| `unit.created` | a unit (never the project, for the rules) | the user or `manager_of` | new (core) |
| `unit.relocated` | the unit's place | the user or `manager_of` | as today |
| `manager.registered` | a workspace's manager | the agent itself, in that workspace | as today; `Intent.manager` read only without a workspace (new, `manager_of`) |
| `workspace.project_set`, `workspace.linked`, `workspace.unlinked`, `project.added`, `project.renamed` | the workspace's project, links, projects | the user | as today (`Body::user_only`) |
| `intent.merged` | nothing of the three (same project required) | as today | as today |

## 4. Old events

`apply` is unchanged for every kind, so every recorded event replays to the same State. The
guards' replay fixtures stay unchanged. What differs is read from that State:

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
  start` runs from your own terminal only.
- `start-intent`: "by the records" now means the intent's recorded project. The refusals
  list gains the start-from-your-own-terminal refusal.
- `units`: who adds a unit, and the two refusals of 2.7. `deferred-batches` already has
  the manager adding the unit.
- `docs/security.md` (and `.zh-CN.md`): the paragraph "Since these rules go by …" names the
  three facts and their writers as in section 3. `caller-identity.md`: the rename limit of
  2.3.

## 6. Not done

- No event for the user to set or change an intent's project after it is added. A
  project-less intent is started by the user. Its workspace's project is the user's to set.
- Records stay matched by label, and links stay resolved by label (2.3). A key of
  workriver's own for a workspace record would end the rename limit, but it changes replay
  and the link format.
- The per-intent manager field and the `--intent` flag are not removed.

## 7. Tests

The rounds' probes become tests: the unit, close-naming and back-to-roadmap probes in core,
and their service forms. Add the two new probes (the stale per-intent manager, and a start
with a forged label onto a linked workspace through the service) and one test per refusal
above. The core probe of a forged label in `manager.registered` stays failing by design:
core takes a workspace as given, and the service makes it the runtime's. Its service form
is the test. Rounds 1 and 2 must stay green, along with the guards
(`docs/design/guards.md`).
