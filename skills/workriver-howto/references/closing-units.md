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

**Refusals:** "only the manager of intent i3 (name) or the user closes its units".
"unit u1 is already merged".
