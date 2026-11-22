# The manager's seat

**When:** you are asked to manage a workspace, were just told you manage one, your seat
was refused from a new terminal, or you take over from another manager.

A workspace is workriver's own (`ws1`: an id, a name that is only shown, one project). It
has one seat. **Only the user seats a manager**; no agent does, itself included, and
`workriver manager here` is gone. Give the user one of:

- on the page: the workspace's line in the left rail, "Manager", then your name;
- from the user's own terminal (a new window, or a Herdr pane with no agent; `!` in your
  session is yours and is refused):

      workriver workspace list
      workriver workspace manager ws1 <your name or session>

Once seated you read in your pane `[workriver] The user made you the manager of workspace
ws1 «…» of project «…» … Active there: i3 «…»`. Then `workriver guide manager` and
`workriver board`. Where your pane sits in Herdr decides nothing: a Herdr workspace is not
a workriver workspace.

**What the seat is.** Your session, its machine and the folder you were started in, and
the *appointment*: this seating. One session holds one seat (seated elsewhere, you leave
the first). What the user sent to, or was asked by, an earlier appointment is not yours: a
message it never got is recorded as not delivered and is not passed on (the user writes
again); its question with options can no longer be answered (read the conversation and ask
anew with `workriver say --option …`); `workriver reply` to its message is refused.

**Your terminal closed, or Herdr restarted.** Resume the same session in the same folder
on the same machine: you are the manager again by yourself, with your appointment, and the
workspace's conversation says so. Anything else (a new session, another folder or
machine) is not the manager until the user seats it: Needs you shows the workspace with
that agent proposed, one click. Until then `workriver` from that pane is refused, saying
so; tell the user in your pane, do not try another way.

**Replaced.** You read `[workriver] You are no longer the manager of workspace ws1 …`:
stop managing its intents. Write down what workriver does not hold for the next manager
(the intent's note, or your shared memory) if you have not.

**What you read.** Your workspace's conversation with the user, earlier managers'
messages included, and of a message to every manager your own copy and replies. Other
workspaces' conversations are not shown to you. Intents, units, tasks, findings and
questions of every workspace stay readable.

**Which workspace is yours** when a command leaves the intent out: the one you hold the
seat of; for a worker, the one workspace of the tasks it has running. A worker with tasks
in two workspaces names the intent.

**Refusals:** "only the user does this (workspace.manager_set)" - ask the user. "the seat
changed: ws1's manager is … now" - the user looks again and chooses. "session … holds a
manager's seat, given to it in …; pane … shows it in …" - a new terminal is the manager
only once the user confirms it. "no running agent …" / "several running agents are …" -
the user names a running agent, by session when names repeat. "… is shown in no terminal"
/ "in several terminals at once" - one session, one terminal: close the others. "ws1 has
an active intent" - a workspace is closed only once its intents are done, shelved or
moved.
