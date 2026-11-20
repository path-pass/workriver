# Conversations: the manager, past questions, discussing a result, agents asking each other

The page keeps one conversation: the side panel's **Manager** section, with a workspace's
manager. What the user has picked and the files they attach go with the message, as
records, not as text; the manager answers itself, or asks the task's agent (`workriver
ask`) and replies. Past questions (the user's from when the page had its own Ask, agents'
to each other) are read in a task's detail. One more place is the user's: a delivered
design or research is discussed, in its detail, with the agent that did it (Discussing a
delivered result).

**Why one.** The page once had a second entry, "Ask why": each question started a
temporary agent that answered from the records and the code. It was hardly used; the user
wrote to the manager instead. A temporary agent lacks the intent's context, is slow to
start, and was one more place to look. So the page never asks a temporary agent (it never
calls `/api/ask`); its context chips and its files moved, as they were, to the Manager box.
Agents still ask each other that way (below).

The discussion of a delivered result narrows this, for one case. Research or a design came
back, and the user's follow-ups each cost a task, a hand-off, a delivery and a gate, or two
hops through the manager per round with the answers kept apart from the result. Its
answerer is never a temporary agent but the agent that wrote the result and still holds its
context; everything else still goes to the manager.

## The Manager section

- **Where.** One section of the side panel, **Manager** (the speech-bubble button, the
  last), in the same place as the others; no switch in its heading. The workspace is the
  open unit's intent's, or picked from those with a manager (a select, when there are
  several; Needs you's unread messages and an intent's "Write to the manager" open the
  section on a workspace). Its messages read as a chat, at the bottom, fixed: the chips, the
  box, and the line saying where it goes.
- **What the user writes** is typed into the manager's terminal ("The user writes to you
  from workriver's page (message m3) … Reply with `workriver reply m3 …`", what it points
  at in workriver's words, then the user's text quoted as theirs); a manager waiting for a
  person in its terminal is never typed over: the message waits, says so in violet with "Go
  to the terminal", and goes in once it is free. Under each message where it is (sending,
  waiting, in the manager's terminal, or not delivered and why); the manager's replies
  (only that manager may reply) follow it on the left with its crown. With no manager
  running in the workspace the box is disabled and says so. A manager on a blind task in
  progress is not written to: the service refuses a new message until it delivers, and a
  message already waiting for it (held while it waited for a person) waits for that task
  too, saying so quietly under it ("Waiting: … is on a blind task"; the service's words on
  hover), and goes once the task is delivered.
- **What is picked goes with it, as chips** above the box: the unit (its title), the picked
  task (its number, a gate's title, a review's label), the picked finding, the picked file –
  or, when lines are picked, the lines instead of their file – the other tasks that changed
  those lines, and the attachment open in the viewer (`attachments.md`). Only to the open
  unit's intent's workspace: with another workspace picked in the select, no chips show and
  none are sent. Every chip has a ×. Taking out the task's chip takes with it what needs the
  task (the finding, the open attachment); the file or lines and the other tasks that wrote
  them stay with the unit. Taking out the unit's leaves no chip; the user's own files stay. A chip left out stays out until the
  selection changes. A gate is a chip like any task: the manager relays to whoever wrote its
  steps.
- **How the box is reached.** The picked task's detail keeps only a small "Ask the
  manager"; it, "Ask the manager" floating by picked lines, the viewer's "Ask the manager"
  and `?` (outside a field) open the panel on Manager (the sheet on a narrow screen), on the
  unit's own workspace, with the cursor in the box. Lines are picked in any diff, the single
  file or the list (click a line number, shift-click to stretch); next to them floats "Ask
  the manager", like the "+" of a code review. It picks the task that last changed those
  lines (worked out from the unit's commits; other tasks that changed some of them come
  along as chips, so the manager knows who wrote them); lines no task changed go with the
  unit's chip (the task is unpicked). `?` with lines picked does the same; otherwise it only
  brings the cursor. A single file shows the whole file, or with "Only the changes" just its
  changes with three lines around each (remembered); the line-number gutter widens to the
  file's longest number. Enter sends, Shift+Enter breaks the line.
- **Files** go with a message too, any kind (a screenshot, a log, Markdown, a PDF), several
  at once: dropped anywhere on the Manager panel, pasted into the box, or picked with the
  small attach button in the box, before Send; to any workspace's manager (they need no
  unit). The browser never opens a dropped file in the page's place, wherever on the page
  it is let go: outside what takes files (the Manager panel, a gate's form) the cursor says
  it cannot be dropped. The box is a text box first: pasted rich text (a spreadsheet's
  cells, a paragraph) goes in as text and the picture of it that comes along is not
  attached; a picture copied in a browser ("Copy image": html that is only the picture, no
  words) is attached; a pasted file is attached, and text of its own beside it goes in too
  (not text that only names the file). A pasted file with no name is named from its type
  (pasted.png, pasted.txt). Each shows at once as a chip after the others (the attach
  glyph, its name, a × that takes it out). As a gate's pictures, the page holds them until
  the message is sent and they are stored only then, so one taken out is never stored; one
  over the store's 50 MB is refused at once, in a red line above the box naming it, which
  goes with its ×, when a chip is taken out, with the next file attached, or when the
  message is sent. The manager is given each file's path in the store (a command that
  fetches it, for a manager on another machine).
- **Under the sent message** what it pointed at shows as a turn did: its unit or task and
  the other chips, then the files as attachments show everywhere: pictures as thumbnails,
  other files as chips, opening in the viewer.

## Past questions

- **In a task's detail**, after Output, a **Questions** block with how many conversations:
  each folds to one row (its first question on one line, who asked, how many exchanges,
  answered / waiting / failed / ended, when last); one opens at a time, as a chat: the
  question on the right on the sunken ground, with who asked ("You", or the asking agent's
  mark and name) and when, its chips and files; the answer on the left in a thin frame,
  rendered as Markdown, with who answered and when; a conclusion where it was drawn. File
  paths in an answer open the file (the sheet gets out of the way). Past three
  conversations only the latest three show, with "N earlier" above them. Read-only: no box,
  no "Draft conclusion", no "End"; a question still waiting shows how far it has got, and
  its agent waiting for a person in its terminal shows in violet with "Go to the terminal".
- **The unit's own** (questions the user once asked about a whole unit): the same block
  under the graph when no task is picked, only when the unit has any.
- Threads are no longer ended from the page (`/api/thread/end` is gone); a temporary agent
  closes after its idle time (`prompts::IDLE`). The `ThreadEnded` event kind stays, so old
  records replay and old threads still read "ended".

## Discussing a delivered result

The user follows up on a delivered research or design task in place, in rounds; the agent
that did it answers there. No task and no gate per round; a gate only when work must wait on
what was concluded. Not a node on the graph: a node is work handed to someone, and what work
waits on is already a node, the decide gate.

- **Which tasks.** Delivered (`done`) research or design tasks, in a research unit or a code
  unit, with a recorded agent (`Task.agent`), while the task's gate (below) is not
  submitted. Not implement, review or tests tasks (the manager relays, as above), not before
  delivery, not after the gate: what the user confirmed there is what later work follows,
  and a change then is a new design or research task.
- **The task's gate** is one relation, read in core and used by everything below:
  `State::result_gate(owner, n)`, beside `design_gate` in `work/graph.rs`. A design: its
  `design_gate` (the decide gate after it, directly or through its reviews, the latest not
  dropped). Research: the same walk through reviews, taking research. None: no gate (yet).
  The page reads it as the task output's `gate` (the gate's id), never works it out.
- **Where.** In the task's detail, its **Questions** block opens on the discussion (a
  thread with a small neutral "Discussion" tag, never folded behind "N earlier"), the
  discussion's box under it: the Manager
  box (Enter sends, Shift+Enter breaks the line, the user's files pasted, picked or dropped
  on it, as there; no chips: the task is the discussion's), a line saying where it goes, and
  "Draft conclusion" once the result has been discussed (before, there is nothing to
  conclude, and the service refuses it). In **Needs you**, one row per discussion with an unread answer, with
  the messages (`navigation.md`). At the task's gate while it waits, "Discuss #1 with
  agent-R" (`activity.md`).
- **One discussion per task.** The first question opens it (its thread id is that
  question's); every later one joins it. A conclusion does not close it: a question after it
  builds on it.
- **Who writes:** only the user, from the page. **Who answers:** only the task's own agent,
  `workriver answer qN --text "…"`; never a temporary agent, the manager or the user.
  Agents' own questions about the task (`workriver ask`) stay their own threads.
- **How far it got**, under the question: going to the agent (it is typed in once the agent
  is idle), held while it works on another task or is on a blind task in progress ("Waiting
  for agent-R: it goes in once agent-R is free", the service's words on hover), waiting for
  a person in its terminal (violet, "Go to the terminal"), being answered, answered, or "No
  answer will come: …" (its agent is no longer running; its gate was submitted). There is no
  answer deadline: the agent may need to research again.
- **Read.** An answer is unread until the discussion itself has been open on screen a moment
  (measured on it, not on the block around it; not in a hidden tab), then read up to its
  newest answer: `questions.read` with the unit, the
  task and that time, recorded through `POST /api/discuss/read`.
- **Closed.** With its agent gone, or its gate submitted, the box is disabled and its line
  says why ("agent-R is no longer running: ask the manager to go on"; "The gate #2 was
  submitted: the discussion is over …"); the detail's own "Ask the manager" is the way on.
  The thread stays readable.
- **Tied to the result.** The thread holds the rounds; the result is the task's note and
  attachments, and answers are never merged into it. The agent amends its delivery only at
  the conclusion, so the gate, `task show`'s "Designed and found before this task" and every
  later task read one current result, shown "amended".
- **The conclusion.** "Draft conclusion" asks for it (a question with `conclude`). The agent
  amends its delivery first if the result changed, then answers with `--conclusion`: what was
  decided, what changed, what stays open. It shows as a conclusion does, and starts the note
  of a waiting gate that follows the task directly (as a thread's conclusion did). It
  triggers nothing: the gate is still the user's to submit.

**What the asked agent is given** (`prompts::discussion`), one paragraph, the user's words
last:

    [workriver] The user discusses your delivered task u2#1 "Compare stores" (question q7,
    discussion q5). Answer with `workriver answer q7 --text "..."`; the user reads it in
    workriver's page, not in this terminal. Short Markdown, in the language of the question.
    Do not amend your delivery now: when the user asks for the conclusion, you amend it if it
    must change. How: `workriver guide how discussion`. The user says, in workriver: «…»

For the conclusion: amend first if the result must change, then `workriver answer q9
--conclusion`. What it points at (the user's files) is named as for a question.

**What the manager hears** (`delivery::discussed`), never each round, only what it must act
on. It is given through `hand` as a duty (`For::Duty`): held while the manager is on a blind
task in progress, waits for a person in its terminal, is not listed or cannot be sent to,
and given once it can take it; never dropped while the service runs (a restart loses one
still held, as every held notice).

- an amendment of the result while its gate is not submitted, whoever made it: "agent-R
  amended u2#1 … while the user discusses it; its gate u2#3 … is not submitted yet." Then,
  when a review between the result and its gate has started (it read the old text): drop
  the gate, add the review again and a new decide gate after it; one not started yet reads
  the amended result, and the gate asks about that;
- the conclusion: whether the result was amended since the discussion began, what is next
  (the gate waits for the user; amended after its review started: drop the gate, review
  again, a new gate; no gate: one only if work must start on this), and the conclusion
  quoted as the agent's.

The manager keeps the agent of a delivered research or design task while its gate is not
submitted (`closing-workers`), and the idle reminder (`reminders::idle_workers`) does not
count it as idle with no task until then. Asked about such a result in the Manager
conversation, it replies with the task's link instead of relaying (`user-message`).

**In the service** (`server/conversations/discussions.rs`):

- The record hook hands every discussion question, however recorded, to the task's agent
  through `hand` (held kind `For::Discussion`), never `start_question`; it records
  `question.started` (not temporary) when the question is typed in, and no deadline.
- `hand` holds it while the agent works, waits for a person, is on a blind task in
  progress, or is not listed; `poll` gives it once the agent is listed idle, and holds it
  again when the agent is not listed (not listed is not gone).
- Gone (`conversations::gone_from`, each poll): the machine each session was last listed on
  is remembered; a session is gone when that machine's read now is complete (`App::readable`,
  as the idle reminder) without it. Never listed in this run, or its machine not read now:
  unknown, never gone. `poll` then fails its open discussion questions, held or typed in, and
  the page reads its discussion "gone" from the same set; the unit's machine plays no part
  (its author may run on a followed machine).
- A restart keeps open discussion questions (`open_questions` leaves them out; their
  answerer goes on), and gives the ones never given (`open_discussions`). First, given or
  not, one whose result's gate was submitted fails: a run that stopped between recording
  the gate and failing them left it open. A question is checked so again before it is
  given.
- The gate submitted: the open discussion questions of the results it asks about fail
  (`gate-submitted`).

**The record and its rules.** `question.asked` gains `discuss` (`serde(default)`), kept on
`Question` and `Thread`; `questions.read { unit, task, at }` keeps `State::questions_read`,
by `unit#task`. Rules (`prepare_questions`), each a refusal in the service's words:

- a `discuss` question only from the user, about a delivered research or design task with
  an agent, its gate not submitted, with text unless it asks for the conclusion; files only
  as for a question (`State::pointable`);
- an empty thread joins the task's discussion (filled in here), or opens it; a named thread
  that is not the discussion is refused; a question without `discuss` naming a discussion
  thread is refused; `conclude` needs a discussion;
- answered only by the task's agent, and not after its gate is submitted;
- `thread.ended` is refused on a discussion thread, so nothing locks a task's discussion.

## Agents asking each other

- Agents ask each other through workriver (`workriver ask`, never in each other's
  terminals); a temporary agent answers from the records and the code, and asks the task's
  own agent only while that one is idle; it closes after a quiet while (`prompts::IDLE`) and
  a later question starts a new one with the conversation so far. Their conversations show
  in the asked task's Questions block, with the asking agent's mark before the question,
  and count in the node's speech-bubble mark; they never appear in Needs you. A node with
  questions carries a small neutral speech-bubble mark with their count on the graph:
  information, not a button.

## A message's record and its rules

- **The record.** `MessageSent` carries, besides the text, `unit` (empty: none), `task` (0:
  the unit, or none) and `context`, a list of `AskContext` (`finding`, `file`, `lines`,
  `attachment`, and extra `task` chips, which carry the task's number in `task`; a chip's
  label is only the page's). All three are `serde(default)`, so messages recorded without
  them replay. `/api/message` takes the same three; the state's `Message` keeps them; the
  page shows them under the message.
- **Rules** (`MessageSent`, `State::picked`), each a refusal in the service's words:
  - `unit` must be a unit (or an intent's own work) of the message's intent; with no intent
    named, of an intent that runs in the very workspace named (its record, not one of the
    same label on another machine). `task` must be one of the unit's tasks.
  - An attachment the user stored (`Attachment.user`) needs no unit: it may go to any
    workspace's manager, from Needs you, an intent's page or another workspace too.
  - Everything else needs a unit and is refused without one: `task`, `finding`, `file`,
    `lines`, and an attachment that is not the user's, which must be shown with the task
    (with no task: with any task of the unit). The same function decides it for a question
    (`State::pointable`), not a copy.
  - An extra `task` chip must name a task of the message's unit by number; a chip without a
    number is refused.
  - Nothing is written to a manager on an undelivered blind task: refused ("… is on a blind
    task: write once it is delivered"), since nothing about other work is typed into a
    blind agent.
- **Files.** The page stores each one with `POST /api/attachments` when the message is sent
  (the same content once) and names the ids in `context`; not `/api/attachments/inbox`,
  which makes a task's folder for an agent. Until sent they are held in the page; one taken
  out is never stored.
- **Limits.** 50 MB per file (`MAX_ATTACHMENT`, the route's body limit and the page's
  refusal line); no limit on how many; the user's text cut at `quote::CAP` (2000
  characters) in the terminal, whole in the record and the page.

## What the manager is given

`delivery::user_message` builds it from the record: one paragraph, workriver's own words,
then the user's text quoted. For example:

    [workriver] The user writes to you from workriver's page (message m3, about intent i2
    "Search"). Reply with `workriver reply m3 --text "..."`; the user reads replies there,
    not in this terminal. It points at: task u1#3 "Paging" (implement, by worker-2); task
    u1#5 "Cursor" (implement, by worker-4); finding P2-1; lines 10-24 of src/api.ts; the
    attachment "shot.png" at ~/workriver-data/attachments/9f2c….png. The files are in
    ~/src/search.worktrees/u1 on branch feat/paging (from main). Details:
    `workriver task show u1#3`. The user says, in workriver: «Why is the list not paged
    here?»

- What it points at is built by the code a question's prompt uses too
  (`delivery::pointed`), from the record: unit and task from the state, an extra task chip
  by its number with its title and agent (`delivery::label`), never the page's label;
  paths and labels through `quote::name`; attachments by the store's own name and path.
  Nothing the user typed is outside «…».
- With a `file` or `lines` in the context it says where they are: the unit's branch and
  base, and its work tree when it has one (found as for a question, `scan::worktree_of`,
  else the repository).
- To a manager on another machine each store path becomes `workriver attach fetch
  <id>.<ext>` (`App::send`).
- The delivery record (`MessageDelivery.message`) holds this text.
- **When it is held.** A manager waiting for a person in its terminal is not typed over: the
  message waits and goes in once it is free. If its manager is then on a blind task in
  progress, it waits for that task's delivery too. A restart while it waits marks it not
  delivered (the service restarted), so the user sees that and can send it again.
- The manager's side (answer, or reply that the task's agent is asked, `workriver ask` it
  without `--wait`, reply again with its answer) is the `user-message` recipe.

## Security

- `/api/message` is user-only (`App::user`), and `MessageSent` is in `Body::user_only`.
  Neither it nor anything of this conversation is on the machine listener
  (`machine_router`).
- A discussion: `POST /api/discuss` and `POST /api/discuss/read` ask for `App::user` and
  are not on `machine_router`. `question.asked` stays out of `Body::user_only` (agents
  ask), so core refuses a `discuss` question from anyone but the user, whatever route
  carried it; `questions.read` is in `Body::user_only`. The answer goes through `App::who`,
  and core accepts it from the task's agent alone (an agent on another machine answers
  through the machine listener's `/api/append`, as any record of its).
- A discussion's question goes into the agent's terminal only through `quote::users`; the
  answer and the conclusion reach the manager only through `quote::quoted`, as the agent's
  words. Nothing is typed into an agent on a blind task in progress (held until it
  delivers), and a blind agent reads no discussion (questions are not in
  `State::blind_view`).
- That a question failed (`question.failed`) is workriver's receipt (`Body::service_only`):
  no agent, nor the user, can close a question in workriver's words.
- Files reach a message only through the store's route, and the service names them only
  after the rules above, so a message cannot hand the manager a file the user could not
  point at.
- Text written by the user goes in only through `quote::users`; labels, paths and names
  through `quote::name`; a quote cannot be left from inside.
- Reading: messages are not in `State::blind_view` (a blind agent sees none, nor their
  context or files); `/api/attachments/{id}` goes through `App::view`.

## Deliberately not done

- No conversation with a working agent from the page: the manager decides whom to ask. The
  one exception is the discussion of a delivered design or research, with the agent that
  did it, about that result.
- A discussion of implement, review or tests tasks, before delivery, or after its gate; an
  answerer when the task's agent is gone (no temporary agent, no restart: the manager may
  hand out a new task); the manager or other agents writing in it; several per task, or
  ending one from the page; choices in an answer (if wanted, `answer --option` stores the
  gate choices' one format).
- An agent's own `deliver --amend` after its gate is submitted, outside a discussion: as
  before, not checked.
- No history of a task's deliveries on the page: it shows the latest, with "amended" and
  when; earlier versions stay in the event store.
- No richer, live conversation with an agent: that is the ACP conversation page, later;
  this widens only what a message to the manager carries.
- The manager does not pass the user's files on as attachments of its `workriver ask` (only
  the user may point a question at a file the user stored); it names their paths in the
  question's text.
- `/api/ask`, the temporary agents and the user's unit questions (task 0) stay in the
  service: agents use the first two, and old records must replay. The user's path to them
  is unused; removing it is a later cleanup.
- No limit on how many files, and no size limit but the store's.
