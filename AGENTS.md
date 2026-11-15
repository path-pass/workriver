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

- **Intent**: a high-level goal; it groups the units that serve it. One intent is one Herdr
  workspace, a mapping that lives only in the runtime adapter.
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
  the `workriver` CLI from their shell, which every harness can do. The CLI knows which
  agent called it from the runtime's environment (under Herdr: `HERDR_PANE_ID`,
  `HERDR_WORKSPACE_ID`), so no harness is configured or special-cased. The instructions
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
skills/             the manager agent's skill; the binary embeds it (`workriver guide`)
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
- Read `DESIGN.md` before changing anything in `frontend/`.
- Runtimes come from mise (node, rust). Do not install tools globally; keep dependencies
  inside the project.
