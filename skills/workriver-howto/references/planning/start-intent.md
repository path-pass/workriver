# Starting an intent from the backlog

**When:** you are the manager of a Herdr workspace and begin a goal.

    workriver manager here                 # once per workspace: registers you
    workriver backlog list                 # the roadmap, in the user's order
    workriver intent start i3              # active here; your workspace's current intent
    workriver board i3

You start intents of the project your workspace works on by the records only: the one the
user set (`workriver workspace list`), else that of its current intent, else that of the
intents that ran in it. A new workspace has none: the user starts its first intent on the
page ("Start" in the roadmap, picking your workspace) or sets its project; ask them
(`ask-user`), then go on. Another project's intent the user starts, in a workspace of that
project or, confirming it, in yours.

Not in the roadmap yet: `workriver backlog add "<title>" --goal "<one sentence>"`, then start it.
Only for a goal the user named. A step or part of a goal you are already on is never an
intent of its own: it goes into that intent, as a task of its unit or a line in a task's
text (`units`).
Finished: `workriver intent done i3`; its output then asks you to tell the user what the
intent came to: a reply to the user's message that asked for it, else `workriver say`
(`closing-units`). Given up: `workriver intent drop
i3 --reason "..."` (an intent you manage; one still in the roadmap is the user's to drop).
Done, dropped or back in the roadmap: only you, as its manager, or the user.
Planned intents that are one goal merge into one (`workriver backlog merge <into>
<from>...`): you may merge intents agents added (an import); ones the user added are the
user's to merge (ask them; "Merge…" in the roadmap). The merged ones are dropped, "merged
into i3", their text in i3's note; i3 keeps its title (`--title` is the user's: they rename
it on its page). `workriver board i3` prints the note.

**Started by the user:** the user may start a planned intent in your workspace from the page;
then you get `[workriver] The user started i3 «title» of project «workriver» (folder
«/path/to/workriver») in your workspace …`. It is already active here: do not start it again;
`workriver board i3`, then split it into units and tasks.

**Another project:** check the project the message (or `workriver board i3`) names against the
one you work on (the folder your pane runs in). If they differ, do not split it: add a
research unit and a decide gate `--after start` asking the user whether to go on here or start
it in a workspace of that project (`ask-user`), and wait. `workriver intent start` of another
project's intent than your workspace's (by the records) is refused; one the records allow is
recorded with a `warning:` line when your pane's folder says another project.

**You will see:** `i3 active`. New units here go to the intent started last, unless you
name one. Research or design for the intent as a whole is a research unit (`units`), not a
task on the intent. Its page is `workriver link i3` (goal, note, units); add what the user
should read there with `workriver intent set i3 --add-note "…"`.

**The intent's text:** its title and goal are the user's (they edit them on its page); if
you think they should read otherwise, say so to the user. Its note is yours too while it is
active: add a paragraph with `--add-note "…"`, or correct it whole with `workriver intent set
i3 --note "$(cat note.md)" --base N`, where N is the version you read: `workriver intent
history i3` lists who changed what and when, and ends with the version now; `--version N`
prints one whole, `--diff N` what it changed. A replace from an older version is refused
when the note changed since ("i3's note changed since v4 (in v5; it is at v5, …)"): read it
and run again with the newer base. Every change is kept and can be restored (the user does
that on the page; you, by `--note` with an older version's text). When the user changes
your active intent you get `[workriver] The user changed i3 (now v5): title: «…»; goal:
«…»; its note changed (workriver intent history i3 --diff 5).`: plan with what it says now.

**A planned intent's note:** what the user tells you about an intent still in the roadmap
(an example, a constraint) goes into its note with `workriver intent set i7 --add-note "…"`,
if a workspace you manage works on its project (as the user set it, `workriver workspace
list`, else the project of its current intent, else that of the intents that ran in it). Only additions: its title, goal and the rest
of its note are the user's until it is started, and so is dropping it. Done and dropped
intents are the user's alone.

**Changing managers** (one handing over to another pane): a workspace has one manager, so
the new one registers only after the old one is closed. The old manager writes down what
workriver does not hold (why things were decided, what it was about to do) in your shared
memory, if you have one, else as a note on the intent (`workriver intent set i3 --add-note
"…"`); the user closes it; the new one runs `workriver manager here` at once, then
`workriver board`, and reads the memory or note. A message sent in between shows as not
delivered on the page: ask the user to paste it to you.

**Refusals:** "only the manager of workspace … starts intents there" - run
`workriver manager here` first, in your own pane. "intent i3 is done; it cannot become
active" - add a new one. "the title and goal of i3 are the user's" - ask the user; write
what you know in its note. "give --base N" - read `workriver intent history i3` first.
"i7 belongs to project …; your workspace's project is not known" - ask the user to set it
(Settings, or `workriver workspace project`), or to start i7 in your workspace on the page.
"i3 belongs to project Q; your workspace works on P" - not yours to start: ask the user.
"i3 has no project: ask the user to start it" - an intent from before projects: the user
starts it.
"only the user or the manager of intent i3 marks it done" - not your intent. "only the user drops planned intent i7" - it is
in the roadmap: ask the user. "workspace … has a manager running: X" - a workspace has one
manager; you are a worker there, not its manager: tell the user, who closes X first if you
are to take over. "an agent manages the workspace it runs in" - run it in your own pane.
