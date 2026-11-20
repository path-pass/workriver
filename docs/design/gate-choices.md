# Choices: an A2UI v1.0 subset, for decide gates and the Manager conversation

A question to the user with options ("merge and push / review again / not now, say why")
carries its options as data. They are stored in one format, a small subset of A2UI v1.0,
whatever carries them. In the first version two things carry them: a decide gate, and a
manager's message in the Manager conversation. The page shows radio buttons. The user's pick
goes back to the manager as the chosen values: at a gate as the result **answered**, in the
conversation as a message from the user. Neither starts anything by itself. Managers never
write JSON: they give `--option ID=LABEL`, and workriver builds the stored form.

This file lives on the branch. Before merging, its rules go into `docs/design/ui/activity.md`
and `docs/design/ui/conversations.md` (the page), the recipes, and `docs/design/layout.md`
(new files). Then this file is removed.

## 1. The stored format

**A choice** is stored as one object, `{"surface": <A2UI message>, "needs_note": [values]}`:

```json
{"surface": {"version": "1.0", "createSurface": {
   "surfaceId": "choice",
   "catalogId": "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
   "components": [
     {"id": "root", "component": "Column", "children": ["options", "submit"]},
     {"id": "options", "component": "ChoicePicker", "variant": "mutuallyExclusive",
      "options": [{"label": "核对通过后合并并推送", "value": "merge"},
                  {"label": "先再审一轮", "value": "review"},
                  {"label": "先不合并", "value": "hold"}],
      "value": {"@path": "/selected"},
      "@checks": [{"condition": {"@call": "required", "args": {"value": {"@path": "/selected"}}}}]},
     {"id": "submit", "component": "Button", "variant": "primary", "child": "submit-label",
      "action": {"event": {"name": "answer", "context": {"selected": {"@path": "/selected"}}}}},
     {"id": "submit-label", "component": "Text", "text": {"@path": "/ui/submit"}}
   ],
   "dataModel": {"selected": []}}},
 "needs_note": ["hold"]}
```

**The subset.** It is pinned to protocol version `"1.0"` and the basic catalog's id. It
allows exactly these four components and properties, in exactly this shape:

| Component | Properties allowed (values as above) |
|---|---|
| `Column` | `id: "root"`, `children: ["options", "submit"]` |
| `ChoicePicker` | `id: "options"`, `variant: "mutuallyExclusive"` (one pick), `options: [{label, value}]` (labels literal strings, no binding), `value: {"@path": "/selected"}`, `@checks` with the one `required` rule |
| `Button` | `id: "submit"`, `variant: "primary"`, `child: "submit-label"`, `action.event` with `name: "answer"` and `context: {"selected": {"@path": "/selected"}}` |
| `Text` | `id: "submit-label"`, `text: {"@path": "/ui/submit"}` |

- A Column is needed because a surface has exactly one root, `id: "root"`, and the picker
  and the button are two components.
- The button's words are UI text, so they are never stored in one language. The renderer
  puts the i18n text at `/ui/submit` in its own copy of the data model before drawing. The
  same goes for the `required` rule's message, which is left out: the renderer shows its own
  i18n text.
- `needs_note` sits beside the surface, not in it. A2UI's option items allow only `label`
  and `value` (`additionalProperties: false`). A note required for some options is workriver's
  own rule, and the note itself is the carrier's field (the gate's note, the user's message
  text).
- Option rules: 2 to 8 options. `value` is `[a-z0-9-]{1,32}`, unique. `label` is plain text
  on one line, not blank, at most 200 characters. Every `needs_note` entry is one of the
  values.

**Building and checking it, in core.** One module, `workriver-core/src/choices/`, does both:

- `surface(options, needs_note)` builds the stored form. The CLI calls it to turn
  `--option`/`--require-note` into the body it sends.
- `options(&choice)` reads `(value, label)` and `needs_note` back out, for the CLI's
  `show`, the messages and the page.
- **Closed schema.** On record (`prepare`), core parses the choice into typed structs with
  `deny_unknown_fields`, where a component is an enum of the four kinds. It reads the
  options out, rebuilds the canonical surface from them and refuses the choice unless the
  two are equal: "a choice is stored as workriver's A2UI 1.0 subset; give it with
  --option". So any other component, property, binding, function, catalog or version is
  refused, whoever sends it (`/api/append` included).

**The answer** is the chosen values, `selected: ["review"]`: one value now, and a list
because turning a string into a list later would break replay. Core checks it against the
stored options:

- exactly one value, and it is one of the options' values;
- a non-blank note when the value is in `needs_note`.

Labels are never sent back. In A2UI terms the Button's action is
`{"version": "1.0", "action": {"name": "answer", "surfaceId": "choice", "sourceComponentId": "submit", "timestamp": …, "context": {"selected": ["review"]}}}`.
The page sends only `context.selected` (plus the carrier's note) through the carrier's
route. No action message is stored.

**Replay.** Every new field is `#[serde(default, skip_serializing_if = …)]`, on events and
on replayed state alike. Old events carry none of them and replay as they were, so
`fixtures/events.jsonl` and `expected-state.json` stay untouched.

**Sources relied on** (A2UI v1.0):

- [protocol](https://a2ui.org/specification/v1.0-a2ui/) and its
  [Markdown source](https://github.com/a2ui-project/a2ui/blob/main/specification/v1_0/docs/a2ui_protocol.md):
  `createSurface` with `surfaceId`, `catalogId`, `components`, `dataModel`; flat components
  `{id, component, …}`; the root `id: "root"`; `@path`; `@checks`.
- [messages](https://a2ui.org/concepts/messages/): `"version": "1.0"`, the `action`
  message's keys.
- [common types](https://raw.githubusercontent.com/a2ui-project/a2ui/main/specification/v1_0/json/common_types.json):
  `Checkable`, `CheckRule` (`condition`, optional `message`), `Action.event` (`name`
  required; `context` and `userMessage` optional), `DataBinding`.
- [basic catalog](https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json): the
  catalog id; `ChoicePicker` (`options` items `{label, value}`, `variant` enum, `value`
  DynamicStringList, Checkable); `Button` (`child`, `action`, `variant` enum, Checkable);
  `Text` (`text`, `variant`); `Column` (`children`); the `required` function (`args.value`).
- [basic catalog guide](https://a2ui.org/specification/v1.0-basic-catalog-implementation-guide/#choicepicker):
  `mutuallyExclusive` is drawn as single choice.

**Not verified:**

- The spec pages were read partly from summaries past their first ~40k characters. The
  `createSurface` keys and the action message come from verbatim text, but no full example
  was copied whole.
- The basic catalog file in the repository (`catalogs/basic/v1/catalog.json`) was not
  read, only its a2ui.org copy.
- No surface above was run through an A2UI validator. The research measured that the
  published `@a2ui/web_core` 0.15.0 rejects the spec's `@checks`; the stored form follows
  the spec, not that package.

## 2. The carriers

### 2a. Decide gates

- **On the gate.** `NewTask` gains an optional `choice` (section 1), in `task.created`. A
  gate with one is a **choice gate**. Core refuses `choice` on anything but
  `--kind gate --gate decide`.
- **The result.** `gate.submitted` gains a third `GateResult`, `answered`, and `selected`.
  Rules, in `prepare_gates`, against the stored gate:

| Gate | `done` | `problem` | `answered` |
|---|---|---|---|
| without `choice` | as today | as today | refused: "… has no choices; submit done or problem" |
| with `choice` | refused: "… asks the user to choose; submit answered --select ID" | as today (findings: the question itself is wrong, no option fits) | `selected` checked as in section 1; no findings |

  `selected` is refused on any other result. A second or late submit is refused as today.

- **Commands.**

```sh
workriver task add u2 --kind gate --gate decide --after 7 \
  --title "u2 下一步？" --text "<what is ready, what was checked>" \
  --option 'merge=核对通过后合并并推送' --option 'review=先再审一轮' \
  --option 'hold=先不合并' --require-note hold

workriver gate submit u2#8 answered --select review [--note "…"]   # the user only
```

  - `--option ID=LABEL` repeats, in the order shown, split at the first `=`. One or more
    make the gate a choice gate. `--require-note ID` repeats and must name an option.
    These two flags work the same on `task add`, `reply` and `say`.
  - The `/api/gate` request (`server/work/mod.rs`, `Gate`) gains `selected`
    (`#[serde(default)]`).
- **`task show`.** After the text and steps it prints:

```
Choices (the user picks one):
  merge   核对通过后合并并推送
  review  先再审一轮
  hold    先不合并 (a note required)
Output (submitted … by the user), result: answered
  Chose: review "先再审一轮"
```

  `--json` prints `choice` as stored, plus `result: "answered"` and `selected`.
- **What the page reads.** `TaskIn` (`common/wire/workspace.rs`) gains `choice` (gates only,
  like `steps`). `TaskOut` gains `selected`. Both are left out when empty; `selected`
  present is what tells the page the gate was answered. `work/types.ts` matches. The
  page-JSON guard's workspace (`the_json_the_page_reads_keeps_its_shape`) gains an answered
  choice gate, and `page-json.txt` is regenerated as the deliberate change it is.

### 2b. The Manager conversation

- **Asking.** `workriver say --text "…" --option … [--require-note …]` and
  `workriver reply m3 --text "…" --option …` carry a `choice`. `message.said` and
  `message.replied` gain an optional `choice`, checked as in section 1. The text is the
  question; it is required as today. Who may is unchanged: only a workspace's manager says,
  and only the manager a message went to replies.
- **Answering.** The page shows the choice under that message or reply. The user's pick is
  a message from the user: `POST /api/message` (`App::user`, `Body::MessageSent`), the same
  path and rules as typing one. `message.sent` gains an optional
  `answer: {"message": "m7", "reply": 2, "selected": ["review"]}`, where `reply` is the
  1-based reply of that message (left out for a `say` message). Its `text` is the user's
  note, and may be blank unless the value is in `needs_note`.
- **Core checks** (`prepare` of `message.sent`):
  - the message or reply exists and carries a `choice`;
  - it belongs to the conversation this message goes to (same workspace, same manager);
  - it was not answered before ("already answered; write to the manager instead");
  - `selected` is checked as in section 1.

  On replay, the asked message or reply records the answer: the answering message's id and
  `selected`.
- **What the manager gets.** The answering message is typed into the manager's terminal
  like any message from the user:

  > [workriver] The user answered your question m7: option `review`, «先再审一轮» (as you
  > wrote it). The user says «…».

  It ends with the usual "Reply with `workriver reply m9 …`". The label is quoted as the
  manager's words and the note as the user's, both through `quote.rs`.
- **It triggers nothing.** No task, no gate, no state other than the message. A decision
  that work waits on (merge, deploy, the goal, anything destructive) is still a decide gate.
  The conversation's choice is for questions while talking ("which of these do you mean?").
- **What the page reads.** The conversation's messages and replies gain `choice` and, once
  answered, `answered: {"by": "m9", "selected": [...]}`, both left out when empty. The
  page-JSON guard's manager conversation gains an answered `say` choice.

## 3. What an answer unlocks: nothing by itself

`answered` means "the question is answered", not "go". Whatever follows is the manager's
act, after reading the answer. For gates, core holds it so (`prepare` of `task.created`):

- **Not on a design's gate.** A choice gate that follows a design (directly or through
  reviews, `designs_asked`) is refused: "a design is confirmed with a decide gate without
  --option; put the choices in --text". So `design_gate`, the "confirmed the design"
  message and `views/task.rs`'s `confirmed` never see one. A design's gate keeps its options
  in `--text`, and `done` confirms the design, with the user's note saying which way. That
  is also the gate an architecture review's third round ends in. The reason: `done` there
  means "confirmed, build after me", and with options only some answers would confirm.
- **Not a blocker.** `--option` with `--blocks` is refused: "a blocker carries no choices;
  tell your manager (`workriver manager tell`)".
- **Only the manager or the user asks.** On a gate, `--option` from anyone else is refused,
  beside the blocker check in `prepare_tasks`: "choices are asked by the manager of intent
  i3 (or the user); tell your manager (`workriver manager tell`)". In the conversation the
  existing rules of `say`/`reply` already allow only the manager.
- **Nothing waits on an open one.** A task created `--after` a choice gate that is not yet
  submitted is refused: "u2#8 asks the user to choose; add what follows once it is
  answered". Once the gate is submitted, `--after` it is allowed: the manager adds the task
  the chosen option calls for, and the graph shows it following the question. A task's
  inputs are fixed when it is created, so this one check covers every way to hang work
  after it.
- **In the conversation**, an answer is a message (section 2b) and nothing else.

An option's words are no wider a go than the question: "merge and push" is the go for those
steps, still one task each (`push-pr-merge`).

## 4. The renderer

- **One component**, `frontend/web/src/content/choice/` (`Choice.tsx`, and `surface.ts` for
  the types and reading the subset). It sits beside Markdown and diagrams: it draws what an
  agent wrote. It takes the stored choice and draws the subset over the installed Radix: the
  ChoicePicker as a radio group (shadcn's `radio-group` over `radix-ui`, added to
  `ui/primitives/`, no new npm package), the Button, the Text. Labels are plain text (no
  Markdown, no links).
- **The carrier supplies the rest:**
  - what goes between the options and the button: a note field, required when the picked
    value is in `needs_note`; at a gate also the pictures;
  - what the button does, receiving `selected`.

  A missing pick or note shows its error in place, from i18n.
- **Each carrier:**
  - `GateForm`: the toggle reads **Answer | Problem** (Answer first). Problem is as today.
    The note's draft, the pictures, the undo window and the delivery line are kept.
  - The Manager conversation (`conversations/Turns.tsx`): the choice under the message or
    reply that carries it. After answering: "Chose: <label>" there. The user's message
    shows in the conversation as usual.
  - Later carriers (ACP, a stuck terminal) use the same component.
- **Answered.** A choice that was answered shows "Chose: <label>", looked up in the stored
  options by value. The optimistic edits (`workspace/edits.ts`) and the gate's message
  preview (`api/actions.ts`) learn the new result. All new UI text ships in en, zh-CN and
  ja. `mock/demo.json` gets one choice gate and one `say` choice.
- **Swapping in the official renderer later**, once a published `@a2ui/react` /
  `@a2ui/web_core` accepts the 1.0 spec's `@checks` (0.15.0 does not):
  - only `Choice.tsx`'s body changes: it hands the stored surface to the official
    `A2uiSurface` with a catalog of these four components, and maps the `answer` action's
    `context.selected` to the carrier's callback;
  - it is loaded only when a choice is shown, as its own chunk (`import()`), the way the
    mermaid frame's script is built apart from the app's bundle and fetched only when a
    diagram is drawn. It is not in a frame, because it draws controls the user uses in the
    page;
  - `surfaceId` is namespaced per carrier when handed over (`u2#8`, `m7`, `m3/2`), since
    every stored surface is `"choice"`;
  - **unchanged:** the stored format, core's checks, the routes, the carriers' props.
- **The library turned down for now:** `@a2ui/react` 0.15.0 with `@a2ui/web_core`. In the
  research's same-question build it was 128,588 B gzip for a four-component catalog, against
  10,408 B for the installed Radix radio group. It also rejects the spec's `@checks`.

## 5. How agents learn it (the user's finding)

| Place | What it says |
|---|---|
| `skills/workriver-howto/CHANGES.md` | New dated entry (managers, workers). A question with options carries one `--option` each, on a decide gate or on `say`/`reply`; the page shows radio buttons. The answer comes as "answered" at a gate and as a message in the conversation; nothing waits on it until you add it. A design's gate keeps its options in `--text`. It names `ask-user`, `user-message`, `push-pr-merge`, `design-gate`, `architecture-review`, `gates-chores`, `blocked`. |
| `decisions/ask-user.md` | Two kinds of decide gate: a **go** (may I merge, deploy, change the goal: done or not confirmed; work may wait behind it) and a **choice** (several answers: `--option ID=LABEL` each, `--require-note` for options that need a reason). Never list options only in `--text` or a step, **except on a design's gate**: there the options stay in `--text` and done confirms the design (`design-gate`). Options in the conversation are for questions nothing waits on (`user-message`). "You will see" gains the answered message; "Refusals" gains the new ones. |
| `communication/user-message.md` | `reply`/`say` with `--option` for a question while talking; the answer arrives as "The user answered your question m7: option …", a message like any other, to reply to. A decision that work waits on is still a decide gate. |
| `decisions/push-pr-merge.md` | When the next step itself is the open question (merge / review again / not now), ask it as a choice gate. The chosen option is the go for its own step only; add that task `--after` the gate. |
| `decisions/design-gate.md` | A design's gate is a plain decide gate: the choices the design leaves go in `--text`, the user confirms with done and says which in the note. `--option` there is refused. |
| `reviews/architecture-review.md` | The gate after a third round is a plain decide gate when what was reviewed is a design (options in `--text`). After a review of code it may be a choice gate. |
| `decisions/gates-chores.md` | A gate is something work waits on, **or a question nothing waits on** (a choice gate: `ask-user`). "You will see" gains the third result, answered, with the option. |
| `communication/blocked.md` | A blocker carries no choices. For a choice the user must make, `manager tell` with the options; the manager asks it and tells you the answer. |
| `skills/workriver-manager/SKILL.md`, Hard rules | New rule: **Questions with options** to the user carry one `--option` each, never options typed into the text: a decide gate when work waits on the answer (`ask-user`), `say`/`reply` when nothing does (`user-message`). The exception is a design's gate, where they stay in `--text` and done confirms the design (`design-gate`). "Stop and ask the user" points at it. |
| `skills/workriver-manager/references/worker.md` (`guide worker`) | In 7): "a question with options: give your manager the options too". Nothing more: workers cannot ask with choices (section 3). |
| `--help` (`cli/work.rs` `TaskCmd::Add`, `cli/mod.rs` `Reply`/`Say`) | `--option ID=LABEL` and `--require-note ID`: "options the user picks from, shown as radio buttons". On `task add` it adds "decide gates only, not on a design's gate". `Say`'s help keeps "a decision goes in a decide gate". `gate submit --help` gains `answered` and `--select`. |
| The gate's answered message (`delivery/messages.rs::gate_message`) | "[workriver] The user answered gate u2#8 (…): option `review`." The label is quoted as the words of whoever recorded the gate (the manager, or the user), and the note as the user's (`quote.rs`). Then: "Next: do what that option says, within the question (push, pull request, merge stay one task each); add its task with `--after 8`." It goes to the manager, like every decide gate today. |
| The conversation's answered message (`server/conversations/`, where the user's message is typed in) | As in section 2b. |
| The hint | See below. |

The "… asked the user for gate …; the work after it waits until the user submits it"
message (`delivery/mod.rs`) is sent only for a gate recorded by neither the manager nor the
user. Core refuses a choice gate from anyone else, so that message never concerns one and
stays as it is.

**The hint.** The CLI checks the title, text and steps it sent, in two cases: after a
decide gate without `--option` is recorded, and after a `say`/`reply` without `--option`.
If they name a choice (`选一个`, `选项`, `还是`, `choose`, `pick`, `one of`, `option`,
case-insensitive) or hold an inline list (`A／B／C`, `A / B / C`, `A | B`), it prints one
more line:

```
hint: this lists options; give them with --option ID=LABEL each so the user picks one, unless it confirms a design (workriver guide how ask-user)
```

The hint is never a refusal: a go question may say "or", and a design's gate keeps its
options in the text. A false hint costs a line. It lives in the CLI, not the service: it
reads only what this command sent, and no rule depends on it.

## 6. Security

The rules do not change because the format is A2UI.

- **Only the subset is accepted** (section 1), on every path that records a choice. The
  renderer draws only those four components. No function runs other than the `required`
  check, and nothing is fetched or opened.
- **Answering is the user's.**
  - At a gate: `gate.submitted` stays in `Body::user_only`. The page's `POST /api/gate`
    asks for `App::user`; the CLI's `gate submit` goes through `/api/append`, `App::who`,
    and the rule refuses anyone but the user.
  - In the conversation: the answer is a `message.sent`, also user-only, through
    `POST /api/message` and `App::user`.
- **Values are checked against the stored choice** in core. The label is never sent back and
  never trusted.
- **Only the intent's manager or the user records choices.** On gates this is a core rule
  (section 3); in the conversation it is the existing `say`/`reply` rules. So a label is
  never a worker's text shown under another name.
- **Labels are agent text** (or the user's). On the page they are plain text. In messages
  they are quoted through `quote.rs` as the words of whoever recorded them; the user's note
  as the user's. A label is never a command and is never run.
- **No route is added**, and nothing goes on `machine_router`. Reads go through `App::view`
  as today: a gate's choice follows its task's blind view, and a conversation's follows the
  conversation's.

## 7. Not done

- **Other components** (TextField, CheckBox, Row, Card, Image, …), other functions, and
  surfaces an agent writes itself. Only the four components, in the one shape, built by
  workriver.
- **Multiple choice** (`variant: "multipleSelection"`) and **confirm**: when a real question
  needs them. `selected` is already a list. Then the subset grows, and so does core's
  canonical builder.
- **Choices on a design's gate**: a design's question gets no radio buttons; its options
  stay in `--text`. `done` there means "confirmed, build after me", and with options only
  some answers would confirm. That would need a per-option confirm and a design projection
  that reads the pick.
- **ACP and terminal carriers** (a permission request's options, the keys of a stuck
  terminal prompt): they would store the same format and use the same renderer, but each
  has its own path back to the agent, which is not designed here.
- **The official renderer now**: see section 4 for when, and how it swaps in.
- Choices asked by sandboxed workers with no terminal (planned apart): they need their own
  identity and a way back for the answer first.
- Recognising options in the text of old gates or messages: those show as they were.

## The user's decision, mapped

The user took the manager's recommendation ("按你的推荐推进吧"): choices bound to gates
alone are too narrow.

- **The format is an A2UI v1.0 subset** (ChoicePicker, with Text and Button): section 1,
  plus Column as the one root, which is required.
- **Gates and the Manager conversation store the same structure**, and later ACP and
  terminal keys: section 2 (2a, 2b); later carriers in sections 4 and 7.
- **Managers write no JSON; the CLI still takes `--option`, and workriver converts it**:
  section 1 ("Building and checking it") and section 2a ("Commands").
- **The page draws the subset with the project's own Radix, and swaps to the official
  renderer when the package and the spec agree, loaded only when used**: section 4.
- **Security does not change because of A2UI**:
  - the service accepts only the subset: sections 1 and 6;
  - a pick is matched only against the stored values: sections 1 and 6;
  - only the user submits: section 6;
  - a gate's "answered" still unlocks nothing: section 3;
  - a choice in the conversation is only a message: sections 2b and 3.

The review findings of the earlier version still hold where they were answered. A design's
gate: section 3 and section 7. The page's fields: section 2a. Who may ask: sections 3 and 6.
No mode field and no `--choice` flag: one or more `--option` make a choice, and the subset's
`variant` stands for the mode.
