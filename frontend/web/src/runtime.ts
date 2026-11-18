// The one place where the UI reaches out to agents, through the workriver service. With the
// sample (VITE_MOCK=1) nothing is reached: focusing a terminal only says so. Asking is made
// up either way for now, and the service records the user's inputs without delivering them
// to agents yet.
import { MOCK, type Decision, type GateResult } from './data'
import type { AskContext, AskStep, Attachment, Finding, Machine, MetricSample, MetricsReport, Sent, Task, Thresholds, Turn, Unit, Workspace } from './types'

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
    // `workriver open` opens the root: a link the user opened while signed out lands there.
    const wanted = recallWanted()
    history.replaceState(null, '', location.pathname === '/' && wanted ? wanted : location.pathname + location.search)
    await fetch('/api/session', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ code }) }).catch(() => undefined)
  }
  const res = await fetch('/api/session')
  if (!res.ok) throw new Error(await res.text())
  return (await res.json()).user === true
}

const WANTED = 'workriver.signin-to'

/** A link opened while signed out (`/u1/3`), kept for the sign-in that follows in a new tab. */
export function rememberWanted() {
  if (location.pathname === '/') return
  try { localStorage.setItem(WANTED, JSON.stringify({ path: location.pathname + location.search, at: Date.now() })) } catch { /* lands on the root */ }
}

/** That link, once, if it is from the last ten minutes. */
function recallWanted(): string | null {
  try {
    const w = JSON.parse(localStorage.getItem(WANTED) ?? 'null')
    localStorage.removeItem(WANTED)
    return w && Date.now() - w.at < 600_000 && typeof w.path === 'string' && /^\/(?!\/)/.test(w.path) ? w.path : null
  } catch { return null }
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

/** Asks the service a question about a task, or with none about the unit as a whole (live
 * mode): it records it, and starts the thread's temporary agent or gives it to the one already there. */
export async function askWhy(unit: Unit, task: Task | null, text: string, context: AskContext[], thread: string | null, conclude = false) {
  await post('/api/ask', { unit: unit.id, task: task ? Number(task.id) : 0, text, context, thread, conclude })
}

/** Writes to a workspace's manager (by the intent it runs, or the workspace's label): the
 * service types it into the manager's terminal; the manager answers with `workriver reply`. */
export async function tellManager(to: { intent?: string; workspace: string }, text: string) {
  await post('/api/message', { ...to, text })
}

/** The sample's made-up reply from a manager; content, not UI text. */
export const mockReply = (text: string) => `（示例回复，还没有接到真正的 manager）收到：「${text}」，我会安排。`

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

/** Marks a task no longer needed: the user's alone from the page; its agent and the manager are told. */
export async function dropTask(unit: Unit, task: Task, reason: string) {
  if (MOCK) return
  await post('/api/task/drop', { unit: unit.id, task: Number(task.id), reason })
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

/** One machine's minutes (means of its samples) over the last `hours`, oldest first. */
export async function metricsHistory(machine: string, hours = 24): Promise<MetricSample[]> {
  if (MOCK) return (await import('./mock/metrics')).mockHistory(machine, hours)
  const res = await fetch(`/api/metrics/history?machine=${encodeURIComponent(machine)}&hours=${hours}`)
  if (!res.ok) throw new Error(await res.text())
  return (await res.json()).points
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

// The sample keeps the stall threshold in memory.
let mockStall = 10

/** After how many idle minutes a running task's agent is reminded (then escalated). */
export async function stallMinutes(): Promise<number> {
  if (MOCK) return mockStall
  const res = await fetch('/api/settings/stall')
  if (!res.ok) throw new Error(await res.text())
  return (await res.json()).minutes
}

/** Sets it (1 to 240 minutes); answers what the service kept. */
export async function setStallMinutes(minutes: number): Promise<number> {
  if (MOCK) return (mockStall = minutes)
  const res = await fetch('/api/settings/stall', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ minutes }) })
  if (!res.ok) throw new Error(await res.text())
  return (await res.json()).minutes
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

// ---- the user's own hands: starting an intent, running a command ----

/** The runtime workspace the user's command shells are in (the service's `run::GROUP`). */
export const RUN_GROUP = 'workriver-commands'

/** A POST that answers JSON; a refusal is thrown with the service's words. */
async function call<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(path, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) })
  if (res.status === 401) window.dispatchEvent(new Event(SIGNED_OUT))
  if (!res.ok) throw new Error(await res.text())
  return res.status === 204 ? (undefined as T) : res.json()
}

/** Where the message to the workspace's manager went. */
export type StartDelivery = Omit<Sent, 'toManager'>

/**
 * Starts a planned intent in a runtime workspace, as the user: recorded as theirs, and its
 * manager is told to split it. `manager` names that manager for the sample.
 */
export async function startIntent(intent: string, workspace: string, manager?: string, confirmOtherProject = false): Promise<StartDelivery> {
  if (MOCK) return manager ? { step: 'delivered', to: manager, message: '' } : { step: 'not-delivered', to: null, reasonCode: 'no-manager', message: '' }
  return (await call<{ delivery: StartDelivery }>('/api/intent/start', { intent, workspace, confirmOtherProject })).delivery
}

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
}

// The sample makes runs up, in memory.
const mockRuns = new Map<string, RunView & { at: number }>()

/** Where and as whose a command would run; nothing runs yet. */
export async function prepareRun(a: RunAsk): Promise<RunView> {
  if (MOCK) {
    const r = { id: Math.random().toString(16).slice(2, 10), command: a.command, dir: a.dir ?? '~', machine: a.machine ?? null, by: a.by ?? 'user', started: false, output: '', exit: null, watching: false, at: 0 }
    mockRuns.set(r.id, r)
    return r
  }
  return call('/api/run/prepare', { command: a.command, unit: a.unit, project: a.project, from: a.from, step: a.step })
}

/** Runs what was prepared, with the exact text the user confirmed. */
export async function runCommand(id: string, command: string): Promise<void> {
  if (MOCK) {
    const r = mockRuns.get(id)
    if (r) mockRuns.set(id, { ...r, started: true, watching: true, at: Date.now() })
    return
  }
  await call('/api/run', { id, command })
}

/** What a run's pane shows now, and its exit status once it ended. */
export async function readRun(id: string): Promise<RunView> {
  if (MOCK) {
    const lines = ['Enumerating objects: 9, done.', 'Writing objects: 100% (5/5), 612 bytes | 612.00 KiB/s, done.', 'To example.com:demo.git', '   1a2b3c4..5d6e7f8  HEAD -> feat/branch']
    const r = mockRuns.get(id)
    if (!r) {
      // One of the sample's recent runs.
      const s = ((await import('./mock/demo.json')).default as unknown as Workspace).runs!.find(x => x.id === id)!
      const kept = s.state !== 'unknown'
      return { id, command: s.command, dir: s.dir, machine: s.machine ?? null, by: s.from ? 'agent-A' : 'user', started: true, output: kept ? lines.slice(0, s.state === 'running' ? 2 : 4).join('\n') : '', exit: s.exit ?? null, watching: s.state === 'running', kept, focusable: s.focusable }
    }
    const k = Math.floor((Date.now() - r.at) / 1000)
    return { ...r, output: lines.slice(0, k).join('\n'), exit: k > lines.length ? 0 : null, watching: k <= lines.length }
  }
  const res = await fetch(`/api/run/${encodeURIComponent(id)}`)
  if (res.status === 401) window.dispatchEvent(new Event(SIGNED_OUT))
  if (!res.ok) throw new Error(await res.text())
  return res.json()
}

/** Brings a run's terminal to the front. */
export function focusRun(id: string) {
  const note = () => window.dispatchEvent(new CustomEvent(NOTE, { detail: { agent: 'workriver' } }))
  if (MOCK) note()
  else call(`/api/run/${encodeURIComponent(id)}/focus`, {}).catch(note)
}
