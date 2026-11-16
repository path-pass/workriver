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
  project's roadmap until a Herdr workspace's manager starts it.
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
  the `workriver` CLI from their shell, which every harness can do. The CLI sends where
  it runs (under Herdr: `HERDR_PANE_ID`) and the service finds the agent there, so no
  harness is configured or special-cased and nobody is anyone by saying so. The instructions
  an agent needs travel inside the task message the manager sends it. MCP is not used
  for now; add it only if a harness that cannot run shell commands has to be supported.
- Each intent (one Herdr workspace) has one manager agent of its own. It registers itself
  with the CLI, splits the work into units and tasks, hands tasks to agents through the
  runtime, and receives the user's accepted findings. workriver records and delivers; the
  manager schedules.
- Who is running, where, in what state, and delivering a message to an agent come from
  the agent runtime, not from per-harness hooks. The core defines a small runtime
  interface in workriver's own terms (agent, session, state, working directory, send a
  message, wait for a state); Herdr (through its socket API) is the first and only
  implementation, kept in one place so it can be replaced. No Herdr names (pane, tab,
  workspace) outside that implementation. Agents not running under Herdr are out of
  scope for now.
- Later: a desktop app and an iOS app (Tauri is the candidate for both; not decided).

## Layout

```
docs/               documents for people, English and Chinese
frontend/web/       the web UI; everything a user sees is built here
frontend/desktop/   desktop shell around the web build (not created yet)
frontend/ios/       iOS shell around the web build (not created yet)
crates/             Rust workspace: workriver-core, workriver-scan, workriver
skills/             what agents read; the binary embeds all of it (`workriver guide …`):
                    workriver-manager (the role, kept under 600 words), workriver-howto (one
                    recipe per situation in references/, and CHANGES.md), workriver-backlog-import.
                    A change in what agents must do goes into the recipe and CHANGES.md.
.local/             local-only working material; ignored by git, never commit it
```

Layering follows memriver: a core crate with models, storage and write rules that knows
nothing about the network or the UI; a scan crate that reads git and the file tree; an
outer crate with the daemon, the CLI and the Herdr runtime implementation.

## Language rules

- Code comments: English.
- Documents for agents (this file, `DESIGN.md`, skills, prompts): English.
- Documents for people: English and Chinese, as `name.md` and `name.zh-CN.md`, kept in
  step. Only the English `README.md` sits at the root. Its Chinese version and every
  other document live under `docs/`.
- UI text: never a literal in a component. Every string goes through the i18n
  dictionaries in `frontend/web/src/i18n/`; English and Chinese (zh-CN) ship now and must stay in
  step. Sample data is content, not UI text.

## Working rules

- `.local/` holds plans, prototypes and research notes. It is in `.gitignore`. Put new
  material of that kind there, not in the tree that gets committed.
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
  request into `record` (it takes a `Principal`). The CLI sends its pane with every
  request (`x-workriver-place`); `App::user` refuses a caller whose pane holds an agent. Text written by agents or the user goes
  into messages only through `quote.rs`. Reading is scoped the same way: a route that
  returns records goes through `App::view` (the user sees all; an agent sees all unless it
  is on an undelivered blind task, then only `State::blind_view`; nobody else reads). A new
  read route that skips it breaks blind reviews.
- Read `DESIGN.md` before changing anything in `frontend/`.
- Runtimes come from mise (node, rust). Do not install tools globally; keep dependencies
  inside the project.
