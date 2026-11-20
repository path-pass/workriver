# Asking the user to decide

**When:** a unit is ready to push or merge (one gate per step: `push-pr-merge`), something is to be deployed, the goal or the agreed
units would change, anything destructive, or a task failed the same way twice. The user
does not watch your pane; they watch Needs you. Ask there, with a decide gate, never only in
your pane.

    workriver task add u2 --kind gate --gate decide --title "Merge u2 (feat/readme) into main?" --after 4 --text "<what is ready, what was checked, what merging does>"
    workriver task add u5 --kind gate --gate decide --title "Drop the export part of the goal?" --after start --text "<why, and the options>"

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
change the goal). Or "did not confirm gate …" with findings: the user decides each, and you
hand out work for those accepted (`deciding-findings`). Only a gate that comes after a
design reads "confirmed the design" (`design-gate`). Until then, wait; do not merge or deploy.

**Asking again:** when a question you asked is out of date (the branch changed after a merge
gate, the options changed), ask anew and drop the old gate, or it waits in Needs you for good.
A task is dropped, never deleted; what follows it no longer waits on it:

    workriver task add u1 --kind gate --gate decide --title "Merge u1 (feat/readme) into main?" --after 16 --text "<what changed since>"
    workriver task drop u1#15 --reason "superseded by u1#17: the branch changed"

The same for any task re-planning made unneeded: its agent is told to stop (the reason is
quoted to it); the board shows `dropped (reason)`.

**Refusals:** "gate … is not open yet: it follows …" - it waits until the tasks before it
are done; that is fine, it shows in Needs you then. "… is done; only work not done yet is
dropped" - a gate the user submitted is done. "… is dropped by the manager of its intent or
the user (or the agent that recorded it)" - a worker tells its manager instead. "… was
dropped as no longer needed (…)" - it is not started, delivered or submitted any more.
