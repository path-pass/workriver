# Closing units and the intent

**When:** a unit's work is through, or the board shows it `ready`.

A unit stands (board, `workriver task show`, the page):

- **working**: a task is to do or being done.
- **waiting**: an open finding, a gate for the user, or a finding accepted with no work
  handed out for it. A review delivered last is not the end.
- **ready**: everything delivered and decided (deferred findings hold nothing up): it can
  be merged.
- **merged**: git shows its branch in its base (fast-forward or merge commit); workriver
  records this itself, with the unit's commits.
- **done**: closed without a merge.

    workriver unit merged u1                                # after a squash merge (git cannot show it)
    workriver unit done u2 --reason "superseded by u1"      # research, abandoned, superseded
    workriver intent done i3                                # once the board says every unit is merged or done

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
"unit u1 is already merged".
