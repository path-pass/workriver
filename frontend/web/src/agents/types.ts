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
