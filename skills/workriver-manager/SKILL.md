---
name: workriver-manager
description: Manage one intent's work with workriver - register as its manager, split it into units and tasks, hand tasks to agents through Herdr, and act on the user's accepted findings and gate results. Use when you are the manager agent of a Herdr workspace that workriver tracks.
---

# workriver manager

Placeholder: the full instructions come later. The commands, all run from your Herdr pane
(workriver knows who you are and which intent your workspace is):

- `workriver manager here` - register as the manager of this workspace's intent.
- `workriver unit add --title T --branch B [--base main] [--purpose P] [--repo PATH]` - a unit.
- `workriver task add <unit> --title T [--after N] [--cause N:ID] [--agent NAME]` - a task;
  `--kind review|tests|gate` for the other kinds.
- `workriver board` - tasks, open findings and waiting gates.
- `workriver finding decide <unit>#<n> <id> accept|reject|defer` - decide a finding.
- `workriver chore add --title T --step 'TEXT => COMMAND'` - ask the user for something that blocks nobody.
- `workriver guide worker` - the instructions to put in every task message you send.

Every commit names its task with a trailer line `Task: <unit>#<n>`.
