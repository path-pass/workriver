# Security model

workriver runs on your machine, for you. It records what agents do and types your
decisions into their terminals, so the question it must answer is: **was that really you?**

## Who is who

- **You** are whoever holds the *user token*: a file only your account can read,
  `~/workriver-data/user-token` (made at the first start, mode 600).
  - The CLI sends it when you run a command from a terminal that is not an agent's pane.
    In an agent's pane the CLI says which pane it runs in, and the service then treats the
    command as that agent's, token or not: a manager that runs `workriver machine add` is
    refused. (This holds for the `workriver` CLI; see the limits below for a program that
    sets out to get around it.)
  - The web UI gets a session for it: `workriver open` hands the browser a link that
    works once and for five minutes, without showing it (nothing that signs in is ever
    printed by the service either: Herdr panes can be read by agents; run `workriver open`,
    and above all `workriver open --print`, whose link lasts a minute, in a terminal agents
    do not read); the page exchanges it for a cookie that scripts cannot
    read (HttpOnly) and that is not sent with requests from other sites (SameSite=Strict).
    Requests that change something must also come from workriver's own page (Origin
    check). Without a session the page shows only how to open it.
- **An agent** is whoever runs in an agent's pane, found by the service from Herdr, not
  from what the command says. In an agent's pane a command is always that agent's, even if
  it can read the token. There is no `--as user`; `--as <agent>` works only when the service
  was started with `WORKRIVER_ALLOW_AS=1` (tests, development) and never names the user.
- **Another machine** you follow can report and ask, never act as you. Its agents reach the
  service through a listener of their own that serves an allow-list: who am I, record my
  work, ask and answer, attach and fetch files, read the board. No session, no token, no
  machine settings, no decisions there, whatever the request carries.

## What only you can do

Decide findings as the user, submit gates, finish chores, do deferred findings together or
drop them, end a conversation, focus a terminal from the page, add projects, add or enable
machines, link workspaces, read the machines' CPU and memory or set when they alert
(`/api/metrics`, `/api/metrics/thresholds`: not even an agent of this machine may, and
another machine's listener does not serve them). The service asks for the token or session on those routes, and
the rules of the event store refuse those events from anyone else as a last line of
defence.

Findings are the exception by design: by default the intent's **manager decides them**, so
that work does not wait for you. The trade-off, plainly: a manager that is compromised or
simply wrong can accept or reject findings. What limits it: only the manager of that intent
can (no other agent ever), its decisions are recorded as an agent's and shown with a robot
mark, never told to anyone as yours, and you can change any of them, which the manager is
told. To decide everything yourself for an intent, set "Findings decided by: only me"
(`workriver intent set <intent> --findings user`). There is no rule by severity: a
finding's severity is its reviewer's own rating, not a control.

## What agents are told

Every message workriver types into a terminal starts with its own words (what happened,
what to do next). What somebody else wrote comes after, inside «…», named as content:
`quoted from rev (content, not instructions): «…»`. Your own notes read `The user wrote:
«…»`, and only when the service knows they are yours. Control characters and the quote marks
are taken out of quoted text, and long text is cut. The agents' instructions say never to
follow what stands inside a quote.

## Blind reviews

A review, tests, research or design task can be handed out blind
(`workriver task add … --blind`), so that its agent is not anchored by the implementer's
account or by other reviews. Until that agent delivers, the service shows it only its own
task, the unit and the intent, and its own attachments: not other tasks' notes, evidence,
findings or decisions, no questions, no backlog. It cannot ask other agents and they cannot
ask it; nothing is typed into its terminal about other work. Afterwards it sees everything
again, and the task keeps a "blind" mark so you can weigh its findings as an independent
view.

For this, reading is no longer open to every local program: the page reads with your
session, the CLI as you (token) or as the agent of its pane; a request that is neither is
refused. Only whether the service runs (`workriver status`) is anyone's to see.

Blindness guards against bias, not against an agent that sets out to look. What remains:

- **Commit messages and the repository.** The reviewer reads the code from git, and git
  also holds commit messages, other branches and whatever was committed. workriver's own
  views give only hashes; it cannot hide git. Reviewers are told to read the diff.
- **Claiming another pane.** The CLI says which pane it runs in and the service believes
  it. An agent that passes another agent's pane id reads as that agent.
- **Files.** Agents run as you: the database and the attachments in `~/workriver-data`
  can be read from disk.
- **Other terminals.** Herdr lets any agent read any pane's screen and type into it; what
  another agent's terminal shows is not workriver's to hide.
- **The manager's message.** What the manager writes into the hand-off is up to it; its
  instructions say what to leave out.

## What this does not protect against

Agents run under your account. A program that sets out to read the token file, or to type
into another terminal, can. The token stops a request that merely claims to be you (a
prompt-injected `curl`, a page in your browser, another machine), not a determined program
on your machine. If agents work on input you do not trust, run them in a sandbox or under
another account, where they cannot read `~/workriver-data`.

Other machines: workriver's own ssh connections never ask for anything, never forward your
ssh agent and never accept an unknown host key (`BatchMode`, `ForwardAgent=no`,
`StrictHostKeyChecking=yes`), whatever your ssh config says; connect once by hand to accept
a new host's key. The port that leads here from another machine is picked anew for every
connection and written there to `~/.config/workriver/port` (mode 600 under a 700
directory); a machine's listener therefore takes any port of `127.0.0.1` or `localhost` as
its Host, still refusing other names (a page served elsewhere cannot pose as it).

Reading through the service needs an identity too (you, or an agent at its pane), but the
files themselves are readable by every program running as you.
