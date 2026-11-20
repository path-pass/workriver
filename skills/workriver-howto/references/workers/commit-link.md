# A commit without its `Task:` trailer

**When:** a commit on the unit's branch shows as unregistered (a gap), or names the wrong
task. When committing, prefer the trailer (`git commit -m "…" -m "Task: u1#3"`), in the
message's last paragraph with any other trailers (`Co-Authored-By: …`) and no blank line
between them: git reads trailers only from the last paragraph. A commit already made is
linked. History is not rewritten.

    workriver commit link 3f2a9c1 u1#3
    workriver commit unlink 3f2a9c1        # take it back

By the task's agent, the intent's manager, or the user (who can also pick the task on the
unregistered node in the browser).

**You will see:** `3f2a9c1… is now part of u1#3`. The link wins over a trailer and follows
the commit through a rebase (matched by patch id).

**Refusals:** "commit … is not one of unit u1's: not on branch … since …" - after a rebase,
first `workriver unit move`. "no commit … in …" - check the hash on the unit's machine.
