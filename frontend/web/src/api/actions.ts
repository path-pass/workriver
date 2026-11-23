// The one place where the UI reaches out to agents, through the workriver service. With the
// sample (VITE_MOCK=1) nothing is reached: focusing a terminal only says so, and a
// manager's reply is made up.
import { MOCK, MOCK_CHANGED } from '@/workspace/source'
import type { Decision, GateResult, Finding, Task } from '@/work/types'
import type { Attachment } from '@/attachments/types'
import type { Intent, IntentText, IntentVersion, Template, Unit } from '@/planning/types'
import type { Machine, MetricSample, MetricsReport, Thresholds } from '@/machines/types'
import type { Answer, AskContext, Sent } from '@/conversations/types'
import type { Workspace } from '@/workspace/types'
import type { RunView, RunReview, RunAsk } from '@/commands/types'
import type { StartSettings } from '@/agents/types'
import { call, post, read, SIGNED_OUT } from './http'
import { labelOf } from '@/content/choice/surface'

/** The event the UI listens to for a short note at the bottom of the screen. */
export const NOTE = 'workriver:note'

/** Where a terminal brought to the front is, when it is on another machine. */
export interface Remote { machine: string; group?: string | null; agent?: string }
// The runtime brings it to the front on that machine; the user's own client cannot be
// switched there from outside, so the note says where it is.
const remoteNote = (r: Remote | undefined) => r?.machine && window.dispatchEvent(new CustomEvent(NOTE, { detail: { remote: r } }))

/** Bring the agent's terminal to the front. */
export function focusAgent(agent: string) {
  // Without a runtime behind the service, say what would have happened.
  const note = () => window.dispatchEvent(new CustomEvent(NOTE, { detail: { agent } }))
  if (MOCK) note()
  else call<Remote | undefined>('/api/focus', { agent }).then(remoteNote, note)
}

// ---- what the user puts in, on its way to the agents ----

/** The bot that turns accepted findings into tasks. */
export const MANAGER = 'manager'

/** recorded: the service took it (delivery to agents is not connected yet); failed: it did not. */
export type DeliveryStep = 'pending' | 'delivered' | 'picked' | 'recorded' | 'failed'
export interface Delivery { step: DeliveryStep; to: string }

/** Where an agent will find an attachment once the daemon stores it. A placeholder for now. */
export const attachmentPath = (a: Attachment) => `<workriver data dir>/attachments/${a.id}${a.name.match(/\.\w+$/)?.[0] ?? '.png'}`

/** The largest file the service's store takes (attachments.rs `MAX`), in bytes. */
export const MAX_ATTACHMENT = 50 * 1024 * 1024

/** Take a file the user attached (a gate's picture, a file for a question): the page keeps
 *  it until what it goes with is sent, and only then is it uploaded. */
export function attach(file: File): Attachment {
  // A pasted file may have no name: it is named from its type, so the stored file's ending says what it is.
  const sub = file.type.split('/')[1]?.split(/[+;]/)[0] ?? ''
  const ext = { 'text/plain': 'txt', 'text/markdown': 'md', 'image/jpeg': 'jpg', 'image/svg+xml': 'svg' }[file.type] ?? (/^[a-z0-9]+$/.test(sub) ? sub : 'bin')
  return { id: Math.random().toString(36).slice(2, 10), name: file.name || `pasted.${ext}`, url: URL.createObjectURL(file), type: file.type || 'application/octet-stream' }
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
  const picked = r.selected?.[0]
  return lines(
    r.result === 'answered' && picked !== undefined
      ? `Answered "${gate.title}": ${(gate.input.choice && labelOf(gate.input.choice, picked)) ?? picked}`
      : `${gate.gate === 'deploy' ? 'Manual deploy' : 'Manual test'} "${gate.title}": ${r.result === 'done' ? 'done' : 'problem found'}`,
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

/** Puts a file the page holds into the service's store; its id. */
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
    await post('/api/gate', { unit: unit.id, task: Number(gate.id), result: r.result, note: r.note, findings, attachments, selected: r.selected ?? [] })
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
 * One notification per `key`. Clicking brings this tab forward and does `open` when given.
 * An agent's terminal-wait notification with an `open` action stays until clicked (on macOS when the
 * browser's notification style is Alerts). Machine alerts use the browser's default.
 */
export function notify(title: string, body: string | undefined, key: string, open?: () => void) {
  if (!notifyWanted() || typeof Notification === 'undefined' || Notification.permission !== 'granted') return
  const n = new Notification(title, { body, tag: `workriver:${key}`, ...(open && { requireInteraction: true }) })
  n.onclick = () => { window.focus(); open?.(); n.close() }
}

/** Writes to a workspace's manager (by the intent it runs, or the workspace's label), with
 * what the user picked, or (`all`) to every manager running: the service types it into each
 * manager's terminal; each answers with `workriver reply`. The user's files are stored now and the message points at
 * them by their ids (the same content twice is one file). */
export async function tellManager(to: { intent?: string; workspace: string; all?: boolean }, text: string, about: { unit: string; task: number; context: AskContext[] }, files: Attachment[] = []) {
  const stored = await Promise.all(files.map(async a => ({ kind: 'attachment' as const, label: a.name, id: await upload(a) })))
  const attached = stored.filter((c, k) => stored.findIndex(x => x.id === c.id) === k)
  await post('/api/message', { ...to, text, ...about, context: [...about.context, ...attached] })
}

/** Answers a manager's question in a workspace's conversation: a message of the user's,
 * naming the question and the value picked, with the user's note as its text. */
export async function answerManager(workspace: string, answer: Answer, note: string) {
  await post('/api/message', { workspace, text: note, answer })
}

/** The user has read a workspace's conversation with its manager, up to `at`. */
export function readMessages(workspace: string, at: string) {
  if (!MOCK) post('/api/messages/read', { workspace, at }).catch(e => console.warn(e))
}

/** A question in the discussion of a delivered design or research (or, `conclude`, the request
 * for its conclusion): the service types it into the task's own agent, which answers with
 * `workriver answer`. The user's files are stored first and named by their ids. A refusal is
 * thrown with the service's words. */
export async function discuss(unit: string, task: number, text: string, conclude: boolean, files: Attachment[] = []) {
  const stored = await Promise.all(files.map(async a => ({ kind: 'attachment' as const, label: a.name, id: await upload(a) })))
  const context = stored.filter((c, k) => stored.findIndex(x => x.id === c.id) === k)
  await post('/api/discuss', { unit, task, text, conclude, context })
}

/** The user has read a task's discussion, up to `at`. */
export function readDiscussion(unit: string, task: number, at: string) {
  if (!MOCK) post('/api/discuss/read', { unit, task, at }).catch(e => console.warn(e))
}

/** The sample's made-up answer from a task's agent; content, not UI text. */
export const mockAnswer = (text: string, conclude: boolean) => conclude
  ? '（示例结论，还没有接到真正的 agent）\n- 决定：照报告的推荐做\n- 改了：没有\n- 未定：无'
  : `（示例回答，还没有接到真正的 agent）关于「${text}」：报告里第 2 节写了理由。`

/** The sample's made-up reply from a manager; content, not UI text. */
export const mockReply = (text: string) => `（示例回复，还没有接到真正的 manager）收到：「${text}」，我会安排。`

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

/**
 * Records the user's edit of an intent's title, goal or note (`editBody`); a refusal is
 * thrown with the service's words (an edit from an older version: `conflictOf`). With the
 * sample it is kept in memory and the sample shown again with it.
 */
export async function editIntent(intent: Intent, body: Record<string, unknown>) {
  if (MOCK) {
    const changed = (await import('@/mock/history')).mockEdit(intent, body)
    window.dispatchEvent(new CustomEvent(MOCK_CHANGED, { detail: (ws: Workspace) => ({ ...ws, intents: ws.intents.map(i => (i.id === intent.id ? { ...i, ...changed } : i)) }) }))
    return
  }
  await post('/api/append', { kind: 'intent.updated', body })
}

/** An intent's versions, oldest first (who, when, what each changed). */
export async function intentHistory(intent: Intent): Promise<IntentVersion[]> {
  if (MOCK) return (await import('@/mock/history')).mockHistory(intent)
  return (await read<{ versions: IntentVersion[] }>(`/api/intent/${encodeURIComponent(intent.id)}/history`)).versions
}

/** Version `n` of an intent whole, with the one before it (none before the first). */
export async function intentVersion(intent: Intent, n: number): Promise<{ text: IntentText; before: IntentText | null }> {
  if (MOCK) return (await import('@/mock/history')).mockVersion(intent, n)
  return read(`/api/intent/${encodeURIComponent(intent.id)}/history?version=${n}`)
}

/** Merges planned intents into one: the user's alone, so it has a route of its own. */
export async function mergeIntents(into: string, from: string[], title?: string) {
  if (MOCK) return
  await post('/api/backlog/merge', { into, from, title })
}

/** CPU and memory of this machine and the followed ones, with the alert thresholds. */
export async function metrics(): Promise<MetricsReport> {
  if (MOCK) return (await import('@/mock/metrics')).mockMetrics()
  const res = await fetch('/api/metrics')
  if (!res.ok) throw new Error(await res.text())
  return res.json()
}

/** One machine's minutes (means of its samples) over the last `hours`, oldest first. */
export async function metricsHistory(machine: string, hours = 24): Promise<MetricSample[]> {
  if (MOCK) return (await import('@/mock/metrics')).mockHistory(machine, hours)
  const res = await fetch(`/api/metrics/history?machine=${encodeURIComponent(machine)}&hours=${hours}`)
  if (!res.ok) throw new Error(await res.text())
  return (await res.json()).points
}

/** Sets when a machine alerts (the same for every machine); answers the report after it. */
export async function setThresholds(t: Thresholds): Promise<MetricsReport> {
  if (MOCK) {
    const mock = await import('@/mock/metrics')
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

// The sample keeps the default of a start in memory, and offers Claude Code only.
let mockStart: StartSettings = { harness: 'claude', model: '', harnesses: [{ kind: 'claude', aliases: ['opus', 'sonnet', 'fable'] }, { kind: 'codex', aliases: [], why: 'not_installed' }] }

/** The harness and model a start of a manager takes by default, with every harness as offered here. */
export async function startSettings(): Promise<StartSettings> {
  if (MOCK) return mockStart
  const res = await fetch('/api/settings/start')
  if (!res.ok) throw new Error(await res.text())
  return res.json()
}

/** Sets them; answers what the service kept. A refusal is thrown in its words. */
export async function setStartSettings(harness: string, model: string): Promise<StartSettings> {
  if (MOCK) return (mockStart = { ...mockStart, harness, model })
  return call('/api/settings/start', { harness, model })
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

/** Where the message to the workspace's manager went. */
export type StartDelivery = Omit<Sent, 'toManager'>

/**
 * Starts a planned intent in a workspace of its project (by the workspace's id), as the
 * user: recorded as theirs, and its manager is told to split it. `manager` names that
 * manager for the sample.
 */
export async function startIntent(intent: string, workspace: string, manager?: string): Promise<StartDelivery> {
  if (MOCK) return manager ? { step: 'delivered', to: manager, message: '' } : { step: 'not-delivered', to: null, reasonCode: 'no-manager', message: '' }
  return (await call<{ delivery: StartDelivery }>('/api/intent/start', { intent, workspace })).delivery
}

/** One template and the text it is edited as: the user's as saved, else the built-in's. The
 *  sample has none. */
export async function templateText(name: string): Promise<{ template: Template; text: string }> {
  if (MOCK) throw new Error('the sample has no templates to edit')
  return read(`/api/template?name=${encodeURIComponent(name)}`)
}

/** Saves a template from its text: the service reads it and refuses it with the line at fault. */
export async function saveTemplate(text: string): Promise<void> {
  if (MOCK) throw new Error('the sample saves no templates')
  await call('/api/template', { text })
}

/** Removes a template of the user's; one of a built-in's name brings the built-in back. */
export async function removeTemplate(name: string): Promise<void> {
  if (MOCK) throw new Error('the sample removes no templates')
  await call('/api/template/remove', { name })
}

/**
 * Adds a project: a folder of the machine workriver runs on (whole, from `/` or `~/`) and a
 * title (empty: the service takes the folder's name). It comes with its first workspace. A
 * refusal is thrown in the service's words. The sample adds it to its own data.
 */
export async function addProject(path: string, title: string): Promise<void> {
  if (MOCK) {
    window.dispatchEvent(new CustomEvent(MOCK_CHANGED, { detail: (ws: Workspace) => mockProject(ws, path, title) }))
    return
  }
  await call('/api/append', { kind: 'project.added', body: { path, title } })
}

/** The sample after a project is added, with its first workspace, as the service would leave it. */
export function mockProject(ws: Workspace, path: string, title: string): Workspace {
  const projects = ws.projects ?? [], spaces = ws.workspaces ?? []
  const name = title.trim() || path.replace(/\/+$/, '').split('/').pop() || path
  return {
    ...ws,
    projects: [...projects, { id: path, key: `p${projects.length + 1}`, title: name, repos: [] }],
    workspaces: [...spaces, { id: `ws${spaces.length + 1}`, name, project: path }],
  }
}

/** The seat a change names as the one it replaces: the seat the user was shown. */
type Replaces = { session: string; since: number } | null

/** What the user does to a workspace: make one in a project, rename it, close it, open it
 *  again, seat its manager (`replaces`: the seat the user was shown), start a manager to be
 *  seated (a harness and a model; empty: the harness's default), move an intent into it. */
export type SpaceAct =
  | { action: 'add'; project: string; name: string }
  | { action: 'rename'; workspace: string; name: string }
  | { action: 'close' | 'open'; workspace: string }
  | { action: 'manager'; workspace: string; manager: string | null; replaces: Replaces }
  | { action: 'start'; workspace: string; harness: string; model: string; replaces: Replaces }
  | { action: 'move'; intent: string; workspace: string }

/** The seat changed since the user was shown it (the service's 409 that says so): look again and choose. */
export class SeatChanged extends Error {}

/** Records one of the user's acts on a workspace; a refusal is thrown in the service's words,
 *  a seat that changed meanwhile as `SeatChanged` (the other 409, a start already pending,
 *  in its words). The sample changes its own data. */
export async function setSpace(act: SpaceAct): Promise<void> {
  if (MOCK) {
    window.dispatchEvent(new CustomEvent(MOCK_CHANGED, { detail: (ws: Workspace) => mockSpace(ws, act) }))
    return
  }
  const res = await fetch('/api/workspaces', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(act) })
  if (res.status === 401) window.dispatchEvent(new Event(SIGNED_OUT))
  if (!res.ok) {
    const words = await res.text()
    throw res.status === 409 && words.startsWith('the seat changed') ? new SeatChanged(words) : new Error(words)
  }
}

/** The sample after one of the user's acts on a workspace, as the service would leave it. */
export function mockSpace(ws: Workspace, act: SpaceAct): Workspace {
  const all = ws.workspaces ?? []
  const each = (id: string, f: (w: NonNullable<Workspace['workspaces']>[number]) => NonNullable<Workspace['workspaces']>[number]) => all.map(w => (w.id === id ? f(w) : w))
  switch (act.action) {
    case 'add': return { ...ws, workspaces: [...all, { id: `ws${all.length + 1}`, name: act.name.trim(), project: act.project }] }
    case 'rename': return { ...ws, workspaces: each(act.workspace, w => ({ ...w, name: act.name.trim() })) }
    case 'close': return { ...ws, workspaces: each(act.workspace, w => ({ ...w, closed: true, manager: undefined, proposal: undefined, start: undefined })) }
    case 'open': return { ...ws, workspaces: each(act.workspace, w => ({ ...w, closed: false, start: undefined })) }
    // The sample starts no agent: the start stays pending until a seating drops it.
    case 'start': return { ...ws, workspaces: each(act.workspace, w => ({ ...w, start: { since: new Date().toISOString(), harness: act.harness, model: act.model || undefined, name: `${w.id}-manager` } })) }
    case 'move': return { ...ws, intents: ws.intents.map(i => (i.id === act.intent ? { ...i, workspace: act.workspace } : i)) }
    case 'manager': {
      const s = ws.sessions.find(x => !x.ended && (x.session ?? x.agent) === act.manager)
      const since = Math.max(0, ...all.map(w => w.manager?.since ?? 0)) + 1
      const seat = s && act.manager ? { session: act.manager, name: s.agent, machine: s.machine, folder: s.folder ?? '', since, state: s.state ?? ('unknown' as const) } : undefined
      // A session holds one seat: seated here, its old one is empty.
      const workspaces = all.map(w => (w.id === act.workspace ? { ...w, manager: seat, proposal: undefined, start: undefined } : seat && w.manager?.session === seat.session ? { ...w, manager: undefined, start: undefined } : w))
      const held = (x: Workspace['sessions'][number]) => workspaces.find(w => w.manager?.session === (x.session ?? x.agent))
      return {
        ...ws, workspaces,
        sessions: ws.sessions.map(x => { const w = held(x); return w ? { ...x, manager: true, workspace: w.id, intent: w.current } : x.manager ? { ...x, manager: false } : x }),
        intents: ws.intents.map(i => { const m = workspaces.find(w => w.id === i.workspace)?.manager; return i.workspace && i.state !== 'planned' ? { ...i, manager: m?.name, managerState: m?.state } : i }),
      }
    }
  }
}

// The sample makes runs up, in memory; a review comes a moment after the dialog opens.
const mockRuns = new Map<string, RunView & { at: number; prepared: number }>()

function mockReview(command: string): RunReview & { state: 'done' } {
  if (/\brm\s+-\S*[rf]|--force\b|reset\s+--hard/.test(command)) {
    return { state: 'done', verdict: 'danger', text: `Deletes or rewrites work for good: \`${command}\` cannot be undone.\n\n- Removes files or history in the folder\n- Nothing to get it back from but a backup\n\nRun it only if this is exactly what the step needs.` }
  }
  if (/\b(push|install|curl|wget)\b/.test(command)) {
    return { state: 'done', verdict: 'caution', text: `Runs \`${command}\` in the work tree; it reaches past this folder.\n\n- Network: talks to a remote server (pushes or downloads)\n- Writes only inside the folder; nothing installed globally\n- No history rewritten, no force\n\nMatches the step.` }
  }
  if (/\.sh\b/.test(command)) {
    return { state: 'done', verdict: 'caution', text: `Runs the script \`${command.split(/\s+/).find(w => w.endsWith('.sh'))}\`: it installs the web app's packages and creates the local database.\n\n- Writes \`node_modules/\` and \`.local/dev.db\` inside the folder; nothing outside it\n- Network: downloads packages from the npm registry\n- No git changes, nothing installed globally\n\nMatches the step: it sets the project up.` }
  }
  if (/\bdeploy\b/.test(command)) {
    return { state: 'done', verdict: 'caution', text: `Runs \`${command}\`: \`package.json\` → \`scripts/deploy.sh\`, which builds the web app and uploads it to staging.\n\n- Writes \`dist/\` inside the folder; nothing outside it\n- Network: uploads the build to \`staging.example.com\` with the key in \`~/.config/demo/staging\`\n- No git changes, nothing installed\n\nMatches the step: it deploys to staging, not to production.` }
  }
  return { state: 'done', verdict: 'ok', text: `Only reads: \`${command}\` prints what it finds and changes nothing.\n\n- No files written or deleted, no git changes\n- No network, nothing installed\n\nMatches the step.` }
}

/** Where and as whose a command would run; nothing runs yet. */
export async function prepareRun(a: RunAsk): Promise<RunView> {
  if (MOCK) {
    const r = { id: Math.random().toString(16).slice(2, 10), command: a.command, dir: a.dir ?? '~', machine: a.machine ?? null, by: a.by ?? 'user', started: false, output: '', exit: null, watching: false, at: 0, prepared: Date.now(), review: { state: 'pending' } as RunReview }
    mockRuns.set(r.id, r)
    return r
  }
  // Its review is written in the language the page shows.
  return call('/api/run/prepare', { command: a.command, unit: a.unit, project: a.project, from: a.from, step: a.step, lang: document.documentElement.lang })
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

/** What a run's terminal shows now, and its exit status once it ended. */
export async function readRun(id: string): Promise<RunView> {
  if (MOCK) {
    const lines = ['Enumerating objects: 9, done.', 'Writing objects: 100% (5/5), 612 bytes | 612.00 KiB/s, done.', 'To example.com:demo.git', '   1a2b3c4..5d6e7f8  HEAD -> feat/branch']
    const r = mockRuns.get(id)
    if (!r) {
      // One of the sample's recent runs.
      const s = ((await import('@/mock/demo.json')).default as unknown as Workspace).runs!.find(x => x.id === id)!
      const kept = s.state !== 'unknown'
      const review = s.verdict ? { ...mockReview(s.command), verdict: s.verdict } : null
      return { id, command: s.command, dir: s.dir, machine: s.machine ?? null, by: s.from ? 'agent-A' : 'user', started: true, output: kept ? lines.slice(0, s.state === 'running' ? 2 : 4).join('\n') : '', exit: s.exit ?? null, watching: s.state === 'running', kept, focusable: s.focusable, review }
    }
    const review = Date.now() - r.prepared > 2500 ? mockReview(r.command) : r.review
    if (!r.started) return { ...r, review }
    const k = Math.floor((Date.now() - r.at) / 1000)
    return { ...r, review, output: lines.slice(0, k).join('\n'), exit: k > lines.length ? 0 : null, watching: k <= lines.length }
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
  else call<Remote | undefined>(`/api/run/${encodeURIComponent(id)}/focus`, {}).then(remoteNote, note)
}
