// The one place where the UI reaches out to agents, through the workriver service. With the
// sample (VITE_MOCK=1) nothing is reached: focusing a terminal only says so. Asking is made
// up either way for now, and the service records the user's inputs without delivering them
// to agents yet.
import { MOCK, type Decision, type GateResult } from './data'
import type { AskContext, AskStep, Attachment, Finding, Machine, MetricsReport, Task, Thresholds, Turn, Unit } from './types'

/** Sent on the window when the service says this page is not the user (any more). */
export const SIGNED_OUT = 'workriver:signed-out'

async function post(path: string, body: unknown) {
  const res = await fetch(path, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) })
  if (res.status === 401) window.dispatchEvent(new Event(SIGNED_OUT))
  if (!res.ok) throw new Error(`${path}: ${res.status} ${await res.text()}`)
}

/**
 * Whether this page is the user's. A one-time code in the address (`#k=…`, from
 * `workriver open`) is exchanged for the session cookie first and taken out of the address,
 * so it is not kept in history or shared with a copied link.
 */
export async function startSession(): Promise<boolean> {
  const code = new URLSearchParams(location.hash.slice(1)).get('k')
  if (code) {
    history.replaceState(null, '', location.pathname + location.search)
    await fetch('/api/session', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ code }) }).catch(() => undefined)
  }
  const res = await fetch('/api/session')
  if (!res.ok) throw new Error(await res.text())
  return (await res.json()).user === true
}

/** The event the UI listens to for a short note at the bottom of the screen. */
export const NOTE = 'workriver:note'

/** Bring the agent's terminal to the front. */
export function focusAgent(agent: string) {
  // Without a runtime behind the service, say what would have happened.
  const note = () => window.dispatchEvent(new CustomEvent(NOTE, { detail: { agent } }))
  if (MOCK) note()
  else post('/api/focus', { agent }).catch(note)
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

/** recorded: the service took it (delivery to agents is not connected yet); failed: it did not. */
export type DeliveryStep = 'pending' | 'delivered' | 'picked' | 'recorded' | 'failed'
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
 * can be taken back; the returned function does that. Then `record` hands it to the service;
 * with the sample the steps are made up.
 */
function deliver(to: string, message: string, record: () => Promise<void>, onStep: (d: Delivery) => void): () => void {
  onStep({ step: 'pending', to })
  if (!MOCK) {
    const out = setTimeout(() => record().then(() => onStep({ step: 'recorded', to }), e => { console.warn(e); onStep({ step: 'failed', to }) }), 3000)
    return () => clearTimeout(out)
  }
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

/** Puts a picture the page holds into the service's store; its id. */
async function upload(a: Attachment): Promise<string> {
  const blob = await (await fetch(a.url)).blob()
  const res = await fetch(`/api/attachments?name=${encodeURIComponent(a.name)}`, { method: 'POST', headers: { 'content-type': blob.type || 'application/octet-stream' }, body: blob })
  if (!res.ok) throw new Error(`upload ${a.name}: ${res.status} ${await res.text()}`)
  return (await res.json()).id
}

export function submitGate(unit: Unit, gate: Task, r: GateResult, to: string, onStep: (d: Delivery) => void) {
  const message = gateMessage(gate, r)
  // The pictures go into the store when the result goes out; the result names them.
  const record = async () => {
    const attachments = await Promise.all(r.attachments.map(upload))
    const findings = await Promise.all(r.findings.map(async f => ({
      severity: f.severity, problem: f.problem, impact: f.impact, where: f.evidence, attachments: await Promise.all(f.attachments.map(upload)),
    })))
    await post('/api/gate', { unit: unit.id, task: Number(gate.id), result: r.result, note: r.note, findings, attachments })
  }
  return { message, cancel: deliver(to, message, record, onStep) }
}

/** A decision on a finding: every one goes to the manager, who tells the workers what they need. */
export function decideFinding(unit: Unit, task: Task, f: Finding, decision: Decision, note: string, onStep: (d: Delivery) => void) {
  const to = MANAGER
  const message = lines(`${f.id} on "${task.title || task.reviewer || task.gate}": ${decision}`, `Problem: ${f.problem}`, note && `Note: ${note}`)
  const record = () => post('/api/decide', { unit: unit.id, task: Number(task.id), finding: f.id, decision, note })
  return { message, cancel: deliver(to, message, record, onStep) }
}

// ---- notifications: an agent waiting for a person, told outside the page ----

const NOTIFY = 'workriver.notify'
/** Whether the user asked to be notified (off until they turn it on in Settings). */
export function notifyWanted() { try { return localStorage.getItem(NOTIFY) === '1' } catch { return false } }
/** Turns notifying on (asking the browser's permission) or off; says whether it is on. */
export async function wantNotify(on: boolean): Promise<boolean> {
  let ok = on && typeof Notification !== 'undefined'
  if (ok && Notification.permission !== 'granted') ok = (await Notification.requestPermission()) === 'granted'
  try { localStorage.setItem(NOTIFY, ok ? '1' : '0') } catch { /* not remembered; still applies now */ }
  return ok
}
/**
 * One notification; clicking it brings this tab forward and, for an agent, its terminal.
 * `key` names what it is about (an agent, or a machine's alert), so one is not shown twice.
 */
export function notify(title: string, body: string | undefined, key: string, agent = true) {
  if (!notifyWanted() || typeof Notification === 'undefined' || Notification.permission !== 'granted') return
  const n = new Notification(title, { body, tag: `workriver:${key}` })
  n.onclick = () => { window.focus(); if (agent) focusAgent(key); n.close() }
}

/** Asks the service a question about a task (live mode): it records it, and starts the
 * thread's temporary agent or gives it to the one already there. */
export async function askWhy(unit: Unit, task: Task, text: string, context: AskContext[], thread: string | null, conclude = false) {
  await post('/api/ask', { unit: unit.id, task: Number(task.id), text, context, thread, conclude })
}

/** Ends a conversation; its temporary agent closes. */
export async function endThread(thread: string) {
  await post('/api/thread/end', { thread })
}

/** A chore is done; it goes to no one, so it is only recorded. */
export function choreDone(id: string) {
  if (!MOCK) post('/api/chore-done', { chore: id }).catch(e => console.warn(e))
}

/**
 * Records a change the user makes to the roadmap or backlog (adding or moving an intent,
 * batching or dropping findings); the service delivers what goes to an agent. Nothing is
 * recorded with the sample.
 */
export async function record(kind: string, body: unknown) {
  if (MOCK) return
  // Who records is the service's to find: this page's session is the user's.
  await post('/api/append', { kind, body })
}

/** Merges planned intents into one: the user's alone, so it has a route of its own. */
export async function mergeIntents(into: string, from: string[], title?: string) {
  if (MOCK) return
  await post('/api/backlog/merge', { into, from, title })
}

/** CPU and memory of this machine and the followed ones, with the alert thresholds. */
export async function metrics(): Promise<MetricsReport> {
  if (MOCK) return (await import('./mock/metrics')).mockMetrics()
  const res = await fetch('/api/metrics')
  if (!res.ok) throw new Error(await res.text())
  return res.json()
}

/** Sets when a machine alerts (the same for every machine); answers the report after it. */
export async function setThresholds(t: Thresholds): Promise<MetricsReport> {
  if (MOCK) {
    const mock = await import('./mock/metrics')
    mock.setMockThresholds(t)
    return mock.mockMetrics()
  }
  const res = await fetch('/api/metrics/thresholds', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(t) })
  if (!res.ok) throw new Error(await res.text())
  return res.json()
}

/** The machines the service knows, followed or not. */
export async function machines(): Promise<Machine[]> {
  if (MOCK) return []
  const res = await fetch('/api/machines')
  if (!res.ok) throw new Error(await res.text())
  return res.json()
}

/** Adds, enables, disables or removes a machine; answers the list after it. */
export async function setMachine(label: string, action: 'add' | 'enable' | 'disable' | 'remove', ssh = ''): Promise<Machine[]> {
  const res = await fetch('/api/machines', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ label, action, ssh }) })
  if (!res.ok) throw new Error(await res.text())
  return res.json()
}
