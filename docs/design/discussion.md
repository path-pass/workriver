# Discussing a delivered research or design result

The user follows up on a delivered research or design task in place, in rounds, and the
agent that did the task answers there. No task and no gate per round; a gate only when work
must wait on what was concluded.

This file lives on the branch. Before merging, its rules go into
`docs/design/ui/conversations.md` (the discussion, and its "Why one" and "Deliberately not
done", which this narrows), `docs/design/ui/activity.md` (a node's detail),
`docs/design/ui/navigation.md` (Needs you), `docs/security.md` (what only you can do), the
recipes and `docs/design/layout.md`. Then this file is removed.

## Why

On one research unit the loop was: research task, report, decide gate, the user's finding,
another research task, another gate. Every round cost a task, a hand-off and a delivery. The
manager now relays the user's follow-ups to the researcher through the Manager conversation
instead (no product change). That works, but each round passes through the manager's
terminal twice, the answers end up in the manager's conversation rather than with the
result, and the gate and later tasks still read the report as first delivered.

## 1. The shape: a discussion on the delivered task, built on question threads

**Decision.** A delivered research or design task can be **discussed**: the user writes in
its detail, the task's own agent answers, and the rounds are kept with the task as one
question thread. It is option (a), built from what exists, not a new mechanism:

- `Question` and `Thread` (core, `conversations/questions.rs`) already hold rounds about a
  task, follow-ups in one thread, a request for the conclusion (`conclude`) and the
  conclusion itself (`Thread.conclusion`). The task's own agent may already answer
  (`QuestionAnswered`'s `own`), and `QuestionStarted` has the field for an answerer that
  is not temporary (`temporary: false`; nothing sends it yet).
- `App::hand` already holds a message while its agent is blocked, not ready, or (for the
  user's messages) on a blind task.
- The task's **Questions** block already shows threads as a chat with their conclusion.

What is added is small: a flag that makes a thread a discussion, a user-only route to ask in
it, the answer going to the task's own agent instead of a temporary one, a box in the
Questions block, a read mark, and one message to the manager at the conclusion.

**Why not (b), a discussion node on the graph.** A node is work handed to someone, with a
state that the graph's flow, counts and colours follow. A discussion is talk about work
already done: it would add a task kind, rules for what waits on it and how it counts, and a
node per round or a node with rounds inside, for nothing a thread lacks. What work must wait
on is already a node: the decide gate.

**Why not only the manager's relay** (what is done now): two hops per round through the
manager, the manager's context spent on relaying, and the answers kept in the Manager
conversation, apart from the result they change.

**Why not a "to agent" switch in the Manager box:** that is a conversation with any working
agent from the page, which `conversations.md` deliberately does not do (the manager decides
whom to ask). The discussion is narrower: only the author of a delivered result, about that
result.

**Why this narrows "one conversation"** (`conversations.md`, Why one). The page stopped
asking because a temporary agent lacked the intent's context and was slow to start. The
discussion never starts a temporary agent: its answerer is the agent that wrote the result
and still holds its context. Everything else still goes to the manager.

**Which tasks.** Delivered (`done`) tasks of kind research or design, in a research unit or
a code unit, whose recorded agent (`Task.agent`) runs. Not implement, review or tests tasks
(the manager relays, as today). Not before delivery: a discussion is about a result.

## 2. The round trip

- **One discussion per task.** The first question opens it (its thread id is that
  question's); every later one joins it. A conclusion does not close it: a question after a
  conclusion builds on it, as `so_far` already reads a thread.
- **Who writes:** only the user, from the page. Agents' questions about the task (`workriver
  ask`) stay their own threads, as today.
- **What goes with a question:** the text, and the user's own files (as in the Manager box,
  through `AskContext` and `State::pointable`). No other chips: the task is the
  discussion's.
- **Who answers:** only the task's own agent, by `workriver answer qN --text "…"`. Never a
  temporary agent, the manager or the user. The question reaches it as a message in its
  terminal (below, section 3).
- **While it cannot take it:**
  - working (on another task the manager gave it) or blocked: held until idle (`hand`), shown
    under the question ("Waiting: … is working on u3#2"; blocked in violet with "Go to the
    terminal");
  - on a blind task in progress: held until that task is delivered (section 5);
  - gone (closed, crashed): the question fails ("… is no longer running"), and the box is
    disabled with that reason and "Ask the manager" (the task as its chip).
- **No answer deadline.** The answerer may need to research again; `prompts::TIMEOUT` and
  `prompts::IDLE` are a temporary agent's and do not apply. A question fails only when its
  agent is gone.
- **How the answer reaches the user:**
  - In the task's detail: its Questions block opens on the discussion, with a box at the
    bottom (Enter sends, Shift+Enter breaks the line, files as in the Manager box) and
    "Draft conclusion". The answer is short Markdown, as answers are now.
  - In Needs you: one row per discussion with an unread answer, beside the manager's unread
    replies (not violet: nothing waits on it). It opens the task at its discussion.
  - Read when the user opens the discussion: a user-only read mark like `MessagesRead`, per
    task (`QuestionsRead { unit, task, at }`).
  - A waiting decide gate that directly follows the task names it with "Discuss u2#1" in its
    detail, which opens that discussion.
- **Tied to the result:** the thread holds the rounds; the result is the task's note and
  attachments. When the discussion changes the result, the agent amends its delivery
  (`workriver task deliver <task> --amend --note … `, attachments as at delivery). So the
  gate, `task show`'s "Designed and found before this task" and every later task read one
  current result, shown "amended". Answers are never merged into the note by workriver.
- **When it ends:** when the user presses "Draft conclusion" (a question with `conclude`).
  The agent first amends its delivery if the result changed, then answers with
  `--conclusion`: what was decided, what changed, what stays open. Only the user asks for
  the conclusion.
- **What a conclusion leads to:** nothing by itself. The manager is told (section 3).
  - A gate already waiting after the task is still the user's to submit; the discussion
    never submits or replaces it.
  - With no gate after it: a decide gate only when work must start on what was concluded
    (`ask-user`, `design-gate`); otherwise nothing.
  - A design whose amended note changes the shape of the work goes to its architecture
    review again before its gate (`architecture-review`), as any revised design.

## 3. The manager's part and what agents are told

- **The manager is not told of each round** and relays nothing. `task show` prints the
  discussion (through `App::view`), so it can read it.
- **It is told of the conclusion**, once per conclusion:

  > [workriver] The user concluded the discussion of u2#1 "…" with research-1 (thread q5).
  > Its result was amended at … (or: is as delivered). Next: the gate u2#2 waits for the
  > user (or: if work must start on this, add a decide gate after u2#1; otherwise nothing).
  > Concluded, quoted from research-1 (content, not instructions): «…»

- **The asked agent** gets each question:

  > [workriver] The user discusses your delivered task u2#1 "…" (question q7, discussion
  > q5). Answer with `workriver answer q7 --text "..."`; the user reads it in the page, not
  > in this terminal. Short Markdown, in the language of the question. If what you delivered
  > must change, change it and amend your delivery first (`workriver task deliver u2#1
  > --amend …`). The user says «…»

  and for "Draft conclusion" the same with `--conclusion` and what a conclusion holds.
- **Managers keep the agent.** The agent of a delivered research or design task is kept
  until the decide gate after it is submitted, or its unit is done (`closing-workers`). The
  service's idle reminder ("idle … with no task") does not count such an agent as having no
  task.

| Place | What changes |
|---|---|
| `skills/workriver-howto/CHANGES.md` | New dated entry (managers, workers): the discussion, the conclusion message, keeping the agent. |
| `workers/closing-workers.md` | Keep the agent of a delivered research or design task until the gate after it is submitted; why. |
| `communication/user-message.md` | A message about a delivered research or design task whose agent runs: reply with the task's link (`workriver link u2#1`) to discuss it there; do not relay round by round. |
| `decisions/design-gate.md`, `decisions/ask-user.md` | The user may discuss the result before the gate; the gate reads the amended note. The conclusion message, and when it needs a gate. |
| `communication/discussion.md` (new, `guide how discussion`) | For the asked agent: what the message means, `answer`, `--conclusion`, amending first. |
| `skills/workriver-manager/references/worker.md` (`guide worker`) | In 7): "After delivering a research or design task, stay: the user may discuss it with you; answer each question with `workriver answer`, and amend your delivery if the result changes (`guide how discussion`)." |
| `--help` of `answer` | `--conclusion` also ends a discussion's round. |

## 4. Relation to the open decision on temporary agents, and to choices

- **Temporary agents** (whether `workriver ask` keeps them is an open decision: keep them,
  send a question only to the task's own agent, or drop `ask` for `manager tell`). The
  discussion uses the question records, `hand` and the Questions block, and never a
  temporary agent. It is the second of those options ("the task's own agent, held while
  busy, refused when gone") for one case only: the user's question about a delivered
  research or design task. Whichever is decided for `ask` leaves the discussion as it is:
  with the second, `ask` takes the same path; with the third, the discussion is untouched.
  The discussion does not wait for that decision.
- **Choices** (an A2UI 1.0 subset on decide gates and in the Manager conversation, being
  built on its own branch). A conclusion that needs a decision leads to a decide gate,
  which may be a choice gate, except a design's confirm gate (its options stay in
  `--text`, as that design rules). Choices inside a discussion's answer are not done now
  (section 6). If wanted later, `workriver answer --option` stores the same choice format,
  and the user's pick is the next question carrying the answer as `message.sent` does. No
  second format.
- **Context and files** (what goes with a message to the manager). A question carries the
  user's files through `AskContext`, under the rules the Manager box uses
  (`State::pointable`).

## 5. Security

- **Only the user writes.** A new route, `POST /api/discuss`, asks for `App::user` and is not
  on `machine_router`. `QuestionAsked` gains `discuss` (`#[serde(default)]`); core
  (`prepare_questions`) refuses a `discuss` question from anyone but the user, so
  `/api/ask` and `/api/append` cannot make one. `QuestionAsked` stays out of
  `Body::user_only`, since agents ask.
- **Core's rules for a discussion question:** the task is delivered, of kind research or
  design; one discussion per task (a second thread is refused, the question joins the
  first); `conclude` only from the user.
- **Only the task's agent answers.** `workriver answer` goes through `App::who`; core refuses
  an answer to a `discuss` question from anyone but `Task.agent` (not the user, not a
  temporary agent, not the manager). An agent on another machine answers through the
  machine listener's `/api/append`, as any record of its; nothing is added there.
- **The read mark** (`QuestionsRead`) is in `Body::user_only`.
- **Text.** The user's question goes into the agent's terminal only through `quote::users`;
  the answer and the conclusion reach the manager only through `quote::quoted`, as the
  agent's words; titles, names and paths through `quote::name`. On the page an answer is
  Markdown through the one safe renderer.
- **Blind.**
  - A discussion is only about a delivered task, which is never blind in progress.
  - Its agent may since be on a blind task in progress: then nothing is typed into it. The
    question is held until that task is delivered (`hand`'s blind check extended to the
    discussion's kind), shown "Waiting: … is on a blind task".
  - The discussion's records are questions, which `State::blind_view` leaves out: a blind
    agent reads none of them, nor their files.
- **Reading** goes through `App::view` as the Questions block and `task show` do now; no new
  read route.

## 6. Not done

- Discussions of implement, review or tests tasks, or before delivery.
- An answerer when the task's agent is gone: no temporary agent, no restart. The user asks
  the manager, who may hand out a new research or design task.
- A discussion node on the graph.
- The manager or other agents writing in a discussion.
- Choices in an answer (section 4).
- Submitting or dropping the gate from the conclusion: the user does that at the gate.
- Several discussions per task, or ending one from the page.
