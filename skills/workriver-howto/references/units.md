# Splitting an intent into units

**When:** planning. Split as little as you can; the user does not review your split.

- **Intent**: a goal the user would name themselves (a feature, usually one or a few
  related PRs). Steps of the work are never intents of their own: never `backlog add` one
  for a step of the goal you are on, and add no intent the user did not ask for.
- **Unit**: by default an intent has exactly **one**. Add another only when work must land
  separately: in another repository, on another machine, or as a merge that has to happen
  on its own. Never split by layer or by step. More work for the same goal goes into the
  open unit as tasks.
- **Task**: one complete outcome for one agent ("implement mentions with their unit
  tests"), not one per step. Reviews and tests are tasks of their own.

A unit is built, reviewed and merged on its own, on its own branch. The branch is the
unit's own, never its base, and says what it holds: `feat/<name>` for new work,
`fix/<name>` for a fix, `chore/<name>` for the rest. Make it from the base's newest commit
(`git fetch`, then `git switch -c feat/<name> origin/main`).

    workriver unit add --title "Mentions" --purpose "Let people @ each other" --branch feat/mention [--base main] [--repo PATH]
    workriver task add u1 --title "Notify the people mentioned, with unit tests" --agent finder [--plan src/notify]

Work of the intent, not of one unit (comparing approaches, a design across repositories):

    workriver task add --kind research --title "Compare sync engines" --agent scout

The first task of a unit (or of the intent) starts from it. Every later one says what it
follows: `--after <n>` (the task it continues; repeat for several), `--after start` to run
beside the first, or `--cause` naming findings of a task in the same unit or intent.

**You will see:** the unit id (`u1`), task refs (`u1#1`, `i3#1`).

**Refusals:** "no intent given, and Herdr workspace … has no current intent" -
`start-intent`. "… is not a git repository" / "base … is not a commit or branch" - fix
`--repo` or `--base`. "implement and tests tasks belong to a unit". "u1 already has tasks: say which one this
follows with --after <n> …" - add `--after`, as above. "a unit's branch starts
with feat/, chore/ or fix/" / "a unit's branch must not be its base" - name the unit's own
branch so, as the message says.
