---
name: workriver-howto
description: Exact commands for a situation under workriver - starting an intent, a unit's template and the steps it places (filling one, leaving one out), a spike (an experiment kept as code, never merged), units on another machine, a renamed or rebased branch, a commit without its trailer, design and decide gates, asking the user to decide (merge, deploy), blind reviews, architecture reviews (code organization and solution architecture against the project's own rules), adding to a decided finding, deciding findings, deferred batches, gates and chores, a worker stuck on what only the user can do (a blocker), reaching the manager, attachments, asking another task, remote machines, the manager's seat (who makes an agent a workspace's manager, a manager's new terminal, taking over), a message that was not delivered, a message from the user to reply to, the user discussing a delivered research or design result with its agent, a worker that stopped without delivering, closing idle workers, updating. Use when a workriver manager or worker needs to know how to do one specific thing, or when a workriver command was refused.
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
| `units` | splitting an intent into units; research and design for the whole intent (a research unit) |
| `spike` | finding out by trying (measure, compare, prototype), keeping the code: a spike unit, never merged |
| `templates` | a unit's steps: choosing its template, filling a step it placed, leaving one out with the reason; "Steps of template … not done" |
| `hand-off` | giving a task to a worker agent |
| `units-other-machine` | the workers run on another machine than you |
| `branch-moved` | a unit's branch was renamed, or rebased onto a new branch |
| `commit-link` | a commit was made without its `Task:` trailer |
| `design-gate` | the user has choices to make before implementation |
| `push-pr-merge` | a ready unit is pushed, gets a pull request, is merged: three tasks |
| `ask-user` | the user must decide: merging, deploying, changing the goal, a repeated failure; asking again (dropping a task no longer needed) |
| `contract` | two tasks (front and back end) must fit together |
| `blind-review` | a review that must not be anchored by others' accounts |
| `architecture-review` | reviewing a design's solution or an implementation's organization against the project's own rules; the review-fix loop, at most 3 rounds |
| `finding-additions` | a reviewer adds to a finding, or has more findings, after delivery |
| `deciding-findings` | who decides findings, and how |
| `closing-units` | what working / waiting / ready / merged / done mean; closing units and the intent |
| `closing-workers` | closing worker agents you started once they have nothing to do |
| `deferred-batches` | deferred findings, and "do together" batches |
| `gates-chores` | steps only the user can do; things the user should do sometime |
| `blocked` | stuck on what only the user can do (a refused command, a missing permission): a blocker; reaching your manager (`manager tell`) |
| `attachments` | screenshots and reports |
| `ask` | asking about another task |
| `machines` | other machines (the user's to set up) |
| `seat` | the manager's seat: being made a workspace's manager (the user's act), a new terminal, taking over, what a seat reads |
| `not-delivered` | a decision or result did not reach an agent |
| `discussion` | the user discusses your delivered research or design result with you; answering, amending at the conclusion |
| `user-message` | the user wrote to you from the page (`message m3`); replying with `workriver reply`; telling the user something unasked with `workriver say` |
| `stalled` | a worker stopped without delivering, or is gone |
| `updating` | after workriver was updated |

What changed lately in workriver's behaviour: `workriver guide changes`.
