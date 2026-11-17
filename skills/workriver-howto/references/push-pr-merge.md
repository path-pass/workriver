# Push, pull request, merge

**When:** a unit is ready (board: `ready`) and its work is to leave the machine.

Three steps, three tasks, in this order, each after the user's go. Never one task doing two
of them: an agent told to push does not also open the pull request or merge.

    workriver task add u2 --kind gate --gate decide --title "Push u2 (feat/readme)?" --after 4 --text "<what is ready, what was checked>"
    workriver task add u2 --title "Push feat/readme" --after 5 --agent <agent> --text "Push the branch only. No pull request, no merge."
    workriver task add u2 --kind gate --gate decide --title "Open the pull request for u2?" --after 6
    workriver task add u2 --title "Open the pull request for feat/readme" --after 7 --agent <agent> --text "Open it (title, description, link it delivers as --note). No merge."
    workriver task add u2 --kind gate --gate decide --title "Merge u2's pull request?" --after 8 --text "<the PR link, its checks, reviews>"
    workriver task add u2 --title "Merge the pull request of u2" --after 9 --agent <agent> --text "Merge it as the user said in the gate. Nothing else."

Add each step when the one before is delivered, not all at once: what the user decides at
one gate (e.g. "squash", "wait for CI") changes the next task's text. A step the user turns
down: drop the tasks after it (`workriver task drop`), with the user's reason. If the user's
note at a gate says to do two steps at once (e.g. "push and open the PR"), that note is the
go for both; still one task per step.

**You will see:** "The user confirmed … in gate …" after each gate; the push and PR tasks
deliver the branch and the PR link as their note; the merge shows up by itself (git sees it,
the unit becomes merged) unless it was a squash merge: then workriver tells you "u2 (feat/readme)
looks merged into main by squash"; check the commit it names and record it (`workriver unit
merged u2`). A unit still ready a day later (the user's `readyHours`) brings "has been ready
to merge for 24 h": look at the pull request; merged, `workriver unit merged u2`; open,
nothing to do; abandoned, `workriver unit done u2 --reason "…"` (`closing-units`).

**Refusals:** a push refused by a hook on that machine is delivered by the worker as a
refusal in its note: ask the user (another gate) how to go on; do not route around the hook.
