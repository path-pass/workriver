# Navigation: the rail, intents, the side panel, addresses, signing in

Where the user goes and how: the rail, an intent's page, the side panel and its Needs you,
Agents and Backlog sections, signing in and addresses.

Moved from `DESIGN.md` when it was split into these files, word for word but for paths
made whole and "see …" naming the file; `DESIGN.md` is the overview and says where each of
its sections went.

## The rail

- A dark rail on the left picks units and nothing else: product name, the project switch
  (a select in the rail's colours, "All projects" first; remembered; switching it opens the
  project's first unit, or its empty state – the start guide naming the project – when it has
  none, and sets the address; the unit on screen stays when it belongs to the project picked),
  search (unit ids, titles and purposes, and intent ids: an intent's id shows it with all its
  units), then the intents in three
  groups under quiet headings: **Active** (with their units), **Roadmap** (planned intents
  in order, each with its id and title – a click opens its page –, a link chip when it is
  tracked elsewhere, and "Start", which opens a dialog to pick
  the Herdr workspace it starts in: a select, workspaces with a manager first, each with its
  manager's name and state ("chat · boss: idle"), those without saying "no manager"
  (workriver's own "workriver-commands" workspace of command shells is left out). Each
  workspace works on one project, which workriver works out: from the folder its manager runs
  in (a registered project, or the repository it is in; another machine's folders only as the
  repositories of units recorded there), else its current intent's project; what the user
  sets in Settings ("Workspaces' projects", a select per workspace) wins. The workspaces of
  the intent's project come first, the first preselected; those known to work on another
  project are folded under "Other projects' workspaces" (a link under the select opens them as
  a group of the select), each saying which project; one picked asks again in a line ("Its
  manager … works on memriver; give it a workriver intent?") and "Start" reads "Yes, start it
  there". An unknown one says "project unknown" and asks nothing more. The service refuses a
  start into another project's workspace unless the page says the user confirmed it. With a manager,
  "Start" records the start as the user's (as `workriver intent start` would) and types into
  that manager's terminal "[workriver] The user started i3 «title» in your workspace … split
  it into units and tasks (`workriver guide how start-intent`)", its goal quoted; the dialog
  then says where it started and whether the message was delivered (or why not, in the user's
  language). A workspace without a manager says so and shows the steps to start one, with the
  commands to copy, the last being to start the intent here once it has one (a workspace's
  first intent is the user's to start; its manager starts only its project's) – workriver
  never starts an agent; a row is moved by dragging its grip, or Alt+↑/↓ on it, within its project; a quiet
  "+" adds one inline, title and one-sentence goal, to the project shown; a quiet "Merge…"
  in the heading, when a project has two planned intents or more, opens a dialog to merge
  them), and **Done**,
  folded; dropped intents are not shown. Settings at the bottom. It is the only large field
  of colour in the interface. Each active or done intent is a group that folds (its chevron),
  on two lines only: its id quietly in mono before its title (as units show `u1`), the title
  cut to one line – a click opens the intent's page, a tooltip shows the title and the
  goal's first sentence –, then one quiet status line: the Herdr workspace it runs in, its
  manager's state, the quiet mark, "Ready to close". "no manager" shows only on an active intent
  that has none, in amber (its findings reach no one); nothing of the kind for planned ones.
  The goal and the note are not in the rail; they are on the intent's page. The intent of the open
  unit is open, the others folded, and a folded group still shows the sum of its units' violet marks, so
  nothing waiting is hidden. A search looks through every unit and opens the groups with a
  match. A unit where nothing has moved for 3 days (no task started, delivered or dropped,
  gate opened, finding decided or commit, and no running task with its agent at it) says
  so quietly: a moon and "quiet 5d" on its line, the date on hover. An active intent says
  the same under its title when nothing moved for 3 days in any of its units (a merge
  counts) or its own tasks, and none has a running task with its agent at it; units that
  never moved do not hold it back. Not for merged or done units, nor planned or done
  intents. The 3 days are fixed for now. Each unit's line carries at most two marks, none drawn at zero: tasks an agent is
  stuck on behind the user (filled violet, a hand, the count) and everything else waiting for
  the user – open findings and commits that name no task – (an outlined violet count). Where
  the work grew in scope is shown on the graph, not here (see "Where the scope grew", `activity.md`). Before them, where the unit stands as a dot and a word, except
  while it is simply being worked on: waiting, ready to merge (in the done tone, so what
  can be merged stands out; it counts nowhere as attention), merged (with its date) or done
  (why, on hover). The unit header carries the same word. An active intent whose units are
  all merged or done says "Ready to close" in its header, for its manager to close it. Who is working is not shown
  here, with one exception: each active intent's status line names its manager's state
  quietly (a crown, then working, idle, not running, or "no manager" in amber), since a
  missing manager means accepted findings reach no one. The rail folds too (its button at the top, or ⌘B / Ctrl+B): to a thin strip with
  the product mark, the button, settings and, if any task anywhere is stuck on the user,
  the filled violet mark; on a narrow screen, to one header row. Folded or not is
  remembered, and the graph refits to the room it gets. Open and on a wide screen, its
  right edge drags its width (200 to 480 px, 248 by default; double-click resets): a 6 px
  hit area with the column-resize cursor that shows a line in the selection colour when
  pointed at, focused or dragged. It is a separator for the keyboard too (arrows move it
  by 16 px, Home and End go to the ends). The width is remembered; the graph refits once
  when the drag ends, not during it. With more width, unit and roadmap titles use it
  (they wrap); an intent's title stays on one line and is cut with its full text on
  hover; at the default width it keeps the line to itself (and the folded group's violet
  marks), everything else about the intent is on the line under it. The handle is a small hook of ours (`frontend/web/src/lib/useResize.ts`), written so the side
  panel (and the Activity graph's height, `activity.md`) could use it too: react-resizable-panels measured +15.3 kB gzip for this one
  handle, which is the case the "prefer a library" rule excepts.
- **Kind, size and tags in the rail.** An intent's kind (feature, fix, chore, research: closed,
  translated) and estimated size (S, M, L) show as one quiet chip, the kind's label and the
  size (`Fix · S`), neutral (colour means the state of work, `presentation.md`; a kind has
  none): on a planned row's second line, before its link chip, and at the start of an active
  or done intent's status line; no chip when neither is set; the size's meaning on hover.
  Tags (free words, content, never translated) are not in the rail; the row's tooltip lists
  them. A quiet filter button in the rail's top row (left of the fold button; a number when
  filters are on) opens a popover in the page's colours: kinds, sizes (each with what it
  means) and the tags of the intents shown, the most used first, each a checkbox. Within a
  group any one ticked passes, the groups together; an intent without the field is left out.
  It narrows Active, Roadmap and Done alike; a line under the top row says "Filtered: 12 of
  31" (of the intents shown, dropped ones not counted) with "Clear". It is remembered per
  project (`workriver.filter.<folder>`, `*` for all), like the project switch. The roadmap is
  never re-sorted by it: its order is the user's priority; drag and Alt+↑/↓ move a row
  relative to the rows shown, recorded before or after that row as without a filter. Search
  matches a tag too: an intent whose tag holds the text shows with all its units, a planned
  one with its row. The inline "+" has a kind select under the goal (optional, "Kind" as
  placeholder); size and tags are set on the page. Sorting or grouping by kind is not the
  rail's (a board view of intents would do it, from the same fields).

## An intent's page

- **An intent's page** (`/i3`, `workriver link i3`; its title in the rail opens it): its id
  and title with "copy link" and a quiet pencil (edit), its state, its kind and size chip
  (as in the rail), "Ready to close", its link chip; its tags as chips on a line under the
  title; its goal under it, and under that, once it has more than one version, a quiet line
  "v5 · edited 2d ago by claude 1 · History" ("by you" for the user; the date on hover). Then a
  block with its note (short Markdown with links, as an agent's longer text; folded past six
  lines; `workriver intent set i3 --add-note` adds a paragraph) and where it runs: the
  workspace (and quietly "works on <project>", when known), the manager (name, state, the terminal button, and "Write to the manager",
  which opens the side panel's Manager section on that workspace's conversation),
  and who decides its findings (the select that was in the rail). A planned intent says it
  is not started yet and has "Start" in its header (the same dialog). Then its units as a
  list, each a row that opens it: id, title, where it stands, its violet marks, quiet, and
  when it last moved ("moved 2d ago", the date on hover). Last, when it has any, "Earlier
  research and design": its own tasks from before research units, as a graph with the same
  detail under it as a unit's, so `/i3/1` opens there with the task picked.

## Editing an intent, and its history

- **Editing** is a dialog opened by the pencil, on every state (the page is the user's, who
  edits every field of every intent; who else may is in `docs/security.md`): Title (one line,
  required), Goal (two rows), Note (a textarea, Markdown, with a Write / Preview switch; the
  preview renders as the note does), then Kind (a select, "Not set" first), Size (S / M / L /
  Not set, a segmented control; what each means is read out) and Tags (a token input: Enter
  or a comma adds one, × or Backspace in the empty field removes one, leaving the field adds
  what is typed; the project's tags are offered as one types; at most eight, kept
  lower-case: what is typed that would make more stays in the field, none of it added, and
  an alert under it says so until it fits or is cleared; while an input method composes,
  Enter, comma and Backspace are its own). What is left typed in the tag field is part of
  the edit: Save sends it with the tags. Save records only the fields that changed against the
  intent as the dialog opened it, with that version as `base`: the page reading the
  workspace again meanwhile changes neither (the same for a restore). The note is saved as
  written, its indentation and blank lines included (they are Markdown); spaces at the ends
  of the title and goal are not kept. A kind or size cleared is sent as null, tags cleared as
  an empty list; one not changed is not sent at all, so it stays as it is. Save is disabled when nothing changed, the title is
  blank or the tag field holds what would make more than eight (the edit stays open as it is). Cancel and Escape close it. Not inline: six fields, a conflict and a preview need
  one place.
- **A conflict**: when a field it saves was changed meanwhile, the service refuses it; the
  dialog stays open with the user's text and, for every field it would save that changed
  since it opened (not only the one the service names, and also one the user edits after the
  refusal), says so with the version the intent is at and shows what that field says now
  against the user's text, word by word; then it offers "Save anyway" (saved from that
  version). Nothing is lost: what it said stays in the history. No
  three-way merge, and no live sign while the dialog is open.
- **The history** is a dialog opened by "History": the versions, the newest first, each with
  its number, who (an agent with its robot mark, as the manager's decisions are shown; "you"
  for the user), how long ago (the date on hover) and what it changed in words ("title, goal
  and note", "added to the note", "created", "i7 merged in", "kind and size", "tags"); beside them (above them on a
  narrow screen) the one picked, the newest at first: title, goal and note against the
  version before it, word by word (removed struck through in the removed colour, added in the
  added colour; words as the language has them, so Chinese and Japanese are not split mid
  word), only what changed, kind, size and tags in words ("Not set" for none); "Whole text"
  shows it whole, the note rendered, its kind and size chip and tags under the goal. The first version
  is shown whole. "Restore this version" (not on the newest) opens the edit dialog filled with
  it, saying which version it restores, its kind, size and tags included; saving makes it a
  new version, nothing is removed.
  Its versions are read when it opens (`/api/intent/{id}/history`), not with the page.

## Merging planned intents

- **Merging planned intents** is a dialog, not a mode of the rail: the planned intents of the
  project shown (with all shown and several to merge, a select picks the project) as a list
  of checkboxes, each with its title, its goal on one line and its note folded; one that
  holds something (tasks, units, backlog findings, chores) is shown but cannot be ticked, and
  says why. With two or more ticked: which one stays (a select among them), its title
  (prefilled from it and following it until the user writes one), and a line saying what
  happens: the others are dropped and what they said goes into its note. The one that stays
  keeps its kind and size; the user's merge gives it the tags of all (its own first, at most
  eight), each merged one's kind, size and tags going into its block of the note
  (``fix · S · `security`, `cli` ``: each tag a code span, so that a tag reads as written
  whatever Markdown it looks like). An agent's merge (`workriver backlog merge`, of intents agents
  added) keeps the target's tags as they are and writes the merged ones' only into the note. Merge and Cancel;
  the service's refusal shows in the dialog. Escape closes it and focus goes back to "Merge…".

## The side panel

- A **side panel** on the right holds what is not looked at all the time: **Needs you**,
  **Agents**, **Commits**, **Backlog**, **Machines**, **Run a command** and **Manager**, one at a time, never mixed. Its buttons work as a
  switch: each shows its section, the one already shown folds the panel again; the shown
  one looks pressed. Folded, it is a thin strip with the buttons (no counts: the rail
  already has them); open, it is 340 px wide by default and its left edge drags it between 280 and 720 px (remembered; double-click for the default; arrow keys too). Which section is open, if any, is
  remembered. The panel spans every intent. A row in it opens its unit and task in the
  main area and the panel stays open; a row names its unit (the inbox also its intent,
  quietly). On a narrow screen the same content is a sheet over the page, opened from the
  same buttons in the unit header, with the same switch inside.

## Needs you

- **Needs you** is the workspace's inbox. One person works one queue across all units, so
  what asks something of them is one list, not a row of chips per unit: a list keeps order
  and age and still reads with twenty agents, chips only pile up. One row per item: the
  kind as an icon, a title, the unit, and how long it has waited; in the narrow panel a
  row takes two lines. Groups, each behind a small header with its count, in this order,
  an empty one not drawn:
  1. **Blocking** – first, agents waiting for a person in their terminal (an approval, a
     question; the runtime says "blocked"), across every workspace, in orange (`--hold`: the gates'
     lightness and chroma, another hue): the agent's mark and "ja-impl is waiting for you in its terminal", its topic, its intent or workspace, how long it has waited; a click brings its
     terminal to the front, and the row leaves by itself once it is no longer waiting. Then
     workers that stopped without delivering: a running task whose agent sat idle past the
     threshold in Settings (10 min by default), was reminded of its task in its terminal, and
     is still idle half that again later, or whose agent is gone: "u1#3: ja-impl stopped
     12 min ago without delivering" (or "is gone"), quietly when it was reminded, the unit;
     the row opens the task, a terminal button of its own opens the terminal; it leaves by
     itself once the agent works again or the task is delivered or dropped. Then
     gates waiting for the user; an agent cannot go on. Violet. Rows with a background stand 2 px apart. Both
     count in the filled violet marks (the intent's in the rail, the folded rail's), the
     tab's title starts with their number ("(2) workriver"), and, if the user turned it on in
     Settings, a newly waiting agent raises a browser notification that opens its terminal.
  2. **Machines** – a machine past an alert threshold: "This machine: Swap 5.2 GB", then
     its level as a word in its colour ("Warning" amber, "Critical" red, the icon too) and
     quietly the threshold ("over 4 GB for 1 min", "over 60 %, at once"), since when, and
     on a third line the five processes using the most memory when it rose or last rose a
     level, each with the agent (and workspace) it descends from ("claude 6.1 GB (reviewer
     · chat), …"). An alert that rises from warning to critical stays the same row. It
     counts in no mark and not in the tab's title (those count work stuck on the user); a
     newly raised one sends a browser notification under the same switch, and one that
     becomes critical sends another; never again while it stays. The row opens the
     Machines section and leaves by itself once the alert clears.
  3. **Decisions** – one row per review or gate with open findings, with the open ones
     per severity and how many carry an agent's suggestion; the highest severity first.
  4. **Messages** – what was said to the user and not read yet, the newest first: one row
     per workspace whose manager wrote (its newest words, how many new in violet, the
     manager), which opens that workspace's Manager conversation; and one row per task whose
     agent answered in its discussion (`conversations.md`, Discussing a delivered result):
     its newest answer's first line, how many new in plain ink (nothing waits on it), the
     agent and the task, which opens the task at its discussion. They count in no mark and
     not in the tab's title, and leave once read.
  5. **Gaps** – commits that name no node.
  6. **Not urgent** – chores, quietest; a row opens its steps. A chore of no intent names
     its project where others name their intent, or nothing when it has none. Then, for an
     intent with no manager, ready units that look squash-merged or have been ready for
     long ("u2 looks squash-merged into main"); the row opens the unit.
  Within a group the oldest comes first. A row of Blocking, Decisions or Gaps opens the
  unit at the node (a review at its first open finding). With nothing to show, one short line says so.

## Agents

- **Agents** are not in a unit, because a session crosses units. They are a tree, as a list
  (indentation and thin guide lines, no cards; it reads at 340 px and up to 720 px), by the
  Herdr workspace each runs in. A group's heading is the workspace's label (the project it
  works on in its tooltip) and, quietly, the title of its current intent (or "no manager"); groups with someone working come first,
  then by name. Under it the manager (with a small crown), and indented under the manager
  its workers: those that took a task (recorded on the task, since Herdr does not say who
  started a session), blocked first (a violet hand: the Blocking meaning), then working,
  idle, idle with nothing to do, gone. A temporary question agent (`why-qN`) is indented
  under the worker whose task it asks about, saying who asked ("asked by agent-B", "you
  asked"). A line: the mark, the name (its marks and machine chip never shrink), its state
  in a word on the right (working, idle 2d, blocked 5m, gone); under them, quietly, the
  harness and the task it is on or holds, else "last:" and the one it last did, with its
  unit. An idle worker holding no work says "can be closed · 5d" in a quiet chip instead.
  A gone worker shows only while it still holds unfinished work, greyed, without a
  terminal button. Sessions that never took a task are not workriver's: a folded "Not
  workriver's (N)" group at the end, with their topic. The amber ring around the mark means
  it is writing, as on a node. A line opens its task (a question agent's: the task asked
  about); the line that led to what is open shows as picked; the terminal button at its
  end opens its terminal. An agent Herdr knows by no name is called by its harness and a
  number within its workspace ("claude 2"); its mark reads "C2". How long an agent has been
  idle is the service's: when the runtime first showed it idle (kept in memory, so a
  restart of the service starts it again); without that, since it last delivered.

## Backlog

- **Backlog** lists the deferred findings of the project picked in the rail (or of all):
  those from reviews, tests and gates, grouped by intent and unit, and those added to the
  backlog from outside (an earlier review, a notes file), grouped by where they came from;
  highest severity first, each with its id, age, note and link chip, and a quiet "no impact
  or place given" when an added one lacks them. A row's problem opens the task that found
  it. Checkboxes pick deferred ones; "Do together" sends them as one message to the manager
  of the intent they belong to (or of an active intent picked from a select) and they show
  as scheduled until the work they cause is delivered; it is disabled, saying to start the
  intent first, when that intent is not active. "Drop" asks for a reason. Planned intents
  are not repeated here; they are the rail's roadmap.

## Signed in or not

The page decides and acts as the user, so it works only with a session (`docs/security.md`).
Without one it shows a single calm screen, nothing of the app: the product mark, one
sentence on why, the command `workriver open` with its copy button, and a quiet line that
the link works once. No form, no token field: the link from the user's own terminal is the
only way in. A session that ended shows the same screen at the user's next action.
A unit's or task's own link opened signed out lands there after `workriver open` (the
page keeps the path for ten minutes; the tab with the link follows once it is in front).

## Addresses

Each unit, intent and task has its own path, never the fragment (it carries the sign-in
code): `/u1`, `/u1/3` (the task picked, its detail open), `/i3` (the intent's page, planned ones
too), `/i3/1` (a task of its earlier own work, picked on its page), and
`?finding=P2-1`; the root opens the first unit of the project remembered in the rail (or of
all). The page follows the address on load and on back and forward, and sets it
when a unit or task is picked; a path that names nothing says so in the main area, the rail
as usual. The unit heading, the task detail and each intent in the rail have a quiet
"copy link" button (in the rail it shows on hover or keyboard focus). Agents get the same
links from `workriver link <ref>`.
