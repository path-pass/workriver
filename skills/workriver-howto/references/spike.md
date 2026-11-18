# A spike: finding out by trying

**When:** the answer comes from trying, not reading: measure, compare, prototype an approach,
and keep the code to show how it was done. Finding out by reading is a research unit
(`units`); code meant to land is a code unit.

A spike is a unit with code on a branch of its own, `spike/<topic>`, shown like any code
unit (commits, uncommitted work, the code view), but **never merged**. Give it its own
worktree like any code unit; the unit still names the repository:

    git -C <repo> fetch && git -C <repo> worktree add <repo>.worktrees/<unit> -b spike/<topic> origin/main
    workriver unit add --kind spike --title "CRDT sync" --purpose "Is it fast enough offline?" --branch spike/crdt [--base main] [--repo <repo>]

Tasks as usual, any kind: `implement` for the code (commits carry `Task: u4#1`), `research`
for the write-up, which delivers the numbers in `--note` and attaches the scripts and
results (saved in `workriver attach --dir u4#2` before delivering; `attachments`):

    workriver task add u4 --title "Benchmark CRDT sync against the current one" --agent bench
    workriver task add u4 --kind research --title "What the numbers say" --after 1 --agent bench
    workriver task deliver u4#2 --note "CRDT: 3x slower on 10k messages; see bench.md"

When its work is delivered and decided it is "ready to close" (never "ready to merge"):

    workriver unit done u4 --reason "CRDTs 3x slower on 10k messages: keep the current sync"
    git -C <repo> worktree remove <repo>.worktrees/u4

The branch stays, so the code can be shown later. What the user should see goes into the
intent's note too (`workriver intent set i3 --add-note "…"`). Code worth keeping goes into a
new code unit, not by merging the spike.

**You will see:** the unit marked "Spike" on the page.

**Refusals:** "a spike's branch starts with spike/ and a name" - name it so. "a unit's
branch starts with feat/, chore/ or fix/ … spike/<name> is for --kind spike" - add `--kind
spike`. "unit u4 is a spike: it is never merged" - close it with `workriver unit done u4
--reason "<what was learned>"`.
