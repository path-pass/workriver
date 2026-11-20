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
