# A worker stopped without delivering

**When:** a `[workriver]` message says a worker "stopped … min ago without delivering" a task,
or "is gone" while its task is still running. Before you hear of it, workriver reminded the
worker itself once (after the threshold the user set, 10 minutes by default) and waited half
that again; a worker waiting on a gate open to the user (a blocker before its task, too), or
on a question it asked, is not counted as stopped.

It stopped (idle in its terminal): look at why.

    herdr agent read <name> --source recent-unwrapped --lines 80
    workriver task show u1#3                  # what it was asked, what it delivered so far

Then one of:

- **Get it going:** what it is to do about what stopped it goes on its task, which tells it:
  `workriver task note u1#3 --text "<what to do>"` (`hand-off`); an agent idle at its prompt
  gets the message as its next turn. A command refused (a hook forbids push, a missing
  permission) that only the user can clear: have it put a blocker before its task, or put
  one yourself (`blocked`); else tell it, the same way, to deliver with what it did and the
  refusal in `--note`.
- **Hand it to another agent:** the task stays as it is; the new worker goes on with `u1#3`
  (no `task start`: it is already running), commits with `Task: u1#3`, and delivers it.
- **Drop it:** `workriver task drop u1#3 --reason "…"`, and re-plan.

It is gone (its terminal closed): hand the task to another agent or drop it, as above;
check its branch for what it committed first.

**You will see:** the board says `stalled since …` under the task until it is delivered or
dropped, or its agent works again; the user sees it in Needs you. You are told once per stop.
If you are the one that stopped, or the intent has no manager, only the user is told.
