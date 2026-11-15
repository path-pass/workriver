# workriver design

Direction set on 2026-11-15 following the Operate mode of the `impeccable` skill. The
structure below was worked out on a throwaway prototype (`.local/prototype/`, not in git);
the real UI lives in `frontend/web/`.

## Use scene

One developer at a desk, next to terminals running twenty or more agent sessions. They
look at this many times a day, for tens of seconds to a few minutes each time. It is a
tool, not a showcase: finding things at a glance comes first.

Light and dark both ship and follow the system. The terminals around it are mostly dark,
so the dark theme is composed on its own, not an inversion of the light one.

## Structure

Terms (after AWS AI-DLC where it fits): an **intent** is a high-level goal; it groups the
**units** that serve it, each a piece that can be implemented on its own, on its own branch;
a unit's work is a graph of **tasks**, each one piece of work handed to one agent
(implement, review) or to the user (a gate). A task is drawn as a node. In the runtime one
intent is one Herdr workspace; the UI only says intent.

Three columns: the rail picks a unit, the main area shows it, the side panel holds what is
looked at now and then.

- A dark rail on the left picks units and nothing else: product name, search, the units
  grouped under their intents, settings. It is the only large field of colour in the
  interface. Each intent is a group that folds; open, it starts with the intent's goal in
  one or two quiet lines (when its manager or the user gave one). The intent of the open
  unit is open, the others folded, and a folded group still shows the sum of its units' violet marks, so
  nothing waiting is hidden. A search looks through every unit and opens the groups with a
  match. Each unit's line carries at most three marks, none drawn at zero: tasks an agent is
  stuck on behind the user (filled violet, a hand, the count), everything else waiting for
  the user – open findings and commits that name no task – (an outlined violet count), and
  "beyond plan" (amber). A merged unit shows its date instead. Who is working is not shown
  here, with one exception: each intent's header names its manager's state quietly (a
  crown, then working, idle, not running, or "no manager"), since a missing manager means
  accepted findings reach no one. The rail folds too (its button at the top, or ⌘B / Ctrl+B): to a thin strip with
  the product mark, the button, settings and, if any task anywhere is stuck on the user,
  the filled violet mark; on a narrow screen, to one header row. Folded or not is
  remembered, and the graph refits to the room it gets.
- A **side panel** on the right holds what is not looked at all the time: **Needs you**,
  **Agents** and **Commits**, one at a time, never mixed. Its three buttons work as a
  switch: each shows its section, the one already shown folds the panel again; the shown
  one looks pressed. Folded, it is a thin strip with the buttons (no counts: the rail
  already has them); open, it is about 340 px wide. Which section is open, if any, is
  remembered. The panel spans every intent. A row in it opens its unit and task in the
  main area and the panel stays open; a row names its unit (the inbox also its intent,
  quietly). On a narrow screen the same content is a sheet over the page, opened from the
  same buttons in the unit header, with the same switch inside.
- **Commits** is the feed of every unit's commits, newest first, by day (today, yesterday,
  then the date). A row: how long ago, the author's mark, the subject, and quietly the unit,
  the task (or the red mark of a commit that names none) and its +/− lines. Above them,
  while any agent has uncommitted work, "Writing now": each such agent with its amber ring
  and the files it is writing. A row opens its unit and task and brings that commit into
  view in the code layer; it stays picked.
- **Needs you** is the workspace's inbox. One person works one queue across all units, so
  what asks something of them is one list, not a row of chips per unit: a list keeps order
  and age and still reads with twenty agents, chips only pile up. One row per item: the
  kind as an icon, a title, the unit, and how long it has waited; in the narrow panel a
  row takes two lines. Groups, each behind a small header with its count, in this order,
  an empty one not drawn:
  1. **Blocking** – gates waiting for the user; an agent cannot go on. Violet, the
     strongest rows.
  2. **Decisions** – one row per review or gate with open findings, with the open ones
     per severity and how many carry an agent's suggestion; the highest severity first.
  3. **Gaps** – commits that name no node, branches that grew beyond the original plan.
  4. **Not urgent** – chores, quietest; a row opens its steps.
  Within a group the oldest comes first. A row of the first three opens the unit at the
  node (a review at its first open finding). With nothing to show, one short line says so.
- **Agents** are not in a unit, because a session crosses units. One line per agent whose
  session has not ended, working first (on a node, or by the runtime's word when it has
  none, as a manager planning), then holding work that has not started (the mark faded),
  then idle (a grey mark and a moon, nothing else). A working or waiting line
  names the unit and node it is on, and a click opens them; an idle line opens the last
  node it finished. The amber ring around the mark means it is writing, as on a node. The
  line that led to what is open shows as picked. Whether a session has ended is the only
  thing recorded about agents; everything else comes from the nodes. Inside a unit, who
  is on which node is the mark on the node. The list is grouped by intent in the rail's
  order; an intent's manager comes first in its group and carries a small crown.
- The main area shows the picked unit: its name and one-line purpose, nothing else (the
  graph already shows what waits: violet gates, violet counts on reviews), then two
  layers stacked top to bottom, each one block:
  1. **Activity** – the flow of work: how the nodes changed.
  2. **Code** – the file tree on the left and the changes on the right, as in a pull
     request's "Files changed". The tree pane, with its "only show changed" switch on
     top, sticks to the top of the window and scrolls by itself, so a long diff never
     hides it. The tree is there to keep attention on where agents are changing things:
     "only show changed" is on by default and hides every file and folder the unit did not
     touch; turned off, the whole repository shows where the changes sit. A chain of
     folders with one child each is one row. A changed file shows how it changed (A, M or
     D in the diff's green and red), its +/− lines and, while an agent writes it, the
     agent's mark with its amber ring; a folder sums its files and shows one mark (with a
     count if several write inside). A file a piece of work plans to change shows a dashed
     circle; one it will add, and that does not exist yet, also has a dashed underline.
     With a piece of work picked, the files it touched are open and normal and the unit's
     other changes are folded and faded; with nothing picked, all of the unit's changes
     are open. Folders holding a file an agent is writing are open at first.
     With no file picked the right side lists the commits of what is picked (or of the
     whole unit), hunk by hunk. Picking a file shows that file alone and whole: every line,
     with its change over what is picked (or over the whole unit) marked, and one line
     above naming the pieces of work the change comes from. Picking it again, Escape or
     "all changes" goes back to the list; picking a node that did not touch the file lets
     it go. On a narrow screen the tree stacks above the changes with a height of its own
     and opens on the pick's files only.
- A second view of the whole repository by function, with a panel listing the units that
  changed the selected module, is postponed. The unit view is the whole app for now.
- Picking a node shows its input and output directly under the graph, not in the side
  panel, and lines cannot be picked: why a node exists is its input. On work that is
  running or still to do, the header names its agent and, while the session lives, a
  terminal button. Output corrected after delivery shows its latest values, with a quiet
  "amended" and the time next to the Output heading.
- **Opening a terminal.** A small terminal button sits at the end of each agent's line in
  the panel (a click target of its own: the line opens the agent's work, the button its
  terminal) and in the header of running or waiting work. It is not on graph nodes. All
  of it goes through one function in the UI (`runtime.focusAgent`); until the daemon is
  there it only says what it would do.
- **Asking why.** There is no "why" button on every element. One ask box sits at the
  bottom of a picked task's detail, always there. What a question is about is the current
  selection, shown as chips above the box: the task always, the picked finding, and the
  picked file – or, when lines are picked, the lines instead of their file. Lines are
  picked in any diff, the single file or the list (click a line number, shift-click to
  stretch); next to them floats "Ask why", like the "+" of a code review. It picks the task
  that last changed those lines (worked out from the unit's commits; other tasks that
  changed some of them come along as chips) and puts the cursor in its box, so the code can
  be asked about with no task picked. A chip can be left out for this question. `?`
  (outside a field) does the same with picked lines, and otherwise puts the cursor in the
  box; Enter sends, Shift+Enter breaks the line. On a gate – the user's own task – the
  question goes to the agent that asked for it (the agent of the task the gate follows; with
  none, the manager bot), so "step 2's curl fails, what now" reaches whoever wrote the
  steps; its chip is the gate's title. A temporary agent answers
  from workriver's records and the code; it asks the agent that did the work only when
  that agent is idle, never one at work, and the answer says quietly whether it did. While
  it works, the step it is at shows. The questions and answers stay on the node, above
  the box; file paths in an answer open the file. A node with questions carries a small
  neutral speech-bubble mark with their count on the graph: information, not a button.

## Colour means one thing: the state of a piece of work

| Role | Meaning | Where |
|---|---|---|
| Water (`--water`) | It has flowed: done | Done nodes, lines that have flowed, the start |
| Amber (`--amber`) | Flowing now: someone is on it | Running nodes, the line into them, the ring around a writing agent's mark, "grew beyond the plan" |
| Violet (`--you`) | Waiting for you, and what you picked | Gates waiting for the user, the count of findings waiting for a decision, selection outline, the file selected in the tree, focus ring, primary button |
| Neutrals | Not started, or just ground | Everything else |

Three things that used to share colour now have their own channel:

- **Who raised it** is an icon: shield for security review, eye for code review, flask
  for tests.
- **What kind of work** is a shape: square corners for implementing, a pill for a review,
  a double frame with sharp corners for a gate (work only the user can do).
- **Which agent** appears only on the agent's mark. Its colour is derived from the name
  and is used nowhere else.

## Findings

A review's output is a list of findings, and they live in the review, not on the graph.
A decision is not a node. Each finding gets one of three outcomes: accepted (work was
handed out for it: one finding may lead to several nodes, several findings to one node),
rejected (struck through), or deferred (quiet, with a clock); until then it is open.
An agent may decide on its own: such a finding carries a small neutral robot mark, a
decision by the user none. An open finding may carry the agent's suggestion, and its
reason once the row is opened.
Severity (P1–P3) is text, never a colour: the review node says how many it found per
severity, and its list is grouped by it. Only open findings wait for the user: they show
as a violet count on the review node, in the unit's outlined count in the rail, and as a
row of the inbox. Picking a
finding lights the review and the nodes it led to.

## Gates and chores

Both are things an agent hands to the user; what separates them is whether an agent is
stuck. A **gate** blocks: a manual deploy, a manual try-out. It is a node, placed where
the flow stops, violet while it waits and water once done; the work behind it stays not
started, and the lines around a waiting gate do not flow, so a stuck branch is visible on
the graph. Its input is the steps the agent prepared, each command with a copy button;
its output is a short note and, for a try-out that went wrong, findings like a review's.
A **chore** blocks nobody (clean the disk, rotate a key) and is never on the graph or
attached to a task. It belongs to an intent (sometimes to one of its units): it is the
last, quietest group of the inbox, opening the same list of steps and a "Done" that takes
it off the list. It is not counted in anything violet and goes to no one.

## What the user puts in

Everything the user decides goes to the service through one place (`runtime.ts`), which
records it and delivers it to the agents through Herdr.

- **A waiting gate's output is a form**: Done or Problem; a note; with a problem, findings
  with the same three fields as a reviewer's (severity P1–P3, what is wrong, what happens
  if it is left, where), at least one. Pictures can be pasted into the note or a finding,
  dropped on the form, or picked with "Attach"; they show as thumbnails with a ×, and one
  opens large on a click. Only images for now. Submitted, the gate is done, its findings are
  open findings like a review's (so they wait in Needs you) and the work behind it stays
  not started until they are decided. The pictures stay with the gate's output and with
  each finding, wherever it shows, including as the cause of the work it led to.
- **An open finding** takes one of three decisions, Accept, Reject or Defer, with an
  optional one-line note; the agent's suggestion, if any, is the filled button. The row
  takes its new state at once and every count drops (the task's badge, Needs you, the rail).
- **Delivery.** A gate's result and a decision then show where their message is: not
  delivered yet (and "Undo" until it goes out), delivered to the agent, picked up by it (it
  started working after the message arrived), or not delivered with the reason in the
  user's language (no manager, manager not running, Herdr not reachable, Herdr refused;
  the service's own words in a tooltip). Every decision goes to the intent's manager agent
  ("sent to the manager bot"), which turns accepted findings into tasks and tells the
  workers what they need; a gate's result goes to the agent waiting behind it, else the
  manager. The service writes and sends the message; the line shows its record, and the
  message can be copied as the agent reads it. The manager is also told, without anything
  on this page, when a worker delivers a task or asks the user for a gate or chore.

## Evidence

Who tests is the work's business: unit tests are part of the implementing agent's own
task, a tests task is an independent agent's acceptance work (however it goes about it),
and a manual try-out is a gate. What workriver records the same way for all of them is
**evidence**: the command run and what came of it ("12 passed", "1 failed, 4 passed"). A
task's output shows one line per check: a tick or a cross, the command in monospace with a
copy button, the summary. Passed takes the done tone (water) and failed the diff's red –
the convention of every diff viewer, not a new colour role. Reviews need none; a tests task
is expected to have some, and a failed run also carries findings. An implement task that is
done with no check run carries a small neutral flask-crossed mark on the graph, "no tests
run": information about work handed in untested, not violet and not counted in Needs you.

Added and removed lines are green and red, the convention of every diff viewer; they
appear only in the code layer.

Colours are defined in OKLCH as semantic tokens. A theme remaps the tokens; components
never name a raw colour.

## Type

- One sans family, the system's. Monospace only for ids, paths, commit hashes and code.
- Fixed sizes, about 1.15 apart: 11, 12, 13, 14, 15, 20 px.
- Tabular numerals, so columns and line numbers align.

## Space and shape

- Spacing in multiples of 4: 4, 8, 12, 16, 20, 24.
- Radius: 6 for small controls and tree rows, 8 for nodes, 10 for blocks, fully round for
  review nodes, 3 for gates.
- Shadow appears only on hover and selection, meaning "this can be picked" or "this is
  picked".

## The signature: lines are a river

- Work that is done: a solid water-coloured line.
- Work in progress: the line into it is a row of dots moving forward. Apart from the
  agent's mark this is the only looping motion, and it carries state.
- The graph has a start and no end. A finished branch simply stops; its water colour says
  it is done.
- A line that would cross columns (a secondary input reaching far) runs through the empty
  space: along the gap between rows, then down just before its target.
- With "reduce motion" set, both are still.

## States

Everything that can be picked has: default, hover, keyboard focus (violet ring), selected
(violet ring plus shadow). With a node picked, nodes and lines unrelated to it fade.

## Rules that came out of iterating with the user

- Languages from the start: English and Chinese; Japanese ships too. Layout must hold in
  all three: English labels are longer, Chinese and Japanese ones are denser.

- If the interface can show it, do not also say it in text. Do not repeat what another
  part of the screen already shows.
- No "outside the plan". Every commit belongs to a node; one that names none is shown as
  unregistered, a gap to close, not a kind of work.
- The code is shown as a file tree, folder by folder. It is not a dependency graph and has
  no lines between files or folders.
- A reason is three structured fields: the problem found, what happens if it is left, and
  where (file paths, no code). They belong to a finding; work that answers findings names
  them and carries no reason of its own.

## Not done yet

- Contrast has not been measured pair by pair, only looked at in both themes.
- No empty or loading states.
- No syntax highlighting in the diff.
