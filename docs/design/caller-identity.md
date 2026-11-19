# Caller identity: which pane a call truly comes from

Design for intent i24. Revision 3 (task u10#6): revision 2 answered the architecture review
u10#2; this one adds what its second round, u10#5, found unsaid (the last two rows of
section 0). The user confirmed it in u10#7 with one change: it is generic, no harness is
special-cased (so what section 4 says a Codex worker must do is, in the recipe, what any
harness whose commands run in a shared background service must do). Implemented in u11#1;
section 9 says where the code is and where it differs from the text. Every number and
output below was measured on 2026-11-19 on this machine (macOS 26.7.3, Herdr client 0.9.5 /
server 0.9.0, workriver 0.1.0 5d3420a20, Codex CLI 0.167.0 with app-server daemon 0.170.0)
or in a Linux container (Debian 13, kernel 6.10); the scripts are attached. What could not
be verified is marked **not verified**.

## 0. What changed in this revision

| Finding | Answer | Where |
|---|---|---|
| P2-1 which agent a pane holds is Herdr's word | Closed where it can be: a session is pinned to the pane root it was first seen at (rule 5). What is left is stated. Section 1 corrected. | 1, 3.1, 3.2, 5, 6 |
| P3-1 `ask --wait` outlives the proof | The CLI fetches a new proof when its own has expired. | 3.1 |
| P3-2 the library rule | Measured: `libproc` on macOS (0.08 ms per chain, +25 KB), `procfs` on Linux (0.01 ms, +92 KB); `sysinfo` ruled out by what it cannot read, with the numbers. The shell snippet stays for the other machine only. | 3.1, 5 |
| P3-3 rule 4's premise | Sampled: a Codex model-run command in daemon mode has **no** terminal run plainly and **has** one run in a pseudo-terminal. Rule 4 is therefore a partial net; 8.1 says so and what was sampled. | 3.1, 4, 6, 8 |
| P3-4 loose ends | One carrier of the named pane; what `/api/here` answers; `WORKRIVER_ALLOW_AS=1` said one way; refusals point at a recipe and no harness is named in the code; CLAUDE.md's Stack bullet listed. | 3.1, 3.2, 6, 7 |

Section 4 is also corrected by this round's samples: what was first read as "the daemon
runs every command" holds for a plain `codex`; a `codex` started with any `-c` override ran
its commands itself.

Added in revision 3, from u10#5 (no new measurement of mine; the review's is cited):

| Finding | Answer | Where |
|---|---|---|
| P3-1 an agent without a session id can be taken by reporting its pane-name | The Herdr implementation does not take a reported id of the pane-name form (`herdr:…`) as a session: such an agent is named by its own pane. | 3.1 (rule 5), 6, 7 |
| P3-2 unsaid around rule 5 | Named: any process can close another agent's pane through Herdr and then take its session. On another machine pins are read only when a call comes from there, at no extra ssh call. A pane with no known shell is not pinned. | 1, 3.1, 3.3, 5, 6 |

## 1. Summary

Today the service believes the pane a request names (`x-workriver-place`, from the CLI's
`HERDR_PANE_ID`). A pane that does not exist, or a plain one, plus the readable user token
makes the caller **the user**; another agent's pane makes it **that agent**.

Proposed: the service stops reading identity from what the request says and reads it from
**the process that calls**:

1. The CLI opens a Unix socket of the service first. The kernel tells the service the pid of
   whoever connected (nothing the caller can choose). The service answers with a short-lived
   proof the CLI sends along with its HTTP requests.
2. The service reads that process's chain of parents and asks the runtime for the root
   process of each pane. The caller is **in the pane whose root is in its chain**.
3. The pane the CLI names is only cross-checked: if it differs from where the process is,
   the request is refused with a sentence saying both.
4. Outside every pane a caller is the user only with the token **and** a controlling
   terminal. This catches a harness's helper that runs commands plainly; it does not catch
   one that gives its commands a pseudo-terminal (sampled, section 4).
5. Which agent a pane holds still comes from the runtime, and Herdr takes that from whoever
   tells it. So a session is **pinned to the pane root it was first seen at**: while that
   root runs, the same session from another pane is refused.
6. On another machine there is no service to connect to; the CLI sends its pid and the
   service checks it on that machine through the runner (weaker, see 3.3; never the user).

What this gives, said exactly:

- The Codex incident becomes a refusal that says why.
- "Be the user from an agent's pane" needs a deliberate detach instead of one variable.
- "Be another agent" needs more than naming its pane **and** more than one call to Herdr:
  the other agent's pane has to be gone, or the program has to type into that pane. "Gone"
  does not mean waiting: the same Herdr socket closes any pane, so it costs two calls and
  that agent's terminal, which the user sees disappear. It is not closed (section 6).
- It does not make agents under your account unable to act as you (section 6).

## 2. What happened, and what is wrong today (verified)

### 2.1 The incident

`workriver task show u9#2` on the real service: `Output (delivered 2026-11-19T06:42:16Z by
the user)`, `P2-1 … reported by the user`. The work was a Codex worker's.

Codex's shared background server, read with `ps` (nothing changed):

```
15525 69172 Wed Nov 18 13:33:55 2026  ~/.codex/packages/app-server-daemon/releases/0.170.0-aarch64-apple-darwin/bin/codex app-server --listen unix:// … --managed-daemon
69172     1 Mon Nov 16 07:23:10 2026  …/bin/codex app-server daemon pid-update-loop
$ ps eww -o command= -p 15525 | tr ' ' '\n' | grep -E '^(HERDR|PWD)'
HERDR_ENV=1
HERDR_PANE_ID=wM:p4
HERDR_SOCKET_PATH=/Users/zhangdongwei/.config/herdr/herdr.sock
PWD=/Users/zhangdongwei/99_git/workriver/.local/worktrees/ja-ui
```

`herdr pane list` has no `wM:p4` (wM holds p1, pT, p5). The daemon was started on Nov 18 from
a pane that is gone and keeps that pane's environment; a plain `codex` since then hands its
commands to it (section 4).

### 2.2 Reproduced on a temporary service

Isolated `herdr --session wr-u10 server` with a plain pane `w1:p1` and two panes reported as
agents (`w1:p2`, `w1:p3`); `workriver serve --port 47931` with its own `WORKRIVER_HOME`.
Every command below was run from a shell that is in **none** of those panes; only
`HERDR_PANE_ID` was set.

```
HERDR_PANE_ID=w1:p2  workriver manager here            -> manager of workspace exp
HERDR_PANE_ID=wM:p4  workriver task start u1#1         -> recorded
HERDR_PANE_ID=w1:p3  workriver task start u1#2         -> recorded
HERDR_PANE_ID=w1:p1  workriver task start u1#3         -> recorded
HERDR_PANE_ID=wM:p4  workriver intent set i1 --findings user   -> recorded   (a user-only act)

sqlite3 workriver.db "select seq,kind,actor,task from events"
 4|manager.registered|herdr:w1:p2|
10|task.started|user|u1#1            <- a pane that does not exist: the user
12|task.started|herdr:w1:p3|u1#2     <- another agent's pane: that agent
13|task.started|user|u1#3            <- a plain pane claimed from outside: the user
```

Without the token the stale pane is refused (`Herdr lists no agent in pane wM:p4 …`). So:

| What the request names | Token readable | Recorded as |
|---|---|---|
| a pane that does not exist | yes | **the user** (the incident) |
| a plain pane, from anywhere | yes | **the user** |
| no pane at all (`env -u HERDR_PANE_ID`) from an agent's pane | yes | **the user** |
| another agent's pane | either | **that agent** (also reads as it: blind reviews) |

The code: `App::who` (serve.rs:838) takes `place` from the request; with no agent there and
`caller.user` it falls through to `App::user`, which refuses only when the *named* pane holds
an agent.

### 2.3 The second step: which agent a pane holds (the review's P2-1)

Measured by the review u10#2 on an isolated Herdr, by processes truly inside the pane they
name: a real Claude's own shell command called `pane.report_agent_session` for its pane with
the manager's session id and its next call was recorded as the manager; Herdr then showed
that session in two panes at once. Any process of the account can make that call for any
pane. This revision adds rule 5 for it.

## 3. The design

### 3.1 The check on this machine

**Who connected.** The service listens on a Unix socket, `<WORKRIVER_HOME>/caller.sock`
(mode 600; the service says so and does not start if the path is longer than the system's
limit for socket paths, 104 bytes on macOS, 108 on Linux; met in this round's experiment:
a socket under the long scratch folder failed with `AF_UNIX path too long`). For each
connection it reads the peer's pid from the kernel (`tokio::net::UnixStream::peer_cred()`:
`LOCAL_PEERPID` on macOS, `SO_PEERCRED` on Linux; tokio 1.54.1, already a dependency,
returns `pid: Some(..)` on both, `src/net/unix/ucred.rs`), reads the process chain (below) at
once, and writes back one line: a random proof (16 bytes from `auth::random_hex`). It keeps
`proof -> chain` for 30 seconds. The CLI sends the proof as `x-workriver-caller` with every
request; `guard` puts the chain into `Caller`. **A command that lasts longer than its
proof** (`workriver ask --wait` polls `/api/state` once a second until its timeout,
main.rs:1519) **fetches a new one**: the service answers an expired proof with its own
status line, and the CLI connects again and repeats the request once. A request without a
valid proof has no process: it is nobody (the page's session cookie is the one other way to
be someone, unchanged).

Verified (`peer.py`): a child process connects, the server reads the pid from the socket.

```
macOS:  unix socket: client pid 93871, kernel says 93871, match=True, 11 us
Linux:  unix socket: client pid 8, kernel says 8, match=True, 14 us
```

**Its chain.** For the pid: itself and its parents up to init, each with parent, controlling
terminal, start time and name; bounded at 64 steps (as `metrics::top_of`). On this machine
it is read in-process with a library, per system:

- macOS: `libproc` 0.15, `proc_pid::kproc_info_raw` (the `sysctl kern.proc.pid` that `ps`
  itself uses). Measured (`kp.rs`), this session's own tool shell:

  ```
  (27157, 27074,        -1, 1795075783, 557073, "kp")
  (27074, 83970,        -1, 1795075782, 596833, "zsh")       <- the tool's shell: no terminal
  (83970, 83912, 268435472, 1795074043, 317037, "2.1.328")   <- claude
  (83912,  6286, 268435472, 1795074043,  80468, "zsh")       <- Herdr: shell_pid of wM:pT
  ( 6286,  6285,        -1, 1794672535, 534420, "herdr")
  ( 6285, 42956, 268435456, 1794672535, 502166, "herdr")
  (42956, 42954, 268435456, 1794621592, 292258, "zsh")
  (42954,  1359, 268435456, 1794621592, 254613, "login")     <- root's process: read too
  ( 1359,     1,        -1, 1794589277, 487189, "ghostty")
  libproc kproc_info: 82.186µs per chain of 9 steps
  ```
  (pid, parent, terminal device or -1, start seconds, start microseconds, name.)
- Linux: `procfs`, `Process::stat()` (`ppid`, `tty_nr`, `starttime` in clock ticks, `comm`).
  Measured in the container (`pf.rs`): `procfs: 9.618µs per chain of 3 steps`.

The numbers that chose them are in section 5.

**The panes' roots.** A new runtime method in workriver's terms, "the processes at the root
of a place" (`Runtime::processes` has this for agents' places only). Herdr: `pane.process_info`
gives `shell_pid` (and the foreground processes, used when it knows no shell, as
`Herdr::processes` does today). Verified on the real Herdr, read-only: `('wM:pT', 83912, …)`,
and `pane_not_found` for a pane that is gone.

**The one carrier of the named pane.** Rule 1 reads the pane the request names from the
`x-workriver-place` header and from nowhere else. The `place` in a query or a body
(`Reader.place`, `Append.place`, `Here.place`, the `?place=` of an attachment link) is no
longer read by the service: the CLI stops sending it, and `App::who` loses its `place`
argument and takes everything from `Caller` (the chain and the header). `--as` stays a
named argument, checked first as today.

**The rule.** `pane(P)` = the pane whose root is P or one of P's parents; else the pane
whose root has the same controlling terminal as P (a job left behind by `( cmd & )` keeps
it); else none.

1. The request names a pane and `pane(P)` is not that pane: **refused**.
2. `pane(P)` holds an agent: the caller is **that agent**, token or not, subject to rule 5.
3. `pane(P)` is a plain pane: **the user** if the request carries the token, else refused
   (as today).
4. No pane: **the user** if it carries the token and P has a controlling terminal; else
   refused.
5. **One session, one pane.** For every agent the runtime reports with a session id of its
   harness, the service keeps the root (pid and start time) of the pane it first saw that
   session in, taken in its regular read of the live agents (the `POLL` tick) and whenever
   it looks a place up, and kept in `<home>/session-roots.json` (as `merge-told.json` is) so
   that a restart does not forget. While that root runs, the session is that pane's only: a
   caller whose pane reports the same session from another root is **refused**. A session
   first seen at two roots in the same read is refused at both until one is left. The pin
   is released when its root no longer runs (the pid is gone or has another start time). A
   pane may show a new session at any time (the agent started a new conversation): that is
   a new agent, as today.

   Three cases around it:
   - **An agent without a session id** is named by its pane (`herdr:<pane>`, with the
     machine's label on another machine), and that name is made only by the runtime
     implementation from the pane the agent is in. A *reported* session id of that form is
     not taken as a session: `herdr.rs` drops it and names the agent by its own pane, so a
     pane that reports `herdr:w1:p3` as its session is `herdr:<its own pane>`, never the
     agent of `w1:p3`. (The review u10#5 measured the hole this closes: such a report was
     recorded as the live pane `w1:p3`'s agent. `user` and `workriver` as reported ids are
     refused by the store already.) A pane-named agent needs no pin: being it takes a
     process in that pane, which rules 1 and 2 check.
   - **A pane where the runtime knows no shell.** Its root is then its foreground
     processes (above), which change, so a pin on them would be released by every change.
     Such a pane is still found by rules 1 to 4, and its session is still refused while
     pinned to another root that runs, but its own session is **not pinned**: another pane
     that reports it is seen beside it, and both are refused (the two-roots case) for as
     long as both show it. All 19 panes of the real Herdr had a known shell when read.
   - **Where the roots are read.** On this machine: in the regular read and at each
     look-up, as said. On another machine: only when a call comes from there (3.3).

`App::user`'s "a caller whose pane holds an agent is not the user" uses `pane(P)` too.

**`/api/here`** ("who and where am I") answers about `pane(P)`: the agent there (rule 5
applied), its group and that group's current intent. It no longer asks the runtime about a
pane taken from the body. A caller that names another pane gets rule 1's refusal; a caller
in no pane gets "no pane" (the user outside Herdr has no group). On a machine's listener it
uses 3.3. It stays answerable without the token, but not without a proof.

Prototype of rules 1 to 4 (`check.py`) against the isolated Herdr and real processes; the
pid is what the socket would report:

```
 1 child in plain pane p1, says p1, token            -> USER (plain pane w1:p1, token) [descends from its shell]
 2 child in plain pane p1, says p1, no token         -> REFUSED: no agent in pane w1:p1 and no user token
 3 grandchild in agent pane p2, says p2, token       -> AGENT of pane w1:p2 (claude) [descends from its shell]
 4 in agent pane p2, says agent pane p3              -> REFUSED: the command says pane w1:p3, but its process sleep(4715) > -zsh(90324) > herdr(89391) runs in pane w1:p2
 5 in agent pane p2, says nothing, token             -> AGENT of pane w1:p2 (claude) [descends from its shell]
 6 in agent pane p2, says a pane that is gone, token -> REFUSED: the command says pane wM:p4 (there is no pane wM:p4 any more), but its process … runs in pane w1:p2
 7 a shell not in that Herdr, says p3, token         -> REFUSED: the command says pane w1:p3, but its process zsh(4703) > claude(83970) > … runs in no pane of this Herdr
 8 an agent's tool shell outside every pane, token   -> REFUSED: process zsh(4703) > claude(83970) > … is in no pane and has no terminal: an agent's helper or a background job, not the user at a terminal
 9 Codex daemon's child, says wM:p4, token           -> REFUSED: the command says pane wM:p4 (there is no pane wM:p4 any more), but its process codex-code-mode-host(56690) > codex(15525) > codex(69172) runs in no pane of this Herdr
10 Codex daemon's child, says nothing, token         -> REFUSED: process codex-code-mode-host(56690) > codex(15525) > codex(69172) is in no pane and has no terminal: …
11 orphan `(cmd &)` from plain pane p1, token        -> USER (plain pane w1:p1, token) [shares its terminal]
12 orphan that left its session (setsid), token      -> REFUSED: process python3(4714) is in no pane and has no terminal: …
13 a process that is gone                            -> REFUSED: the calling process 99998 is gone
14 the user's Ghostty shell outside Herdr, token     -> USER (terminal ttys000 outside Herdr, token)
15 the same, no token                                -> REFUSED: neither in a pane nor the user
```

Line 10 holds for a command run plainly; the same daemon's command run in a
pseudo-terminal has a terminal and would pass rule 4 (section 4).

Rule 5 on the review's sequence, with the roots read this round from an isolated Herdr
(`(28023, …, 1795075812, 53486, "zsh")` for `w1:p2`, `(28039, …, 1795075812, 79067,
"zsh")` for `w1:p3`: two panes made in the same second differ in their microseconds):

```
p2 calls as SESS-A      -> agent SESS-A
p3 calls as SESS-A      -> REFUSED: session SESS-A was first seen at root p2, which still runs; it cannot also be at p3
p2 calls as SESS-X      -> agent SESS-X  (a pane may start a new session)
p2 closed; p3 as SESS-A -> agent SESS-A  (what is left: a session whose pane is gone)
```

That one session shows in two panes at once in Herdr's answer is the review's measurement
(`SESS-A` live in `w1:p1` and `w1:p4`); my own `pane.report_agent` on plain shells this round
was answered `ok` but not listed, so I did not reproduce that part myself.

### 3.2 Every caller

| Caller | How it is known | If the check fails |
|---|---|---|
| An agent in its pane | proof -> chain reaches the pane's root; the pane holds an agent (rule 2); its session is pinned to this root or to none that runs (rule 5) | another pane named: "this command says it runs in pane X, but its process A > B > C runs in pane Y / in no pane; workriver takes the agent from the pane the command really runs in (`workriver guide how hand-off`)". Session held elsewhere: "session S was first seen in pane X, whose shell still runs; one session is one pane: close that pane, or start a new session here" |
| The user in a plain Herdr pane | chain reaches a pane with no agent, and the token (rule 3) | no token: today's `no_agent_at` sentence |
| The user in a terminal outside Herdr | no pane, the token, a controlling terminal (rule 4) | no terminal: "… is in no pane and has no terminal: workriver cannot tell whose command this is. As the user: a terminal window, or the page. An agent: from its own pane; if its commands do not run there, see `workriver guide how hand-off`" |
| The page | session cookie from our origin (unchanged; a browser has no process to check and needs none) | today's `NOT_THE_USER` |
| A request with the token but no proof (`curl`, an old CLI) | nobody: the token alone no longer makes the user | "this request does not show which process sends it: use the `workriver` CLI of this version, or the page" |
| An agent on another machine | section 3.3 | "… its process is not in pane X on machine M"; never the user (unchanged: `machine_router`, no token there) |
| `--as <agent>` | only when the service was started with `WORKRIVER_ALLOW_AS=1` (below) | "--as is off …" (unchanged) |

No refusal names a harness or a harness's flag; they point at the recipe, where what a
worker of each harness must do is written (section 4).

**`WORKRIVER_ALLOW_AS=1`, one way.** It does exactly two things, for tests and development:
(a) `--as <agent>` names an agent without any process check, never the user (as today);
(b) rule 4 does not ask for a terminal (a test run has none). Everything else stays on: the
proof, rules 1 to 3 and 5, the token for the user. In-process tests give the service a chain
as they give it a fake runtime.

Never the user and never another agent on a failure of rules 1 to 5: every failing branch
returns an error; none falls through to `App::user`. What rule 5 cannot see is in section 6.

### 3.3 Another machine

The service is not on that machine, so no socket of the kernel's can vouch there. The CLI
sends its own pid (`x-workriver-pid`); the machine's listener (and only it) takes it. The
service reads the chain on that machine through its runner with one shell snippet
(`chain.sh`: `/proc/<pid>/stat` where there is `/proc`, else `ps -o
pid=,ppid=,tty=,lstart=,comm= -p <pid>` per step), as `metrics::PS` is run there today; reads
the pane's root from that machine's Herdr (the forwarded socket, as `processes(machine)`
does); and applies rules 1, 2 and 5 (the pinned root carries its machine). Anything else is
refused, as today.

Rule 5 there costs no call of its own. The service does not read that machine's roots on
its regular tick (that would be an ssh call per machine every 2 seconds): a session of that
machine is pinned when its first call arrives, with the root's pid and start time taken from
the caller's own chain, in which the root is one of the lines. Whether a pinned root still
runs is asked only when another root there claims the same session: one more run of the
snippet for that pid. The price of reading late: there the pin goes to the first pane that
*calls* with a session, not the first that shows it, so a pane that reports another
agent's session before that agent has made any call is pinned as it. An agent that has
called once (a manager registers itself at its start) is pinned like here. Two extra conditions, because the pid is the caller's word: the process
must be alive and be named `workriver`.

What that is worth: it stops the stale environment and the one-variable claim. A program on
that machine that sets out to can name the pid of another pane's `workriver` call while it
runs. A caller there is never the user in any case. **Not verified**: no machine is followed
here (`workriver machine list` prints nothing), so neither the cost of the extra ssh call nor
the chain over ssh was measured; the snippet's `/proc` branch was run in the Linux container
(3.4 ms per chain), its `ps` branch on this machine (24 ms for 9 steps).

The attachment links printed for remote agents (`…?place=$HERDR_PANE_ID`, "fetch it with
curl", machines.rs:955) cannot carry a checked identity. They become a CLI command
(`workriver attach fetch <link>`), which sends its pid like every other call and uses the
route that is already on the machine's listener.

### 3.4 Races

- **The process exits.** The CLI waits on the socket for the proof, so the chain is read
  while the peer is connected. A peer that vanished: no proof (line 13).
- **Pid reuse.** The chain is read within the accept, 0.08 ms on macOS. As a second line
  the service drops a chain whose first process started after the accept: a process that
  took the pid later cannot have started earlier. With the libraries the start time has
  microseconds on macOS and clock ticks (10 ms) on Linux, so the one-second weakness of the
  first revision's `lstart` is gone on this machine; it remains for the other machine's
  `ps` branch (macOS there), where the pid is the caller's word anyway.
- **The proof** is bound to that chain, lives 30 s, is sent over loopback only, and may be
  used for the several requests one command makes; an expired one is replaced (3.1). A
  process that hands its proof to another lends its identity; it could as well run the
  command for it.
- **Parents change while being read.** A parent that exits moves its children to init (or
  a subreaper on Linux): a chain can only lose its way to a pane, never gain one into
  another pane. Verified: `( sleep 202 & )` -> `94423 1 … ttys022`.
- **A pane's root.** Read fresh from the runtime on each request, never kept, except the
  pinned (pid, start time) of rule 5, which is compared with the fresh one.
- **First sight of a session** (rule 5). A session id is a random id of the harness's, so
  it cannot be taken before its agent exists. Between the agent's start and the service's
  next read another pane can report the same id: both are then seen together and both are
  refused, never the wrong one accepted.

### 3.5 What the service can read about other processes

- macOS: `sysctl kern.proc.pid` (libproc's `kproc_info_raw`, and `ps`) answers for every
  account's processes: parent, terminal, start time (verified: the root-owned `login`
  between the user's shell and Ghostty is read). `proc_pidinfo` does not: `sysinfo` and
  libproc's `pidinfo::<BSDInfo>` both stopped at `login` (measured, section 5). The
  environment of another account's process is not shown.
- Linux: `/proc/<pid>/stat` and `status` of another account's process are readable
  (verified as `nobody` on a process of another user: `PPid`, `tty_nr`, start time);
  `environ` is not (`Permission denied`). A system mounted with `hidepid` hides them: the
  chain then cannot be read and callers of other accounts are refused. `ps`, `lsof` and
  `ss` are absent from a slim image (verified), which is why the other machine's snippet
  reads `/proc`.
- Processes in another pid namespace (a container) show pid 0 or none to the service:
  refused. Not a case workriver has today.

## 4. Codex

**What was sampled** (isolated Herdr panes; `probe.sh` / `probe3.sh` print the pane the
command sees, its terminal, its chain, and what it can connect to):

| Codex started as | Command | Pane it sees | Runs under | Terminal |
|---|---|---|---|---|
| `codex` | `!` shell command | `wM:p4` (stale) | the daemon 15525 > 69172 > 1 | none |
| `codex` | model-run, plain | `wM:p4` (stale) | the daemon | none |
| `codex` | model-run, pseudo-terminal | `wM:p4` (stale) | the daemon | **`/dev/ttys026`** |
| `codex --no-daemon` | `!` shell command | its own pane | the TUI > the pane's shell | none |
| `codex -c model_reasoning_effort="low"` | `!`, model-run plain | its own pane | the TUI > the pane's shell | none |
| the same | model-run, pseudo-terminal | its own pane | the TUI > the pane's shell | `/dev/ttys026` (not the pane's) |

```
--- mode=daemon-model PANE=wM:p4 TTY=not a tty pid=33577
(33577, 15525, -1, …, "bash")  (15525, 69172, -1, …, "codex")  (69172, 1, -1, …, "codex")
--- mode=daemon-model PANE=wM:p4 TTY=/dev/ttys026 pid=33591
(33591, 15525, 268435482, …, "bash")  (15525, 69172, -1, …, "codex")  (69172, 1, -1, …, "codex")
--- mode=daemon (started with -c …) PANE=w1:p1 TTY=not a tty pid=31457
(31457, 28108, -1, …, "bash")  (28108, 28016, 268435479, …, "codex")  (28016, 27640, 268435479, …, "zsh")  (27640, 1, -1, …, "herdr")
```

Three model turns were used for this (two at low effort, one at the configured effort); no
model-run sample was taken under an explicit `--no-daemon`: the `-c` run stands in for it,
and its `!` command shows the same tree as `--no-daemon`'s.

**Why the pane is stale.** Since Codex CLI 0.157.0 a plain `codex` hands its work to a
shared background server (`codex features list`: `daemon_auto_start  stable  true`). The
server is started once, detaches (`pid-update-loop`, parent 1) and keeps the environment of
the session that started it; both the `!` commands and the model's commands run there.

**What Codex offers.** `codex --help`: "`--no-daemon`  Run without the shared background
server, even if it is already running". Official reference: "Run the interactive CLI without
the shared background server. Can't be combined with `--remote` or `codex agents`." Turning
the feature off is not enough while a daemon runs:

```
$ codex --disable daemon_auto_start
Error: Cannot use the shared background server: This session requires daemon_auto_start to be disabled.
To work without the background server, rerun the same command with --no-daemon (including resume or fork and its arguments).
```

A `codex` started with a `-c` override also ran its commands itself (the table). That is
an observation of this version, not something its help promises, so the recipe uses
`--no-daemon`. No option was found that makes the daemon run a command with the calling
TUI's environment or as its child. Commands do carry `CODEX_THREAD_ID`, but nothing ties it
to a pane that workriver could check: Herdr's own Codex hook (`~/.codex/herdr-agent-state.sh`)
reports the session with `$HERDR_PANE_ID` and so, in daemon mode, reports it to the daemon's
pane. Where that pane is live and holds another Codex agent, the hook would overwrite that
agent's session in Herdr; under rule 5 the overwritten pane then shows a session pinned
elsewhere, or a new one. **Not measured.**

**The sandbox.** A model-run command inside Codex's sandbox could reach neither workriver's
port nor a Unix socket, in either mode (`tcp 127.0.0.1:47820/api/status -> 000`, `unix … ->
PermissionError: [Errno 1] Operation not permitted`); `ps` was refused there too, the
library's read of the chain was not. Its `!` commands reached both. So the socket adds no
new wall: a `workriver` call from Codex already has to run outside the sandbox today.

**What a Codex worker must do.** Be started with `--no-daemon`:
`herdr agent start <name> --kind codex --pane <id> -- --no-daemon` (Herdr passes what follows
`--` to the agent; `herdr agent start --help`), and `codex resume --no-daemon` to come back.
This is written in `skills/workriver-howto/references/hand-off.md` and `CHANGES.md`, and
nowhere in the code. A Codex worker in daemon mode is refused on its first `workriver task
start` by rule 1 with the sentence of 3.2, which points at that recipe; its manager restarts
it. Nothing in the user's Codex config has to change, and the daemon may keep running for
the user's other Codex sessions. Whether Herdr learns a `--no-daemon` Codex's session id
through its hook was **not verified**, and the design no longer depends on it: with an id the
worker is pinned by rule 5, without one it is named by its pane, which cannot be reported
by another (rule 5, first case).

Left on this machine by the experiments: four Codex sessions in `~/99_git/workriver`
(`01a1f2c8-da60-7003-aede-2107fb181034`, `01a1f2c9-b051-7673-ad4b-f238906e1b19` from the
first round, empty; `01a1f2df-0985-78a3-a963-af3a616824f0` and one whose id I did not keep
from this round, each with the probe turns). No config written, the daemon not touched
(still pid 15525, started Nov 18 13:33:55).

## 5. What it costs

Per CLI command, on this machine (medians):

| Step | Cost | Note |
|---|---|---|
| Connect to the socket, kernel's pid | 0.011 ms (0.014 ms Linux) | `peer.py` |
| The chain, in-process | 0.08 ms for 9 steps on macOS (`libproc`); 0.01 ms for 3 steps on Linux (`procfs`) | `kp.rs`, `pf.rs`, 1000 runs |
| Herdr `session.snapshot` | 0.5 ms (19 panes, 10 agents) | already paid today by `locate` |
| Herdr `pane.process_info`, the named pane | 0.2 ms | one more runtime call |
| the same for all 19 panes (caller names none) | 3.7 ms | the user outside Herdr only |
| Rule 5 in the regular read (every 2 s): a root per agent pane | 10 x (0.2 + 0.01) ms per tick | this machine only, not per call; on another machine nothing per tick (3.3) |
| A whole CLI call today (`workriver status`) | about 10 ms | about 11 ms after |

Caching: the proof carries the chain for the command's requests, so a command pays once.
Nothing is kept between commands (each is a new process); pane roots are read fresh.

**The library for the chain** (CLAUDE.md: prefer an established library; say which one was
turned down and why, with the number). Release builds of a one-function program against an
empty one, same profile as workriver's (its binary is 16.3 MB):

| Way | Per chain | Size | What it reads |
|---|---|---|---|
| `libproc` 0.15 `kproc_info_raw` (macOS) | 0.08 ms | +24.6 KB | parent, terminal, start in microseconds; every account's processes |
| `libproc` 0.15 `pidinfo::<BSDInfo>` (macOS) | 0.002 ms | +21.8 KB | the same fields, but stops at a root-owned parent (`login`) |
| `sysinfo` 0.40 (macOS) | 0.15 ms | +87.0 KB | parent, start in **seconds**, **no controlling terminal**; stops at `login` |
| `procfs` (Linux) | 0.010 ms | +91.9 KB | parent, `tty_nr`, start in ticks |
| by hand, `/proc/<pid>/stat` (Linux) | 0.009 ms | +6.8 KB | the same |
| one `ps` per step (macOS, revision 1) | 24 ms | 0 | start in seconds |
| one `ps -A` for the whole table (macOS) | 104 ms with the terminal column (33 ms without) | 0 | 904 processes |

Chosen: `libproc` on macOS and `procfs` on Linux, behind one function. Turned down:
`sysinfo`, not for its cost but because it gives no controlling terminal (rules 4 and the
`( cmd & )` case need it) and whole seconds for the start time (3.4); the hand-written
`/proc` reader, which would save 85 KB of 16.3 MB (0.5 %) and is not worth leaving the rule
for; `ps`, 300 to 1300 times slower. `libproc` builds its bindings with `bindgen` (25
crates with its build dependencies, clang at build time); if that weighs on the build, the
same `sysctl` through `libc`, which is already in the lock file, is twenty lines of
`unsafe`. The shell snippet stays only for the other machine, where nothing of ours runs
but what the runner starts.

Other ways considered and turned down:

- **The CLI sends its pid, on this machine too** (the intent's first direction). It fixes
  the accident, but the pid is the caller's word: a `curl` with the token names pid 1 or a
  pid in another pane. Kept only where nothing better exists (3.3).
- **Find the pid from the TCP connection** (no change to the CLI): `lsof
  -iTCP@127.0.0.1:<port>` took 61 ms and returns both ends; `lsof` and `ss` are not on a slim
  Linux.
- **All of the CLI's HTTP over the Unix socket**: ureq 3.5.2 has other transports only in
  its `unversioned` module ("not considered final API"); it would mean another HTTP client
  in the CLI.
- **A list of harness process names** to tell agents' helpers from the user: against "no
  harness is special-cased", and brittle (the kernel's name for Claude's process is
  `2.1.328`; Herdr tells it from its screen, per the review). It would catch the
  pseudo-terminal case rule 4 misses, for the harnesses on the list only.

What breaks, for whom:

1. **Codex workers in daemon mode**: refused until started with `--no-daemon`. Intended.
2. **Requests with the token and no proof** (`curl`, scripts): refused. In the repository
   only the remote attachment link uses `curl` (3.3). The skills already say "a bare `curl`
   … " is not an identity (`CHANGES.md:277`).
3. **An older CLI against the new service**: refused with the sentence of 3.2. On this
   machine the CLI and the service are one binary; on a followed machine the CLI must be
   updated with the service (`workriver guide how updating`).
4. **The user's commands without a terminal**, outside Herdr (cron, launchd, an editor's
   task runner): refused as the user. From a terminal or the page they work.
5. **Agents that pose as the user for their own experiments** by unsetting
   `HERDR_PANE_ID`: in a pane of the Herdr that service follows they are now that agent;
   outside it they have no terminal and are refused. Such a service is started with
   `WORKRIVER_ALLOW_AS=1`.
6. **Agents under another account or in a sandbox** that cannot open
   `<WORKRIVER_HOME>/caller.sock` (the home is `drwx------`): cannot be identified, refused.
   Today another account can record over TCP. (Codex's own sandbox blocks TCP as well, see
   section 4: nothing new there.)
7. **Agents inside a multiplexer of their own in a pane** (tmux): their processes descend
   from the tmux server, not from the pane. With the pane in their environment: refused
   (rule 1). **Not verified** with tmux itself; it follows from lines 9 to 12.
8. **A session resumed in a second pane while its first pane is still open** (rule 5):
   refused until the first pane is closed, with the sentence of 3.2. After a restart of
   Herdr every root is new and the old ones are gone, so sessions it reopens are pinned
   afresh (**not verified** against a real Herdr restart).

A command that outlives its proof (`ask --wait`) does not break: 3.1.

## 6. What is left

Plainly: agents run under your account, and this check does not change that.

- **A program that sets out to be you still can.** Read the token, start a process that
  leaves the pane (`setsid`, a double fork), give it a terminal of its own (`script`), call
  the socket. One line of shell. What changed is that it takes that line: no accident and no
  single variable does it any more.
- **Herdr's own socket is the account's.** Which agent a pane holds, or that it holds none,
  is what Herdr was told, and any process of the account can tell it for any pane. Rule 5
  narrows what that buys; it leaves:
  - **A session whose pane is gone can be taken.** Once the pinned root no longer runs,
    whoever reports that session for its own pane is that agent: its records, its tasks, and
    a manager's right to decide findings if it was a manager. Agents whose panes were closed
    are exactly those. **And a pane does not have to be waited for:** Herdr's socket closes
    any pane (`pane.close`), so a program that sets out to be another agent closes that
    agent's pane and then reports its session: two calls. What it cannot do is do that
    unseen: the agent's terminal is gone, and the work recorded afterwards under its name
    comes from another pane.
  - **On another machine the pin goes to the first pane that calls** with a session, not
    the first that shows it (3.3).
  - **A pane with no known shell is not pinned** (rule 5): its session reported by another
    pane gets both refused, which stops that agent rather than replacing it.
  - **An agent can be given another name.** A report for somebody else's pane makes that
    pane show a new session: its next calls are a new agent's and its own tasks refuse it.
    Nobody is impersonated; work stops until someone notices.
  - **A pane Herdr knows as an agent only by report can be released** and is then a plain
    pane: the user, with the token. The review measured this for a report-only agent and
    found a real Claude's pane stays an agent's after a release. Workers started with
    `herdr agent start` are real harnesses; a harness Herdr cannot tell from its screen
    would be open to this.
  - **Two panes showing a session at first sight** are both refused: a way to stop an
    agent's first calls.
- **Typing into another terminal.** Herdr lets any agent type into any pane: a command
  typed into agent B's pane is B's, one typed into a plain pane is yours.
- **A harness's background service that gives its commands a pseudo-terminal, started
  outside Herdr or with no pane in its environment**, passes rule 4 with the token and is
  the user. Sampled: Codex's daemon does give a terminal when the model asks for one. Rule 4
  catches the plain case only.
- **An agent that does not run in a Herdr pane but has a terminal** (another terminal
  window, tmux started outside Herdr) is the user as far as the service can tell, as today.
- **The files.** The database, the attachments, the token and `session-roots.json` in
  `~/workriver-data` can be read and written from disk without the service.
- **Another machine.** The pid there is the caller's word (3.3).
- **Injecting into a process** of another pane (a debugger) makes its calls that pane's.
- **`WORKRIVER_ALLOW_AS=1`** lets any local caller with a proof record as any agent by
  name, and drops rule 4's terminal; nothing else (3.2). The service says so at start.

`docs/security.md` keeps its last section; its sentences "the CLI says which pane it runs in
and the service believes it", "found by the service from Herdr, not from what the command
says" and the "Claiming another pane" limit are replaced by the above.

## 7. What changes

- `crates/workriver-core/src/runtime.rs`: `Runtime` gains "the root processes of a place"
  (default: none); `machines.rs` routes it per machine; `herdr.rs` implements it with
  `pane.process_info` and holds the Herdr words of the refusals. No harness is named in
  the code: refusals point at `workriver guide how hand-off`. In `agents_of`
  (herdr.rs:278) a reported session id that starts with `herdr:` is dropped, so the agent
  is named by its pane.
- `crates/workriver/src/serve.rs`: the socket and the proofs beside `Auth`; `Caller` carries
  the chain and the header's pane; `App::who` loses its `place` argument and applies rules 1
  to 5, `App::user` uses `pane(P)`; `App::here` answers about `pane(P)`; the `place` fields
  of `Reader`, `Append`, `Here`, `Upload` and the others go; the pins of rule 5 with their
  file; the machine listener takes `x-workriver-pid` (3.3). `App::view` is unchanged (it
  goes through `who`). `machine_router` gets no new route.
- `crates/workriver/src/main.rs`: the CLI fetches a proof once per command and again when
  it has expired; sends the pane only as the header, and its pid; `attach fetch`.
- The chain: one function, `libproc` (macOS) and `procfs` (Linux) as target-specific
  dependencies of the outer crate; the snippet for the other machine beside `metrics::PS`,
  with its parser and tests on both formats.
- Tests: each row of 3.2, each line of the prototype, the four lines of rule 5, a reported
  `herdr:` id and a pane with no known shell as tests
  with a fake runtime and a fake chain; one end-to-end test with a real socket and a real
  child process; `ask --wait` past a short-lived proof.
- Documents: `docs/security.md` and `.zh-CN.md`; in `CLAUDE.md` / `AGENTS.md` both the
  security rule ("the CLI sends its pane with every request …") and the Stack bullet ("The
  CLI sends where it runs (under Herdr: `HERDR_PANE_ID`) and the service finds the agent
  there"), which becomes: the service finds the caller's process and its pane itself; the
  CLI's "Who records" help; `hand-off.md` (what a worker of a harness with a background
  service must be started with; Codex: `-- --no-daemon`), `attachments.md`, `CHANGES.md` in
  `skills/`.

## 8. For the gate

Three points are the user's to confirm; the design assumes the first answer of each.

1. **The terminal rule** (rule 4). What it buys, as sampled: outside every pane, a command
   without a terminal is refused instead of being the user. That covers Claude's tool
   commands, Codex's `!` commands and Codex's model-run commands run plainly, in daemon mode
   and without it. It does **not** cover a Codex model-run command that asks for a
   pseudo-terminal: in daemon mode that one has a terminal (`/dev/ttys026`) and, from a
   daemon with no pane in its environment, would be the user. No other harness was
   sampled. What it costs: the user's own commands without a terminal outside Herdr (cron,
   launchd, an editor's task runner) are refused (cost 4). Recommended: yes, as a net for
   the usual case and said to be no more than that; the fix for Codex is the recipe of
   point 3, not this rule. Without it nothing else changes in the design.
2. **Other accounts and sandboxes** (cost 6). Recommended: not now; the socket stays in the
   home.
3. **Codex**: workers are started with `--no-daemon` by the hand-off recipe; the user's
   Codex config and daemon stay as they are. Recommended: yes.

## 9. As built (u11#1)

Where it is: `crates/workriver/src/caller.rs` (the chain with `libproc` / `procfs`, the
snippet and its parser, the caller socket and its proofs, the pins' file, the rule
`place_of`); `App::who`, `App::place`, `App::one_place`, `App::see_roots`, `App::here` and
`guard` in `serve.rs`; `Runtime::roots` in the core, implemented in `herdr.rs` (with the
refusals' words) and routed per machine in `machines.rs`; `Daemon::send` and
`Daemon::process` in `main.rs`. Tests: `caller::tests`, `herdr::tests`, the last block of
`src/tests.rs` (every line of 3.1's prototype, every row of 3.2, rule 5, the other machine,
a proof that expires under `ask --wait`), and `tests/caller.rs` (the real binary, the real
socket, real processes, a stand-in for Herdr's socket; the u9#2 incident among them).

Where it differs from the text above, or says more:

- `Runtime::roots(machine, place)` answers a place's root together with what is at the
  place (its agent and group), so the usual call costs one look at the runtime; with no
  place it answers every place.
- An expired or unknown proof is answered `401` with the header `x-workriver-caller:
  expired`; the CLI fetches a new proof and repeats the request once.
- `/api/here` from a process in no pane answers `actor`, `name`, `group` and `intent` as
  null rather than an error; the CLI asks it for every command that needs the workspace,
  whether or not its environment names a pane.
- Pane-named agents (`herdr:<pane>`) are pinned like the others. It changes nothing: that
  name can only come from the pane itself.
- On another machine the chain of the process id a request gives is read there at every
  request (one run of the snippet each), so a process that has ended is nobody's at once
  (u11#2 P3-1; it was kept 30 s, which let a program there name the id of a `workriver`
  call that had just ended).
- `workriver attach fetch` takes the stored file's name (`<id>.<ext>`) or a link ending in
  it, and saves it in the current folder.
- A caller on a machine's listener that shows no process is answered 401 (it was 403).
- After the reviews (u11#4):
  - **The page's session** (u11#2 P1-1): the cookie was a random part signed with the token,
    so whoever read the token could make one and be the user, past every rule above. Now it
    is 32 random bytes the service hands out for a one-time code and nothing derives; the
    service keeps only their SHA-256 and the second each ends (`<home>/user-sessions`, mode
    600, written whole and renamed), so sessions outlive a restart and neither the token
    nor that file yields one. Cookies of the old form are refused; the page is opened again
    with `workriver open`. A line written into the file counts after a restart: the same
    class as writing the database (section 6, the files).
  - `GET /api/session` answers `App::user(..).is_ok()` (u11#2 P3-2), so it says the user
    only where every route would take the request for the user.
  - The regular read (every 2 s) asks the runtime for the roots of places that hold an
    agent only (`Runtime::agent_roots`; Herdr: `pane.process_info` for agent panes only),
    as section 5 costs it; so does rule 5's "shown in two places" check (u11#3 P3-2). What
    remains beyond section 5: each tick reads `session.snapshot` twice (once for the agents
    the page shows, once with the roots).
  - On another machine a process outside every place's root is in no place: the terminal
    fallback (the `( cmd & )` case) is looked for on this machine only, so a refusal there
    costs no ssh run per place (u11#3 P3-2; 3.3 names rules 1, 2 and 5 there).
  - The identity code says place, not pane (`place_of`, `App::place`, `App::one_place`), as
    the runtime interface does (u11#3 P3-3); the refusals' words stay in `herdr.rs`.
- Verified on Linux (Debian, `rust:slim` container): the `procfs` chain, the snippet's
  `/proc` branch and the socket's peer pid (`caller::tests`). **Still not verified:** a real
  followed machine over ssh, a Herdr restart against rule 5, tmux inside a pane.

## Appendix: the experiments

Attached: `h.py` (one request to an isolated Herdr socket), `peer.py` (kernel's peer pid,
`lsof` timing), `chain.sh` (the other machine's snippet), `check.py` (the prototype of rules
1 to 4), `probe.sh` and `probe3.sh` (what a harness's command sees and can reach), `linux.sh`
(Linux facts), `kp.rs` / `lp.rs` / `si.rs` (the chain with libproc's two calls and with
sysinfo), `pf.rs` / `hand.rs` (the chain on Linux with procfs and by hand), `probe3-*.out`
(the Codex samples).

- Isolated Herdr: `herdr --session wr-u10 server` (round 1) and `wr-u10b` (this round);
  both stopped and deleted (`herdr session list` shows `default` only).
- Temporary service: `WORKRIVER_HOME=<scratch>/home workriver serve --port 47931` (round 1);
  stopped. None was needed this round.
- Two throwaway Unix sockets for the sandbox sample (`/tmp/wr-u10.sock` and one in this
  task's attachment folder); removed.
- Real Herdr: only `session.snapshot`, `pane.process_info`, `agent list`, `pane list`,
  `status` were called. Real service on 47820: `task show`, `board`, `task start` and
  `deliver` for these tasks, `status`, `machine list`, and `GET /api/status` from the
  sandbox probe.
- Libraries: built in a scratch folder with its own `CARGO_HOME`; Linux in `docker run --rm
  rust:slim`, `debian:stable-slim`, `python:3-slim`.
- Codex: `codex`, `codex --no-daemon`, `codex -c model_reasoning_effort="low"` in panes of
  the isolated Herdr; `!` commands, and three model turns that ran the probe.

Sources for Codex beyond its own `--help`: the CLI reference
(https://learn.chatgpt.com/docs/cli/reference.md) and, for "the daemon keeps the environment
of whichever session started it", https://www.developersdigest.tech/blog/codex-cli-background-daemon-auto-start-disable-2026
(a third party; confirmed here by the daemon's own environment).
