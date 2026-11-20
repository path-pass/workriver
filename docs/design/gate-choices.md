# A decide gate's choices

A question to the user with options ("merge and push / review again / not now, say why")
is a decide gate that carries its options as data. The page shows radio buttons; the user's
pick goes back to the manager as **answered**, with the option's id and the user's note.
The manager acts on it. No A2UI (see Not done). A design's own gate is not such a question
(section 2).

This file lives on the branch. Before merging, its rules go into `docs/design/ui/activity.md`
(the page), the recipes and `docs/design/layout.md` (new files), and this file is removed.

## 1. Data and CLI

**On the gate** (`NewTask`, recorded in `task.created`): an optional `choices`, the options
in the order shown. A gate with `choices` is a **choice gate**; the user picks one.

```json
"choices": [
  {"id": "merge",  "label": "核对通过后合并并推送"},
  {"id": "review", "label": "先再审一轮"},
  {"id": "hold",   "label": "先不合并", "needs_note": true}
]
```

- `id`: `[a-z0-9-]{1,32}`, unique in the gate; what is stored and sent back.
- `label`: plain text, one line, not blank, at most 200 characters.
- `needs_note`: the user must write a note when picking this option (default false,
  left out when false).
- 2 to 8 options. Core's rules (`prepare` of `task.created`) refuse anything else, and
  `choices` on anything but `--kind gate --gate decide`.
- No mode now: one pick. Multiple choice adds a mode field then, `#[serde(default)]` to one
  pick, so gates recorded before replay unchanged.

**The result** (`gate.submitted`): a third `GateResult`, `answered`, and `selected: [id]`
(one id now; a list because turning a string into a list later would break replay). Rules,
in core's `prepare_gates`, against the stored gate:

| Gate | `done` | `problem` | `answered` |
|---|---|---|---|
| without `choices` | as today | as today | refused: "… has no choices; submit done or problem" |
| with `choices` | refused: "… asks the user to choose; submit answered --select ID" | as today (findings: the question itself is wrong, no option fits) | exactly one id among the gate's options; a note when that option `needs_note`; no findings |

`selected` is refused on any other result. A second or late submit is refused as today (the
gate no longer waits).

**Replay.** The new fields are `#[serde(default, skip_serializing_if = …)]` on the events
(`choices`, `selected`) and on the replayed `Task` (`choices` left out when empty,
`selected` when empty). Old events carry neither, and the state they replay to serializes
as before: `fixtures/events.jsonl` and `expected-state.json` stay untouched.

**Commands.**

```sh
workriver task add u2 --kind gate --gate decide --after 7 \
  --title "u2 下一步？" --text "<what is ready, what was checked>" \
  --option 'merge=核对通过后合并并推送' \
  --option 'review=先再审一轮' \
  --option 'hold=先不合并' --require-note hold

workriver gate submit u2#8 answered --select review [--note "…"]   # the user only
```

- `--option ID=LABEL` repeats, in the order shown; split at the first `=`. One or more
  make the gate a choice gate (core then wants 2 to 8). `--require-note ID` repeats and must
  name an option: the one refusal the CLI makes before sending.
- `gate submit`: `answered` joins `done|problem`; `--select ID` goes with `answered` only.
  The `/api/gate` request (`server/work/mod.rs`, `Gate`) gains `selected`
  (`#[serde(default)]`), passed into `Body::GateSubmitted`.

**What `task show` prints** for a choice gate, after its text and steps:

```
Choices (the user picks one):
  merge   核对通过后合并并推送
  review  先再审一轮
  hold    先不合并 (a note required)
Output (submitted … by the user), result: answered
  Chose: review "先再审一轮"
  Note: …
```

`--json`: the task carries `choices` as stored and, once submitted, `result: "answered"`
and `selected: ["review"]`.

**What the page reads** (`/api/workspace`, built in `server/views/mod.rs` `task()`, types in
`common/wire/workspace.rs`):

- `TaskIn` gains `choices` (gates only, like `steps`; left out when the gate has none).
- `TaskOut` gains `selected` (left out when empty). The view carries no gate result today;
  `selected` present is what tells the page a choice gate was answered.
- `frontend/web/src/work/types.ts`: `input.choices?` and `output.selected?` to match.
- The page-JSON guard (`the_json_the_page_reads_keeps_its_shape`, `docs/design/guards.md`):
  its workspace gains an answered choice gate, so both key paths are pinned, and
  `page-json.txt` is regenerated as the deliberate change it is.

## 2. What an answer unlocks: nothing by itself

`answered` means "the question is answered", not "go". Whatever follows is the manager's
act, after reading the answer. Core holds it so (`prepare` of `task.created`):

- **Not on a design's gate.** A choice gate that follows a design (directly or through
  reviews, `designs_asked`) is refused: "a design is confirmed with a decide gate without
  --option; put the choices in --text". So `design_gate`, the "confirmed the design" message
  and `views/task.rs`'s `confirmed` never see one. A design's gate keeps its options in
  `--text`, and `done` confirms the design (with the user's note saying which way); that is
  also the gate an architecture review's third round ends in. Why: `done` there means
  "confirmed, build after me"; with options, only some answers would confirm, and the
  design's gate would need a per-option confirm (Not done).
- **Not a blocker.** `--option` with `--blocks` is refused: "a blocker carries no
  choices; tell your manager (`workriver manager tell`)".
- **Only the manager or the user asks.** `--option` from anyone else is refused, beside the
  blocker check in `prepare_tasks`: "choices are asked by the manager of intent i3 (or the
  user); tell your manager (`workriver manager tell`)".
- **Nothing waits on an open one.** A task created `--after` a choice gate that is not yet
  submitted is refused: "u2#8 asks the user to choose; add what follows once it is
  answered". Once it is submitted, `--after` it is allowed: the manager adds the task the
  chosen option calls for, and the graph shows it following the question. A task's inputs
  are fixed when it is created, so this one check covers every way to hang work after it.
  (`--cause 8:P1-1` after a `problem` is the same: the gate is already done.)

An option's words are no wider a go than the question: "merge and push" is the go for
those steps, still one task each (`push-pr-merge`).

## 3. The page

In `GateForm` for a waiting choice gate (`input.choices` present):

- The result toggle reads **Answer | Problem** (Answer first, selected); Problem is as today.
- Under Answer, the options as a radio group (shadcn's `radio-group` over the installed
  `radix-ui`, added to `ui/primitives/`; no new npm package), in the stored order, none
  picked at first. Labels are plain text (no Markdown, no links).
- Submit needs a pick; an option with `needs_note` marks the note as required and Submit
  needs it filled. A missing pick or note shows the error in place, like a problem without a
  finding.
- Kept as they are: the note (and its draft from a conversation), pictures (paste, drop,
  Attach), the undo window, the delivery line.
- The page sends the id only, never the label (`POST /api/gate`: `result: "answered"`,
  `selected`). The optimistic edit (`workspace/edits.ts`, which sets `output.selected`) and
  the message preview (`api/actions.ts`) learn the new result.

A submitted choice gate's output (`output.selected` present) shows "Chose: <label>", looked
up in `input.choices` by id, then the note and pictures. The node is done (water) like any
submitted gate. A gate without `choices` looks as it does today. All new UI text in en,
zh-CN and ja. One choice gate in `mock/demo.json`.

## 4. How agents learn it (the user's finding)

| Place | What it says |
|---|---|
| `skills/workriver-howto/CHANGES.md` | New dated entry (managers, workers): a question with options is a decide gate with one `--option` each; the page shows radio buttons; the answer comes as "answered" with the option; nothing waits on it until you add it; a design's gate keeps its options in `--text`. Names `ask-user`, `push-pr-merge`, `design-gate`, `architecture-review`, `gates-chores`, `blocked`. |
| `decisions/ask-user.md` | Two kinds of decide gate: a **go** (may I merge, deploy, change the goal: done or not confirmed; work may wait behind it) and a **choice** (several answers: `--option ID=LABEL` each, `--require-note` for options that need a reason). The example above. Never list options only in `--text` or a step, **except on a design's gate**: there the options stay in `--text` and done confirms the design (`design-gate`). "You will see" gains the answered message; "Refusals" gains the new ones. |
| `decisions/push-pr-merge.md` | When the next step itself is the open question (merge / review again / not now), ask it as a choice; the chosen option is the go for its own step only; add that task `--after` the gate. |
| `decisions/design-gate.md` | A design's gate is a plain decide gate: the choices the design leaves go in `--text`, the user confirms with done and says which in the note; `--option` there is refused. |
| `reviews/architecture-review.md` | The gate after a third round (its example `--after 8`, after a review) is a plain decide gate when what was reviewed is a design: options in `--text`. After a review of code it may be a choice gate. |
| `decisions/gates-chores.md` | A gate is something work waits on, **or a question nothing waits on** (a choice gate: `ask-user`). "You will see" gains the third result, answered with the option, beside done and found a problem. |
| `communication/blocked.md` | A blocker carries no choices. A choice the user must make: `manager tell` with the options; the manager asks it and tells you the answer. |
| `skills/workriver-manager/SKILL.md`, Hard rules | New rule: **Questions with options** to the user are a decide gate with one `--option` each (`ask-user`), never options typed into the text; **except a design's gate**, where they stay in `--text` and done confirms the design (`design-gate`). "Stop and ask the user" points at it. |
| `skills/workriver-manager/references/worker.md` (`guide worker`) | In 7): "a question with options: give your manager the options too". No more: workers cannot make choice gates (section 2). |
| `task add --help` (`cli/work.rs`, `TaskCmd::Add`) | `--option ID=LABEL`, `--require-note ID` documented, with "a decide gate's options, shown as radio buttons; not on a design's gate". `gate submit --help`: `answered`, `--select`. |
| The answered message (`delivery/messages.rs::gate_message`) | "[workriver] The user answered gate u2#8 (…): option `review`." The label quoted as the words of who recorded the gate (the manager, or the user), the note as the user's (`quote.rs`). Then: "Next: do what that option says, within the question (push, pull request, merge stay one task each); add its task with `--after 8`." Goes to the manager, like every decide gate today. |
| The hint on `task add` | See below. |

The "… asked the user for gate …; the work after it waits until the user submits it"
message (`delivery/mod.rs`) is sent only for a gate recorded by neither the manager nor the
user; core refuses a choice gate from anyone else, so that message never concerns one and
stays as it is.

**The hint.** The CLI, after a decide gate without `--option` is recorded, looks at the
title, text and steps it sent. If they name a choice (`选一个`, `选项`, `还是`, `choose`,
`pick`, `one of`, `option`, case-insensitive) or hold an inline list (`A／B／C`,
`A / B / C`, `A | B`), it prints one more line:

```
hint: this question lists options; ask it with --option ID=LABEL for each so the user picks one, unless it confirms a design (workriver guide how ask-user)
```

Never a refusal (a go question may say "or"; a design's gate keeps its options in the
text); a false hint costs a line. In the CLI, not the service: it reads only what this
command sent, and no rule depends on it.

## 5. Security

- **Submitting is the user's.** `gate.submitted` stays in `Body::user_only`: the page's
  `POST /api/gate` asks for `App::user`; the CLI's `gate submit` goes through `/api/append`,
  `App::who`, then the rule refuses anyone but the user. `answered` and `selected` change
  neither path.
- **Ids are checked against the stored gate** in core, never against what the page sends
  beside them; the label is never sent back and never trusted.
- **Only the intent's manager or the user records choices**, a core rule in
  `prepare_tasks` (section 2), so a label is never a worker's text shown under another
  name.
- **Labels are agent text** (or the user's). On the page, plain text. In messages, quoted
  through `quote.rs` as the words of who recorded the gate; the user's note as the user's.
  A label is never a command and never run.
- **No route added**, nothing on `machine_router`. Reads go through `App::view` as today;
  `choices` and `selected` are part of the task and follow its blind view.

## 6. Not done

- **Choices on a design's gate**: a design's question gets no radio buttons; its options
  stay in `--text`. `done` there means "confirmed, build after me"; with options only some
  answers would confirm, so it would need a per-option confirm and a design projection that
  reads the pick. Taken up with confirm below, when asked for.
- Multiple choice (a mode field, section 1) and confirm: when a real question needs them.
- A2UI: when an agent must produce a form beyond one question, or an external producer
  speaks it (research report: 128,588 B gzip against 10,408 B for Radix, and a
  spec/package mismatch). `choices` maps onto A2UI's `ChoicePicker` then.
- Choices asked by sandboxed workers with no terminal (planned apart): they need their own
  identity and a way back for the answer first. Until then a worker asks through its
  manager.
- Recognising options in old gates' text: old gates show as they were.

## Review findings answered

- **P2-1** (design questions vs. the hard rule): section 2 "Not on a design's gate";
  section 4 rows for `ask-user`, `design-gate`, `architecture-review`, `gates-chores`, the
  manager skill's hard rule, `task add --help`, the hint; section 6 first item.
- **P3-1** (where the data sits for the page): section 1 "Replay" (`Task` fields skipped
  when empty) and "What the page reads"; the `/api/gate` request under "Commands"; section 3.
- **P3-2** (who may record choices): section 2 "Only the manager or the user asks";
  section 5; section 4's answered-message row and the paragraph on the "asked the user"
  message (it cannot concern a choice gate, so it stays as it is).
- **P3-3** (no mode, no `--choice`): section 1: `choices` with no mode, one or more
  `--option` make a choice gate, a mode field comes with multiple choice; `selected` stays
  a list.
