# Stuck on what only the user can do: a blocker

**When:** your task cannot go on until the user does something: a command was refused (a hook
forbids push, a sandbox denies a write), a permission or a login is missing, a key must be
set. Do not wait to be reminded and do not only say it in your pane: the user does not watch
it. Put a blocker before your task, at once:

    workriver task add u2 --kind gate --gate test --blocks 5 --title "Allow git push in this repository" --text "git push origin feat/x was refused: pre-push hook 'no-push' (exit 1)" --step "Allow it => git config --local hooks.allowpush true"

A blocker is a gate put before a task still to do or running: it waits for the user at once
(Needs you, "blocking"), stands before your task on the graph, and your task now follows it.
`--blocks` takes the task number (`5` or `u2#5`); without `--after` the gate follows what
your task follows. Say in `--text` exactly what was refused and why, and give the user steps
they can run (`TEXT => COMMAND`, see `gates-chores`). Use `--gate test` for "try / allow
something", `deploy` for a deploy, `decide` when the user must choose.

**You will see:** "… finished gate …: done" with the user's note: go on. "found a problem":
read the note; if you still cannot go on, deliver what you did with the refusal in `--note`.
Your manager is told when you put the blocker and may re-plan (drop it, hand the task on).

**Who:** the agent of the blocked task, the manager of its intent, or the user. Anyone else is
refused: tell the manager instead,

    workriver manager tell u2#5 --text "u2#5 is stuck: ... ; it needs a blocker"

`workriver manager tell <task> --text "…"` is how any worker reaches its manager (a renamed
branch, a contract that seems wrong, being stuck on a blind task): the text is typed into the
manager's terminal, quoted, while it runs. Never type into another agent's terminal.

**Not a blocker:** something the user should do sometime that holds nobody up is a chore
(`gates-chores`). A question about another task is `workriver ask` (`ask`).

**Managers:** a worker's blocker comes as "… is stuck on u2#5 …: it put gate u2#7 …", with
what it asks the user quoted. Nothing to do unless the plan changes; a worker waiting on its
blocker is not reminded as stopped. A blocker no longer needed is dropped like any task
(`workriver task drop u2#7 --reason "…"`); the blocked task then waits on it no more.
