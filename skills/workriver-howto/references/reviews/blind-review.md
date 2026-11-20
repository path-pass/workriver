# A blind review

**When:** the reviewer must form its own view. Also for tests, research and design tasks.

    workriver task add u1 --kind review --reviewer security-review --after 1 --agent sec --blind --text "Find ways around the token check"

Until it delivers, workriver shows that agent only its own task, the unit and the intent; it
can neither ask nor be asked. Your hand-off carries **only** the ref and goal, the machine,
repository, branch and range, "run `workriver task show <ref>`" (its own task only), and
the worker instructions: not the implementer's note, the evidence, other findings, or your
opinion. Tell it to read the diff, not the commit messages. Use an agent new to the unit.

As in every review, what is wrong (a bug, a hole, a broken test) is a finding by itself;
how the code should look or be organized is judged by the project's own rules (its
`CLAUDE.md` / `AGENTS.md`) and the design, never by a style of the reviewer's, and such a
finding names the rule. The design before it is other work and stays hidden too: a decision
the review must hold the code to goes in `--text`.

**You will see:** `[blind]` on the board; the reviewer's own board starts "blind review:
other work is hidden until you deliver" and gives the `git diff` command.

**Refusals:** "--blind is for … a review, tests, research or design task". "… is a blind
task in progress" - ask after it delivered.
