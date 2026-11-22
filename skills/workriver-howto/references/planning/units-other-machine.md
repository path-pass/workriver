# Units when the workers run on another machine

**When:** you are on one machine and the work happens on another.

    workriver machine list                 # is that machine followed, and connected?
    workriver unit add --title "Queue" --machine box --repo /home/me/app --branch feat/queue [--base main]
    workriver board u1                     # machine, path and branch as the worker sees them

Give `--repo` and `--branch` as they are **on that machine**: never a path or ref that
exists only on yours (a relay remote's `hub/feat/queue`); the worker cannot check it out.

As the intent's manager you put its units on any machine workriver follows (following one
is the user's: `machines`). There is nothing to link: a workspace has no machine, and a
worker over there is in your workspace through the tasks you give it. A worker adds a unit
only on its own machine.

**You will see:** `u1`; on the board `[on box: /home/me/app, branch feat/queue from main]`.

**Refusals:** "only the user or the manager of intent i3 puts its units on another
machine" - you are not its manager. "machine box cannot be reached now; see `workriver
machine list`". "--repo is a path on box: give it in full".

Recorded in the wrong place already: `branch-moved`.
