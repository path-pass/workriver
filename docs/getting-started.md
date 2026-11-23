# Getting started

[中文](getting-started.zh-CN.md)

This takes a Mac with [mise](https://mise.jdx.dev), git and the agent runtime
([Herdr](https://herdr.dev); what it needs: [runtimes/herdr.md](runtimes/herdr.md)) to a
running workriver with a manager agent seated in each workspace.

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
on the page, "Add project" (at the foot of the left rail's projects), with the folder's
whole path (from `/` or `~/`), or `workriver project add ~/code/shop` (its repositories are
its direct subfolders with a `.git`); a single repository is registered the same way. A
project is a folder, never a file, and is not inside another project nor holds one. Only you register projects, and a
repository is part of a registered project or of none: an agent that adds an intent or a
deferred finding names a project you registered (`p1`, or a folder inside it) and is
refused otherwise, with the command for you to run.

Each project gets a **workspace** when you add it, named as the project: where its intents
run, with one manager and one conversation with you. A workspace is workriver's own
(`ws1`, shown beside its name), not the runtime's: where a terminal sits in the runtime
decides nothing. One workspace per project is the usual; add another for a big intent or a
separate conversation (the "+" on the project in the left rail, or `workriver workspace
add p1 --name "Checkout"`), and close one you are done with (its intents and conversation
stay).

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

A workspace runs one or more intents of its project; its manager agent plans and hands out
their work. Give it the skills (below), then start one from the page, or start a coding agent
in the runtime yourself and seat it.

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

**Start one from the page**: the workspace's line in the left rail, "Start manager" (or
"Manager", then "Start a new one"). Pick the harness, Claude Code or Codex (offered when its
command is on the machine and the runtime's integration for it is current; otherwise shown
greyed with why), and the model: your default, an alias the harness knows (`opus`,
`sonnet`, `fable` for Claude Code), or a model's full name. workriver opens a terminal in the
project's folder (in the runtime's group named as the workspace), starts the harness there
with only that model argument, and seats it once it is ready with a session; it is told it manages the
workspace, as below. It runs with your own configuration of the harness, as when you type
its name in that folder. In a folder the harness has never seen it first asks whether you
trust it: the workspace's line says it waits for you, and you answer in its terminal or from
Needs you. If the harness reports a problem as it starts (a model it does not know), it is
not seated: the line shows the harness's own words and its terminal stays. If nothing gets a
session within ten minutes, the line says why and the terminal stays; seat it by hand once it
runs. Settings → Starting managers sets the default harness and model.

Or **seat one yourself**: on the page, the workspace's line in the left rail, "Manager", and pick the
agent among those running (the page shows each one's harness, machine and folder); or from
your own terminal

```sh
workriver workspace list                 # ws1  shop  project shop (p1)  no manager
workriver workspace manager ws1 boss     # the agent's name, or its session
```

workriver types into that agent's terminal that it is now the manager of the workspace,
with where its role is written. Only you seat a manager: an agent cannot make itself one,
and the seat does not follow a name or a place in the runtime. Seat someone else at any time (the one replaced is told), or empty the seat
(`--none`).

When a manager's terminal closes, its workspace shows "manager not running". The same
session resumed in the same folder on the same machine is the manager again by itself (the
conversation says so). A new session, or the old one in another folder, is not: Needs you
lists the workspace with that agent proposed, and one click seats it.

"Start" next to a planned intent lists the open workspaces of its project, each with its
manager; an intent runs only in a workspace of its project.

Optional, if agents take screenshots with the Playwright MCP: by default its files land in
the project (`.playwright-mcp/`). Give it workriver's folder for files it names itself,
e.g. for Claude Code
`claude mcp add playwright -- npx @playwright/mcp@latest --output-dir ~/workriver-data/attachments/inbox/playwright`
(or set `PLAYWRIGHT_MCP_OUTPUT_DIR`). This covers screenshots taken without a file name;
a file name an agent gives is still taken relative to the project. Agents then attach what
they need with `workriver attach <file> --task u1#3 --move`.

## 5. What happens next

Start the first intent: "Start" on i1 "Mentions" in the roadmap, picking the workspace; its
manager is told to split it. The manager starts its project's next intents itself, in the
workspace it is seated in (`workriver intent start i2`); another project's it cannot.

The manager, from its terminal:

```sh
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
workriver task start u1#1 --model claude-opus-5-5  # the model it works with, if it knows
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
A requirement that comes up after a task was handed out is added to it, not typed into its
agent's terminal: `workriver task note u1#3 --text "Also cover the Codex adapter"`, by the
intent's manager or you, while the task is not delivered. It shows under what the task asks,
with who and when, on the page and in `workriver task show`, and the agent on the started
task is told.
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

Agents over there show up with the machine's name. They work in the same workspaces as
agents here: a workspace has no machine. A worker over there is in a workspace through the
tasks it is given, and you can seat an agent over there as a workspace's manager like any
other.

A unit is recorded where its work happens: an agent over there
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
there are uploaded through the connection; messages to agents there give the command that
fetches each one into the folder the agent is in (`workriver attach fetch <id>.<ext>`; a
`curl` to the service is refused). The terminal button focuses the agent's terminal in that
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

A store written before workspaces were workriver's own is refused at start ("holds events
in the form from before virtual workspaces"; nothing in it is changed). Convert a copy of
it once with the script the repository no longer carries: commit `7cdf576` holds it, as
`scripts/migrate-virtual-workspaces/`, and its `README.md` says how to run it. On any
machine with a clone, take that commit into a worktree of its own (`git worktree add
--detach .local/migrate-script 7cdf576`) and follow the README there.

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
  on the machine workriver runs on. Decisions, gates, chores, projects, machines,
  workspaces and their managers' seats are yours alone; see [Security model](security.md).
- **"Not delivered: …"** on the page (hover for the details):
  - *the intent has no manager*: seat one in its workspace (the workspace's line, "Manager",
    or `workriver workspace manager <ws> <agent>`);
  - *the manager is not running*: resume it where it ran (it is the manager again by
    itself), or seat another agent;
  - *the seat was given anew*: the message was for the earlier manager and is never passed
    on; write to the manager again;
  - *… is not reachable* or *… refused it* (the page names the runtime): see
    [runtimes/herdr.md](runtimes/herdr.md#troubleshooting); sort it out in the terminal, then
    paste the message by hand with "Copy message".
