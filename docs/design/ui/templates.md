# Templates

A **template** is a named list of steps for one kind of unit (code, research or spike). A
unit is made with one, and its steps are placed in it as tasks nobody holds yet: the flow
still due is tasks on the board, not something the manager has to remember. The goal is
only that a step cannot be forgotten and that one left out is seen; nothing here stops a
manager that sets out to get around a step.

## The library

- The user's: added, changed and removed only by the user (`workriver template add FILE` /
  `remove NAME` from the user's terminal, or this page). Three are built in: `research` (a
  research unit: research, then the user's `direction` gate), `development` (a code unit:
  design, its architecture review `design-review`, the user's `confirm` gate, implement,
  then `code-review`, `security-review` and `architecture-review` beside each other, then the
  user's `go` gate) and `spike` (a spike: `experiment`, then `learned`; no gate). A template
  the user saves under a built-in's name replaces it whole; removing it brings the built-in
  back. A template of no steps is the user's alone to save (none is built in).
- The page and every CLI show what the service gives them (`/api/workspace`'s `templates`,
  `/api/state`'s for the CLI): the user's, then each built-in the user has none of, marked
  built in. Neither keeps a copy of the built-ins.
- A change touches only units made after it: a unit's steps are a snapshot, recorded in the
  unit's own event with the template's name and, for the user's, its revision. Whose words a
  step's description is (the user's or workriver's) is read from the unit, never from the
  template as it is now.

## The format

YAML: `name` (`[a-z0-9-]`, 1–32), `description` (what it is for and when to choose it; not
blank, at most 2000 characters), `unit` (`code`, `research`, `spike`; `code` when left out),
`steps` (0–20). A step: `id` (`[a-z0-9-]`, unique), `kind` (implement, review, tests, gate,
design, research), `description` (what kind of step it is; at most 1000 characters), and
where it applies `reviewer` (a review; code-review when left out), `gate` (a gate, required:
decide, deploy, test) and `after` (steps written earlier; left out: the step before; `[]`:
the start). A research template has no implement or tests step. Any other key is refused, and
every refusal names its line. The text as the user wrote it (comments and all) is kept to be
edited again; it is not in the state. Not in the format: conditions, parameters, an agent or
model per step, blind, rounds, inheritance, includes.

## The page

A dialog of its own, like Settings, reached from "Templates" at the rail's foot (an icon in
the folded rail) and from a unit's template name ("Template development", a tag in the
unit's heading, and the name in a step line or a gate's list), which opens it on that
template.

- Left: the library, each name with the kind of unit it is for and "built in" or "yours";
  "New template" under it.
- Right: the picked template (the one asked for, else the first): its kind of unit and whose
  it is, its description, its steps in order (id, kind as the graph names it, what it
  follows, its description), and "A change touches only units made after it".
- **Edit** opens a plain mono textarea with the text as saved (a built-in's own text when
  the user has none of that name) and Save; the service's refusal shows under it with its
  line ("Not saved: line 5: kind "build" is not one of …"). No editor library: the textarea is
  the platform's. Saving a built-in's name saves the user's template of that name. **New
  template** opens the same form on a short sample.
- One of the user's has **Remove**, asked first ("Remove template quick? Units made with it
  keep their steps."); one of a built-in's name has **Back to the built-in** instead, asked
  the same way. A built-in has neither.
- The page makes no unit, so it picks no template for one.

## On the work

- **The unit**: its heading carries the template's name as a tag (`Template development`).
- **A placed task**: on the graph, one nobody filled yet reads `#3 step confirm, to fill`
  (no title yet). Its detail opens with the step: "Step design-review · development (built
  in)", its description in the template's words, and while it is not filled "Not filled yet:
  its manager fills it, or leaves it out with the reason." What it asks is the manager's text
  once filled. A placed gate is not in Needs you until it is filled: it stays to do.
- **Every gate of a unit with placed steps** lists them, under "Template <name> (built in |
  yours)": *Done* (each step delivered, with its agent's name; a gate: "by you"), *Left out*
  (each dropped, with who and the reason as written) and *Not done* (each neither, gates
  aside). A placed gate lists the steps before it; one added by hand all of the unit's, so a
  gate put in front of the flow shows what it jumps. A blocker lists nothing. Waiting, the
  list sits above the gate's form, so the user reads it before answering; otherwise at the
  top of its output. The service works the list out (core's `State::gate_steps`) and gives it
  with the task (`gateSteps`); the page computes nothing.

## What the manager reads

The same on the board (`· step <id>, to fill` on a task, `· template <name>` on the unit, the
list under a gate not done yet) and in `task show` (the step above what it asks, the list on
a gate). Every notice the manager gets about such a unit ends with its steps still to fill;
adding or filling a gate, or closing the unit, while steps are not done is answered (not
refused) with which, and that a drop needs the reason the user will read there.
