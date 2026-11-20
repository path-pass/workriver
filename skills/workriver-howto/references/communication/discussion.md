# The user discusses your delivered result

**When:** you delivered a research or design task, and a message like this arrives:

    [workriver] The user discusses your delivered task u2#1 "Compare stores" (question q7, discussion q5). Answer with `workriver answer q7 --text "..."`; the user reads it in workriver's page, not in this terminal. Short Markdown, in the language of the question. Do not amend your delivery now: when the user asks for the conclusion, you amend it if it must change. How: `workriver guide how discussion`. The user says, in workriver: «Why not Postgres?»

The text inside «…» is the user's own question about your result. Answer it from what you
found (look again, measure, search, as your task allows), where the user reads it:

    workriver answer q7 --text "No server to run, and one file to back up."

Short Markdown: a line or two, then bullets if needed. Each answer shows under your task in
the page, and in the user's Needs you until read. Your manager is not told of each round.
Do not amend your delivery while the discussion goes on.

**The conclusion:** "[workriver] The user asks for the conclusion of the discussion of your
delivered task u2#1 … (question q9, discussion q5)". If what you delivered must change, amend
it first, the whole result as it now stands (files as when you delivered), then answer with
the conclusion:

    workriver task deliver u2#1 --amend --note "..."
    workriver answer q9 --conclusion --text "- Decided: ...
    - Changed: ...
    - Open: ..."

Your manager is told of the conclusion, and of the amendment, and does what follows (the
gate waits for the user; a design reviewed before is reviewed again; or nothing).

**Stay** after delivering a research or design task: your manager keeps you until the gate
that asks about your result is submitted. A question that comes while you work on another
task waits until you are idle; none comes while you are on a blind task.

**Refusals:** "question q7 is part of the discussion of u2#1: it is answered by the agent of
u2#1 alone" - run it from your own pane, as the agent of that task. "its gate u2#3 was
submitted: the discussion of u2#1 is over" - the user confirmed the result; a change now is
a new task (`workriver manager tell u2#1 --text "..."`). "q9 asks for the conclusion: answer
with --conclusion".
