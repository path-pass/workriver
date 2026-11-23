// workriver's agents, and the workspaces (workriver's own) they work in.

/** A manager's seat: the session the user seated in a workspace, and where it was seated. */
export interface Seat {
  session: string;
  /** The name it goes by. */
  name: string;
  /** Absent: this machine. */
  machine?: string;
  /** The folder it was seated in. */
  folder: string;
  /** The appointment: what a change of the seat names as the seat it replaces. */
  since: number;
  /** Its state in the runtime, or 'gone' when it is not running. */
  state: RuntimeState | 'gone';
}

/**
 * A workspace: workriver's own record (`ws1`), in one project, with one manager the user
 * seats. Its name is shown only and may repeat; everything names it by its id. Nothing of
 * the agent runtime identifies it.
 */
export interface Space {
  id: string;
  name: string;
  /** Its project's folder (a `Project.id`). */
  project: string;
  /** Closed by the user: not in the rail; its intents and conversation stay, read-only. */
  closed?: boolean;
  /** The intent new work goes to. */
  current?: string;
  manager?: Seat;
  /** Its manager does not run where it was seated: the session to offer for the seat (the
   *  seated one in another terminal, or the agent now in the old terminal). */
  proposal?: string;
  /** The user's last start of a manager here, until the next start or seating: pending while
   *  it has no `reason`, else how it ended. */
  start?: ManagerStart;
}

/** Why a start of a manager ended unseated (a key; the page says it in words). */
export type StartEnd = 'runtime' | 'no_agent' | 'no_session' | 'refused' | 'restarted' | 'startup_error'

/** A manager started from the page: when, with what, under which name its agent runs until it
 *  has a session; how it ended, with the runtime's or the rules' words. */
export interface ManagerStart {
  since: string;
  harness: string;
  /** Absent or empty: the harness's own default. */
  model?: string;
  name: string;
  reason?: StartEnd;
  words?: string;
}

/** A harness a manager can be started with, as this machine has it: offered (no `why`), or
 *  why not (not installed; the runtime's hook for it missing or old; the runtime not answering). */
export interface HarnessOffer {
  kind: string;
  /** The model names the harness resolves itself, offered as choices. */
  aliases: string[];
  why?: 'not_installed' | 'not_integrated' | 'runtime';
}

/** The default harness and model of a start, with every harness as offered here. */
export interface StartSettings {
  harness: string;
  /** Empty: the harness's own default. */
  model: string;
  harnesses: HarnessOffer[];
}

/** An agent's state as the runtime reports it. */
export type RuntimeState = 'working' | 'idle' | 'done' | 'blocked' | 'unknown';

export interface Session {
  /** The name shown; it may repeat. */
  agent: string;
  /** The session's own id, unique; absent in the sample. */
  session?: string;
  /** What the session is about (its terminal title), while it lives. */
  topic?: string;
  /** Waiting for a person in its terminal (a question, an approval) since then. */
  blockedSince?: string;
  /** It waits because its screen shows a question the runtime does not report: counted and
   *  shown as blocked (it has `blockedSince`), with one quiet line on its gate. */
  onScreen?: boolean;
  /** Where its terminal is in the runtime: a quiet hint, never a workspace. */
  group?: { id: string; label: string; machine?: string };
  /** Its workspace by the records (a `Space.id`): the one whose seat it holds, else the one
   *  workspace of the tasks it has. */
  workspace?: string;
  /** The folder it was started in, while it lives. */
  folder?: string;
  /** The machine it runs on; absent: this one. */
  machine?: string;
  /** Its machine cannot be reached: shown as last seen. */
  unreachable?: boolean;
  /** The session is gone: the agent is not shown anywhere any more. */
  ended: boolean;
  /** While it lives: its state in the runtime. */
  state?: RuntimeState;
  /** Id of the current intent of its workspace. */
  intent?: string;
  /** It holds its workspace's seat. */
  manager?: boolean;
  /** The harness the runtime names (claude, codex), when known. */
  harness?: string;
  /** Idle (or done) in its terminal since then, while it lives. */
  idleSince?: string;
}

// ---- an agent's screen, as the service reads it, and the keys the user may press ----

/** The keys the service will ever send. */
export type Key = 'up' | 'down' | 'enter' | 'escape' | 'tab' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9'
export const KEYS: Key[] = ['up', 'down', 'enter', 'escape', 'tab', '1', '2', '3', '4', '5', '6', '7', '8', '9']

/** One option of a dialog read off a screen. */
export interface DialogItem {
  number: number | null
  words: string
  /** The lines under it. */
  more: string[]
  /** The selection marker is on it. */
  marked: boolean
  row: number
}

/** A key the dialog names at its bottom ("Esc to cancel"). */
export interface DialogHint { key: Key; words: string }

/** A dialog read off a screen. `top` and `bottom` are the rows (inclusive) of the region to show. */
export interface Dialog {
  options: DialogItem[]
  /** Answered by its digit; otherwise a marked list, answered by arrows then Enter. */
  numbered: boolean
  hints: DialogHint[]
  above: string | null
  question: string | null
  top: number
  bottom: number
  /** The region was cut at twelve lines: the whole screen must be read. */
  cut: boolean
}

/** An agent's visible screen: plain text, one line per row, the marks that reorder or hide
 *  text already made visible by the service. Kept in memory only, never stored. */
export interface ScreenView {
  text: string
  cols: number
  rows: number
  /** Binds a press to this screen: the service sends nothing when its screen differs. */
  digest: string
  dialog: Dialog | null
  /** The runtime's reason, when it reports the agent blocked. */
  why?: string
  waiting?: 'blocked' | 'screen'
  since?: string
}

/** What the user presses: an option by its place in the dialog (1-based), or one key. */
export type Press = { option: number } | { key: Key }

export type Outcome = 'answered' | 'open' | 'new' | 'sent' | 'changed' | 'unsure'

/** What a press did, with the screen after it (unless answered). */
export interface Pressed { outcome: Outcome; keys: string[]; screen?: ScreenView }

/** A refusal of the screen, keys or view routes, with the service's words. */
export interface Refusal { code: string; reason: string; screen?: ScreenView }

/** An agent as the page asks for its gate or terminal view: its session and its machine are
 *  one pair (the same session can live on two machines); absent machine: this one. */
export interface AgentRef { name: string; session?: string | null; machine?: string }
