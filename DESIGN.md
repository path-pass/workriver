# workriver design

Direction set on 2026-11-15 following the Operate mode of the `impeccable` skill. The UI
lives in `frontend/web/`.

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
one code unit are a **research unit**: a unit like any other in the rail, with no
repository, branch or code layer, never merged ("Ready to close" where a code unit says
"Ready to merge"; its manager closes it). Before research units such work was recorded on the
intent itself (`i3#1`); those tasks still work, and show on the intent's page as "Earlier
research and design", never in the rail.

Three columns: the rail picks a unit, the main area shows it, the side panel holds what is
looked at now and then.

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
  (workriver's own "workriver-commands" workspace of command shells is left out). With a manager,
  "Start" records the start as the user's (as `workriver intent start` would) and types into
  that manager's terminal "[workriver] The user started i3 «title» in your workspace … split
  it into units and tasks (`workriver guide how start-intent`)", its goal quoted; the dialog
  then says where it started and whether the message was delivered (or why not, in the user's
  language). A workspace without a manager says so and shows the steps to start one, with the
  commands to copy – workriver never starts an agent; a row is moved by dragging its grip, or Alt+↑/↓ on it, within its project; a quiet
  "+" adds one inline, title and one-sentence goal, to the project shown; a quiet "Merge…"
  in the heading, when a project has two planned intents or more, opens a dialog to merge
  them), and **Done**,
  folded; dropped intents are not shown. Settings at the bottom. It is the only large field
  of colour in the interface. Each active or done intent is a group that folds (its chevron),
  on two lines only: its id quietly in mono before its title (as units show `u1`), the title
  cut to one line – a click opens the intent's page, a tooltip shows the title and the
  goal's first sentence –, then one quiet status line: the Herdr workspace it runs in, its
  manager's state, the quiet mark, "Finished". "no manager" shows only on an active intent
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
  intents. The 3 days are fixed for now. Each unit's line carries at most three marks, none drawn at zero: tasks an agent is
  stuck on behind the user (filled violet, a hand, the count), everything else waiting for
  the user – open findings and commits that name no task – (an outlined violet count), and
  "beyond plan" (amber). Before them, where the unit stands as a dot and a word, except
  while it is simply being worked on: waiting, ready to merge (in the done tone, so what
  can be merged stands out; it counts nowhere as attention), merged (with its date) or done
  (why, on hover). The unit header carries the same word. An active intent whose units are
  all merged or done says "Finished" in its header, for its manager to close it. Who is working is not shown
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
  marks), everything else about the intent is on the line under it. The handle is a small hook of ours (`lib/useResize.ts`), written so the side
  panel could use it too: react-resizable-panels measured +15.3 kB gzip for this one
  handle, which is the case the "prefer a library" rule excepts.
- **An intent's page** (`/i3`, `workriver link i3`; its title in the rail opens it): its id
  and title with "copy link", its state, "Finished", its link chip; its goal under it. Then a
  block with its note (short Markdown with links, as an agent's longer text; folded past six
  lines; `workriver intent set i3 --add-note` adds a paragraph) and where it runs: the
  workspace, the manager (name, state, the terminal button, and "Write to the manager",
  which opens the side panel's Ask section on the manager conversation for that workspace),
  and who decides its findings (the select that was in the rail). A planned intent says it
  is not started yet and has "Start" in its header (the same dialog). Then its units as a
  list, each a row that opens it: id, title, where it stands, its violet marks, quiet, and
  when it last moved ("moved 2d ago", the date on hover). Last, when it has any, "Earlier
  research and design": its own tasks from before research units, as a graph with the same
  detail under it as a unit's, so `/i3/1` opens there with the task picked.
- **Merging planned intents** is a dialog, not a mode of the rail: the planned intents of the
  project shown (with all shown and several to merge, a select picks the project) as a list
  of checkboxes, each with its title, its goal on one line and its note folded; one that
  holds something (tasks, units, backlog findings, chores) is shown but cannot be ticked, and
  says why. With two or more ticked: which one stays (a select among them), its title
  (prefilled from it and following it until the user writes one), and a line saying what
  happens: the others are dropped and what they said goes into its note. Merge and Cancel;
  the service's refusal shows in the dialog. Escape closes it and focus goes back to "Merge…".
- A **side panel** on the right holds what is not looked at all the time: **Needs you**,
  **Agents**, **Commits**, **Backlog**, **Machines**, **Run a command** and **Ask why**, one at a time, never mixed. Its buttons work as a
  switch: each shows its section, the one already shown folds the panel again; the shown
  one looks pressed. Folded, it is a thin strip with the buttons (no counts: the rail
  already has them); open, it is 340 px wide by default and its left edge drags it between 280 and 720 px (remembered; double-click for the default; arrow keys too). Which section is open, if any, is
  remembered. The panel spans every intent. A row in it opens its unit and task in the
  main area and the panel stays open; a row names its unit (the inbox also its intent,
  quietly). On a narrow screen the same content is a sheet over the page, opened from the
  same buttons in the unit header, with the same switch inside.
- **Run a command** is a small box: "Where" (a select of the active units in the project
  shown, the unit on screen first – its work tree is the directory – then the project
  folders), the command (one line, monospace) and "Run", which opens the confirm dialog below.
- **Running a command as the user.** Where an agent may not do something (a hook blocks its
  push), the manager makes it a gate step with the command. Next to the copy button of every
  command in a gate's or chore's steps sits "Run" (a play icon). It, and the side panel's box,
  open one dialog: the exact command in monospace, its directory (the unit's work tree, else
  its repository; a project's folder), the machine (the unit's), and who wrote it – "you", or
  "agent X, an agent: read it before you run it" in amber, since an agent's text is not to be
  trusted. A command that can destroy work for good (`rm -r`/`-f`, `push --force`/`-f`/`+ref`,
  `reset --hard`, `clean -f…`, `branch -D`, `mkfs`, `dd of=`) is flagged in red: the box red,
  a line saying what it can do, the Run button the destructive one. A heuristic for the
  obvious ones, not a guard. Run and Cancel; nothing runs before Run. Then workriver types it
  into a new tab of a plain shell (no agent) in a Herdr workspace named "workriver-commands" on the
  unit's machine (made on first use, reused after) as `cd -- '<dir>' && eval '<command>';
  echo "__workriver_end_<id>_$?__"`: a tab per run, so a run never types into a shell an
  earlier one still holds (a password prompt, a pager) and each output starts clean. It is the
  user's own shell, so their credentials and hooks apply exactly as when they type it. The
  dialog then shows what the tab shows (read every second; the echo of the typed line left
  out), "Running…" in amber, then "Done (exit 0)" in the done tone or "Failed (exit N)" in red
  once the marker line appears (the typed line carries `$?`, not digits, so its echo never
  counts; the id is random, so the command's own output cannot fake it); "Open in terminal"
  brings that tab to the front. A tab that cannot be read for five seconds (closed, or its
  shell exited) says so. The marker needs a POSIX shell (zsh, bash, sh).
- **Commits** is every unit's commits as a git graph, newest first, like `git log --graph`
  or GitHub's network view: the base on the left as a plain neutral line labelled at its
  top with the base branch's name (its own commits are not drawn), one lane per unit branch
  in the unit's own tone (`--lane-0` … `--lane-5`, picked from its id; see the lane colours
  below), leaving the base at the
  unit's first commit and, merged, joining it again where it was merged (the unit's
  `merged` time when known, else at its last commit); a unit closed without merging just
  ends. A lane is used again once its unit has ended, so the graph is as wide as the most
  units running side by side. One graph per repository, under its name when there are
  several. A row: the dot on its lane, the subject, quietly the unit, the task (or the red
  mark of a commit that names none) and its +/− lines, and how long ago (the date on
  hover). On top, while any agent has uncommitted work, "Writing now": the files each such
  agent is writing and its unit and task, as a hollow dashed dot on its unit's lane, dashed
  down to the unit's newest commit; the agent's mark with its amber ring is in the last
  column, where a commit's age is, so it never sits on the lanes. A row opens its unit and task and brings that
  commit into view in the code layer; it stays picked. Drawn as one small inline SVG per
  row (rows of a fixed 44 px touch, so the lines run on; 2 px lines on whole pixels stay
  crisp at 1x and 2x); no graph library.
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
  "25.4 / 32 GB"). Lines are neutral; a metric in alert takes the colour of its level,
  amber (`--amber`, `--amber-ink` on `--amber-bg`) for a warning and red (`--del` on
  `--del-row`) for critical, the same as the pressure word: a machine's state is the
  system's rating, not work waiting for you, so it is never violet. A pill per alert in the
  machine's header, critical first, says which, its value and for how long ("CPU 97 % ·
  6m", "Compressor 66 % · 2m", "Disk / 86 % used, 14 GB free · 40m"). A machine that cannot be reached keeps its last values, faded, with the
  dashed chip and "disconnected since …". Memory used is total minus available, the same
  on every machine: one small sh probe reads them all through the runner git uses, here
  or over the machine's ssh connection, and nothing is installed anywhere.
  Under the two lines, a third, "24 h": CPU (neutral) and memory (water) over the last 24
  hours in one small inline SVG, one point a minute, broken where the machine was away;
  read when the card shows and every minute after. Under that, small key–value pairs of
  the newest reading, only those the machine's system has, wrapping: Available (total −
  used, on Linux MemAvailable); Pressure, on macOS the kernel's level as a word, normal
  green (`--add`), warning amber (`--amber-ink`), critical red (`--del`), on Linux PSI's
  "some 0.59 % · full 0.12 %" (the last 10 s), or "unknown" without PSI; System free
  (macOS, the kernel's free share, `memory_pressure`'s number); Compressed and Compressor
  ("66 % of its limit": segments or compressed pages over the kernel's limit, whichever is
  higher; at 100 % macOS is out of compressor space) (macOS); OOM kills (Linux: since boot
  · while watched); Swap used / total (left out without swap); Disk free of the home
  folder's filesystem, and of `/` by its mount point when that is another one. Sizes in
  MB, GB or TB (steps of 1024). A reading in alert takes its level's colour (a
  filesystem by its own level). The same probe reads it all (one round trip per sample); a piece a kernel lacks is a
  field without a value, never a failed reading.
  **History**: each machine's samples are folded into one line a minute (the mean of each
  reading, the worst pressure level, counters and disks at the minute's end) appended to
  `<data dir>/metrics/<label>.jsonl` (0600), kept 7 days: at most once an hour the file
  is rewritten without older lines (one file, not one per day, so a look back is one
  read). About 0.5 MB per machine per day. Not in the event store, and never read back
  into the 5 s samples alerts judge. `GET /api/metrics/history?machine=&hours=` (1–168)
  reads it, with the access of `GET /api/metrics`.
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
     workers that stopped without delivering: a running task whose agent sat idle past the
     threshold in Settings (10 min by default), was reminded of its task in its terminal, and
     is still idle half that again later, or whose agent is gone: "u1#3: ja-impl stopped
     12 min ago without delivering" (or "is gone"), quietly when it was reminded, the unit;
     the row opens the task, a terminal button of its own opens the terminal; it leaves by
     itself once the agent works again or the task is delivered or dropped. Then
     gates waiting for the user; an agent cannot go on. Violet, the strongest rows. Both
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
  4. **Gaps** – commits that name no node, branches that grew beyond the original plan.
  5. **Not urgent** – chores, quietest; a row opens its steps. A chore of no intent names
     its project where others name their intent, or nothing when it has none. Then, for an
     intent with no manager, ready units that look squash-merged or have been ready for
     long ("u2 looks squash-merged into main"); the row opens the unit.
  Within a group the oldest comes first. A row of Blocking, Decisions or Gaps opens the
  unit at the node (a review at its first open finding). With nothing to show, one short line says so.
- **Agents** are not in a unit, because a session crosses units. They are a tree, as a list
  (indentation and thin guide lines, no cards; it reads at 340 px and up to 720 px), by the
  Herdr workspace each runs in. A group's heading is the workspace's label and, quietly, the
  title of its current intent (or "no manager"); groups with someone working come first,
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
- The main area shows the picked unit: its name and one-line purpose, nothing else (the
  graph already shows what waits: violet gates, violet counts on reviews), then two
  layers stacked top to bottom, each one block:
  1. **Activity** – the flow of work: how the nodes changed. Its key (what the colours,
     marks and lines mean) is a row under the graph's canvas, wrapping as it needs, never
     over it: no node can hide behind it. Only React Flow's credit stays inside the canvas,
     in its own bottom-right corner, with room left for it when the graph is fitted; room is
     left too for the zoom buttons at the top right, so they never cover the last node.
     **Where the view settles**: the whole graph, centred, when it fits at a readable zoom
     (0.75 or more, never above 1: below 0.75 a card's 12 px title drops under 9 px). When it
     does not (a long unit), the view settles on the latest work at 0.75 instead of the
     centre: the graph's right end against the right edge, moved only as far as it takes to
     show where the running work and waiting gates start, never past the graph's own ends;
     at the top when the height fits, else centred on that work. The rest is a pan away, and
     every node stays drawn. The view settles again when the graph grows (a new task, another
     shape; not on a refresh that changes neither), when the room changes (the rail or side
     panel, the window), and with the fit button – but never once the user has moved the view
     (dragged, pinched, zoomed with the buttons, used the overview): it is theirs until they
     pick another unit or press the fit button. While the graph does not fit, React Flow's
     overview (MiniMap) sits in the bottom-left corner, 240 × 48 px, under the graph (the
     canvas takes the zoomed graph's height plus room for it): the nodes in their state
     colours (done water, running amber, a waiting gate violet, the rest neutral), the part
     in view clear and the rest shaded; a drag or a click there moves the view. It is hidden
     when the whole graph fits, where it would only repeat it.
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
  Longer text written by agents or the user – a task's input, a delivery or gate note, an
  answer in Ask why – is short Markdown, rendered compact by the same safe renderer as
  Markdown attachments (see Attachments): the surrounding size, headings no bigger than the
  text, tight lists, and a single line break kept as one. In the detail a long one shows its
  first six lines fading out, with "Show more"; until the renderer has loaded, the plain
  text with its line breaks. Shorter text (a finding's problem, impact and reason, a
  decision's note, a question, a chore's or gate's step, a reason for dropping) is plain,
  its line breaks kept. Agents are told to write it so (a line or two of what, then bullets).
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
- **Stopped workers.** Settings has "Stopped workers", before "Machine alerts": one sentence
  with the minutes (1 to 240, default 10) an agent may sit idle on a running task before it
  is reminded; half that again later the manager is told and the row shows in Needs you.
  Kept in the service's settings with the alert thresholds; the user's alone.
- **Machine alerts.** Settings has "Machine alerts", before "Other machines", the same for
  this machine and every followed one, each applying where the machine's system has the
  value, in three groups. Every machine: CPU and Memory, "≥ [90] % for [5] min" (one
  level, warning), and Disk, "warn over [80] % used or under [50] GB free, critical over
  [90] % or under [20] GB, for [1] min" (each filesystem listed; the worst one alerts).
  macOS: Compressor (warn over 60 %, critical over 80 %, at once), Swap (4 / 10 GB), the
  kernel's Pressure level (warning warns, critical is critical), System free (under 25 /
  20 %). Linux: Available (MemAvailable / MemTotal under 25 / 12 %), Memory stall (PSI
  `full` avg10 over 5 / 20 %: all tasks stalled, real lost work; `some` rises with one
  task paging), OOM kills (any new kill is critical at once, held 5 min after the last).
  Each has a switch by its name and one sentence with its fields. The duration of the
  memory and disk alerts defaults to a minute: a spike never alerts, and a leak that fills
  a Mac's compressor within minutes is still caught early. A level rises once every
  sample of that time is past it (the compressor and OOM kills on the sample that sees
  them), steps down from critical, or clears, after a minute back; while a machine cannot
  be reached its alerts neither rise nor clear. A field is saved when left or on Enter;
  out of range, or crossing its other level, it goes back; a refusal shows and brings back
  the service's values. On a narrow screen the sentence wraps under the switch. The
  thresholds are kept with the list of machines in the service's settings, not as events;
  a settings file without the newer keys takes their defaults.
- **Opening a terminal.** A small terminal button sits at the end of each agent's line in
  the panel (a click target of its own: the line opens the agent's work, the button its
  terminal) and in the header of running or waiting work. It is not on graph nodes. All
  of it goes through one function in the UI (`runtime.focusAgent`); until the daemon is
  there it only says what it would do.
- **Asking why.** There is no "why" button on every element. Asking happens in the side
  panel's **Ask why** section, as a chat sits beside a pull request's diff: its heading, the
  picked task's conversations, and at the bottom, fixed, the context chips, the box and the
  line saying who answers. With no task picked the questions are about the unit as a whole
  (its chip is the unit's title): a temporary agent answers from the intent's goal, the
  unit's purpose, its tasks and findings in short, and the code on its branch, and asks none
  of the unit's agents; the unit's conversations show there. Only the user asks about a
  unit; agents' `workriver ask` names a task. The intent's own work, with no task picked,
  says what to pick. The picked task's detail keeps only a
  small "Ask why" with how many conversations it has; it, "Ask why" on picked lines and `?`
  open the panel on Ask (the sheet on a narrow screen) with the cursor in the box. What a question is about is the current
  selection, shown as chips above the box: the task always, the picked finding, and the
  picked file – or, when lines are picked, the lines instead of their file – and the
  attachment open in the viewer (see Attachments). A single file shows the whole file, or
  with "Only the changes" just its changes with three lines around each (remembered); the
  line-number gutter widens to the file's longest number. Lines are
  picked in any diff, the single file or the list (click a line number, shift-click to
  stretch); next to them floats "Ask why", like the "+" of a code review. It picks the task
  that last changed those lines (worked out from the unit's commits; other tasks that
  changed some of them come along as chips) and puts the cursor in its box, so the code can
  be asked about with no task picked; lines no task changed are asked about the unit (the
  task is unpicked). A chip can be left out for this question (its ×).
  `?` (outside a field) does the same with picked lines, and otherwise opens the panel on
  Ask; Enter sends, Shift+Enter breaks the line. On a gate – the user's own task – the
  question goes to the agent that asked for it (the agent of the task the gate follows; with
  none, the manager bot), so "step 2's curl fails, what now" reaches whoever wrote the
  steps; its chip is the gate's title. A temporary agent answers
  from workriver's records and the code; it asks the agent that did the work only when
  that agent is idle, never one at work, and the answer says quietly whether it did. While
  it works, the step it is at shows; if none can be started, the turn says why in the
  user's language (the service's words on hover). An agent that waits for a person in its
  terminal (a harness's first-run prompt in a folder it has not seen, such as "trust this
  folder", or a permission prompt) is never typed over and nothing answers the prompt for
  the user: the question waits, the turn says so in violet ("Waiting for you in Herdr: …
  needs an answer in its terminal") with "Go to the terminal", the agent is a Blocking row
  in Needs you like any waiting agent, and the question goes in by itself once the agent is
  free; still waiting at the question's deadline, it fails saying so. The same holds for
  follow-ups and for an agent's question to another, which also goes to a temporary agent
  (never straight to the task's own agent, even an idle one). A new agent the runtime does
  not take a prompt for yet (still launching) is tried again at each poll (2 s) for up to
  60 s; still not ready then, it may sit on a prompt the runtime does not report, so it is
  shown and held exactly like a waiting one above, and the question goes in once it is
  idle. Never ready by the deadline, the question fails saying so, and its tab is left open
  for the user (only a start that failed, or an agent that never appeared, closes it). Questions form conversations on the node:
  question, answer, question, answer, with the box under the last one; follow-ups go to the
  same temporary agent while it lives (it closes after a quiet while, and a later question
  starts a new one with the conversation so far). Under a conversation: "Draft conclusion"
  (its agent writes what was decided, what changes, what stays open; shown at the end, and
  it starts the note of a waiting gate that follows the task) and "End" (its agent closes);
  "New question" (beside the chips) starts another conversation. File paths in an answer
  open the file (the sheet gets out of the way). Each conversation folds to one row
  (its first question on one line, who asked, how many exchanges, answered / waiting
  / failed / ended, when last), and one opens at a time: it takes all the height left above
  the box (at least 12rem), as its own scroll area (a log for screen readers) kept at the
  latest exchange (it follows a new answer only while the reader is at the bottom); the one
  just asked in, one still waiting, or else the latest, is open. It reads as a chat: the
  question on the right on the sunken ground, with who asked ("You", or the asking agent's
  mark and name) and when; the answer on the left in a thin frame, rendered as Markdown,
  with who answered and when. Past three conversations only the latest three show, with
  "N earlier" above them. The box stays under them.
  **The manager.** A two-way switch in the section's heading, "Why | Manager"
  (remembered; a request to ask why switches back), opens a conversation with a workspace's
  manager in the same place, rather than a seventh side section: it is a conversation like
  the others, with the same box. The workspace is the open unit's intent's, or picked from
  those with a manager (a select, when there are several). What the user writes is typed
  into the manager's terminal ("The user writes to you from workriver's page (message m3) …
  Reply with `workriver reply m3 …`", the text quoted as the user's); a manager waiting for a
  person in its terminal is never typed over: the message waits, says so in violet with "Go
  to the terminal", and goes in once it is free. Under each message where it is (sending,
  waiting, in the manager's terminal, or not delivered and why); the manager's replies
  (only that manager may reply) follow it on the left with its crown. With no manager
  running in the workspace the box is disabled and says so.
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

One colour role is not a state: **lane colours** (`--lane-0` … `--lane-5`) only tell the
branches apart in the commit graph. They take hues none of the states use (blue, rose and
olive, each dark and light; no water, amber or violet), have light and dark values like
every token, and appear nowhere else.

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

Who decides an intent's findings is the user's choice: a small select on the intent's page
("Findings: Auto / Manual": Auto, the manager decides; Manual, the user does; Auto by default).
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

A ready unit whose every change is already in its base (or the base's upstream) was most
likely merged by squash, which git cannot show: next to "Ready to merge" the header says
"Looks squash-merged" (on hover: where, and the commit that brought it), then the command
that records it, `workriver unit merged u1`, with a copy button. Its manager is told once per
branch tip; a unit ready for longer than the ready period (`readyHours` in the service's
settings.json, 24 by default) has its manager asked once to check its pull request. With no
manager, either is a row in Needs you under "Not urgent" that opens the unit.

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
the graph. Its input is the steps the agent prepared, each command with a copy button and
"Run" (see Running a command as the user);
its output is a short note and, for a try-out that went wrong, findings like a review's.
A **chore** blocks nobody (clean the disk, rotate a key) and is never on the graph or
attached to a task. It belongs to an intent (sometimes to one of its units): it is the
last, quietest group of the inbox, opening the same list of steps (with "Run" too; a chore
of no unit runs in its project's folder) and a "Done" that takes
it off the list. It is not counted in anything violet and goes to no one.

A task **no longer needed** (a gate asked again after the branch changed) is dropped,
never deleted. Its node stays where it was, neutral: a dashed frame, grey text struck
through, and a small "no longer needed" mark (a ban sign) where the untested mark would be.
It holds nothing up and counts nowhere. Its details open with who marked it and when, then
the reason as written. Work not done yet has a quiet "No longer needed…" at the foot of its
details; it opens a reason field (required) with "Mark no longer needed" and "Cancel". The
service tells the agent on it to stop, and the manager.

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
  Images preview as thumbnails; any other file is a small chip. Pictures, Markdown and plain
  text (logs, JSON, YAML, CSV…) open in the **viewer**; any other file (and SVG) downloads.
  The store keeps every file for good, like the records.
  The viewer shows one place's previewable attachments one at a time, in their order: the
  name (mono) as its title, "2 / 5" with previous / next (← and → too, wrapping round; the
  position and name are read out), for a picture the zoom toggle and "open original",
  "Ask why", and a download button for every file. Markdown is rendered (tables, code,
  links) by react-markdown with remark-gfm, loaded only when one is shown (49 kB gzip,
  against 24 kB for marked + DOMPurify: chosen because it never puts an HTML string into
  the page, and the text is an agent's): raw HTML is dropped, links open in a new tab with
  `noopener noreferrer`, and images in it are not fetched but shown as links. Plain text is
  monospace (whole JSON indented). Markdown is rendered up to its first 256 kB (about 1 s to
  render, measured), plain text up to 1 MB; past that a line says how much is shown.
  It is not modal: a large panel over the rail and the main area that stops at the side
  panel (open or folded), so the side panel stays in use; a click outside leaves it open, and
  Escape closes it unless typed in a field outside it. Focus starts in the file (↑ ↓ scroll)
  and goes back to the thumbnail or chip on close. On a narrow screen it covers the page,
  and its "Ask why" opens the Ask sheet over it; closing the sheet comes back to it.
  While it shows a stored attachment, the ask box carries it as a chip (the attachment's
  name) next to the task's; the question then points at it, and the temporary agent is given
  its path in the store (a link through the tunnel on another machine). A question may point
  only at an attachment shown with its task (its output, checks, findings and what was added
  to them, and the findings it answers – but not those for a blind task not yet delivered);
  the service refuses any other.
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

## States

Everything that can be picked has: default, hover, keyboard focus (violet ring), selected
(violet ring plus shadow). With a node picked, nodes and lines unrelated to it fade.

## Loading

The page must stay quick with twenty agents committing, so it reads in two parts.

- **The snapshot** (`/api/workspace`) is everything the rail, the graph and the side panel
  show, and no file text: each commit's files by path, how each changed (A, M, D) and its
  +/− lines (counted once by the service, as the diff package counts them), the uncommitted
  files the same way, and the repositories' trees (small: about 12 kB for this repository).
  Texts were nearly all of it: 8.7 MB with them, about 250 kB without.
- **A unit's code** (`/api/unit/{id}/code`: every text of its commits and of its uncommitted
  work) is read when its code layer is shown, and again only when the unit's `code` in the
  snapshot changes (a new commit, other uncommitted texts); then only the commits not read
  before (a commit never changes) and the uncommitted work come. The graph does not wait
  for it; the code layer appears once its texts are there. Read like the snapshot, so a
  blind task gets none of what its view hides.
- **Refreshing.** Each "changed" from the service reads the snapshot again, at most one read
  at a time: changes told during a read make one more read after it. A hidden tab reads
  nothing and reads once when it is shown again. A snapshot that leaves the unit on screen
  as it was keeps it, so its graph and code are not worked out and drawn again.
- The built files carry their content's hash in their names and are kept by the browser for
  good; the page itself is asked for every time. Nothing is compressed: on 127.0.0.1 gzip
  costs more time than the bytes it saves.

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
- No loading state (the code layer is simply not there until its texts are read). The one empty state: with no units recorded, the main area is a short
  start guide (start a manager agent per Herdr workspace, give it the skill, register, start
  an intent from the roadmap; the commands copyable; the commit trailer; the import skill
  for an existing backlog file). An intent with no units yet keeps its
  rail group, open, with a quiet "No units yet".
- No syntax highlighting in the diff.
- Trackers: intents and findings carry an optional external id (`system`, `key`, `url`),
  shown as a link chip. Later a tracker adapter (a trait like the runtime one) imports items
  into the backlog and pushes status and links back; agents never talk to trackers directly.
