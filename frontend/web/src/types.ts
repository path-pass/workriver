// The shape of what the UI shows: what the workriver service answers on /api/workspace,
// and what src/mock/demo.json holds for the sample.

export interface Workspace {
  /** Every repository a unit works in. */
  repos: Repo[];
  builtAt: string;
  intents: Intent[];
  /** One per agent. What it is working on comes from the tasks; only this does not. */
  sessions: Session[];
}

/**
 * A high-level goal and the units that serve it. (In the runtime one intent is one Herdr
 * workspace; the UI only says intent.)
 */
export interface Intent {
  id: string;
  title: string;
  /** One sentence on what it is for, set by its manager or the user; may be empty or absent. */
  goal?: string;
  units: Unit[];
  /** Things agents left for the user that block nobody. Never on a graph. */
  chores: Chore[];
  /** The agent that splits the intent's work and hands it out; absent when none registered. */
  manager?: string;
  /** The manager's state in the runtime, or 'gone' when it is not running. */
  managerState?: RuntimeState | 'gone';
}

/** An agent's state as the runtime (Herdr) reports it. */
export type RuntimeState = 'working' | 'idle' | 'done' | 'blocked' | 'unknown';

export interface Repo {
  /** Where it is on disk; a unit names its repository by this. */
  path: string;
  name: string;
  /** The code as a tree of folders and files. The root has an empty path. */
  modules: ModuleNode;
}

export interface Session {
  agent: string;
  /** The session is gone: the agent is not shown anywhere any more. */
  ended: boolean;
  /** While it lives: its state in the runtime. */
  state?: RuntimeState;
  /** Id of the intent it works in. */
  intent?: string;
  /** It is the manager of that intent. */
  manager?: boolean;
}

/**
 * Where the service's message about a user's input (a decision, a gate result) is: not
 * delivered (with why), delivered, or picked up (the receiver started working on it).
 */
export interface Sent {
  step: 'not-delivered' | 'delivered' | 'picked';
  /** The receiving agent. */
  to: string | null;
  toManager: boolean;
  /** Why it was not delivered; shown in the user's language. */
  reasonCode?: 'no-manager' | 'manager-gone' | 'runtime-down' | 'runtime-error';
  /** The same in the service's words (for a runtime error, the runtime's own). */
  reason?: string;
  /** The message as sent. */
  message: string;
}

/** One thing to do by hand, with the command for it when there is one. */
export interface Step {
  text: string;
  command?: string;
}

export interface Chore {
  id: string;
  title: string;
  /** The agent that asks for it. */
  by: string;
  steps: Step[];
  state: 'open' | 'done';
  /** When it was asked for. */
  since: string;
  /** Id of the unit it belongs to; absent when it concerns the intent as a whole. */
  unit?: string;
}

export interface ModuleNode {
  name: string;
  /** Repository-relative path; '' for the root. */
  path: string;
  /** Lines of code under this node. */
  loc: number;
  /** Number of source files under this node (1 for a file). */
  files: number;
  /** Present on folders (possibly empty), absent on files. */
  children?: ModuleNode[];
}

export type UnitStatus = 'active' | 'planned' | 'merged';

/** A piece of an intent that can be implemented on its own, on its own branch. */
export interface Unit {
  id: string;
  title: string;
  /** One sentence on what the unit is for; may be empty. */
  purpose: string;
  /** Path of its repository, one of `Workspace.repos`. */
  repo: string;
  /** The branch the work happens on; null once merged. */
  branch: string | null;
  status: UnitStatus;
  /** The work handed out, as a graph. Empty for units that predate recording. */
  tasks: Task[];
  /** Oldest first. The last one may be uncommitted work (`live`). */
  commits: Commit[];
}

/**
 * implement  work an agent does
 * review     a look at that work
 * gate       work only the user can do, which an agent cannot continue without
 */
export type TaskKind = 'implement' | 'review' | 'gate';

/** What a gate asks of the user. More kinds may follow. */
export type GateKind = 'deploy' | 'test';

/**
 * done         finished
 * running      an agent is on it now
 * todo         handed out, not started
 * waiting      gates only: the user has not done it yet
 * unregistered a commit that names no node: a gap in the record
 */
export type TaskState = 'done' | 'running' | 'todo' | 'waiting' | 'unregistered';

export type Severity = 'P1' | 'P2' | 'P3';

/**
 * open: waiting for the user's decision. accepted: work was handed out for it.
 * rejected: decided against. deferred: put off, to be raised again later.
 */
export type FindingState = 'open' | 'accepted' | 'rejected' | 'deferred';

/**
 * One problem a review, or the user at a test gate, found. A decision on it is not a node:
 * it stays inside the node that found it.
 */
export interface Finding {
  /** Unique within its node, shown to the user, e.g. 'P1-2'. */
  id: string;
  severity: Severity;
  problem: string;
  /** What happens if it is left; may be empty. */
  impact: string;
  /** File paths that show the problem. */
  evidence: string[];
  state: FindingState;
  /** Ids of the nodes handed out for it. Only an accepted finding has any; it may have several. */
  leadsTo: string[];
  /** Who made the decision; absent while the finding is open. An agent may decide on its own. */
  decidedBy?: 'agent' | 'user';
  /** While open: what the agent suggests, with one sentence of why. */
  recommendation?: { decision: 'accept' | 'reject' | 'defer'; reason: string };
  /** The user's one line with a decision. */
  note?: string;
  /** Pictures that show it (screenshots from a manual test). */
  attachments?: Attachment[];
  /** Where the message with the user's decision is. */
  delivery?: Sent;
}

/** A check run on a piece of work: the command and its result, e.g. "12 passed". */
export interface Evidence {
  command: string;
  result: 'passed' | 'failed';
  summary: string;
}

/** A picture the user attached. Only images for now. */
export interface Attachment {
  id: string;
  name: string;
  /** Where the page can show it from (an object URL while the service does not store it). */
  url: string;
}

export type Reviewer = 'security-review' | 'code-review' | 'test';

export interface TaskInput {
  /** Id of the node this one follows, or 'start' for work cut straight from the unit. */
  from: string;
  /** The direct cause. Others (primary: false) are merely involved; drawn dashed. */
  primary: boolean;
}

/** One piece of work handed to one agent (implement, review) or to the user (gate). */
export interface Task {
  id: string;
  kind: TaskKind;
  state: TaskState;
  /** The task number shown as #n; null for reviews and unregistered commits. */
  taskId: string | null;
  /** What the work is; empty for reviews (their label is built from reviewer and result). */
  title: string;
  agent: string | null;
  /** True while the agent has uncommitted changes in its working directory. */
  writing: boolean;
  /** Who raised an implement node. 'plan' means the original breakdown. */
  raisedBy?: Reviewer | 'user' | 'plan';
  /** Gate nodes only. */
  gate?: GateKind;
  /**
   * When the node came to wait for the user: a gate became waiting, a review or gate with
   * open findings finished. Absent when nothing waits.
   */
  since?: string;
  /** Questions asked about this work and their answers, oldest first. */
  thread?: Turn[];
  /** Review nodes only. */
  reviewer?: Reviewer;
  /** Review nodes only: found problems, found none, or still running. */
  result?: 'found' | 'passed' | 'running';
  inputs: TaskInput[];
  input: {
    text: string;
    /**
     * Which findings of which review (or gate) nodes the work answers. Empty for work cut
     * straight from the unit or asked for by the user.
     */
    causes: { review: string; findings: string[] }[];
    /** Gate nodes only: what the agent prepared for the user to do. */
    steps?: Step[];
  };
  output: {
    /** Implement nodes: ids of the commits that carry the work out. */
    commits?: string[];
    /** Review nodes, and gates where the user found something: what was found. */
    findings?: Finding[];
    /** Gate nodes: what the user reported back, in a few words. */
    note?: string;
    /** Gate nodes: pictures with the note. */
    attachments?: Attachment[];
    /** Implement and tests tasks: the commands run to check the work, and what came of them. */
    evidence?: Evidence[];
  };
  /** Paths the work still expects to change (files or folders, possibly not existing yet). */
  plans: string[];
  /** Gates: where the message with the user's result is. */
  delivery?: Sent;
  /** When its output was last corrected after delivery. */
  amended?: string;
}

/** What a question is about, shown as a chip with it. */
export interface AskContext {
  kind: 'task' | 'finding' | 'file' | 'lines';
  label: string;
  /** file and lines: the repository path. */
  path?: string;
}

/**
 * One question about a piece of work and its answer. A temporary agent answers from the
 * records and the code; it asks the agent that did the work only when that agent is idle.
 */
export interface Turn {
  question: string;
  context: AskContext[];
  /** Null while it comes. */
  answer: string | null;
  /** While it comes: how far it has got. */
  step?: AskStep;
  /** The agent that did the work: asked, or not asked because it was busy or gone. */
  consulted?: { agent: string; outcome: 'asked' | 'busy' | 'ended' };
  at: string;
}

export type AskStep = 'starting' | 'reading' | 'asking';

export interface Commit {
  /** Short hash, or 'uncommitted' for work not committed yet. */
  id: string;
  subject: string;
  date: string;
  author: string;
  live: boolean;
  files: FileChange[];
}

export interface FileChange {
  path: string;
  status: 'A' | 'M' | 'D';
  /** The whole file before this commit; null when the commit adds it. */
  before: string | null;
  /** The whole file after this commit; null when the commit deletes it. */
  after: string | null;
  /** Derived on load from before and after: */
  add: number;
  del: number;
  /** Unified-diff lines including `@@ -a,b +c,d @@ context` hunk headers. */
  lines: string[];
}
