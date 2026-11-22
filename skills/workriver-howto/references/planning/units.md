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

Units are added by the intent's manager (you, for an intent that runs in the workspace you
manage) or by the user, by nobody else: a worker that finds work for another unit tells
you. An intent that is planned and never ran, or runs in a workspace whose seat is empty,
has no manager: start it first (`start-intent`), or the user adds the unit. One put back in the
roadmap keeps the manager of the workspace it last ran in, who still adds its units.

A unit is built, reviewed and merged on its own, on its own branch. The branch is the
unit's own, never its base, and says what it holds: `feat/<name>` for new work,
`fix/<name>` for a fix, `chore/<name>` for the rest; the first unit's prefix follows the
intent's kind (`feature` → `feat/`, `fix` → `fix/`, `chore` → `chore/`; a `research` intent
starts with a research unit). Each code unit gets a git worktree of its own, next
to the repository, made from the base's newest commit:

    git -C <repo> fetch && git -C <repo> worktree add <repo>.worktrees/<unit> -b feat/<name> origin/main

The worker works there, never in the repository's own folder, which stays on the base:
two units of one repository in one folder would switch each other's branch and mix their
changes. The unit still names the repository (`--repo <repo>`, not the worktree): workriver
finds its branch in whichever worktree has it, uncommitted work included. A project folder
holding several repositories: one worktree folder next to each (`proj/front.worktrees/u3`),
never a worktree directly in the project folder (it would read as another repository).

    workriver unit add --title "Mentions" --purpose "Let people @ each other" --branch feat/mention [--base main] [--repo PATH]
    workriver task add u1 --title "Notify the people mentioned, with unit tests" --agent finder [--plan src/notify] [--text "…"]

`--text` is what the worker and the user read in the task's detail. Write it as short
Markdown: a line or two of what is wanted, then bullets (requirements, constraints, where to
start); no wall of text.

Work of the intent, not of one code unit (comparing approaches, a design across
repositories) is a **research unit**: no repository, branch or base, never merged. It takes
research, design, review and gate tasks; implementing goes in a code unit.

    workriver unit add --kind research --title "Sync engine" --purpose "Pick one for offline chat"
    workriver task add u2 --kind research --title "Compare sync engines" --agent scout

When its work is delivered and decided it is "ready to close": `workriver unit done u2
--reason "…"`. What the user should see on the intent's page (the decision, a link to the
report) goes into the intent's note: `workriver intent set i3 --add-note "…"` (a paragraph
added after what is there).

Finding out by trying (measure, compare, prototype) with code worth keeping to show how is
a **spike**: a unit with a `spike/<topic>` branch and its own worktree like a code unit,
any kind of task, never merged; closed with `workriver unit done <u> --reason "<what was
learned>"`. `workriver guide how spike`.

Tasks are always in a unit: `workriver task add i3 …` is refused; research and design go
into a research unit.

The first task of a unit starts from it. Every later one says what it follows: `--after
<n>` (the task it continues; repeat for several), `--after start` to run beside the first,
or `--cause` naming findings of a task in the same unit.

**You will see:** the unit id (`u1`), task refs (`u1#1`).

**Refusals:** "no intent given, and workspace ws1 has no current intent; its manager starts
one" - `start-intent`. "… is not a git repository" / "base … is not a commit or branch" - fix
`--repo` or `--base`. "i3 is an intent: tasks go into a unit … workriver unit add i3 --kind research …" - add a research
unit and the task there. "u2 is a research unit: it takes research, design, review and gate tasks" -
implementing goes in a code unit. "u2 is a research unit: …" - it has no branch to merge,
move or link commits to; close it with `unit done`. "u1 already has tasks: say which one this
follows with --after <n> …" - add `--after`, as above. "a unit's branch starts
with feat/, chore/ or fix/" / "a unit's branch must not be its base" - name the unit's own
branch so, as the message says. "only the user or the manager of intent i3 (boss) adds its
units" - not your intent: tell its manager (`workriver manager tell <your task> --text "…"`).
"i7 has no manager: units are added by the manager of the workspace it runs in, once it is
started, or by the user" - it never ran, or its workspace's seat is empty: start it first
(`start-intent`), or ask the user.
