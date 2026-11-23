# Closing units and the intent

**When:** a unit's work is through, or the board shows it `ready`.

A unit stands (board, `workriver task show`, the page):

- **working**: a task is to do or being done. A step its template placed (`templates`) is
  a task to do until it is done or dropped: a unit with one is never ready.
- **waiting**: an open finding, a gate for the user, or a finding accepted with no work
  handed out for it. A review delivered last is not the end.
- **ready**: everything delivered and decided (deferred findings hold nothing up): it can
  be merged.
- **merged**: git shows its branch in its base (fast-forward or merge commit); workriver
  records this itself, with the unit's commits.
- **done**: closed without a merge. A task added to a done unit opens it again (its state is
  worked out from its tasks once more); make its worktree again if it was removed
  (`git -C <repo> worktree add <repo>.worktrees/<unit> <branch>`).

Once a unit is merged or done, remove its worktree after checking nothing is left
uncommitted there (`git -C <repo>.worktrees/<unit> status`): `git -C <repo> worktree remove
<repo>.worktrees/<unit>`.

A research unit (`units`) has no branch: it goes from ready ("ready to close") to done with
`workriver unit done`; it is never merged.

    workriver unit merged u1                                # after a squash merge (git cannot show it)
    workriver unit done u2 --reason "superseded by u1"      # research, abandoned, superseded
    workriver intent done i3                                # once the board says every unit is merged or done

Closing a unit over steps its template placed that are neither done nor dropped is not
refused; `unit done` answers "Steps of template … not done: …; u2 is closed without them",
and they stay `todo` on the board. Drop each with its reason first, so the record says why
(`templates`).

**Tell the user** what came of it each time, by the rule of `user-message`: work the user
asked for in a message (`m3`, also one to every manager) is answered there, as a reply to
it; work nobody asked for is `workriver say`. When a unit is merged or done, or the user
marks the intent done, your message ends with "Tell the user what was merged and what they
should do now, if anything (restart a service, re-open the page, update other machines): if
the user asked for this work in a message, reply to that message with `workriver reply
<message> --text "..."`; otherwise with `workriver say --text "..."`."; when you run
`workriver intent done`, its output says it after `i3 done`. One line: what changed for the
user and what they must do.

    workriver reply m3 --text "u1 is merged: replies show in the page. Restart workriver serve and re-open the page."   # m3 asked for it
    workriver say --text "u1 is merged: replies show in the page. Restart workriver serve and re-open the page."        # nobody asked

A squash merge leaves the branch's commits off the base, so git cannot show it; workriver
notices and tells you, once per branch tip:

- `[workriver] u2 (feat/x) looks merged into main by squash: every change of it is there
  (commit abc123 matches)`. Every file the unit changed is the same in the base (or its
  upstream) as on its branch. Check that commit (`git show abc123 --stat`); if it is the
  unit's merge, `workriver unit merged u2`; if not (the same change landed some other way, or
  the branch is to be merged later), ignore it.
- `[workriver] u2 (feat/x) has been ready to merge for 24 h and git shows no merge`: once per
  ready period (24 h by default; the user sets `readyHours` in settings.json). Check its pull
  request (`push-pr-merge`): merged, `workriver unit merged u2`; still open, nothing to do;
  abandoned, `workriver unit done u2 --reason "…"`.

The page shows "Looks squash-merged" next to "Ready to merge". With no manager, the user sees
both in Needs you instead.

**Refusals:** "only the manager of intent i3 (name) or the user closes its units".
"unit u1 is already merged". "u2 is a research unit: …" - `unit done`, not `unit merged`.
