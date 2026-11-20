# Conversations: asking why, the manager, agents asking each other

The side panel's Ask why section: questions about a task or a unit, the files that go with
them, the conversation with a workspace's manager, and agents' questions to each other.

Moved from `DESIGN.md` in phase 7b of the restructure, word for word but for paths
made whole and "see …" naming the file; `DESIGN.md` is the overview and says where each of
its sections went.

## Asking why

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
  attachment open in the viewer (see Attachments, `attachments.md`). A single file shows the whole file, or
  with "Only the changes" just its changes with three lines around each (remembered); the
  line-number gutter widens to the file's longest number. Lines are
  picked in any diff, the single file or the list (click a line number, shift-click to
  stretch); next to them floats "Ask why", like the "+" of a code review. It picks the task
  that last changed those lines (worked out from the unit's commits; other tasks that
  changed some of them come along as chips) and puts the cursor in its box, so the code can
  be asked about with no task picked; lines no task changed are asked about the unit (the
  task is unpicked). A chip can be left out for this question (its ×).
  Files go with a question too, any kind (a screenshot, a log, Markdown, a PDF), several at
  once: dropped anywhere on the Ask panel, pasted into the box, or picked with the small
  attach button in the box, before Send. The browser never opens a dropped file in the
  page's place, wherever on the page it is let go: outside what takes files (the Ask panel
  in its Why mode, a gate's form) the cursor says it cannot be dropped. The box is a text
  box first: pasted rich text (a spreadsheet's cells, a paragraph) goes in as text and the
  picture of it that comes along is not attached; a picture copied in a browser ("Copy
  image": html that is only the picture, no words) is attached; a pasted file is attached, and text of its
  own beside it goes in too (not text that only names the file). A pasted file with no name
  is named from its type (pasted.png, pasted.txt). Each shows at once as a chip after the others (the
  attach glyph, its name, a × that takes it out). As a gate's pictures, the page holds them
  until the question is sent and they are stored only then, so one taken out is never
  stored; one over the store's 50 MB is refused at once, in a red line above the box naming
  it, which goes with its ×, when a chip is taken out, with the next file attached, when the
  question is sent, or with another task picked. The answering agent is given each file's path in the store (a link, for an agent on
  another machine). In the conversation they show under the question as attachments do
  everywhere: pictures as thumbnails, other files as chips, opening in the viewer. Only the
  user attaches; the manager conversation takes no files.
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
