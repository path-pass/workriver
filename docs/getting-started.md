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

Then, from a terminal that is not an agent's pane, `workriver open`: it opens the page in
your browser with a one-time link that signs this browser in as you (the same link is in
the service's first log line). The plain address shows only how to open it; see
[Security model](security.md) for why. `workriver status` should say
`agent runtime: connected`.

- Data: `~/workriver-data/workriver.db` (set `WORKRIVER_HOME` to put it elsewhere). One
  SQLite file; copy it while the service is stopped to back it up. Attachments
  (screenshots, reports) live next to it in `attachments/`, one file per content, and are
  never deleted, like the records; `workriver status` shows how much they take.
- The page's terminal button brings the agent's pane to the front in Herdr, then the
  terminal app (Ghostty by default; `workriver serve --terminal-app iTerm`, or `''` for none).

## 3. Projects and the backlog

A **project** is a folder with one or more repositories. Register one that holds several:
`workriver project add ~/code/shop` (its repositories are its direct subfolders with a
`.git`). A repository outside every registered folder is a project of its own; nothing to do.

Each project has a **backlog**: its **roadmap** (planned intents, goals in the order you
want them) and its **deferred findings** (problems put off for later). Add to the roadmap
in the rail of the page ("+" under Roadmap; drag to reorder), or:

```sh
workriver backlog add "Offline sync" --goal "Notes can be taken offline and merge later." --project ~/code/shop
workriver backlog list
```

**Importing an existing list.** Notes files differ every time, so there is no importer
that parses them; an agent does it with the import skill (`skills/workriver-backlog-import/`,
or paste what `workriver guide import` prints). Tell it which file and which project; it
reads the file, works out which items are goals and which are leftover findings, shows you a
preview table and waits for your yes, then adds them in the file's order with
`workriver backlog add` and `workriver backlog add-finding`. It never changes the file.
`workriver backlog export --project P` prints the backlog back as Markdown.

## 4. Give each workspace a manager

A Herdr workspace runs one or more intents; its manager agent plans and hands out their
work. In each workspace, start a coding agent as its manager and give it the manager skill
from `skills/workriver-manager/`:

- **Claude Code**: link the folder into your skills, for one project or for all:
  `ln -s "$PWD/skills/workriver-manager" <project>/.claude/skills/workriver-manager`, or
  `ln -s "$PWD/skills/workriver-manager" ~/.claude/skills/workriver-manager`.
- **Codex**: the same into `<project>/.agents/skills/` or `~/.agents/skills/`.
- **Any other harness**: run `workriver guide manager` and paste the text into its first
  message.

Then tell it, for example: "You are the workriver manager of this workspace. Start the
first intent of the roadmap, plan units and tasks, and hand them to worker agents." On the
page, "Start" next to a planned intent shows the same steps with the commands to copy.

Optional, if agents take screenshots with the Playwright MCP: by default its files land in
the project (`.playwright-mcp/`). Give it workriver's folder for files it names itself,
e.g. for Claude Code
`claude mcp add playwright -- npx @playwright/mcp@latest --output-dir ~/workriver-data/attachments/inbox/playwright`
(or set `PLAYWRIGHT_MCP_OUTPUT_DIR`). This covers screenshots taken without a file name;
a file name an agent gives is still taken relative to the project. Agents then attach what
they need with `workriver attach <file> --task u1#3 --move`.

## 5. What happens next

The manager, from its pane:

```sh
workriver manager here                 # registers it as the workspace's manager
workriver backlog list                 # the roadmap: i1 "Mentions" ...
workriver intent start i1              # active here; the workspace's current intent
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
Accept, revise, reject or defer each one there. Deferred ones wait in the **Backlog**
panel; tick several and "Do together" sends them to the intent's manager as one message,
which turns them into one cleanup unit. Your decision and your gate results go to the right
agent through Herdr (an accepted finding to the manager, which hands out the work); the
line under it shows whether it was delivered and picked up, and "Copy message" gives you
the same text to paste by hand.

## 6. Other machines

One workriver can follow agents on other machines too. Each machine runs its own Herdr;
workriver keeps one ssh connection to each, which brings that machine's Herdr socket here
and gives the `workriver` CLI over there a way back to this service.

Requirements: Herdr 0.9.1 or later on both sides (same socket protocol), running on the
other machine; `ssh -o BatchMode=yes <target> true` works from here without a prompt (a
key in ssh-agent, or your own ControlMaster in `~/.ssh/config`); the other machine's sshd
allows forwarding (the default); `git` there.

```sh
workriver machine add box --ssh me@box        # a name of yours, and what ssh takes
workriver machine enable box
workriver machine list                        # "connected, 3 agents", or why not
```

The same is in Settings on the page. Then, on the other machine, build the CLI and check
that it reaches this service through the connection:

```sh
git clone <this repository> && cd workriver && cargo build --release
ln -s "$PWD/target/release/workriver" ~/.local/bin/workriver    # anywhere on PATH
workriver status                              # answers from the service on your machine
```

If port 47820 is taken over there, add the machine with `--remote-port 47821` and put
`export WORKRIVER_PORT=47821` into the shell profile there. If Herdr's socket is not at
`~/.config/herdr/herdr.sock` there (a named session), give it: `--herdr-socket <path>`.

Agents over there show up with the machine's name. Ids are never assumed to be the same
across machines, and a workspace there is its own, even with the same name as one here.
To make one workspace of the two (one manager, the same intents), link them:

```sh
workriver workspace list                      # box:chat  not linked
workriver workspace link box:chat chat
```

or pick it in Settings. A unit recorded by an agent over there has its repository there;
workriver reads its commits with git over ssh, every 15 seconds. Screenshots attached
there are uploaded through the connection; messages to agents there link to them
(`http://127.0.0.1:47820/api/attachments/…`). The terminal button focuses the pane in that
machine's Herdr; switch to the machine in Herdr's sidebar yourself.

## 7. Update

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
- **A machine shows "not connected"**: `workriver machine list` prints ssh's own words.
  *Permission denied* or *Host key verification failed*: make `ssh -o BatchMode=yes
  <target> true` work first. *remote port forwarding failed*: the port is taken there, see
  above. *connected, but its Herdr does not answer*: Herdr is not running there, or its
  socket is elsewhere (`--herdr-socket`).
- **"cannot tell which agent this is"** from the CLI in a pane: the service cannot reach
  Herdr (see above). To run a command as yourself meanwhile, use a terminal outside Herdr.
- **"this is the user's to do…" (401)**: the command or the page is not known to be you.
  The page: `workriver open`. The CLI: run it from a terminal that is not an agent's pane,
  on the machine workriver runs on. Decisions, gates, chores, projects, machines and
  workspace links are yours alone; see [Security model](security.md).
- **"Not delivered: …"** on the page (hover for the details):
  - *the intent has no manager*: run `workriver manager here` in the manager's pane of
    the workspace the intent was started in;
  - *the manager is not running*: start it again and register it;
  - *Herdr is not reachable*: see above;
  - *Herdr refused it*: usually `agent_not_ready` (the agent is still starting, or is not
    the pane's foreground program) or `agent_blocked` (it waits at a question or approval).
    Sort it out in the terminal, then paste the message by hand with "Copy message".
