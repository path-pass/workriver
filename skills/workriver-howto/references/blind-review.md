# A blind review

**When:** the reviewer must form its own view. Also for tests, research and design tasks.

    workriver task add u1 --kind review --reviewer security-review --after 1 --agent sec --blind --text "Find ways around the token check"

Until it delivers, workriver shows that agent only its own task, the unit and the intent; it
can neither ask nor be asked. Your hand-off carries **only** the ref and goal, the machine,
repository, branch and range, "run `workriver task show <ref>`" (its own task only), and
the worker instructions: not the implementer's note, the evidence, other findings, or your
opinion. Tell it to read the diff, not the commit messages. Use an agent new to the unit.

**You will see:** `[blind]` on the board; the reviewer's own board starts "blind review:
other work is hidden until you deliver" and gives the `git diff` command.

**Refusals:** "--blind is for … a review, tests, research or design task". "… is a blind
task in progress" - ask after it delivered.
