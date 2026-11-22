# A workspace key of workriver's own

Design, not built yet. It replaces "a workspace's record is found by the runtime's id, else
by its label" (`workspace_at`, `workspace_mut`, `not_another_open`) and every use of a label
as a key (conversations, read marks, who answers a question, links). Before the branch is
merged its rules go into `docs/security.md`, `caller-identity.md`, `layout.md`, `guards.md`
and `docs/runtimes/herdr.md`, and this file is removed.

## 0. What the runtime gives (probed, Herdr 0.9.5, an isolated server)

| Fact | Consequence |
| --- | --- |
| A workspace keeps its id over a server stop and start (restored from the session file). | "The id changed at restart" is not the normal case; no automatic re-matching is needed for it. |
| An id is never reused while one server runs. | While readings go on without a gap, an id is one workspace. |
| After a restart the next new workspace takes highest id + 1: a closed newest workspace's id is given again. With the session file lost, ids start again at the first. | **An id is not an identity either.** Today the reuser is matched to the closed workspace's record by id, whatever its label. |
| Two open workspaces may share a label; any program renames, creates and closes workspaces through the socket. | A label proves nothing. |
| While stopping or restoring, the server answers with the whole list or not at all (533 answered reads during 3 stops, 174 during a start: never a short list). | A workspace missing from an answered list is closed. |
| Workspace metadata tokens are display-only and written by any program. | The key cannot be kept in the runtime. |

## 1. The key

- **What.** `ws<N>`, N counting the records in the order they are made (as `i<N>`, `p<N>`).
  It never changes and is never given again. A record is never removed or merged.
- **Where.** On the record (`Workspace.key`), in workriver's store only. The record also has
  its *binding*: the machine and runtime id it was last seen under, the label last seen, and
  whether it is open. Label and runtime id are shown; nothing is found by them but the
  binding below.
- **Everything names the key.** An intent's workspace, a message's conversation, a read
  mark, a link, a manager's seat: all hold a key. A new event carries the key beside the
  group as it was seen (the label there is only what it was called then); `apply` of an
  event with a key goes by the key and matches nothing.
- **When given: first sight.** The service records `workspace.seen {key, group}`
  (service-only) when an answered reading of a machine's groups has a (machine, id) no open
  record is bound to: at the poll, and in `App::record_in` under the store's lock when an
  event's group has no record yet (the caller's group from `App::who_in`, or the one the
  user named), so every event finds its key. `workspace.seen` again with the same key
  records a new label (a rename). `workspace.closed {key}` (service-only) is recorded when
  an answered reading of the record's machine no longer lists its id. A reading that could
  not be made (error, machine unreachable) records nothing.
- **Matching a group to a record: by binding only.**

  | The group's (machine, id) is … | … and | It is |
  | --- | --- | --- |
  | an open record's binding | readings of that machine had no gap since it was last seen | that record (another label: a rename) |
  | an open record's binding | there was a gap (the service started, or a reading failed) and the label is the same | that record |
  | an open record's binding | there was a gap and the label differs | not that record: it is closed, the group gets a new key (id given again, or a rename nobody saw: the user joins it, below) |
  | no open record's binding | | a new key |

  **Nothing matches by label alone, for anyone, ever**; nor across machines; nor to a closed
  record. `not_another_open` and its 409 ("two open workspaces have one name") go.
- **Who may claim a closed record: the user only**, by a join (section 3). Not the manager
  registering again: a session whose terminal is gone can be taken (a limit
  `caller-identity.md` already states), and a closed workspace is exactly that case, so a
  claim by session would hand the record to whoever reports the session. The page proposes
  the likely record (same machine, same label); the user confirms.

## 2. Same names

- **On other machines:** each (machine, id) has its own key. Never one record unless linked.
- **Two open here:** two keys, two managers, two conversations. Nothing is refused.
- **A closed one's name used again:** a new key. The closed record keeps its intents,
  conversation and (not running) manager; the page shows it as closed. The user continues it
  in the new workspace on purpose (a join), or moves single intents (`intent.linked`).

## 3. Joins (links)

One event, the user's only: `workspace.linked {from: key, into: key}`; undone by
`workspace.unlinked {from}`. It says: *from now on, `from` is part of `into`*. Two shapes,
anything else refused:

1. **Link:** `from` is on another machine, `into` on this one.
2. **Continue:** both on one machine, `into` is closed, `from` is open.

One level: `into` is not itself joined, `from` has nothing joined into it, a key is joined
once. The CLI and page name workspaces by key; a name is accepted only when exactly one
open workspace of that machine has it in a reading made at the request (else refused with
the keys to choose from).

- **Resolved once, when recorded.** `home(k)` = what `k` is joined into now, else `k`. `apply`
  writes what an event sets to `home` of the key it carries: the manager's seat, the current
  intent, the project, an intent's workspace, a message's conversation. Reads never resolve
  a link again, except to say where a running agent or an open group is now.
- **Follows the link** (while it stands): the manager (one for the whole; it may run in
  either), where new work goes, the project, intents started from either, the conversation
  with its questions and read marks, placing units on the other machine (`may_place`).
- **Does not follow:** what `from` recorded before the join stays `from`'s own (its intents,
  conversation, seat), listed apart on the page; `from`'s binding, label and machine; a
  later workspace of the same name (it is another key: the user links it again, also after
  the remote one was closed and made again).
- **Unlinking** moves nothing back: what was recorded in `into` stays there. If `into`'s
  manager registered from `from` (the record keeps the key the registration carried), the
  seat is emptied.

## 4. What changes over to the key

| Today (label or runtime group) | With keys |
| --- | --- |
| `Intent.workspace`: a group, rewritten by `workspace_mut` to follow the record | the key; no rewriting. `manager_of`, `intent_workspace`, `workspace_project`, `same_group` compare keys. |
| An agent's registration or start: the caller's group, matched by id, else label | the key of the caller's group (`App::who_in` → binding → `home`). The ownership rules stay as they are, read through keys; a body naming another workspace is refused as now. |
| `message.sent` / `message.said` name a label; `said` takes the first record the actor manages | the key: the page sends it; `say` is the caller's workspace and the caller must be its manager. Each copy of a message to every manager names its key. |
| `messages.read {label}`; `State.read` by label | by key. The conversation with every manager keeps its empty name. |
| `manages(label, session)`, `choice_open`, routing an answer | the question's key: open while not answered and its asker is that record's manager. A rename changes nothing. |
| An agent reads one message if its group's record is the message's | `home(caller's key)` is the message's key, or the message went to it. |
| Links by two labels, resolved at every read | by two keys, resolved when recorded (section 3). |
| `/api/intent/start`, `workspace.project_set` name a runtime id | a key; the service checks in a reading made at the request that it is open. |
| The page's Manager select: `w:<label>`, same labels folded into one entry | one entry per key: open workspaces, then keys that have messages but are closed or joined away, marked so. Same labels are told apart by machine, then by key. Settings lists closed records that have active intents with "continue in …", and the links by key. |

**Old events are never rewritten and replay deterministically.** An event without a key is
applied by today's matching (id, else label; a linked group of another machine by its link's
labels), kept under a name of its own, used for nothing else and never changed again
(a guard: a fixture of old events gives fixed keys). Each record it makes takes the next key.
The service writes every new event, so no keyless event follows a keyed one.

What replay can differ in from today's state:

1. **Shape** (the replay guard's expected state is blessed again once): records gain key,
   open; intents and messages hold a key; read marks by key beside the old ones by label.
2. **Matched once, at the event, not again at every read.** An old intent or message stays
   in the record its own event found. Today a later event can change what its stored group
   matches (a link made or removed after an intent started in the remote group; a later
   record with its id or label). Only those differ; the implementer lists each from the
   copied-database replay (run with the user's say-so) against today's answer.
3. **Old records merged by label stay one record.** History is not split.
4. **Old read marks** (by label) keep their effect on old messages: unread = written after
   both the key's mark and, for a keyless message, its label's mark. Same-label old
   conversations stay read together; new ones never are.
5. **An old question** is open by its record's manager: one that a rename closed opens again.
   An old message whose record cannot be found has no conversation key: it is shown under
   its label as closed, and its choice is closed.
6. **Old links (by label) do nothing for new events.** The page and `workspace list` show
   each as "linked by name before: confirm", preset when exactly one workspace matches on
   each side; one confirmation by the user records the keyed link. *Open choice for the
   user's gate:* the alternative is that the service converts them itself at the upgrade
   (one last match by name). Recommended: confirm by hand; it is once, and a link is then
   never a side effect of a name.
7. **At the upgrade** every old record's binding is checked by the gap rule: same id and
   label stays; a record whose label is stale (renamed with nothing recorded since) is
   closed and the user continues it.

## 5. Security

An agent can no longer, by naming, creating or renaming a workspace through the runtime:

- be matched to another workspace's record: take a closed workspace's manager seat and
  intents, or the target of a link;
- read another workspace's conversation, share its read marks, or have an answer routed by
  a name;
- close the user's unanswered question by a rename, or block another workspace's
  registration by opening one with its name.

What stays a limit (the runtime's interface is the user's account's):

- **An agent can still rename or close a workspace through the socket.** A rename now only
  changes what is shown: the page can show two workspaces with one name (told apart by
  machine and key), and the user may pick the wrong one for a join or a start. Closing ends
  its agents and closes its record; nobody else gets it.
- **Bait for a join:** an agent makes a workspace with a closed one's name (or, on another
  machine, the name the user expects) and waits for the user to continue or link it. The
  join is the user's; the page shows the machine, key and agents of both sides before it.
- **An id given again inside a gap, under the same label** (the newest workspace closed, the
  runtime restarted, a new one made with its name, no reading in between) is taken for the
  same workspace. Restarting the runtime ends every agent in it.
- A rename nobody saw (during a gap) closes the record: work stops until the user joins it;
  nothing is taken.
- A machine is the label the user gave it; a session can be taken once its terminal is gone
  (both as today).

## 6. Not done

- Splitting records that old events merged by label.
- A key kept in the runtime (only display tokens any program writes), or a runtime "life"
  id to tell a rename from an id given again without the label.
- Moving a workspace's history into another by a join, or undoing it by an unlink.
- A claim by the manager's session (turned down, section 1).
- Machines known by anything but their label; agents outside the runtime.

## 7. Tests the implementation must pass

The three review reproductions (same-label open workspaces: the second reads nothing of the
first; a rename keeps a question open; read marks per conversation); the earlier ownership
probes, with the 409 case now two records; id given again after a gap with another label
(new key) and without a gap (rename); closed then same name (new key, nothing follows);
link and continue by key, both refusals of shape, unlink empties a seat taken from `from`;
a remote workspace made again is not linked; an unanswered reading records nothing; the
frozen replay of old events; old read marks; the route-access guard for the message read.
