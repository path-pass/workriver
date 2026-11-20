# Editing an intent, with its history

The design of this branch. Before it is merged its rules go into the living spec
(`docs/design/ui/navigation.md`, "An intent's page"; `docs/security.md`; the recipes) and
this file is removed.

## Today (read in the code)

- `intent.updated` (`workriver intent set`) sets `title`, `goal`, `findings`, or appends a
  paragraph (`add_note`). Core's rule: the user, or `manager_of(intent)`; `findings` the
  user only.
- A planned intent is the user's only by accident: `manager_of` finds no workspace for one
  never started. One that went back to the roadmap (`intent.state_changed` active →
  planned) keeps its workspace, so that workspace's manager can still change it, and drop
  it (`(Planned, Dropped)` asks `manager_of` too).
- A manager cannot write down what the user said about a planned intent (an example the
  user gave): `--add-note` on it is refused, since it has no manager.
- The page edits nothing of it (only the findings select). The note only grows.
- Every change is an event; nothing reads them back as versions.
  `WorkStore::history` exists but may not be answered to a reader (no blind cut for events).

## 1. What is edited, and where

- **Title, goal, note** (the note as a whole, Markdown). Not the external link: it is the
  tracker's, and the tracker adapter (DESIGN.md, Not done) will keep it in step; a hand
  edit would be overwritten or disagree.
- **Page, intent page header**: a quiet pencil button after "copy link", on every state.
  It opens a dialog (as Start and Merge do, so a refusal has a place to show):
  - Title (one line, required), Goal (two rows), Note (a textarea, Markdown, with a
    Write / Preview switch; Preview renders with `Prose`, as the page renders the note).
  - Save sends only the fields that changed, with `base` (below); disabled when nothing
    changed or the title is blank. Cancel and Escape close it; focus goes back to the pencil.
  - Not inline editing: three fields, a conflict and a preview need one place; the dialog
    already exists (`ui/primitives/dialog.tsx`).
  - The page is the user's, so every field is editable there (section 2).
- **CLI**: `workriver intent set i3 [--title …] [--goal …] [--note TEXT | --add-note TEXT]
  [--base N]`.
  - **Every whole-field replace needs `--base`**: `--title`, `--goal`, `--note`. Without it
    the command is refused before anything is sent ("give --base N: the version you read,
    `workriver intent history i3`"). `--add-note` and `--findings` need none: they
    overwrite nothing.
  - `--note` replaces the whole note (`--note ""` empties it; a long one as
    `--note "$(cat note.md)"`). `--note` with `--add-note` is refused.
  - `workriver intent history i3` lists the versions; `--version N` prints one whole;
    `--diff N` prints it against N−1. It reads the history route (section 3), not
    `/api/state`. `workriver board i3` shows the version next to the goal.

## 2. Who edits what

The user's rule.

| | planned | active | done, dropped |
|---|---|---|---|
| **title, goal**: the user | yes | yes | yes |
| **title, goal**: any agent | no | no | no |
| **note**: the user | replace, add | replace, add | replace, add |
| **note**: the intent's manager | (has none) | replace, add | no |
| **note**: a manager of a workspace of the intent's project | add only | no (unless it is the intent's manager) | no |
| **note**: any other agent | no | no | no |
| **drop**: the user | yes | yes | – |
| **drop**: the intent's manager | (has none) | yes (as now) | – |

- **The rule is by state in core** (`prepare_intents`), no longer by whether `manager_of`
  finds someone. That closes the returned-to-roadmap gap above for edits and for dropping:
  a planned intent is dropped by the user only (merging agent-added planned intents stays
  as it is: tidying, its own event and rule).
- **Title and goal are the user's.** They say what the user asked for; a manager that thinks
  they should read otherwise says so to the user. Today a manager may change them: that
  ends (CHANGES.md, section 4).
- **The note is where agents write.** While the intent is active its manager adds to it
  and corrects it; it answers for the intent.
- **A planned intent's note takes additions from a manager of its project.** That is what
  was missing: a manager told something by the user about an intent still in the roadmap
  writes it down where whoever starts it will read it. Add only: replacing would rewrite
  what the user or another manager put there, on an intent nobody answers for yet.
  - Core checks that the caller manages a workspace (a `manager.registered` for it that
    still holds) and sends `add_note` only. Same project is the service's check, in the
    append handler before recording, with the same reckoning as the start dialog
    (`group_project`: what the user set, the folder its manager runs in, its current
    intent) against the intent's project (`project_folder`). It needs the runtime and the
    file tree, which core does not see. A project unknown on either side, or a runtime
    that cannot be read, is a refusal ("i3 belongs to project workriver; your workspace's
    project is not known: ask the user"), never a pass.
- **Done or dropped: the user only.** The work on it is over; the user may still correct a
  record (a title, a link to the outcome).
- **Other agents: no.** A worker tells its manager (`workriver manager tell`).
- `findings`: unchanged, the user only.
- **Every edit, by anyone, is a version** and can be restored (section 3).

## 3. Versions

- **No new event kind.** `intent.updated` gains `note` (the whole note) and `base` (the
  version the edit started from). Old records replay unchanged.
- **What is a version** (one rule, in core): v1 is `intent.created`; a new version for each
  `intent.updated` that sets title, goal, note or add_note, and for each `intent.merged`
  into it. A findings-only update or a state change is not a version. Existing records
  become history at once.
- **What `State` keeps**, per intent, and only this: `version` (its last n), `edited` (at,
  by: the event's actor, never the body) and `changed` (the version that last changed each
  of title, goal and note). That is all the base check and the board need; `/api/state` and
  `/api/workspace` carry these few fields and no history.
- **The history is built on read.** A named read of the port, `WorkStore::intent_events(id)`
  (the events that make its versions, oldest first), and one core function over them
  (`intent_versions`) that applies the rule above. Each version holds what its event set:
  the `add_note` paragraph, not the whole note, so the list grows with the events, not with
  the square of the paragraphs. The whole text of version N is the same function replaying
  up to N.
- **The route**: `GET /api/intent/{id}/history` lists the versions (n, at, by, the fields
  changed, what the event set, the intents merged in); `?version=N` answers version N whole
  and N−1 whole, for the diff. The page and `workriver intent history` both read it.
- **Concurrent edits**: a replace is refused when `changed` of a field it sets is after its
  `base` (`add_note` counts as changing the note). An edit of other fields goes through. A
  `base` above `version` is refused. The refusal names the version, who and when: "i3's note
  changed since v4 (v5 by claude 1, 2 min ago); read it (`workriver intent history i3
  --version 5`) and run again with `--base 5`". The page always sends `base`; the CLI must
  (section 1).
- **On the page after a refusal**: the dialog stays open with the user's text, shows the
  newer text of the field that conflicts as a diff against it, and "Save anyway" (sends
  again with the newest base). Overwriting loses nothing: the overwritten version stays in
  the history. No three-way merge.
- **History on the page**: under the goal, only when there is more than one version, a quiet
  line "v5 · edited 2d ago by claude 1 · History" (the date on hover; an agent with its
  robot mark, as the manager's decisions are shown). "History" opens a dialog:
  - the versions newest first: `vN`, who, when, what changed in words ("title, note",
    "added to the note", "created", "i7 merged into it");
  - the one picked (the newest first): title and goal as old → new, the note as a word diff
    (`diffWordsWithSpace` of `diff`, already a dependency for the code layer; ins/del in the
    diff tokens of `presentation.md`); a switch "Whole text" renders that version's note;
  - "Restore this version" (not on the newest) opens the edit dialog filled with that
    version's title, goal and note, `base` the newest. Saving is a new version; nothing is
    removed or rewritten.
- **Why not in `/api/workspace` or `/api/state`**: both are read again and again (the page on
  every change; the CLI on every `board`, from every agent on every machine), and the
  history is looked at rarely; today it is about 41 KB (the bodies of the 99 intent events)
  and only grows.

## 4. What agents are told

- **The manager is told when the user edits its active intent**: `[workriver] The user
  changed i3 (now v5): title «…», goal «…»; its note changed (workriver intent history i3
  --diff 5).` Title and goal quoted through `quote.rs` with `users` (only the user changes
  them, and only the user's edits are told); the note not quoted (long), only pointed at.
  Its own edits: nothing. A manager's addition to a planned intent's note: nobody is told
  (the intent has no manager; the user sees it in the history). Done, dropped: nothing. It
  goes through the delivery path messages already use, so it waits while the manager is on
  a blind task.
- **CHANGES.md** (managers): title and goal are the user's now (a manager's `--title` or
  `--goal` is refused: ask the user); `--note` replaces the whole note; `--title`, `--goal`,
  `--note` need `--base` from `workriver intent history`, and the refusal on a changed base;
  `--add-note` on a planned intent of your workspace's project now works, add only;
  dropping a planned intent is the user's; the message above.
- **Recipes**: `start-intent` (the note paragraph: replace as well as add, the base, the
  message when the user edits; title and goal are the user's; writing down what the user
  said about a planned intent with `--add-note`; the returned-to-roadmap intent is no longer
  the manager's to drop). `units`, `design-gate`, `spike` keep `--add-note` on the active
  intent and need no change. The manager skill's hard rules do not change.

## 5. Security

- **Writes: no new route.** The page records `intent.updated` through `/api/append` as it
  does for the findings select; `App::who` makes the page's session the user and an agent's
  CLI that agent. Who may edit what is core's rule on the `Principal` and the intent's state
  (section 2), the last line for page and CLI alike. Not `Body::user_only`: managers may
  record it too; the user-only parts (title, goal, `findings`, replacing a planned note,
  anything on a done or dropped intent, dropping a planned intent) are refused in
  `prepare_intents` for anyone else.
- **The project check** for a planned intent's addition is the service's, before the record:
  any read it needs that fails (store, runtime) fails the request; an unknown project is a
  refusal. Core still refuses anyone but a manager, and anything but `add_note`, whatever
  the service let through.
- **Reads**: `GET /api/intent/{id}/history` asks `App::who`; for an agent on an undelivered
  blind task (`blind_view` gives a cut) it is refused, as the note is hidden from it; else it
  reads `intent_events` through the port. A missing intent is refused in core's words
  (`no intent i9`, 422) as other reads by id are; a failed read fails the request (500). One
  line in `route-access.txt` per listener.
- **On `machine_router` too**: `workriver intent history` now reads it, and a manager may run
  on another machine (a linked workspace); it is a read under the same cut as `/api/task`,
  which is already there. Nothing written goes there that is not already (`/api/append`).
- **Messages**: section 4; what an editor wrote goes in through `quote.rs`. `by` is the
  event's actor, so "edited by" is never a caller's word.
- **Rendering**: the note in the dialog's preview and the history renders with `Prose`, as
  on the page now; a diff renders text, never Markdown.

## 6. Depends on

- **Who moves an active intent to done or back to the roadmap.** These rules rest on the
  intent's state, but `prepare_intents` checks no actor for `(Active, Done)` and
  `(Active, Planned)`: any agent can record either for any active intent, and so also take
  the note away from its manager. Those transitions should be the user's or the intent's
  manager's, as `(Active, Dropped)` is. Left to a unit of its own (pre-existing, deferred);
  until then this design is only as strong as that rule.

## 7. Not done

- Editing the external link (the tracker adapter's).
- Agents changing a planned intent's title or goal, or replacing its note, also one an
  agent added.
- A three-way merge of concurrent edits; showing live, while the dialog is open, that
  someone else changed the intent (the refusal on save covers it).
- History of findings policy, state, workspace or roadmap order: the history is the
  intent's text only.
- A `restore` command: the user restores on the page; an agent from `intent history
  --version N` with `--note` (title and goal are the user's).
- Removing or hiding a version: the record is append-only.
- Anything about edits in the rail.

## What this revision answers

- The review: versions out of `State`, history built on read through its own route, the
  CLI on that route → section 3 ("What `State` keeps", "The history is built on read",
  "The route", "Why not …"), section 1 (`intent history`), section 5 (Reads, `machine_router`).
- The review: `--base` on every whole-field replace → section 1 (CLI), section 3
  (Concurrent edits).
- The review: dropping a planned intent by state, the user only → section 2 (table, "The
  rule is by state"), section 5.
- The review, deferred: who moves an active intent to done or planned → section 6.
- The user's rule on who edits what (title and goal the user's; the note the active
  intent's manager's; additions to a planned intent's note from a manager of its project;
  done and dropped the user's; every edit kept and restorable) → section 2, with sections 4
  (what managers are told) and 5 (where each part is checked).
