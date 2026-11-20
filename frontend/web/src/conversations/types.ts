// Conversations: questions about work and their answers, the user's messages to managers,
// and where a message is on its way.
import type { Attachment } from '@/attachments/types'

/** The user's message to a workspace's manager, typed into its terminal; the manager replies with `workriver reply`. Or one the manager started (`workriver say`). */
export interface ManagerMessage {
  /** "m3"; a local one the service refused has none. */
  id?: string;
  /** The intent it is about, if any. */
  intent?: string;
  /** The runtime workspace whose manager it went to, by label. */
  workspace: string;
  /** The manager's name. */
  manager: string;
  text: string;
  at: string;
  /** blocked: the manager waits for a person in its terminal; the message goes in once it is free. */
  step: 'sending' | 'blocked' | 'delivered' | 'not-delivered';
  reasonCode?: ReasonCode;
  reason?: string;
  blocked?: { agent: string; session: string };
  /** Each reply is the manager's; unread: written after the user last read this workspace's conversation. */
  replies: { by: string; text: string; at: string; unread?: boolean }[];
  /** Started by the manager (`workriver say`): `text` is the manager's, and nothing above it is the user's. */
  byManager?: boolean;
  /** The manager's message (byManager), not read yet. */
  unread?: boolean;
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
}

/** Why a message was not delivered, or a question got no answer. */
export type ReasonCode = 'no-manager' | 'manager-gone' | 'runtime-down' | 'runtime-error' | 'no-workspace' | 'timeout' | 'restarted' | 'blocked' | 'not-ready';

export type AskStep = 'starting' | 'reading' | 'asking' | 'blocked';
