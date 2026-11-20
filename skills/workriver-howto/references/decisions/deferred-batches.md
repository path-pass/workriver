# Deferred findings and batches

**When:** a finding was deferred, or the user picked several to do together.

Deferred findings wait in the project's backlog:

    workriver backlog list

The user ticks some in the browser ("Do together"). You get one message: "The user picked N
deferred findings of intent i3 … to do together", with each finding quoted and the commands.
Make **one** cleanup unit and one task with all of them as causes:

    workriver unit add i3 --title "Cleanup: token handling" --branch chore/cleanup-tokens
    workriver task add u7 --title "Fix the batch" --cause u1#4:P2-1 --cause f3 --agent <name>

(`u1#4:P2-1` is a finding of another unit's task; `f3` one added to the backlog directly.)

**You will see:** the findings marked scheduled, and done when that task is delivered.

**Refusals:** "finding … is neither accepted, revised nor scheduled; decide it first".
Dropping deferred findings (`workriver finding drop`) is the user's.
