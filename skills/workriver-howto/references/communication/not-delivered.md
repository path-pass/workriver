# A decision or result was not delivered

**When:** the page shows "Not delivered" under a decision or gate result, or an expected
`[workriver]` message did not come.

    workriver status                          # "agent runtime: connected"?
    workriver board i3                        # who is the manager
    workriver manager here                    # in the manager's pane: registers (again)

Why, as the page says it:

- *the intent has no manager* - nobody registered for the workspace the intent runs in.
- *the manager is not running* - start it again, then `workriver manager here`.
- *Herdr is not reachable* - the service must run where it reaches Herdr's socket.
- *Herdr refused it* - the agent was starting, or waits at a question or approval in its
  terminal: sort that out there.

Nothing is lost: the decision is recorded either way and `workriver board` shows it; the
user can paste the message by hand with "Copy message" on the page.
