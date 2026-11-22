# Migrating a store to virtual workspaces (once)

This version of workriver keeps workspaces of its own (`ws1`: an id, a name that is only
shown, a project), made by you, and a manager you seat. The store you have was written
in the earlier form, where a workspace was the agent runtime's group and a manager
registered itself. This version does not read that form: `workriver serve` refuses such a
store, changes nothing in it, and names this folder.

This folder converts a copy of your store once. It is not part of workriver: two Cargo
packages of their own, never built or shipped with it, removed from the repository once
your store is migrated.

- `old/` reads the copy with the code that wrote it (workriver as of the last commit before
  the change) and writes the new events: `old plan`, then `old apply`.
- `new/` stores them with this version's code and compares the result with the old state:
  `new write`, then `new check`.

Nothing here writes to your store. The copy is opened read-only, every output is a new
file or folder, and any path inside your data home (`$WORKRIVER_HOME`, else
`~/workriver-data`) is refused.

## Before

- Answer the choices still open and let pending messages be delivered: after the migration
  an open choice can no longer be answered and a message not yet delivered stays so (the
  manager it waited for is no longer seated). `old plan` lists them.
- Commands below run from the repository's root, with the Rust that `mise.toml` names.

## 1. The old code and a copy

Stop `workriver serve` (Ctrl-C), then:

```sh
git worktree add --detach .local/migrate-old 1a6a6a4   # the last commit before the change
mkdir -p .local/migrate
cp ~/workriver-data/workriver.db .local/migrate/copy.db
```

That commit is the main this change was last brought up to: the code your service runs
until you switch, so it reads every kind of event your store has (an earlier one does not
know `agent.keys`, which answering a stuck agent from the page records, and stops there).
If your service runs a later main, take that commit instead. Copy only while the service is stopped. If `workriver.db-wal` is there and not empty, copy
it too, as `copy.db-wal`. Keep the original as it is: it is how you go back. The service
may run again meanwhile; what it records after the copy is not in the migration, so copy
again (and plan again) right before you switch.

## 2. Plan

```sh
alias old='cargo run -q --manifest-path scripts/migrate-virtual-workspaces/old/Cargo.toml --'
alias new='cargo run -q --manifest-path scripts/migrate-virtual-workspaces/new/Cargo.toml --'
old plan .local/migrate/copy.db .local/migrate/mapping
```

`plan` prints and writes the mapping: what each old workspace record becomes, and a line
with `?` for each point it cannot tell. Fill in every `?`; change a name, or `open` /
`closed`, where you want it otherwise. Lines starting with `#` say where each point comes
from: every text from the store in them (labels, names, folders) is a JSON string, so none
makes a line of its own. Give each key once: a folder, a record's or label's workspace (by
the project's id or its folder), a message or an intent given on two lines is refused,
naming both.

| Line | What you write |
| --- | --- |
| `folder "<path>" = ?` | A folder an intent (or `workspace project`) named that no registered project holds. `register`: you add it as a project (recorded as yours, before the first event naming it); `into <project>`: its intents are that project's; `none`: they are of no project (planned ones go to the roadmap of "No project"). The comment says who named it: the script never registers a folder by itself, since an agent may have written it. |
| `record <n> <project> = name "...", open` | One workspace per old record and project. `?` in the project's place: nothing tells the record's project (its intents have none); name the one it belongs to. A record whose intents are of several projects is split, one line each. |
| `label "<label>" <project> = name "...", closed` | Messages to a name no record carried: a closed workspace for them, in the project you name. |
| `message <id> = ?`, `intent <id> = ?` | A message or a copy of one (or an intent of no project) in a split record: the project of the workspace it goes to. |

A project is named by its id as the comment lists it (`p2`) or by its folder.

## 3. Apply, write, check

```sh
old apply .local/migrate/copy.db .local/migrate/mapping .local/migrate/out
new write .local/migrate/out .local/migrate/new.db
new check .local/migrate/out .local/migrate/new.db
```

- `apply` refuses while a `?` is left, or when the mapping does not settle something (it
  says which line to add), and writes nothing then. It refuses, saying what to do, two
  histories the store it was written for does not have: an intent started before its
  project was registered, and read marks one workspace's mark cannot keep (a conversation
  the runtime renamed, read under one label and not the other: read it to its end in the
  old version first). It prints the workspaces it made and
  what it dropped. In `out/`: `events.jsonl` (the new events in order), `seats.jsonl` (the
  old registrations, used by the check only) and `old-state.json` (the old state, with
  where the mapping puts each record and message).
- `write` makes `new.db` with this version's store (so it is in the new form), and stores
  each event with its own time and author. Each event the script wrote or changed is first
  put through this version's rules on the state before it, and must come out as written;
  events kept as they were are not (some were accepted under rules that later tightened).
  A refusal stops it, lists every such event with the rule's words, and writes nothing.
- `check` replays a copy of `new.db` with this version and compares it with the old state:
  everything that is not a workspace is equal, every intent and message is where the
  mapping says, unread counts are the same, no choice is open, no seat is set. A
  difference the mapping does not explain fails it, listed.

## 4. Look, then switch

Look at it in the page before you switch, on a home of its own and another port:

```sh
mkdir -p .local/migrate/home && cp .local/migrate/new.db .local/migrate/home/workriver.db
WORKRIVER_HOME=$PWD/.local/migrate/home workriver serve --port 47830
```

The rail (project, workspace, intent, unit), each conversation, Needs you. Then, with the
service stopped:

```sh
mv ~/workriver-data/workriver.db ~/workriver-data/workriver.db.before-virtual-workspaces
cp .local/migrate/new.db ~/workriver-data/workriver.db
```

(Move the old `-wal` and `-shm` files aside with it, if any.) Start the new version.

## After

- **Seat each manager.** The seats start empty: an agent's registration is not your act.
  Needs you shows each workspace with an active intent and no running manager; pick its
  manager there (or the workspace's line, Manager; or `workriver workspace manager <ws>
  <agent>` from your own terminal). The plan's comments list who was registered last.
- Links are gone: an agent on another machine works in the same workspace through the
  tasks it is given.
- **Going back**: the old store with the old binary.
- Then this folder and `.local/migrate-old` go (`git worktree remove .local/migrate-old`).

## What it does, row by row

| Old | New |
| --- | --- |
| A workspace record | `workspace.created` (yours), just before the first event that needs it, once its project is registered. Name: its last label. Closed at the end when no intent is active in it. |
| A record whose intents are of several projects | One workspace per project. |
| Records merged by label (a new id after a restart) | One workspace. |
| Same label, another machine, not linked | Each its own workspace. |
| A link (`workspace.linked` / `unlinked`) | Dropped. While it held, what the linked group recorded went to the workspace it was linked into; before and after, to its own record's workspace. |
| `manager.registered` | Dropped; the seats start empty. |
| `workspace.project_set` | Folded into the workspace's project (used when its intents have none); dropped. |
| A start, an intent made in a group, `intent.linked` | The start with the workspace's id; `intent.workspace_set` (yours). |
| `intent.created`'s runtime group | Dropped from the event; the rest as stored. |
| `message.sent`, `message.said`, each copy to every manager | The id of the workspace the message went to when it was recorded. |
| `messages.read` by label | One per workspace that carried the label then. |
| A folder the mapping registers | `project.added` (yours) before the first event naming it; a project added later moves up one id, and its renames with it. |
| A registered project with no workspace | `workspace.created`, named as the project, at the end. |
| Every other event | As stored, byte for byte. |

## Tests

```sh
cargo test --manifest-path scripts/migrate-virtual-workspaces/old/Cargo.toml
cargo test --manifest-path scripts/migrate-virtual-workspaces/new/Cargo.toml
```

`old`'s tests run the rows above on fixtures of old events and keep what `apply` writes in
`new/tests/__fixtures__/` (`WORKRIVER_BLESS=1` writes it again); `new`'s tests write and
check those, and show that a written event the rules refuse, or one that comes out
different, fails the run, that an old event kept as it was is not put through today's
rules, and that an unexplained difference fails the check.
