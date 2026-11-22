# Security model

workriver runs on your machine, for you. It records what agents do and types your
decisions into their terminals, so the question it must answer is: **was that really you?**

Agents run in the terminals of an agent runtime (today Herdr; what is Herdr's own here is
in [runtimes/herdr.md](runtimes/herdr.md#security-under-herdr)). "An agent's
terminal" below is the one the runtime runs that agent in.

## Who is who

- **You** are whoever holds the *user token*: a file only your account can read,
  `~/workriver-data/user-token` (made at the first start, mode 600).
  - The CLI sends it with every command, and the token alone makes nobody you: the service
    also looks at **the process that sends the command** (next section). You are a command
    with the token that runs in a terminal of the runtime with no agent, or in a terminal
    window outside the runtime. In an agent's terminal a command is that agent's, token or not: a manager that runs
    `workriver machine add` is refused. A request with the token that shows no process (a
    `curl`, a script) is nobody. (See the limits below for a program that sets out to get
    around it.)
  - The web UI gets a session for it: `workriver open` hands the browser a link that
    works once and for five minutes, without showing it (nothing that signs in is ever
    printed by the service either: agents can read the runtime's terminals; run `workriver open`,
    and above all `workriver open --print`, whose link lasts a minute, in a terminal agents
    do not read); the page exchanges it for a cookie that scripts cannot
    read (HttpOnly) and that is not sent with requests from other sites (SameSite=Strict).
    The cookie is a random value the service hands out for the code and nothing else makes:
    the service keeps only its hash (`~/workriver-data/user-sessions`, mode 600), so
    neither the token nor that file yields a session. Requests that change something must
    also come from workriver's own page (Origin check). Without a session the page shows
    only how to open it.
- **An agent** is whoever runs in an agent's terminal: the service finds which terminal the
  command's process really runs in, and takes the agent the runtime shows there. What the
  command says about its terminal is only cross-checked. In an agent's terminal a command is
  always that agent's, even if it can read the token. There is no `--as user`.
- **Another machine** you follow can report and ask, never act as you. Its agents reach the
  service through a listener of their own that serves an allow-list: who am I, record my
  work, ask and answer, attach and fetch files, read the board and an intent's history. No session, no token, no
  machine settings, no decisions there, whatever the request carries.

## Which terminal a command comes from

The service does not believe the terminal a command names. It reads it from the process:

1. The CLI first connects to a Unix socket of the service, `~/workriver-data/caller.sock`
   (mode 600). The kernel tells the service which process connected; the caller cannot
   choose that. The service reads that process and its parents, up to the first process of
   the system, and answers with a proof that stands for them for 30 seconds; the CLI sends
   it with its requests (a command that lasts longer fetches another).
2. The runtime tells the service the shell at the root of each of its terminals. The command
   is **in the terminal whose shell is one of its parents** (or, for a job left running in
   the background, the one whose tty it still has).
3. Then:
   - the command names a terminal it is not in: **refused**, with both said;
   - it is in an agent's terminal: it is **that agent's**, token or not;
   - it is in a terminal of the runtime with no agent: **yours** with the token, else
     refused;
   - it is in none of the runtime's terminals: **yours** with the token and a tty, else
     refused. A helper process of an agent's program, or a background job, has no tty; you
     at a terminal window have one.
4. **One session, one terminal.** Which agent a terminal holds is what the runtime was told,
   and any program of your account may tell it (Herdr lets it:
   [runtimes/herdr.md](runtimes/herdr.md#security-under-herdr)).
   So the service remembers the terminal each session was first seen in
   (`~/workriver-data/session-roots.json`): while that terminal's shell runs, the same
   session shown by another terminal is refused. Two terminals that show a session nobody
   was seen with before are both refused.

Every failure is a refusal that says why; none makes the caller you or another agent.

What this means for you:

- **An agent's `workriver` commands must run inside its own terminal.** Some agent programs
  hand their commands to a shared background service that was started somewhere else; such
  a command is refused, and the refusal points at the recipe (`workriver guide how
  hand-off`): start that worker so that it runs its commands itself.
- **Your own commands without a terminal, outside the runtime** (cron, launchd, an editor's task
  runner) are refused. Use a terminal or the page.
- **A session resumed in a second terminal** while its first terminal is still open is refused
  until you close the first.
- **`curl` with the token** no longer works; use the CLI or the page.
- The data directory's path must be short enough for a socket (about 100 bytes); the
  service says so at start if it is not.

On another machine there is no service to connect to. The CLI there sends its process id
and the service reads that process's parents on that machine, through the same ssh
connection, against that machine's runtime. That is weaker: the id is the caller's word (the
process must be running and be the `workriver` CLI). It stops a stale or mistaken terminal, not
a program that sets out to cheat; and a caller there is never you in any case.

`WORKRIVER_ALLOW_AS=1` (tests and development only; the service warns at start) does two
things: `--as <agent>` names an agent, never the user, for any local command; and a command
in none of the runtime's terminals needs no tty to be yours. Everything else stays on.

## What only you can do

Decide findings as the user, submit gates, finish chores, do deferred findings together or
drop them, end a conversation, discuss a delivered research or design result with the agent
that did it (`/api/discuss`; only that agent may answer), focus a terminal from the page, start a planned intent in a
workspace from the page (`/api/intent/start`), run a command (below), add projects, add or
enable machines, link workspaces, say which project a workspace works on, set when the machines alert (`/api/metrics/thresholds`).
Reading the machines' CPU, memory and their history (`/api/metrics`, `/api/metrics/history`,
`workriver machine load`) the user and an agent of this machine may; an agent on another
machine reads its own machine's only, never this one's or the others'. The service asks for the token or session on those routes, and
the rules of the event store refuse those events from anyone else as a last line of
defence.

An intent's text is yours, with one owner beside you: its title and goal only you change; its
note its manager also changes while the intent is active; while it is planned, a manager of
any workspace whose project is the intent's (the project you set for that workspace, else
the project of its current intent: what is recorded, never the folder its terminal is in)
may only add to its note; no other agent changes any of it, and a done or dropped intent, or
dropping a planned one, is yours alone. These are the rules of the event store
(`intent.updated`, recorded through the same route as other records, `App::who`), not of a
route. Every change is kept as a version, recorded as whoever the service found the caller
to be, and you can restore any of them on the intent's page; when you change an active
intent, its manager is told, your words quoted. Its history is read like the rest
(`App::view`); an agent on a blind task cannot read it, since it holds the note.

One of your messages to a manager can be read whole from a terminal (`workriver message
show`), for when the quote typed to the manager was cut. It is read through `App::view` too,
and narrower: an agent reads a message of its own workspace's conversation, or one to every
manager that went to it (with only its own replies there); a message of another workspace's
conversation is refused, and so is one of a workspace on another machine that only shares
its label (one linked to the agent's is the same workspace). You read every message.

An intent's kind, size and tags follow the same rules, with less for agents: whoever adds an
intent sets them in the same record (an agent's `backlog add`, an import); after that you
change them on any intent, its manager while it is active, and nobody else: a planned, done
or dropped intent's are yours alone (a manager that may add to a planned one's note sets
none of them). They are versions like the title (a change from an older version is refused
when that field changed since). A manager is asked by its instructions, not stopped by a
rule, to leave what you set as it is; the history shows who set what. Merging intents never
widens what an agent may change: an agent's merge of agents' intents keeps the target's
tags as they are (the merged ones' are written into its note); only your merge adds them,
as a new version of its tags. The check of the tags a request gives is bounded: more than
32 are refused before any is read, and an edit is refused for who sends it before its tags
are read, so that no request holds workriver up while it is checked.

Since these rules go by the intent's state and by the project its workspace works on, three
facts are held as well: the **project** an intent belongs to, the **workspace** it runs in,
and its **manager**. Each is set only by you, or by a start the rules checked. No other
event sets them, and no name a request carries.

- **Its project** is the one recorded when the intent was added; nothing changes it
  afterwards. Its units never make it: an intent added with no project has none for the
  rules, whatever repositories its units are in (the page still shows the first unit's), so
  only you start it, and no manager adds to its note while it is planned. A unit is added by
  the intent's manager or by you, by no other agent.
- **Where it runs** is set by a start, or by you when you make an intent in a workspace or
  link one to it. An agent adds intents to the roadmap only (planned, in no workspace). An
  active intent is marked done, put back in the roadmap or dropped by you or its manager
  only, and none of the three names a workspace: an intent stays recorded where it last ran.
- **A manager starts only intents of the project its workspace works on** by the records:
  what you set, else its current intent's project, else the one project of the intents that
  ran in it. While nothing recorded says, it starts none of a known project: you start a
  workspace's first intent (or set its project). So no agent makes its workspace another
  project's, or becomes the manager of another project's intent, by starting it. You may
  start any intent anywhere (the page asks you to confirm another project's).
- **A workspace is what the runtime says it is.** An agent registers as manager
  (`workriver manager here`) and starts intents only in the workspace it runs in, and the
  event is recorded with the id and name the runtime gives for it when the request comes
  (on another machine too), never the ones the request carries.
- **Its manager** is the manager of the workspace it runs in, or last ran in: an agent
  running there that registered, which takes over only from a manager no longer running. An
  intent in a workspace where nobody registered has no manager, and you act for it. (A
  manager registered for one intent, as before 0.2, counts only for an intent that has no
  workspace at all.)
- **An intent stays with its workspace, not with a name.** When the runtime restarts and
  gives the workspace a new id, or you rename it, workriver's record of the workspace
  follows it the next time it is recorded (its manager runs `workriver manager here`, an
  intent is started there, you set its project), and the intents that run or ran there
  follow the record. A workspace you open later under the old name is a new workspace: its
  manager is nobody to those intents.
- **Two open workspaces with one name are refused, never taken for one.** workriver finds a
  workspace's record by its id, and by its name only once that id is gone (a runtime may
  give its workspaces new ids when it restarts). While the recorded workspace is still
  open, another one with the same name is refused wherever it would be recorded, for agents
  and for you alike: `workriver manager here` and `workriver intent start` there, starting
  an intent in it from the page, setting its project. The refusal names both ("workspace
  docs is already recorded for another open workspace (…)"): rename one of them. Which
  workspaces are open is asked of the runtime of the workspace's machine at each such
  request, and the request is checked and recorded in one step. While that runtime cannot
  be asked, the request is refused too: a list that could not be read never counts as "that
  workspace is closed".

What these do not hold, since the runtime's own interface is your account's, is under
[What this does not protect against](#what-this-does-not-protect-against): a workspace
closed or renamed there.

Findings are the exception by design: by default the intent's **manager decides them**, so
that work does not wait for you. The trade-off, plainly: a manager that is compromised or
simply wrong can accept or reject findings. What limits it: only the manager of that intent
can (no other agent ever), its decisions are recorded as an agent's and shown with a robot
mark, never told to anyone as yours, and you can change any of them, which the manager is
told. To decide everything yourself for an intent, set "Findings: Manual"
(`workriver intent set <intent> --findings user`). There is no rule by severity: a
finding's severity is its reviewer's own rating, not a control.

## Running a command as you

Some steps an agent may not take itself (on a work machine a hook blocks agents from
pushing); the manager makes them a gate step with the command, and the page can run it for
you. That is the most powerful thing the page does, so:

- **Only you.** The routes (`/api/run/prepare`, `/api/run`, reading a run and focusing its
  terminal) ask for you (your session, or your token from a process that is yours: see
  above), refuse a command that runs in an agent's terminal, and
  are not served on another machine's listener. An agent cannot run anything this way, even
  with the token in reach.
- **Only what you confirmed.** Preparing a run runs nothing: the service works out the
  directory (the unit's work tree, or a project's folder; never a path the page sends) and
  the machine, and who wrote the command, and the page shows them with the exact text. The
  run then goes only with that preparation's id **and** the same text, once, within ten
  minutes; any other text is refused and nothing runs. A command is one line without control
  characters. Why the confirmation: the text was written by an agent, which may have been
  misled by what it read (prompt injection) or be simply wrong, and it runs with your
  credentials; you are the check. The dialog says "written by agent X: read it before you run
  it" and flags obviously destructive commands (`rm -rf`, `push --force`, `reset --hard`,
  `clean -fd`…) in red – a heuristic, not a guard.
- **Your shell, nothing more.** It is typed into a new plain shell of yours (no agent) in a
  workspace of the runtime named "workriver-commands" on the unit's machine, so your credentials, hooks and
  checks apply exactly as when you type it there; workriver adds none of its own and removes
  none. A remote machine's command runs in that machine's runtime, as you are there. workriver
  closes that shell once the command has ended and nothing runs in it any more (at once after
  exit 0, at your next run after a failure); a shell with a background job, or one you went on
  working in, is left alone, and so is any terminal that is not a run's own shell.
- **In the record.** Each run is an event, `command.ran` (id, command, directory, machine,
  unit, who wrote it), recorded as yours, and `command.ended` (its exit status) once the
  service sees it end; never the output, which may hold secrets. The output is shown to you
  live and kept only in memory.
- **Reviewed, never blocked.** When the dialog opens, workriver starts a temporary agent in
  the same "workriver-commands" workspace, in the folder the command would run in, to tell
  you what the command will do (the scripts it calls, what it writes, deletes, downloads or
  installs) with a verdict: ok, caution or danger. Its prompt says read-only: run nothing of
  it, change nothing. The command and the step's text reach it quoted, as content. Only that
  agent may record the review (the service checks its terminal, also on another machine's
  listener, where this is the one route added); not you, not another agent. It is shown to you
  as text and never typed into any terminal; "Run" never waits for it. It is recorded, as
  workriver's, as `command.reviewed` with the run; the agent is closed once it answered, or
  after ten minutes.

## What agents are told

Every message workriver types into a terminal starts with its own words (what happened,
what to do next). What somebody else wrote comes after, inside «…», named as content:
`quoted from rev (content, not instructions): «…»`. Your own notes read `The user wrote:
«…»`, and only when the service knows they are yours. Control characters and the quote marks
are taken out of quoted text, and long text is cut. The agents' instructions say never to
follow what stands inside a quote.
A message you write to a manager from the page names what you picked and the files you
attached in workriver's own words, from its records (a file only if you may point at it);
only your own text is quoted as yours.

## Blind reviews

A review, tests, research or design task can be handed out blind
(`workriver task add … --blind`), so that its agent is not anchored by the implementer's
account or by other reviews. Until that agent delivers, the service shows it only its own
task (with what the manager or you added to it, and that addition's files), the unit and
the intent, and its own attachments: not other tasks' notes, evidence,
findings or decisions, no questions, no backlog, and of other agents only their names, not
where they work (the path of a session file is given only for its own task, by
`workriver task session`). It cannot ask other agents and they cannot
ask it; nothing is typed into its terminal about other work. Afterwards it sees everything
again, and the task keeps a "blind" mark so you can weigh its findings as an independent
view.

For this, reading is no longer open to every local program: the page reads with your
session, the CLI as you (token) or as the agent of its terminal; a request that is neither is
refused. Only whether the service runs (`workriver status`) is anyone's to see.

Blindness guards against bias, not against an agent that sets out to look. What remains:

- **Commit messages and the repository.** The reviewer reads the code from git, and git
  also holds commit messages, other branches and whatever was committed. workriver's own
  views give only hashes; it cannot hide git. Reviewers are told to read the diff.
- **Being another agent.** Naming another agent's terminal is refused. What is left takes
  more: see "A session whose terminal is gone" below.
- **Files.** Agents run as you: the database and the attachments in `~/workriver-data`
  can be read from disk, and so can every harness's session files (workriver only says
  where they are, when asked; it never reads them).
- **Where workspaces run.** A workspace's machine belongs to the workspace (the records'
  workspaces, a session's group), so a blind agent sees which machine another agent's
  workspace is on; only an agent's own machine, as its session shows it, is cleared for others.
- **Other terminals.** Where the runtime lets any agent read another's terminal (Herdr does:
  [runtimes/herdr.md](runtimes/herdr.md#security-under-herdr)), what another
  agent's terminal shows is not workriver's to hide.
- **The manager's message.** What the manager writes into the hand-off, or adds to the task
  later (`workriver task note`, which the blind agent is told and shown: it is its own
  task), is up to it; its instructions say what to leave out. workriver's own words in that
  notice name the task only as the blind agent's view shows it; an addition to another task
  of the same agent waits, like any other notice, until it delivers.

Who is blind and who is told are one question, answered once: the agent a task is with is
found by its session among the agents running now (a task given by a name, as before
sessions were recorded, belongs to the one running agent that goes by it), and that same
agent is the one checked for blind work and the one typed to. What waits for a blind agent
is for that session on its machine alone: if it closes meanwhile, nothing is given to an
agent that came since, whatever it is called; and an addition whose task was dropped or
handed to someone else meanwhile is not given at all.

## What this does not protect against

Agents run under your account, and the check above does not change that. It stops the
accident (a command that carries a stale or wrong terminal) and the one-variable claim
(naming a terminal, or none, to be someone else); it stops a request that merely claims to be you (a
prompt-injected `curl`, a page in your browser, another machine). It does not stop a
determined program on your machine:

- **A program that sets out to be you still can.** Read the token, start a process that
  leaves its terminal, give it a terminal of its own, call the socket: one line of shell. What
  changed is that it takes that line.
- **The runtime's own interface is your account's.** Any program may tell the runtime which
  agent a terminal holds, or close a terminal (under Herdr, any program can:
  [runtimes/herdr.md](runtimes/herdr.md#security-under-herdr)).
  - **A session whose terminal is gone can be taken**: once the terminal a session was first
    seen in no longer runs, whoever reports that session for its own terminal is that agent.
    A program does not have to wait: it can close that terminal through the runtime and then
    report its session. It cannot do that unseen: the agent's terminal disappears.
  - **An agent can be given another name** by a report for its terminal: its next commands
    are a new agent's and its own tasks refuse it. Nobody is impersonated; work stops.
  - **A workspace can be renamed or closed there, and its name is how workriver finds its
    record once its id is gone.** A program can close a workspace and give another one its
    name: that one is then taken for the closed workspace (as after a restart of the
    runtime), with its manager's place to register in and its intents. It can also give a
    workspace the name a link to another machine's workspace names. workriver does not see
    either happen, and no request to workriver does it; you would see a workspace gone or
    renamed. Two workspaces that are both open under one name are refused instead (above).
    A workspace that gets a new id and a new name with nothing recorded in between (the
    runtime restarts and it is renamed before anything names it again) is not found again:
    it is a new workspace to workriver, and its old record is a closed one's, as here.
  - **A terminal the runtime knows as an agent's only by report can be released** and is
    then a plain terminal: yours, with the token. (Under Herdr, which agents are
    not such terminals: [runtimes/herdr.md](runtimes/herdr.md#security-under-herdr).)
  - On another machine the session goes to the first terminal that *calls* with it. A
    terminal where the runtime knows no shell is not remembered: its session shown by
    another terminal gets both refused.
- **Typing into another terminal.** Where the runtime lets any agent type into any terminal
  (Herdr does): a command typed into another agent's terminal is that agent's, one typed
  into a plain terminal is yours.
- **A background service that gives its commands a terminal**, started outside the runtime
  or with none of its terminals in its environment, is taken for you with the token: the
  terminal rule catches the plain case only.
- **An agent that runs outside the runtime at a terminal** (another terminal window) is you
  as far as the service can tell.
- **The files**: the token, the database, the attachments, `session-roots.json` and
  `user-sessions` can be read and written from disk without the service (`user-sessions`
  holds hashes: reading it gives no session; a line written into it counts after the
  service restarts).

If agents work on input you do not trust, run them in a sandbox or under another account,
where they cannot read `~/workriver-data` (they then cannot reach the caller socket either
and cannot record; that is not supported yet).

Other machines: workriver's own ssh connections never ask for anything, never forward your
ssh agent and never accept an unknown host key (`BatchMode`, `ForwardAgent=no`,
`StrictHostKeyChecking=yes`), whatever your ssh config says; connect once by hand to accept
a new host's key. The port that leads here from another machine is picked anew for every
connection and written there to `~/.config/workriver/port` (mode 600 under a 700
directory); a machine's listener therefore takes any port of `127.0.0.1` or `localhost` as
its Host, still refusing other names (a page served elsewhere cannot pose as it).

Reading through the service needs an identity too (you, or an agent in its terminal), but the
files themselves are readable by every program running as you.
