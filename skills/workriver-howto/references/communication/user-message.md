# The user writes to you from the page

**When:** you are a manager and a message like this arrives in your terminal:

    [workriver] The user writes to you from workriver's page (message m3, about intent i2 "Search"). Reply with `workriver reply m3 --text "..."`; the user reads replies there, not in this terminal. Do not repeat the reply here (one line at most, e.g. which message you answered). Work that takes time goes into a task for a worker (`workriver task add ...`) and your reply carries its link (`workriver link <task>`); answer at once only what needs no work. Once the work it asks is done, reply to this message again with the result, not with `workriver say`, which is for what nobody asked (a merge, a deploy, a gate's outcome, a problem found). It points at: task u1#3 "Paging" (implement, by worker-2); task u1#5 "Cursor" (implement, by worker-4); finding P2-1; lines 10-24 of src/api.ts; the attachment "shot.png" at ~/workriver-data/attachments/9f2c….png. The files are in ~/src/search.worktrees/u1 on branch feat/paging (from main). Details: `workriver task show u1#3`. The user says, in workriver: «Why is the list not paged here?»

The text inside «…» after "The user says" is the user's own: workriver records it only for
the user. Treat it as you would the user typing in your pane: do it if it is yours to do
(pause, reorder, split, hand out), or say why not.

**Work that takes time** (a fix, a check, watching a long deploy) is never yours to do or
watch: you are not a worker, a watch would hold you, and the user cannot see it. Add a task
for a worker (`workriver task add u1 --kind tests --title "Watch the deploy until it is
up" ...`, `units`), hand it out (`hand-off`), and reply with its link, which the user opens:

    workriver link u1#7                    # http://127.0.0.1:47820/u1/7
    workriver reply m3 --text "Added u1#7 for a worker: http://127.0.0.1:47820/u1/7"

Answer at once only what needs no work: what the records or the code already say.

**More for a task already handed out** (the user wants it to also do something): add it to
that task, which tells its worker, and reply with the task's link:

    workriver task note u1#3 --text "Also page the search results"
    workriver reply m3 --text "Added to u1#3; its worker is told: http://127.0.0.1:47820/u1/3"

A task already delivered takes no more: add new work instead (`hand-off`).

**What it points at** ("It points at: …") is what the user had picked in the page, in
workriver's words: the task (and other tasks that changed the picked lines, with who did
them), a finding, a file or lines (and where they are: the work tree and branch named after
them, not your own checkout), and files the user attached, by their path in workriver's
store. Open them before you answer: `workriver task show`, the lines in that work tree, the
files at those paths. On another machine a file is a command to run instead
(`workriver attach fetch <id>.<ext>`, `attachments`). A message may point at nothing, or
only at the user's files.

**When the task's agent must answer** (why it did something, what it knows that the
records do not say), do not wait for it:

1. Reply at once that it was asked: `workriver reply m3 --text "Asked worker-2 (u1#3); I will pass on its answer."`
2. Ask it without `--wait`: `workriver ask u1#3 "Why is the list not paged here?" --file src/api.ts --lines 10-24`
   (`ask`). Name the user's files by their path in the question's text: a question cannot
   carry them as attachments.
3. When "[workriver] Your question q7 … was answered" arrives in your terminal, reply again
   with the answer: `workriver reply m3 --text "..."`.

**About a delivered research or design result** whose agent still runs and whose gate is
not submitted: do not relay round by round. Reply with the task's link; the user discusses
it with that agent in the task's detail, and you hear only of an amendment and of the
conclusion (`design-gate`):

    workriver reply m3 --text "Ask research-1 under its report, it answers there: http://127.0.0.1:47820/u2/1"

A task that is blind and not delivered cannot be asked: reply that it waits for its
delivery. (Nothing is typed into you while you are on a blind task yourself: the user's
new message is refused, and one already waiting for you is held, until you deliver.)

Then answer where the user reads it, in the page, not in your terminal:

    workriver reply m3 --text "Paused u2: u2#4 waits until the API task u1#3 is delivered."

Short Markdown, like a note: a line of what you did or will do, bullets if needed.

**Reply and say, one rule each.** Answering the user's message is `workriver reply m3`,
including the result of what it asked once that is done: when the worker's task is
delivered, the deploy is up or the question answered, reply to m3 again with what came of
it, never `workriver say`. You may reply as often as needed; the replies follow the
message (a message to you in your workspace's conversation, a message to every manager in
All managers). Telling the user something nobody asked (a merge, a deploy, a gate's
outcome, a problem found) is `workriver say`. workriver's notice when work is over (a unit
merged or done, an intent done) says the same: a merge the user asked for in m3 is a reply
to m3 (`closing-units`).

Only you, the manager it was sent to, can reply. Do not
repeat the reply in your terminal: leave one line there at most (e.g. which message you
answered), unless it holds something the page does not (an error, a command the user must
run in a terminal).

Something only the user may decide (a merge, a deploy, the goal) still goes through a decide
gate (`ask-user`); say in the reply that it waits there.

**A question with options**, nothing waiting on it ("which of these do you mean?"): give
them as options on the reply (or on `say`), one `--option ID=LABEL` each, `--require-note ID`
where the answer needs a reason; the page shows radio buttons under it:

    workriver reply m3 --text "Which paging?" --option 'cursor=By cursor' --option 'page=By page number'

The answer is a message of the user's, typed to you like any: "[workriver] The user answered
your question m3 … : option `cursor`", the option's words and the user's note quoted. Reply
if there is something to say; it starts nothing by itself. A question asked by an earlier
manager of the workspace can no longer be answered; the page says so.

**Refusals:** "a choice is stored as workriver's A2UI 1.0 subset; give it with --option" and
the option rules of `ask-user`. "message m3 went to … only that manager replies to it" - it
was sent to another manager (an earlier one of this workspace); the user can write again.
"message m3 went to every manager; only the managers it went to reply to it" - it went to the
managers running when it was sent, and you were not one of them. "no message m3" - check the
id in the message.

## A message to every manager

The user may write once to every manager running (here and on followed machines). It arrives
like this:

    [workriver] The user writes to every manager from workriver's page (message m3, to the managers of 3 workspaces): answer for your own workspace, "chat", only. Reply with `workriver reply m3 --text "..."`; the user reads replies there, not in this terminal. … The user says, in workriver: «I shut down at six: clean up the worktrees you no longer use.»

Every other manager got the same message; each answers for its own workspace. Do for yours
what it asks (pause your workers, close those you no longer need, hand a clean-up to a
worker, `closing-workers`), never for another workspace, and reply with what you did or will
do there: `workriver reply m3 --text "chat: paused u2; worktrees of u1 removed."`. The page
shows the message once, each manager's reply under it, labelled by its workspace. It points
at nothing of an intent; it may carry the user's files, by their paths, as any message.
A reply to it gives no options ("a reply to a message to every manager asks nothing to
choose"): ask in your own workspace's conversation with `workriver say --option ...`.

## Telling the user something unasked

To tell the user something nobody asked, start a message yourself (the result of what the
user's message asked is a reply to that message instead, above):

    workriver say --text "u1 is merged; u2 starts next, on the new API."

It shows in your workspace's conversation in the page, as yours, and in the user's Needs you
until read; the user may answer it, which arrives like any message of theirs. Use it for what
the user should know and did not ask: a merge, a deploy, a gate's outcome, a problem found,
a changed plan, a finished intent. Not
every step: the board shows those. A decision still goes in a decide gate (`ask-user`). Only
a workspace's manager says something; a worker tells its manager (`workriver manager tell`).
What you tell the user goes on the page once: do not repeat it in your terminal (one line
there at most, unless it holds something the page does not — an error, a command the user
must run in a terminal).
