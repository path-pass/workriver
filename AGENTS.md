# workriver

A local, always-on tool that lets one person keep track of twenty or more coding agents
working in parallel: what each is doing now, how a piece of work came to be the way it is,
and which parts of the code it reached. It records and shows; it does not start or
schedule agents.

Status: first version usable. The daemon, the event store, the git scan, the CLI, the
Herdr runtime, delivery of the user's decisions to agents, the manager skill
(`skills/workriver-manager/`) and the web UI on real data work. People start with
`docs/getting-started.md`.

## Terms

- **Intent**: a goal in a project; it groups the units that serve it. It is planned in the
  project's roadmap until it is started in a workspace of that project.
- **Workspace**: where a project's intents run: one manager, one conversation with the
  user. workriver's own record (`ws1`: an id, a name that is only shown, a project), made
  by the user; not the agent runtime's group, which decides nothing.
- **Unit**: a piece of an intent that can be implemented on its own, on its own branch.
- **Task**: one piece of work handed to one agent (implement, review) or to the user (a
  gate); a node in a unit's graph. Not "bolt": in AI-DLC a bolt is a coarser delivery slice.
- **Finding**: a problem a review (or the user at a gate) found; it is accepted, rejected
  or deferred, and an accepted one leads to tasks.
- **Gate**: a task only the user can do (a manual deploy or try-out) that an agent waits on.
- **Evidence**: a check run on a task's work, the command and its result ("12 passed");
  recorded the same way whoever ran it.
- **Chore**: something an agent asks of the user that blocks nobody; it belongs to an
  intent and is never on a graph.

## Stack (decided)

- Service, CLI, runtime adapter: Rust, one Cargo workspace.
- UI: one web UI, written once: TypeScript, React, Vite, shadcn/ui (Radix + Tailwind),
  React Flow for the graph with @antv/layout's dagre for the initial placement, diff2html
  for diffs, i18next. Built to static files. React Flow's attribution stays visible (its
  authors ask for that unless one subscribes); do not hide it.
  The service serves them to a browser; the desktop and iOS apps are shells around the
  same build.
- Storage: one SQLite database outside the repositories it watches, append-only events.
- Agents report the content of their work (tasks, output, evidence, findings) by running
  the `workriver` CLI from their shell, which every harness can do. The service finds the
  caller's process itself (the kernel names who connected to its Unix socket) and the pane
  that process runs in (its chain of parents against the pane roots the runtime gives); the
  pane the CLI names (under Herdr: `HERDR_PANE_ID`) is only cross-checked. So no harness
  is configured or special-cased and nobody is anyone by saying so
  (`docs/design/caller-identity.md`). The instructions
  an agent needs travel inside the task message the manager sends it. MCP is not used
  for now; add it only if a harness that cannot run shell commands has to be supported.
- Each workspace has one manager agent, seated by the user (the page, or `workriver
  workspace manager` from the user's terminal; never by an agent). It splits the work of
  the intents that run there into units and tasks, hands tasks to agents through the
  runtime, and receives the user's accepted findings. workriver records and delivers; the
  manager schedules.
- Who is running, where, in what state, and delivering a message to an agent come from
  the agent runtime, not from per-harness hooks. The core defines a small runtime
  interface in workriver's own terms (agent, session, state, working directory, send a
  message, wait for a state); Herdr (through its socket API) is the first and only
  implementation, kept in one place so it can be replaced. No Herdr names (pane, tab)
  outside that implementation; workspace is workriver's own word and record, never the
  runtime's (the runtime's group of a terminal is shown as a hint only: no record holds
  it and no rule reads it). Agents not running under Herdr are out of scope for now.
- Later: a desktop app and an iOS app (Tauri is the candidate for both; not decided).

## Layout

```
docs/               documents for people, English and Chinese: getting-started, security,
                    architecture (the code map), runtimes/herdr (what is Herdr's own)
docs/design/        specifications for agents, English only: ui/ (the page's rules, one
                    file per part; DESIGN.md is their overview), layout (the code's
                    tree, file by file), guards (the tests that hold behaviour),
                    caller-identity
frontend/web/       the web UI; everything a user sees is built here
  src/              by domain: app/ (composition), api/ (the only way to the service),
                    workspace/, planning/, work/, attention/, agents/, conversations/,
                    code/, attachments/, commands/, machines/, session/; shared: content/,
                    ui/ (ui/primitives/: shadcn), lib/, config/, i18n/; mock/ (sample data)
frontend/desktop/   desktop shell around the web build (not created yet)
frontend/ios/       iOS shell around the web build (not created yet)
crates/             Rust workspace, four crates:
  workriver-core/   records/ (Body, Event, State), planning/, work/, conversations/,
                    choices/ (a question's options as recorded, a fixed A2UI v1.0 subset:
                    a record format and its write rule, no UI: core draws nothing and holds
                    no UI text), access/ (authority, the blind view), runtime.rs (the runtime
                    interface), ports/ (the data port: WorkStore, what the service asks of
                    storage)
  workriver-data/   the SQLite event store (sqlite/), which implements the data port
  workriver-scan/   git and the file tree: runner.rs, history/, files/
  workriver/        the binary: cli/ (the CLI process), server/ (the service: app/,
                    access/, http/, then a folder per domain), runtime/ (the facade;
                    herdr/, the one adapter, private to it), common/ (both sides: config/,
                    home.rs, process.rs, wire/, diagnostics/)
skills/             what agents read; the binary embeds all of it (`workriver guide …`):
                    workriver-manager (the role, the loop and the hard rules only; details
                    go in recipes), workriver-howto (one recipe per situation in
                    references/<group>/, the groups planning, workers, decisions, reviews,
                    communication, operations; and CHANGES.md), workriver-backlog-import.
                    A change in what agents must do goes into the recipe and CHANGES.md.
scripts/            migrate-virtual-workspaces/: the one-off conversion of a store of the
                    earlier form (two Cargo packages of their own, not in the workspace, not
                    built or shipped; README.md says how the user runs it once). Removed
                    once the user's store is migrated.
.local/             local-only working material; ignored by git, never commit it
```

Layering follows memriver: a core crate with models and write rules that knows nothing
about the network, the UI or how events are stored; a data crate with the SQLite store; a
scan crate that reads git and the file tree; an outer crate with the daemon, the CLI and
the Herdr runtime implementation. Dependencies run one way: the outer crate on the other
three, data on core, core and scan on neither. Inside the outer crate `cli/` and `server/`
never import each other (except the service's tests, which drive it through the CLI's
client), `runtime/` imports neither, `common/` none of the three, and only `runtime/`
imports `runtime/herdr/`. The service holds storage only as core's `ports::WorkStore`
(`App.store`): it reads what is recorded and records through that, and only `main.rs` and
`server/mod.rs`, where the service is put together, name workriver-data, besides tests; no
SQL, table or connection outside that crate. One record, or what a previous run left, is
read by the port's named reads (add one when it is missing); `recorded()`, the whole state,
only where a function of the whole state is called or several records of one revision are
needed. Every read may fail. Before a request's record is committed, a failed read fails
the request: the handler passes it on with `?` (500, never 422). The request's record is
the one it asks for: `agent.seen`, which `App::who` may record of the caller first, is not
it (a retry records it no second time), so the reads after it are still the request's own
and fail it. After the commit, and in
the poll and at start-up, it goes through `got`: logged, and the step skipped or ended as
a failure of that step is ended (a question fails, a review fails, a delivery says not
delivered); the request still answers as the committed one it is. A failed read is never
replaced by a value: no default harness, no empty text, no "not there". `docs/architecture.md` is the map;
`docs/design/layout.md` has the detail, file by file, and the data port. A change that only
moves code keeps the guards of `docs/design/guards.md` green without editing them.

## Language rules

- Code comments: English.
- Documents for agents (this file, `DESIGN.md`, `docs/design/`, skills, prompts): English.
- Documents for people: English and Chinese, as `name.md` and `name.zh-CN.md`, kept in
  step. Only the English `README.md` sits at the root. Its Chinese version and every
  other document live under `docs/`.
- UI text: never a literal in a component. Every string goes through the i18n
  dictionaries in `frontend/web/src/i18n/`; English, Chinese (zh-CN) and Japanese (ja) ship
  now and must stay in step. Sample data is content, not UI text.

## Working rules

- `.local/` holds plans, prototypes and research notes. It is in `.gitignore`. Put new
  material of that kind there, not in the tree that gets committed.
- Committed files (code, comments, tests, documents) name no local workriver id: no
  intent, unit, task, finding or message of this project's own records (`i<N>`, `u<N>`,
  `u<N>#<n>`, a finding of one, `m<N>`), which exist only on the machine that keeps them. Say what was
  decided or found instead. Generic examples that show the format (`u1#3`, `i3` in CLI
  help, recipes and tests) are not such ids. A commit message's `Task:` trailer stays:
  workriver links commits by it.
- A feature's design document lives on its branch. Before the branch is merged, its rules
  go into the living spec (`docs/design/ui/`, or the file of `docs/design/` for that part)
  and the design file is removed.
- Before the first push, the repository is cleaned: nothing local-only, no generated
  files, no third-party skill copies.
- Prefer an established, maintained library over writing the same thing by hand. Write
  it yourself only when the library would clearly cost performance or bundle size; say
  which library you turned down and why, with the number.
- Do not commit or push unless the user says so.
- Security rule (see `docs/security.md`): the service never takes anyone's word for who
  they are. Whatever acts as the user, or changes where workriver reaches (machines,
  links, projects), asks for the user in its handler (`App::user`, or `App::who` for what
  agents may record too) and is refused by the event rules for anyone else
  (`Body::user_only`). Never add a route that acts as the user without that; never serve
  such a route on a machine's listener (`machine_router` is an allow-list; add to it only
  what an agent's CLI on another machine needs); never pass an actor string taken from a
  request into `record` (it takes a `Principal`). Who a caller is
  comes from its process, never from the request: the CLI fetches a proof from the caller
  socket and sends it (`x-workriver-caller`); `App::who` finds the pane the process is in
  and refuses a request that names another (`x-workriver-place`, the one carrier of the
  named pane: never read a pane from a query or a body), one in no pane without a terminal,
  and a session held by another pane whose root runs; `App::user` goes through `App::who`.
  Every failure there is a refusal: no branch falls through to the user or to another
  agent. No harness is named in code or in a refusal; what a harness needs is in the
  `hand-off` recipe. Text written by agents or the user goes
  into messages only through `quote.rs`. An intent's project, the workspace it runs in and
  its manager are set only by the user or by a start the rules checked (`docs/security.md`),
  and all three are checked in core's `prepare`, against the store, when the event is
  recorded: a workspace is found by its id (`ws1`), never by anything the runtime says of
  its groups, and never taken from the request's word alone (it exists, is open, is of the
  intent's project, and for a manager's start is the one it holds the seat of, which the
  rules fill in). A manager's seat is set by one event, the user's only
  (`workspace.manager_set`): no agent event sets a seat, a workspace or a project, and a
  new kind that does one of these is `Body::user_only`. The service adds to a seating only
  what it reads of the runtime, for the user's own event (`App::seating`: the one root the
  session's machine shows it at; its machine and folder; the pin), never a seat. A seated
  session's root is kept (`App::one_place`, `see_roots`): it comes back by itself only on
  the seat's machine and in its folder, anything else is refused until the user seats it.
  What is sent to or asked under one appointment (`Seat.since`) never reaches another
  (`appointed`, `still_appointed`). The rules never read an intent's project from its units
  or from a folder no registered project holds (`intent_project`). Reading is
  scoped the same way: a route that
  returns records goes through `App::view` (the user sees all; an agent sees all but the
  conversations of workspaces whose seat it does not hold, `State::conversations_of`,
  unless it is on an undelivered blind task, then only `State::blind_view`; nobody else
  reads). A new read route that skips it breaks blind reviews and lets one workspace's
  manager read another's conversation.
- Read `DESIGN.md`, and the file of `docs/design/ui/` for the part you change, before
  changing anything in `frontend/`.
- Test data files live in a `__fixtures__/` folder beside the tests that read them.
- Runtimes come from mise (node, rust). Do not install tools globally; keep dependencies
  inside the project.
