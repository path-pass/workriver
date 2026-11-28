# Hosted agent process

The service starts one `workriver host <dir>` per Codex instance. The internal command
runs before service setup. It uses a single polling loop for the Unix socket and Codex's
stdio, with no database, HTTP listener or watcher. The service starts the host in its own
session with no terminal and redirects its output to `host.log`. The host starts
`codex app-server --listen stdio://` in the registered directory, as the leader of a
different process group. The service can disconnect and connect again without stopping
that process. The internal command uses the same binary as the service.

## Instance files

The service reserves an id and creates `<WORKRIVER_HOME>/agents/<id>/` with mode 700. The
starting `instance.json` contains `id`, `harness: "codex"`, absolute `cwd`, optional
`model`, `auth: "chatgpt" | "api_key"`, `generation` and `state: "starting"`. The host
takes an exclusive, nonblocking flock on `lock` before touching the socket or journal.
It publishes its PID and OS start time in `host`, the child PID in `agentGroup`, and,
once ready, the thread id in `session` with the reported `model`, `approvals`,
`versions` and narrowed `billing`. It writes this metadata by syncing and renaming a
temporary file. The service verifies the process and controls publication. It owns
the 60-second startup deadline measured from reservation and can send `close` while
the instance is still starting.
The host's binary-version probe and pipe writes have their own I/O bounds.

`host.sock` and `host.key` have mode 600. The service writes the key as 32 random bytes
before spawn. A client proves knowledge of it in its first frame by sending its
64-character lowercase hexadecimal encoding. A successful handshake replaces the current
connection and keeps the journal head unchanged. Hosts omit the `attached` entry.
The instance directory and the key keep other local accounts out. Herdr agents running
under the same account can reach host sockets and keys permitted by that account.
Access to that account's Herdr socket also permits reading or typing into its terminals.
[Security under Herdr](../runtimes/herdr.md#security-under-herdr)

## Socket protocol

The host speaks newline-delimited JSON-RPC 2.0 over the socket. Before admitting a
connection to the handshake, it checks the peer PID from the kernel and refuses peers
with a workriver host in their process ancestry or session, or whose identity cannot be
checked. The process check recognizes the installed `workriver` executable and processes
whose executable name matches the receiving host, then checks the invoked subcommand.
Up to four unauthenticated connections can await hello, each for up to two
seconds. New arrivals neither evict pending handshakes nor replace the current client.

A connection begins with `hello {proof, protocolVersion: 1}`. The host refuses an
incompatible version and leaves the current connection in place. Compatible hello reports
`protocolVersion`, `id`, `session`, `generation`, `state`, `head`, `versions`, `model`,
`approvals` and `billing`. A starting host has no session yet. An authenticated client can
repeat hello. JSON-RPC request ids are numbers or short strings.

| Method | Result and effect |
|---|---|
| `status {}` | `{state, generation, head, pending}`. The state is `starting`, `idle`, `working`, `blocked` or `closing`. The entries in `pending` are keyed by journal seq. Each has an ACP projection and a native projection. |
| `read {after, limit}` | `{entries, head}`. Entries have seq greater than `after`, limited by the requested count (default 200, maximum 1000) and a 2 MiB encoded response budget. Continue at the last returned seq, with `head` indicating whether more entries exist. |
| `prompt {generation, session, key, text}` | A known delivery key answers `{seq, repeat: true}`. A different session, future generation or closing instance answers `gone` for a new key. An older generation of the live session answers `moved`. A pending approval answers `blocked`, an active turn `busy`, and an unverified account `billing`. An accepted prompt answers `{seq}`. |
| `answer {generation, request, optionId}` | When the request belongs to this generation and the option is still offered, returns `{seq, replied}` with the decision's seq and local write confirmation. Other keys and options answer `settled`. Once closing it answers `gone`. |
| `cancel {generation}` | Settles pending requests before interrupting the turn. Returns the state `cancelled` for an interrupted turn, `ended` if it ended another way, `idle` if no turn was active, or `sent` after ten seconds. |
| `close {generation}` | Enters `closing` when the host receives the command and immediately returns `{accepted: true}`. The result is the journal's `closed` entry. |

There is one active turn and no prompt queue. A prompt's delivery key and seq are synced
to disk before its native write. A duplicate answers with the original seq even across
generations. This is at-most-once delivery. A crash between the sync and the pipe write can
lose an accepted prompt, and the key still prevents a second write.

## Journal and approvals

`journal.jsonl` holds one JSON entry per line, with continuous `seq`, generation `gen`,
and `at` in RFC 3339. An ACP entry has `kind: "acp"`, `dir: "in" | "out"` and `msg`.
Translated ACP messages use version 1. Native entries have `kind: "native"` and put
retained frames in `msg`, with sensitive account payloads redacted. A host entry has
`kind: "host"` and an `event` naming its receipt, settlements, billing check, usage or
lifecycle transition. Host entries are synced. An interrupted final line is discarded
when opening the journal. History remains on disk after exit or close.
The host indexes one byte offset per 256 lines and scans from the nearest indexed line
when paging. The page budget counts the bytes of the persisted entries.

Ordinary journal entries have a 256 KiB encoded limit. Approval entries can retain up to
1.5 MiB. Oversized entries contain a `cut` marker. Live approvals have a tighter 24 KiB
encoded limit per entry, including their native projection, and the status response is
bounded at 512 KiB. A truncated entry offers Reject only. An approval that cannot fit in
the aggregate budget is cancelled immediately with a `settled` journal entry whose
`by` is `budget`. Every pending request shown is separately answerable by its seq.
The adapter keeps the native request id with its JSON type for the native reply. A reused
native id gets a new journal seq after the previous request is settled.
An active native id belongs to its first request. A malformed approval with a usable,
unclaimed id is recorded and receives the native cancellation or zero-permission reply.
A failed turn cancels any approvals still pending for that turn.

Allow once requires a complete command description or the complete file change. Requests
without a command, requests to grant a root, turn-wide permissions and truncated requests
offer Reject only. Command and file decisions map to `accept`, `decline` and `cancel`.
Permissions requests receive the zero grant `{permissions: {}, scope: "turn"}`.

The host syncs the `decided` entry, writes the reply to Codex's pipe, then syncs the
`replied` entry. A failed decision append sends no reply. After a durable decision, the
host never resends the reply. A missing `replied` means delivery is unconfirmed. The
write may have failed or reached the pipe before the host could sync `replied`.
A `replied` receipt confirms the pipe write, with no guarantee that the harness acted.
The service can reconcile a lost answer receipt by comparing durable decisions to its
recorded answers.

## Closing and process ownership

Only the host signals the app-server group. It observes the child with `waitid` using
`WNOWAIT` prior to its final reap. Holding the original leader prevents reuse of the
group id during signalling. One deadline starts when close is accepted, with a maximum
of 30 seconds. The host settles approvals and waits up to ten seconds for turn cancellation,
then waits up to five seconds each for unsubscribe, EOF, and SIGTERM. SIGKILL gets up to
two seconds. Every phase is clipped to that same deadline. A process that has moved to
another group is outside this close.

`ended` requires an observed leader exit and an empty group after excluding that verified
exited leader. A live unreaped leader remains in the outcome's PID list. If the group is
still occupied or its state cannot be verified, the outcome is `stuck`. The host reaps a
leader only when it has observed exit. It drains pending stdout before the deadline,
syncs one `closed` entry and then updates `instance.json`. A natural exit records
`exited` and leaves the instance stopped.
A fatal host error after the child starts is written to the host log. The host closes
the child's input, attempts to kill its group and waits within the close deadline or a
two-second failure budget. If the journal is writable, it records `exited` with
`reason: "host failure"` and attempts to save the stopped state.

## Codex translation and account check

The adapter uses Codex's stable app-server interface and records the observed binary
version. It projects text, reasoning, plan, tool calls, approvals, usage and compaction
into ACP. The journal also retains native token data with its source. When the app-server
identifies a sub-thread, its thread identity is attached to tool and approval projections
under `_meta.workriver.source`. A tool shell normally descends from the same host.

For each active turn, the adapter retains the most recent valid aggregate
`turn/diff/updated` frame. The host writes it as one native entry when that turn ends.
For the root turn, this precedes the ACP prompt response.

A native frame whose projection fails is recorded with a reason and skipped.
Malformed JSON or oversized lines produce a cut entry with a reason and byte count.
If a malformed frame is identifiable as necessary to finish the active turn, the host
records an ACP prompt error and clears the active turn before accepting another prompt.

The child starts with an environment allow-list, the service's `WORKRIVER_HOME` and
`WORKRIVER_PORT`, and `WORKRIVER_PLACE=acp:<id>/<generation>`. The OpenAI API key is
passed only when explicitly selecting API-key auth. Before starting a thread the adapter
checks Codex's effective config and its raw layers for provider and endpoint overrides.
It requires the built-in OpenAI provider and the selected account kind, and repeats the
account check after `account/updated`. During that check, prompts are refused. Account
receipts record the kind and optional plan, with no email, account id or credentials.

The process-boundary tests in `crates/workriver/tests/hosted.rs` drive the real host over
its Unix socket with `tests/__fixtures__/hosted_app_server.rs`. They validate the journal
and native approval replies against the pinned ACP v1 and Codex 0.172.0 schemas in
`tests/__fixtures__/`.
