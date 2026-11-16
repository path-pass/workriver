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

Terms (after AWS AI-DLC where it fits): a **project** is a folder holding one or more
repositories (or one repository outside every registered folder); an **intent** is a goal in
it, which may span its repositories; it groups the **units** that serve it, each a piece
that can be implemented on its own, on its own branch; a unit's work is a graph of
**tasks**, each one piece of work handed to one agent (implement, design, research, review)
or to the user (a gate). A task is drawn as a node. An intent is planned (in its project's
roadmap), active (started in a Herdr workspace by that workspace's manager), done or
dropped. A workspace may run several active intents; the one started last is its current
intent, where new work goes. Research and design that belong to the intent rather than to
one unit are intent-level tasks, shown as a unit of their own named "Research and design",
first under the intent, with no branch and no code layer.

Three columns: the rail picks a unit, the main area shows it, the side panel holds what is
looked at now and then.

- A dark rail on the left picks units and nothing else: product name, the project switch
  (a select in the rail's colours, "All projects" first; remembered), search, then the intents in three
  groups under quiet headings: **Active** (with their units), **Roadmap** (planned intents
  in order, each with its goal, a link chip when it is tracked elsewhere, its note behind a
  small "Note" disclosure on a line of its own, and "Start", which
  shows the steps to start it in Herdr with the commands to copy – workriver never starts an
  agent; a row is moved by dragging its grip, or Alt+↑/↓ on it, within its project; a quiet
  "+" adds one inline, title and one-sentence goal, to the project shown; a quiet "Merge…"
  in the heading, when a project has two planned intents or more, opens a dialog to merge
  them), and **Done**,
  folded; dropped intents are not shown. Settings at the bottom. It is the only large field
  of colour in the interface. Each active or done intent is a group that folds; open, it starts with the intent's goal in
  one or two quiet lines (when its manager or the user gave one), then its note.
  An intent's note (more about it; what merged intents said is added to it) is plain text
  with its line breaks and clickable links, cut to three lines with "Show more". The intent of the open
  unit is open, the others folded, and a folded group still shows the sum of its units' violet marks, so
  nothing waiting is hidden. A search looks through every unit and opens the groups with a
  match. Each unit's line carries at most three marks, none drawn at zero: tasks an agent is
  stuck on behind the user (filled violet, a hand, the count), everything else waiting for
  the user – open findings and commits that name no task – (an outlined violet count), and
  "beyond plan" (amber). Before them, where the unit stands as a dot and a word, except
  while it is simply being worked on: waiting, ready to merge (in the done tone, so what
  can be merged stands out; it counts nowhere as attention), merged (with its date) or done
  (why, on hover). The unit header carries the same word. An active intent whose units are
  all merged or done says "Finished" in its header, for its manager to close it. Who is working is not shown
  here, with one exception: each active intent's header names its manager's state quietly
  (a crown, then working, idle, not running, or "no manager"), since a missing manager means
  accepted findings reach no one; opened, its goal line also names the Herdr workspace it
  runs in. The rail folds too (its button at the top, or ⌘B / Ctrl+B): to a thin strip with
  the product mark, the button, settings and, if any task anywhere is stuck on the user,
  the filled violet mark; on a narrow screen, to one header row. Folded or not is
  remembered, and the graph refits to the room it gets. Open and on a wide screen, its
  right edge drags its width (200 to 480 px, 248 by default; double-click resets): a 6 px
  hit area with the column-resize cursor that shows a line in the selection colour when
  pointed at, focused or dragged. It is a separator for the keyboard too (arrows move it
  by 16 px, Home and End go to the ends). The width is remembered; the graph refits once
  when the drag ends, not during it. With more width, unit and roadmap titles use it
  (they wrap); an intent's title stays on one line and is cut with its full text on
  hover. The handle is a small hook of ours (`lib/useResize.ts`), written so the side
  panel could use it too: react-resizable-panels measured +15.3 kB gzip for this one
  handle, which is the case the "prefer a library" rule excepts.
- **Merging planned intents** is a dialog, not a mode of the rail: the planned intents of the
  project shown (with all shown and several to merge, a select picks the project) as a list
  of checkboxes, each with its title, its goal on one line and its note folded; one that
  holds something (tasks, units, backlog findings, chores) is shown but cannot be ticked, and
  says why. With two or more ticked: which one stays (a select among them), its title
  (prefilled from it and following it until the user writes one), and a line saying what
  happens: the others are dropped and what they said goes into its note. Merge and Cancel;
  the service's refusal shows in the dialog. Escape closes it and focus goes back to "Merge…".
- A **side panel** on the right holds what is not looked at all the time: **Needs you**,
  **Agents**, **Commits**, **Backlog**, **Machines** and **Ask why**, one at a time, never mixed. Its six buttons work as a
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
- **Machines** shows CPU and memory of this machine ("This machine", first) and of every
  followed one (by its machine chip), over the last ten minutes, read every 5 s while the
  section is open. Per machine two lines, CPU and Memory: the label, a small line of the
  ten minutes with the alert threshold dashed across it (inline SVG; a chart library is
  not worth one line), and the value right-aligned in a fixed column ("97 %",
  "25.4 / 32 GB"). Lines are neutral; a metric in alert takes the violet of waiting for
  you, and a pill in the machine's header says which, its value and for how long ("CPU
  97 % · 6m"). A machine that cannot be reached keeps its last values, faded, with the
  dashed chip and "disconnected since …". Memory used is total minus available, the same
  on every machine: one small sh probe reads them all through the runner git uses, here
  or over the machine's ssh connection, and nothing is installed anywhere.
- **Needs you** is the workspace's inbox. One person works one queue across all units, so
  what asks something of them is one list, not a row of chips per unit: a list keeps order
  and age and still reads with twenty agents, chips only pile up. One row per item: the
  kind as an icon, a title, the unit, and how long it has waited; in the narrow panel a
  row takes two lines. Groups, each behind a small header with its count, in this order,
  an empty one not drawn:
  1. **Blocking** – first, agents waiting for a person in their terminal (an approval, a
     question; the runtime says "blocked"), across every workspace: the agent's mark and
     name, its topic, its intent or workspace, how long it has waited; a click brings its
     terminal to the front, and the row leaves by itself once it is no longer waiting. Then
     gates waiting for the user; an agent cannot go on. Violet, the strongest rows. Both
     count in the filled violet marks (the intent's in the rail, the folded rail's), the
     tab's title starts with their number ("(2) workriver"), and, if the user turned it on in
     Settings, a newly waiting agent raises a browser notification that opens its terminal.
  2. **Machines** – a machine past an alert threshold: "This machine: CPU 97 %", quietly
     the threshold ("at or above 90 % for 5 min"), and since when. It counts in no mark and
     not in the tab's title (those count work stuck on the user); a newly raised one sends
     a browser notification under the same switch. The row opens the Machines section and
     leaves by itself once the alert clears.
  3. **Decisions** – one row per review or gate with open findings, with the open ones
     per severity and how many carry an agent's suggestion; the highest severity first.
  4. **Gaps** – commits that name no node, branches that grew beyond the original plan.
  5. **Not urgent** – chores, quietest; a row opens its steps. A chore of no intent names
     its project where others name their intent, or nothing when it has none.
  Within a group the oldest comes first. A row of Blocking, Decisions or Gaps opens the
  unit at the node (a review at its first open finding). With nothing to show, one short line says so.
- **Agents** are not in a unit, because a session crosses units. One line per agent whose
  session has not ended, working first (on a node, or by the runtime's word when it has
  none, as a manager planning), then holding work that has not started (the mark faded),
  then idle (a grey mark and a moon, nothing else). A working or waiting line
  names the unit and node it is on, and a click opens them; an idle line opens the last
  node it finished. The amber ring around the mark means it is writing, as on a node. The
  line that led to what is open shows as picked. Whether a session has ended is the only
  thing recorded about agents; everything else comes from the nodes. Inside a unit, who
  is on which node is the mark on the node. The list is grouped by the Herdr workspace each
  agent runs in, headed by the workspace's label and, quietly, the title of its current
  intent, with a quiet "no manager" when none is registered; groups with someone working come first, then
  by name. In a group an agent waiting for a person in its terminal comes first, with a
  violet hand (the Blocking meaning, not the amber writing ring), then the manager (with a
  small crown), then working, holding work, idle. Under each name, quietly, what the session is about (its terminal
  title); the agent's mark shows it on hover wherever the name is unique. An agent Herdr
  knows by no name is called by its harness and a number within its workspace ("claude
  2"); its mark reads "C2". A line is a grid, so nothing is drawn over anything: the mark;
  the name (truncated; its marks and machine chip never shrink) over the topic; where it
  works, in a column of its own that truncates; the terminal button. In a panel narrower
  than about 340 px, where it works moves to a third line (an idle moon stays on the first).
- The main area shows the picked unit: its name and one-line purpose, nothing else (the
  graph already shows what waits: violet gates, violet counts on reviews), then two
  layers stacked top to bottom, each one block:
  1. **Activity** – the flow of work: how the nodes changed. Its key (what the colours,
     marks and lines mean) is a row under the graph's canvas, wrapping as it needs, never
     over it: no node can hide behind it. Only React Flow's credit stays inside the canvas,
     in its own bottom-right corner, with room left for it when the graph is fitted.
  2. **Code** – the file tree on the left and the changes on the right, as in a pull
     request's "Files changed". A bar at the block's top (it stays in view while the diff
     scrolls) holds a toggle that hides or shows the tree (remembered). Where the block is
     wide (768 px or more, measured on the block, since the rails and the side panel change
     it) the tree is a pane beside the diff, as wide as its right edge is dragged (between
     220 px and half the block; remembered; a double click resets); narrower, the tree
     starts hidden and the toggle shows it as a drawer over the diff from the left (Escape
     or a press outside closes it; focus goes in and back), never squeezing the diff. The
     tree has "Filter files…" (by path; folders above a match stay; Escape clears) and its
     "only show changed" switch on top, sticks below the bar and scrolls by itself, so a
     long diff never hides it. The tree is there to keep attention on where agents are changing things:
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
- **Other machines.** Agents may run on other machines, each with its own Herdr. Whatever is
  on another machine carries a small chip with the machine's name (a server glyph): an
  agent's line in Agents, a unit's header and its line in the rail; an agent mark's tooltip
  ends with "@machine". When the machine cannot be reached, its agents stay as last seen,
  faded, the chip dashed (its tooltip says since when it is disconnected), and they raise
  nothing in Needs you. Settings says "disconnected since …" and when it connects again. A workspace of another
  machine is its own group in Agents, with the chip in its heading, until the user links it
  to a workspace of this machine; then its agents sit in that group (one manager, the same
  intents). Nothing is ever merged by name alone. Settings has "Other machines": each with
  a follow switch, its state in a few words (connecting… until the first attempt ends,
  connected and how many agents, not connected, connected but no Herdr) and ssh's own words when it is down; a name and ssh target to add
  one; and, per workspace over there, a select that links it to a workspace here.
- **Machine alerts.** Settings has "Machine alerts", before "Other machines": CPU and
  Memory, each with a switch by its name, then "≥ [90] % for [5] min", the same for this
  machine and every followed one (defaults 90 % for 5 min, both on). A value alerts once
  every sample of that time is at or above it, so a spike never does, and clears after a
  minute below; while a machine cannot be reached its alerts neither rise nor clear. A
  field is saved when left or on Enter; out of range it goes back. On a narrow screen the
  threshold wraps under the switch as one piece. The thresholds are kept with the list of
  machines in the service's settings, not as events.
- **Opening a terminal.** A small terminal button sits at the end of each agent's line in
  the panel (a click target of its own: the line opens the agent's work, the button its
  terminal) and in the header of running or waiting work. It is not on graph nodes. All
  of it goes through one function in the UI (`runtime.focusAgent`); until the daemon is
  there it only says what it would do.
- **Asking why.** There is no "why" button on every element. Asking happens in the side
  panel's **Ask why** section, as a chat sits beside a pull request's diff: its heading, the
  picked task's conversations, and at the bottom, fixed, the context chips, the box and the
  line saying who answers. With no task picked it says what to pick (a task in the graph, or
  lines in the code); every question is about a task. The picked task's detail keeps only a
  small "Ask why" with how many conversations it has; it, "Ask why" on picked lines and `?`
  open the panel on Ask (the sheet on a narrow screen) with the cursor in the box. What a question is about is the current
  selection, shown as chips above the box: the task always, the picked finding, and the
  picked file – or, when lines are picked, the lines instead of their file. Lines are
  picked in any diff, the single file or the list (click a line number, shift-click to
  stretch); next to them floats "Ask why", like the "+" of a code review. It picks the task
  that last changed those lines (worked out from the unit's commits; other tasks that
  changed some of them come along as chips) and puts the cursor in its box, so the code can
  be asked about with no task picked. A chip can be left out for this question (its ×).
  `?` (outside a field) does the same with picked lines, and otherwise opens the panel on
  Ask; Enter sends, Shift+Enter breaks the line. On a gate – the user's own task – the
  question goes to the agent that asked for it (the agent of the task the gate follows; with
  none, the manager bot), so "step 2's curl fails, what now" reaches whoever wrote the
  steps; its chip is the gate's title. A temporary agent answers
  from workriver's records and the code; it asks the agent that did the work only when
  that agent is idle, never one at work, and the answer says quietly whether it did. While
  it works, the step it is at shows; if none can be started, the turn says why in the
  user's language (the service's words on hover). Questions form conversations on the node:
  question, answer, question, answer, with the box under the last one; follow-ups go to the
  same temporary agent while it lives (it closes after a quiet while, and a later question
  starts a new one with the conversation so far). Under a conversation: "Draft conclusion"
  (its agent writes what was decided, what changes, what stays open; shown at the end, and
  it starts the note of a waiting gate that follows the task) and "End" (its agent closes);
  "New question" (beside the chips) starts another conversation. File paths in an answer
  open the file (the sheet gets out of the way). Each conversation folds to one row
  (its first question in up to two lines, who asked, how many exchanges, answered / waiting
  / failed / ended, when last), and one opens at a time, as its own scroll area of at most
  360 px kept at the latest exchange (it follows a new answer only while the reader is at
  the bottom); the one just asked in, or one still waiting, is open. Past three
  conversations only the latest three show, with "N earlier" above them. The list scrolls;
  the box stays under it.
  Agents ask each other the same way, through workriver (never in each other's terminals):
  their conversations show on the task asked about like the user's, with the asking agent's
  mark before the question, and count in the node's speech-bubble mark; they never appear in
  Needs you. A node with questions carries a small
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
  a double frame with sharp corners for a gate (work only the user can do). Design work is
  drawn as implementing with a compass icon; its output is its design note, and it never
  carries the "no tests run" mark. A gate to confirm a design ("Confirm design") has a
  scale icon; its note is the decision.
- **Which agent** appears only on the agent's mark. Its colour is derived from the name
  and is used nowhere else.

## Findings

A review's output is a list of findings, and they live in the review, not on the graph.
A decision is not a node. Each finding gets one of four outcomes: accepted (work was
handed out for it: one finding may lead to several nodes, several findings to one node),
revised (the same, but fixed the user's way: a note is required and travels to the manager;
shown like accepted with a small pencil), rejected (struck through), or deferred (quiet,
with a clock; it waits in the project's backlog, becomes scheduled with a clock when picked
to be done together, done when the work it caused is delivered, or dropped with a reason,
all quiet); until then it is open. The buttons read Accept, Revise, Reject, Defer;
Revise waits for the note. Every decision goes to the manager of the workspace the intent
runs in.
An agent may decide on its own: such a finding carries a small neutral robot mark, a
decision by the user none. An open finding may carry the agent's suggestion, and its
reason once the row is opened.
Severity (P1–P3) is text, never a colour: the review node says how many it found per
severity, and its list is grouped by it. Only open findings wait for the user: they show
as a violet count on the review node, in the unit's outlined count in the rail, and as a
row of the inbox. Picking a
finding lights the review and the nodes it led to.

Who decides an intent's findings is the user's choice: a small select in the intent's open
group in the rail ("Findings decided by: the manager / only me"; the manager by default).
There is no choice by severity: severity is the reviewer's own rating, so it would only look
like a control. A finding that is the manager's to decide stays open on its review, in quiet
ink with a small crown and "The manager decides", and does not count in the violet marks or
in Needs you: violet means waiting for the user. With no manager running it falls to the
user: it reads "No manager running: yours to decide" in violet and counts. The user can
always decide it, and can change what the manager decided: under a finding with the robot
mark, the details offer the decision again ("Decided by the manager. You can decide
differently").

A unit whose branch was renamed or rebased onto a new one is the same unit: under its name
the header says, quietly and in mono, "previously on <branch>". A task whose commits were
only on a branch the unit left says "Its commits are not on the current branch: <hashes>"
where it would say it has none. A commit that names no task is a gap (an unregistered node);
its detail has a small select, "It belongs to task", that links it by hand; a commit linked
that way says "linked by hand" after its stats.

What is added to a finding after it was reported (a requirement for the fix, a detail) is
never merged into its text: the row shows a small "+n", and its details list each addition
with its author's mark and time. The same list shows with the finding where it is a cause
of other work, since that work must meet it.

A review done blind (its agent saw nothing of other work until it delivered) carries a small
neutral "eye crossed out" mark: at the top left of its node, in its detail header with the
word "Blind", and in its row in Needs you. It is information for weighing agreement between
reviews (two reviews finding the same thing, one of them blind, is stronger than two that
read each other); nothing is matched or merged automatically.

## Gates and chores

Both are things an agent hands to the user; what separates them is whether an agent is
stuck. A **gate** blocks: a manual deploy, a manual try-out. It is a node, placed where
the flow stops, violet while it waits and water once done. It waits only once every task
it follows is done; before that it is drawn like any task not started and counts nowhere
(not in Needs you, the marks, the tab's title or the unit's state), and its age in Needs
you counts from the moment it opened. The work behind it stays not
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
  opens large on a click. Only images in the form. They go into the service's store when
  the result goes out. Submitted, the gate is done, its findings are
  open findings like a review's (so they wait in Needs you) and the work behind it stays
  not started until they are decided. The pictures stay with the gate's output and with
  each finding, wherever it shows, including as the cause of the work it led to.
- **Attachments** from agents (screenshots, reports, logs, any file) show the same way
  wherever they belong: with a task's output, a finding, or a check (next to its line).
  Images preview as thumbnails; any other file is a small chip that downloads it. The store
  keeps every file for good, like the records.
- **An open finding** takes one of four decisions, Accept, Revise, Reject or Defer, with an
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
- A chip (a file, a context, a link, a machine) is one line and never wider than what holds
  it; a list of chips wraps whole chips. A long name in it is shortened in the middle,
  keeping its start and its extension ("2026-11-16-new-acc…source.md"), and cut at the end
  only if even that does not fit; the whole name is in its tooltip and read out by screen
  readers. A long path inside running text (an answer) breaks where it must.
- Pickers (the project switch, who decides findings, a workspace link, the task a commit
  belongs to, the intent of a batch) are Radix Select in our colours, never the system's
  menu: on the rail in the rail's colours, elsewhere on the page's.

## The signature: lines are a river

- Work that is done: a solid water-coloured line.
- Work in progress: the line into it is a row of dots moving forward. Apart from the
  agent's mark this is the only looping motion, and it carries state.
- The graph has a start and no end. A finished branch simply stops; its water colour says
  it is done.
- A line that would cross columns (a secondary input reaching far) runs through the empty
  space: along the gap between rows, then down just before its target.
- With "reduce motion" set, both are still.

## Signed in or not

The page decides and acts as the user, so it works only with a session (`docs/security.md`).
Without one it shows a single calm screen, nothing of the app: the product mark, one
sentence on why, the command `workriver open` with its copy button, and a quiet line that
the link works once. No form, no token field: the link from the user's own terminal is the
only way in. A session that ended shows the same screen at the user's next action.

## States

Everything that can be picked has: default, hover, keyboard focus (violet ring), selected
(violet ring plus shadow). With a node picked, nodes and lines unrelated to it fade.

## Rules that came out of iterating with the user

- Two languages from the start, English and Chinese. Layout must hold in both: English
  labels are longer, Chinese ones are denser.

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
- No loading state. The one empty state: with no units recorded, the main area is a short
  start guide (start a manager agent per Herdr workspace, give it the skill, register, start
  an intent from the roadmap; the commands copyable; the commit trailer; the import skill
  for an existing backlog file). An intent with no units yet keeps its
  rail group, open, with a quiet "No units yet".
- No syntax highlighting in the diff.
- Trackers: intents and findings carry an optional external id (`system`, `key`, `url`),
  shown as a link chip. Later a tracker adapter (a trait like the runtime one) imports items
  into the backlog and pushes status and links back; agents never talk to trackers directly.
