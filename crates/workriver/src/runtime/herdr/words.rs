// Herdr's own words: what a refused caller is told about panes, and how an agent uses
// Herdr's CLI. Content only: nobody is anyone by what a sentence says.

/// The runtime's name, where a text must show it: the page's say `{{runtime}}`.
pub const NAME: &str = "Herdr";

/// Why the CLI at `place` cannot tell which agent it is.
pub fn no_agent_at(place: &str) -> String {
    format!("Herdr lists no agent in pane {place} (see `herdr agent list`); an agent runs this from its own pane; the user, from a terminal that is not an agent's, on the machine workriver runs on (where the user token is).")
}

/// The same when the service cannot ask Herdr.
pub fn cannot_ask(place: &str, why: &str) -> String {
    format!("cannot tell which agent runs in Herdr pane {place}: {why}. Is Herdr running and `workriver serve` started inside it? (As the user: use a terminal outside Herdr.)")
}

/// The recipe a refused caller is sent to: what to do when a command does not run in its pane.
const RECIPE: &str = "`workriver guide how hand-off`";

/// A request with the token but nothing that shows its process.
pub const NO_PROCESS: &str = "this request does not show which process sends it: use the `workriver` CLI of this version (the same build as the service: `workriver status`), or the page (`workriver open`)";

/// The command names one pane and its process runs in another, or in none.
pub fn elsewhere(said: &str, gone: bool, process: &str, runs_in: Option<&str>, machine: Option<&str>) -> String {
    let gone = if gone { format!(" (there is no pane {said} any more)") } else { String::new() };
    let on = machine.map(|m| format!(" on machine {m}")).unwrap_or_default();
    let really = match runs_in {
        Some(p) => format!("in pane {p}{on}"),
        None => format!("in no pane of the Herdr workriver follows{on}"),
    };
    format!("this command says it runs in pane {said}{gone}, but its process {process} runs {really}; workriver takes the agent from the pane the command really runs in, never from what it says. An agent runs workriver from its own pane; if its commands do not run there, see {RECIPE}")
}

/// A caller in no pane and without a terminal, with the user token.
pub fn no_terminal(process: &str) -> String {
    format!("process {process} is in no pane and has no terminal: workriver cannot tell whose command this is. As the user: a terminal window, or the page (`workriver open`). An agent: from its own pane; if its commands do not run there, see {RECIPE}")
}

/// A caller on another machine whose process is in no pane there.
pub fn in_no_place_on(machine: &str, process: &str) -> String {
    format!("process {process} is in no pane of the Herdr workriver follows on machine {machine}: an agent there runs workriver from its own pane; if its commands do not run there, see {RECIPE}")
}

/// A process on another machine that is gone, or is not the CLI.
pub fn not_the_cli(machine: &str, pid: Option<u32>, why: &str) -> String {
    let which = pid.map(|p| format!("process {p}")).unwrap_or_else(|| "the calling process".into());
    format!("{which} on machine {machine} {why}: workriver answers the `workriver` CLI of this version there, run by an agent from its own pane")
}

/// The session a pane shows is held by another pane whose root still runs.
pub fn held(session: &str, pane: &str, machine: Option<&str>) -> String {
    let on = machine.map(|m| format!(" on machine {m}")).unwrap_or_default();
    format!("session {session} was first seen in pane {pane}{on}, whose shell still runs; one session is one pane: close that pane, or start a new session here")
}

/// A session that holds a manager's seat shows in a pane that is not where it was seated.
pub fn seat_elsewhere(session: &str, pane: &str, machine: Option<&str>, folder: &str, seat_machine: Option<&str>, seat_folder: &str) -> String {
    let on = |m: Option<&str>| m.map(|m| format!(" on machine {m}")).unwrap_or_default();
    format!(
        "session {session} holds a manager's seat, given to it in {seat_folder}{}; pane {pane} shows it in {folder}{}. A manager's new terminal is the manager only once the user confirms it: ask the user (the workspace's line in the web UI, Manager, or `workriver workspace manager <ws> {session}` from the user's own terminal)",
        on(seat_machine),
        on(machine)
    )
}

/// Two panes show one session nobody was seen with before.
pub fn twice(session: &str, here: &str, there: &str) -> String {
    format!("session {session} shows in pane {here} and in pane {there} at once, and workriver has seen it in neither before: it takes it from neither. One session is one pane: close one of them, or start a new session in one")
}

/// How an agent may ask another agent one question through Herdr's own CLI.
pub fn ask_hint(agent: &str) -> String {
    format!(
        "If {agent} (the agent that did the work) is idle - check with `herdr agent get {agent}` - you may ask it one short question with `herdr agent prompt {agent} \"...\" --wait --timeout 120000` and read the reply with `herdr agent read {agent} --source recent-unwrapped --lines 80`; never while it is working or blocked. If you asked it, add `--asked {agent}` to your answer."
    )
}

/// How a manager looks at a stopped worker's terminal and gets it going, through Herdr's own CLI.
pub fn stall_hint(agent: &str) -> String {
    format!("Look at its terminal (`herdr agent read {agent} --source recent-unwrapped --lines 80`) and either get it going (`herdr agent prompt {agent} \"...\"`),")
}

/// Why a followed machine's agents cannot be read, when nothing answers on its socket.
pub const REMOTE_UNREACHABLE: &str = "its Herdr cannot be reached (no connection, or Herdr is not running there)";

// ---- what the service says ----

/// `--as` where the service does not allow it.
pub const AS_OFF: &str = "--as is off: workriver takes the agent from the pane the command really runs in (for tests, start the service with WORKRIVER_ALLOW_AS=1)";

/// The service was started allowing `--as`: said once, in its log.
pub const AS_ALLOWED: &str = "WORKRIVER_ALLOW_AS is set: any local caller may record as any agent (--as), and a command in no pane needs no terminal to be the user's. For tests and development only.";

/// An agent asks for what only the user does.
pub const ONLY_THE_USER: &str = "only the user does this: in the web UI, or with the CLI from a terminal that is not an agent's: a new terminal window, or a Herdr pane with no agent (`!` in an agent's session runs in the agent's pane and does not count). Ask the user to run it there; give them the exact command";

/// A request that shows neither the user nor an agent.
pub const NOT_THE_USER: &str = "workriver answers the user, and agents from their own Herdr pane; this request does not show it comes from either. As the user: in the browser, open workriver with `workriver open`; with the `workriver` CLI, use a terminal that is not an agent's pane, on the machine workriver runs on (it reads the user token there).";

/// Why nothing can be handed to an agent while the runtime does not answer.
pub const NOT_CONNECTED: &str = "the agent runtime (Herdr) is not connected";

/// An intent with nowhere to start an agent for it.
pub fn no_group(intent: &str) -> String {
    format!("neither the manager of intent {intent} nor the task's agent runs in Herdr: there is no terminal to open an agent beside")
}

// ---- what the CLI says ----

/// `--as user`, which names nobody any more.
pub const AS_USER_GONE: &str = "--as user is gone: the user is whoever runs the CLI from a terminal that is not an agent's pane, on the machine workriver runs on";

/// `workriver open`, run by an agent.
pub const OPEN_IS_THE_USERS: &str = "`workriver open` is the user's: run it from a terminal that is not an agent's pane";

/// Before `workriver open --print` shows its link.
pub const LINK_ON_A_SCREEN: &str = "Anyone who can read this terminal can open workriver as you with this link (agents read Herdr panes). It works once, for 60 seconds.";

/// A machine whose connection is up while its agents cannot be read, in `machine list`.
pub const REMOTE_SILENT: &str = "connected, but its Herdr does not answer";

// ---- the CLI's help ----

/// In `workriver --help`: who a command is recorded as.
pub const WHO_RECORDS: &str = "Who records: the service finds it, from the process that runs this command (the kernel
tells the service which process connected; the pane the CLI names is only cross-checked).
A command run in an agent's Herdr pane is that agent's (by its session). One run in a pane
with no agent, or in a terminal window outside Herdr, on the machine workriver runs on, is
the user's (the CLI sends the user token, `<WORKRIVER_HOME>/user-token`). Anything else is
refused: a command that names a pane it does not run in, one with no terminal in no pane,
a request that is not this CLI's. Nobody is anyone by saying so. Decisions as the user, gates, chores done, projects, machines, workspaces and their
managers' seats are the user's alone. A workspace is workriver's own (ws1…), not Herdr's:
an agent's is the one it manages, else the one its tasks are in, and commands that take an
intent may then leave it out (that workspace's current one).";

pub const HELP_AS: &str = "For tests and development: record as this agent instead of the one in this Herdr pane. Refused unless the service was started with WORKRIVER_ALLOW_AS=1; never the user";
pub const HELP_SERVE: &str = "Run the daemon: records events, reads git, follows the agents in Herdr, serves the web UI. Data lives in $WORKRIVER_HOME (default ~/workriver-data)";
pub const HELP_OPEN: &str = "Open the web UI in the browser as the user: with a one-time link that gives the page its session. Not from an agent's pane";
pub const HELP_MACHINE: &str = "Other machines whose agents (in their own Herdr) are followed over ssh";
pub const HELP_WORKSPACE: &str = "Workspaces: workriver's own (ws1…), each in a project, with one manager the user seats; not Herdr's workspaces, which decide nothing";
pub const HELP_MANAGER: &str = "The manager: the one agent per workspace, seated by the user, that splits work and hands it out";
pub const HELP_TASK_SHOW: &str = "Everything about one task that you may see, in full: what it asks, where the work is, the findings it must answer with all that was added to them and every decision, steps, its output, commits and questions. Read it before you start a task. Without a task, in an agent's pane: the task that agent is on";
pub const HELP_CHORE_INTENT: &str = "The intent it belongs to (i1…); default: the current intent of your workspace";
pub const HELP_UNIT_INTENT: &str = "The intent it serves (i1…); default: the current intent of your workspace";

/// `workriver machine add`'s option for where the runtime's socket is over there: its id,
/// its flag and its help.
pub const SOCKET_ARG: &str = "herdr_socket";
pub const SOCKET_FLAG: &str = "herdr-socket";
pub const HELP_SOCKET: &str = "Herdr's socket over there; default: asked of the machine ($HERDR_SOCKET_PATH, else ~/.config/herdr/herdr.sock)";
