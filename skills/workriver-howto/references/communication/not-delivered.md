# A decision or result was not delivered

**When:** the page shows "Not delivered" under a decision or gate result, or an expected
`[workriver]` message did not come.

    workriver status                          # "agent runtime: connected"?
    workriver board i3                        # who is the manager
    workriver workspace list                  # each workspace's manager, and whether it runs

Why, as the page says it:

- *the intent has no manager* - its workspace's seat is empty (or it runs in none): the
  user seats one (`seat`).
- *the manager is not running* - resumed where it ran (the same session, machine and
  folder), it is the manager again by itself; else the user seats an agent (`seat`).
- *the seat was given anew* - the message was for an earlier manager of the workspace and
  is never passed on to the next: the user writes again.
- *Herdr is not reachable* - the service must run where it reaches Herdr's socket.
- *Herdr refused it* - the agent was starting, or waits at a question or approval in its
  terminal: sort that out there.

Nothing is lost: the decision is recorded either way and `workriver board` shows it; the
user can paste the message by hand with "Copy message" on the page.
