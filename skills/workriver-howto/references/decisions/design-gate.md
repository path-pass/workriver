# Design first, then a decision, then implementation

**When:** the goal leaves choices the user has not made (how it looks, which approach).
What is already decided needs no design task: cite the decision in the task's `--text`.

    workriver task add u1 --kind design --title "Design the mention picker" --agent designer
    workriver task add u1 --kind review --reviewer architecture-review --after 1 --agent arch
    workriver task add u1 --kind gate --gate decide --title "Confirm the design" --after 2
    workriver task add u1 --title "Build the picker" --after 3 --agent builder

The review (task 2) is for a design that sets the shape of the work: the gate comes after
it, so the user is not asked to confirm what the review is about to change
(`architecture-review`). A design that does not set the shape skips the review: the gate
`--after 1`, the build after the gate.

The design's gate is a plain decide gate: the choices the design leaves go in its `--text`,
and the user confirms with done, saying which in the note. `--option` there is refused (an
answer to options would confirm nothing; `ask-user`).

The designer delivers the design as its note (`workriver task deliver u1#1 --note "..."`).
The user discusses it in the browser with the designer, in the design task's detail
(`discussion`: keep the designer until the gate is submitted), and confirms at the gate.
You hear of the discussion only when the design is amended in it and when it is concluded:

    [workriver] designer amended u1#1 "Design the picker" while the user discusses it; its gate u1#3 "Confirm the design" is not submitted yet. …
    [workriver] The user concluded the discussion of u1#1 "Design the picker" with designer (thread q5). Its result was amended at …. Next: …

When the amended design's architecture review before the gate had started, the review and
the gate saw the old text: drop the gate (`workriver task drop u1#3 --reason "amended in its
discussion"`), add the review again and a new decide gate after it. A review not started
yet reads the amended design; otherwise the gate now asks about the amended design. The
message says which. Once the gate is submitted the discussion is over; a change
then is a new design task (below). Every task after it in the
unit reads that note and its files in `workriver task show` ("Designed and found before
this task", with whether the user confirmed it) and follows it; reviews check the work
against it.

A design for the whole intent (across units or repositories) goes in a research unit
(`units`): `workriver unit add --kind research --title "…"`, then the design task and the
gate there, the implementation in the code units after it. Once delivered, that design is
shown to every task of the intent's other units the same way, confirmed or not yet. Once
confirmed, put the decision on the intent's page: `workriver intent set i3 --add-note "Design: … (u2#2)"`.

**You will see:** "The user confirmed the design in gate …", the decision quoted: hand out
the implementation. Or "did not confirm …": for each finding accepted or revised, a new
design task `--cause 3:<id>`, its architecture review if the design sets the shape of the
work, a new decide gate after that, the implementation after the **new** gate. The revising
task is shown the old design as "the design you revise"; once it is delivered it takes the
old one's place in `task show` of every task after the old one, marked "revises u1#1"
(until then the old one reads "being revised in …").

**Refusals:** "a design task delivers its design as the note".
