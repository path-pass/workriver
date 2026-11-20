# Getting started

[中文](getting-started.zh-CN.md)

This takes a Mac with [mise](https://mise.jdx.dev), git and the agent runtime
([Herdr](https://herdr.dev); what it needs: [runtimes/herdr.md](runtimes/herdr.md)) to a
running workriver with a manager agent in each workspace.

## 1. Build

```sh
git clone https://github.com/path-pass/workriver.git
cd workriver
mise install                 # Rust, Node and pnpm as pinned in mise.toml, for this folder only
cd frontend/web && pnpm install && pnpm build && cd ../..
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

Start it in a terminal of its own in the runtime, so it finds the runtime and keeps running
([runtimes/herdr.md](runtimes/herdr.md#installing-and-running) says where Herdr is found):

```sh
workriver serve              # http://127.0.0.1:47820
```

Then, from a terminal that is not an agent's, `workriver open`: it opens the page in
your browser with a one-time link that signs this browser in as you. The link is never
printed, since agents can read the runtime's terminals; on a machine without a browser,
`workriver open --print` prints one that lasts a minute: use it in a terminal no agent reads. The plain address shows only how to open it; see
[Security model](security.md) for why. `workriver status` should say
`agent runtime: connected`.

- Data: `~/workriver-data/workriver.db` (set `WORKRIVER_HOME` to put it elsewhere). One
  SQLite file; copy it while the service is stopped to back it up. Attachments
  (screenshots, reports) live next to it in `attachments/`, one file per content, and are
  never deleted, like the records; `workriver status` shows how much they take.
- The page's terminal button brings the agent's terminal to the front in the runtime, then
  the terminal app (Ghostty by default; `workriver serve --terminal-app iTerm`, or `''` for none).

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
reads the file, works out which items are goals and which are leftover findings (the steps
of one goal become one intent, with them as a checklist in its note), shows you a
preview table and waits for your yes, then adds them in the file's order with
`workriver backlog add` and `workriver backlog add-finding`. It never changes the file.
`workriver backlog export --project P` prints the backlog back as Markdown.
Split too finely? "Merge…" in the roadmap's heading opens a dialog: tick the intents, pick the
one that stays and, if you like, give it a new title. Or `workriver backlog merge i3 i4 i5
--title "Search"`: i4 and i5 are dropped and what they said goes into i3's note, shown under
its goal. An agent may merge intents that agents added (the import agent tidying its own
import); intents you added only you merge.

## 4. Give each workspace a manager

A workspace of the runtime runs one or more intents; its manager agent plans and hands out
their work. In each workspace, start a coding agent as its manager and give it the skills.

Install **stubs**, once: small skill files that send the agent to `workriver guide …`, which
always matches the installed `workriver`. A copy of the real skill files goes stale with
every update, and a manager following an old copy does not know the commands that exist.

```sh
for s in manager howto import; do
  mkdir -p ~/.claude/skills/workriver-$s          # Codex: ~/.agents/skills/workriver-$s
  workriver guide install-stub $s > ~/.claude/skills/workriver-$s/SKILL.md
done
```

(Per project: `<project>/.claude/skills/` or `<project>/.agents/skills/`. On a machine that
has this repository, linking works too and stays current with `git pull`:
`ln -s "$PWD/skills/workriver-manager" ~/.claude/skills/workriver-manager`, and the same for
`workriver-howto` and `workriver-backlog-import`.) Any other harness: have the agent run
`workriver guide manager` first.

The skills are three: the manager's role (`workriver guide manager`, short), recipes for
each situation (`workriver guide how`, an index, and `workriver guide how <topic>`), and
importing a backlog (`workriver guide import`). After an update, a manager that was already
running is told once to read `workriver guide changes`.

Then tell it, for example: "You are the workriver manager of this workspace. Start the
first intent of the roadmap, plan units and tasks, and hand them to worker agents." On the
page, "Start" next to a planned intent shows the same steps with the commands to copy.

Each workspace works on one project. workriver works out which from the folder its manager
runs in (else from its current intent); you can set it in Settings, "Workspaces' projects",
or with `workriver workspace project <workspace> <p1>` (`--clear` to let workriver work it
out again). "Start" lists the workspaces of the intent's project first and asks before
giving an intent to a manager that works on another project.

Optional, if agents take screenshots with the Playwright MCP: by default its files land in
the project (`.playwright-mcp/`). Give it workriver's folder for files it names itself,
e.g. for Claude Code
`claude mcp add playwright -- npx @playwright/mcp@latest --output-dir ~/workriver-data/attachments/inbox/playwright`
(or set `PLAYWRIGHT_MCP_OUTPUT_DIR`). This covers screenshots taken without a file name;
a file name an agent gives is still taken relative to the project. Agents then attach what
they need with `workriver attach <file> --task u1#3 --move`.

## 5. What happens next

You start the workspace's first intent: "Start" on i1 "Mentions" in the roadmap, picking the
workspace (or set the workspace's project in Settings and let the manager start it). From
then on the workspace works on that project, and its manager starts that project's next
intents itself (`workriver intent start i2`); another project's it leaves to you.

The manager, from its terminal:

```sh
workriver manager here                 # registers it as the workspace's manager
workriver backlog list                 # the roadmap: i1 "Mentions" (active, started by you) ...
workriver unit add --title "Mentions" --branch feat/mention   # its own branch: feat/, fix/ or chore/
workriver task add u1 --title "Notify the people mentioned, with unit tests" --agent finder
```

Then it starts the worker `finder` in the runtime and gives it the task: its id, its goal
in a line, where to work, "run workriver task show u1#1", and the text of `workriver guide
worker` (the commands under Herdr: [runtimes/herdr.md](runtimes/herdr.md#workspaces-and-managers)).

The worker records its own work:

```sh
workriver task show u1#1                          # the task in full: what it asks, its causes, notes
workriver task start u1#1
git commit -m "Find mentions" -m "Task: u1#1"     # the trailer links the commit to the task
workriver task deliver u1#1 --evidence 'npm test => passed: 6 passed'
```

The manager splits little: an intent is one unit unless work must land separately (another
repository, another machine, a merge of its own), and a task is one complete outcome for
one agent, not one per step.
A commit on the unit's branch without a `Task:` line shows as unregistered, a gap to close.
Each unit says where it stands: working, waiting (an open finding, a gate), ready to merge,
merged (workriver sees a fast-forward or merge commit itself; after a squash merge run
`workriver unit merged u1`) or done (`workriver unit done u1 --reason "..."`, for work
finished without a merge). When every unit of an intent is merged or done, its manager
closes it with `workriver intent done i1`.
A task that is no longer needed (a gate asked again after the branch changed) is dropped,
never deleted: "No longer needed…" in its details on the page, or `workriver task drop u1#15
--reason "superseded by u1#17"`. It turns grey, leaves Needs you, holds nothing up, and its
agent and the manager are told.
Reviews and tests are tasks of their own. Their findings are decided by the intent's
manager by default (shown with a robot mark; you can change any decision on the page, or
keep the deciding to yourself per intent: "Findings: Manual" in the rail, or `workriver
intent set i1 --findings user`). Findings that are yours wait in **Needs you**: accept,
revise, reject or defer each one there. Deferred ones wait in the **Backlog**
panel; tick several and "Do together" sends them to the intent's manager as one message,
which turns them into one cleanup unit. Your decision and your gate results go to the right
agent through the runtime (an accepted finding to the manager, which hands out the work);
the line under it shows whether it was delivered and picked up, and "Copy message" gives you
the same text to paste by hand.

## 6. Other machines

One workriver can follow agents on other machines too. Each machine runs its own runtime;
workriver keeps one ssh connection to each, which brings that machine's runtime here
and gives the `workriver` CLI over there a way back to this service.

Requirements: the runtime running on the other machine (for Herdr, which version:
[runtimes/herdr.md](runtimes/herdr.md#other-machines)); `ssh -o BatchMode=yes <target>
true` works from here without a prompt (a key in ssh-agent, or your own ControlMaster in
`~/.ssh/config`); the other machine's sshd allows forwarding (the default); `git` there.

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

The port over there that leads to this service is a new one for every connection: ssh
picks a free one, and workriver writes it there to `~/.config/workriver/port`, which the
`workriver` command there reads (unless `WORKRIVER_PORT` is set). Nothing to configure. If
the runtime over there is not where workriver looks for it, `machine add` takes where it is
([runtimes/herdr.md](runtimes/herdr.md#other-machines)).

Agents over there show up with the machine's name. Ids are never assumed to be the same
across machines, and a workspace there is its own, even with the same name as one here.
To make one workspace of the two (one manager, the same intents), link them:

```sh
workriver workspace list                      # box:chat  not linked
workriver workspace link box:chat chat
```

or pick it in Settings. A unit is recorded where its work happens: an agent over there
records it there as usual; a manager on this machine names the machine, with the path and
branch as they are there (`workriver unit add --machine box --repo /home/me/app --branch
feat/queue`), and a unit recorded in the wrong place is moved with `workriver unit move u1
--machine box --repo … --branch …`. The same command follows a branch that was renamed, or
rebased onto a new branch (`workriver unit move u1 --branch feat/queue-2 [--base main-2]`): the
unit, its tasks and findings stay, its commits are read from the new branch, the page says
"previously on feat/queue", and a task whose commits stayed on the old branch says so. A commit
made without its `Task:` trailer is linked to its task afterwards, by its agent, the
manager or you: `workriver commit link <hash> u1#3`, or the "It belongs to task" select on
the unregistered node; the link follows the commit through a rebase. For a unit over there,
workriver reads its commits with git over ssh, every 15 seconds. Screenshots attached
there are uploaded through the connection; messages to agents there link to them
(`http://127.0.0.1:$(cat ~/.config/workriver/port)/api/attachments/…`). The terminal button focuses the agent's terminal in that
machine's runtime ([runtimes/herdr.md](runtimes/herdr.md#other-machines) says what is left
to you).

## 7. Update

```sh
git pull
cd frontend/web && pnpm install && pnpm build && cd ../..
cargo build --release
```

Then stop `workriver serve` (Ctrl-C) and start it again. The data is kept.

Do the same build on **every other machine** you follow (`git pull`, `cargo build
--release` there): the `workriver` command agents run there must match the service.
`workriver status`, here and on each of them, shows the version and the commit of the
service and says so when the command it was run with is another build.

Cargo never removes old builds from `target/`: after some weeks of updates `target/debug`
can pass 20 GB. `cargo clean --profile dev` removes `target/debug` only; the
`target/release/workriver` the service runs from stays. Each git worktree has its own
`target/` (about 1.5 GB after a build and its tests), removed with the worktree. Worktrees
do not share one build folder on purpose: cargo names this repository's own crates by their
path inside the checkout and decides what to rebuild by file times, so a worktree whose
files are older than another's last build would run that other worktree's code.

## Troubleshooting

- **"cannot listen on 127.0.0.1:47820"**: another workriver (or something else) uses the
  port. Stop it, or use `workriver serve --port 47821` and give the CLI the same port
  (`--port 47821` or `WORKRIVER_PORT=47821`).
- **The page says it cannot reach the service**: `workriver serve` is not running, or runs
  on another port.
- **`agent runtime: not connected`**, or a warning about the runtime at start (`workriver
  status` shows it): the runtime is not running, or the service looks for it elsewhere; see
  [runtimes/herdr.md](runtimes/herdr.md#troubleshooting).
- **A machine shows "not connected"**: `workriver machine list` prints ssh's own words.
  *Permission denied* or *Host key verification failed*: make `ssh -o BatchMode=yes
  <target> true` work first. *remote port forwarding failed*: a forward in your ssh config
  for that host fails workriver's connection; give workriver a Host alias without it.
  *connected, but its … does not answer*: the runtime is not running there, or is
  elsewhere ([runtimes/herdr.md](runtimes/herdr.md#troubleshooting)).
- **Something is slow**: the service keeps a timing log in `~/workriver-data/logs/`
  (`perf.jsonl`, five files of 5 MB at most). `workriver perf [--since 1h]` sums it up:
  the slowest requests, how long snapshots and each unit took, how long other machines
  were disconnected. For each git and ssh call too, start the service with `workriver
  serve --perf-detail` (or `WORKRIVER_PERF=detail`). `workriver perf --bundle` writes the
  summary and the newest lines into one file and prints its path: that is the file to
  send. It holds times, counts, route templates (`/api/task`, never the query), unit ids
  and machine names; never commit messages, file texts, task texts, findings, notes,
  prompts, tokens, cookies or paths (git calls show `…` where a path was). Look at it
  before you send it.
- **A machine goes "disconnected" after this computer slept or changed networks**: that
  is expected and mends itself. ssh notices a dead line within about 30 seconds (and at
  once after a sleep), workriver connects again, first after 2 seconds and then waiting
  longer, at most a minute, and at once when the computer wakes. Meanwhile nothing waits
  on that machine: its units and agents show as last seen, marked "disconnected since
  14:02", and `workriver machine list` gives the last error and the next attempt. Each
  connection gets a new port over there, so the sessions of dead connections that the
  machine's sshd still keeps (it does not check that its clients are alive, and may keep
  them for half an hour) are in nobody's way. The `workriver` command over there gives up
  after a few seconds with "cannot reach workriver … through the ssh tunnel (it may be
  reconnecting); try again shortly", rather than hanging.
  If you run that machine's sshd, it can drop such sessions sooner: in
  `/etc/ssh/sshd_config`, `ClientAliveInterval 15` and `ClientAliveCountMax 3`. Not
  needed. To see them anyway, there: `ss -tnp | grep sshd` (a session per connection,
  some with data waiting to be sent to a peer that is gone); they end on their own, or
  with `kill` on that sshd process.
- **"cannot tell which agent this is"** from the CLI in an agent's terminal: the service
  cannot reach the runtime (see above). To run a command as yourself meanwhile, use a
  terminal outside the runtime.
- **"workriver answers the user, and agents from their own …" (401)**: the command or the page is not known to be you.
  The page: `workriver open`. The CLI: run it from a terminal that is not an agent's,
  on the machine workriver runs on. Decisions, gates, chores, projects, machines and
  workspace links are yours alone; see [Security model](security.md).
- **"Not delivered: …"** on the page (hover for the details):
  - *the intent has no manager*: run `workriver manager here` in the manager's terminal in
    the workspace the intent was started in;
  - *the manager is not running*: start it again and register it;
  - *… is not reachable* or *… refused it* (the page names the runtime): see
    [runtimes/herdr.md](runtimes/herdr.md#troubleshooting); sort it out in the terminal, then
    paste the message by hand with "Copy message".
