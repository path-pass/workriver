# Restructure: workriver organized by domain

The plan for intent i25, confirmed by the user in u12#9 (2026-11-19): proposal B (u12#8)
with the user's adjustments, the whole intent on one branch (`chore/restructure`), merged
and pushed once at the end (the user's answer of 2026-11-20, section 4). This file is what
the work follows; it is written for the agents doing it. Phase 0 (u13#1, its review fixes
in u13#4) wrote it and the guards in section 5; phase 1 (core and scan by domain, u13#10)
phase 2 (storage out of core, u13#15) and phase 3 (the CLI into cli/, common/ started,
u13#20) are done.

Sources, for what this file leaves out:

- Proposal B, revision 2 (u12#8, `workriver task show u12#8`): the reasons, the data port,
  the runtime boundary, ACP and i17. Its section 3 is the tree below before the user's
  adjustments; its section 6 is the commit list the phases group.
- The user's adjusted tree, in Chinese, attached to u12#9 (`proposal-b-tree.zh-CN.md`):
  where it differs from proposal B's English text, it wins. Section 2 here is that tree
  in English.
- The behaviour list (u12#3, attached there; `.local/refactor-coverage.md` in the main
  checkout): every behaviour today with the tests that hold it, and the gaps phase 0 closed.

## 1. What does not change

- **No behaviour changes.** A phase moves code; it fixes nothing, renames no public
  command, route, event, JSON field, settings key, file in the data directory,
  localStorage key, URL or guide topic. A row of the behaviour list that turns out wrong is
  told to the manager, not fixed in the move. Three steps are changes, named as such and
  reviewed as such: the runtime's words (phase 4), the page's runtime-neutral text (phase 7)
  and the data port (phase 8). One change is made apart from the phases, in a commit of its
  own (u13#7, the user's decision of 2026-11-20): `POST /api/attachments/inbox` (`workriver
  attach --dir`) asks who calls (`App::who`), as AGENTS.md's rule wants of every handler;
  before, any local process could have an existing task's folder made and learn its path.
- **The user's data.** The event store, attachments, settings, sessions, pins, metrics
  history and caches keep their paths, formats and permissions. No migration. The event
  enum (`Body`) keeps its 48 variants, its tags and its serde attributes; it is moved whole,
  not split.
- **No new features during the restructure** (u12#9 item 8). Only places are left for what
  comes later (section 6); no empty module is created for it.
- **Libraries.** No new dependency except the new internal crate `workriver-data` (phase 2)
  and, already added in phase 0, the web UI's dev dependencies: Vitest (u12#9 item 1) and
  happy-dom, the DOM the session gate's tests need to run its effects (7 dependencies
  against jsdom's 18), which the user kept when asked on 2026-11-20. No version upgrades;
  `Cargo.lock` changes only for the new crate.
- **Formatting.** `cargo fmt` is not used in this repository (it would rewrite every file);
  moved code keeps its lines as they are. Review with rename and move detection:
  `git show --color-moved=dimmed-zebra --color-moved-ws=ignore-all-space <commit>`.

## 2. The target tree

Dependency notation used in the comments:

- **C**: core domain types, state, rules, history and ports; no network, UI, SQLite, Herdr,
  clap or axum.
- **D**: the data adapter implementing core's port; depends on C, never on the service, the
  runtime or the UI.
- **S**: scan's public API; no core policy or service dependency.
- **R**: core's `Runtime` interface and workriver's runtime values. Concrete adapters are
  private to the outer `runtime/`.
- **W**: web domain types and pure selectors; type-only references between domains are
  allowed. **U**: React, i18n, UI primitives and the presentation libraries installed today.
- A directory's permission applies to its files unless a comment is narrower. `mod.rs` and
  `index.ts` declare ownership and exports; they are not barrels to import everything through.

### Repository and Rust core

```text
workriver/
├── AGENTS.md                         # Agent contract; links to the design and recipes. CLAUDE.md links to it.
├── DESIGN.md                         # English product/design entry; indexes the detailed specifications.
├── README.md                         # English starting point; links to the bilingual human docs.
├── Cargo.toml / Cargo.lock           # Four workspace crates; storage dependencies relocated, not upgraded.
├── crates/                           # outer -> core/data/scan; data -> core; core and scan independent.
│   ├── workriver-core/
│   │   ├── Cargo.toml                # No rusqlite; no new dependency for the port.
│   │   └── src/
│   │       ├── lib.rs                # Curated re-exports (today's crate-root names); now() and,
│   │       │                         # until phase 3 moves it to common/home.rs, data_dir().
│   │       ├── runtime.rs            # The runtime port as today; grows only with the later ACP work.
│   │       ├── ports/                # Contracts in workriver terms; C/std, no adapter types. (phase 8)
│   │       │   ├── mod.rs            # Narrow port exports.
│   │       │   └── data.rs           # WorkStore: typed reads, checked record, history, status, errors.
│   │       ├── planning/             # Projects, intents, units, workspaces and their transitions.
│   │       │   ├── mod.rs            # Planning types/exports and transition entry points.
│   │       │   ├── intents.rs        # Roadmap; intent create/update/start/close/merge; finding policy.
│   │       │   ├── units.rs          # Code/research/spike units, relocation, merges, commit links.
│   │       │   ├── projects.rs       # Projects, runtime workspace associations, links, current intent.
│   │       │   └── tests.rs          # Today's planning cases, moved intact.
│   │       ├── work/                 # The task graph and its outcomes; no IO.
│   │       │   ├── mod.rs            # Task/input/evidence types, task-ref parser, entry points.
│   │       │   ├── tasks.rs          # Create/start/deliver/amend/drop, ownership, lookup.
│   │       │   ├── graph.rs          # Inputs, ancestors, follow-ups, progress, readiness, and design
│   │       │   │                     # ancestry (decide gates, superseding designs): no design.rs (u12#9).
│   │       │   ├── findings.rs       # Findings, suggestions, decisions, additions, deferred batches.
│   │       │   ├── gates.rs          # Gate submission/opening and results.
│   │       │   ├── chores.rs         # Non-blocking chores and their completion.
│   │       │   ├── commands.rs       # The user's recorded command runs/reviews/ends and Verdict.
│   │       │   └── tests.rs          # Today's graph/work/decision cases.
│   │       ├── conversations/        # Durable communication records.
│   │       │   ├── mod.rs            # Question/thread/message/delivery types and transitions.
│   │       │   ├── questions.rs      # Ask/answer/conclude/end, attachment context checks.
│   │       │   ├── messages.rs       # Manager messages, replies, read position, agents seen.
│   │       │   ├── attachments.rs    # Attachment metadata, links, lookup; no file IO.
│   │       │   └── tests.rs          # Today's conversation/attachment/delivery cases.
│   │       ├── access/               # Authority and visibility.
│   │       │   ├── mod.rs            # Principal/Rejected, reserved actors, Body::user_only.
│   │       │   ├── blind.rs          # blind_tasks/blind_view.
│   │       │   └── tests.rs          # User/agent/service authority and blind-view cases.
│   │       └── records/              # The aggregate and its history; every core domain.
│   │           ├── mod.rs            # Event, State, prepare/apply orchestration, the name helper; no domain bodies.
│   │           ├── body.rs           # The 48-variant Body enum, serde defaults/tags, refs().
│   │           ├── tests.rs          # The shared test fixtures and general rule/replay tests.
│   │           ├── tests/replay.rs   # The replay guard (its stored half leaves in phase 2).
│   │           └── fixtures/         # Already here since phase 0.
│   │               ├── events.jsonl  # A synthetic stored history, old encodings first.
│   │               └── expected-state.json # What it adds up to (section 5).
│   ├── workriver-data/               # The concrete storage (phase 2); D, no outer imports.
│   │   ├── Cargo.toml                # Core + today's rusqlite, relocated.
│   │   └── src/
│   │       ├── lib.rs                # Re-exports Store and Error; (phase 8) the WorkStore implementation.
│   │       ├── sqlite/
│   │       │   ├── mod.rs            # The Store (open a given file, append, reads) and its Error.
│   │       │   ├── migrations.rs     # Today's schema, PRAGMAs and triggers, unchanged.
│   │       │   ├── events.rs         # The stored row both ways: its encoding (insert) and replay.
│   │       │   └── tests.rs          # Append-only, replay rebuilds the same state, and the stored
│   │       │                         # half of the replay guard (old rows reopen as core folds them).
│   │       └── tests.rs              # The port's contract on this backend (phase 8).
│   └── workriver-scan/
│       ├── Cargo.toml                # Same dependencies; still no core dependency.
│       └── src/
│           ├── lib.rs                # Public exports, Error, Result.
│           ├── runner.rs             # Local/SSH Runner and checked command execution.
│           ├── history/
│           │   ├── mod.rs            # Repo/rev/ref discovery, names, history exports.
│           │   ├── branches.rs       # Branch membership, fork/squash, patch ids.
│           │   └── commits.rs        # Batched commit/blob reads, commit metadata.
│           ├── files/
│           │   ├── mod.rs            # FileChange/counts, before/after, live/worktree reads.
│           │   └── tree.rs           # Module tree and line counts.
│           └── tests.rs              # Today's eight git fixture tests.
```

`Body` and `State` stay the domain vocabulary and aggregate. `records::State::prepare` keeps
the shared principal checks before it hands a variant to its owner; `apply` dispatches once
and keeps the gate opening after each event. Domain modules reference the aggregate while it
dispatches into them: intended coordination inside one crate, not separate bounded
contexts; no handler registry, no per-domain store. The crate-root names callers use stay;
the old `model` path is kept as re-exports during phase 1 and removed at its end. The
concrete `Store` is the exception: it leaves for `workriver-data` (phase 2) and core never
re-exports it (that would be a crate cycle).

### The outer crate

The top level answers one question: where the code runs. `cli/` in the CLI process,
`server/` in the service, `runtime/` adapts outside programs (Herdr; later ACP), `common/`
is used by both sides. `main.rs` alone puts them together.

- `cli/` → core, `common/`, the `runtime/` facade (only to read where this process runs);
  **never** `server/`.
- `server/` → core, scan, the `runtime/` facade and the `Runtime` interface, `common/`; only
  the assembly in `server/mod.rs` knows the concrete storage. **Never** `cli/`.
- `runtime/` → `core::runtime`, scan's runner, `common/`; **never** `cli/` or `server/`.
  `runtime/herdr/` is imported by `runtime/` alone.
- `common/` → core types, scan's runner (to read a process chain on another machine), std and
  installed libraries; **never** `cli/`, `server/` or `runtime/`.
- Inside `server/`, the folders carry the same names as core's domains.

```text
crates/workriver/
├── Cargo.toml                        # One binary; the data crate added at the composition.
├── build.rs                          # Build revision metadata, as today.
├── src/
│   ├── main.rs                       # Declares the four groups; enters cli or starts server.
│   ├── cli/                          # The CLI process: parse, fetch the caller proof, call the API, print.
│   │   ├── mod.rs                    # The clap tree (Cli, Cmd), the shared context (Cx), status,
│   │   │                             # open, board, link, guide, perf; one dispatch per domain.
│   │   ├── client.rs                 # Daemon, proof fetch and renewal (the CLI side of today's
│   │   │                             # caller.rs), headers, bounded response reading.
│   │   ├── planning.rs               # Intent/unit/project/workspace/backlog/manager-here commands
│   │   │                             # (their subcommand enums and run()); backlog list/export.
│   │   ├── work.rs                   # Task/finding/commit/gate/chore/attach/run-review commands;
│   │   │                             # delivery staging, task show.
│   │   ├── conversations.rs          # Ask/answer/reply/say/manager-tell and waits.
│   │   ├── machines.rs               # Machine/load commands.
│   │   ├── display.rs                # Board, history and load text, and a task or finding in full
│   │   │                             # (the task view's text, from show.rs); common/wire values.
│   │   ├── guide.rs                  # Embedded guides, topic lookup, install stubs.
│   │   ├── parse.rs                  # Input/evidence/step/external-ref parsers.
│   │   ├── tests.rs                  # Parser/guide/help/link/text tests; no running service.
│   │   ├── guards.rs                 # Phase 0's CLI guards: guide topics, the command tree.
│   │   └── cli-tree.txt              # The command tree it pins.
│   ├── server/                       # The service process.
│   │   ├── mod.rs                    # Start/bind/serve, Options; the one place that assembles
│   │   │                             # the concrete storage and runtime.
│   │   ├── app/
│   │   │   ├── mod.rs                # App, ownership of the store, shared in-memory state; explicit locks.
│   │   │   ├── lifecycle.rs          # Startup/recovery/watch/poll/shutdown, in today's order.
│   │   │   ├── record.rs             # App::record/complete/may_manage, post-commit order.
│   │   │   └── progress.rs           # Pending hand-offs, picked-up tracking, reminders run.
│   │   ├── access/                   # The whole security spine: who calls, what they may do and see.
│   │   │   ├── mod.rs                # App::who/user/view/place/one_place/see_roots.
│   │   │   ├── auth.rs               # Token, random sessions, one-time codes, modes, atomic saves.
│   │   │   ├── caller.rs             # Unix peer identity, proof store and listener (service side).
│   │   │   ├── pins.rs               # Session-root pins, start-time checks; pins file unchanged.
│   │   │   └── tests.rs              # Identity/auth/parser tests; fake process reader.
│   │   ├── http/                     # Transport wiring only; no domain policy.
│   │   │   ├── mod.rs                # router() and the machine_router() allow-list; no route discovery.
│   │   │   ├── guard.rs              # Host/Origin/proof/header extraction.
│   │   │   ├── session.rs            # Session status/code/exchange routes.
│   │   │   ├── error.rs              # ApiError, status mapping, blocking helper.
│   │   │   └── assets.rs             # Embedded web dist, MIME/caching, SPA fallback.
│   │   ├── planning/
│   │   │   └── mod.rs                # Intent/project/workspace/start/merge handlers.
│   │   ├── work/
│   │   │   └── mod.rs                # Append, task show/drop, decisions, gates, chores.
│   │   ├── conversations/
│   │   │   ├── mod.rs                # Ask/end/reply/message/read handlers, shared state.
│   │   │   ├── questions.rs          # Start/hold/retry/answer/deadline/idle close, same order.
│   │   │   ├── messages.rs           # Manager conversation send/hold/delivery/failure.
│   │   │   └── prompts.rs            # Ask, follow-up and answer-back prompts.
│   │   ├── commands/
│   │   │   ├── mod.rs                # Prepare/run/read/focus handlers; the user check in each.
│   │   │   ├── shell.rs              # Command checks, marker parsing, polling, safe close.
│   │   │   └── review.rs             # The temporary reviewer's prompt/result/deadline.
│   │   ├── machines/                 # Machine observation and its handlers; no runtime protocol.
│   │   │   ├── mod.rs                # Observation types, machine and settings handlers.
│   │   │   └── metrics/
│   │   │       ├── mod.rs            # Sampling worker (report types in common/wire/metrics.rs).
│   │   │       ├── probe.rs          # Probe/ps snippets, parsers, readings.
│   │   │       ├── alerts.rs         # Thresholds, validation, window/hysteresis.
│   │   │       ├── history.rs        # Minute folding, JSONL read/retention, modes.
│   │   │       └── tests.rs          # Today's 28 metrics cases.
│   │   ├── files/                    # Attachments: handlers and the bytes on disk.
│   │   │   ├── mod.rs                # Upload/inbox/fetch handlers, scoped checks.
│   │   │   ├── store.rs              # Paths, content ids, atomic put, usage.
│   │   │   └── tests.rs              # Modes, content, dedup, concurrent writes.
│   │   ├── views/                    # What the page and the CLI read; git attribution; caches.
│   │   │   ├── mod.rs                # Workspace build, project/group derivation.
│   │   │   ├── handlers.rs           # Workspace/state/code/metrics/history/settings reads.
│   │   │   ├── code.rs               # A unit's code, light snapshots, code keys, incremental reads.
│   │   │   ├── cache.rs              # Branch/commit/tree/remote caches, persisted format.
│   │   │   ├── task.rs               # Builds the task view (its shape is common/wire/task.rs, its
│   │   │   │                         # text the CLI's display.rs).
│   │   │   └── tests.rs              # Projection/cache fixtures.
│   │   ├── delivery/                 # Pure delivery plans and text.
│   │   │   ├── mod.rs                # Plan, event-to-recipient planning.
│   │   │   ├── messages.rs           # Decision/gate/task/manager templates, all through quote.
│   │   │   ├── reminders.rs          # Stall/idle/merge reminder decisions.
│   │   │   ├── quote.rs              # The one sanitizer for authored text.
│   │   │   └── tests.rs              # Plans/messages/stall cases.
│   │   └── tests/                    # The in-process daemon tests, compiled with the binary under cfg(test).
│   │       ├── mod.rs                # Declares the suites only.
│   │       ├── support.rs            # Fake, Kernel, daemon constructors, git helpers, HTTP client.
│   │       ├── guards.rs             # Phase 0's service guards: routes and methods, who may
│   │       │                         # use each route, the page's JSON (today: src/tests/guards.rs).
│   │       ├── route-access.txt  page-json.txt   # What they pin.
│   │       ├── planning.rs  work.rs  access.rs  conversations.rs
│   │       ├── commands.rs  machines.rs  files.rs  views.rs
│   ├── runtime/                      # Adapting outside programs: the facade and composition.
│   │   ├── mod.rs                    # Neutral construction/context/wording facade (including where
│   │   │                             # this process runs); picks today's only adapter.
│   │   ├── follow.rs                 # Several machines as one Runtime; routing by machine.
│   │   ├── ssh.rs                    # Tunnel supervision, publication, reconnect timing.
│   │   └── herdr/                    # Private; nobody outside runtime/ imports it. ACP goes beside it later.
│   │       ├── mod.rs                # Herdr implements Runtime; protocol values to workriver values.
│   │       ├── protocol.rs           # Socket discovery, protocol 22, framing, checked RPC.
│   │       ├── agents.rs             # Snapshot normalization, roots, states, names, ids.
│   │       ├── terminals.rs          # Send/focus/temporary agents/shells, ownership checks.
│   │       ├── remote.rs             # Remote socket discovery/construction, legacy option mapping.
│   │       ├── words.rs              # Herdr's own refusal text, CLI context/help, agent hints.
│   │       └── tests.rs              # Protocol fixtures; no real Herdr panes.
│   └── common/                       # Used by both sides, owned by no domain; mod.rs: BUILD.
│       ├── config/
│       │   ├── mod.rs                # Settings file read/update; keys and defaults unchanged.
│       │   └── defaults.rs           # Tunable defaults now scattered (timeouts, caps).
│       ├── home.rs                   # The data directory (data_dir(), moved from core) and the name of
│       │                             # every file in it (token, sessions, pins, port file,
│       │                             # workriver.db, …), in one place; it makes the directory
│       │                             # (0700) and the database file (0600) before the store opens.
│       ├── process.rs                # Proc/Chain/Procs/System, libproc/procfs, the remote chain
│       │                             # snippet/parser, the bounded child-output helper (output_within).
│       ├── files.rs                  # The type a file is sent and served as (mime), for both sides.
│       ├── diagnostics/
│       │   ├── mod.rs                # Performance log init/writing.
│       │   └── report.rs             # Perf summary/bundle/parsing for the CLI.
│       └── wire/                     # What service, CLI and page agree on: JSON shapes and the
│                                     # constants both sides use (header values, limits); no logic.
│           ├── mod.rs                # Shared values: EXPIRED (the proof header), MAX_ATTACHMENT.
│           ├── task.rs               # The task view's shape (TaskShow and its parts) and Stall.
│           ├── workspace.rs          # The snapshot DTOs (today in snapshot.rs); names and serde unchanged.
│           └── metrics.rs            # The metrics report types.
└── tests/                            # Process-boundary tests running the real binary.
    ├── caller.rs                     # Real CLI/socket/processes against a stand-in runtime socket.
    ├── boundaries.rs                 # The runtime-name ratchet over the repository (phase 0).
    ├── cli.rs                        # The CLI's text and the user's machine changes (phase 0).
    └── output.rs                     # serve prints nothing that signs anyone in.
```

Today's `machines.rs` is not renamed into a generic runtime: settings go to `common/config`,
SSH supervision to `runtime/ssh.rs`, several-machines delegation to `runtime/follow.rs`,
**all** remote Herdr construction, discovery and options to `runtime/herdr/remote.rs`;
observation and metrics stay in `server/machines/`. `common/config/defaults.rs` gathers
tunable defaults, not every constant: serde defaults stay on their types; wire names,
protocol versions and security invariants stay with their owner.

### The web UI

```text
frontend/web/
├── package.json / package-lock.json  # Today's dependencies; Vitest/happy-dom dev-only (phase 0).
├── vite.config.ts                    # Build, proxy, the `test` block; aliases only if paths move.
├── tsconfig*.json / components.json  # Type and shadcn setup; ui/utils aliases follow the moves.
└── src/
    ├── main.tsx                      # Start i18n/theme/session, mount the app.
    ├── index.css                     # Themes/layout/accessibility; selectors unchanged.
    ├── app/                          # Composition and current selection; no business rules.
    │   ├── App.tsx  Side.tsx  actions.ts  useWorkspace.ts  link.ts
    ├── api/                          # The only way to the service; no domain UI imports.
    │   ├── http.ts                   # GET/POST, errors, SIGNED_OUT.
    │   ├── session.ts                # One-time code exchange, the remembered deep link.
    │   └── actions.ts                # Today's runtime.ts actions, focus included.
    ├── workspace/                    # Snapshot loading and the optimistic overlay; no components.
    │   ├── types.ts  source.ts  edits.ts
    ├── planning/                     # types.ts select.ts Rail.tsx IntentPage.tsx Start.tsx Merge.tsx
    ├── work/                         # types.ts select.ts growth.ts (today's scope.ts) actions.ts
    │                                 # Activity.tsx layout.ts Nodes.tsx TaskDetail.tsx GateForm.tsx Findings.tsx
    ├── attention/                    # select.ts Inbox.tsx Backlog.tsx
    ├── agents/                       # types.ts select.ts Agents.tsx Mark.tsx (workriver's agents, not Herdr's)
    ├── conversations/                # types.ts select.ts Ask.tsx Turns.tsx; never imports app/
    ├── code/                         # types.ts files.ts load.ts useCode.ts CodeLayer.tsx Commits.tsx select.ts
    ├── attachments/                  # types.ts Viewer.tsx Thumbs.tsx preview.ts; Viewer calls onAsk, never Ask
    ├── commands/                     # types.ts Run.tsx context.ts
    ├── machines/                     # types.ts format.ts Machines.tsx Settings.tsx
    ├── session/                      # Session.tsx
    ├── content/                      # Markdown.tsx Prose.tsx Chips.tsx; no api or domain UI imports
    ├── ui/                           # primitives/ (the eleven shadcn files, unchanged) Choice.tsx
    │                                 # Icon.tsx CopyButton.tsx Tip.tsx; never feature/data/API imports
    ├── lib/                          # utils.ts useResize.ts isField.ts (out of Ask.tsx)
    ├── config/defaults.ts            # Quiet/resize/cache/preview defaults, values unchanged.
    ├── theme.ts
    ├── i18n/                         # en, zh-CN, ja, index, the i18next declaration; keys unchanged.
    └── mock/                         # demo.json, lockscreen.svg, metrics.ts: sample content.
```

The shadcn primitives are `button`, `collapsible`, `dialog`, `input`, `scroll-area`,
`select`, `sheet`, `switch`, `toggle-group`, `toggle` and `tooltip`. UI composition runs one
way: app → domain views → shared content and controls. Pure selectors import no views, no
app, no API actions, no mocks. `isField` goes to `lib/` and `Thumbs` to `attachments/`,
which breaks today's Ask → Inputs → Viewer → Ask cycle. `api/actions.ts` stays the one
action boundary; moving it gives no component a reason to fetch on its own. Per-call 401
behaviour, CSS selectors, localStorage keys, lazy loading of the Markdown renderer and the
code texts stay. The `*.test.ts(x)` files sit beside what they test and move with it.

### Skills and documents

```text
skills/
├── workriver-manager/SKILL.md, references/worker.md   # Names unchanged.
├── workriver-backlog-import/SKILL.md
└── workriver-howto/
    ├── SKILL.md                      # The topic index; links follow the grouped paths.
    ├── CHANGES.md                    # Kept whole, chronological.
    └── references/                   # Grouped by work domain; topic = file basename, unchanged.
        ├── planning/        start-intent, units, units-other-machine, spike, closing-units
        ├── workers/         hand-off, closing-workers, stalled, branch-moved, commit-link
        ├── decisions/       design-gate, ask-user, gates-chores, deciding-findings, deferred-batches, push-pr-merge
        ├── reviews/         architecture-review, blind-review, contract, finding-additions
        ├── communication/   ask, user-message, not-delivered, blocked, attachments
        └── operations/      machines, updating
docs/
├── README.zh-CN.md, getting-started(.zh-CN).md, security(.zh-CN).md, images/
├── architecture.md / architecture.zh-CN.md   # A short code map for people (phase 7).
├── runtimes/herdr.md / herdr.zh-CN.md        # Everything runtime-specific in the human docs (phase 7).
└── design/                                   # English specifications for agents.
    ├── caller-identity.md                    # Kept whole; source paths updated.
    ├── restructure.md                        # This file.
    └── ui/ navigation, activity, code, conversations, operations, attachments, presentation (.md)
```

`workriver guide how <topic>` finds the embedded recipe whose basename is the topic (rust-embed's
own iterator, no manifest), asserts basenames are unique, and keeps the unknown-topic
fallback and the worker's one-line paragraph. DESIGN.md becomes the overview and reading
map; its normative paragraphs move once, whole, to `docs/design/ui/`.

## 3. The data port (phase 8)

Core defines `WorkStore` in `ports/data.rs` (proposal B, section 4B.2): read domain views at
one revision (owned values, never a borrowed replay cache), record an authenticated request
and return the committed change, read history by cursor, report storage status. No SQL,
connection, transaction, table or row id crosses it; no unchecked write path. The 52
`.state()` calls on the store in today's `serve.rs` become named reads, and its three direct
appends (in `App::record`, in the agents' poll and in delivery) go through the checked
record; `App::record` keeps completion and post-commit order outside the adapter. `State` stays the
pure aggregate used by the rules and by `/api/state`. This changes behaviour paths, so it
comes after every pure move and is reviewed on its own (u12#9 item 4).

## 4. The phases

Each phase is implemented, then gets a code review and an architecture review (at most three
rounds), all on `chore/restructure` (u12#9 item 8). The branch is merged and pushed once, at
the end, after the user's gate: asked on 2026-11-20 whether item 8's merge-and-push gate
comes after each phase or once on the one branch, the user chose once at the end. Every
commit is green on its own (section 5.3). Proposal B's commit numbers are in brackets.

| Phase | What moves | Done when |
|---|---|---|
| **0** | This plan; the guards of section 5; Vitest. Nothing moves. | u13#1 |
| **1. Core and scan by domain** | `model.rs` into `records/` (Body, Event, State) and `planning/`, `work/` (design ancestry in `graph.rs`), `conversations/`, `access/`; core tests to their owners, `src/tests/replay.rs` to `records/tests/replay.rs`; the temporary `model` facade removed at the end [B4–B8]. *As done (u13#10):* each kind's `prepare`/`apply` arm moved verbatim into a `prepare_<group>`/`apply_<group>` of its domain, records keeping the shared checks and one exhaustive dispatch; domain files read their neighbours with `use crate::*`; a helper is private when only its own file calls it, `pub(super)` when only its domain folder does (the caller imports it from its sibling), and `pub(crate)` only where a caller crosses a domain folder (u13#13); no `model` facade was needed (nothing outside core named it). Scan's `lib.rs` into `runner.rs`, `history/`, `files/` [B11]. | Replay fixture green unchanged; every core and scan test name present once; the real database's copy replays to `before-phase1.json` byte for byte (`cmp`, section 5.2). |
| **2. Storage out of core** | New crate `workriver-data` with `sqlite/{mod,migrations,events}`; rusqlite leaves core; the stored half of replay and the three `Connection::open` tests move there; core's Store-backed tests become pure State fixtures without losing a scenario. The service still holds the concrete store: no port yet [B9]. *As done (u13#15):* core's tests record through `Records`, an in-memory stand-in for what `Store::append` does (prepare, record, apply) that rebuilds the state from the events' stored form; the branch-rule test plants its old row that way (its SQL form is covered by the stored replay, whose fixture holds such a branch); `now()` and `data_dir()` stayed in core's `lib.rs`; the row's encoding (`insert`) sits beside its decoding (`replay`) in `sqlite/events.rs`, `Store::append` keeping prepare and apply (u13#18). | `cargo tree -p workriver-core` has no rusqlite; the real database's copy replays to `before-phase1.json` byte for byte (`cmp`, section 5.2). |
| **3. CLI and common** | `main.rs` into `cli/` (client, parsers, commands by domain, display, guide), `backlog.rs` into `cli/planning.rs`; CLI-only cases of `tests.rs` into `cli/tests.rs`, `src/tests/cli_guards.rs` with `cli-tree.txt` into `cli/guards.rs`; `common/` gets `home.rs`, `process.rs`, `diagnostics/` (today's `perf.rs`) [B12, B13, part of B2]. `cli/` imports nothing of the service. **A small change of workriver-data's API, named as such (decided in u13#17):** `Store::open_default` leaves workriver-data. `common/home.rs` takes `data_dir()` from core and, with the other files of the data directory, names `workriver.db`; it makes the directory (0700) and the file (0600) as `open_default` does today, and the composition calls `Store::open(path)` on that same path. workriver-data then opens only the file it is given; core keeps `now()`. *As done (u13#20):* each command's arm moved verbatim into run() of its domain file with its subcommand enums and helpers (a command that printed for itself returns `Ok(None)`); every file name of the data directory is in `home.rs` (with `read_token`, the inbox path and its `mkdir`, the port and host files over there; since u13#23 also the perf log's older files and (u13#25) its bundles, the scan cache's two files and the temporary names, built from the names they stand beside), except the ssh control sockets (`ssh-%C`), which workriver-scan's `Runner::ssh` names in the folder it is given: the runner owns its ssh options, and moving the name would change scan's API and the scan test that pins it; `home.rs` says so where it lists the files; the CLI's halves of service modules moved out of them (`caller::fetch` to `cli/client.rs`, show's text to `cli/display.rs`) and what both sides use moved to `common/` (the task view's shape and Stall to `wire/task.rs`, `EXPIRED` and the attachment limit to `wire/mod.rs`, `mime` to `files.rs`, `BUILD` to `common/mod.rs`; serve.rs keeps its own BUILD until phase 5). `cli/` names only `common/` and, to read where this process runs, `herdr::place_from_env` (phase 4's facade replaces it). A directory or file that cannot be made is still reported as workriver-data's `Error::Io`, so the startup failure reads exactly as before; that variant stays for it. | Every command and option unchanged (`every_command_keeps_its_options`, its file not edited); printed text and exit codes unchanged (`tests/cli.rs`, `tests/output.rs`, `tests/caller.rs`); the service opens the same file with the same modes; workriver-data names no directory or file of its own. |
| **4. Runtime** | `herdr.rs` and the remote Herdr parts of `machines.rs` into `runtime/herdr/`; `runtime/{mod,follow,ssh}.rs`; settings to `common/config/` (it gets its content here, so it is created here). **A change, reviewed as one:** sentences of the outer crate that name Herdr or panes outside the adapter come from it through the facade (the texts stay the same) [B14, B15]. Core is not touched: its four refusals that name the pane (in `model.rs` today: "…in its manager's pane", "run `workriver manager here` in its pane") cannot reach the facade, and no new mechanism is added to core for them. | The ratchet (section 5.1) lists, among Rust sources, only: core's files (those refusals, and comments naming the runtime, until phase 7), and tests that stand in for the runtime or assert its texts (the in-process daemon tests, `tests/caller.rs`, `tests/cli.rs`, `tests/output.rs`). Its `ADAPTER` becomes the folder. |
| *after 4* | **A unit of its own: workriver's own agent commands** (`start`, `prompt`, `read`, `close`, through the runtime facade). Designed first (who may use them, whether they are recorded, security, that ACP can carry them), architecture review, a design gate, then implemented; the `hand-off`, `closing-workers` and `stalled` recipes then use only workriver's commands (u12#9 item 6). | Its own design gate and reviews. |
| **5. Server by domain** | `serve.rs`, `ask.rs`, `run.rs`, `deliver.rs`, `quote.rs`, `stall.rs`, `snapshot.rs`, `show.rs`, `attachments.rs`, `auth.rs`, the service side of `caller.rs` and `metrics.rs` into `server/` (section 2); snapshot DTOs and metrics report types into `common/wire/`; `tests.rs` split into `server/tests/` by suite, first in the phase [B2, B16–B21]. Visibility narrowed with `pub(super)`/`pub(in …)`, never widened to `pub`. | Route table, page JSON shape and every u11 identity/blind test green unchanged; no handler lost its `App::user`/`App::who`/`App::view` call. |
| **6. Web UI by domain** | `types.ts`, `data.ts`, `runtime.ts`, `act.ts`, `scope.ts`, `link.ts` and the components into the tree of section 2; the Ask/Inputs/Viewer cycle broken; temporary re-export files removed at the end [B22–B24]. Pure functions first, components after; keys, providers and markup kept. | `npm test` green; the mock-page checklist (section 5.4) passes before and after. |
| **7. Documents and runtime-neutral text** | Recipes grouped, guide lookup by basename [B25]; DESIGN.md split into `docs/design/ui/`; AGENTS.md's layout and rules follow the tree; `docs/architecture(.zh-CN).md`. **A change:** the page names no runtime: neutral words, and where a name must be shown the runtime adapter supplies it; the runtime-specific parts of the human docs move to `docs/runtimes/herdr.md` and `.zh-CN.md` (u12#9 item 7). Core's four pane refusals get runtime-neutral words in the same change, and core's comments stop naming the runtime. i18n key names (`machines.noHerdr`) and CSS selectors stay: only texts count (section 5.1). | Every guide topic answers (the guide guard unedited); the ratchet lists no frontend file and no core file; English and Chinese docs in step. |
| **8. The data port** | Section 3 [B3, B10]. | Every projection, recovery result and post-commit effect order compared; reviewed apart. |

Phases 1 and 6 do not touch the same files and could run apart, but on one branch they are
done in this order unless the manager says otherwise. A step that cannot be reviewed as
moves plus a small named extraction is split.

## 5. The guards

Written in phase 0, before anything moves. A move keeps every one green **without editing
it**, except to follow a file it names. Changing what a guard expects is a change of
behaviour, made only on purpose and named in the commit.

| Guard | Where | Holds |
|---|---|---|
| Replay of a synthetic history | `workriver-core`: `src/tests/replay.rs` → `records/tests/replay.rs` (phase 1). Its stored half (`the_stored_history_reopens_to_the_same_state`) → `workriver-data`'s `sqlite/tests.rs` (phase 2): it reads core's fixture by path (`../workriver-core/src/records/fixtures/events.jsonl`, from its `CARGO_MANIFEST_DIR`) with a small decoder of its own (a line is a stored row: seq, at, actor, kind, body), and compares with a fold through core's public `State::apply`. No copy of the fixture, no test feature in core. | 96 stored events covering all 48 kinds, old encodings first (an intent with no state and a runtime ref, a manager registered for an intent, a task with no intent, an intent-level task, a decision with no `by`, a question with no `temporary`, a branch named before the rule, links without patch ids). Folded with `apply` only, and through rows of a reopened SQLite file. Pinned: the state, the command runs (never serialized with it), the blind view, the managers, how each event is written again (serde attributes), and the ids and refusals the rules give next. |
| Routes and methods | `src/tests/guards.rs` → `server/tests/guards.rs` (phase 5): `every_route_answers_only_its_methods` | The 34 routes of this machine's listener and the 13 of a followed machine's, each with exactly its methods, of all nine standard ones: HEAD where GET is and nowhere else (answering exactly what GET answers), any other undeclared method 405 (a POST or an OPTIONS added to a GET path fails); every local-only path refused by the machine listener's fallback; the sources declare no other `.route(` (this scan reads `src/` and stays with the route guard). |
| Who may use each route | Same file: `who_may_use_each_route`, with `route-access.txt` | Every declared route and method, and HEAD wherever GET is (each has a probe, or the test fails), gets a request its handler takes, from nobody, from an agent in its place, and from the user (token, a process in no agent's place); on a machine's listener the same, there through the combined runtime. The status each answers is pinned; and whatever the file says, nobody is answered with success except on `GET`/`HEAD /api/session`, `GET`/`HEAD /api/status`, `POST /api/here`. A handler that loses its `App::user`/`App::who`/`App::view` call fails here. |
| Guide topics | `src/tests/cli_guards.rs` → `cli/guards.rs` (phase 3): `the_guide_keeps_its_topics` | The 27 topics, each answering with its own recipe file, word for word, under a head naming it. Recipes are found and compared by basename wherever they sit, each name once, so grouping them into folders (phase 7) leaves the guard unedited. Every `guide` kind answers. |
| Every command and option | Same file: `every_command_keeps_its_options`, with `cli-tree.txt` | Every command of `Cli::command()` with its aliases, and every argument: long and short forms, aliases, positional index, value names, number of values, possible values, defaults, environment variable, action, required, global, hidden, conflicts. |
| Runtime names (a ratchet) | `tests/boundaries.rs` (an integration test: it reads the whole repository, so it never moves) | `herdr` (any case) and `pane(s)` appear outside the adapter only in the files listed; a new file naming them fails, and a listed file that stops naming them must leave the list (section 5.1). |
| The page's JSON | `guards.rs`: `the_json_the_page_reads_keeps_its_shape`, with `src/tests/page-json.txt` | Every key path and kind of value of `/api/workspace` and `/api/unit/{id}/code` on a workspace with most of what the page shows (projects, roadmap, backlog, a manager, committed/unregistered/uncommitted work, a research unit, a design, a blind review with decided findings, a waiting gate, a chore, an attachment, a question, a manager conversation, a second machine). A key that workspace leaves out is not pinned. |
| The CLI's text and machines | `tests/cli.rs` (real binary) | `project list`, `workspace list`, `backlog list`/`export`, `finding show`/`drop` (an agent refused), and the user adding, enabling, disabling and removing a machine down to `settings.json` and the tunnel's process. |
| The page's logic | `frontend/web/src/**/*.test.ts(x)`, `npm test` | Markdown safety (no raw HTML, no unsafe link protocols, `noopener noreferrer`, no image fetched), the signed-out screen and the ten-minute deep link, the destructive-command flag, addresses, scope growth, quiet, the content-language guess. |

`WORKRIVER_BLESS=1` rewrites the blessed files (the replay's expected state,
`route-access.txt`, `page-json.txt`, `cli-tree.txt`); it is for a deliberate change, never
during a move. Run the guards on their own (`guards` matches both guard modules):

```sh
cargo test -p workriver-core replay
cargo test -p workriver --bin workriver guards
cargo test -p workriver --test boundaries
cargo test -p workriver --test cli
(cd frontend/web && npm test)
```

### 5.1 The ratchet's lists

`ADAPTER`, `OWN_SENSE` and the two lists are at the top of `tests/boundaries.rs`. A moved
file's line follows it in the same commit; a file split in two keeps a line for each part
that still names the runtime. In phase 4, `ADAPTER` becomes a folder prefix
(`runtime/herdr/`). What may stay listed after each phase is in its done-when (section 4):
after phase 4, among Rust sources, core's files and the tests that stand in for the runtime
or assert its texts; after phase 7, only those tests.

What does not count:

- **i18n key names.** A dictionary's keys, and a quoted string that is one of those keys
  (`t('machines.noHerdr')`; the key set is read from `en.ts`), are names that stay
  (sections 1–2); their texts count. Any other quoted name counts: a runtime method such
  as `"pane.close"` outside the adapter fails. The frontend leaves the list when
  its texts and comments no longer name the runtime.
- **Words in workriver's own sense** (`OWN_SENSE`): in `CodeLayer.tsx`, "pane" is the file
  tree beside the diff, a layout term with a frozen selector (`data-tree="pane"`).
- **`tab` and `workspace` are not checked.** AGENTS.md names them with `pane` as Herdr's
  words, but workriver has workspaces of its own (an intent runs in one; `/api/workspace`,
  `workriver workspace`), so the word cannot tell the two apart, and `tab` names nothing of
  the runtime's outside the adapter today.

### 5.2 Replaying a copy of the real database

The one check that the user's own records still replay. `replays_a_copied_database` stays
`#[ignore]`d and is run by hand, on a **copy**: never on the live data home, never with a
service started on it.

**The copy and its baseline exist.** On 2026-11-20 the manager made a read-only copy with
SQLite's online backup (which reads the live file and writes nothing to it; a plain `cp`
can miss what is still in the WAL file) in the main checkout's `.local/replay/` (ignored by
git): `workriver.db`, and `before-phase1.json`, its replay on cbc5b16 (1 888 events). Every
phase compares with that baseline; nobody makes another copy for it, and nobody writes to
it. A later copy (for more events) is made the same way, by the user or with the user's
say-so, and gets its own baseline first:

```sh
sqlite3 "$HOME/workriver-data/workriver.db" ".backup '.local/replay/workriver.db'"
```

On the phase's last commit, with `REPLAY` the main checkout's `.local/replay`:

```sh
WORKRIVER_REPLAY_DB="$REPLAY/workriver.db" WORKRIVER_REPLAY_OUT=/tmp/after.json \
  cargo test -p workriver --bin workriver replays_a_copied_database -- --ignored
cmp "$REPLAY/before-phase1.json" /tmp/after.json
```

(Write the output to a scratch place of your own, not into `.local/replay`.) u13#4 ran it on
its commit: identical.

The test copies the file into a temporary directory before opening it (opening runs
migrations and writes a WAL), so even the copy stays as it was. `WORKRIVER_REPLAY_OUT` holds
the event count, the whole replayed state and the command runs. No phase may change it by a
byte.

### 5.3 Checks for every commit

With the runtime's variables unset (tests start their own stand-ins) and nothing on port
47820 touched:

```sh
env -u HERDR_ENV -u HERDR_PANE_ID -u HERDR_SOCKET_PATH -u HERDR_TAB_ID -u HERDR_WORKSPACE_ID \
  cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd frontend/web && npm run typecheck && npm run build && npm test
```

The number of tests never drops; a split or renamed test is renamed in the behaviour list
in the same commit (u12#3). Section 5.2's `cmp` is part of phases 1 and 2's done-when; run
it after phases 5 and 8 as well.

### 5.4 The mock-page checklist (frontend phases)

`npm run dev:mock` (sample data, no service). Before and after, the same sample, in English,
Chinese and Japanese, light and dark, wide and at 390 px:

1. `/u1`, `/u1/3`, `/i1`, `/i1/1`, `?finding=P2-1` open what they name; back and forward
   follow; a path naming nothing says so; "copy link" copies the address.
2. Rail: project switch, search, Active/Roadmap/Done, intent and unit marks, fold (⌘B),
   resize by mouse and keyboard, all remembered after a reload.
3. Graph: fit or latest work on first view, pan never reset after the user moved it, the
   MiniMap only when it does not fit, the legend folded and remembered, React Flow's
   attribution visible, quiet moon and scope-growth marks where they were.
4. Task detail: input/output, "Show more", evidence, findings with Accept/Revise/Reject/Defer
   and the optimistic update with Undo, a waiting gate's form, chore steps with copy and Run.
5. Code layer: tree with only the changed files by default, filter, a diff per pick, a
   single file with "Only the changes", picking lines and the floating "Ask why".
6. Ask: chips, a conversation opened and folded, the manager switch; a pasted image and a
   dropped file become chips; a stray drop does not open the file.
7. Viewer: ←/→, zoom, Escape returns focus; a Markdown attachment renders without its raw
   HTML and with no image fetched.
8. Run: the confirmation dialog shows the exact command, where, who wrote it, and the red
   flag for `rm -rf x`.
9. Needs you, Agents, Commits, Machines, Backlog and Settings panels open and show what they
   showed.
10. Signed out (the real service with no session): only the calm screen with
    `workriver open`.

The built chunks' hashes may change; the layout and behaviour may not.

## 6. Later, only places now (u12#9 item 9)

- **ACP runtime**: `runtime/acp/` beside `runtime/herdr/`; ACP's only standard transport is
  the agent as a child process over stdio, so it needs a small separate host process that
  holds the agents across service restarts. Today's `Runtime` trait is terminal-shaped and
  will grow then, not now.
- **A conversation page and transcripts**: transcripts are recorded apart from the event
  store (only approvals and agent start/close would be events); `conversations/` in the
  page is already the home.
- **Relational tables** behind `WorkStore`: only if measured, with the backup, import,
  compare and switch-over of proposal B section 4B.3; never inside this restructure.
- **i17, the mirror on AWS**: projections out, signed actions in, verified locally and
  still authorized by `App::user`. If AWS serves the page's script, a compromise of AWS can
  change that script, so what end-to-end encryption covers is decided first.
- "workriver does not start agents" (AGENTS.md) is rewritten when ACP lands; it already
  starts temporary ones.
