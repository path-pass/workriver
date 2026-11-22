# Herdr

[中文](herdr.zh-CN.md)

workriver learns who is running, where and in what state, and carries messages to agents,
through an agent runtime. Today that runtime is [Herdr](https://herdr.dev), the only one
workriver supports. This page holds everything in the other documents that is Herdr's
own: where the [README](../../README.md), [Getting started](../getting-started.md) and the
[Security model](../security.md) say "the runtime", read Herdr; where they say an agent's
terminal, read the Herdr pane it runs in. A workspace is not Herdr's: workriver's
workspaces are its own records (`ws1`), and a Herdr workspace is only where a pane sits.

## Installing and running

workriver needs Herdr 0.9 (protocol 22) on the machine it runs on.

Start the service in a Herdr pane of its own, so it finds Herdr's socket and keeps running:

```sh
workriver serve              # http://127.0.0.1:47820
```

Herdr's socket is `~/.config/herdr/herdr.sock` (or `HERDR_SOCKET_PATH`); without Herdr the
service still records and shows, it just sees no agents. `workriver status` should say
`agent runtime: connected`.

The page's terminal button opens the agent's terminal in the page, read-only; "Open in
terminal app" in that view brings the agent's pane to the front in Herdr, then the
terminal app.

## Answering a stuck agent from the page

What workriver uses of Herdr for this, all through the same socket:

- **Noticing.** The service still reads every agent's state every 2 s. One long
  `events.subscribe` connection (to `pane.agent_detected`, `pane.closed`, and
  `pane.agent_status_changed` for each agent pane, which Herdr takes only per pane) makes
  that read happen at once when a state changes; a pane detected or closed opens the
  connection again with the panes there are then.
- **Dialogs Herdr misses.** Herdr 0.9.5 reports Codex's "Hooks need review" as idle and
  "Implement this plan?" as done. The service reads the pane's screen when an agent becomes
  idle or done (and again at the first poll half a tick or more later, however many polls a
  change or a read woke between), and before typing any message into a pane: options with
  a key named under them make the agent waiting on its screen, counted and held like a
  blocked one. Another machine's panes are read beside the poll, one read per agent at a
  time, so a slow machine never holds this one's states back; what such a read finds shows
  when it ends, and only a read that changed the mark wakes the poll.
- **The gate.** `agent.list` (through the session snapshot) turns a session into its one
  pane; `pane.read` (source `visible`, plain text) is the screen; `agent.explain` gives the
  reason Herdr calls it blocked, of which workriver keeps the id of the rule that matched and
  that rule's own fixed phrases (`bash_permission_prompt: …`), Herdr's
  words, not the screen's: the evidence preview Herdr reads off the screen is left out, since
  the gate shows the screen itself; `pane.send_keys` sends the keys, in
  Herdr's names (`up`, `down`, `enter`, `esc`, `tab`, digits); a message goes to that pane
  (a seated manager's to the pane it is held to, never resolved again) with `agent.prompt`,
  once a fresh `agent.list` still shows the session there.
- **The terminal view.** Herdr streams a pane's terminal on its client socket,
  `herdr-client.sock` next to `herdr.sock` (for a socket `X.sock`: `X-client.sock`), in a
  protocol of its own; workriver runs Herdr's own CLI for it, `herdr terminal session
  observe <pane>`, one per open view, with `HERDR_SOCKET_PATH` set to the socket it uses and
  the pane variables of its own terminal cleared. So the `herdr` on the service's `PATH` must
  be the same version as the Herdr server. A view is read-only and never resizes the pane.

## Workspaces and managers

A workspace is workriver's own record, made by you in a project; a Herdr workspace is only
a group of panes. Nothing of Herdr's workspaces identifies one of workriver's: Herdr gives
a closed workspace's id again after a restart, two may share a label, and any program
renames, creates and closes them through the socket. So arrange your panes as you like
(one Herdr workspace per workriver workspace reads well; nothing requires it): workriver
shows the Herdr workspace beside an agent as where its pane is, and opens a temporary
agent's pane beside the one it serves.

You give a workspace its manager yourself, one of two ways. Start one from the page (the
workspace's line, "Start manager"): workriver opens a new tab, without taking the focus, in
the Herdr workspace labelled as the workriver workspace (the first with that label, else one
made now), in the project's folder; reads its shell (`pane.process_info`); records the
start; then runs Herdr's `agent.start` there with the harness's kind and its arguments as
Herdr's list (only `--model <model>` when you picked one), under a name of that start's own,
`<ws>-manager-<n>` (`n` the start's number in workriver's records): Herdr takes a live name
once across its server (`agent_name_taken`), and an earlier start's agent may still run. It
seats the agent once Herdr reports it ready (idle or done) with a session of the harness's
own in that pane, and its screen shows no line that starts with `⚠`, which is how Claude Code
and Codex report a model they do not know as they start (both still start, ready). A start
whose screen shows one ends unseated, the agent left running in its pane, and the page shows
that line. Herdr's
`agent_not_ready` (the harness blocked at startup, e.g. on its trust question in a new
folder) keeps the pane: answer in it or from Needs you. Which harnesses are offered comes
from Herdr's `integration.list`: the command found, and Herdr's integration for it current
(`herdr integration status`; `herdr integration install claude` installs it). Measured with
Herdr 0.9.5: over the socket
`agent.start` answers at once with the launch still pending, and refuses a pane whose shell
is not at its prompt yet (`agent_pane_busy`); workriver asks again for a while, then waits as
`herdr agent start` does (ready, or blocked: not ready). Herdr quotes the arguments it
types. An agent's name is lowercase letters, digits, `-` and `_`. Or start a coding agent in a pane yourself, then pick
it on the page (the workspace's line, "Manager") or run `workriver workspace manager ws1
<agent>` in a pane with no agent or a terminal outside Herdr. After Herdr restarts, every pane is
new: a manager resumed in the same folder is the manager again by itself; one that is not
waits for your click in Needs you.

The manager starts a worker and gives it its task with Herdr's own commands (see `herdr
--skill`):

```sh
herdr agent start finder --kind claude --pane <pane id>   # see `herdr --skill`
herdr agent prompt finder "<task u1#1, its goal in a line, where to work, \"run workriver task show u1#1\", and the text of: workriver guide worker>"
```

## Other machines

Each machine runs its own Herdr; workriver keeps one ssh connection to each, which brings
that machine's Herdr socket here and gives the `workriver` CLI over there a way back to
this service.

Requirements: Herdr 0.9.1 or later on both sides (same socket protocol), running on the
other machine.

If Herdr's socket is not at `~/.config/herdr/herdr.sock` there (a named session), give it:
`--herdr-socket <path>`.

The connection brings that machine's client socket here too (`herdr-<label>-client.sock` in
the data directory, from `-client.sock` beside its Herdr socket there), so the page's
terminal view of an agent over there works as here; without it, the view says so and the
gate still answers (it needs only reads and keys). That socket's path is seven bytes longer
than the API socket's, and Herdr finds it only next to it: when it is too long for a socket
(104 bytes on macOS, 108 on Linux, with a long data directory or machine name), it is not
forwarded, the machine is followed as before, and its terminal view says why. Only a loopback ssh was measured, not a
second machine over a network.

"Open in terminal app" focuses the pane in that machine's Herdr; switch to the machine in
Herdr's sidebar yourself.

## Security under Herdr

- The CLI names the pane it runs in (`HERDR_PANE_ID`); the service only cross-checks it
  against the pane it finds the process in. Herdr tells the service the shell at the root
  of each pane.
- Herdr panes can be read by agents: that is why nothing that signs in is ever printed by
  the service, and why `workriver open`, and above all `workriver open --print`, belong in
  a terminal agents do not read.
- A command you run from the page is typed into a new tab of a plain shell (no agent) in a
  Herdr workspace named "workriver-commands" on the unit's machine. A remote machine's
  command runs in that machine's Herdr, as you are there.
- **Herdr's own socket is your account's.** Any program can tell Herdr which agent a pane
  holds, or close a pane. A program can close the pane a session was first seen in through
  Herdr and then report its session. Agents started with `herdr agent start` are not panes
  Herdr knows as an agent's only by report.
- **Other terminals.** Herdr lets any agent read any pane's screen and type into it; what
  another agent's terminal shows is not workriver's to hide. workriver's own routes that
  read a pane's screen, stream its terminal or press keys in it are yours alone (see the
  [Security model](../security.md)).

## Troubleshooting

- **`agent runtime: not connected`**, or a warning about Herdr at start (`workriver
  status` shows it): Herdr is not running, or the service looks at another socket. Start
  the service in a Herdr pane, or set `HERDR_SOCKET_PATH`. A protocol warning means a Herdr
  other than 0.9; workriver may still work, but it is untested.
- **A machine says *connected, but its Herdr does not answer***: Herdr is not running
  there, or its socket is elsewhere (`--herdr-socket`).
- **The terminal view says it cannot be had**: the client socket is not there (a Herdr that serves no
  terminal stream, or another machine connected before this version: disable and enable it), its
  path here is too long for a socket (a shorter data directory or machine name), or
  `herdr` on the service's `PATH` is missing or of another version than the server.
- **"Not delivered: …"** on the page:
  - *Herdr is not reachable*: see the first item;
  - *Herdr refused it*: usually `agent_not_ready` (the agent is still starting, or is not
    the pane's foreground program) or `agent_blocked` (it waits at a question or approval).
    Sort it out in the terminal, then paste the message by hand with "Copy message".
