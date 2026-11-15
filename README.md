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

It runs on your machine as one service. Agents report their work through MCP; who is
running and where comes from [Herdr](https://herdr.dev); you look through a browser.

## Status

Early. Only the UI exists, running on sample data.

```
cd frontend/web
npm install
npm run dev
```

## License

[MIT](LICENSE)
