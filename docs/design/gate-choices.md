# A decide gate's choices

A question to the user with options ("merge and push / review again / not now, say why")
is a decide gate that carries its options as data. The page shows radio buttons; the user's
pick goes back to the manager as **answered**, with the option's id and the user's note.
The manager acts on it. No A2UI (see Not done).

This file lives on the branch. Before merging, its rules go into `docs/design/ui/activity.md`
(the page), the recipes and `docs/design/layout.md` (new files), and this file is removed.

## 1. Data and CLI

**On the gate** (`NewTask`, recorded in `task.created`): an optional `choice`.

```json
"choice": {"mode": "single", "options": [
  {"id": "merge",  "label": "核对通过后合并并推送"},
  {"id": "review", "label": "先再审一轮"},
  {"id": "hold",   "label": "先不合并", "needs_note": true}
]}
```

- `mode`: only `single` now (one option picked). Kept so a later mode needs no new field.
- `id`: `[a-z0-9-]{1,32}`, unique in the gate; what is stored and sent back.
- `label`: plain text, one line, not blank, at most 200 characters; written by the agent.
- `needs_note`: the user must write a note when picking this option (default false,
  left out when false).
- 2 to 8 options. Core's rules (`prepare` of `task.created`) refuse anything else, and
  `choice` on anything but `--kind gate --gate decide`.

**The result** (`gate.submitted`): a third `GateResult`, `answered`, and `selected: [id]`
(one id for `single`; a list so a later mode keeps the shape). Rules, in core's
`prepare_gates`, against the stored gate:

| Gate | `done` | `problem` | `answered` |
|---|---|---|---|
| without `choice` | as today | as today | refused: "… has no choices; submit done or problem" |
| with `choice` | refused: "… asks the user to choose; submit answered --select ID" | as today (findings: the question itself is wrong, no option fits) | exactly one id among the gate's options; a note when that option `needs_note`; no findings |

`selected` is refused on any other result. A second or late submit is refused as today (the
gate no longer waits).

**Replay.** Both fields are `#[serde(default, skip_serializing_if = …)]`: old events carry
neither and replay as they were; `fixtures/events.jsonl` and `expected-state.json` stay
untouched. `Task` gains `choice` and `selected` from the events.

**Commands.**

```sh
workriver task add u2 --kind gate --gate decide --after 7 \
  --title "u2 下一步？" --text "<what is ready, what was checked>" \
  --choice single \
  --option 'merge=核对通过后合并并推送' \
  --option 'review=先再审一轮' \
  --option 'hold=先不合并' --require-note hold

workriver gate submit u2#8 answered --select review [--note "…"]   # the user only
```

- `--option ID=LABEL` repeats, in the order shown; split at the first `=`.
  `--require-note ID` repeats and must name an option. `--option`/`--require-note` without
  `--choice`, or `--choice` without `--option`, is refused by the CLI before sending.
- `gate submit`: `answered` joins `done|problem`; `--select ID` goes with `answered` only.

**What `task show` prints** for a choice gate, after its text and steps:

```
Choice (the user picks one):
  merge   核对通过后合并并推送
  review  先再审一轮
  hold    先不合并 (a note required)
Output (submitted … by the user), result: answered
  Chose: review "先再审一轮"
  Note: …
```

`--json`: the task carries `choice` as stored and, once submitted, `result: "answered"`
and `selected: ["review"]`.

## 2. What an answer unlocks: nothing by itself

`answered` means "the question is answered", not "go". Whatever follows is the manager's
act, after reading the answer. Core holds it so (`prepare` of `task.created`):

- **Not a design confirmation.** A choice gate that follows a design (directly or through
  reviews, `designs_asked`) is refused: "a design is confirmed with a decide gate without
  --choice". So `design_gate`, the "confirmed the design" message and `views/task.rs`'s
  `confirmed` never see one.
- **Not a blocker.** `--choice` with `--blocks` is refused: "a blocker cannot carry
  choices; tell your manager (`workriver manager tell`)".
- **Nothing waits on an open one.** A task created `--after` a choice gate that is not yet
  submitted is refused: "u2#8 asks the user to choose; add what follows once it is
  answered". Once it is submitted, `--after` it is allowed: the manager adds the task the
  chosen option calls for, and the graph shows it following the question. A task's inputs
  are fixed when it is created, so this one check covers every way to hang work after it.
  (`--cause 8:P1-1` after a `problem` is the same: the gate is already done.)

An option's words are no wider a go than the question: "merge and push" is the go for
those steps, still one task each (`push-pr-merge`).

## 3. The page

In `GateForm` for a waiting choice gate:

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
  `selected`). The optimistic edit (`workspace/edits.ts`) and the message preview
  (`api/actions.ts`) learn the new result.

A submitted choice gate's output shows "Chose: <label>" (looked up from the stored options
by id), then the note and pictures. The node is done (water) like any submitted gate. A gate
without `choice` looks as it does today. All new UI text in en, zh-CN and ja. One choice
gate in `mock/demo.json`.

## 4. How agents learn it (the user's finding)

| Place | What it says |
|---|---|
| `skills/workriver-howto/CHANGES.md` | New dated entry (managers, workers): a question with options is a decide gate with `--choice`; the page shows radio buttons; the answer comes as "answered" with the option; nothing waits on it until you add it. Names `ask-user`, `push-pr-merge`, `blocked`. |
| `decisions/ask-user.md` | Two kinds of decide gate: a **go** (may I merge, deploy, change the goal: done or not confirmed; work may wait behind it) and a **choice** (several answers: `--choice single --option …`, `--require-note` for options that need a reason). The example above. Never list options only in `--text` or a step. "You will see" gains the answered message; "Refusals" gains the new ones. |
| `decisions/push-pr-merge.md` | When the next step itself is the open question (merge / review again / not now), ask it as a choice; the chosen option is the go for its own step only; add that task `--after` the gate. |
| `decisions/design-gate.md` | One line: a design's gate is a plain decide gate; a choice gate after a design is refused. |
| `communication/blocked.md` | A blocker carries no choices. A choice the user must make: `manager tell` with the options; the manager asks it and tells you the answer. |
| `skills/workriver-manager/SKILL.md`, Hard rules | New rule: **Questions with options** to the user are a decide gate with `--choice` and one `--option` each (`ask-user`); never options typed into the text. "Stop and ask the user" points at it. |
| `skills/workriver-manager/references/worker.md` (`guide worker`) | In 7): "a question with options: give your manager the options too". No more: workers never make choice gates. |
| `task add --help` (`cli/work.rs`, `TaskCmd::Add`) | `--choice`, `--option ID=LABEL`, `--require-note ID` documented, with "a decide gate's options, shown as radio buttons". `gate submit --help`: `answered`, `--select`. |
| The answered message (`delivery/messages.rs::gate_message`) | "[workriver] The user answered gate u2#8 (…): option `review`." The label quoted as the manager's words, the note as the user's (`quote.rs`). Then: "Next: do what that option says, within the question (push, pull request, merge stay one task each); add its task with `--after 8`." Goes to the manager, like every decide gate today. |
| The hint on `task add` | See below. |

**The hint.** The CLI, after a decide gate without `--choice` is recorded, looks at the
title, text and steps it sent. If they name a choice (`选一个`, `选项`, `还是`, `choose`,
`pick`, `one of`, `option`, case-insensitive) or hold an inline list (`A／B／C`,
`A / B / C`, `A | B`), it prints one more line:

```
hint: this question lists options; ask it with --choice single --option ID=LABEL … so the user picks one (workriver guide how ask-user)
```

Never a refusal (a go question may say "or"); a false hint costs a line. In the CLI, not
the service: it reads only what this command sent, and no rule depends on it.

## 5. Security

- **Submitting is the user's.** `gate.submitted` stays in `Body::user_only`: the page's
  `POST /api/gate` asks for `App::user`; the CLI's `gate submit` goes through `/api/append`,
  `App::who`, then the rule refuses anyone but the user. `answered` and `selected` change
  neither path.
- **Ids are checked against the stored gate** in core, never against what the page sends
  beside them; the label is never sent back and never trusted.
- **Labels are agent text.** On the page, plain text. In messages, quoted through `quote.rs`
  as the manager's words; the user's note as the user's. A label is never a command and
  never run.
- **Who may add a choice gate** is who may add a gate today; a worker's only gate is a
  blocker, which cannot carry choices.
- **No route added**, nothing on `machine_router`. Reads go through `App::view` as today;
  `choice` and `selected` are part of the task and follow its blind view.

## 6. Not done

- Multiple choice (`mode: multiple`) and confirm: when a real question needs them.
- A2UI: when an agent must produce a form beyond one question, or an external producer
  speaks it (research report: 128,588 B gzip against 10,408 B for Radix, and a
  spec/package mismatch). `choice` maps onto A2UI's `ChoicePicker` then.
- Choices asked by sandboxed workers with no terminal (planned apart): they need their own identity
  and a way back for the answer first. Until then a worker asks through its manager.
- Recognising options in old gates' text: old gates show as they were.
