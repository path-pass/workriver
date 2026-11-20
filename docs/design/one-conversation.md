# One conversation on the page (i29, u14)

The page keeps one conversation: the one with a workspace's manager. What the user has
picked (a task, a finding, lines, a file, an open attachment) and the files they drop or
paste go with the message to the manager, as records, not as text. The manager answers
itself, or asks the working agent with `workriver ask` (unchanged, for agents) and replies.
The page's Ask why (a temporary agent per question) goes away.

Why (the intent's note): Ask was used twice (2026-11-16); the user writes to the manager.
A temporary agent lacks the intent's context, is slow to start, and is a second entry.

## 1. What goes, what stays, where

| Today | After |
|---|---|
| Side section **Ask why** with the switch "Why \| Manager" | One section, **Manager** (`talk` icon, same place, last button). No switch. The workspace select stays (several workspaces with a manager). |
| Why mode: a question to a temporary agent about a task, a gate or the unit | Gone from the page. The page never calls `/api/ask`. |
| Ask's context chips and its files (drop, paste, attach button, 50 MB refusal line) | Move as they are to the Manager box. |
| Floating "Ask why" next to picked lines (`CodeLayer` `onAsk`, App's `askAbout`) | Same button, labelled "Ask the manager": opens the Manager section, cursor in the box, the lines as a chip. It still picks the task that last changed them and adds the others as chips (the manager gets who wrote them); lines no task changed: the unit's chip. |
| The viewer's "Ask why" (`ViewerContext.onAsk`) | "Ask the manager": the open attachment as a chip, as today. |
| `?` key; the task detail's "Ask why" button | `?` the same (lines, or the box). The detail's button becomes "Ask the manager". |
| Asking on a gate (went to the agent that asked for the gate) | The gate is a chip like any task; the manager relays to whoever wrote its steps. |
| Past threads on a task (the user's and agents') | Read-only in the task's detail, a **Questions** block after Output: folded rows, one opens, the chat as today (`ThreadRow`, `ThreadBody`), no box, no "Draft conclusion", no "End". Agents' new questions keep showing there, and in the node's speech-bubble count. |
| Past threads about a unit (task 0, only the user's) | The same block under the graph when no task is picked, only when the unit has any. |
| `/api/thread/end` (user-only, only the page called it) | Removed; temporary agents still close after their idle time (`prompts::IDLE`). The `ThreadEnded` event kind stays, for old records. |
| `/api/ask`, `QuestionAsked` and its rules, temporary agents, `workriver ask`/`answer` | Unchanged. The user's unit questions (task 0) become unused but stay valid, so old records replay. |

Context goes only with a message to the open unit's intent's workspace. With another
workspace picked in the select, no chips show and none are sent.

Every chip has a × (the task's and the unit's too: a message, unlike a question, needs no
subject). A chip left out stays out until the selection changes, as today.

## 2. The message's shape

**In the event.** `MessageSent` gains three fields, all `serde(default)` so old records
replay: `unit: String` (empty: none), `task: u32` (0: the unit, or none), `context:
Vec<AskContext>` (the existing type: `finding`, `file`, `lines`, `attachment`, and extra
`task` chips as labels `#n` of the same unit). `/api/message` takes the same three. The
state's `Message` keeps them; the workspace view shows them under the user's message as a
turn does (chips, then attachments as thumbnails and chips, opening in the viewer).

**Rules** (`prepare_messages`, `MessageSent`), refusals in the service's words:
- `unit` must be a unit (or an intent's own work) of the message's intent; with no
  intent named, of an intent of the named workspace. `task` must be one of its tasks.
- Context without a unit is refused.
- An `attachment` must be in the store and either stored by the user (`Attachment.user`)
  or shown with that task (with the unit: any of its tasks), `State::attachments_of`: the
  rule `QuestionAsked` has. Both call one function (moved into `conversations/`, e.g.
  `State::pointable(unit, task, id, user)`), not two copies.
- The manager it goes to on an undelivered blind task: refused ("… is on a blind task:
  write once it is delivered"), since nothing about other work is typed into a blind agent.

**Files.** The page stores each one with `POST /api/attachments` when the message is sent,
as Ask does now (`askWhy`'s `upload`, the same content once), and names the ids in
`context`. Not `/api/attachments/inbox`: that makes a task's staging folder for an agent and
has no part here. Held in the page until sent; one taken out is never stored.

**Limits**, as i23: 50 MB per file (`MAX_ATTACHMENT`, the route's body limit and the page's
refusal line); no count limit; the user's text cut at `quote::CAP` (2000 characters) in the
terminal, whole in the record and the page.

**In the manager's terminal.** `delivery::user_message` builds it from the record: one
paragraph, workriver's own words, then the user's text quoted.

    [workriver] The user writes to you from workriver's page (message m3, about intent i2
    "Search"). Reply with `workriver reply m3 --text "..."`; the user reads replies there,
    not in this terminal. It points at: task u1#3 "Paging" (implement, by worker-2); finding
    P2-1; lines 10-24 of src/api.ts; the attachment "shot.png" at
    ~/workriver-data/attachments/9f2c….png. Details: `workriver task show u1#3`.
    The user says, in workriver: «Why is the list not paged here?»

- What it points at is built by the same code as a question's (`prompts::pointed`, moved
  to `delivery/` and shared), from the record: unit and task from the state, paths and
  labels through `quote::name`, attachments by the store's own name and path. Nothing the
  user typed is outside «…».
- On another machine, `App::send` already turns each store path into
  `workriver attach fetch <id>.<ext>`; nothing new.
- The delivery record (`MessageDelivery.message`) holds this text, as now.

## 3. Security

- `/api/message` stays user-only: `App::user`, and `MessageSent` stays in
  `Body::user_only`. The new fields add no route.
- `machine_router` is unchanged: neither `/api/message` nor anything new is on it.
- Files only through the existing store route; the service names them only after the rule
  above, so a message cannot hand the manager a file the user could not point at.
- Text written by the user goes in only through `quote::users`; labels, paths and names
  through `quote::name`; a quote cannot be left from inside (`quote.rs`'s test).
- Reading: messages are not in `State::blind_view` (a blind agent sees none, so none of
  their context or attachments); `/api/attachments/{id}` keeps going through `App::view`,
  so a blind agent still fetches only what it attached.
- `docs/security.md`: one sentence under "What agents are told": a message from the page
  names what the user picked and the files they attached, as workriver's words; only the
  user's own text is quoted as theirs.

## 4. What else changes

**The manager** (`skills/workriver-howto/references/communication/user-message.md`): the
example message with "It points at: …"; open the paths (or run the fetch commands); if the
working agent must answer, `workriver ask <unit>#<n> "…" [--file p --lines a-b] --wait`,
then `workriver reply` with what it learned; a blind task in progress cannot be asked:
say it waits for its delivery. `skills/workriver-howto/CHANGES.md`, new date: "**Messages
from the page carry what the user picked and attached** (managers): the page has no Ask
why any more; answer, or ask the task's agent with `workriver ask` and reply.
(`user-message`)".

**Documents for agents**: `docs/design/ui/conversations.md` rewritten to this (the Manager
section, the box with chips and files, past threads read-only, agents asking each other);
`attachments.md` (the viewer's button), `code.md` and `navigation.md` (the section's name),
`activity.md` (the Questions block); `DESIGN.md`'s table line for `conversations.md`.
For people: `README.md` and `docs/README.zh-CN.md` (their Ask why paragraph, and the
task-detail screenshot that shows the button), in step. `docs/security.md` and its zh-CN
twin get the sentence of section 3. `workriver-manager/SKILL.md` needs nothing: it names
`workriver reply` and the recipe, not Ask why.

**i18n** (en, zh-CN, ja, in step): the section's title `talk.title` ("Manager", "管理者",
"マネージャー"); `ask.attach`, `tooLarge`, `dismiss`, `remove`, `send` move under `talk`;
"Ask the manager" (`talk.ask`) for the line button, the viewer and the detail; `talk.how`
says the files and chips go with the message. Removed: `ask.mode.*`, `ask.placeholder*`,
`ask.how*`, `ask.unitHint`, `ask.empty`, `ask.fresh`, `ask.conclude`, `ask.end*`,
`ask.label`. Kept for the read-only threads: `ask.step.*`, `ask.state.*`, `ask.exchanges`,
`ask.earlier`, `ask.conclusion`, `ask.asked`/`busy`/`ended`, `ask.cannot`, `ask.you`,
`ask.answerer`, `ask.log`, plus `ask.questions` for the block's heading.

**Tests that pin it**
- Core (`conversations/tests.rs`): a message with context is recorded with it; context
  without a unit, a unit of another intent, a task not in it, an attachment neither the
  user's nor shown with the task are each refused; a `MessageSent` recorded before (no new
  fields) replays; one to a manager on a blind task is refused.
- Service (`tests/conversations.rs`, `talking_to_a_manager`): the text given to the manager
  names the task, finding, lines and the stored file's path, with the user's text only in
  «…»; a path holding `»` cannot close the quote; to a manager on another machine the path
  is a `workriver attach fetch` command. `tests/views.rs`: a blind agent's view has no
  messages. `tests/guards.rs` and `route-access.txt`: `/api/thread/end` gone, nothing
  added to the machine listener.
- Web: `i18n/index.test.ts` (dictionaries in step); a unit test of the function that turns
  the selection into chips and the chips into `/api/message`'s body (lines over file, the
  extra tasks, nothing for another workspace); in the page (sample data), the line button,
  the viewer's button and `?` open the Manager box with the chips, a dropped and a pasted
  file show as chips and appear under the sent message.

## 5. Not done

- The ACP conversation page (i31): a richer, live conversation with an agent. This keeps the
  existing manager conversation and only widens what a message carries.
- No conversation with a working agent from the page: the manager decides whom to ask.
- The manager does not pass the user's files on as attachments of its `workriver ask`
  (the question rule lets only the user point at user-stored files); it names the path in
  its question's text.
- `/api/ask`, the temporary agents and the user's unit questions (task 0) are not removed
  from the service: agents use them, and old records must replay. Removing the unused
  user path is a later cleanup.
- No count limit on files and no new size limit.
