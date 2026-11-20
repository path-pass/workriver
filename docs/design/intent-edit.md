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
  planned) keeps its workspace, so that workspace's manager can still change it.
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
- **CLI**: `workriver intent set i3 [--title …] [--goal …] [--note TEXT | --add-note TEXT]
  [--base N]`.
  - `--note` replaces the whole note (`--note ""` empties it; a long one as
    `--note "$(cat note.md)"`). It needs `--base`. `--note` with `--add-note` is refused.
  - `--add-note` stays as it is (append; needs no base: it overwrites nothing).
  - `workriver intent history i3` lists the versions; `--version N` prints one whole;
    `--diff N` prints it against N−1. `workriver board i3` shows the version next to the goal.

## 2. Who edits what

| | planned | active | done, dropped |
|---|---|---|---|
| the user (page, own terminal) | yes | yes | yes |
| the intent's manager | no | yes | no |
| any other agent | no | no | no |

- The rule is by state in core (`prepare_intents`), no longer by whether `manager_of`
  finds someone. That closes the returned-to-roadmap gap above.
- **Planned stays the user's.** The roadmap is the user's queue and a planned intent has no
  manager answering for it; whoever starts it owns it from then on. An agent that imported
  a wrong title asks the user, who now fixes it in one dialog. (Agents may still *merge*
  agent-added planned intents: tidying, not rewording; unchanged.)
- **Done or dropped: the user only.** The manager's work on it is over; the user may still
  correct a record (a title, a link to the outcome).
- **Other agents: no.** One owner per intent, as for its findings; a worker tells its
  manager (`workriver manager tell`).
- `findings`: unchanged, the user only.

## 3. Versions

- **No new event kind.** `intent.updated` gains `note` (the whole note) and `base` (the
  version the edit started from). Old records replay unchanged.
- **Versions are derived on replay**, in core's `apply`, into `Intent.versions`: v1 from
  `intent.created`; a new version for each `intent.updated` that sets title, goal, note or
  add_note, and for each `intent.merged` into it. A findings-only update or a state change
  is not a version. Each version keeps: `n`, `at`, `by` (the event's actor, never the
  body), which fields changed and their new text (the note whole when it changed), and for
  a merge which intents went in. Existing records become history at once (19 updates today).
- **Concurrent edits**: with `base`, the edit is refused when a version after `base` changed
  a field this edit sets (title, goal, note; `add_note` counts as changing the note). An
  edit of other fields goes through. The refusal names the version, who and when:
  "i3's note changed since v4 (v5 by claude 1, 2 min ago); read it
  (`workriver intent history i3 --version 5`) and run again with `--base 5`". The page
  always sends `base`.
- **On the page after a refusal**: the dialog stays open with the user's text, shows the
  newer text of the field that conflicts as a diff against it, and "Save anyway" (sends
  again with the newest base). Overwriting loses nothing: the overwritten version stays in
  the history. No three-way merge.
- **History on the page**: under the goal, only when there is more than one version, a quiet
  line "v5 · edited 2d ago by claude 1 · History" (the date on hover; an agent with its
  robot mark, as the manager's decisions are shown). "History" opens a dialog:
  - the versions newest first: `vN`, who, when, what changed in words ("title, note",
    "created", "i7 merged into it");
  - the one picked (the newest first): title and goal as old → new, the note as a word diff
    (`diffWordsWithSpace` of `diff`, already a dependency for the code layer; ins/del in the
    diff tokens of `presentation.md`); a switch "Whole text" renders that version's note;
  - "Restore this version" (not on the newest) opens the edit dialog filled with that
    version's title, goal and note, `base` the newest. Saving is a new version; nothing is
    removed or rewritten.
- **How the page reads it**: not in `/api/workspace`, which is read again on every change
  and would carry every intent's whole history (today about 41 KB, the bodies of the 99
  intent events, and it only grows) for a dialog opened rarely. The view carries each
  intent's `version` (its last n) and last editor; the history dialog reads
  `GET /api/intent/{id}/history`. The CLI reads `/api/state` as `board` does (versions are
  in `State`).

## 4. What agents are told

- **The manager is told when someone else edits its active intent** (today only the user
  can): `[workriver] The user changed i3 (now v5): title «…», goal «…»; its note changed
  (workriver intent history i3 --diff 5).` Title and goal quoted through `quote.rs`: `users`,
  since only the user's edits are told (an agent's text would take `quoted`); the note not
  quoted (long), only pointed at. Its own edits: nothing.
  Planned, done, dropped: no manager, nothing. It goes through the delivery path messages
  already use, so it waits while the manager is on a blind task.
- **CHANGES.md** (managers): `--note` replaces the whole note, with `--base` from
  `workriver intent history`; the refusal on a changed base; planned, done and dropped
  intents are the user's; the message above.
- **Recipes**: `start-intent` (the note paragraph: replace as well as add, the base, the
  message when the user edits; the returned-to-roadmap intent is no longer the manager's).
  `units`, `design-gate`, `spike` keep `--add-note` and need no change. The manager skill's
  hard rules do not change.

## 5. Security

- **Writes: no new route.** The page records `intent.updated` through `/api/append` as it
  does for the findings select; `App::who` makes the page's session the user and an agent's
  CLI that agent. Who may edit what is core's rule on the `Principal` (section 2), the last
  line for page and CLI alike. Not `Body::user_only`: the manager may record it too; the
  user-only parts (planned, done, dropped, `findings`) are refused in `prepare_intents` for
  anyone else.
- **Reads**: the new `GET /api/intent/{id}/history` goes through `App::view`, then reads
  `Intent.versions` of the view's `State`; a missing intent is refused in core's words
  (`no intent i9`, 422) as other reads by id are, a failed read fails the request (500). `blind_view` empties `versions` as it empties `note` (they hold the note and
  others' edits): a blind agent sees the current title and goal only. One line in
  `route-access.txt` (anonymous 401, agent 200, user 200).
- **Not on `machine_router`**: only the page reads the route; agents on other machines are
  workers, which do not edit intents, and read the board through `/api/state` as now.
- **Messages**: section 4; everything an editor wrote goes in through `quote.rs`. `by` is
  the event's actor, so the page's "edited by" is never a caller's word.
- **Rendering**: the note in the dialog's preview and the history renders with `Prose`, as
  on the page now; a diff renders text, never Markdown.

## 6. Not done

- Editing the external link (the tracker adapter's).
- Agents editing planned intents, also those an agent added.
- A three-way merge of concurrent edits; showing live, while the dialog is open, that
  someone else changed the intent (the refusal on save covers it).
- History of findings policy, state, workspace or roadmap order: the history is the
  intent's text only.
- A `restore` command: an agent restores with `--title`/`--goal`/`--note` from
  `intent history --version N`.
- Removing or hiding a version: the record is append-only.
- Anything about edits in the rail.
