# Activity: the graph, a task's detail, findings, gates and evidence

The main area's first layer (the flow of work) and everything a node carries: its detail,
findings and their decisions, gates and chores, evidence and where the scope grew.

Moved from `DESIGN.md` in phase 7b of the restructure, word for word but for paths
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

## A node's detail

- Picking a node shows its input and output directly under the graph, not in the side
  panel, and lines cannot be picked: why a node exists is its input. On work that is
  running or still to do, the header names its agent and, while the session lives, a
  terminal button. Output corrected after delivery shows its latest values, with a quiet
  "amended" and the time next to the Output heading.
  Longer text written by agents or the user – a task's input, a delivery or gate note, an
  answer in Ask why – is short Markdown, rendered compact by the same safe renderer as
  Markdown attachments (see Attachments, `attachments.md`): the surrounding size, headings no bigger than the
  text, tight lists, and a single line break kept as one. In the detail a long one shows its
  first six lines fading out, with "Show more"; until the renderer has loaded, the plain
  text with its line breaks. Shorter text (a finding's problem, impact and reason, a
  decision's note, a question, a chore's or gate's step, a reason for dropping) is plain,
  its line breaks kept. Agents are told to write it so (a line or two of what, then bullets).

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
"Run" (see Running a command as the user, `operations.md`);
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

Everything the user decides goes to the service through one place (`frontend/web/src/api/actions.ts`), which
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
  uncommitted work, not merge commits (the base merged into the branch brings the base's
  work, not the task's; the service marks a commit with more than one parent `merge`).
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
