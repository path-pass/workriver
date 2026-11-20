// workriver's agents and the runtime workspaces they run in.

/** A runtime (Herdr) workspace, of this machine or of another. */
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

/** An agent's state as the runtime (Herdr) reports it. */
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
  /** The runtime workspace (in Herdr) it runs in; a remote one not linked names its machine. */
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
