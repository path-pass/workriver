# Handing a task to a worker

**When:** a task is recorded and an agent should do it. Use Herdr's own CLI for panes and
agents (`herdr --skill`); reuse an idle worker before starting one. Hand tasks only to
named agents (`herdr agent start <name> --kind … --pane …` always names one; an agent someone
started by typing `claude` in a pane has none): others reach a worker by its name.

    workriver task show u1#3               # check it says all the worker needs
    workriver guide worker                 # the worker's instructions: paste them whole
    herdr agent prompt <name> "<one paragraph>"

Requirements go into the task's `--text` when you add it, or fill it (`task fill`, for a
step its unit's template placed: `templates`; short Markdown: see `units`), not into the
message. A step not filled yet cannot be started: fill it before you hand it off. Its `task
show` begins with "Step <id> of template <name> (built in | the user's): <what kind of step
it is>", above what it asks: the step says what kind of work this is; what to do is your
text. The
message stays short, one paragraph: the ref (`u1#3`), its goal in one line, the machine,
the unit's worktree and branch (`units`: "work in <repo>.worktrees/u1 on feat/x"; never the
repository's own folder), "run `workriver task show
u1#3` and read it fully before you start", and the worker instructions.

**You will see:** the worker records `task start` (with `--model`: the model it works
with), commits with `Task: u1#3` (in the message's last paragraph, with any other trailers
and no blank line between them), and `task deliver`; you are told when it delivers.

Which harness and model did a task is in `task show` (under `Agent:`) and in the task's
detail on the page; nothing else of the session is shown there. Where the harness keeps
the session is recorded: `workriver task session u1#3` prints the path of the session
file, on the machine the agent ran on, for you or the user to open with the harness's own
tools (`task show --json` has it too, under `trace`, with the session id). workriver never
reads that file, and it reads only what the task is open to (a blind task's agent: its
own). It is there once the task is started or delivered, for the agent recorded with
that; a task still to do shows none. A worker that did not say its model shows "model
(not given)"; the model is only ever the worker's own word, so a start you record for it
(`task start u1#3 --agent impl`) takes no `--model`. A path "worked out, not seen there"
is where the file would be by the directory the agent was started in, not one found; a
session whose working directory was never seen, or a harness workriver does not know,
says why instead of a path.

For a blind task leave out everyone else's account: see `blind-review`.

## More for a task already handed out

A requirement that comes up after the task was recorded (the user asks for more, you
thought of something) is added to the task, never typed into the worker's terminal:

    workriver task note u1#3 --text "Also cover the Codex adapter" [--attach spec.md]

It is kept under what the task asks, with who and when, in `task show` and on the page. A
worker on the started task is told at once ("Your task u1#3 … has an added requirement",
your text quoted; a blind task's worker too: it is its own task; of another task of its,
only once it delivers the blind one, and not at all if that task was dropped or the worker
closed meanwhile) and reads it with `task show`; one not started yet
reads it when it starts. The command says whether it was told; "not told yet: … is not
running" (or waits at a dialog): it is held and told once that worker runs and can take it,
on whichever machine, so do not add it again. A worker that is gone is replaced
(`stalled`). Only you and the user add one, while
the task is not delivered or dropped; for a delivered task record the rest as new work
(`workriver task add u1 --after 3 ...`). An addition to a finding is `workriver finding
note` (`finding-additions`).

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
from that harness: tell the user (`workriver chore add <intent> --title "…"`). Nothing in the user's own
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

## A hosted worker

A worker the user started as a hosted agent (`workriver agent list` shows it; no terminal)
takes its hand-off the same way, with workriver's own commands instead of Herdr's:

    workriver agent prompt <name> --text "<one paragraph>"   # it reaches it as "From \"<you>\" (your manager): …"
    workriver agent read <name> --after <seq>               # its conversation, a page at a time
    workriver agent status <name>                           # its state and what waits for the user
    workriver agent cancel <name>                           # stops its turn

Only the user starts, answers and closes one; you prompt, read and cancel only the workers
of your workspace, and none that holds a manager's seat. A prompt is refused while it is
in a turn or waits for an approval: try again once `agent show` says `idle`. Its
`workriver` commands work from its own tools as from a terminal; one run from a job that
left its tools' process tree (a detached daemon) is refused.

