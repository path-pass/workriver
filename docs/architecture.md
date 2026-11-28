# How the code is laid out

[中文](architecture.zh-CN.md)

A short map for people who want to find their way around workriver's source. The full
tree, with a line for each file, is in `design/layout.md` (for agents, in English).

workriver is one Rust binary with entry points for the service, the CLI and the internal
per-agent host. The service serves the web page as static files. Domains such as planning,
work and conversations have the same folder names across layers.

## The parts

| Where | What it is |
|---|---|
| `crates/workriver-core/` | What workriver records and the rules for it, with no network, storage or UI: `records/` (the events and the state they add up to), `planning/` (projects, intents, units, and the templates that place a unit's steps), `work/` (tasks, findings, gates, chores), `conversations/` (questions, messages, attachments' records), `hosted/` (the receipts of the agents workriver holds itself), `choices/` (a question's options as recorded, a fixed A2UI v1.0 subset: its write rule only; core draws nothing), `access/` (who may do what, the blind view), `runtime.rs` (the runtime interface, in workriver's own words), `screens/` (what an agent's terminal shows: a dialog read off it, the keys workriver may press), `ports/` (the data port: what the service asks of whatever keeps the records) |
| `crates/workriver-data/` | The event store: one SQLite file, append-only (`sqlite/`). It implements core's data port, which is all the service knows of it |
| `crates/workriver-scan/` | Reads git and the file tree, here or over ssh: `runner.rs`, `history/`, `files/` |
| `crates/workriver/src/cli/` | The `workriver` command: parses, proves who calls, asks the service, prints |
| `crates/workriver/src/server/` | The service: `app/` (start-up, recording, the poll), `access/` (who calls and what they may do and see), `http/` (routes and the request guard), `schedule/` (jobs by the clock: their timers, one run at a time, the last result), then a folder per domain (`planning/` (with the templates' YAML), `work/`, `conversations/`, `commands/`, `terminals/` (an agent's terminal from the page: its screen, keys, view), `machines/`, `files/`, `views/`, `delivery/`, `hosted/` (the agents workriver holds itself: start, close, reconciliation)) |
| `crates/workriver/src/runtime/` | The agent runtime facade includes `follow.rs` (several machines as one), `ssh.rs` (tunnels), `herdr/` (the [Herdr runtime adapter](runtimes/herdr.md)), `acp/` (the [hosted Codex process and journal](design/hosted-agent.md), and the service's side of the hosts) and `composite.rs` (terminals and hosts as one runtime) |
| `crates/workriver/src/common/` | What the service and the CLI share: settings (`config/`), the data directory's files (`home.rs`), processes, and the JSON shapes both sides and the page agree on (`wire/`) |
| `crates/workriver/tests/hosted.rs` | Process-boundary tests for the host with a scripted Codex app-server in `tests/__fixtures__/hosted_app_server.rs` |
| `crates/workriver/tests/hosted_service.rs` | The real service, CLI and hosts end to end: a hosted worker's start, attribution, approvals, restart and close |
| `frontend/web/src/` | The page: `app/` (the shell), `api/` (the only way to the service), `workspace/` (loading the snapshot), a folder per domain (`planning/`, `work/`, `attention/`, `agents/`, `conversations/`, `code/`, `attachments/`, `commands/`, `machines/`, `session/`), then shared parts (`content/`, `ui/`, `lib/`, `config/`, `i18n/`) and `mock/` (sample data) |
| `skills/` | What agents read, built into the binary (`workriver guide`): the manager's role, one recipe per situation (grouped in `workriver-howto/references/`), the backlog import |
| `docs/` | Documents for people (this one, getting started, security, the runtime) and, in `design/`, the specifications agents work from |

## Which way the dependencies go

```text
workriver (the binary) ──► workriver-core
                       ──► workriver-data ──► workriver-core
                       ──► workriver-scan
workriver-core and workriver-scan depend on no crate of ours.

inside the binary:
  cli/    ──► runtime/ (the facade) ──► runtime/herdr/
  server/ ──► runtime/
  main.rs (host command) ──► runtime/ ──► runtime/acp/
  cli/, server/, runtime/ ──► common/
  cli/ and server/ never import each other (the service's tests use the CLI's client);
  runtime/ imports neither;
  common/ imports none of the three.

the page (frontend/web/src):
  app/ ──► the domain folders ──► content/, ui/, lib/
  only api/ talks to the service (a stored file's own address aside, read where it is shown)
```

The core names no runtime: what is Herdr's own (its words, its socket, its places) stays
in `runtime/herdr/`, and a test (`crates/workriver/tests/boundaries.rs`) fails when its
name turns up anywhere else in the sources but the tests that stand in for it. So another
runtime can be added beside it without touching the core, the service's domains or the
page.
