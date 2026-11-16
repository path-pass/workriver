# Asking about another task

**When:** you need to know something about work another agent did, or its contract. Never
type into another agent's terminal.

    workriver ask u1#3 "Is the list paged?" [--file src/api.ts [--lines 10-24]] [--wait]
    workriver answer q7 --text "Yes, 50 per page."          # when you are the one asked

The task's own agent answers if it is free; else a temporary agent reads the records and
the code.

**You will see:** the question id (`q7`); later, in your terminal, "Your question q7 … was
answered" with the answer quoted. With `--wait` the command prints it.

If the answer means an agreed contract or plan must change, do not agree it between you:
the manager decides (`contract`).

**Refusals:** "you are on a blind task: …" - tell your manager instead. "u1#3 is a blind
task in progress: …" - ask once it is delivered. "no answer to q7 within … s" - it comes later.
