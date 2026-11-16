# Units when the workers run on another machine

**When:** you are on one machine and the work happens on another.

    workriver workspace list               # is a workspace there linked to yours?
    workriver unit add --title "Queue" --machine box --repo /home/me/app --branch feat/queue [--base main]
    workriver board u1                     # machine, path and branch as the worker sees them

Give `--repo` and `--branch` as they are **on that machine**: never a path or ref that
exists only on yours (a relay remote's `hub/feat/queue`); the worker cannot check it out.

**You will see:** `u1`; on the board `[on box: /home/me/app, branch feat/queue from main]`.

**Refusals:** "no workspace on box is linked with this intent's workspace" - the user
links one (`machines`). "machine box cannot be reached now; see `workriver machine
list`". "--repo is a path on box: give it in full".

Recorded in the wrong place already: `branch-moved`.
