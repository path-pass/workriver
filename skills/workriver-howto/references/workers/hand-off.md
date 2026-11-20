# Handing a task to a worker

**When:** a task is recorded and an agent should do it. Use Herdr's own CLI for panes and
agents (`herdr --skill`); reuse an idle worker before starting one. Hand tasks only to
named agents (`herdr agent start <name> --kind … --pane …` always names one; an agent someone
started by typing `claude` in a pane has none): others reach a worker by its name.

    workriver task show u1#3               # check it says all the worker needs
    workriver guide worker                 # the worker's instructions: paste them whole
    herdr agent prompt <name> "<one paragraph>"

Requirements go into the task's `--text` when you add it (short Markdown: see `units`), not
into the message. The
message stays short, one paragraph: the ref (`u1#3`), its goal in one line, the machine,
the unit's worktree and branch (`units`: "work in <repo>.worktrees/u1 on feat/x"; never the
repository's own folder), "run `workriver task show
u1#3` and read it fully before you start", and the worker instructions.

**You will see:** the worker records `task start`, commits with `Task: u1#3`, and
`task deliver`; you are told when it delivers.

For a blind task leave out everyone else's account: see `blind-review`.

## A worker's commands must run in its own pane

workriver takes the agent from the pane a command's **process** really runs in (the kernel
tells the service which process calls; the pane a command names is only cross-checked). So
a worker's `workriver` commands must be run by the worker itself, inside its pane.

Some harnesses hand their shell commands to a shared background service that was started
once, somewhere else, and keeps that place's environment. Such a worker is refused on its
first `workriver task start`:

- "this command says it runs in pane X (there is no pane X any more), but its process …
  runs in pane Y / in no pane …": the command ran elsewhere than it says.
- "process … is in no pane and has no terminal …": the command ran outside every pane.

What to do, for any harness: close that worker and start it again so that it runs its
commands itself, as a child of its pane's shell. Look in the harness's own `--help` for the
option that makes this run work without its shared background service, and pass it when
you start the worker (Herdr passes what follows `--` to the harness), and again when a
session is resumed:

    herdr agent start <name> --kind <harness> --pane <id> -- <that option>

Check it before handing out work: in the new worker, `workriver task show <task>` answers
instead of being refused. If the harness has no such option, its workers cannot record
from that harness: tell the user (`workriver chore add`). Nothing in the user's own
configuration has to change, and the background service may keep running for their other
sessions.

Other refusals of the same check:

- "session S was first seen in pane X, whose shell still runs; one session is one pane …":
  the worker's session was resumed in a second pane. Close the first pane (`closing-workers`),
  or start a new session in the second.
- "session S shows in pane X and in pane Y at once …": the same; close one.
- "this request does not show which process sends it …": something other than this build's
  `workriver` CLI called the service (a `curl`, an older CLI: `updating`).

Never work around a refusal by setting or unsetting `HERDR_PANE_ID`, or by running the
command from another pane: it is refused there too, or recorded as someone else.
