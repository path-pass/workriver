# workriver design

Direction set on 2026-11-15 following the Operate mode of the `impeccable` skill. The UI
lives in `frontend/web/`.

## Use scene

One developer at a desk, next to terminals running twenty or more agent sessions. They
look at this many times a day, for tens of seconds to a few minutes each time. It is a
tool, not a showcase: finding things at a glance comes first.

Light and dark both ship and follow the system. The terminals around it are mostly dark,
so the dark theme is composed on its own, not an inversion of the light one.

## Structure

Terms (after AWS AI-DLC where it fits): a **project** is a registered folder holding one or
more repositories (a repository is part of a registered project or of none: what names none
shows under "No project"); a **workspace** is where a project's intents run, workriver's own
record (`ws1`: an id, a name that is only shown and may repeat, one project), made by the
user, with one manager the user seats and one conversation with the user; it is not the agent
runtime's group, which is shown only as where a terminal is; an **intent** is a goal in
it, which may span its repositories; it groups the **units** that serve it, each a piece
that can be implemented on its own, on its own branch; a unit's work is a graph of
**tasks**, each one piece of work handed to one agent (implement, design, research, review)
or to the user (a gate). A task is drawn as a node. An intent is planned (in its project's
roadmap, in no workspace), active (started in an open workspace of its project, by the user
or by that workspace's manager), done or
dropped. A workspace may run several active intents; the one started last is its current
intent, where new work goes. Research and design that belong to the intent rather than to
one code unit are a **research unit**: a unit like any other in the rail, with no
repository, branch or code layer, never merged ("Ready to close" where a code unit says
"Ready to merge"; its manager closes it). Before research units such work was recorded on the
intent itself (`i3#1`); those tasks still work, and show on the intent's page as "Earlier
research and design", never in the rail. A **spike** is an experiment kept as code: a unit
with a `spike/` branch, shown like a code unit (commits, code layer) with a small "Spike" tag
in its rail line and heading, never merged ("Ready to close", no squash or merge reminder).

The rail is project → workspace → intent → unit.

Three columns: the rail picks a unit, the main area shows it, the side panel holds what is
looked at now and then.

## Where the rules are

This file is the overview: the use scene and the structure above, the rules every part
follows, and what is not done yet (below). The detailed rules of each part are in
`docs/design/ui/`; read the file of the part you change, and this one, before changing
anything in `frontend/`.

| File | What it holds |
|---|---|
| [`navigation.md`](docs/design/ui/navigation.md) | The rail (project → workspace → intent → unit), the manager select, an intent's page, editing an intent and its history, merging planned intents, the side panel and its Needs you, Agents and Backlog sections, signing in, addresses |
| [`activity.md`](docs/design/ui/activity.md) | The main area and its first layer (the graph), a node's detail, findings, gates and chores, what the user puts in, evidence, where the scope grew |
| [`code.md`](docs/design/ui/code.md) | The code layer, the Commits section, the view by function (postponed) |
| [`conversations.md`](docs/design/ui/conversations.md) | The manager conversation (what is picked and attached goes with a message), past questions, discussing a delivered result with its agent, agents asking each other |
| [`operations.md`](docs/design/ui/operations.md) | Running a command as the user, Machines, other machines, stopped workers, machine alerts, Settings (with its list of workspaces), opening a terminal |
| [`attachments.md`](docs/design/ui/attachments.md) | Attachments and the viewer |
| [`presentation.md`](docs/design/ui/presentation.md) | Colour, type, space and shape, the river lines, states, loading |

Where each section of this file went when it was split (moved whole, word for word but for
paths made whole and "see …" naming the file):

| Was here | Now in |
|---|---|
| Structure: the rail; an intent's page; merging planned intents; the side panel | `navigation.md` |
| Structure: Run a command; Running a command as the user | `operations.md` |
| Structure: Commits | `code.md` (Commits) |
| Structure: Backlog | `navigation.md` |
| Structure: Machines | `operations.md` |
| Structure: Needs you; Agents | `navigation.md` |
| Structure: the main area, its layer 1 (Activity) | `activity.md` (The main area) |
| Structure: the main area, its layer 2 (Code) | `code.md` (The code layer) |
| Structure: a second view by function (postponed) | `code.md` (A view by function) |
| Structure: picking a node, longer text as Markdown | `activity.md` (A node's detail) |
| Structure: Other machines; Stopped workers; Machine alerts; Opening a terminal | `operations.md` |
| Structure: Asking why (with the manager, and agents asking each other) | `conversations.md` (now the Manager section and past questions) |
| Colour means one thing | `presentation.md` |
| Findings; Gates and chores | `activity.md` |
| What the user puts in: the gate's form, an open finding, delivery | `activity.md` |
| What the user puts in: Attachments | `attachments.md` |
| Evidence: its first two paragraphs | `activity.md` (Evidence) |
| Evidence: diff colours and OKLCH tokens (its last two paragraphs) | `presentation.md` (end of the colour section) |
| Type; Space and shape; The signature: lines are a river; States; Loading | `presentation.md` |
| Where the scope grew | `activity.md` |
| Signed in or not; Addresses | `navigation.md` |
| Use scene; Structure's terms and columns; Rules that came out of iterating with the user; Not done yet | here |

## Rules that came out of iterating with the user

- Languages from the start: English and Chinese; Japanese ships too. Layout must hold in
  all three: English labels are longer, Chinese and Japanese ones are denser.

- If the interface can show it, do not also say it in text. Do not repeat what another
  part of the screen already shows.
- No "outside the plan". Every commit belongs to a node; one that names none is shown as
  unregistered, a gap to close, not a kind of work.
- The code is shown as a file tree, folder by folder. It is not a dependency graph and has
  no lines between files or folders.
- A reason is three structured fields: the problem found, what happens if it is left, and
  where (file paths, no code). They belong to a finding; work that answers findings names
  them and carries no reason of its own.

## Not done yet

- Contrast has not been measured pair by pair, only looked at in both themes.
- No loading state (the code layer is simply not there until its texts are read). The one empty state: with no units recorded, the main area is a short
  start guide (add the project, which comes with its first workspace; start a coding agent; seat
  it as the workspace's manager, in the rail or with `workriver workspace manager`; start a goal from
  the roadmap, after which the manager starts that project's; the commands copyable; the commit trailer; the import skill
  for an existing backlog file). An intent with no units yet keeps its
  rail group, open, with a quiet "No units yet".
- No syntax highlighting in the diff.
- Trackers: intents and findings carry an optional external id (`system`, `key`, `url`),
  shown as a link chip, not edited on the page (it is the tracker's to keep in step). Later a tracker adapter (a trait like the runtime one) imports items
  into the backlog and pushes status and links back; agents never talk to trackers directly.
