// The one place where the UI reaches out to agents. On sample data nothing is reached yet:
// focusing a terminal only says so, and an answer is made up. Later both go through the
// daemon (and Herdr).
import type { Decision, GateResult } from './data'
import type { AskContext, AskStep, Attachment, Finding, Task, Turn } from './types'

/** The event the UI listens to for a short note at the bottom of the screen. */
export const NOTE = 'workriver:note'

/** Bring the agent's terminal to the front. */
export function focusAgent(agent: string) {
  console.info(`[workriver] would focus the terminal of ${agent}`)
  window.dispatchEvent(new CustomEvent(NOTE, { detail: { agent } }))
}

/**
 * Ask about a piece of work. A temporary agent starts, reads workriver's records and the
 * code, and asks the agent that did the work only if that agent is idle; a working agent is
 * never interrupted. `onStep` follows its progress. For now every step is made up.
 */
export async function ask(
  question: string,
  context: AskContext[],
  /** The agent that did the work, and whether it can be asked now. */
  owner: { agent: string; state: 'idle' | 'busy' | 'ended' } | null,
  onStep: (step: AskStep) => void,
): Promise<Pick<Turn, 'answer' | 'consulted'>> {
  const wait = (ms: number) => new Promise(r => setTimeout(r, ms))
  onStep('starting'); await wait(500)
  onStep('reading'); await wait(600)
  if (owner?.state === 'idle') { onStep('asking'); await wait(600) }
  const about = context.map(c => c.label).join('、')
  const paths = [...new Set(context.flatMap(c => (c.path ? [c.path] : [])))]
  return {
    answer: `（示例回答，还没有接到真正的 agent）关于「${question}」：临时 agent 会读 ${about} 的记录和代码来回答。` +
      (paths.length ? `相关的改动在 ${paths.join('、')}。` : ''),
    consulted: owner ? { agent: owner.agent, outcome: owner.state === 'idle' ? 'asked' : owner.state } : undefined,
  }
}

// ---- what the user puts in, on its way to the agents ----

/** The bot that turns accepted findings into tasks. */
export const MANAGER = 'manager'

export type DeliveryStep = 'pending' | 'delivered' | 'picked'
export interface Delivery { step: DeliveryStep; to: string }

/** Where an agent will find an attachment once the daemon stores it. A placeholder for now. */
export const attachmentPath = (a: Attachment) => `<workriver data dir>/attachments/${a.id}${a.name.match(/\.\w+$/)?.[0] ?? '.png'}`

/** Take a picture the user attached. Later this uploads it; for now the page keeps it. */
export function attach(file: File): Attachment {
  return { id: Math.random().toString(36).slice(2, 10), name: file.name || 'pasted.png', url: URL.createObjectURL(file) }
}
export const detach = (a: Attachment) => { if (a.url.startsWith('blob:')) URL.revokeObjectURL(a.url) }

/**
 * Send a message to an agent or the manager bot. It waits a moment before it goes out, so it
 * can be taken back; the returned function does that. For now the steps are made up.
 */
function deliver(to: string, message: string, onStep: (d: Delivery) => void): () => void {
  onStep({ step: 'pending', to })
  const out = setTimeout(() => { console.info(`[workriver] to ${to}:\n${message}`); onStep({ step: 'delivered', to }) }, 3000)
  const taken = to === MANAGER ? 0 : setTimeout(() => onStep({ step: 'picked', to }), 5500)
  return () => { clearTimeout(out); clearTimeout(taken) }
}

const lines = (...l: (string | false | undefined)[]) => l.filter(Boolean).join('\n')
const files = (list: Attachment[]) => list.length > 0 && `  Attachments: ${list.map(attachmentPath).join(', ')}`

/** What a gate's result says to the agent waiting on it. */
export function gateMessage(gate: Task, r: GateResult): string {
  return lines(
    `${gate.gate === 'deploy' ? 'Manual deploy' : 'Manual test'} "${gate.title}": ${r.result === 'done' ? 'done' : 'problem found'}`,
    r.note && `Note: ${r.note}`,
    files(r.attachments),
    ...r.findings.flatMap(f => [
      `- ${f.severity}: ${f.problem}`,
      f.impact && `  If left: ${f.impact}`,
      f.evidence.length > 0 && `  Where: ${f.evidence.join(', ')}`,
      files(f.attachments),
    ]),
  )
}

export function submitGate(gate: Task, r: GateResult, to: string, onStep: (d: Delivery) => void) {
  const message = gateMessage(gate, r)
  return { message, cancel: deliver(to, message, onStep) }
}

/** A decision on a finding: accepted ones go to the manager bot, the others to the agent. */
export function decideFinding(task: Task, f: Finding, decision: Decision, note: string, agent: string | null, onStep: (d: Delivery) => void) {
  const to = decision === 'accept' || !agent ? MANAGER : agent
  const message = lines(`${f.id} on "${task.title || task.reviewer || task.gate}": ${decision}`, `Problem: ${f.problem}`, note && `Note: ${note}`)
  return { message, cancel: deliver(to, message, onStep) }
}
