// Herdr's own words: what a refused caller is told about panes, and how an agent uses
// Herdr's CLI. Content only: nobody is anyone by what a sentence says.

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
pub fn no_pane_on(machine: &str, process: &str) -> String {
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
