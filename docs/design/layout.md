# The code's layout

Where each part of workriver lives, file by file, and which way each part may import. For
agents who add or move code. The short map for people is `docs/architecture.md`; the layering
rules in AGENTS.md (Layout) are the summary of this file; the tests that hold it are in
`guards.md`.

The code is cut by domain: the same thing has the same folder name in every layer
(`planning`, `work`, `conversations`, …).

## Notation

Dependency notation used in the comments of the trees:

- **C**: core domain types, state, rules, history and ports; no network, UI, SQLite, Herdr,
  clap or axum.
- **D**: the data adapter implementing core's port; depends on C, never on the service, the
  runtime or the UI.
- **S**: scan's public API; no core policy or service dependency.
- **R**: core's `Runtime` interface and workriver's runtime values. Concrete adapters are
  private to the outer `runtime/`.
- **W**: web domain types and pure selectors; type-only references between domains are
  allowed. **U**: React, i18n, UI primitives and the presentation libraries installed.
- A directory's permission applies to its files unless a comment is narrower. `mod.rs` and
  `index.ts` declare ownership and exports; they are not barrels to import everything
  through. (Known exception: the domain files of core and scan import with `use crate::*`
  through the crate root's re-exports; replacing that with explicit imports is open work.)

## Repository and the three library crates

```text
workriver/
├── AGENTS.md                         # Agent contract; links to the design and recipes. CLAUDE.md links to it.
├── DESIGN.md                         # English product/design entry; indexes docs/design/ui/.
├── README.md                         # English starting point; links to the bilingual human docs.
├── Cargo.toml / Cargo.lock           # Four workspace crates.
├── crates/                           # outer -> core/data/scan; data -> core; core and scan independent.
│   ├── workriver-core/               # C. No rusqlite.
│   │   └── src/
│   │       ├── lib.rs                # Crate-root re-exports of the domains; now().
│   │       ├── runtime.rs            # R: the runtime port in workriver's words.
│   │       ├── screens/              # R: what an agent's terminal shows, in workriver's words; pure.
│   │       │   ├── mod.rs            # Key (the only keys ever pressed), Screen, Frame; parse: a
│   │       │   │                     # dialog read bottom up (hints, options, region, cut); marked:
│   │       │   │                     # every Default_Ignorable_Code_Point made visible; kept and
│   │       │   │                     # question_line: what the record keeps of a screen; Outcome.
│   │       │   ├── tests.rs          # The parser against the measured dialogs; marked; kept.
│   │       │   └── __fixtures__/     # The dialogs as captured (Claude, Codex), two idle screens
│   │       │                         # quoting one, a cut region, Unicode's DICP table.
│   │       ├── ports/                # Contracts in workriver terms; C/std, no adapter types.
│   │       │   ├── mod.rs            # Narrow port exports.
│   │       │   ├── data.rs           # WorkStore: reads at a revision, checked record, history, status,
│   │       │   │                     # an intent's events, errors.
│   │       │   └── tests.rs          # What every store must answer (reads that fail say so).
│   │       ├── choices/              # A question's options as recorded: a fixed A2UI v1.0 subset;
│   │       │   │                     # its write rule (build, check, read the values). No UI.
│   │       │   ├── mod.rs            # Choice, ChoiceOption; new/check/options/check_answer.
│   │       │   ├── tests.rs          # The stored form, its rules, and the A2UI schema guard.
│   │       │   └── __fixtures__/a2ui-v1_0/  # The A2UI v1.0 JSON Schemas (Apache-2.0), vendored.
│   │       ├── planning/             # Projects, intents, units, workspaces and their transitions.
│   │       │   ├── mod.rs            # Planning types/exports and transition entry points.
│   │       │   ├── intents.rs        # Roadmap; intent create/update/start/close/merge; finding policy;
│   │       │   │                     # who edits an intent's text, its versions (Edits), the base check,
│   │       │   │                     # an intent's project, the workspace it starts or moves in.
│   │       │   ├── history.rs        # An intent's versions and a version's text, rebuilt from its events.
│   │       │   ├── units.rs          # Code/research/spike units, relocation, merges, commit links.
│   │       │   ├── projects.rs       # Projects; workspaces (workriver's own: id, name, project), the
│   │       │   │                     # manager's seat and its appointment, an agent's workspace.
│   │       │   ├── tests.rs
│   │       │   └── tests/ownership.rs  # Who sets an intent's project, workspace and manager.
│   │       ├── work/                 # The task graph and its outcomes; no IO.
│   │       │   ├── mod.rs            # Task/input/evidence types, task-ref parser, entry points.
│   │       │   ├── tasks.rs          # Create/start/deliver/amend/drop, ownership, lookup.
│   │       │   ├── graph.rs          # Inputs, ancestors, follow-ups, progress, readiness, and design
│   │       │   │                     # ancestry (decide gates, superseding designs); result_gate.
│   │       │   ├── findings.rs       # Findings, suggestions, decisions, additions, deferred batches.
│   │       │   ├── gates.rs          # Gate submission/opening and results.
│   │       │   ├── chores.rs         # Non-blocking chores and their completion.
│   │       │   ├── commands.rs       # The user's recorded command runs/reviews/ends and Verdict.
│   │       │   ├── answers.rs        # agent.keys: the user's answer in an agent's terminal, cut
│   │       │   │                     # by its rule; on the task the agent held.
│   │       │   └── tests.rs
│   │       ├── conversations/        # Durable communication records.
│   │       │   ├── mod.rs            # Question/thread/message/delivery types and transitions.
│   │       │   ├── questions.rs      # Ask/answer/conclude/end, attachment context checks; the
│   │       │   │                     # user's discussion of a delivered result, its read mark.
│   │       │   ├── messages.rs       # Manager messages, replies, read position, agents seen.
│   │       │   ├── attachments.rs    # Attachment metadata, links, lookup; no file IO.
│   │       │   └── tests.rs
│   │       ├── access/               # Authority and visibility.
│   │       │   ├── mod.rs            # Principal/Rejected, reserved actors, Body::user_only, Body::service_only.
│   │       │   ├── blind.rs          # blind_tasks/blind_view; conversations_of (an agent reads only its
│   │       │   │                     # own workspace's conversation, and its own copy of a broadcast).
│   │       │   └── tests.rs
│   │       └── records/              # The aggregate and its history; every core domain.
│   │           ├── mod.rs            # Event, State, prepare/apply orchestration, the name helper; no domain bodies.
│   │           ├── body.rs           # The Body enum, serde defaults/tags, refs().
│   │           ├── tests.rs          # Shared test fixtures and general rule tests.
│   │           ├── tests/replay.rs   # The replay guard (guards.md).
│   │           └── fixtures/         # events.jsonl and expected-state.json (guards.md).
│   ├── workriver-data/               # D: the concrete storage; no outer imports.
│   │   └── src/
│   │       ├── lib.rs                # Re-exports Store and Error; the WorkStore implementation.
│   │       ├── sqlite/
│   │       │   ├── mod.rs            # The Store (open a given file, append, reads) and its Error.
│   │       │   ├── migrations.rs     # Schema, PRAGMAs and triggers; the format step (run only on a store
│   │       │   │                     # with no events; one with events and without it is refused).
│   │       │   ├── events.rs         # The stored row both ways: its encoding (insert) and replay; an
│   │       │   │                     # intent's events with those merged into it.
│   │       │   └── tests.rs          # Append-only, replay rebuilds the same state, and the stored
│   │       │                         # half of the replay guard.
│   │       └── tests.rs              # The port's contract on this backend.
│   └── workriver-scan/               # S: git and the file tree, here or over ssh; no core dependency.
│       └── src/
│           ├── lib.rs                # Public exports, Error, Result.
│           ├── runner.rs             # Local/SSH Runner and checked command execution.
│           ├── history/
│           │   ├── mod.rs            # Repo/rev/ref discovery, names, history exports.
│           │   ├── branches.rs       # Branch membership, fork/squash, patch ids.
│           │   └── commits.rs        # Batched commit/blob reads, commit metadata, where a branch meets its base, diffs.
│           ├── files/
│           │   ├── mod.rs            # FileChange/counts, before/after, live/worktree reads.
│           │   └── tree.rs           # Module tree and line counts.
│           └── tests.rs              # Git fixture tests.
```

`Body` and `State` are the domain vocabulary and the aggregate. `records::State::prepare`
makes the shared principal checks before it hands a variant to its owner; `apply`
dispatches once and opens gates after each event. Domain modules reference the aggregate
while it dispatches into them: coordination inside one crate, not separate bounded
contexts; no handler registry, no per-domain store. A helper is private when only its own
file calls it, `pub(super)` when only its domain folder does, and `pub(crate)` only where a
caller crosses a domain folder. Core never re-exports the concrete `Store` (that would be a
crate cycle). workriver-data opens only the file it is given; the data directory and the
names of its files are the outer crate's (`common/home.rs`).

## The data port

Core defines `WorkStore` in `ports/data.rs`; it is all the service knows of storage
(`App.store` is a `Box<dyn WorkStore>`). It asks four things:

- `recorded()`: everything recorded at one `Revision`, as the reader's own value (never a
  borrowed replay cache).
- `record(&Principal, Body)`: checks the request with the rules on what is recorded now,
  commits it and returns the committed event, its revision and what is recorded at it (what
  follows a record is worked out on the state it made, not on a later read). The only way to write; there
  is no unchecked write path.
- `history(after, limit)`: events oldest first, by cursor. A revision is the number of the
  last committed event and is the history's cursor too. For the service and tests only:
  there is no blind cut for events (core's blind rule cuts a `State`), so no route may serve
  it until such a cut exists.
- `status()`: where the store is, how many events, the revision.
- `intent_events(id)`: the events that make an intent's versions (`intent.created`,
  `intent.updated`, `intent.merged` into it), and those of every intent merged into it,
  transitively, oldest first; core's `intent_versions` and `intent_text` rebuild its history
  from them. Like `history`, not answered to a reader as it is: the one route that reads it
  (`/api/intent/{id}/history`) goes through `App::view` and refuses a reduced view first.

Named reads of one record (`unit`, `task`, `question`, `agent`, `thread`, `run`, …, and what
a previous run left: `delivered`, `open_questions`, `open_discussions`, `held_messages`,
`thread_agents`) are
provided methods over `recorded()` that a store may answer without the whole state; add one
when it is missing. `recorded()` is used only where a function of the whole state is called
or several records of one revision are needed. Every read returns a `Result`.

No SQL, connection, transaction, table or row id crosses the port. Reads take no principal:
who asks and what they may see is settled before a read, by `App::who` and by `App::view`
with core's two cuts (the blind view, else the conversations of the caller's own workspace:
`State::conversations_of`), so no second copy of that decision sits in an adapter. There is no
conflict error, since one service writes. `DataError` is `Rejected` (a broken rule, core's
`Rejected` inside), `Unavailable` or `Corrupt`, the last two with the storage's own text;
the service answers 422 for a broken rule and 500 otherwise.

`App::record` keeps, outside the adapter, what surrounds a write: `complete` before it, and
after the commit, in this order, delivery, the kind's follow-up, the branch watch and the
page's signal. Nothing of the runtime's groups is read for a record: which workspace an
event names, whether it is open, the caller's and of the intent's project is the rules' to
check (`State::prepare`), on the store, when the event is recorded. Two things the service
adds, both for the user's own events only. A seating (`workspace.manager_set` naming a
session): `App::seating` reads, with nothing locked, where that session's machine's runtime
shows it (refused at no root, at several, or away from a pin that runs), fills in the seat's
machine and folder, and pins the session once the event is committed (`App::seated`); then
the new manager and the one it replaced are told (`tell_of_seat`). And a project:
`project.added` and its first `workspace.created` are recorded under one lock of the store.
A seat that is not the one the event's `replaces` names is refused by the rules and answered
409 (`core::SEAT_CHANGED`). No runtime is asked while the store is locked. A requirement added to a task (`task.noted`) is
told apart: the append handler calls `App::noted` after the commit, which tells the task's
agent (found in the runtime now, not the last poll) and gives how far that got, answered
beside the recorded event. An agent not found while machines are followed may have started
on one since it was last read: the answer says it is not told yet, and a thread of its own
(`find_noted`) reads the machines again (`Machines::refresh`, each on its own thread) and
tells it after the first read that shows it, so no request ever waits for a machine. One whose agent is
on a blind task is kept as the event and the agent it is for (`App.notes`), not as text:
each poll works it out again (`give_notes`), so it is given only while the task is still
started and with that agent. It goes out by `App::send_to` (the runtime's `send_to`): to
that session on that machine and no other (a seated manager: at its pinned place only,
the runtime's `send_at`), files as that machine can get them. A held
notice (`For::Notice`) is matched and sent the same way, by session only. How a failed read is handled before and after the commit is in
AGENTS.md (Layout).

## The outer crate

The top level answers one question: where the code runs. `cli/` in the CLI process,
`server/` in the service, `runtime/` adapts outside programs, `common/` is used by both
sides. `main.rs` alone puts them together.

- `cli/` → core, `common/`, the `runtime/` facade (only for where this process runs, the
  runtime's words, and what `machine add` sends of a machine's options); **never** `server/`.
- `server/` → core, scan, the `runtime/` facade and the `Runtime` interface, `common/`; only
  the assembly in `server/mod.rs` (and `main.rs`, which opens the store) names the concrete
  storage. **Never** `cli/` (the service's tests excepted: they drive it through the CLI's
  client).
- `runtime/` → `core::runtime`, scan's runner, `common/`; **never** `cli/` or `server/`.
  `runtime/herdr/` is imported by `runtime/` alone (it is a private module of the facade).
- `common/` → core types, scan's runner (to read a process chain on another machine), std and
  installed libraries; **never** `cli/`, `server/` or `runtime/`. `common/wire/` may also name
  the value types of scan that the page's shapes carry (`FileChange`, `Module`), and nothing
  else of scan beyond the runner.
- Inside `server/`, the folders carry the same names as core's domains.

```text
crates/workriver/
├── build.rs                          # Build revision metadata.
├── src/
│   ├── main.rs                       # Declares the four groups; enters cli or starts server.
│   ├── cli/                          # The CLI process: parse, fetch the caller proof, call the API, print.
│   │   ├── mod.rs                    # The clap tree (Cli, Cmd), the shared context (Cx), status,
│   │   │                             # open, board, link, guide, perf; one dispatch per domain.
│   │   ├── client.rs                 # Daemon, proof fetch and renewal, headers, bounded response reading.
│   │   ├── planning.rs               # Intent/unit/project/workspace/backlog commands (a seating included).
│   │   ├── work.rs                   # Task/finding/commit/gate/chore/attach/run-review commands;
│   │   │                             # delivery staging, task show.
│   │   ├── conversations.rs          # Ask/answer/reply/say/manager-tell and waits.
│   │   ├── machines.rs               # Machine/load commands.
│   │   ├── display.rs                # Board, an intent's history, a machine's history and load text,
│   │   │                             # and a task or finding in full.
│   │   ├── guide.rs                  # Embedded guides, topic lookup, install stubs.
│   │   ├── parse.rs                  # Input/evidence/step/external-ref parsers.
│   │   ├── tests.rs                  # Parser/guide/help/link/text tests; no running service.
│   │   ├── guards.rs                 # The CLI's guards: guide topics, the command tree (guards.md).
│   │   └── cli-tree.txt              # The command tree it pins.
│   ├── server/                       # The service process.
│   │   ├── mod.rs                    # Start/bind/serve, Options; the one place that assembles
│   │   │                             # the concrete storage and runtime.
│   │   ├── app/
│   │   │   ├── mod.rs                # App, the store, shared in-memory state, `got`.
│   │   │   ├── lifecycle.rs          # Start-up/recovery/watch/poll/shutdown.
│   │   │   ├── record.rs             # App::record/complete, a seating's read and what follows it, noted/give_notes,
│   │   │   │                         # post-commit order.
│   │   │   └── progress.rs           # Pending hand-offs, picked-up tracking, reminders run.
│   │   ├── access/                   # The security spine: who calls, what they may do and see.
│   │   │   ├── mod.rs                # App::who/user/view/here, a seated session's root (seating, seated, the
│   │   │   │                         # take-back and the proposals of see_roots), the Caller and the Reader.
│   │   │   ├── auth.rs               # Token, random sessions, one-time codes, modes, atomic saves.
│   │   │   ├── caller.rs             # Unix peer identity, proof store and listener (caller-identity.md).
│   │   │   ├── pins.rs               # Session-root pins (caller-identity.md, rule 5).
│   │   │   └── tests.rs
│   │   ├── http/                     # Transport wiring only; no domain policy.
│   │   │   ├── mod.rs                # router() and the machine_router() allow-list; no route discovery.
│   │   │   ├── guard.rs              # Host/Origin/proof/header extraction: what fills the Caller.
│   │   │   │                         # Part of the security spine (below).
│   │   │   ├── drain.rs              # Reads what a handler leaves of a body (capped, timed), so
│   │   │   │                         # a client still sending gets the answer, not a reset;
│   │   │   │                         # one that asked first (Expect) and was not read, none.
│   │   │   ├── session.rs            # Session status/code/exchange routes.
│   │   │   ├── error.rs              # ApiError, status mapping, blocking helper.
│   │   │   └── assets.rs             # Embedded web dist, MIME/caching, SPA fallback.
│   │   ├── planning/mod.rs           # Start and merge handlers; the user's acts on workspaces
│   │   │                             # (/api/workspaces: add, rename, close, open, manager, move).
│   │   ├── work/mod.rs               # Append, task show/drop, decisions, gates, chores, focus.
│   │   ├── terminals/                # An agent's terminal from the page; all three routes App::user.
│   │   │   ├── mod.rs                # Terminals (digest key, presses, writers, marks, views),
│   │   │   │                         # /api/agent/screen and /keys handlers, Refusal, ScreenView.
│   │   │   ├── press.rs              # One press: resolve once, compare the digest, digit or a walk
│   │   │   │                         # of arrows, the reads after it, agent.keys.
│   │   │   ├── marks.rs              # type_in / type_at (every message: resolve, or a seated
│   │   │   │                         # manager's pin; read, type at that place); the poll's look
│   │   │   │                         # at screens; waiting on its screen.
│   │   │   └── view.rs               # /api/agent/view: the capped, bounded frame stream.
│   │   ├── conversations/
│   │   │   ├── mod.rs                # Ask/discuss/end/reply/message/read handlers, shared state.
│   │   │   ├── questions.rs          # Start/hold/retry/answer/deadline/idle close.
│   │   │   ├── discussions.rs        # A discussion's question to its task's agent: hand, gone,
│   │   │   │                         # gate submitted, restart; what the manager hears of it.
│   │   │   ├── messages.rs           # Manager conversation send/hold/delivery/failure.
│   │   │   └── prompts.rs            # Ask, follow-up, answer-back and discussion prompts.
│   │   ├── commands/
│   │   │   ├── mod.rs                # Prepare/run/read/focus handlers; the user check in each.
│   │   │   ├── shell.rs              # Command checks, marker parsing, polling, safe close.
│   │   │   └── review.rs             # The temporary reviewer's prompt/result/deadline.
│   │   ├── machines/                 # Machine observation and its handlers; no runtime protocol.
│   │   │   ├── mod.rs                # Machine and settings handlers.
│   │   │   └── metrics/
│   │   │       ├── mod.rs            # Sampling worker (report types in common/wire/metrics.rs).
│   │   │       ├── probe.rs          # Probe/ps snippets, parsers, readings.
│   │   │       ├── alerts.rs         # The thresholds' methods and validation (their types are in
│   │   │       │                     # common/config), window/hysteresis.
│   │   │       ├── history.rs        # Minute folding, JSONL read/retention, modes.
│   │   │       └── tests.rs
│   │   ├── files/                    # Attachments: handlers and the bytes on disk.
│   │   │   ├── mod.rs                # Upload/inbox/fetch handlers, scoped checks, links for
│   │   │   │                         # an agent on another machine.
│   │   │   ├── store.rs              # Paths, content ids, atomic put, usage.
│   │   │   └── tests.rs
│   │   ├── views/                    # What the page and the CLI read; git attribution; caches.
│   │   │   ├── mod.rs                # Snapshot build (workspaces with their seats), the files
│   │   │   │                         # two open units both change.
│   │   │   ├── handlers.rs           # Workspace/state/code/metrics/history/settings reads; an intent's
│   │   │   │                         # history (through App::view, a reduced view refused).
│   │   │   ├── code.rs               # A unit's code, light snapshots, code keys, incremental reads, net change,
│   │   │   │                         # the files the code tree shows it changing.
│   │   │   ├── cache.rs              # Branch/commit/tree/remote caches, persisted format.
│   │   │   ├── task.rs               # Builds the task view (its shape is common/wire/task.rs, its
│   │   │   │                         # text the CLI's display.rs).
│   │   │   ├── trace.rs              # A task's trace: harness, model, and where each harness keeps the
│   │   │   │                         # session (the one place that knows; never reads the file).
│   │   │   └── tests.rs
│   │   ├── delivery/                 # Pure delivery plans and text.
│   │   │   ├── mod.rs                # Plan, event-to-recipient planning.
│   │   │   ├── messages.rs           # Decision/gate/task/manager templates, all through quote.
│   │   │   ├── reminders.rs          # Stall/idle/merge reminder decisions (tests beside them).
│   │   │   ├── quote.rs              # The one sanitizer for authored text (tests beside it).
│   │   │   └── tests.rs
│   │   └── tests/                    # The in-process daemon tests, compiled with the binary under cfg(test).
│   │       ├── mod.rs                # Declares the suites only.
│   │       ├── support.rs            # Fake runtime, Kernel, daemon constructors, git helpers, HTTP client.
│   │       ├── guards.rs             # The service's guards (guards.md).
│   │       ├── route-access.txt  page-json.txt   # What they pin.
│   │       ├── planning.rs  work.rs  access.rs  conversations.rs
│   │       ├── commands.rs  machines.rs  files.rs  views.rs
│   │       ├── terminals.rs          # Reading, answering and watching an agent's terminal.
│   │       └── ownership.rs          # Who sets an intent's project, workspace and manager.
│   ├── runtime/                      # Adapting outside programs: the facade and composition.
│   │   ├── mod.rs                    # Neutral construction/context/wording facade (connect,
│   │   │                             # caller_place, words, remote_ref, Settings); picks the adapter.
│   │   ├── follow.rs                 # Several machines as one Runtime; routing by machine; each
│   │   │                             # machine's change stream (watch) beside its reads.
│   │   ├── ssh.rs                    # Tunnel supervision, publication, reconnect timing.
│   │   └── herdr/                    # Private to runtime/. Another runtime goes beside it.
│   │       ├── mod.rs                # Herdr implements Runtime; protocol values to workriver values.
│   │       ├── protocol.rs           # Socket discovery, protocol version, framing, checked RPC.
│   │       ├── agents.rs             # Snapshot normalization, roots, states, names, ids.
│   │       ├── terminals.rs          # Send/focus/temporary agents/shells, ownership checks.
│   │       ├── screens.rs            # resolve, pane.read, send_keys, prompt at a pane, explain, the
│   │       │                         # observe child and its frames; the client socket's path.
│   │       ├── watch.rs              # events.subscribe as wake-ups (per agent pane).
│   │       ├── remote.rs             # Remote socket discovery/construction, its machine options.
│   │       ├── words.rs              # Every sentence that names Herdr or a pane: refusals,
│   │       │                         # errors, the CLI's help that names the runtime, hints.
│   │       └── tests.rs              # Protocol fixtures; no real Herdr panes.
│   └── common/                       # Used by both sides, owned by no domain; mod.rs: BUILD.
│       ├── config/
│       │   ├── mod.rs                # Settings file read/update, generic over the adapter's options
│       │   │                         # for a machine; the alert thresholds' types.
│       │   └── defaults.rs           # Tunable defaults (timeouts, caps).
│       ├── home.rs                   # The data directory and the name of every file in it, in one
│       │                             # place; it makes the directory (0700) and the database file
│       │                             # (0600) before the store opens. (The ssh control sockets are
│       │                             # named by scan's runner, which owns its ssh options.)
│       ├── process.rs                # Process chains (libproc/procfs), the remote chain snippet and
│       │                             # its parser, the bounded child-output helper.
│       ├── files.rs                  # The type a file is sent and served as (mime).
│       ├── diagnostics/
│       │   ├── mod.rs                # Performance log init/writing.
│       │   └── report.rs             # Perf summary/bundle/parsing for the CLI.
│       └── wire/                     # What service, CLI and page agree on: JSON shapes and the
│                                     # constants both sides use; no logic.
│           ├── mod.rs                # Shared values: EXPIRED (the proof header), MAX_ATTACHMENT.
│           ├── task.rs               # The task view's shape (TaskShow and its parts), Stall and Trace.
│           ├── machines.rs           # A followed machine as the service reports it (MachineState).
│           ├── workspace.rs          # The snapshot shapes (not re-exported at wire's root).
│           └── metrics.rs            # The metrics report types.
└── tests/                            # Process-boundary tests running the real binary.
    ├── caller.rs                     # Real CLI/socket/processes against a stand-in runtime socket.
    ├── boundaries.rs                 # The runtime-name ratchet over the repository (guards.md).
    ├── cli.rs                        # The CLI's text and the user's machine changes (guards.md).
    └── output.rs                     # serve prints nothing that signs anyone in.
```

**Visibility in `server/`.** Nothing under `server/` is bare `pub`. A folder's `mod.rs`
widens to `pub(super)`, a child file to `pub(in crate::server)`: both mean the service, and
no narrower level would let the handlers in sibling folders use them. One type is
`pub(crate)`: `Callers` (`access/caller.rs`), because `Options` has a field of it and
`main.rs` builds `Options`. The parts of an identity check (`chain_of`, `place`,
`one_place`, `runs`, `no_process`) are private to `access/mod.rs`, so no handler can put an
identity check together out of them. The shapes in `common/wire/` are another matter: they
are what the service, the CLI and the page agree on.

**`http/guard.rs` belongs to the security spine.** It decides facts `access/` then relies
on: that the token and the page's session count only on this machine's listener, that a
session counts for a request that is no GET only with our own Origin, the Host allow-list,
and the proof's look-up with its refusal when it has expired. The `Caller` it fills is
defined in `access/`. A change to `guard.rs` is a security change and is reviewed with
`access/`; a review of the spine covers both.

**The views and delivery.** The views name `delivery::quote` and nothing else of delivery;
delivery reads `project_folder` and `project_title` from the views, where the project
derivations stay (`views::build` uses them for every snapshot).

**Runtime names.** Every sentence of the outer crate that names Herdr or a pane is in
`runtime/herdr/words.rs` and reaches the rest through the facade; comments outside the
adapter say "the runtime" and "place". `common/config` names no adapter and no key of one:
it is generic over what the adapter keeps for a machine, and the facade puts the two
together (`runtime::Settings`, `runtime::Setting`, `runtime::MachineOptions`).

**Defaults.** `common/config/defaults.rs` gathers tunable defaults, not every constant:
serde defaults stay on their types; wire names, protocol versions and security invariants
stay with their owner.

## The web UI

```text
frontend/web/
├── package.json / pnpm-lock.yaml     # Dependencies; Vitest/happy-dom dev-only.
├── vite.config.ts                    # Build, proxy, the `test` block.
├── tsconfig*.json / components.json  # Type and shadcn setup; ui/utils aliases.
└── src/
    ├── main.tsx                      # Start i18n/theme/session, mount the app.
    ├── index.css                     # Themes/layout/accessibility.
    ├── app/                          # Composition and current selection; no business rules.
    │   ├── App.tsx  Side.tsx  link.ts
    ├── api/                          # The only way to the service; no domain UI imports. Allowed
    │                                 # outside it: a stored file's own URL, read where it is shown
    │                                 # (an <img src>, the viewer's text preview).
    │   ├── http.ts                   # GET (read), POST (post, call), the change stream, errors, SIGNED_OUT.
    │   ├── session.ts                # One-time code exchange, the remembered deep link.
    │   ├── actions.ts                # The page's actions, focus included.
    │   └── terminal.ts               # An agent's terminal: its screen, the keys pressed, its view's stream.
    ├── workspace/                    # Snapshot loading and the optimistic overlay; no components.
    │   ├── types.ts  source.ts  edits.ts
    ├── planning/                     # types.ts select.ts edit.ts Rail.tsx IntentPage.tsx Start.tsx
    │                                 # Merge.tsx EditIntent.tsx IntentHistory.tsx
    │                                 # classify.ts (kind, size, tags, the rail's filter) Sort.tsx (their chips and fields)
    ├── work/                         # types.ts select.ts growth.ts actions.ts layout.ts (select.test.ts)
    │                                 # Activity.tsx Nodes.tsx TaskDetail.tsx GateForm.tsx Findings.tsx
    ├── attention/                    # select.ts Inbox.tsx Backlog.tsx sound.ts (the sound for a new row)
    ├── agents/                       # types.ts select.ts Agents.tsx Mark.tsx (workriver's agents, not Herdr's)
    │                                 # Gate.tsx (a stuck agent answered: its screen, options, keys)
    │                                 # TerminalView.tsx (the dock, xterm.js loaded on first use) context.tsx
    ├── conversations/                # types.ts select.ts Manager.tsx Turns.tsx Discussion.tsx; never imports app/
    ├── code/                         # types.ts files.ts load.ts useCode.ts CodeLayer.tsx Commits.tsx select.ts
    ├── attachments/                  # types.ts Viewer.tsx Thumbs.tsx preview.ts; Viewer calls onAsk, never Ask
    ├── commands/                     # types.ts Run.tsx context.tsx
    ├── machines/                     # types.ts format.tsx Machines.tsx Settings.tsx
    ├── session/                      # Session.tsx
    ├── content/                      # Markdown.tsx Prose.tsx Chips.tsx TerminalText.tsx (screen text in the
    │                                 # order a terminal draws it), diagram/ (mermaid),
    │                                 # choice/ (a question's options: surface.ts Choice.tsx);
    │                                 # no api or domain UI imports
    ├── ui/                           # primitives/ (the shadcn files, radio-group.tsx and popover.tsx among them) Choice.tsx Icon.tsx
    │                                 # CopyButton.tsx Tip.tsx; never feature/data/API imports
    ├── lib/                          # utils.ts useResize.ts isField.ts
    ├── config/defaults.ts            # Quiet/resize/cache/preview defaults, widths, the graph's height, a hidden tab's reads.
    ├── theme.ts
    ├── i18n/                         # en, zh-CN, ja, index, the i18next declaration.
    └── mock/                         # demo.json, lockscreen.svg, metrics.ts, history.ts (intent
                                      # histories kept in memory): sample content.
```

UI composition runs one way: app → domain views → shared content and controls. Pure
selectors import no views, no app, no API actions, no mocks. No value import cycle among the
page's files (the viewer opens Ask through its context's `onAsk`). `api/` carries every
request: `http.ts` the shared GET (`read`) and POST helpers (`post`, `call`) and the change
stream; `actions.ts` and `session.ts` call `fetch` for their own requests. SIGNED_OUT is sent
on a 401 only by the POSTs made through `post` and `call` and by a run's read; the other
requests do not send it. No component fetches on its own. The `*.test.ts(x)` files sit
beside what they test.

## Skills and documents

```text
skills/
├── workriver-manager/SKILL.md, references/worker.md
├── workriver-backlog-import/SKILL.md
└── workriver-howto/
    ├── SKILL.md                      # The topic index; names topics, not paths.
    ├── CHANGES.md                    # Kept whole, chronological.
    └── references/                   # Grouped by work domain; topic = file basename.
        ├── planning/        start-intent, units, units-other-machine, spike, closing-units
        ├── workers/         hand-off, closing-workers, stalled, branch-moved, commit-link
        ├── decisions/       design-gate, ask-user, gates-chores, deciding-findings, deferred-batches, push-pr-merge
        ├── reviews/         architecture-review, blind-review, contract, finding-additions
        ├── communication/   ask, user-message, not-delivered, blocked, attachments
        └── operations/      machines, updating
scripts/
└── migrate-virtual-workspaces/       # One-off: a store of the earlier form (workspaces as the runtime's
    │                                 # groups) to workriver's own workspaces. Not in the Cargo workspace,
    │                                 # never built, tested or shipped with the product; removed once the
    │                                 # user's store is migrated.
    ├── README.md / README.zh-CN.md   # How the user runs it once; what each old event becomes.
    ├── old/                          # Own [workspace]; workriver-core as of the last commit before the
    │                                 # change (.local/migrate-old): `plan` (the mapping), `apply`.
    └── new/                          # Own [workspace]; this tree's core and data: `write` (the one insert
                                      # into the events table outside workriver-data), `check`.
docs/
├── README.zh-CN.md, getting-started(.zh-CN).md, security(.zh-CN).md, images/
├── architecture.md / architecture.zh-CN.md   # A short code map for people.
├── runtimes/herdr.md / herdr.zh-CN.md        # Everything runtime-specific in the human docs.
└── design/                                   # English specifications for agents.
    ├── layout.md                             # This file.
    ├── guards.md                             # The tests that hold behaviour and layout.
    ├── caller-identity.md                    # Who a call comes from: the rules.
    └── ui/ navigation, activity, code, conversations, operations, attachments, presentation (.md)
```

`workriver guide how <topic>` finds the embedded recipe whose basename is the topic
(rust-embed's own iterator, no manifest), asserts basenames are unique, and keeps the
unknown-topic fallback and the worker's one-line paragraph. DESIGN.md is the overview and
reading map of `docs/design/ui/`.
