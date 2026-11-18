# Other machines and linked workspaces

**When:** agents run on another machine. Setting this up is **the user's**, from a terminal
that is not an agent's pane, on the machine workriver runs on: a new terminal window, or a
Herdr pane with no agent (`!` in your session runs in your pane and is refused). Give the
user the exact lines:

    workriver machine add box --ssh me@box [--herdr-socket PATH]
    workriver machine enable box          # waits, then says connected or why not
    workriver workspace link box:<workspace there> <workspace here>

What a manager or worker can do:

    workriver machine list                    # on workriver's machine: connected, or why not
    workriver workspace list                  # what is linked to what
    workriver status                          # on the other machine: must answer, same build

How loaded each machine is (read-only, for you too; the user sets the alert thresholds). On
another machine it shows that machine only:

    workriver machine load                    # CPU and memory per machine, and alerts with their level
    workriver machine load --hours 24         # also mean / max CPU and memory over 24 h (max 168)

Under each machine's line, a second one: available memory, memory pressure (macOS:
normal / warning / critical; Linux: PSI "some … full …", or "unknown"), compressed memory,
how full the compressor is ("compressor 17% of its limit") and the kernel's free share
("system free 50%") (macOS), OOM kills (Linux: since boot, and while watched), swap, free
disk of the home folder's filesystem (and `/` if another).

An alert in the first line has a level: `ALERT: compressor WARN since …, disk CRIT since …`.
Metrics: cpu, memory, disk (every machine); compressor, swap, pressure, systemFree (macOS);
available, psi, oom (Linux). The last line lists the thresholds the user set. A macOS
compressor at 100 % of its limit can panic the machine; an OOM alert means the kernel
killed a process there within the last minutes.

Before handing heavy work (a build, a test run, a browser) to an agent on a machine with a
CRIT alert, do not: pick another machine or wait. With a WARN alert, prefer another machine
for heavy work. Also wait or pick another machine when its pressure is warning or critical,
its OOM kills grew while watched, or its disk is nearly full.

A workspace on another machine is one with yours only if linked; do not hand work to agents
in unlinked ones. Units there: `units-other-machine`.

**Refusals:** "only the user does this" - ask the user. "a caller on machine … is an agent
of that machine, never the user".
