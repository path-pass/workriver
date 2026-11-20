# Herdr

[中文](herdr.zh-CN.md)

workriver learns who is running, where and in what state, and carries messages to agents,
through an agent runtime. Today that runtime is [Herdr](https://herdr.dev), the only one
workriver supports. This page holds everything in the other documents that is Herdr's
own: where the [README](../../README.md), [Getting started](../getting-started.md) and the
[Security model](../security.md) say "the runtime", read Herdr; where they say an agent's
terminal, read the Herdr pane it runs in; a workspace is a Herdr workspace.

## Installing and running

workriver needs Herdr 0.9 (protocol 22) on the machine it runs on.

Start the service in a Herdr pane of its own, so it finds Herdr's socket and keeps running:

```sh
workriver serve              # http://127.0.0.1:47820
```

Herdr's socket is `~/.config/herdr/herdr.sock` (or `HERDR_SOCKET_PATH`); without Herdr the
service still records and shows, it just sees no agents. `workriver status` should say
`agent runtime: connected`.

The page's terminal button brings the agent's pane to the front in Herdr, then the
terminal app.

## Workspaces and managers

A Herdr workspace runs one or more intents; one manager agent per Herdr workspace plans
them. The manager of a workspace registers with `workriver manager here` from its own
pane, one per workspace.

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

The terminal button focuses the pane in that machine's Herdr; switch to the machine in
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
  another agent's terminal shows is not workriver's to hide.

## Troubleshooting

- **`agent runtime: not connected`**, or a warning about Herdr at start (`workriver
  status` shows it): Herdr is not running, or the service looks at another socket. Start
  the service in a Herdr pane, or set `HERDR_SOCKET_PATH`. A protocol warning means a Herdr
  other than 0.9; workriver may still work, but it is untested.
- **A machine says *connected, but its Herdr does not answer***: Herdr is not running
  there, or its socket is elsewhere (`--herdr-socket`).
- **"Not delivered: …"** on the page:
  - *Herdr is not reachable*: see the first item;
  - *Herdr refused it*: usually `agent_not_ready` (the agent is still starting, or is not
    the pane's foreground program) or `agent_blocked` (it waits at a question or approval).
    Sort it out in the terminal, then paste the message by hand with "Copy message".
