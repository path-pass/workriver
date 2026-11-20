# Closing workers you no longer need

**When:** a worker you started has nothing more to do. Idle agents hold memory, and twenty
of them slow the user's machine; a fresh one starts in seconds when there is work again.
workriver never closes them: you do. Close one when

- its unit is merged or done, or the intent is done, and you have no other task for it;
- it sits idle with no task of yours now or soon (start a fresh one when work comes);
- it was a blind reviewer and it delivered: the next blind review needs a fresh agent anyway.

Never close an agent you did not start: the user's own panes, another manager's workers,
the temporary `why-qN` agents answering questions (workriver closes those).

First, uncommitted work. Its work tree is the unit's repository, or its branch's worktree
(`git -C <repo> worktree list`):

    git -C <path> status --short

Anything listed: tell the worker to commit it (with `Task: u1#3`) or say why not, and wait.
Then close its pane:

    herdr agent get <name>                 # its pane id
    herdr pane close <pane_id>

**You will see:** `[workriver] u2 is merged: ja-impl, ja-test worked on it and are still
running with no other task; close them if not needed …` when a unit is merged or done or the
intent is done, followed by "Tell the user with `workriver say` …" (`closing-units`); with no
worker to close, the message is only `[workriver] u2 is merged.` and that sentence; and
`[workriver] ja-test has been idle for 31 min with no task (its last: u2#4 …)` once per
idle period, for a worker idle 30 minutes (the user's `idleMinutes`) with no
task to do or being done. Each is a reminder, not an order: keep a worker you are about to
give work.
