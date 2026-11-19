# workriver's own agent commands

A design, not built yet (u13#32; item 6 of the user's decision in u12#9). The user confirms
it at a gate before anything is implemented. It is written for the agents who will build
and review it.

**The goal.** Today a manager starts and drives its workers with the runtime's own CLI. The
recipes name it in six places (`grep -rn "herdr " skills`):

| Recipe | Runtime command it names today |
|---|---|
| `hand-off` | `herdr agent start <name> --kind … --pane … [-- <option>]`, `herdr agent prompt <name> "…"` |
| `stalled` | `herdr agent read <name> --source recent-unwrapped --lines 80`, `herdr agent prompt <name> "…"` |
| `closing-workers` | `herdr agent get <name>` (for its pane id), `herdr pane close <pane_id>` |

Two sentences workriver itself types into agents name the same commands (both in
`runtime/herdr/words.rs`): the hint a stalled worker's manager gets (`stall_hint`: read,
prompt) and the hint the agent answering a question gets (`ask_hint`: get, prompt `--wait`,
read).

After this unit the recipes and both hints name only `workriver agent …`. The commands go
through the runtime facade, so a second runtime (later ACP) needs no recipe change.

**Recommendation in one paragraph.** Five commands under `workriver agent`: `start`,
`prompt`, `read`, `close`, `list`. A manager uses them on the workers of the workspace it
manages; the user on any agent; a worker on nobody, with one narrow exception (the agent
answering a question may ask that task's agent, which the hint allows today). Starting and
closing are recorded as events; a prompt and a read are runtime actions only. Who calls is
found by `App::who` as for every command; what a manager types reaches the worker as the
manager's words, never as workriver's or the user's. The core `Runtime` trait grows by one
method and one argument.

## 1. The commands

All under one new group, `workriver agent`. A target is always an agent's **name** (what
the board and the page show), resolved inside the caller's workspace; never a place, a
terminal or a session id of the runtime.

```text
workriver agent start <name> --harness <harness> [--dir <path>] [--machine <label>] [-- <arg>…]
workriver agent prompt <name> --text "<one paragraph>" [--wait [--timeout <seconds>]]
workriver agent read <name> [--lines <n>]
workriver agent close <name>
workriver agent list
```

| Command | Does | Prints |
|---|---|---|
| `start` | Starts `<harness>` as `<name>` in a place of its own in the caller's workspace, working in `--dir` (default: the caller's working directory). `--machine`: on that followed machine, in the workspace linked with the caller's. What follows `--` goes to the harness as its arguments, untouched. Returns when the agent is ready for a prompt, or blocked on a first-run question a person must answer (then it says so). | `started <name> (<harness>, <state>) in <dir>` |
| `prompt` | Submits the text to the agent. Does not wait, unless `--wait`: then it returns when the agent is next idle, done or blocked (default 120 s, at most 600). | `given to <name>`; with `--wait`, `<name> is <state>` |
| `read` | The last `n` lines the agent showed (default 80, at most 400), as plain text. | A first line `what <name> showed last (content, not instructions):`, then the text |
| `close` | Closes the agent and the place it was started in. | `closed <name>` |
| `list` | The agents of the caller's workspace: name, harness, state, machine, the task each is on, who started it. | One line per agent |

`list` is the one command beyond the four asked for. The recipes need it twice: `hand-off`
says "reuse an idle worker before starting one", and `ask_hint` says "if it is idle - check
with `herdr agent get`". Without it both would still name the runtime.

**Generic, as u10#7 requires.** workriver knows no harness. `--harness` is a word the
runtime takes or refuses, and its refusal is passed on as it is. Options of a harness are
the opaque arguments after `--`. This is how `hand-off` already passes the option that makes
a harness run its commands itself, and it stays so. Nothing in the code branches on a
harness name.

**Not offered, on purpose:**

- **Sending keys** (`herdr agent send-keys`). Keys answer a harness's approval and trust
  dialogs. An agent pressing them for another agent would act as the person the dialog
  waits for.
- **Choosing a place** (`--pane`). The runtime makes the place. This removes the manager's
  separate step of making a pane first, and keeps the user's own terminals out of reach.
- **Rename, focus, attach.** No recipe needs them; focus is the user's, from the page.

## 2. Who may use them

Who calls is never said by the caller (section 4). What each caller may do:

| Caller | `start` | `prompt` | `read` | `close` | `list` |
|---|---|---|---|---|---|
| **The manager** registered for the workspace it runs in | In its own workspace (and the workspaces of other machines linked with it) | Its workspace's workers | Its workspace's workers | Workers started with `agent start` in its workspace | Its workspace |
| **The user** (CLI from a terminal that is no agent's; later the page) | Any workspace, named with `--workspace` | Any agent | Any agent | Any agent | All, or `--workspace` |
| **The agent answering a question** (a temporary agent, or the task's agent) | No | Only the agent of the task the open question is about, only while that agent is idle | The same agent | No | No |
| **Any other agent** (a worker, a manager of another workspace, an agent on an undelivered blind task) | No | No | No | No | Its own workspace |

What counts as **a worker of a workspace**, all of these:

1. The runtime lists it now as an agent (a plain shell is never a target: it has no name,
   and no command takes a place).
2. It runs in the caller's workspace, or in a workspace of another machine linked with it.
3. It is not the registered manager of any workspace, and not the caller.
4. It is not one of workriver's own temporary agents (`why-qN`, the command reviewers):
   workriver starts and closes those.
5. workriver knows it as a worker: it was started with `agent start`, **or** it is the agent
   of a task of one of the workspace's intents. The second half keeps workers started before
   this unit, and agents the user started by hand and the manager gave a task, reachable.

`close` is narrower than `prompt` and `read`: only an agent started with `agent start` in
that workspace (by this manager or the one before it). "Never close an agent you did not
start" is a sentence in `closing-workers` today; it becomes a refusal. The user's own
agents, another manager's workers and the temporary agents cannot be closed by a manager.

**The user's own terminals are never reachable**: conditions 1 and 5 exclude a shell and an
agent the user runs for themselves, and no command takes a place.

**What a worker may do to another worker: nothing**, except the one case in the table. The
worker instructions already say "never type into another agent's terminal", and
`workriver ask` is the way to ask about another task. The exception exists because
`ask_hint` tells the answering agent today that it may ask the task's agent one short
question. The service knows the open question, who answers it and which task it is about,
so it can check exactly that.

## 3. Recorded or not

| What | Recorded | Why |
|---|---|---|
| `start` | **An event**, `agent.started`: name, harness, arguments, directory, workspace, machine, the first session id, the opaque place the runtime gave. Actor: who started it. | It answers "who started this agent, with what" for the page, and it is what `close` checks ownership against after a service restart. |
| `close` | **An event**, `agent.closed`: name, session. Actor: who closed it. Also recorded by the service itself when a started agent is no longer listed (its terminal closed, the runtime restarted). | The life of a started agent has two ends on the record. Ownership ends with it, so a later agent at a reused place is not taken for the old one. |
| `prompt` | No event. A line in the timing log: who, to whom, how long, the outcome; no text. | What a hand-off asks is already the task's text, and the worker instructions are 4 127 bytes pasted each time (`worker.md`). As events they would be most of the store's growth and repeat what is recorded. |
| `read` | No event. A line in the timing log, without the text. | Reading changes nothing. |
| `list` | Nothing. | A read. |

This matches what the confirmed plan says for later (`restructure.md` section 6): "only
approvals and agent start/close would be events"; transcripts stay apart from the event
store.

**The page.** The Agents panel shows for a started agent who started it and when, beside what
the runtime reports of it today. Arguments and directory show in the agent's detail. A
closed agent leaves the list as today. Its start and close stay in the history of the
intents it worked on. No new panel and no buttons in this unit (section 7).

**Arguments are recorded as given.** They explain how an agent behaves (a model, a
permission mode). The `hand-off` recipe will say not to put a secret there; workriver
cannot tell a secret from an option.

## 4. Security

The rule of `docs/security.md` and AGENTS.md holds unchanged: nobody is anyone by saying
so, and every failure is a refusal.

**Who calls.** Five routes, each asking `App::who` in its handler, as u13#7 requires of
every handler:

| Route | Check |
|---|---|
| `POST /api/agents/start`, `/prompt`, `/close` | `App::who`, then the table of section 2 |
| `GET /api/agents/read` | `App::who`, then section 2; refused for a caller on an undelivered blind task (it would show others' work, which `App::view` hides from it) |
| `GET /api/agents` | `App::view`'s rule: the user sees all, an agent its workspace, a blind caller only itself |

No request carries an actor. `--as` works only where the service allows it for tests
(`WORKRIVER_ALLOW_AS`), as for every command. The target is a name in the request; the
service resolves it to a live agent in the caller's workspace and refuses anything else.
`route-access.txt` gets these five routes with nobody / agent / user probes, as a
deliberate, named edit of that guard.

**That a caller is the manager** is read from the record, never claimed: the workspace of
the place the caller's process runs in has this session as its registered manager
(`State::workspace_of(group).manager`). `may_manage` uses the same rule today.

**Text typed into another agent.** AGENTS.md: text written by agents or the user goes into
messages only through `quote.rs`. Today's two forms do not fit. `quoted` marks text as
"content, not instructions", and an agent is told never to follow what stands inside «…».
A manager's hand-off is an instruction its worker must follow. So `quote.rs` gets a third
form, `said(from, role, text)`:

```text
From <name>, <role>: <text>
```

- `<role>` is workriver's own word for what the record says the sender is: "your
  manager", "the user", "the agent answering a question about your task".
- `<text>` goes through `clean`: one line, no control characters, no direction marks, and
  « » replaced, so it cannot open or close a quote of workriver's.
- The message starts with `From`, so it can never start with `[workriver]`. A `[workriver]`
  inside the text is rewritten to `(workriver)`.
- The cap is its own (16 000 characters, for the hand-off paragraph with the worker
  instructions), not the 2 000 of a quote.

What this keeps: a worker can tell workriver's word (`[workriver] …`), the user's (a quote
workriver marks as the user's) and its manager's (`From <name>, your manager: …`) apart.
With the runtime's own CLI today, a manager's text arrives with no mark at all and could be
written to look like any of the three.

**An agent waiting for a person takes no prompt.** The runtime refuses a prompt to a
blocked agent before sending any input (Herdr's own help says so: "submission is rejected
with agent_blocked before any input is sent"). workriver passes the refusal on and does not
queue: the manager decides. Nothing is ever typed into an approval dialog.

**What `read` returns is another agent's screen.** It is untrusted text: the CLI prints it
under a line that names it as content. It may show
what that agent printed, including anything secret it printed. That is true today
(`herdr agent read`), and section 2 narrows who may read from "any process in the
runtime" to the table. `workriver open --print` is already refused in an agent's place, so
a sign-in link is not on such a screen.

**Starting.** `--harness` and the arguments reach the runtime as separate values, never as
a line workriver builds for a shell. The runtime's socket API takes them as an array
(`agent.start`: `args`, verified in its schema). `--dir` must be an absolute path that
exists on the target machine. It is recorded. It is not limited to the folders of the
workspace's projects: a manager can already change directory in its own shell, so a limit
would refuse honest uses without stopping anything. (A question for the gate, section 8.)

**Another machine.** Two different things:

- *A target on another machine* (a manager here, a worker over there): the facade already
  routes by machine (`runtime/follow.rs` does it for `send` and `close`), and
  `start --machine` goes the way `start_temporary` does for a group `id@machine`.
- *A caller on another machine* (a manager that runs over there): the five routes are added
  to `machine_router`'s allow-list, since otherwise that manager would have no command left
  once the recipes name only workriver. Its identity is the remote check of
  `caller-identity.md` section 3.3, which is weaker than the local one. So a caller on
  machine M may act only on agents of machine M, the rule the metrics handlers already
  follow. It can never reach an agent on the machine workriver runs on, and is never the
  user.

**Refusals**, each a sentence saying what to do instead. They name no runtime and no
harness, so they are workriver's own and live with the handlers, not in the adapter:

- "only the manager of this workspace starts and closes its workers; you are a worker here:
  tell your manager (`workriver manager tell`)"
- "no agent `<name>` in workspace `<label>`; `workriver agent list` shows them"
- "`<name>` is not a worker of your workspace (it is its manager / a temporary agent of
  workriver / an agent with no task here): workriver does not type into it for you"
- "`<name>` was not started with `workriver agent start`: workriver closes only those; ask
  the user"
- "there is an agent `<name>` in this workspace already: pick another name"
- "`<name>` waits for a person in its terminal and takes no message now"

## 5. The facade, now and with ACP

**What the core `Runtime` trait has, and what it must grow now** (`workriver-core/src/runtime.rs`,
read in full):

| Command | Trait today | Grows now |
|---|---|---|
| `start` | `start_temporary(group, harness, name, cwd)`: a new place in the group, the harness started there, returned when ready. The adapter's `launch` does exactly what a manager's start needs. | One argument, `args: &[String]`, and the place in what it returns (for the event). The method is renamed `start`: it is no longer only for temporary agents. |
| `prompt` | `send(agent, text)` | Nothing. |
| `prompt --wait` | `agents()` (states) | Nothing: the service polls the state once a second until it leaves "working", on its own tick. |
| `read` | `read_place(machine, place, lines)`: takes a place, which callers of these commands never have | One method, `read_agent(agent, lines)`. Herdr: `agent.read` with target, source and lines (verified in its schema). |
| `close` | `close(agent)`: the agent and its place | Nothing. |
| `list` | `agents()` | Nothing. |

So: **one new method and one new argument**. `runtime/follow.rs` routes both by machine as
it does their neighbours. The facade `runtime/mod.rs` needs nothing. The two hints leave
`runtime/herdr/words.rs` and become workriver's own sentences, which shrinks the adapter's
words.

**The same commands over ACP.** The surface above is not shaped like a terminal: no place,
no keys, no screen size. Each command has a meaning when workriver starts and owns the
agent process (the host process of proposal A r2 section 3.3; proposal B r2 sections
4.2–4.5):

| Command | Over a terminal runtime (today) | Over ACP (later) |
|---|---|---|
| `start` | A new place, the harness typed into its shell | The host starts the agent's process and does the ACP handshake. `--harness` names a launch configuration; the arguments after `--` are its argv. |
| `prompt` | Text typed and submitted | `session/prompt`. A prompt while a turn runs is queued by the host, not typed over it. |
| `prompt --wait` | Polls the state | The prompt's own completion says the turn ended. |
| `read` | The last lines of the screen | The last lines of the transcript the host keeps. This is why `read` is defined as "what the agent showed last, as plain text", not as a screen. |
| `close` | Closes the place | The host stops the process it started. |
| `list` | The runtime's agents | The host's sessions. |

**What grows then, not now:** updates as a stream, permission requests and their answers,
cancelling a turn, a prompt's delivery states (queued, submitted, uncertain). Proposal B r2
section 4.2 names them and says the trait changes with the ACP work. None of the five
commands needs them.

**Identity gets simpler over ACP**: the root of a started agent is the process workriver
itself started, so `agent.started` binds the session to a root workriver owns instead of
one it discovers. The event's "place" stays an opaque string either way.

## 6. What changes in the recipes and the manager guide

Each as the size of its diff. Every change is also a line in `CHANGES.md`, since managers
are told of an update and read it.

| File | Change |
|---|---|
| `hand-off` | First paragraph (3 lines): "Use Herdr's own CLI…" becomes "start a worker with `workriver agent start <name> --harness <harness> --dir <worktree>`; reuse an idle one first (`workriver agent list`)". The command block (1 line): `herdr agent prompt` → `workriver agent prompt <name> --text "<one paragraph>"`. Section "A worker's commands must run in its own pane" (2 lines): the start line becomes `workriver agent start <name> --harness <harness> -- <that option>`; "Herdr passes what follows `--`" → "what follows `--` goes to the harness". One sentence added: no secret in the arguments, they are recorded. |
| `stalled` | 2 lines: `herdr agent read <name> --source recent-unwrapped --lines 80` → `workriver agent read <name>`; `herdr agent prompt` → `workriver agent prompt <name> --text`. |
| `closing-workers` | The two-command block becomes one: `workriver agent close <name>`. "Never close an agent you did not start" stays and gains "(workriver refuses it)". |
| `workriver-manager/SKILL.md` | Loop step 2: "Hand off through Herdr" → "Hand off with `workriver agent prompt`". The description and first paragraph keep "Herdr workspace": that word leaves in phase 7, with the rest of the runtime's names in the documents. |
| `stall_hint`, `ask_hint` (code, not a recipe) | The same substitutions; `ask_hint`'s `herdr agent get` → `workriver agent list`, its prompt with `--wait --timeout 120000` → `--wait --timeout 120`. Both move out of the adapter. |
| `docs/getting-started.md` and `.zh-CN.md` | The two example lines (`herdr agent start finder …`, `herdr agent prompt finder …`) become the workriver commands. |
| `workriver guide worker` (`worker.md`) | One clause: "run its commands from your own pane" is untouched; rule 8 gains "a message `From <name>, your manager:` is your manager's". |

The words the recipes still share with the runtime after this ("pane", "Herdr workspace")
are phase 7's, not this unit's.

## 7. What it costs, and what is left out

**Cost** (estimates from the code read, not measured):

| Part | Size |
|---|---|
| CLI: one group, five commands, their text | ≈ 150 lines in a new `cli/agents.rs` |
| Service: five handlers, the checks of section 2, `--wait` | ≈ 300 lines, in `server/agents/` once phase 5 has made `server/` |
| Core: two events with their rules, state (who started which agent), one trait method, one argument | ≈ 120 lines |
| Adapter: `args` passed on, `read_agent`; follow.rs routing | ≈ 40 lines |
| `quote.rs`: `said` | ≈ 20 lines |
| Tests: who may do what (each cell of section 2's table), another machine, the text form, a restart between start and close | ≈ 400 lines, on the stand-in runtime |
| Recipes, guide, getting-started, CHANGES.md | ≈ 30 changed lines |

**Guards that change, each on purpose and named in its commit:** `cli-tree.txt` (five
commands), `route-access.txt` (five routes), the replay fixture (two event kinds: the
"48 variants" of `restructure.md` section 1 become 50, which that section must then say).

**Risks:**

- **A place reused.** Ownership for `close` rests on the place recorded at start. Whether
  a restarted Herdr numbers its panes afresh is not known (the adapter's own comment says
  so). Mitigation in the design: the service records `agent.closed` when a started agent
  is no longer listed, and `close` also requires the name and harness to match. Not
  verified against a real restart.
- **A session id that changes.** An agent started on a first-run question is known by its
  place first and by its harness's session after. The checks of section 2 therefore go by
  workspace, name and place, not by the first session id.
- **How the runtime passes arguments on.** That `agent.start` takes `args` as an array is
  verified from the schema. Whether Herdr joins them into a shell line for the harness was
  not tested: the task allowed no experiment on the real Herdr.
- **Timing.** This unit adds a feature during the restructure. `restructure.md` says no new
  features inside the phases; the plan lists this as "a unit of its own" after phase 4. The
  service side is far easier to place after phase 5 (`server/` exists). Recommended order:
  build it after phase 5, before phase 7 rewrites the documents.

**Left out:**

- Buttons on the page (start, close, prompt from the Agents panel). The routes would serve
  them; the UI is its own design.
- `agent prompt --task u1#3`, where workriver would write the hand-off itself (the ref,
  where to work, the worker instructions) and the manager adds one line. It would end the
  pasting of `workriver guide worker` and make hand-offs uniform, but it changes what a
  hand-off is, beyond replacing the runtime's command.
- A limit on how many agents a workspace may have running.
- Queueing a prompt for a blocked agent.
- Agents that are not under a runtime workriver follows (AGENTS.md: out of scope).
- Rewriting AGENTS.md's "it does not start or schedule agents": workriver still schedules
  nothing, but with `agent start` it starts agents on a manager's word. One sentence there
  should change with the implementation.

## 8. For the gate

Decisions the design takes that the user may want otherwise:

1. **Workers get no commands** (except the answering agent's one question). The other
   choice: any agent may `read` and `list` in its own workspace.
2. **Prompts are not recorded.** The other choice: an event per prompt (the page would show
   every hand-off's text; the store grows by about 4 kB per hand-off).
3. **`--dir` is not limited** to the workspace's projects.
4. **`list` is added** as a fifth command.
5. **Managers on another machine** get the routes, limited to agents of their own machine.
6. **When**: after phase 5.

## What was verified, and how

- The six runtime commands in the recipes and the two hints: `grep -rn "herdr " skills`,
  and `runtime/herdr/words.rs`.
- Herdr 0.9.5, protocol 22, read-only: `herdr agent start|prompt|read|get|wait --help`,
  `herdr pane close --help`, and `herdr api schema --json` (`AgentStartParams` has `args`,
  an array of strings; `AgentReadParams` has target, source, lines; `AgentWaitParams` and
  the prompt's wait option exist; a prompt to a blocked agent is refused before input).
- The trait and its one implementation: `workriver-core/src/runtime.rs`,
  `runtime/herdr/{mod,terminals}.rs` (`launch`, `start_temporary`, `close`, `read_place`),
  `runtime/follow.rs` (routing by machine).
- Identity and authority: `serve.rs` (`App::who`, `App::user`, `App::view`, `may_manage`,
  `machine_router`), `quote.rs`, `docs/design/caller-identity.md`.
- Not run: nothing was started, prompted, read or closed on the real Herdr, and the service
  on 47820 was not called for anything but this task's own records.
