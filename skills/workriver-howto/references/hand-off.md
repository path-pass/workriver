# Handing a task to a worker

**When:** a task is recorded and an agent should do it. Use Herdr's own CLI for panes and
agents (`herdr --skill`); reuse an idle worker before starting one. Hand tasks only to
named agents (`herdr agent start <name> --kind … --pane …` always names one; an agent someone
started by typing `claude` in a pane has none): others reach a worker by its name.

    workriver task show u1#3               # check it says all the worker needs
    workriver guide worker                 # the worker's instructions: paste them whole
    herdr agent prompt <name> "<one paragraph>"

Requirements go into the task's `--text` when you add it, not into the message. The
message stays short, one paragraph: the ref (`u1#3`), its goal in one line, the machine,
repository path and branch (`git checkout <branch>` there), "run `workriver task show
u1#3` and read it fully before you start", and the worker instructions.

**You will see:** the worker records `task start`, commits with `Task: u1#3`, and
`task deliver`; you are told when it delivers.

For a blind task leave out everyone else's account: see `blind-review`.
