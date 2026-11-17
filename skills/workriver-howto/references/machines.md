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

How loaded each machine is (read-only, for you too; the user sets the alert thresholds):

    workriver machine load                    # CPU and memory per machine, and alerts
    workriver machine load --hours 24         # also mean / max CPU and memory over 24 h (max 168)

Under each machine's line, a second one: available memory, memory pressure (macOS:
normal / warning / critical; Linux: PSI "some … full …", or "unknown"), compressed memory
(macOS), OOM kills (Linux: since boot, and while watched), swap, free disk of the home
folder's filesystem (and `/` if another).

Before handing heavy work (a build, a test run, a browser) to an agent on a machine in
alert, prefer another machine or wait. Also wait or pick another machine when its
pressure is warning or critical, its OOM kills grew while watched, or its disk is nearly
full.

A workspace on another machine is one with yours only if linked; do not hand work to agents
in unlinked ones. Units there: `units-other-machine`.

**Refusals:** "only the user does this" - ask the user. "a caller on machine … is an agent
of that machine, never the user".
