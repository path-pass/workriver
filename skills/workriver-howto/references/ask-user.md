# Asking the user to decide

**When:** a unit is ready to merge, something is to be deployed, the goal or the agreed
units would change, anything destructive, or a task failed the same way twice. The user
does not watch your pane; they watch Needs you. Ask there, with a decide gate, never only in
your pane.

    workriver task add u2 --kind gate --gate decide --title "Merge u2 (feat/readme) into main?" --after 4 --text "<what is ready, what was checked, what merging does>"
    workriver task add i3 --kind gate --gate decide --title "Drop the export part of the goal?" --text "<why, and the options>"

A unit's question goes on the unit, after its last task; a question about the whole
intent goes on the intent (`i3`). Put in `--text` what the user needs to decide without
opening anything else: what is ready or wrong, what was checked, the options.

**You will see:** "The user confirmed … in gate …" with their note: do it (merge, deploy,
change the goal). Or "did not confirm …" with findings: handle them like a design gate's
(`design-gate`). Until then, wait; do not merge or deploy.

**Refusals:** "gate … is not open yet: it follows …" - it waits until the tasks before it
are done; that is fine, it shows in Needs you then.
