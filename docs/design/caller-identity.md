# Caller identity: which place a call truly comes from

The rules by which the service decides who a request acts as. For agents who change
`server/access/`, `server/http/guard.rs`, the CLI's client or the runtime's roots. The
security rule in AGENTS.md is their summary; `docs/security.md` says what they mean for the
user. Code and tests cite the rules by number (1 to 5); keep the numbers.

Why: the service once took the pane a request named (`x-workriver-place`, from the CLI's
`HERDR_PANE_ID`) at its word. A harness that ran its commands in a shared background
service kept the environment of a pane long gone, and with the readable user token its
worker's delivery was recorded as the user's. Naming another agent's pane made a caller that
agent.

## 1. In short

The service reads identity from **the process that calls**, never from what the request
says:

1. The CLI opens a Unix socket of the service first. The kernel tells the service the pid of
   whoever connected (nothing the caller can choose). The service answers with a short-lived
   proof the CLI sends with its HTTP requests.
2. The service reads that process's chain of parents and asks the runtime for the root
   process of each place. The caller is **at the place whose root is in its chain**.
3. The place the CLI names is only cross-checked; a mismatch is refused, naming both.
4. Outside every place a caller is the user only with the token **and** a controlling
   terminal.
5. Which agent a place holds comes from the runtime, which takes it from whoever tells it,
   so a session is **pinned to the root it was first seen at**.
6. On another machine there is no service to connect to: the CLI sends its pid and the
   service checks it on that machine through the runner (weaker; never the user).

## 2. The check on this machine

**Who connected.** The service listens on a Unix socket, `<WORKRIVER_HOME>/caller.sock`
(mode 600; it does not start if the path is longer than the system's limit for socket
paths, 104 bytes on macOS, 108 on Linux, and says so). For each connection it reads the
peer's pid from the kernel (`tokio::net::UnixStream::peer_cred()`: `LOCAL_PEERPID` on macOS,
`SO_PEERCRED` on Linux), reads the process chain at once, and writes back one line: a random
proof (16 bytes, `auth::random_hex`). It keeps `proof -> chain` for 30 seconds. The CLI sends
the proof as `x-workriver-caller` with every request; `guard` puts the chain into `Caller`.
An expired or unknown proof is answered `401` with the header `x-workriver-caller: expired`;
the CLI fetches a new proof and repeats the request once (so `ask --wait`, which outlives a
proof, keeps working). A request without a valid proof has no process: it is nobody (the
page's session cookie is the one other way to be someone, section 3).

**Its chain.** For the pid: itself and its parents up to init, each with parent, controlling
terminal, start time and name; bounded at 64 steps. Read in-process with `libproc`
(`kproc_info_raw`, the `sysctl kern.proc.pid` that `ps` uses) on macOS and `procfs`
(`Process::stat()`) on Linux, behind one function in `common/process.rs`. Turned down:
`sysinfo`, because it gives no controlling terminal (rule 4 and the `( cmd & )` case need it)
and whole seconds for the start time (races, below); a hand-written `/proc` reader, which
would save 85 KB of a 16.3 MB binary (0.5 %); one `ps` per step, 300 to 1300 times slower
(24 ms for 9 steps against 0.08 ms).

**The places' roots.** `Runtime::roots(machine, place)` answers a place's root process
together with what is at the place (its agent and group), so the usual call costs one look
at the runtime; with no place it answers every place. Herdr: `pane.process_info` gives the
shell's pid, or the foreground processes when it knows no shell.

**The one carrier of the named place.** Rule 1 reads the place the request names from the
`x-workriver-place` header and from nowhere else: never from a query or a body. `App::who`
takes everything from `Caller` (the chain and the header). `--as` is a named argument,
checked first (section 3).

**The rule.** `place(P)` = the place whose root is P or one of P's parents; else the place
whose root has the same controlling terminal as P (a job left behind by `( cmd & )` keeps
it); else none.

1. The request names a place and `place(P)` is not that place: **refused**.
2. `place(P)` holds an agent: the caller is **that agent**, token or not, subject to rule 5.
3. `place(P)` holds no agent: **the user** if the request carries the token, else refused.
4. No place: **the user** if it carries the token and P has a controlling terminal; else
   refused. This catches a harness's helper that runs commands plainly; it does not catch
   one that gives its commands a pseudo-terminal (section 6).
5. **One session, one place.** For every agent the runtime reports with a session id of its
   harness, the service keeps the root (pid and start time) of the place it first saw that
   session in, taken in its regular read of the live agents and whenever it looks a place
   up, and kept in `<home>/session-roots.json` so that a restart does not forget. While that
   root runs, the session is that place's only: a caller whose place reports the same
   session from another root is **refused**. A session first seen at two roots in the same
   read is refused at both until one is left. The pin is released when its root no longer
   runs (the pid is gone or has another start time). A place may show a new session at any
   time (the agent started a new conversation): that is a new agent.

   Around it:
   - **An agent without a session id** is named by its place (`herdr:<pane>`, with the
     machine's label on another machine), a name only the runtime implementation makes,
     from the place the agent is in. A *reported* session id of that form is not taken as a
     session: the adapter drops it and names the agent by its own place. Such agents are
     pinned like the others, which changes nothing: being one takes a process in that place.
   - **A place where the runtime knows no shell.** Its root is then its foreground
     processes, which change, so its own session is **not pinned**: another place that
     reports it is seen beside it, and both are refused for as long as both show it. It is
     still found by rules 1 to 4, and its session is still refused while pinned to another
     root that runs. An agent's place whose root cannot be read is listed the same way.
   - **The first-sight question fails closed.** When the runtime cannot be asked whether
     the session shows at another place, the call is refused (503, `cannot_ask`) and nothing
     is pinned.
   - **The regular read** (every tick of the poll) asks the runtime for the roots of places
     that hold an agent only (`Runtime::agent_roots`), on this machine only (section 4).

`App::user`'s "a caller whose place holds an agent is not the user" uses `place(P)` too.

**`/api/here`** ("who and where am I") answers about `place(P)`: the agent there (rule 5
applied), its group and that group's current intent. A caller that names another place gets
rule 1's refusal; a caller in no place gets `actor`, `name`, `group` and `intent` as null.
It is answerable without the token, but not without a proof.

**The cases** (held by the tests in `server/tests/access.rs`):

| Caller | Named place | Token | Result |
|---|---|---|---|
| a child of a plain place's shell | that place | yes | the user (rule 3) |
| the same | that place | no | refused: no agent there and no user token |
| a grandchild of an agent place's shell | that place | yes | that agent (rule 2) |
| in an agent's place | another agent's place | — | refused, naming both (rule 1) |
| in an agent's place | none | yes | that agent |
| in an agent's place | a place that is gone | yes | refused (rule 1) |
| a shell in no place of this runtime | some place | yes | refused (rule 1) |
| an agent's tool shell in no place, no terminal | none | yes | refused (rule 4) |
| a harness's background service, no place, no terminal | a stale place | yes | refused (rule 1) |
| the same | none | yes | refused (rule 4) |
| an orphan `( cmd & )` from a plain place | none | yes | the user (same terminal) |
| an orphan that left its session (`setsid`) | none | yes | refused (rule 4) |
| a process that is gone | — | — | refused: no proof |
| the user's shell outside the runtime, with a terminal | none | yes | the user (rule 4) |
| the same | none | no | refused |
| a place reporting a session pinned to another root that runs | — | — | refused (rule 5) |
| the pinned root gone; another place reports the session | — | — | that agent (section 6) |

## 3. Every caller

| Caller | How it is known | If the check fails |
|---|---|---|
| An agent in its place | proof -> chain reaches the place's root; the place holds an agent (rule 2); its session is pinned to this root or to none that runs (rule 5) | another place named: the command says it runs at X, but its process A > B > C runs at Y / at no place; workriver takes the agent from the place the command really runs at (`workriver guide how hand-off`). Session held elsewhere: session S was first seen at X, whose shell still runs; close that place, or start a new session here |
| The user at a place with no agent | chain reaches a place with no agent, and the token (rule 3) | no token: no agent there |
| The user in a terminal outside the runtime | no place, the token, a controlling terminal (rule 4) | no terminal: in no place and no terminal, workriver cannot tell whose command this is (the user: a terminal window or the page; an agent: its own place, see `hand-off`) |
| The page | the session cookie, from our origin | not the user |
| A request with the token but no proof (`curl`, an old CLI) | nobody: the token alone does not make the user | this request does not show which process sends it: use the `workriver` CLI of this version, or the page |
| An agent on another machine | section 4 | its process is not at place X on machine M; never the user (`machine_router`, no token there) |
| `--as <agent>` | only when the service was started with `WORKRIVER_ALLOW_AS=1` | --as is off |

No refusal names a harness or a harness's flag; they point at the `hand-off` recipe, where
what a worker of each harness must do is written. The refusals' words that name the runtime
are in `runtime/herdr/words.rs`.

**`WORKRIVER_ALLOW_AS=1`** does exactly two things, for tests and development: (a) `--as
<agent>` names an agent without any process check, never the user; (b) rule 4 does not ask
for a terminal (a test run has none). Everything else stays on: the proof, rules 1 to 3 and
5, the token for the user. The service says so at start. In-process tests give the service
its process reader as they give it a runtime: `Options` carries `procs` and `callers`; the
CLI's `Daemon` has a `prove` field (`from_socket` for the CLI, a made-up process at its place
for the in-process tests); those tests run the real `Daemon::process` and `Daemon::send`.

**The page's session.** The cookie is 32 random bytes the service hands out for a one-time
code (`workriver open`) and nothing derives from the token; the service keeps only their
SHA-256 and the second each ends (`<home>/user-sessions`, mode 600, written whole and
renamed), so sessions outlive a restart and neither the token nor that file yields one.
`GET /api/session` answers `App::user(..).is_ok()`: it says the user only where every route
would take the request for the user.

Never the user and never another agent on a failure of rules 1 to 5: every failing branch
returns an error; none falls through to `App::user`.

## 4. Another machine

No socket of the kernel's can vouch there. The CLI sends its own pid (`x-workriver-pid`);
the machine's listener (and only it) takes it. At every request the service reads the
chain on that machine through its runner with one shell snippet (`/proc/<pid>/stat` where
there is `/proc`, else `ps -o pid=,ppid=,tty=,lstart=,comm= -p <pid>` per step; in
`common/process.rs`), so a process that has ended is nobody's at once; reads the place's
root from that machine's runtime; and applies rules 1, 2 and 5 (the pinned root carries its
machine). The terminal fallback (`( cmd & )`) is looked for on this machine only. Two more
conditions, because the pid is the caller's word: the process must be alive and be named
`workriver`. A caller there that shows no process is answered 401. Anything else is refused.

Rule 5 there costs no call of its own: the service does not read that machine's roots on
its tick. A session of that machine is pinned when its first call arrives, with the root
taken from the caller's own chain; whether a pinned root still runs is asked only when
another root there claims the same session. So there the pin goes to the first place that
*calls* with a session, not the first that shows it.

Stored files are fetched there with `workriver attach fetch <id.ext or link>` (it sends its
pid like every other call and saves into the current folder), never with a link a bare
`curl` could use.

## 5. Races

- **The process exits.** The CLI waits on the socket for the proof, so the chain is read
  while the peer is connected. A peer that vanished gets no proof.
- **The service answers before the CLI reads.** The service writes the proof and closes at
  once, so a busy CLI may find its socket closed by the other end. macOS then refuses a
  read timeout on it (`EINVAL`); the CLI reads the proof already waiting, without waiting
  for more. Dropping it there once refused ordinary commands now and then, as requests
  that show no process.
- **Pid reuse.** The chain is read within the accept. The service drops a chain whose first
  process started after the accept. Start times have microseconds on macOS and clock ticks
  on Linux; on another machine's `ps` branch they have seconds, where the pid is the
  caller's word anyway.
- **The proof** is bound to that chain, lives 30 s, is sent over loopback only, and may be
  used for the several requests one command makes. A process that hands its proof to
  another lends its identity; it could as well run the command for it.
- **Parents change while being read.** A parent that exits moves its children to init (or a
  subreaper): a chain can only lose its way to a place, never gain one into another place.
- **A place's root** is read fresh from the runtime on each request, never kept, except the
  pinned (pid, start time) of rule 5, which is compared with the fresh one.
- **First sight of a session.** A session id is a random id of the harness's, so it cannot
  be taken before its agent exists. Between the agent's start and the service's next read
  another place can report the same id: both are then seen together and both are refused,
  never the wrong one accepted.

What the service can read: on macOS `sysctl kern.proc.pid` answers for every account's
processes (`proc_pidinfo` stops at a root-owned parent such as `login`); on Linux
`/proc/<pid>/stat` of another account's process is readable, unless the system is mounted
with `hidepid`, where such callers are refused. Processes in another pid namespace show pid
0 or none: refused.

## 6. What is left

Agents run under the user's account, and this check does not change that.

- **A program that sets out to be the user still can**: read the token, start a process
  that leaves the place (`setsid`, a double fork), give it a terminal of its own (`script`),
  call the socket. What changed is that it takes that: no accident and no single variable
  does it.
- **The runtime's own socket is the account's.** Which agent a place holds is what the
  runtime was told, and any process of the account can tell it for any place. Rule 5
  narrows what that buys; it leaves:
  - **A session whose place is gone can be taken**: its records, its tasks, and a manager's
    right to decide findings. The runtime's socket closes any place, so that takes two
    calls; it cannot be done unseen: the agent's terminal is gone, and the work recorded
    afterwards under its name comes from another place.
  - **On another machine the pin goes to the first place that calls** with a session.
  - **A place with no known shell is not pinned**: its session reported by another place
    gets both refused, which stops that agent rather than replacing it.
  - **An agent can be given another name**: a report for somebody else's place makes it
    show a new session; its next calls are a new agent's and its own tasks refuse it.
  - **A place the runtime knows as an agent only by report can be released** and is then a
    place with no agent: the user, with the token. Workers started as real harnesses are
    not open to this; a harness the runtime cannot tell from its screen would be.
  - **Two places showing a session at first sight** are both refused: a way to stop an
    agent's first calls.
  - **A workspace's record is found by its label once its id is gone**, and the runtime's
    socket renames and closes workspaces. The service records an agent's workspace as the
    runtime names it (never as a request does): the group of the place the caller was
    found at, read with that place (`App::who_in`), on a followed machine too, never the
    one a regular read of the agents last had. It refuses a group whose record is another
    workspace still open (`not_another_open` in `App::record_in`: every event that names a
    group, the user's too). Which workspaces are open is asked of the group's own machine
    at the request (`Runtime::groups_on`): a list that cannot be read is no proof that a
    workspace closed, so the request is refused (503), on this machine while others are
    followed as well, and a followed machine's last regular read is never asked. The check
    and the record are one step under the store's lock, on what is recorded then: two
    requests cannot both pass against a state that has neither's record, and a
    registration that names only an intent is for the workspace the intent is in when it
    is recorded. So two open workspaces that share a label are refused, not matched.

    An intent is in its workspace's record, not under the id or label it was started
    with: when an event that names the workspace moves its record to a new id (the runtime
    restarted) or a new label (it was renamed), the intents that run or ran there are
    rewritten with it (`workspace_mut`; one in a linked group of another machine keeps that
    group). A workspace opened later under the old label is then a workspace of its own,
    and its manager is nobody to those intents. A workspace that got a new id and a new
    label with nothing recorded in between is found by neither: it is a new workspace to
    workriver, and its old record is a closed workspace's (below).

    What is left goes through the socket only:
    - an agent gives its own workspace the label a link names: when that label is next
      recorded, from the runtime, the link resolves to the first local record that has it,
      and the linked workspace's intents are that record's manager's;
    - an agent closes a workspace and gives its own that label: a group with no record of
      its own is then matched to the closed one's record (the restart case, which is what
      the label is for), with its manager's seat and its intents. Any local record is open
      to this, not only one a link names.

    workriver does not see either happen, and no request to workriver does it. The rules in
    core take a workspace as an event names it; only the service knows the runtime.
- **Typing into another terminal.** The runtime lets any agent type into any place: a
  command typed into agent B's place is B's, one typed into a place with no agent is the
  user's.
- **A harness's background service that gives its commands a pseudo-terminal**, started
  outside the runtime or with no place in its environment, passes rule 4 with the token and
  is the user. Rule 4 catches the plain case only. Such a harness's workers must run their
  commands in their own place: the `hand-off` recipe says how to start them (for Codex,
  without its shared background server: `-- --no-daemon`); nothing in the code names a
  harness.
- **An agent with a terminal outside the runtime** (another terminal window, a multiplexer
  started outside it) is the user as far as the service can tell. An agent inside a
  multiplexer of its own within a place descends from that multiplexer's server, not from
  the place: with the place in its environment it is refused (rule 1).
- **The files.** The database, the attachments, the token, `session-roots.json` and
  `user-sessions` in the data home can be read and written from disk without the service.
- **Another machine.** The pid there is the caller's word (section 4).
- **Injecting into a process** of another place (a debugger) makes its calls that place's.
- **`WORKRIVER_ALLOW_AS=1`** lets any local caller with a proof record as any agent by
  name, and drops rule 4's terminal; nothing else.
- **Not verified:** a real followed machine over ssh, a runtime restart against rule 5
  (after one every root is new, so reopened sessions should be pinned afresh), a
  multiplexer inside a place.
