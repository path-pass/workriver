// The commands the user runs from the page, as the service shows them.

/** A command the user ran from the page; its output is read with `/api/run/{id}`. */
export interface RecentRun {
  id: string;
  command: string;
  dir: string;
  machine?: string;
  unit?: string;
  /** Whose step it was (`u1#3`, `c2`) and which step (from 1); none: typed in the Run section. */
  from?: string;
  step?: number;
  at: string;
  /** running: its terminal is read; done: it ended (`exit`); unknown: no longer followed (the service restarted, or its terminal closed) and no end seen. */
  state: 'running' | 'done' | 'unknown';
  exit?: number;
  /** Its terminal can be brought to the front. */
  focusable: boolean;
  /** What the review of it found, if it has one. */
  verdict?: Verdict;
}

/** What workriver's review of a command found: ok (nothing risky), caution (worth a look), danger (destructive or irreversible). */
export type Verdict = 'ok' | 'caution' | 'danger';

/** A command to run as the user, and where: a unit's work tree, or a project's folder. */
export interface RunAsk {
  command: string
  /** A run already made (from the recent runs): shown, not prepared again. */
  id?: string
  unit?: string
  project?: string
  /** Whose step it is: a task (`u1#3`) or a chore (`c2`); none: the user's own. */
  from?: string
  /** Which of its steps (from 1). */
  step?: number
  /** For the sample only: who wrote it, the folder and machine it would run in. */
  by?: string
  dir?: string
  machine?: string
}

/** A run as the service shows it: what the confirm dialog shows, then what its pane shows. */
export interface RunView {
  id: string
  command: string
  dir: string
  machine?: string | null
  /** "user", or the agent that wrote it. */
  by: string
  started: boolean
  output: string
  exit: number | null
  /** Its pane is still read. */
  watching: boolean
  /** False: recorded before the service restarted; its output was not kept. */
  kept?: boolean
  /** Its terminal can be brought to the front. */
  focusable?: boolean
  /** What a workriver agent says it will do; none for a run recorded without one. */
  review?: RunReview | null
}

/** A command's review: being written, written (a verdict and short Markdown), or given up. */
export type RunReview =
  | { state: 'pending' }
  /** Its agent waits for a person in its terminal (a first-run prompt): `session` to go there. */
  | { state: 'blocked'; agent: string; session: string }
  | { state: 'done'; verdict: Verdict; text: string }
  | { state: 'failed'; reason: string }
