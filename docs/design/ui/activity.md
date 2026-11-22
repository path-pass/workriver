# Activity: the graph, a task's detail, findings, gates and evidence

The main area's first layer (the flow of work) and everything a node carries: its detail,
findings and their decisions, gates and chores, evidence and where the scope grew.

Moved from `DESIGN.md` when it was split into these files, word for word but for paths
made whole and "see …" naming the file; `DESIGN.md` is the overview and says where each of
its sections went.

## The main area

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
     **The canvas's height**: with none set, the graph's own height plus room (plus the
     overview's, while it shows), at most 70% of the window. Its bottom edge drags it, the
     same handle as the rail's and the side panel's edges turned on its side (a 6 px hit area
     with the row-resize cursor, a line in the selection colour when pointed at, focused or
     dragged; a separator for the keyboard, arrows ↑/↓ by 16 px, Home and End to the ends):
     between 160 px and the window's height as it is now. A drag or a key starts from the
     height the canvas shows, which is less than the one set after the window shrinks; a
     click that does not move changes nothing. The height set is one for every unit and is
     remembered (`workriver.flow.height`); a double-click forgets it, back to the graph's
     own height. The view settles again once when a drag ends, not during it.

## A node's detail

- Picking a node shows its input and output directly under the graph, not in the side
  panel, and lines cannot be picked: why a node exists is its input. On work that is
  running or still to do, the header names its agent and, while the session lives, a
  terminal button. Output corrected after delivery shows its latest values, with a quiet
  "amended" and the time next to the Output heading.
  Work an agent started or delivered (any kind but a gate) has a quiet line under the
  header, its **trace**, from the agent recorded with that start or delivery; a task still
  to do has none (its assignee's session is other work's until it starts). The line holds
  the harness as the runtime named it ("harness not known" before the agent was seen) and
  the model the agent gave when it started the task itself ("model not given" when it gave
  none; a start recorded for it by someone else carries none), and nothing else: no copy
  button. The harness's own session id and where the harness keeps the session file are
  recorded, not shown (the user asked for less): the page's JSON carries only the harness
  and the model, and `task show` prints the same two under `Agent:`. Whoever needs the
  file asks for it: `workriver task session u1#3` prints its path, and `task show --json`
  keeps the whole trace for tools (the session, the path, the machine, why there is none).
  The service works the path out only for those (one place, `server/views/trace.rs`;
  Claude Code by the directory the agent was started in, or by its id among its folders
  when it was begun elsewhere; Codex by listing its sessions and archived sessions folders
  for the id; a session not found is looked for again only after a while) and never reads
  the file; the line is there only where the task is (a blind agent sees its own).
  After Output, a **Questions** block holds the past questions about the task (the user's
  from when the page had its own Ask, agents' to each other), read-only, folded to rows (`conversations.md`,
  Past questions); a delivered design or research also has its discussion there, open, with
  a box under it (`conversations.md`, Discussing a delivered result); at the bottom a small
  "Ask the manager" opens the Manager box with the task as its chip. With no node picked, the unit's own past questions show under the graph
  the same way, when it has any.
  Longer text written by agents or the user – a task's input, a delivery or gate note, an
  answer to a question, a manager's reply – is short Markdown, rendered compact by the same safe renderer as
  Markdown attachments (see Attachments, `attachments.md`): the surrounding size, headings no bigger than the
  text, tight lists, and a single line break kept as one. In the detail a long one shows its
  first six lines fading out, with "Show more"; until the renderer has loaded, the plain
  text with its line breaks. Shorter text (a finding's problem, impact and reason, a
  decision's note, a question, a chore's or gate's step, a reason for dropping) is plain,
  its line breaks kept. Agents are told to write it so (a line or two of what, then bullets).
  What the intent's manager or the user added to what a task asks after it was recorded
  (`workriver task note`) is never merged into its text: under that text (a review's: under
  what it reviews), set off by a thin line on the left, each addition oldest first, with its
  author's mark, "<name> added to what it asks" ("You added …" for the user's own, as the
  service marks it from who recorded it, never from the name) and the time, then its text as Markdown like the input's, and its files. A task with none shows
  nothing there.

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
A review node's name gives way before what it found: with counts under it the name stays on
one line and is cut with an ellipsis when too long (アーキテクチャレ…), and the counts keep
their line; without findings the name is cut the same way, and the punctuation after it and
the state ("：問題なし") are never clipped. The full name is in the node's tooltip.

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
"Run" (see Running a command as the user, `operations.md`), and, while it waits, a "Discuss
#1 with agent-R" button for each design or research it asks about that may still be
discussed, which picks that task (`conversations.md`, Discussing a delivered result);
its output is a short note and, for a try-out that went wrong, findings like a review's.
A decide gate may instead ask a **question with options** (a choice gate, see Choices
below): named "Question", answered by picking one; nothing follows from the answer but what
the manager adds after it.
Files attached to the gate (a plan to read, a build to try) show with its output the way a
task's attachments do (`attachments.md`), while it waits too: above the form, to be looked
at before answering it. Submitted, they stay, the form's pictures after them.
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

A task is **never inserted** before another (no `--before`): a task's inputs are written
once, when it is recorded. The one exception is a blocker (`--blocks`), a gate put before the
task still to do or running that an agent is stuck on, never before a gate. A gate asks about the designs and research before it, through the
reviews between; work put before a gate (a design, a review of another design) changes what
the gate asks, so a gate open or already answered could read as confirming a design the user
never saw. An attempt to allow it under checks was given up after four review rounds, each
finding another path to that (through a review a choice gate follows, a choice gate already
answered, a gate after the one inserted before, a review of another design); the branch
`feat/insert-task` keeps it. A recorded gate is dropped and asked anew instead: the new work
after what the gate followed, a new gate after it, and what waited on the old gate added
again after the new one (`ask-user`). The user sees a gate about what is there now, and the
dropped one says why.

## What the user puts in

Everything the user decides goes to the service through one place (`frontend/web/src/api/actions.ts`), which
records it and delivers it to the agents through Herdr.

- **A waiting gate's output is a form**: Done or Problem; a note; with a problem, findings
  with the same three fields as a reviewer's (severity P1–P3, what is wrong, what happens
  if it is left, where), at least one. Pictures can be pasted into the note or a finding,
  dropped on the form, or picked with "Attach"; they show as thumbnails with a ×, and one
  opens large on a click. Only images in the form. They go into the service's store when
  the result goes out, after the files the gate already had. Submitted, the gate is done, its findings are
  open findings like a review's (so they wait in Needs you) and the work behind it stays
  not started until they are decided. The pictures stay with the gate's output and with
  each finding, wherever it shows, including as the cause of the work it led to.
- **A waiting choice gate's form** reads Answer | Problem (Answer first). Under Answer, its
  options as radio buttons (Choices), Other last, none picked at first; then the note and
  its pictures; then "Answer", inactive until an option is picked. An option marked "say
  why" makes the note required ("Why (needed for this answer)"), Other too ("What to do
  instead (needed for Other)"), with the error in place when it is empty.
  Problem is the form above (the question itself is wrong). The undo window and the
  delivery line are as for any gate. Submitted, the output shows "Chose: <the option's
  words>" ("Chose: Other"), then the note and pictures.

- **An open finding** takes one of four decisions, Accept, Revise, Reject or Defer, with an
  optional one-line note; the agent's suggestion, if any, is the filled button. The row
  takes its new state at once and every count drops (the task's badge, Needs you, the rail).
- **Delivery.** A gate's result and a decision then show where their message is: not
  delivered yet (and "Undo" until it goes out), delivered to the agent, picked up by it (it
  started working after the message arrived), or not delivered with the reason in the
  user's language (no manager, manager not running, Herdr not reachable, Herdr refused;
  the service's own words in a tooltip), or held while its receiver is on a blind task in
  progress ("Waiting: … is on a blind task", why in a tooltip): it goes once that task is
  delivered, and a restart in between marks it not delivered (the service restarted), so it
  can be sent again. Starting an intent says the same in its dialog. Every decision goes to the intent's manager agent
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

## Where the scope grew

Along each line of work (the paths through a unit's graph from the start) the folders its
tasks' commits changed add up. A task whose commits reach folders outside everything
changed before it on its line carries a small neutral mark at the bottom middle of its
node: an expand icon and "+N", N the new folders. Its tooltip, its screen-reader text and
the task's output say which folders and beyond which earlier work ("Also reached src/auth/,
docs/ — beyond #1, #3": the tasks before it that widened the scope). Tasks that stay within
the scope so far carry nothing, so the marks show where the scope grew, step by step; it
is information, not a call to act: no colour, no motion, not counted in Needs you.

- Only commits count, by their files' folders: not plans (nothing changed yet), not
  uncommitted work, not a merge that brings the base into the branch (it brings the base's
  work, not the task's; the service marks such a merge `merge`, and a merge of any other
  branch counts: what it brings is the branch's).
- Reviews and gates change no files: never compared, never marked. Tests tasks are compared
  like the rest. A task with nothing changed before it on its line (the first) is not marked.
- Where lines join, "before" is everything before any of them.
- Worked out once per unit per change of its graph's shape or its tasks' commits, in one
  pass over the graph in order, never per render or per node (`frontend/web/src/work/growth.ts`).
- The legend shows the mark with "+" and no number, as it shows the findings count without
  one: a sample, not a count.
- The legend sits below the canvas, folded to one line ("Legend" behind a chevron) until
  clicked: it is read once, then it is noise under every graph. Open or folded is remembered
  per viewer (localStorage; without it, folded). It lists reviews once: the pill and the eye
  together.

## Choices

A question to the user with options carries them as data, whoever asks it: a decide gate
(above) or a manager's message or reply in the conversation (`conversations.md`). One
stored form, one renderer, the same rules.

- **The stored form** is a fixed subset of A2UI v1.0 (a `createSurface` message, version
  `1.0`, the basic catalog): a Column (the one root, `id: "root"`) holding a single-choice
  ChoicePicker (`mutuallyExclusive`, `options: [{label, value}]`, labels literal text, its
  value bound to `/selected`) and a primary Button (`action.event` `answer` with
  `context.selected`, the `required` check on `/selected` as its `@checks`) whose label is a
  Text bound to `/ui/submit`; and beside the surface `needs_note`, the values whose answer
  needs the user's note (A2UI's option items hold a label and a value only). 2 to 8 options;
  a value is 1 to 32 of `[a-z0-9-]`, unique; a label one line of 1 to 200 characters. Agents
  never write it: `--option ID=LABEL` and `--require-note ID` on `task add`, `say` and `reply`
  are built into it by core (`workriver-core/src/choices/`), which also refuses, on every
  path, any choice that is not exactly the form it builds from its own options (another
  component, property, binding, function, catalog or version). A test validates every form
  core builds against the A2UI v1.0 JSON Schemas (`docs/design/guards.md`).
- **Other, on every choice.** workriver ends every choice with Other, so the user can
  always answer outside the options; agents do not add it. It is not in the stored form:
  its value is the reserved `other` (`OTHER` in core and `surface.ts`), which core refuses
  as an asker's option ("workriver adds Other with a note to every choice itself") and
  accepts as the answer to any choice, with a note, which is the user's instruction to the
  agent in place of an option. Its words are the page's (Other, "say what instead"), never
  the asker's. The agent reads the answer as Other with the note quoted after `Other:` (the
  gate's outcome, the answer to a question), and acts on the note within the question. A
  choice recorded before this rule with an option of value `other` keeps it as the asker's
  option (shown once, its own words and note rule): records replay as they were. A design's
  gate and a `--go` gate are not choices and have no Other.
- **The answer** is the chosen values, `selected` (one now; a list so that multiple choice
  needs no new shape), checked by core against the stored values and Other, with a note
  where the option needs one. The page sends the value only, never the label; the label is the
  asker's text, shown plain (no Markdown, no links), and quoted as the asker's in messages.
- **Not on every gate.** A design's gate (after a design, directly or through its reviews)
  takes no options: it is a go, its choices stay in its text, and done confirms the design.
  A blocker takes none, and only the intent's manager or the user asks with options. No task
  hangs after a choice gate until it is answered.
- **A decide gate says which it asks.** `task add --kind gate --gate decide` takes exactly
  one of `--go` (it asks may I: merge, deploy, change the goal, a design's gate; confirmed
  or not) and two or more `--option` (it asks which). Neither, one option, or both is
  refused by the CLI before anything is sent, with a message naming the two (the minimum is
  core's, `MIN_OPTIONS`; the service checks it again for API callers); `--go` on any other gate or
  task is refused too. `--go` is the CLI's alone: what is recorded is as before, a decide
  gate without a choice or with one, so the service, its API and the page do not change.
  The guess that a text lists options (a hint printed after recording) stays for `say` and
  `reply` only: a gate says what it asks.
- **The renderer** is one component, `frontend/web/src/content/choice/Choice.tsx`
  (`ChoiceSurface`, and `Chose` for a pick shown), over the installed Radix (shadcn's
  `radio-group`, no other package): the ChoicePicker as radio buttons and Other last, then what the carrier
  puts before the action (a note, pictures), then the Button, its words the page's
  ("Answer"), inactive with nothing picked. Nothing else of a surface is drawn; one that is
  not the subset's shape shows no options, and no Other (`surface.ts`). The note's words
  follow the pick (`noteWords`): why, for an option that needs it; what to do instead, for
  Other.
- **A2UI's own renderer later.** Turned down for now: `@a2ui/react` with `@a2ui/web_core`
  0.15.0, about 128,588 B gzip for these four components against 10,408 B for the Radix
  radio group, and it rejects the 1.0 spec's `@checks`. When a published package accepts the
  spec, two things change. `Choice.tsx`'s insides: the official renderer, loaded only when a
  choice is shown, as its own chunk (as the mermaid frame's script is fetched only for a
  diagram); each surface namespaced by its carrier (`u2#8`, `m7`, `m3/2`), since every stored
  one is `"choice"`; the `answer` action's `context.selected` handed to the carrier. And each
  carrier (the gate's form, the conversation): the official Column draws the picker and the
  button together, so nothing can go between them; the carrier's note moves after the
  surface, and the carrier holds the `answer` action until a required note is there, showing
  the error in place. Other is then an item the carrier adds last to the surface it hands
  the renderer, in the page's words, never to the stored one. The stored form, core's checks
  and the routes stay as they are.
