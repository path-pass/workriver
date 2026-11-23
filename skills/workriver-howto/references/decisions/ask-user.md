# Asking the user to decide

**When:** a unit is ready to push or merge (one gate per step: `push-pr-merge`), something is to be deployed, the goal or the agreed
units would change, anything destructive, or a task failed the same way twice. The user
does not watch your pane; they watch Needs you. Ask there, with a decide gate, never only in
your pane.

    workriver task add u2 --kind gate --gate decide --go --title "Merge u2 (feat/readme) into main?" --after 4 --text "<what is ready, what was checked, what merging does>"
    workriver task add u5 --kind gate --gate decide --go --title "Drop the export part of the goal?" --after start --text "<why, and what changes>"

**Two kinds of decide gate; say which, one of the two.** A **go** (`--go`) asks may I (merge,
deploy, change the goal, a design's gate): the user confirms (done) or not (findings), and
work may wait behind it. A **choice** asks which of several answers: give each as an option,
`--option ID=LABEL` (ID: 1 to 32 of a-z, 0-9, -; 2 to 8 options, in the order shown), and
`--require-note ID` for one whose answer needs a reason. The page shows them as radio
buttons; never list options only in `--text` or a step. workriver ends every choice with
**Other** itself (the user answers outside your options with a note saying what to do
instead): do not add an option for it; the ID `other` is workriver's and refused. A decide gate with neither, or with
both, is refused before anything is recorded.

    workriver task add u2 --kind gate --gate decide --title "u2: next?" --after 7 --text "<what is ready, what was checked>" \
      --option 'merge=Merge and push once u2#7 is checked' --option 'review=Review once more' \
      --option 'hold=Not now' --require-note hold

**Except a design's gate** (after a design, directly or through its reviews): it is a `--go`
gate; its choices stay in `--text` and done confirms the design, the note saying which way
(`design-gate`); `--option` is refused there. A question nothing waits on, while talking with the user,
goes in the conversation instead (`workriver say --option …`, `user-message`).

A unit's question goes on the unit, after its last task; a question about the whole
intent goes in the intent's research unit (`units`; add one if it has none), after the task
that raised it, or `--after start` when none did. Put in `--text` what the user needs to decide without
opening anything else: what is ready or wrong, what was checked, the options.

**After research:** the user may discuss the report with its researcher before deciding,
in the research task's detail (`discussion`); keep the researcher until the gate is
submitted. The conclusion comes to you as a message saying what is next; research
concluded with no gate after it needs one only when work must start on it.

**Pointing the user at it** (or at any task, unit or intent): `workriver link u2#5` prints
the link that opens it in the user's browser (`u2`, `i3`, `u2#5:P2-1` work too),
also on another machine. `workriver task show` and `workriver board` print links as well.

**You will see:** "The user confirmed gate …" with their note: do it (merge, deploy,
change the goal). A choice: "The user answered gate u2#8 …: option `review`", the option's
words and the user's note quoted: nothing follows from it by itself; add the task that option
calls for, `--after 8`, within what the question said (push, pull request and merge stay one
task each). Or "The user answered gate u2#8 … with none of its options: Other (`other`), with
a note saying what to do instead", the note quoted after `Other:`: the note is the user's
instruction in place of an option; do what it says, within the question, the same way (add
its task `--after 8`; a note that changes the goal or asks something you cannot do: ask
again). A note too long for the terminal is quoted cut: the message says "the note is cut
here; read it whole first with `workriver task show u2#8`", and so do that before acting:
the end may hold a condition. Or "did not confirm gate …" with findings: the user decides each, and you
hand out work for those accepted (`deciding-findings`). Only a gate that comes after a
design reads "confirmed the design" (`design-gate`). Until then, wait; do not merge or deploy.

**Asking again:** when a question you asked is out of date (the branch changed after a merge
gate, the options changed), ask anew and drop the old gate, or it waits in Needs you for good.
A task is dropped, never deleted; what follows it no longer waits on it:

    workriver task add u1 --kind gate --gate decide --go --title "Merge u1 (feat/readme) into main?" --after 16 --text "<what changed since>"
    workriver task drop u1#15 --reason "superseded by u1#17: the branch changed"

The same for any task re-planning made unneeded: its agent is told to stop (the reason is
quoted to it); the board shows `dropped (reason)`.

**Nothing is put before a recorded gate.** Once a gate is recorded, no task goes before it:
work put there would change what the gate asks, and a gate open
or already answered could then read as confirming what the user never saw. When new work
must come first (a fix, another review, an amended design), ask anew: drop the gate with
its reason, record the new work after what the gate followed, and a new gate after that
work. What waited on the dropped gate waits no more (a gate after it would open at once),
so drop that first and add it again after the new gate. Say gate u1#8 follows review 7,
push 9 follows the gate, and a fix of the review's P2-1 must come first:

    workriver task drop u1#9 --reason "waits on the gate asked anew: a fix comes first"
    workriver task drop u1#8 --reason "asked anew: a fix of P2-1 of u1#7 comes first"
    workriver task add u1 --title "Fix P2-1" --cause 7:P2-1 --after 7 --agent fix
    workriver task add u1 --kind gate --gate decide --go --title "Push u1 (feat/readme)?" --after 10 --text "<what is there now>"
    workriver task add u1 --title "Push feat/readme" --after 11 --agent pusher --text "Push the branch only."

The user sees a new gate about what is there now, and the dropped one says why. A gate the
user submitted is done and is not dropped: what changes after it is new work after it (a
changed design is a new design task, `design-gate`).

**Refusals:** "a decide gate asks either may I (--go …) or which (--option …)" - give one
of the two; "a decide gate takes --go or --option, not both"; "--go is for a decide gate
only" - a deploy or test gate, or a task that is no gate, takes no `--go`. "gate … is not open yet: it follows …" - it waits until the tasks before it
are done; that is fine, it shows in Needs you then. "a design is confirmed with a decide gate
without --option (--go)" - a design's gate: `--go`, options in `--text`. "u2#8 asks the user to choose; add
what follows once it is answered" - nothing hangs after a choice not answered yet. "choices
are asked by the manager of intent …" - a worker tells its manager. "a choice has 2 to 8
options", "an option's ID is 1 to 32 of [a-z0-9-]", "an option's ID may not be other:
workriver adds Other with a note to every choice itself" - leave that option out, "--require-note … names no option". "… is done; only work not done yet is
dropped" - a gate the user submitted is done. "… is dropped by the manager of its intent or
the user (or the agent that recorded it)" - a worker tells its manager instead. "… was
dropped as no longer needed (…)" - it is not started, delivered or submitted any more.
