// Conversations: questions about work and their answers, the user's messages to managers,
// and where a message is on its way.
import type { Attachment } from '@/attachments/types'
import type { Lines } from '@/code/types'
import type { Intent } from '@/planning/types'
import type { Session } from '@/agents/types'
import type { Choice } from '@/content/choice/surface'

/** What the user's message answers: a manager's message (`m7`) or its `reply`-th reply, and the value picked. */
export interface Answer {
  message: string
  reply?: number
  selected: string[]
}

/** A manager's question with options (`say` or `reply` with `--option`): its options, the
 *  user's answer (by which message), and whether the user can still answer it (not answered,
 *  and asked by the workspace's manager now). */
export interface Asked {
  choice?: Choice
  answered?: { by: string; selected: string[] }
  open?: boolean
}

/** The user's message to a workspace's manager, typed into its terminal; the manager replies with `workriver reply`. Or one the manager started (`workriver say`). Or one to every manager (`to`). */
export interface ManagerMessage extends Asked {
  /** "m3"; a local one the service refused has none. */
  id?: string;
  /** The intent it is about, if any. */
  intent?: string;
  /** The runtime workspace whose manager it went to, by label; `EVERY` (empty): to every manager. */
  workspace: string;
  /** The manager's name; empty for one to every manager. */
  manager: string;
  text: string;
  /** What the user picked with it: the unit, one of its tasks, and the rest, shown as chips. */
  unit?: string;
  task?: number;
  context?: AskContext[];
  /** The stored files its context points at (the page's own copies with the sample), to open. */
  attachments?: Attachment[];
  at: string;
  /** blocked: the manager waits for a person in its terminal; the message goes in once it is free.
   *  held: the manager is on a blind task; the message goes once that task is delivered (`reason` says so). */
  step: 'sending' | 'blocked' | 'held' | 'delivered' | 'not-delivered';
  reasonCode?: ReasonCode;
  reason?: string;
  blocked?: { agent: string; session: string };
  /** Each reply is the manager's; unread: written after the user last read this workspace's conversation. */
  replies: ({ by: string; text: string; at: string; unread?: boolean } & Asked)[];
  /** To every manager: each copy, where it is, with its manager's replies (`replies` then empty). */
  to?: Copy[];
  /** The user's message that answers a manager's question (then `text` is the user's note). */
  answer?: Answer;
  /** Started by the manager (`workriver say`): `text` is the manager's, and nothing above it is the user's. */
  byManager?: boolean;
  /** The manager's message (byManager), not read yet. */
  unread?: boolean;
}

/** One manager's copy of the user's message to every manager. */
export interface Copy {
  /** The workspace it manages, by label, and its machine (absent: this one). */
  workspace: string;
  machine?: string;
  /** The manager's name. */
  manager: string;
  step: ManagerMessage['step'];
  reasonCode?: ReasonCode;
  reason?: string;
  blocked?: { agent: string; session: string };
  replies: ManagerMessage['replies'];
}

/**
 * Where the service's message about a user's input (a decision, a gate result) is: not
 * delivered (with why), delivered, or picked up (the receiver started working on it); or
 * held while its receiver is on a blind task (with why), given once that task is delivered.
 */
export interface Sent {
  step: 'not-delivered' | 'delivered' | 'picked' | 'held';
  /** The receiving agent. */
  to: string | null;
  toManager: boolean;
  /** Why it was not delivered; shown in the user's language. */
  reasonCode?: ReasonCode;
  /** The same in the service's words (for a runtime error, the runtime's own). */
  reason?: string;
  /** The message as sent. */
  message: string;
}

/** What a question is about, shown as a chip with it. */
export interface AskContext {
  kind: 'task' | 'unit' | 'finding' | 'file' | 'lines' | 'attachment';
  label: string;
  /** attachment: its id in the service's store. */
  id?: string;
  /** file and lines: the repository path. */
  path?: string;
  /** lines: the first and last line. */
  from?: number;
  to?: number;
  /** task: its number in the unit (the label is only the page's). */
  task?: number;
}

/**
 * One question about a piece of work and its answer. A temporary agent answers from the
 * records and the code; it asks the agent that did the work only when that agent is idle.
 */
/** A conversation about a task; its questions share one temporary agent while it lives. */
export interface Thread {
  id: string;
  /** The user ended it; a new question starts a new thread. */
  ended: boolean;
  /** Its temporary agent runs, so a follow-up keeps its context. */
  live: boolean;
  conclusion?: string;
  turns: Turn[];
  /** The user's discussion of a delivered design or research with the agent that did it:
   *  one per task, its box under it while the task may be discussed. */
  discuss?: boolean;
}

export interface Turn {
  /** Its id with the service ("q3"); absent in the sample. */
  id?: string;
  /** The agent that asked; absent when the user did. */
  by?: string;
  question: string;
  context: AskContext[];
  /** The stored files its context points at (attached to the question, or the task's), to open. */
  attachments?: Attachment[];
  /** Null while it comes. */
  answer: string | null;
  /** Who answered (an agent's name, or "user") and when; absent in the sample. */
  answerBy?: string;
  answeredAt?: string;
  /** While it comes: how far it has got. */
  step?: AskStep;
  /** step 'blocked': the agent that waits for a person in its terminal; the question waits too. */
  blocked?: { agent: string; session: string };
  /** The agent that did the work: asked, or not asked because it was busy or gone. */
  consulted?: { agent: string; outcome: 'asked' | 'busy' | 'ended' };
  at: string;
  /** No answer will come, and why. */
  failed?: { code: ReasonCode; reason: string };
  /** A request for the thread's conclusion rather than a question. */
  conclude?: boolean;
  /** step 'held': why, in the service's words (its agent is at work, or on a blind task). */
  reason?: string;
  /** A discussion's answer the user has not read yet. */
  unread?: boolean;
}

/** Why a message was not delivered, or a question got no answer. */
export type ReasonCode = 'no-manager' | 'manager-gone' | 'runtime-down' | 'runtime-error' | 'no-workspace' | 'timeout' | 'restarted' | 'blocked' | 'not-ready' | 'agent-gone' | 'gate-submitted';

/** A discussion's question goes straight to its task's agent: "starting" is its "sending",
 *  "held" waits while that agent works or is on a blind task. */
export type AskStep = 'starting' | 'reading' | 'asking' | 'blocked' | 'held';

/** What the side panel's Manager section and the task detail need from the page. */
export interface Talk {
  sessions: Session[]
  /** The file picked in the code layer, and lines picked in it. */
  file: string | null
  lines: Lines | null
  /** The intent on screen: its page, or the open unit's. */
  intent?: string
  /** The workspace whose conversation is shown, by label: picked in the select, asked for
   *  (Needs you, an intent's page) or followed to on a project switch; `EVERY`: every
   *  manager; none: the one on screen. */
  to?: string
  /** The user picks a workspace (or `EVERY`) in the select. */
  onPick: (workspace: string) => void
  /** The project the rail shows (its folder); none: all. */
  project?: string
  /** The project a workspace (by label) works on, as the rail knows it; none: unknown. */
  projectOf: (workspace: string) => string | undefined
  /** The user has seen a workspace's conversation, up to the manager's newest words shown. */
  onRead: (workspace: string, at: string) => void
  onFile: (path: string) => void
  /** Changes when the box should take the cursor (`?`, "Ask the manager" on lines, the viewer or a task). */
  focusAt: number
  /** Opens the side panel on Manager, the box taking the cursor (the node detail's "Ask the manager"). */
  onOpen: () => void
  /** More chips for the next message: other tasks that changed the picked lines, the open attachment. */
  extra: AskContext[]
  /** The intents (whose workspace runs the picked unit), the messages so far. */
  intents: Intent[]
  messages: ManagerMessage[]
  /** `about`: the unit, task and context the message carries; `files`: the user's, still held by the page. */
  onTell: (to: { intent?: string; workspace: string; all?: boolean }, text: string, manager: string, about: { unit: string; task: number; context: AskContext[] }, files: Attachment[]) => void
  /** A question (or, `conclude`, the request for the conclusion) in the discussion of a
   *  delivered design or research, to its own agent; refused: the service's words. */
  onDiscuss: (unit: string, task: string, agent: string, text: string, conclude: boolean, files: Attachment[]) => Promise<void>
  /** The user has seen a task's discussion, up to its newest answer shown. */
  onDiscussRead: (unit: string, task: string, at: string) => void
  /** The user answers a manager's question in a workspace's conversation: a message of theirs. */
  onAnswer: (workspace: string, answer: Answer, note: string, manager: string) => void
}
