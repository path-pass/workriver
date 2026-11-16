# Starting an intent from the backlog

**When:** you are the manager of a Herdr workspace and begin a goal.

    workriver manager here                 # once per workspace: registers you
    workriver backlog list                 # the roadmap, in the user's order
    workriver intent start i3              # active here; your workspace's current intent
    workriver board i3

Not in the roadmap yet: `workriver backlog add "<title>" --goal "<one sentence>"`, then start it.
Only for a goal the user named. A step or part of a goal you are already on is never an
intent of its own: it goes into that intent, as a task of its unit or a line in a task's
text (`units`).
Finished: `workriver intent done i3`. Given up: `workriver intent drop i3 --reason "..."`
(an intent you started; one still in the roadmap is the user's to drop).

**You will see:** `i3 active`. New units and tasks here go to the intent started last,
unless you name one.

**Refusals:** "only the manager of workspace … starts intents there" - run
`workriver manager here` first, in your own pane. "intent i3 is done; it cannot become
active" - add a new one.
