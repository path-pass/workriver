# workriver

[中文](docs/README.zh-CN.md)

Keep track of many coding agents working in parallel.

When twenty agent sessions are running, nobody can watch them all. workriver records the
work as it happens and shows it as one picture:

- **Intents and units** – an intent is a goal; it is split into units that can be built
  on their own.
- **Activity** – inside a unit, every task handed to an agent (or to you, as a manual
  step) is a node with an input and an output. Tasks connect into a flow that branches
  when a review raises findings and joins again.
- **Code** – the files the unit touched, as a tree next to the diff.
- **Needs you** – one queue across everything: manual steps an agent is waiting on,
  findings to accept, reject or defer, and gaps such as commits that name no task.

It runs on your machine as one service. Agents report their work by running the
`workriver` CLI from their shell; who is running and where comes from
[Herdr](https://herdr.dev); you look through a browser.

## Status

Early. The service records intents, units, tasks, findings and chores, reads the units'
commits from git, follows the agents running in Herdr (one intent per Herdr workspace, one
manager agent per intent), delivers your decisions and gate results to the right agent, and
serves the web UI on real data.

## Build and run

Runtimes come from [mise](https://mise.jdx.dev) (`mise.toml`: Rust and Node). The web UI is
built into the binary, so build it first:

```
cd frontend/web && npm install && npm run build && cd ../..
cargo build --release
./target/release/workriver serve          # http://127.0.0.1:47820
```

Data lives in `~/workriver-data/workriver.db` (set `WORKRIVER_HOME` to move it).
`workriver --help` explains the commands; a commit belongs to a task through a trailer line
`Task: u1#3` in its message. Inside a Herdr pane the CLI knows which agent runs it; the
manager of a workspace registers with `workriver manager here`, and `workriver guide
manager` / `workriver guide worker` print the instructions for agents. Herdr's socket is
`~/.config/herdr/herdr.sock` (or `HERDR_SOCKET_PATH`); without Herdr the service still
records and shows, it just sees no agents.

Working on the UI: `npm run dev` in `frontend/web` uses a running `workriver serve`;
`npm run dev:mock` uses the sample data instead.

## License

[MIT](LICENSE)
