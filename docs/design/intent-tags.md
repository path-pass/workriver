# Design: an intent's kind, size and tags

What an intent is (a fix, a feature, …), how big it is expected to be, and free tags for
what it touches, so the roadmap and the intent lists can be read, filtered and later
measured by them. A feature design: it lives on its branch; before the branch is merged its
rules go into `docs/design/ui/navigation.md`, `docs/security.md` (who may set them) and the
howto recipes, and this file is removed.

## 1. What is fixed and what is free

Decision: **a closed kind, a closed size, and free tags.** All three optional.

- **Kind**, one of four: `feature` (new behaviour), `fix` (behaviour that is wrong: a bug,
  a hole, a flaky test), `chore` (no new behaviour: refactor, docs, build, dependencies,
  release), `research` (the outcome is a decision or a report, not code). The first three
  are the branch prefixes units already take (`feat/`, `fix/`, `chore/`, `units.rs`), the
  fourth is the research unit; an intent's kind tells its manager which to use first.
  Spike is not a kind: it is how a unit tries something, inside an intent of any kind.
- **Size**, an estimate: `s`, `m`, `l`.
  - S: one unit, one round of implement and review; hours.
  - M: one or two units, a few review rounds; a day or two.
  - L: three units or more, or a design first; days.
- **Tags**: free words for the area or theme (`security`, `code-tree`, `messages`,
  `release`). A tag is 1–32 characters with no space or comma, kept lower-case (other
  scripts as written), at most 8 per intent, no repeats. They are content, shown as written,
  never translated.

Why this split, from the 55 recorded intents:

- A kind is wanted on every intent and must mean one thing, to filter and compare by: free
  tags drift (`bug`, `bugfix`, `fix`). Four cover all 55 without strain, by title and goal:
  37 features (Japanese UI, machine metrics, broadcast, ACP runtime), 12 fixes (mermaid
  rendering, moved files in the code tree, the flaky caller test, the security
  hardenings), 4 chores (worktree dependencies, PyPI release, README, structure
  clean-ups), 2 research (the restructure discussion, whether temporary agents stay). A fifth kind ("improvement",
  "security") was tried against the list and every case fit one of the four plus a tag.
- Closed values are UI text: translated (en, zh-CN, ja), one label each, a stable order to
  sort by. Free tags cannot be.
- Size is asked for by the user, and only an estimate helps where it is used: picking from
  the roadmap, before any work exists. Three steps are what a person can estimate; finer
  is false precision. What the intent really took is derived (section 3), not entered.
- Tags carry what recurs across kinds and is not a kind: the security hardenings (caller
  pane check, workspace ownership, CI secrets), the code tree (overlapping units, moved
  files, edge cases, odd file names), messages to managers (reminders, broadcast, reply
  rules), distribution (packaging, AWS, client, PyPI).

Turned down: tags only (no shared meaning to filter or measure by; the user's own example
names a kind and a size, which are one value each); a size derived only (nothing to see
on a planned intent, where it is wanted); story points or hours (estimates of agent work in
hours mean nothing yet).

## 2. Who sets them, and when; versions

- **The user**: on any intent in any state, at `backlog add`, the rail's inline "+", or
  later on the intent's page or with `intent set`.
- **Whoever adds an intent** sets them in the same `intent.created` (an agent's `backlog
  add`, the backlog import).
- **The manager of an active intent** sets all three, as it may replace the note: when it
  splits the intent it can judge the size, and it sets a missing kind. A planned, done or
  dropped intent's are the user's alone (a planned one's note takes only additions from a
  manager; there is no addition of a kind). The rule is `may_edit`'s, extended; its refusals
  say who may.
- The manager changes nothing the user set without asking: a recipe rule, not a check
  (the edits keep only who made the last version, not who set each field). The history
  shows every change and who made it, and the user restores.
- **Versioned like the title**: a change of kind, size or tags is a version of the intent
  (`Edits` gains a counter per field, `kind`, `size`, `tags`), shown in the history in words
  ("kind and size", "tags"), and part of what a version holds (`Text`), so a restore brings
  them back. A replace made from an older `base` is refused when the field it sets changed
  since, as for the title (`unchanged_since`). The estimate at the start and at the end are
  then both in the history, which is what a later comparison needs.
- **The user's change reaches the manager** of an active intent, in the notice it gets for
  title and goal (`delivery::edited`): "kind fix; size M; tags «security, cli»", tags quoted
  as the user's text. An agent's change is not told to anyone.

## 3. Where they show, and what is derived

- **Rail**: a planned row and an active or done intent's status line start with a quiet
  chip: the kind's short label and the size (`Fix · S`), neutral tone (colour means the state
  of work, `presentation.md`; a kind has no colour). No chip when neither is set. Tags are
  not in the rail; the row's tooltip lists them.
- **Filter, not sort, in the rail**: a quiet filter button in the rail's heading row opens a
  popover: kinds (checkboxes), sizes (checkboxes), tags of the project shown (checkboxes, the
  most used first). It narrows Active, Roadmap and Done alike; the button shows the number of
  filters on, a line under the heading says "Filtered: 12 of 31" with "Clear". Remembered per
  project like the project switch. The roadmap is never re-sorted: its order is the user's
  priority. Drag and Alt+↑/↓ still move a row relative to the rows shown; the move is
  recorded before or after that row, as now. Rail search also matches a tag.
- **The rail's inline "+"** gets a kind select beside the goal (optional, "Kind" as
  placeholder); size and tags on the page.
- **An intent's page**: the chip and the tags (chips) in the header after its state. The edit
  dialog gets Kind (select, "Not set" first), Size (S / M / L / Not set, a segmented
  control) and Tags (a token input offering the project's tags as one types; Enter or comma
  adds one, × removes). They are saved with the other fields that changed, from the same
  `base`.
- **What it took, derived** (never recorded): on an intent's page, once it has units, one
  quiet line: units, tasks, review rounds (review and tests tasks), findings accepted, and
  days active (started to done, or to now). Computed in core from the state, one function,
  so the CLI's `board` and a later analysis (estimate against what it took, by kind) read the
  same numbers. Lines of code are left out: they need git per unit and measure the wrong
  thing for research.
- **Not shown** in Needs you (rows name the intent; a kind adds nothing to "what waits for
  me") nor in Backlog (deferred findings). Sorting by kind or size belongs to the board view
  of intents and units planned separately; it reads the same fields.

## 4. Events and replay

- **No new event kind.** `intent.created` gains `kind`, `size` and `tags`; `intent.updated`
  gains the same three. All absent by default and skipped when absent or empty, so an event
  written without them is byte for byte what is written today, and every old event replays
  unchanged (an intent with no kind, no size, no tags). Replay fixtures need no edit.
- In `intent.updated`: a field absent leaves it as it is; `kind` and `size` written as
  `null` clear it (a double option); `tags` is the whole new set, `[]` clearing it. The rules
  normalise tags (trim, lower-case, drop repeats) and refuse one that breaks the limits,
  naming it.
- `intent.updated` with only a kind, size or tags is not "nothing to change"; its refusal
  text lists the new flags.
- **Merge** (`intent.merged`): the kept intent keeps its kind and size; its tags become the
  union of all (cut to 8, its own first); each merged intent's kind, size and tags go into
  its block of the note, one line (`fix · S · security, cli`). The merge is already a version.

## 5. CLI and what agents are told

- `workriver backlog add … [--kind feature|fix|chore|research] [--size s|m|l] [--tag T]…`
- `workriver intent set i3 [--kind K|none] [--size S|none] [--tags a,b|none]` (`--tags`
  replaces the set; with `--base` like the others).
- `workriver intent add` takes the same three as `backlog add`.
- `backlog list` and `board` print them after the title (`[fix S] security, cli`);
  `backlog list --kind fix --tag security` filters; `backlog export` writes them in each
  roadmap entry; `board` prints the derived line for an intent with units.
- Recipes: `start-intent` (when you split it: set a missing kind, and the size you judge,
  with `intent set`; do not change what the user set, tell them when you think it is wrong;
  the first unit's branch prefix follows the kind), `units` (one line on the prefix), the
  import skill (set the kind when the source says it: a bug list is `fix`; tags from its
  headings, few), `workriver-manager`'s hard rules unchanged. One `CHANGES.md` entry for
  managers with the flags and the rule.

## 6. Not done

- No per-field "set by the user" lock: a manager changing the user's kind or size is caught
  by the recipe and the history, not refused.
- No tag management (rename, merge, a project's list of allowed tags, colours): tags are
  only what intents carry; a rename is done intent by intent.
- No kind, size or tags on units or tasks; units keep their branch prefix.
- No sort or grouping by kind in the rail; that is the separate board view's.
- No analysis page: the derived numbers are shown per intent; comparing estimates across
  intents waits until there are enough sized ones.
- No tracker mapping (a Jira issue type or a GitHub label to a kind or tag): the tracker
  adapter, when it comes, does it.
- Existing intents are not classified by workriver; the user (or a manager asked to) sets
  them.
