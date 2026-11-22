// workriver's agents and the runtime workspaces they run in.

/** A workspace of the agent runtime, of this machine or of another. */
export interface RuntimeWorkspace {
  id: string;
  label: string;
  /** Absent: this machine. */
  machine?: string;
  /** A remote one: the label of the workspace of this machine it is one with. */
  linkedTo?: string;
  /** The project (its folder) it works on, its title, and how workriver knows; absent: unknown. */
  project?: string;
  projectTitle?: string;
  /** manager: its manager runs in the project's folder; intents: its current intent's; user: set by the user. */
  projectFrom?: 'manager' | 'intents' | 'user';
}

/** An agent's state as the runtime reports it. */
export type RuntimeState = 'working' | 'idle' | 'done' | 'blocked' | 'unknown';

export interface Session {
  /** The name shown; it may repeat across runtime workspaces. */
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
  /** The runtime workspace it runs in; a remote one not linked names its machine. */
  group?: { id: string; label: string; machine?: string };
  /** The machine it runs on; absent: this one. */
  machine?: string;
  /** Its machine cannot be reached: shown as last seen. */
  unreachable?: boolean;
  /** The session is gone: the agent is not shown anywhere any more. */
  ended: boolean;
  /** While it lives: its state in the runtime. */
  state?: RuntimeState;
  /** Id of the current intent of the workspace it runs in. */
  intent?: string;
  /** It is the manager of its workspace. */
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

/** An agent as the page asks for its gate or terminal view. */
export interface AgentRef { name: string; session?: string | null }
