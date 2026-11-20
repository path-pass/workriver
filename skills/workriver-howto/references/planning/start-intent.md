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
Finished: `workriver intent done i3`; its output then asks you to tell the user with
`workriver say` what the intent came to (`closing-units`). Given up: `workriver intent drop
i3 --reason "..."` (an intent you started; one still in the roadmap is the user's to drop).
Planned intents that are one goal merge into one (`workriver backlog merge <into> <from>...
[--title "<new title>"]`): you may merge intents agents added (an import); ones the user
added are the user's to merge (ask them; "Merge…" in the roadmap). The merged ones are
dropped, "merged into i3", their text in i3's note; `--title` renames i3. `workriver board
i3` prints the note.

**Started by the user:** the user may start a planned intent in your workspace from the page;
then you get `[workriver] The user started i3 «title» of project «workriver» (folder
«/path/to/workriver») in your workspace …`. It is already active here: do not start it again;
`workriver board i3`, then split it into units and tasks.

**Another project:** check the project the message (or `workriver board i3`) names against the
one you work on (the folder your pane runs in). If they differ, do not split it: add a
research unit and a decide gate `--after start` asking the user whether to go on here or start
it in a workspace of that project (`ask-user`), and wait. `workriver intent start` says the
same on a `warning:` line when workriver knows your workspace works on another project
(from your pane's folder, else your intents, or as the user set it:
`workriver workspace list`).

**You will see:** `i3 active`. New units here go to the intent started last, unless you
name one. Research or design for the intent as a whole is a research unit (`units`), not a
task on the intent. Its page is `workriver link i3` (goal, note, units); add what the user
should read there with `workriver intent set i3 --add-note "…"`.

**Changing managers** (one handing over to another pane): a workspace has one manager, so
the new one registers only after the old one is closed. The old manager writes down what
workriver does not hold (why things were decided, what it was about to do) in your shared
memory, if you have one, else as a note on the intent (`workriver intent set i3 --add-note
"…"`); the user closes it; the new one runs `workriver manager here` at once, then
`workriver board`, and reads the memory or note. A message sent in between shows as not
delivered on the page: ask the user to paste it to you.

**Refusals:** "only the manager of workspace … starts intents there" - run
`workriver manager here` first, in your own pane. "intent i3 is done; it cannot become
active" - add a new one. "workspace … has a manager running: X" - a workspace has one
manager; you are a worker there, not its manager: tell the user, who closes X first if you
are to take over. "an agent manages the workspace it runs in" - run it in your own pane.
