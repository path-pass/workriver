# Other machines

**When:** agents run on another machine. Setting this up is **the user's**, from a terminal
that is not an agent's pane, on the machine workriver runs on: a new terminal window, or a
Herdr pane with no agent (`!` in your session runs in your pane and is refused). Give the
user the exact lines:

    workriver machine add box --ssh me@box [--herdr-socket PATH]
    workriver machine enable box          # waits, then says connected or why not

What a manager or worker can do:

    workriver machine list                    # on workriver's machine: connected, or why not
    workriver status                          # on the other machine: must answer, same build

On another machine the service checks a command there, not here: the CLI sends its process
id, and the service reads that process's parents over ssh against that machine's Herdr. A
command there must run in its agent's own pane like here (`hand-off`), is never the user,
and needs the same build as the service (`updating`): an older CLI is refused with "the
calling process on machine … is not given".

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

A workspace has no machine: agents on a followed machine work in the same workspaces as
agents here, through the tasks they are given, and the user may seat one of them as a
workspace's manager (`seat`). There are no links between workspaces any more. Units there:
`units-other-machine`.

**Refusals:** "only the user does this" - ask the user. "a caller on machine … is an agent
of that machine, never the user".
