---
name: workriver-howto
description: Exact commands for a situation under workriver - starting an intent, units on another machine, a renamed or rebased branch, a commit without its trailer, design and decide gates, asking the user to decide (merge, deploy), blind reviews, adding to a decided finding, deciding findings, deferred batches, gates and chores, attachments, asking another task, remote machines, a message that was not delivered, a message from the user to reply to, a worker that stopped without delivering, closing idle workers, updating. Use when a workriver manager or worker needs to know how to do one specific thing, or when a workriver command was refused.
---

# workriver: how to

Short recipes, one per situation. Read only the one you need:

    workriver guide how            # this index
    workriver guide how <topic>    # one recipe

The text printed by `workriver guide …` matches the installed workriver; prefer it over a
copy of this file.

| topic | when |
|---|---|
| `start-intent` | starting a goal from the project's backlog |
| `units` | splitting an intent into units; research and design that belong to no unit |
| `hand-off` | giving a task to a worker agent |
| `units-other-machine` | the workers run on another machine than you |
| `branch-moved` | a unit's branch was renamed, or rebased onto a new branch |
| `commit-link` | a commit was made without its `Task:` trailer |
| `design-gate` | the user has choices to make before implementation |
| `push-pr-merge` | a ready unit is pushed, gets a pull request, is merged: three tasks |
| `ask-user` | the user must decide: merging, deploying, changing the goal, a repeated failure; asking again (dropping a task no longer needed) |
| `contract` | two tasks (front and back end) must fit together |
| `blind-review` | a review that must not be anchored by others' accounts |
| `finding-additions` | a reviewer adds to a finding, or has more findings, after delivery |
| `deciding-findings` | who decides findings, and how |
| `closing-units` | what working / waiting / ready / merged / done mean; closing units and the intent |
| `closing-workers` | closing worker agents you started once they have nothing to do |
| `deferred-batches` | deferred findings, and "do together" batches |
| `gates-chores` | steps only the user can do; things the user should do sometime |
| `attachments` | screenshots and reports |
| `ask` | asking about another task |
| `machines` | other machines and linked workspaces (the user's to set up) |
| `not-delivered` | a decision or result did not reach an agent |
| `user-message` | the user wrote to you from the page (`message m3`); replying with `workriver reply` |
| `stalled` | a worker stopped without delivering, or is gone |
| `updating` | after workriver was updated |

What changed lately in workriver's behaviour: `workriver guide changes`.
