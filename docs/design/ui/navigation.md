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
  none, and sets the address; the unit on screen stays when it belongs to the project picked;
  the Manager section follows to the project's manager, `conversations.md`),
  search (unit ids, titles and purposes, and intent ids: an intent's id shows it with all its
  units), then four levels: **project → workspace → intent → unit**. Each project is a
  section under a quiet heading (its title with "All projects"; "Workspaces" with one project
  picked), with a quiet "+" that adds a workspace to it (a small popover: its name, which is
  only shown and may repeat). Under it each **open workspace** of the project is a group that
  folds (its chevron; open unless the user folded it, and the open unit's opens again), the
  only level whose group folds its intents: its name with its id quietly in mono (`chat ws1`;
  the id is what tells two workspaces of one name apart), and one quiet status line, its
  manager: a crown, the manager's name, then working, idle, not running, or "no manager";
  amber while the workspace has an active intent and no manager running (its findings reach
  no one). A "…" button at the end of its line opens its menu: Rename (a dialog with the
  name), Manager… (the manager select, below), Close (the service refuses it while an intent
  is active there; its words show in the rail). Under the workspace its **active intents**
  with their units. A workspace shows with nothing active in it too (a search or a filter
  leaves out the ones with no match); a project with no open workspace says so and keeps its
  "+". A closed workspace is not in the rail (Settings → Workspaces lists them,
  `operations.md`); an active intent whose workspace is not an open one of its project shows
  under a quiet "In no workspace". After its workspaces the project's **Roadmap** (planned
  intents are in no workspace; the order is the project's: each with its id and title – a
  click opens its page –, a link chip when it is
  tracked elsewhere, and "Start", which opens a dialog to pick
  the workspace it starts in: a select of the **open workspaces of the intent's project**,
  each "name id · manager: state" or "· no manager"; the only one, else the one last started
  in for that project (remembered in the browser), else the first whose manager runs, is
  preselected. An intent runs in a workspace of its project, so nothing of another project
  is offered and nothing is asked twice (an intent of no project is offered every open
  workspace, each saying its project; it takes that project). With a manager running,
  "Start" records the start as the user's (as `workriver intent start` would) and types into
  that manager's terminal "[workriver] The user started i3 «title» in your workspace … split
  it into units and tasks (`workriver guide how start-intent`)", its goal quoted; the dialog
  then says where it started and whether the message was delivered (or why not, in the user's
  language). A workspace whose manager does not run shows the manager select right in the
  dialog (seat one, then start), and "Start without a manager" beside it: a workspace needs
  no running terminal, and the start is the user's either way. A project with no open
  workspace offers only a new one (a name and "New workspace") – workriver
  never starts an agent; a row is moved by dragging its grip, or Alt+↑/↓ on it, within its project; a quiet
  "+" adds one inline, title and one-sentence goal, to that project; a quiet "Merge…"
  in the heading, when the project has two planned intents or more, opens a dialog to merge
  them), and **Done**,
  folded, each done intent naming the workspace it ran in (name and id) on its status line;
  dropped intents are not shown. After the projects, **No project** holds the planned
  intents that name no registered project (a repository is part of a registered project or
  of none), with "Start" and no "+"; it is not drawn when it is empty, nor with one project
  picked. Settings at the bottom. It is the only large field
  of colour in the interface. Each active or done intent is a group that folds (its chevron),
  on two lines only: its id quietly in mono before its title (as units show `u1`), the title
  cut to one line – a click opens the intent's page, a tooltip shows the title and the
  goal's first sentence –, then one quiet status line: its kind and size, the quiet mark,
  "Ready to close" (a done one: the workspace it ran in). Its manager is not on it: that line
  is its workspace's, said once for every intent there.
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
  here, with one exception: each workspace's status line names its manager and its state
  quietly (a crown, the name, then working, idle, not running, or "no manager"; amber when
  work is active there and no manager runs), since a
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
- **The manager select** is how a workspace gets its manager, and the only way: the user
  seats one; no agent makes itself or another a manager (`docs/security.md`). It opens from a
  workspace's menu in the rail, its Needs you row, Settings → Workspaces, an intent's page,
  the Manager section of a workspace nobody would read, and it is part of Start's dialog. A
  dialog titled with the workspace's name and id, its project in the lead, then the seat as
  it is ("Manager now: … seated in <folder>", "Seated: … not running", "No manager is
  seated."), then the agents running now as radio rows (a question's temporary agent and the
  command shells' reviewer are left out). By default only those of the workspace's project:
  started in its folder or inside it, on the machine workriver runs on (where a project's
  folder is), and the agent seated now and the one preset wherever they run; below them, in
  the list, a quiet switch, "Show all agents (5)" (every row's count), lists the others
  under it, so they show where it was turned on. Hiding them again unpicks one picked among them. With none in the project the
  dialog says so ("No agent runs in this project's folder; all agents running now are
  listed.") and the switch shows open, and stays so. Who may be seated does not change with
  the list. Each row: the agent's mark and name, its harness, its machine chip, its state;
  the folder it was started in, short (inside a registered project on this machine as the
  project's name and the path under it, "Web/crates/core", just the name at its root, the
  deepest project holding it; else under a home folder, "~/code/x"; else whole), mono, on
  one line, cut with an ellipsis and whole in its tooltip; every row, its highlight and its state word stay inside the dialog
  at any window width (a dialog and a radio group are one column that may shrink below its
  content, so no long line widens them); what the records know of it ("manager
  of ws2", "2 tasks (u1#3, …)", or "no task, no seat") and, quietly, where its terminal is in
  the runtime ("in Herdr: chat"), which is a hint and decides nothing. Those with no task
  come first. "Seat as manager" seats the one picked; picking one while another manager runs
  there asks once, in a line ("… runs and is the manager now. Seat … instead? … is told it no
  longer manages this workspace."), and the button then reads "Yes, seat …". "Empty the seat"
  takes the manager away. Every change names the seat the user was shown (its session and
  appointment); when the seat changed meanwhile the service refuses (409) and the dialog
  says so and shows the seat as it is now, to choose again: a seat is never overwritten
  unseen. Any other refusal shows in the service's words (the agent is shown in several
  terminals, its machine cannot be asked). The agent seated is told in its terminal by the
  service. **A proposal**: for a workspace whose seated manager does not run where it was
  seated, the service may name a session to preset (`proposal`): the agent now in the old
  terminal, or the seated session itself shown by another terminal. The second is said as it
  is: "A new terminal reports the session of …; anything can report a session id", with the
  folder and machine it was seated in ("gone") beside where it shows now, and that
  confirming is a new appointment (the earlier one's questions close, what was queued for it
  is not delivered); the button reads "Seat it: a new appointment". (The same session
  showing again where it was seated needs no click: it has its seat back by itself, and the
  conversation says so in one line, `conversations.md`.)
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
  workspace (its name and its id; a "closed" tag when it is; for an active intent whose
  project has another open workspace, a quiet "Move to…" select, which records the move as
  the user's), the manager (name, state, the terminal button, "Manager…" when none runs, and "Write to the manager",
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
     question; the runtime says "blocked", or only its screen shows a question the runtime
     missed: "waiting on its screen", the same row), wherever they run, in orange (`--hold`: the gates'
     lightness and chroma, another hue): the agent's mark and "ja-impl is waiting for you in its terminal", its topic, its intent or workspace, how long it has waited; a click opens
     its gate (below, "Answering a stuck agent"), its terminal button opens the terminal view
     (`operations.md`, Opening a terminal), and the row leaves by itself once it is no longer waiting. Then
     workers that stopped without delivering: a running task whose agent sat idle past the
     threshold in Settings (10 min by default), was reminded of its task in its terminal, and
     is still idle half that again later, or whose agent is gone: "u1#3: ja-impl stopped
     12 min ago without delivering" (or "is gone"), quietly when it was reminded, the unit;
     the row opens the task, a terminal button of its own opens the terminal; it leaves by
     itself once the agent works again or the task is delivered or dropped. Then
     gates waiting for the user; an agent cannot go on. Violet. Rows with a background stand 2 px apart. Both
     count in the filled violet marks (the intent's in the rail, the folded rail's), the
     tab's title starts with their number ("(2) workriver"), and, if the user turned it on in
     Settings, a newly waiting agent raises a browser notification that opens its gate.
  2. **Managers** – one row per open workspace that has an active intent and no manager
     running: an amber crown, "<workspace>: no manager" or "<workspace>: its manager is not
     running" with the workspace's id in mono, the seated manager's mark and name when there
     is one, and what a click does: seat a manager (nothing proposed), confirm the agent now
     in its terminal or pick another (a proposal), or "A new terminal reports this session:
     confirming is a new appointment" (the proposal is the seated session itself, elsewhere
     than it was seated). The row opens the manager select preset to the proposal, and
     leaves by itself once a manager runs there. It counts in no mark and not in the tab's
     title, and plays no sound.
  3. **Machines** – a machine past an alert threshold: "This machine: Swap 5.2 GB", then
     its level as a word in its colour ("Warning" amber, "Critical" red, the icon too) and
     quietly the threshold ("over 4 GB for 1 min", "over 60 %, at once"), since when, and
     on a third line the five processes using the most memory when it rose or last rose a
     level, each with the agent (and workspace) it descends from ("claude 6.1 GB (reviewer
     · chat), …"). An alert that rises from warning to critical stays the same row. It
     counts in no mark and not in the tab's title (those count work stuck on the user); a
     newly raised one sends a browser notification under the same switch, and one that
     becomes critical sends another; never again while it stays. The row opens the
     Machines section and leaves by itself once the alert clears.
  4. **Decisions** – one row per review or gate with open findings, with the open ones
     per severity and how many carry an agent's suggestion; the highest severity first.
  5. **Messages** – what was said to the user and not read yet, the newest first: one row
     per workspace (by its id: two of one name are two rows) whose manager wrote (its newest
     words, how many new in violet, the manager, the workspace's name and id), which opens
     that workspace's Manager conversation; and one row per task whose
     agent answered in its discussion (`conversations.md`, Discussing a delivered result):
     its newest answer's first line, how many new in plain ink (nothing waits on it), the
     agent and the task, which opens the task at its discussion. They count in no mark and
     not in the tab's title, and leave once read.
  6. **Gaps** – commits that name no node.
  7. **Not urgent** – chores, quietest; a row opens its steps. A chore of no intent names
     its project where others name their intent, or nothing when it has none. Then, for an
     intent with no manager, ready units that look squash-merged or have been ready for
     long ("u2 looks squash-merged into main"); the row opens the unit.
  Within a group the oldest comes first. A row of Blocking, Decisions or Gaps opens the
  unit at the node (a review at its first open finding). With nothing to show, one short line says so.
- **Answering a stuck agent.** The gate of an agent waiting in its terminal opens from its
  Blocking row, from its running task's detail ("Waiting for you in its terminal · Answer"),
  from its blocked line in Agents, and from the browser notification; each names the agent
  by session and machine together (the same resumed session can live on two machines), and
  the gate stays on that one across refreshes. A task's entries (its detail's waiting line
  and terminal button, a stalled worker's row) find its agent by its session (by its name
  when it has none) in its intent's workspace (workriver's id: the records put an agent
  there by its seat or its tasks, never by the runtime's group) and, when the task's unit
  has a repository, on that repository's machine; a unit with none (research, an intent's
  own work) names no machine. When no instance or more than one fits (the same session on
  two machines, a name in two workspaces), they are not shown: never a guess at another. The terminal is not
  shown by default: the gate shows the dialog's text and buttons to answer it. A dialog
  (Radix, modal, up to 960 px wide so 120 columns fit); opening it reads the agent's screen
  (`/api/agent/screen`); a failed read says why, with the keys disabled and "Try again". Top
  to bottom: the agent (mark, name), its task as a link or its workspace, how long it has
  waited, the runtime's reason quietly ("bash_permission_prompt: …"), or for one waiting on
  its screen one quiet line ("The runtime reports it idle; its screen shows a question.");
  the dialog's region as quoted agent text (mono, plain text, never Markdown, links inert):
  its options, its hints, and above them the lines up to the harness's own rule, at most
  twelve (its rule is a solid line drawn from column 0 across the screen; a rule-like line
  anywhere else, as in a command the dialog indents, is text the agent may have written and
  ends nothing, so the agent never decides where the region starts);
  "Whole screen" shows every visible line (shown from the start, with a line why, when the
  twelve lines cut the region: a long command is seen whole before it is approved); the
  options as buttons in their order, each its own words and, as a small key cap, what will
  be sent ("2" for a numbered option, its digit whatever is selected; "↓ ↓ ↵" for a marked
  list, which the service walks one arrow at a time, reading the screen after each, and ends
  with Enter only on the option's words); the hint keys ("Esc · cancel"); last, the generic
  keys ↑ ↓ ↵ Esc Tab 1–9, one key each, for doing it by hand: folded under "Other keys" when
  options were read, open when none could be, and then "Open terminal view" stands out.
  Nothing else can be sent: no text, never a key outside these (an option that is a text
  field is a button like any other; after it the dialog is still there with the selection
  on the field, the gate says so, and such an answer is given in the terminal).
  The region, the whole screen, the option buttons and the recorded line are drawn in the
  order a terminal draws them (`direction: ltr; unicode-bidi: bidi-override`; one shared
  component in `content/`), with the marks that reorder or hide text shown as their code
  point (`⟨U+202E⟩`, the service's). Buttons wait while a press runs. Answered: the gate
  closes and the row leaves by itself. Still open, a new question, sent while the runtime
  still reports it waiting, or something else changed (Enter not sent), or the key may have
  gone (its answer lost; never sent again by itself): the gate shows the new screen and its
  new buttons with one line saying which. Refused or failed: the reason in the user's
  language, the service's words in a tooltip ("The screen changed; look again" with the new
  screen; "It shows in two places; answer it in its terminal"; "It is no longer waiting").
  A press is bound to the screen shown: the service sends nothing unless the whole screen
  still reads the same, and a second click never goes. The dialog the runtime missed
  (Codex's "Hooks need review", "Implement this plan?") shows and counts the same.
- **A sound.** If the user turned it on in Settings ("Play a sound when something new comes
  into Needs you", under the notifications switch, off by default), the page plays one short,
  quiet sound (two soft notes, made with Web Audio; no file) when a row comes into Blocking,
  Machines, Decisions or Messages that was not there before, whether or not the tab is in
  front (a hidden tab reads on each change while the sound is on: presentation.md,
  Refreshing). A new unread word in a conversation already in Messages (a manager's next
  reply, a discussion's next answer) counts as a new row; reading counts as nothing. Gaps and
  Not urgent stay quiet: they block nobody. Not for the rows there when the page loads, not
  for a row that only changes (an alert turning critical), not for the user's own act (a gate
  they passed with a problem, whose findings come into Decisions), and not again within five
  seconds, however many come. All open tabs of the page share what was played: a row plays
  once, not once per tab, even in a tab that sees it later (a coming of a row is remembered
  for ten minutes; the same agent asking again, or a gate waiting again, is a new coming, even
  when no read saw the agent working between), and
  the five seconds hold for every tab. A tab decides right before it plays, and only a tab
  that can play takes the sound, so a tab the browser holds back does not silence one that
  can. Two words written in the same second are two words, whichever messages they answer. Rows that come while it is off are not played later. Turning it on plays it once. The browser
  lets a page play only after a click or a key on it; a sound that cannot play is dropped,
  not played late, and Settings then says so in one quiet line under the switch. Browser
  notifications are separate; they share only the hidden tab's reading.

## Agents

- **Agents** are not in a unit, because a session crosses units. They are a tree, as a list
  (indentation and thin guide lines, no cards; it reads at 340 px and up to 720 px), by the
  workspace the records put each in: the one whose seat it holds, else the one workspace of
  the tasks it has; never by where the runtime shows its terminal. A group's heading is the
  workspace's name and its id in mono (its project in its tooltip) and, quietly, the title of
  its current intent (or "no manager"); groups with someone working come first,
  then by name. Agents with tasks that are in no one workspace come last, under "In no
  workspace". Under it the manager (with a small crown), and indented under the manager
  its workers: those that took a task (recorded on the task, since Herdr does not say who
  started a session), blocked first (a violet hand: the Blocking meaning), then working,
  idle, idle with nothing to do, gone. Where an agent's terminal is in the runtime (its
  Herdr workspace's name) shows quietly after its harness, as a hint. A temporary question agent (`why-qN`) is indented
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
  end opens its terminal view (`operations.md`, Opening a terminal); a blocked line also
  offers its gate. An agent Herdr knows by no name is called by its harness and a
  number within its Herdr workspace ("claude 2"); its mark reads "C2". How long an agent has been
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
