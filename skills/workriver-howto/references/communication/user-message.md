# The user writes to you from the page

**When:** you are a manager and a message like this arrives in your terminal:

    [workriver] The user writes to you from workriver's page (message m3, about intent i2 "Search"). Reply with `workriver reply m3 --text "..."`; ... The user says, in workriver: «Pause u2 until the API settles.»

The text inside «…» after "The user says" is the user's own: workriver records it only for
the user. Treat it as you would the user typing in your pane: do it if it is yours to do
(pause, reorder, split, hand out, ask a worker through `workriver ask`), or say why not.

Then answer where the user reads it, in the page, not in your terminal:

    workriver reply m3 --text "Paused u2: u2#4 waits until the API task u1#3 is delivered."

Short Markdown, like a note: a line of what you did or will do, bullets if needed. You may
reply again later (when it is done). Only you, the manager it was sent to, can reply.

Something only the user may decide (a merge, a deploy, the goal) still goes through a decide
gate (`ask-user`); say in the reply that it waits there.

**Refusals:** "message m3 went to … only that manager replies to it" - it was sent to
another manager (an earlier one of this workspace); the user can write again. "no message
m3" - check the id in the message.

## Telling the user something unasked

To tell the user something without being asked, start a message yourself:

    workriver say --text "u1 is merged; u2 starts next, on the new API."

It shows in your workspace's conversation in the page, as yours, and in the user's Needs you
until read; the user may answer it, which arrives like any message of theirs. Use it for what
the user should know without acting: a milestone, a changed plan, a finished intent. Not
every step: the board shows those. A decision still goes in a decide gate (`ask-user`). Only
a workspace's manager says something; a worker tells its manager (`workriver manager tell`).
