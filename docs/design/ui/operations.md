# Operations: running commands, machines and settings

Running a command as the user, the machines workriver follows and their alerts, stopped
workers, Settings, and opening an agent's terminal.

Moved from `DESIGN.md` when it was split into these files, word for word but for paths
made whole and "see …" naming the file; `DESIGN.md` is the overview and says where each of
its sections went.

## Run a command

- **Run a command** is a small box: "Where" (a select of the active units in the project
  shown, the unit on screen first – its work tree is the directory – then the project
  folders), the command (one line, monospace) and "Run", which opens the confirm dialog below.

## Running a command as the user

- **Running a command as the user.** Where an agent may not do something (a hook blocks its
  push), the manager makes it a gate step with the command. Next to the copy button of every
  command in a gate's or chore's steps sits "Run" (a play icon). It, and the side panel's box,
  open one dialog: the exact command in monospace, its directory (the unit's work tree, else
  its repository; a project's folder), the machine (the unit's), and who wrote it – "you", or
  "agent X, an agent: read it before you run it" in amber, since an agent's text is not to be
  trusted. A command that can destroy work for good (`rm -r`/`-f`, `push --force`/`-f`/`+ref`,
  `reset --hard`, `clean -f…`, `branch -D`, `mkfs`, `dd of=`) is flagged in red: the box red,
  a line saying what it can do, the Run button the destructive one. A heuristic for the
  obvious ones, not a guard. Under it, **What it will do**: when the dialog opens, workriver
  starts a temporary agent (in "workriver-commands", in the command's folder, read-only) that
  follows the command into the scripts it calls; "Reviewing what this command does…" with the
  pending dot, then its verdict as a chip ("ok" in the done tone, "caution" amber, "danger"
  red) and its short Markdown, or "Could not review it: …". It only warns: Run is never
  disabled or delayed by it, and a review that comes after Run still shows. The dialog is
  wide (`sm:max-w-3xl`) so commands and output read on one line. Run and Cancel; nothing runs
  before Run. Then workriver types it
  into a new tab of a plain shell (no agent) in a Herdr workspace named "workriver-commands" on the
  unit's machine (made on first use, reused after) as `cd -- '<dir>' && eval '<command>';
  echo "__workriver_end_<id>_$?__"`: a tab per run, so a run never types into a shell an
  earlier one still holds (a password prompt, a pager) and each output starts clean. It is the
  user's own shell, so their credentials and hooks apply exactly as when they type it. The
  dialog then shows what the tab shows (read every second; the echo of the typed line left
  out), "Running…" in amber, then "Done (exit 0)" in the done tone or "Failed (exit N)" in red
  once the marker line appears (the typed line carries `$?`, not digits, so its echo never
  counts; the id is random, so the command's own output cannot fake it); "Open in terminal"
  brings that tab to the front. A tab that cannot be read for five seconds (closed, or its
  shell exited) says so. The marker needs a POSIX shell (zsh, bash, sh).
  A run that ended with exit 0 has its tab closed three seconds later (its output stays in the
  dialog; "Open in terminal" goes, and asking for it answers that the terminal was closed). A
  run that failed, still runs or waits for input keeps its tab, for the user to look or answer
  there; the next run closes the tabs of every earlier run that has ended, never one still
  going. A tab is closed only when nothing is left running in its shell: nothing in front of
  the shell and nothing it started still alive, so a job the command left in the background
  (`npm run dev &`) or whatever the user went on doing there keeps it, and the next run looks
  again; so it does when the tab could not be closed just then (the machine not reached).
  Only tabs workriver opened for runs are closed, and each is checked again right before: still
  in "workriver-commands", still a plain shell with no agent; anything else is left for good.
  A shell whose prompt keeps a helper process of its own alive always counts as busy.
  Under the Run section's box, **Recent runs** (the newest 20, newest first; nothing when
  there are none): one row each with the command (monospace, one line, the whole text on
  hover), whose step it was (`u3#4 step 2`, a chore's id, or nothing when typed there), a
  state chip ("running" amber with the pulse, "✓ ok", "exit N" in red, "unknown" when the
  service restarted or the terminal closed before the end was seen), the review's verdict chip
  when it has one, and how long ago. A row
  opens the same dialog on that run. They come in the user's snapshot only (agents never see
  them), from the recorded runs and ends; the output lives in the service's memory, so after
  a restart the dialog says it was not kept and offers no terminal.

## Machines

- **Machines** shows CPU and memory of this machine ("This machine", first) and of every
  followed one (by its machine chip), over the last ten minutes, read every 5 s while the
  section is open. Per machine two lines, CPU and Memory: the label, a small line of the
  ten minutes with the alert threshold dashed across it (inline SVG; a chart library is
  not worth one line), and the value right-aligned in a fixed column ("97 %",
  "25.4 / 32 GB"). Lines are neutral; a metric in alert takes the colour of its level,
  amber (`--amber`, `--amber-ink` on `--amber-bg`) for a warning and red (`--del` on
  `--del-row`) for critical, the same as the pressure word: a machine's state is the
  system's rating, not work waiting for you, so it is never violet. A pill per alert in the
  machine's header, critical first, says which, its value and for how long ("CPU 97 % ·
  6m", "Compressor 66 % · 2m", "Disk / 86 % used, 14 GB free · 40m"). A machine that cannot be reached keeps its last values, faded, with the
  dashed chip and "disconnected since …". Memory used is total minus available, the same
  on every machine: one small sh probe reads them all through the runner git uses, here
  or over the machine's ssh connection, and nothing is installed anywhere.
  Under the two lines, a third, "24 h": CPU (neutral) and memory (water) over the last 24
  hours in one small inline SVG, one point a minute, broken where the machine was away;
  read when the card shows and every minute after. Under that, small key–value pairs of
  the newest reading, only those the machine's system has, wrapping: Available (total −
  used, on Linux MemAvailable); Pressure, on macOS the kernel's level as a word, normal
  green (`--add`), warning amber (`--amber-ink`), critical red (`--del`), on Linux PSI's
  "some 0.59 % · full 0.12 %" (the last 10 s), or "unknown" without PSI; System free
  (macOS, the kernel's free share, `memory_pressure`'s number); Compressed and Compressor
  ("66 % of its limit": segments or compressed pages over the kernel's limit, whichever is
  higher; at 100 % macOS is out of compressor space) (macOS); OOM kills (Linux: since boot
  · while watched); Swap used / total (left out without swap); Disk free of the home
  folder's filesystem, and of `/` by its mount point when that is another one. Sizes in
  MB, GB or TB (steps of 1024). A reading in alert takes its level's colour (a
  filesystem by its own level). The same probe reads it all (one round trip per sample); a piece a kernel lacks is a
  field without a value, never a failed reading.
  **History**: each machine's samples are folded into one line a minute (the mean of each
  reading, the worst pressure level, counters and disks at the minute's end) appended to
  `<data dir>/metrics/<label>.jsonl` (0600), kept 7 days: at most once an hour the file
  is rewritten without older lines (one file, not one per day, so a look back is one
  read). About 0.5 MB per machine per day. Not in the event store, and never read back
  into the 5 s samples alerts judge. `GET /api/metrics/history?machine=&hours=` (1–168)
  reads it, with the access of `GET /api/metrics`.

## Other machines

- **Other machines.** Agents may run on other machines, each with its own Herdr. Whatever is
  on another machine carries a small chip with the machine's name (a server glyph): an
  agent's line in Agents, a unit's header and its line in the rail; an agent mark's tooltip
  ends with "@machine". When the machine cannot be reached, its agents stay as last seen,
  faded, the chip dashed (its tooltip says since when it is disconnected), and they raise
  nothing in Needs you. Settings says "disconnected since …" and when it connects again. An agent on another
  machine is in a workspace as any agent is: seated as its manager by the user, or through
  the tasks it has (Agents groups by that, `navigation.md`); there are no links between
  workspaces, and nothing is ever joined by a name. Settings has "Other machines": each with
  a follow switch, its state in a few words (connecting… until the first attempt ends,
  connected and how many agents, not connected, connected but no Herdr) and ssh's own words when it is down; a name and ssh target to add
  one.

## Stopped workers

- **Stopped workers.** Settings has "Stopped workers", before "Machine alerts": one sentence
  with the minutes (1 to 240, default 10) an agent may sit idle on a running task before it
  is reminded; half that again later the manager is told and the row shows in Needs you.
  Kept in the service's settings with the alert thresholds; the user's alone.

## Starting managers

- **Starting managers.** Settings has "Starting managers", after Workspaces: the harness and
  model "Start a new one" in the manager select takes unless the user changes them there
  (`navigation.md`), with the same controls and greying (a harness not offered here is greyed
  with why; the model empty for "your default", the harness's names as quick picks, or one
  word typed, checked before it is sent). Save (enabled once something changed and the
  harness is offered and the model is of the form) records it (`/api/settings/start`, kept
  in the service's settings with the alert thresholds; the user's alone), then says "Saved."
  quietly; a refusal shows in the service's words. **Try it** starts that harness and model
  once in a temporary folder, as a start would but seating nothing, and closes it again
  (`/api/settings/start/try`): the service may take a minute or more, so the button waits
  and a quiet line says it is trying; then the result in the service's words ("It started:
  …", or "It did not get ready: …" in the removed colour) and, when it is not empty, the
  screen it read, preformatted, mono, scrolling within a fixed height. A refusal shows in
  the service's words.

## Machine alerts

- **Machine alerts.** Settings has "Machine alerts", before "Other machines", the same for
  this machine and every followed one, each applying where the machine's system has the
  value, in three groups. Every machine: CPU and Memory, "≥ [90] % for [5] min" (one
  level, warning), and Disk, "warn over [80] % used or under [50] GB free, critical over
  [90] % or under [20] GB, for [1] min" (each filesystem listed; the worst one alerts).
  macOS: Compressor (warn over 60 %, critical over 80 %, at once), Swap (4 / 10 GB), the
  kernel's Pressure level (warning warns, critical is critical), System free (under 25 /
  20 %). Linux: Available (MemAvailable / MemTotal under 25 / 12 %), Memory stall (PSI
  `full` avg10 over 5 / 20 %: all tasks stalled, real lost work; `some` rises with one
  task paging), OOM kills (any new kill is critical at once, held 5 min after the last).
  Each has a switch by its name and one sentence with its fields. The duration of the
  memory and disk alerts defaults to a minute: a spike never alerts, and a leak that fills
  a Mac's compressor within minutes is still caught early. A level rises once every
  sample of that time is past it (the compressor and OOM kills on the sample that sees
  them), steps down from critical, or clears, after a minute back; while a machine cannot
  be reached its alerts neither rise nor clear. A field is saved when left or on Enter;
  out of range, or crossing its other level, it goes back; a refusal shows and brings back
  the service's values. On a narrow screen the sentence wraps under the switch. The
  thresholds are kept with the list of machines in the service's settings, not as events;
  a settings file without the newer keys takes their defaults.

## Settings

- **Settings** (the gear at the rail's foot) is a dialog that shows one group at a time, as
  tabs, in this order: General (language, theme, notifications and the sound), Stopped
  workers, Workspaces, Starting managers, Machine alerts, Other machines. The tab names the group, so
  a group has no heading of its own. A group with nothing to set has no tab: Workspaces
  while there is none, Other machines on the sample
  page. **Workspaces** lists every workspace, open and closed, in the order they were made:
  its name with its id in mono (and a "closed" tag), under it its project's title and its
  manager with its state ("Manager mgr-chat: working", "not running", "no manager"). An open
  one has "Manager…" (the manager select, `navigation.md`) and "Close" (the service refuses
  it while an intent is active there, in its words); a closed one has "Open again". A
  closed workspace is nowhere else on the page but in the Manager select's closed group. A tab whose setting comes from the service (Stopped workers, Starting managers, Machine alerts) is never
  an empty panel: while the setting loads it says so in a quiet line, and when it cannot be
  read it says why (`role=alert`). The dialog keeps one height whichever tab shows (each
  scrolls on its own); on a narrow screen the tabs wrap. The arrow keys move between tabs.
  The tab picked is kept in this browser (`workriver.settings.tab`) and the gear opens
  Settings on it next time; a kept tab that has no tab now opens General. A link to a
  setting opens Settings on its group's tab (`openSettings` in
  `frontend/web/src/machines/Settings.tsx`) without changing the kept tab; nothing calls it
  yet. No setting does anything else because of the tabs.

## Opening a terminal

- **Opening a terminal.** A small terminal button sits at the end of each agent's line in
  the panel (a click target of its own: the line opens the agent's work, the button its
  terminal), in a Blocking row, and in the header of running or waiting work. It is not on
  graph nodes. It opens the agent's **terminal view** in the page; "Open terminal view" in a
  gate does the same. Bringing the terminal app to the front (`focusAgent` in
  `frontend/web/src/api/actions.ts`, the one function for it) is "Open in terminal app" in
  the view's header. A view, like a gate, names its agent by session and machine together
  (the same resumed session can live on two machines): every line, row, button and
  notification passes both, and the view stays on that one across refreshes. A task's button
  finds its agent in its intent's workspace (on the unit's machine when the unit has a
  repository), only when exactly one instance fits (see Answering a stuck agent in
  `navigation.md`).
- **Where.** A dock at the bottom of the main area, absent until a view is opened: the
  agent's mark and name, its state, "Open in terminal app", fold, close; its height dragged
  with `useResize` (remembered). One view per page: opening another agent's replaces it. On
  a narrow screen it is a sheet.
- **One stream, while shown.** The dock folded, the page hidden (`visibilitychange`) or the
  view closed ends its stream (`/api/agent/view`, Server-Sent Events); showing it again opens
  a new one, whose first frame is the whole screen (5–10 KB). So a page holds at most one
  stream, and the runtime pays for a working terminal only while it is looked at. A stream
  that ends for another reason (the connection, a reader too slow) is reopened once, from a
  whole frame. Within one stream a frame is drawn only after a whole frame came on it, and
  only if its `seq` is above the last drawn: a delta before any whole frame, or one out of
  order, ends that stream as an end does (never drawn on a blank or old screen). The service
  caps open views at four across every page.
- **Drawing.** xterm.js (`@xterm/xterm`), loaded only when a view first opens, with its DOM
  renderer (one terminal shown; no WebGL addon, no context limit to meet). Columns and rows
  are the pane's (from the frames); the font scales so the width fits, down to 9 px, then
  the view scrolls sideways. The terminal is never resized. Read-only: `disableStdin`, no key
  reaches it; no clipboard write (OSC 52 ignored). No links: no link addon, and since xterm.js
  makes OSC 8 hyperlinks in its core, the Terminal is created with
  `linkHandler: { activate: () => {}, allowNonHttpProtocols: false }`, so a click on one does
  nothing and shows nothing. While its agent waits, the gate sits under the view (the same
  component, the same request), for when only the terminal shows what to press: the buttons
  with the screen they are bound to, as in the gate (the region; the whole screen folded, open
  with the cut line when the region was cut or nothing was parsed), since the stream is not
  that screen (it may be blank, late, or show another dialog). It scrolls within its share of
  the dock.
- **Gone.** An agent that leaves, a machine that cannot be reached, or a view that cannot be
  had (another machine without its runtime's terminal stream brought here, a stream that
  fails): the view keeps its last frame, greyed, or shows nothing, with one line why. The
  gate still works: it needs only reads and keys.
