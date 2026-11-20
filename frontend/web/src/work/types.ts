// The work of a unit: its tasks as a graph, what they found, and what the user puts in.
import type { Sent, Thread, Turn } from '@/conversations/types'
import type { External } from '@/planning/types'
import type { Attachment } from '@/attachments/types'
import type { Choice } from '@/content/choice/surface'

/** One thing to do by hand, with the command for it when there is one. */
export interface Step {
  text: string;
  command?: string;
}

export interface Chore {
  id: string;
  /** A chore of no intent: its project's folder ('' when it belongs to no project). */
  project?: string;
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

/**
 * implement  work an agent does
 * review     a look at that work
 * gate       work only the user can do, which an agent cannot continue without
 */
export type TaskKind = 'implement' | 'review' | 'gate';

/** What a gate asks of the user. More kinds may follow. */
/** decide: the user discusses a design (asking why) and confirms it; the note is the decision. */
export type GateKind = 'deploy' | 'test' | 'decide';

/**
 * done         finished
 * running      an agent is on it now
 * todo         handed out, not started
 * waiting      gates only: the user has not done it yet
 * unregistered a commit that names no node: a gap in the record
 * dropped      no longer needed (kept, never deleted); nothing waits on it
 */
export type TaskState = 'done' | 'running' | 'todo' | 'waiting' | 'unregistered' | 'dropped';

export type Severity = 'P1' | 'P2' | 'P3';

/**
 * open: waiting for the user's decision. accepted: work was handed out for it. revised: to
 * be fixed the user's way (their note says how), work handed out like accepted.
 * rejected: decided against. deferred: put off, waiting in the project's backlog.
 * scheduled: picked with others to be done together. done: the work it caused was
 * delivered. dropped: taken out of the backlog; it will not be done.
 */
export type FindingState = 'open' | 'accepted' | 'revised' | 'rejected' | 'deferred' | 'scheduled' | 'done' | 'dropped';

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
  /** When it was last decided. */
  decided?: string;
  /** While open: what the agent suggests, with one sentence of why. */
  recommendation?: { decision: 'accept' | 'reject' | 'defer'; reason: string };
  /** The user's one line with a decision. */
  note?: string;
  /** Pictures that show it (screenshots from a manual test). */
  attachments?: Attachment[];
  /** Where the message with the user's decision is. */
  delivery?: Sent;
  external?: External;
  /** Since when it is known: its task's delivery, or when it was added to the backlog. */
  since?: string;
  /** Dropped: why. */
  reason?: string;
  /** While open: 'manager' when it is the intent's manager's to decide (and one is running). */
  decider?: 'manager';
  /** While open: it would be the manager's, but none is registered or running: it falls to the user. */
  noManager?: boolean;
  /** What was added to it since it was reported, oldest first. */
  notes?: { by: string; at: string; text: string; attachments?: Attachment[] }[];
}

/** A check run on a piece of work: the command and its result, e.g. "12 passed". */
export interface Evidence {
  command: string;
  result: 'passed' | 'failed';
  summary: string;
  /** A screenshot or report of the check. */
  attachment?: Attachment;
}

export type Reviewer = 'security-review' | 'code-review' | 'architecture-review' | 'test';

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
  /** The agent's session, to tell agents of the same name apart; absent in the sample. */
  agentSession?: string | null;
  /** True while the agent has uncommitted changes in its working directory. */
  writing: boolean;
  /** Who raised an implement node. 'plan' means the original breakdown. */
  raisedBy?: Reviewer | 'user' | 'plan';
  /** Gate nodes only. */
  gate?: GateKind;
  /**
   * Gate nodes only: a blocker. An agent's running task got stuck on something only the user
   * can do (a refused command, a missing permission); this is the id of that task, which has
   * the gate as an extra non-primary input.
   */
  blocks?: string;
  /**
   * When the node came to wait for the user: a gate became waiting, a review or gate with
   * open findings finished. Absent when nothing waits.
   */
  since?: string;
  /** Questions asked about this work and their answers, oldest first (the sample's form). */
  thread?: Turn[];
  /** Conversations about this work, oldest first (from the service). */
  threads?: Thread[];
  /** A design task: implement work whose output is its design note. */
  design?: boolean;
  /** A research task: implement work whose output is its note and attached report. */
  research?: boolean;
  /** Done without seeing other work (notes, evidence, other findings): an independent view. */
  blind?: boolean;
  /** It has no commits on the unit's branch now, but had these on a branch the unit left. */
  lostCommits?: string[];
  /** Waiting gates: the latest conclusion about it or the work it follows, to start the note from. */
  draft?: string;
  /**
   * Running, and its agent stopped without delivering: idle since `since` (or gone), when it
   * was reminded, and whether that did not help (then it is the user's and the manager's).
   */
  stalled?: { since: string; gone?: boolean; reminded?: string; escalated?: boolean };
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
    /** A decide gate's options (the manager's), which the user picks from. */
    choice?: Choice;
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
    /** An answered decide gate: the option picked (its value). */
    selected?: string[];
  };
  /** Paths the work still expects to change (files or folders, possibly not existing yet). */
  plans: string[];
  /** Gates: where the message with the user's result is. */
  delivery?: Sent;
  /** When its output was last corrected after delivery. */
  amended?: string;
  /** When its agent started it, and when it was delivered (a gate: submitted). */
  started?: string;
  delivered?: string;
  /** Dropped as no longer needed: when, by whom ("user", or an agent's name), why. */
  dropped?: { at: string; by: string; reason: string };
}

/** What is picked inside a unit: one piece of work, or one finding (by its id) of a review. */
export type Pick = { kind: 'task'; id: string; finding?: string } | null

// ---- what the user puts in: gate results, decisions on findings, chores done ----

export type Decision = 'accept' | 'revise' | 'reject' | 'defer'

export interface NewFinding {
  severity: Severity
  problem: string
  impact: string
  evidence: string[]
  attachments: Attachment[]
}

export interface GateResult {
  /** answered: a gate with options, one picked (`selected`). */
  result: 'done' | 'problem' | 'answered'
  selected?: string[]
  note: string
  attachments: Attachment[]
  findings: NewFinding[]
  at: string
}
