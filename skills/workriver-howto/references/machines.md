# Other machines and linked workspaces

**When:** agents run on another machine. Setting this up is **the user's**, from a terminal
that is not an agent's pane, on the machine workriver runs on:

    workriver machine add box --ssh me@box [--herdr-socket PATH]
    workriver machine enable box          # waits, then says connected or why not
    workriver workspace link box:<workspace there> <workspace here>

What a manager or worker can do:

    workriver machine list                    # on workriver's machine: connected, or why not
    workriver workspace list                  # what is linked to what
    workriver status                          # on the other machine: must answer, same build

A workspace on another machine is one with yours only if linked; do not hand work to agents
in unlinked ones. Units there: `units-other-machine`.

**Refusals:** "only the user does this" - ask the user. "a caller on machine … is an agent
of that machine, never the user".
