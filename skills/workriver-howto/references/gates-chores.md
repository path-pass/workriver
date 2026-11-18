# Gates and chores

**A gate** is a step only the user can do and that work waits on (a manual deploy, a
try-out, confirming a design):

    workriver task add u1 --kind gate --gate deploy --title "Deploy to staging" --after 3 --step "Deploy => npm run deploy -- --env staging"
    workriver task add u1 --kind gate --gate test --title "Try it on a phone" --after 3 --step "Open the app, mention someone"

The user submits the result in the browser. **You will see:** "… finished gate …: done" (to
the agent waiting behind it, or you): go on. "found a problem": its findings are decided first.

**A chore** is something the user should do that blocks nobody (stuck on the user yourself: a
blocker, `blocked`):

    workriver chore add --title "Rotate the staging key" [--unit u1] --step "Open the console => open https://example.com"
    workriver chore add --project <path or name> --title "Renew the certificate"    # no intent

It prints the chore's id (`c1`).

**Commands in steps** (`TEXT => COMMAND`): the user's page has "Run" next to each, which runs
it as the user, in the user's own shell, in the unit's work tree (a chore with no unit: its
project's folder), after the user reads and confirms it. Write one exact line the user could
type there; nothing destructive the step does not need. Before it runs, a workriver agent
reads it and tells the user what it will do, so write the real commands, not a script that
hides them: a script is opened and explained, but the user reads the commands more easily.

**Refusals:** "only the user does this (gate.submitted)" / "(chore.done)": results and
ticking off are the user's.
