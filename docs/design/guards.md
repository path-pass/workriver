# The guards

Tests that hold what other agents, the page and the user's stored records depend on: the
meaning of stored events, the routes and who may use them, the JSON the page reads, the
guide's topics, the CLI's commands and text, and where the runtime's names may appear. For
agents who change or move code. Where each file lives is in `layout.md`.

## The rule

A change that only moves code keeps every guard green **without editing it**, except to
follow a file it names. Changing what a guard expects is a change of behaviour, made only on
purpose and named in the commit. A move never drops a test: a split test keeps every case,
a renamed one says so in its commit.

`WORKRIVER_BLESS=1` rewrites the blessed files (the replay's expected state,
`route-access.txt`, `page-json.txt`, `cli-tree.txt`); it is for a deliberate change, never
during a move.

`cargo fmt` is not used in this repository (it would rewrite every file); moved code keeps
its lines as they are. Review a move with rename and move detection:
`git show --color-moved=dimmed-zebra --color-moved-ws=ignore-all-space <commit>`.

## What each holds

| Guard | Where | Holds |
|---|---|---|
| Replay of a synthetic history | `workriver-core`: `records/tests/replay.rs`, with `records/fixtures/events.jsonl` and `expected-state.json`. Its stored half, `the_stored_history_reopens_to_the_same_state`, is workriver-data's `sqlite/tests.rs`: it reads core's fixture by path with a small decoder of its own (a line is a stored row: seq, at, actor, kind, body) and compares with a fold through core's public `State::apply`. No copy of the fixture, no test feature in core. | Stored events covering every event kind, old encodings first (an intent with no state, a task with no intent, an intent-level task, a decision with no `by`, a question with no `temporary`, a branch named before the rule, links without patch ids), then every kind of today: the workspace kinds (made, renamed, closed, opened, a seat set, kept and emptied, a manager back, an intent moved) and a message to every manager among them. Events from before virtual workspaces (a runtime ref, a registration, a link) are not in it: the product does not read that form (below), and fixtures of it lived with the migration script only (commit 7cdf576, removed since). Folded with `apply` only, and through rows of a reopened SQLite file. Pinned: the state, the command runs (never serialized with it), the blind view, the managers, how each event is written again (serde attributes), the intents' versions (derived, never serialized), and the ids and refusals the rules give next. A new event kind is added to the fixture. |
| The store's format | `workriver-data`: `sqlite/tests.rs`, `a_store_without_the_format_mark_is_refused_and_never_marked` | A store with events and without the format's schema step is refused, naming the migration script and the commit that holds it, and is left as it is (not marked, not written) however often it is opened; a new store, and one with no events yet, is marked. |
| Routes and methods | `server/tests/guards.rs`: `every_route_answers_only_its_methods` | Every route of this machine's listener and of a followed machine's, each with exactly its methods, of all nine standard ones: HEAD where GET is and nowhere else (answering exactly what GET answers), any other undeclared method 405; every local-only path refused by the machine listener's fallback; the sources declare no other `.route(`. |
| Who may use each route | Same file: `who_may_use_each_route`, with `route-access.txt` | Every declared route and method, and HEAD wherever GET is (each has a probe, or the test fails), gets a request its handler takes, from nobody, from an agent in its place, and from the user (token, a process in no agent's place); on a machine's listener the same, there through the combined runtime. The status each answers is pinned; and whatever the file says, nobody is answered with success except on `GET`/`HEAD /api/session`, `GET`/`HEAD /api/status`, `POST /api/here`. A handler that loses its `App::user`/`App::who`/`App::view` call fails here. |
| The page's JSON | Same file: `the_json_the_page_reads_keeps_its_shape`, with `page-json.txt` | Every key path and kind of value of `/api/workspace` and `/api/unit/{id}/code` on a workspace with most of what the page shows (projects, roadmap, backlog, a manager, committed/unregistered/uncommitted work, a research unit, a design, a blind review with decided findings, a waiting gate, a chore, an attachment, a question, a discussion of a delivered research with an unread answer and a held question, a manager conversation, a second machine). A key that workspace leaves out is not pinned. |
| A2UI v1.0 (the stored choice) | `workriver-core`: `choices/tests.rs`, `every_choice_is_valid_a2ui_v1_0`, with the schemas in `choices/__fixtures__/a2ui-v1_0/` (their source commit in the README there) | Every choice core builds (2 to 8 options, with and without notes) is a valid A2UI v1.0 `createSurface` message, and the Button's answer a valid action, by the `jsonschema` crate (a dev-dependency); a shape outside the spec is refused by the same schemas. Keeps the stored form one that A2UI's own renderer can draw. |
| Guide topics | `cli/guards.rs`: `the_guide_keeps_its_topics` | Every topic, each answering with its own recipe file, word for word, under a head naming it. Recipes are found and compared by basename wherever they sit, each name once. Every `guide` kind answers. Adding or removing a recipe changes the topic list in the same commit. |
| Every command and option | Same file: `every_command_keeps_its_options`, with `cli-tree.txt` | Every command of `Cli::command()` with its aliases, and every argument: long and short forms, aliases, positional index, value names, number of values, possible values, defaults, environment variable, action, required, global, hidden, conflicts. |
| Runtime names (a ratchet) | `tests/boundaries.rs` (an integration test: it reads the whole repository) | `herdr` (any case) and `pane(s)` appear outside the adapter only in the files listed; a new file naming them fails, and a listed file that stops naming them must leave the list (below). |
| The CLI's text and machines | `tests/cli.rs` (real binary) | `project list`, `workspace list`, `backlog list`/`export`, `finding show`/`drop` (an agent refused), and the user adding, enabling, disabling and removing a machine down to `settings.json` and the tunnel's process; a decide gate with neither `--go` nor `--option`, with one `--option`, with both, and `--go` elsewhere, each refused before anything is sent. |
| The page's logic | `frontend/web/src/**/*.test.ts(x)`, `pnpm test` | Markdown safety (no raw HTML, no unsafe link protocols, `noopener noreferrer`, no image fetched), the signed-out screen and the ten-minute deep link, the destructive-command flag, addresses, scope growth, quiet, the content-language guess, what an intent's edit sends and the word diff of its history, what was added to a task. |

Run the guards on their own (`guards` matches both guard modules):

```sh
cargo test -p workriver-core replay
cargo test -p workriver-core every_choice_is_valid_a2ui
cargo test -p workriver-data stored_history
cargo test -p workriver --bin workriver guards
cargo test -p workriver --test boundaries
cargo test -p workriver --test cli
(cd frontend/web && pnpm test)
```

## The ratchet's lists

`ADAPTER`, `FACADE`, `OWN_SENSE` and the two lists (`STILL_HERDR`, `STILL_PANE`) are at the
top of `tests/boundaries.rs`. `ADAPTER` is a folder prefix (`runtime/herdr/`). Of the facade
`runtime/mod.rs`, only the two lines that declare and pick the adapter's module are let
through (`FACADE`); every other line of it counts. A moved file's line follows it in the same
commit; a file split in two keeps a line for each part that still names the runtime. What is
listed now are only the tests that stand in for the runtime or assert its texts: no frontend
file and no core file.

What does not count:

- **i18n key names.** A dictionary's keys, and a quoted string that is one of those keys
  (`t('machines.noHerdr')`; the key set is read from `en.ts`), are names that stay; their
  texts count. Any other quoted name counts: a runtime method such as `"pane.close"` outside
  the adapter fails.
- **Words in workriver's own sense** (`OWN_SENSE`): in `CodeLayer.tsx`, "pane" is the file
  tree beside the diff, a layout term with a frozen selector (`data-tree="pane"`).
- **`tab` and `workspace` are not checked.** AGENTS.md names `tab` with `pane` as Herdr's
  words and says `workspace` is workriver's own word too: workriver has workspaces of its
  own (an intent runs in one; `/api/workspace`, `workriver workspace`), so the word cannot
  tell the two apart, and `tab` names nothing of the runtime's outside the adapter (what is
  left of the word there is the browser's tab).

## Replaying a copy of the real database

The one check that the user's own records still replay, for a change that touches events,
the rules, replay or storage. `replays_a_copied_database` (`server/tests/work.rs`) is
`#[ignore]`d and is run by hand, on a **copy**: never on the live data home, never with a
service started on it.

The copy is made with SQLite's online backup (it reads the live file and writes nothing to
it; a plain `cp` can miss what is still in the WAL file), into the main checkout's
`.local/replay/` (ignored by git), by the user or with the user's say-so. A copy gets its
baseline first: its replay on the commit before the change. Nobody writes to a copy or its
baseline afterwards.

```sh
sqlite3 "$HOME/workriver-data/workriver.db" ".backup '.local/replay/workriver.db'"
```

Then, with `REPLAY` the main checkout's `.local/replay` and the output in a scratch place of
your own (never in `.local/replay`):

```sh
WORKRIVER_REPLAY_DB="$REPLAY/workriver.db" WORKRIVER_REPLAY_OUT=<scratch>/after.json \
  cargo test -p workriver --bin workriver replays_a_copied_database -- --ignored
cmp "$REPLAY/<baseline>.json" <scratch>/after.json
```

The test copies the file into a temporary directory before opening it (opening runs
migrations and writes a WAL), so even the copy stays as it was. `WORKRIVER_REPLAY_OUT` holds
the event count, the whole replayed state and the command runs. A change that is not meant
to change what stored records mean leaves it identical, byte for byte.

## Checks for every commit

With the runtime's variables unset (tests start their own stand-ins) and nothing on port
47820 touched:

```sh
env -u HERDR_ENV -u HERDR_PANE_ID -u HERDR_SOCKET_PATH -u HERDR_TAB_ID -u HERDR_WORKSPACE_ID \
  cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd frontend/web && pnpm typecheck && pnpm build && pnpm test
```

Two runs of the suite may overlap (several agents test at once): a test counts and talks
only to what it started. `tests/cli.rs` gives its tunnel a sleep no other run has
(`6417.<pid>`) and counts only that. A request refused from its headers alone (or a record
larger than taken) gets its refusal through the CLI's own client, however large its body up
to the largest attachment, and above that the CLI says the service closed the connection:
`server/tests/access.rs`, `a_refused_large_request_gets_its_refusal` (the service reads what a
handler leaves of a body before it answers, `server/http/drain.rs`). A client that asks before
it sends (Expect: 100-continue) and is refused from its headers gets the refusal at once, not
an invitation to send: `a_request_that_asks_first_is_refused_before_it_sends`.

## The mock-page checklist (a change to the page that should show nothing new)

`pnpm dev:mock` (sample data, no service). Before and after, the same sample, in English,
Chinese and Japanese, light and dark, wide and at 390 px; shoot the before build twice for a
noise baseline (relative times move with the clock, live marks animate):

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
   single file with "Only the changes", picking lines and the floating "Ask the manager".
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
