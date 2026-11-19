# Design first, then a decision, then implementation

**When:** the goal leaves choices the user has not made (how it looks, which approach).
What is already decided needs no design task: cite the decision in the task's `--text`.

    workriver task add u1 --kind design --title "Design the mention picker" --agent designer
    workriver task add u1 --kind gate --gate decide --title "Confirm the design" --after 1
    workriver task add u1 --title "Build the picker" --after 2 --agent builder

The designer delivers the design as its note (`workriver task deliver u1#1 --note "..."`).
The user discusses it in the browser and confirms at the gate. Every task after it in the
unit reads that note and its files in `workriver task show` ("Decided before this task")
and follows it; reviews check the work against it. A design that sets the shape of the
work can get an architecture review before the gate (`architecture-review`).

A design for the whole intent (across units or repositories) goes in a research unit
(`units`): `workriver unit add --kind research --title "…"`, then the design task and the
gate there, the implementation in the code units after it. Once confirmed, put the decision
on the intent's page: `workriver intent set i3 --add-note "Design: … (u2#2)"`.

**You will see:** "The user confirmed the design in gate …", the decision quoted: hand out
the implementation. Or "did not confirm …": for each finding accepted or revised, a new
design task `--cause 2:<id>`, a new decide gate after it, the implementation after the
**new** gate.

**Refusals:** "a design task delivers its design as the note".
