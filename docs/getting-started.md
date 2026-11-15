# Getting started

[中文](getting-started.zh-CN.md)

This takes a Mac with [mise](https://mise.jdx.dev), git and [Herdr](https://herdr.dev)
(0.9, protocol 22) to a running workriver with a manager agent in each Herdr workspace.

## 1. Build

```sh
git clone https://github.com/path-pass/workriver.git
cd workriver
mise install                 # Rust and Node as pinned in mise.toml, for this folder only
cd frontend/web && npm install && npm run build && cd ../..
cargo build --release
```

If mise is not activated in your shell, prefix the commands with `mise exec --`. The web UI
is built into the binary, which is why it is built first.

The binary is `target/release/workriver`. Nothing is installed for you; pick one:

- add the folder to your PATH in your shell's startup file:
  `export PATH="$HOME/<path to>/workriver/target/release:$PATH"`;
- or link it into a folder already on your PATH, e.g.
  `ln -s "$PWD/target/release/workriver" ~/.local/bin/workriver`;
- or `cargo install --path crates/workriver` (copies it to `~/.cargo/bin`; repeat after each update).

Agents call `workriver` from their shells, so it must be on the PATH they see.

## 2. Run the service

Start it in a Herdr pane of its own, so it finds Herdr's socket and keeps running:

```sh
workriver serve              # http://127.0.0.1:47820
```

Open <http://127.0.0.1:47820>. `workriver status` should say `agent runtime: connected`.

- Data: `~/workriver-data/workriver.db` (set `WORKRIVER_HOME` to put it elsewhere). One
  SQLite file; copy it while the service is stopped to back it up.
- The page's terminal button brings the agent's pane to the front in Herdr, then the
  terminal app (Ghostty by default; `workriver serve --terminal-app iTerm`, or `''` for none).

## 3. Give each workspace a manager

One Herdr workspace is one intent (a goal). In each, start a coding agent as its manager
and give it the manager skill from `skills/workriver-manager/`:

- **Claude Code**: link the folder into your skills, for one project or for all:
  `ln -s "$PWD/skills/workriver-manager" <project>/.claude/skills/workriver-manager`, or
  `ln -s "$PWD/skills/workriver-manager" ~/.claude/skills/workriver-manager`.
- **Codex**: the same into `<project>/.agents/skills/` or `~/.agents/skills/`.
- **Any other harness**: run `workriver guide manager` and paste the text into its first
  message.

Then tell it, for example: "You are the workriver manager of this workspace. Goal: let
people mention each other in chat. Plan units and tasks, and hand them to worker agents."

## 4. What happens next

The manager, from its pane:

```sh
workriver manager here                 # registers it; the intent is named after the workspace
workriver unit add --title "Mentions" --branch mention
workriver task add u1 --title "Find the people mentioned in a message" --agent finder
herdr agent start finder --kind claude --pane <pane id>   # see `herdr --skill`
herdr agent prompt finder "<task u1#1, its goal, and the text of: workriver guide worker>"
```

The worker records its own work:

```sh
workriver task start u1#1
git commit -m "Find mentions" -m "Task: u1#1"     # the trailer links the commit to the task
workriver task deliver u1#1 --evidence 'npm test => passed: 6 passed'
```

A commit on the unit's branch without a `Task:` line shows as unregistered, a gap to close.
Reviews and tests are tasks of their own; their findings wait in **Needs you** on the page.
Accept, reject or defer each one there. Your decision and your gate results go to the right
agent through Herdr (an accepted finding to the manager, which hands out the work); the
line under it shows whether it was delivered and picked up, and "Copy message" gives you
the same text to paste by hand.

## 5. Update

```sh
git pull
cd frontend/web && npm install && npm run build && cd ../..
cargo build --release
```

Then stop `workriver serve` (Ctrl-C) and start it again. The data is kept.

## Troubleshooting

- **"cannot listen on 127.0.0.1:47820"**: another workriver (or something else) uses the
  port. Stop it, or use `workriver serve --port 47821` and give the CLI the same port
  (`--port 47821` or `WORKRIVER_PORT=47821`).
- **The page says it cannot reach the service**: `workriver serve` is not running, or runs
  on another port.
- **`agent runtime: not connected`**, or a warning about Herdr at start (`workriver
  status` shows it): Herdr is not running, or the service looks at another socket. Start
  the service in a Herdr pane, or set `HERDR_SOCKET_PATH`. A protocol warning means a Herdr
  other than 0.9; workriver may still work, but it is untested.
- **"cannot tell which agent this is"** from the CLI in a pane: the service cannot reach
  Herdr (see above). `--as <name>` records under a name you give.
- **"Not delivered: …"** on the page (hover for the details):
  - *the intent has no manager*: run `workriver manager here` in the manager's pane;
  - *the manager is not running*: start it again and register it;
  - *Herdr is not reachable*: see above;
  - *Herdr refused it*: usually `agent_not_ready` (the agent is still starting, or is not
    the pane's foreground program) or `agent_blocked` (it waits at a question or approval).
    Sort it out in the terminal, then paste the message by hand with "Copy message".
