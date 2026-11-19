# Deciding findings

**When:** a review, tests task or gate was delivered with findings. By default **the
intent's manager decides**; the delivery message lists them.

    workriver board u1
    workriver finding show u1#4 P2-1                                          # one in full
    workriver finding decide u1#4 P2-1 accept --note "Real risk"
    workriver finding decide u1#4 P3-1 revise --note "Do it in the client instead"
    workriver finding decide u1#4 P3-2 reject --note "Not a risk here"       # or: defer
    workriver task add u1 --title "<outcome>" --cause 4:P2-1 --agent <name>  # accepted / revised

What is wrong (a bug, a hole, a broken test) is a finding by itself. How the code should
look or be organized is judged by the project's own rules (`CLAUDE.md` / `AGENTS.md`) and
the design before it, and such a finding names the rule: one of that kind with no rule
behind it is a taste, to reject or defer.

**You will see:** `recorded`. The user can decide any finding instead and change yours:
"The user rejected finding … This changes the earlier decision …" - stop or adjust the work
handed out (`workriver task drop u1#5 --reason "P2-1 rejected by the user"` for work no longer
needed; its agent is told). The user's switch: `workriver intent set i3 --findings user|manager`.

**Refusals:** "revise needs a note". "… is the user's to decide (intent i3: findings user)" -
wait. "… is for the manager of intent i3 to decide (or the user), not for other agents".
"… is already accepted; only the user changes a decision".
