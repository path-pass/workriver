# Starting an intent from the backlog

**When:** you are the manager of a workspace (the user seated you: `seat`) and begin a goal.

    workriver backlog list                 # the roadmap, in the user's order
    workriver intent start i3              # active in your workspace; its current intent
    workriver board i3

You start intents in the workspace you hold the seat of, and nowhere else: the command
names no workspace, workriver fills in yours. And only intents of your workspace's project
(the project the workspace was made in; `workriver workspace list`). An intent's project is
the one recorded on it when it was added, never where its units are. Another project's
intent runs in a workspace of that project: the user starts it there. An intent of no
project is the user's to start.

Not in the roadmap yet: `workriver backlog add "<title>" --goal "<one sentence>" [--kind K]
[--size S] [--tag T]...`, then start it.
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
then you get `[workriver] The user started i3 «title» in your workspace …`. It is already
active here: do not start it again; `workriver board i3`, then split it into units and tasks.

**Moved by the user:** the user may move an active intent to another workspace of its
project; its manager is then that workspace's. What was said about it before stays in the
conversation it was said in.

**Its kind, size and tags:** when you split it, set a kind it lacks and the size you judge,
with `workriver intent set i3 [--kind feature|fix|chore|research] [--size s|m|l] [--tags
a,b] --base N` (each replaces what is there; `none` clears it). S: one unit, one round of
implement and review (hours); M: one or two units, a few review rounds (a day or two); L:
three units or more, or a design first (days). Do not change a kind, size or tags the user
set (`workriver intent history i3` shows who set what): if you think one is wrong, tell the
user why. The first unit's branch prefix follows the kind (`units`). You set them only while
the intent is active and yours; a planned, done or dropped intent's are the user's. When the
user changes them you hear `kind fix; size M; tags «security, cli»` in the same notice as a
title or goal.

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
if it is of your workspace's project. Only additions: its title, goal and the rest
of its note are the user's until it is started, and so is dropping it. Done and dropped
intents are the user's alone.

**Changing managers** is the user's act (`seat`): the user seats the new one, and the old
one is told it no longer manages. The old manager writes down what workriver does not hold
(why things were decided, what it was about to do) in your shared memory, if you have one,
else as a note on the intent (`workriver intent set i3 --add-note "…"`), before or when it
is told. The new one reads `workriver board`, the conversation on the page's record
(`workriver board` names the open messages) and the memory or note. A message the user
sent the old manager and it never got shows as not delivered on the page: it is not passed
on to you; the user writes again.

**Refusals:** "only a workspace's manager, or the user, starts an intent" - you hold no
seat: tell the user which workspace you are meant for (`seat`). "you manage workspace ws2
(…): a manager starts intents there, and nowhere else" - leave the workspace out. "i3
belongs to project Q; workspace ws2 (…) is of project P: an intent runs in a workspace of
its project" - not yours to start: tell the user, who starts it in a workspace of Q.
"i3 has no project: ask the user to start it" - the user starts it (it then takes the
project of the workspace). "workspace ws2 is closed" - the user opens it first. "… is in no
registered project: name a project by its id, or ask the user to add it first" - `backlog
add --project` takes a project the user registered (`workriver project list`): give the
user the line `workriver project add <folder>`. "intent i3 is done; it cannot become
active" - add a new one. "the title and goal of i3 are the user's" - ask the user; write
what you know in its note. "i7 is planned: its kind, size and tags are the user's" - start it
first, or ask the user; write what you think in its note. "give --base N" - read `workriver intent history i3` first.
"i7 belongs to project …; the workspace you manage is of another project" - its note is not
yours to add to.
"only the user or the manager of intent i3 marks it done" - not your intent. "only the user drops planned intent i7" - it is
in the roadmap: ask the user. "a state change names
a workspace only when it starts an intent" - `intent done` and `intent drop` name none.
