# Handing a task to a worker

**When:** a task is recorded and an agent should do it. Use Herdr's own CLI for panes and
agents (`herdr --skill`); reuse an idle worker before starting one.

    workriver board u1                     # the unit's machine, path, branch, and the task
    workriver guide worker                 # the worker's instructions: paste them whole
    herdr agent prompt <name> "<one paragraph>"

The message, as one paragraph, carries: the task ref (`u1#3`) and its goal; the machine,
repository path and branch as the board shows them (`git checkout <branch>` there); what it
follows and, for each cause, the finding with what was added to it; the worker instructions.

**You will see:** the worker records `task start`, commits with `Task: u1#3`, and
`task deliver`; you are told when it delivers.

For a blind task leave out everyone else's account: see `blind-review`.
