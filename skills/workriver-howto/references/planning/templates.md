# Templates: a unit's steps

**When:** making a unit (every new unit names a template), filling or leaving out a step it
placed, or a command answered "Steps of template … not done".

A template is a named list of steps for one kind of unit (code, research or spike). The
unit you make names one, and its steps are placed in it at once as tasks nobody holds yet:
the flow still due is tasks on the board, not something to remember. The library is the
user's (they add, change and remove templates); three are built in:

- `research` (a research unit): research, then the user's `direction` gate.
- `development` (a code unit): design, its architecture review (`design-review`), the
  user's `confirm` gate, implement, then `code-review`, `security-review` and
  `architecture-review` beside each other, then the user's `go` gate.
- `spike` (a spike): `experiment` (implement), then `learned` (research); no gate, closed
  with `unit done`.

**Choose** by the descriptions: they say what each is for and when to use it.

    workriver template list --unit code     # names, descriptions, steps (--unit research | spike)
    workriver template show development     # each step: kind, what it follows, what it is for
    workriver unit add i3 --title "Faster sync" --branch feat/sync --template development

A unit without `--template` is refused, with the templates that fit its kind. The template
never changes afterwards: a wrong pick is put right by dropping what does not apply (each
with its reason) and adding what is missing by hand, or by closing the unit (`unit done
--reason`) and making another. `unit add` prints the tasks it placed:

    u23  placed from template development (built in):
      #1 design · step design, to fill
      #2 review (architecture-review) · step design-review, to fill
      …

**Fill** each step when its turn comes, as you would hand out a task: what it asks and who
does it. You become its author. Its kind, reviewer and gate are the template's.

    workriver task fill u23#1 --agent designer --title "Shape the sync batching" --text "…"
    workriver task fill u23#3 --go --title "Confirm the sync design" --text "the choices: …"

`task fill` takes the flags of `task add` that apply (`--title`, `--text`, `--agent`,
`--plan`, `--step`, `--go`/`--option`, `--blind`) and `--after N`, which adds to what the
step follows (what the template placed it after stays). A step not filled is not started,
delivered or noted ("… is a step of the template, not filled yet"). Then hand it off as
any task (`hand-off`); the worker's `task show` names the step and what it is for.

**A placed gate is filled last**, after the last round of reviews, with `--after` that
round: it is filled once and opens (asks the user) as soon as what it follows is done or
dropped. Filled early, it opens when the first round is delivered, before the fixes and the
next round, and cannot be moved behind them (only dropped and added again by hand). A
second round of review, a fix, the push and pull request are tasks you add as usual (`task
add --after …`); the template sets the least.

**Leave a step out** only with its reason, which the user reads on every gate of the unit:

    workriver task drop u23#6 --reason "only the batching changed: no input, access or secret"

Never put a task of your own in a step's place: fill the step, or drop it and say why.

**What the user sees.** Every gate of a unit with placed steps lists them under
"Template <name>": done (by whom), left out (who, why), not done. A placed gate lists the
steps before it; a gate you add by hand lists all of them, so a gate put in front of the
flow shows what it jumps. A placed gate is not in the user's Needs you until it is filled.
A unit with a step neither done nor dropped is never ready.

**You will see:** every notice about the unit ends with "Steps of u23 still to fill: #4
code-review, #5 security-review, #7 go (…)". Adding or filling a gate, or closing a unit,
while steps are not done answers (not refuses) "Steps of template development not done:
architecture-review (#6); gate u23#7 lists them to the user as not done. Fill each …, or
drop it with the reason …".

**Refusals:** "a unit names its template: `workriver unit add ... --template NAME` …" - pick
one from the list it prints. "template development is for a code unit; this is a research
unit" - one made for the unit's kind. "u23#2 is a step of the template (design-review), not
filled yet" - fill it first. "u23#1 is already filled" - add to what it asks with `task
note`. "a step of a template is filled by the manager of intent i3 or the user" - not
yours: tell the manager. "only the user does this (template.set)" - the library is the
user's: tell the user what template you would want (`workriver template add FILE`, or the
Templates page).
