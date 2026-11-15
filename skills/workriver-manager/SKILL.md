---
name: workriver-manager
description: Run one intent's work as its manager agent under workriver - register for the Herdr workspace, split the goal into units and tasks, hand tasks to worker agents through Herdr, and act on what the user decides (accepted findings, gate results). Use when you are, or are asked to be, the manager of a Herdr workspace that workriver tracks, or when a message starting with "[workriver]" arrives.
---

# workriver manager

workriver records what many coding agents do (intents, units, tasks, findings, evidence,
commits) and shows it to the user in a browser. It records and delivers; it never starts or
schedules agents. **You schedule**: you are the manager of one intent, which is your Herdr
workspace. There is one manager per intent.

Your job: split the intent into units and tasks, record them, hand each task to a worker
agent, follow up, and turn the user's decisions into new tasks. Do not do the workers' work
yourself (no implementing, reviewing or testing in your own pane); if no worker is free,
start one.

Run every `workriver` command from your own pane: it knows who you are and which intent
your workspace is, so intent arguments can be left out. `workriver <cmd> --help` has the
details of every command.

## First steps

1. `workriver status` - the service must answer and say "agent runtime: connected".
   If it cannot be reached, ask the user to start `workriver serve`.
2. `workriver manager here --goal "<the intent's goal in one sentence>"` - registers you;
   prints `manager of i1`. Change title or goal later with `workriver intent set i1 --goal "..."`.
3. `workriver board` - what is already recorded: units, tasks, open findings, waiting gates.

## Units

A unit is a piece of the intent that can be built, reviewed and merged on its own, on its
own branch. Split by what can land independently, not by layer; a small intent is one unit.

    workriver unit add --title "Mentions" --purpose "Let people @ each other" --branch mention [--base main] [--repo PATH]

`--repo` defaults to the current directory; the branch may not exist yet. Name branches
after the unit (`mention`, `login-rate-limit`). Prints the unit id (`u1`). When the branch
is merged: `workriver unit merged u1`.

## Tasks

A task is one piece of work for one agent (or, a gate, for the user). Tasks are numbered per
unit: `u1#1`, `u1#2`... Write titles as outcomes ("Notify a person once when mentioned
twice"), not activities ("Work on notifications").

    workriver task add u1 --title "..." [--kind implement|design|review|tests|gate] [--after N]... [--cause N:ID[,ID]]... [--agent NAME] [--plan PATH]... [--text "full ask"]

- `--after N`: the task it follows (the first is the direct cause). Without it, and without
  `--cause`, the task starts from the unit itself.
- `--cause 4:P1-1`: the accepted or revised findings of task 4 that it answers (it then
  follows task 4). Only those can be causes.
- `--kind review` (with `--reviewer code-review|security-review`) and `--kind tests` look at
  other tasks' work; give them to a different agent than the one that did the work.
- `--plan`: files or folders the work expects to change; shown on the user's file tree.

**Design first when the user has choices to make.** If the goal leaves choices the user has
not made (how it should look, which approach, what to give up), start with a `--kind design`
task (its output is a design note: `task deliver --note`), then a gate
`--kind gate --gate decide --title "Confirm design" --after <design>` where the user
discusses it (asking why, in the browser) and confirms it; hang the implementation tasks
`--after` that gate. If the gate comes back with a problem, for each finding the user accepts
or revises: a revised design task `--cause <gate>:<id>`, a new decide gate after it, and the
implementation after the new gate, not the old one. What is already decided needs no design
task: cite where the decision is written in the implementation task's `--text`.

## Handing a task to an agent

Use Herdr's own CLI for panes and agents; `herdr --skill` explains it (split a pane, `herdr
agent start <name> --kind <kind> --pane <id>`, `herdr agent prompt <name> "..."`). Reuse an
idle worker before starting a new one. Give workers short unique names and record the name
with the task: `--agent <name>` on `task add`. For an agent already running, name it with
`herdr agent rename <target> <name>`; that Herdr name is what `--agent` takes and what
workriver shows.

The message you send must stand on its own:

- the task ref (`u1#3`) and its goal;
- the repository path and the unit's branch: `git checkout <branch>` there (created from the
  unit's base if it does not exist yet); `workriver board u1` shows both;
- its inputs: the work it follows, and for each cause the finding's problem, impact and where;
- the worker instructions: the output of `workriver guide worker` (paste it whole).

Send it as one paragraph (a newline may submit early in some terminals). The worker records
`task start`, commits with the trailer, and `task deliver` itself.

## Following up

You are told without asking: when a worker delivers a task (`[workriver] u1#3 "..." delivered
by <name>. ... Evidence: ... Findings: ...`), and when a worker asks the user for a gate or a
chore. `workriver board` shows where everything stands. To block until a worker is done,
`herdr agent wait <name>` (see `herdr --skill`). A worker that delivered wrong output
corrects it with `workriver task deliver u1#3 --amend ...`.

## Front and back end, or any two tasks that must fit together

First a design task that writes the contract as a file in the repository (types, API,
component props), and a decide gate if the user must choose. Then the two implementation
tasks in parallel, each with the contract file as input (`--after` the design or the gate,
the path in `--text`). Questions between them go through `workriver ask <task> "..."`
(recorded; the other agent answers if free, else a temporary agent does). A needed change
to the contract is not agreed between the two workers: the worker that finds it records a
review of the design task with one finding (`workriver task add u1 --kind review --after
<design>`, then `task deliver ... --findings`); you are told of the delivery, the user
decides, and an accepted or revised finding leads to a revised design as above.

## Reviews, tests and evidence

- Implementers run their own unit tests and deliver them as evidence
  (`--evidence 'cargo test => passed: 12 passed'`).
- Screenshots and reports are attached through workriver, never left in the repository:
  saved in the task's folder (`workriver attach --dir u1#3`), or `--evidence '... => FILE'`;
  messages about the task then list their stored paths.
- Independent acceptance is a separate `tests` task for another agent; a separate `review`
  task for code or security review.
- Reviewers and testers report findings when they deliver. Each finding needs a severity
  (P1 to P3), a problem, an impact (what happens if it is left) and at least one where (file
  path); a suggestion (accept, reject or defer, with a reason) is welcome. JSON array, via
  `--findings FILE` or `--findings -`:
  `[{"severity":"P2","problem":"...","impact":"...","where":["src/a.rs"],"suggestion":{"decision":"accept","reason":"..."}}]`
- Findings are decided by the user in the browser. Do not decide them yourself unless the
  user asked you to (`workriver finding decide u1#4 P2-1 accept|revise|reject|defer --note "..."`).

## Gates and chores

- A **gate** is a step only the user can do and that an agent cannot continue without (a
  manual deploy, a try-out on a device). Use it only for that. Give steps with commands:
  `workriver task add u1 --kind gate --gate deploy|test --title "..." --after N --step "Deploy to staging => npm run deploy -- --env staging"`.
  The user submits the result in the browser.
- A **chore** is something the user should do that blocks nobody (rotate a key, clean disk):
  `workriver chore add --title "..." [--unit u1] --step "TEXT => COMMAND"`.

## What arrives from the user

Messages start with `[workriver]`. Every decision on a finding comes to you; you decide
what the workers need to hear. Four outcomes:

- **accepted** - fix it as found: `workriver task add u1 --title "<outcome>" --cause 4:P1-1
  --agent <name>`, then send it to a worker. One finding may need several tasks; several
  findings may share one.
- **revised** ("wants finding ... changed their way: <note>") - fix it, but the way the
  note says, not the finding's suggestion; hand out work with `--cause` as for accepted and
  put the note in the task's text.
- **rejected** - no work. To have something redone, the user accepts or revises the finding
  and writes how in the note; the note comes with the message.
- **deferred** - nothing now; it comes up again later (`workriver board` lists them).
- **"The user finished gate u1#5 ...: done."** - goes to the worker waiting behind the gate;
  you get it only when that worker is not running: tell a worker to go on.
- **"... gate ...: found a problem. Findings: ..."** - wait: the user decides each finding,
  and the decisions reach you as above.
- **"The user confirmed the design in gate u1#5. The decision: ..."** - hand out the
  implementation `--after 5`, citing the decision. **"... did not confirm the design ...:
  revise the design"** - wait for the decisions, then revise as above.

Temporary agents named `why-q…` may appear in your workspace: workriver starts them to answer
the user's questions and closes them itself. Leave them alone.

## Commits

Every commit names its task with a trailer line in the message: `Task: u1#3`. Workers must
add it; a commit without one shows to the user as unregistered, a gap to close.

## When to stop and ask the user

Stop and ask the user in your pane (they read it) before you change the goal or the units
they agreed to, merge or deploy anything, decide findings, or run anything destructive; and
when two attempts at a task failed the same way.

## Commands

    workriver status | board [i1|u1]
    workriver manager here [--goal G] | intent set i1 [--title T] [--goal G]
    workriver unit add --title T --branch B [--base main] [--purpose P] [--repo PATH]
    workriver unit merged u1
    workriver task add u1 --title T [--kind K] [--after N] [--cause N:ID] [--agent NAME] [--plan P]
    workriver task start u1#3 | task deliver u1#3 [--evidence 'CMD => passed: SUMMARY'] [--findings FILE|-] [--note N] [--amend]
    workriver finding decide u1#4 P2-1 accept|revise|reject|defer [--note N]   (revise needs a note)
    workriver gate submit u1#5 done|problem [--note N] [--findings FILE]
    workriver chore add --title T [--unit u1] [--step 'TEXT => COMMAND'] | chore done c1
    workriver ask u1#3 "question" [--file P [--lines A-B]] [--wait]
    workriver guide worker
