# Splitting an intent into units

**When:** planning. A unit is a piece that can be built, reviewed and merged on its own, on
its own branch. Split by what can land independently, not by layer; a small intent is one unit.

    workriver unit add --title "Mentions" --purpose "Let people @ each other" --branch mention [--base main] [--repo PATH]
    workriver task add u1 --title "Notify once when mentioned twice" --agent finder [--plan src/notify]
    workriver unit merged u1               # once its branch is merged

Work of the intent, not of one unit (comparing approaches, a design across repositories):

    workriver task add --kind research --title "Compare sync engines" --agent scout

**You will see:** the unit id (`u1`), task refs (`u1#1`, `i3#1`).

**Refusals:** "no intent given, and Herdr workspace … has no current intent" -
`start-intent`. "… is not a git repository" / "base … is not a commit or branch" - fix
`--repo` or `--base`. "implement and tests tasks belong to a unit".
