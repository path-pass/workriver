# A unit's branch was renamed or rebased onto a new branch

**When:** the work now lives on another branch (every rebase a new branch, no force-push),
base, path or machine. It is the same unit: **never create a second unit for the same work.**

    workriver unit move u1 --branch feat/queue-2 [--base main-2] [--repo PATH] [--machine box|local]
    workriver board u1

What you do not give stays as it is.

**You will see:** `u1 is now …`; the board says "previously on feat/queue (from main), 3 commits
seen there". Tasks, findings and questions stay. Commits are read from the new branch and
keep their task by the `Task:` trailer or their link. A task whose commits were only on the
old branch shows "commits not on the current branch" in the browser. A merge gate asked
before the move is out of date: ask again and drop the old one (`ask-user`).

**Refusals:** "only the manager of intent i3 (name) or the user moves its units" - a worker
tells its manager the new branch. "base … is not a commit or branch in …". "a unit's branch
starts with feat/, chore/ or fix/" - a new name follows the rule (a unit named before it
keeps its name while only its base, path or machine moves).
