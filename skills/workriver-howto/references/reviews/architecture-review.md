# An architecture review

**When:** a design that sets the shape of the work was delivered (review the solution), or
an implementation that added or moved more than a little code (review how it is organized).
A third reviewer beside `code-review` and `security-review`; in everything else it is a
review like those (findings, decisions, blind or not).

The `development` template places both (`design-review` and `architecture-review`,
`templates`): fill them (`workriver task fill u1#2 --agent arch --text "…"`), or drop one
with the reason the user reads on the unit's gates ("added or moved little code: 40 lines in
one file"). Later rounds are added by hand, as below; fill the placed `go` gate only after
the last round, `--after` its review.

    workriver task add u1 --kind review --reviewer architecture-review --after 1 --agent arch --text "The design of u1#1 against CLAUDE.md"
    workriver task add u1 --kind review --reviewer architecture-review --after 3 --agent arch --text "How u1#3 is organized, against CLAUDE.md and the design u1#1"

A design's decide gate comes **after** its architecture review (in a later round: after the
review of the revised design), never beside it: the user confirms what the review has been through.

    workriver task add u1 --kind gate --gate decide --go --title "Confirm the design" --after 2   # 2: the review of design 1

**What it checks**

- **Code organization** (after implementation): layers, modules, who is responsible for
  what, duplication.
- **Solution architecture** (after a design): boundaries, where data and state live, which
  way dependencies point, deployment, what can be replaced.

**Against what:** as in every review, what is wrong (a bug, a hole, a broken test) is a
finding by itself; how the code should look or be organized is judged by the project's own
rules (the repository's `CLAUDE.md` / `AGENTS.md`) and the design tasks before it
(`workriver task show` prints them under "Designed and found before this task"; the design
a review follows directly is marked "the design under review": judge it, do not follow it),
never by a style of workriver's or the reviewer's. Such a finding names the rule or the
decision it holds the work to (file and line, or the task). Where the project has no rule,
a preference is at most a P3. A rule or a design that seems wrong is a finding of its own,
not a reason to ignore it (`contract`).

**The loop:** review, fix, review again, until a round has no accepted P1 or P2 (one you
rejected or deferred does not count):

    workriver finding decide u1#4 P2-1 accept --note "…"                     # `deciding-findings`
    workriver task add u1 --title "<outcome>" --cause 4:P2-1 --agent builder
    workriver task add u1 --kind review --reviewer architecture-review --after 5 --agent arch   # the next round

At most **3 rounds**. A third round that still has an accepted P1 or P2: stop, and ask the user with
a decide gate that says what is still open and the choices (`ask-user`):

    workriver task add u1 --kind gate --gate decide --go --title "Architecture: 3 rounds, P2-1 still open" --after 8 --text "…"

When what was reviewed is a design, that gate is the design's: `--go`, the choices stay in
`--text` (`design-gate`). After a review of code it may ask which instead, one `--option`
each in place of `--go` (`ask-user`).

**You will see:** `review (architecture-review)` on the board; its findings are delivered
and decided like any review's.
