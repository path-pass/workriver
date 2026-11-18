// Loading the workspace (from the service, or the sample with VITE_MOCK=1) and everything
// the views derive from it. Apart from the loading, pure functions only.
import { diffLines, structuredPatch } from 'diff'
import type { T } from './i18n'
import type { Attachment, Chore, Commit, FileChange, Intent, Finding, MachineAlert, Unit, ModuleNode, Repo, Session, Severity, Task, Workspace } from './types'

/** Sample data instead of the service, with `VITE_MOCK=1`. */
export const MOCK = import.meta.env.VITE_MOCK === '1'

/**
 * A file with its texts. Its diff lines are worked out when first read, which is when its diff
 * is drawn: diffing every file of a big unit up front held its first showing for half a
 * second. The line counts are the service's (a minimal diff, as here); the sample has none.
 */
const withLines = (f: FileChange): FileChange => {
  const before = f.before ?? null, after = f.after ?? null
  let lines: string[] | undefined
  const out = { ...f, before, after }
  if (f.add === undefined || f.del === undefined) { const c = changeOf(before, after); Object.assign(out, { add: c.add, del: c.del }); lines = c.lines }
  return Object.defineProperty(out, 'lines', { get: () => (lines ??= changeOf(before, after).lines), enumerable: true })
}

/**
 * The service's snapshot names each file with its line counts and leaves its texts out (a
 * unit's come with `loadCode`); the sample carries them inline.
 */
function prepare(ws: Workspace): Workspace {
  for (const unit of unitsOf(ws)) for (const c of unit.commits) c.files = c.files.map(f => (f.before === undefined && f.after === undefined ? { ...f, before: null, after: null, lines: [] } : withLines(f)))
  if (import.meta.env.DEV && MOCK) checkCounts(ws)
  return ws
}

/** A unit's file texts, by commit id and path, as of its `code`. */
export interface UnitCode { code: string; files: Map<string, FileChange> }
const fileKey = (commit: string, path: string) => `${commit}\0${path}`
// The units looked at last, each as last read: a unit's texts can be tens of MB, so only the
// last few are kept (going back and forth between two stays instant).
const codes = new Map<string, UnitCode>()
const KEEP_CODES = 2

/**
 * A unit's file texts, read when its code is shown; read again only once its `code` changed,
 * and then only for commits not read before (a commit never changes) and uncommitted work.
 */
export async function loadCode(unit: Unit): Promise<UnitCode> {
  const had = codes.get(unit.id)
  // Read again or not, it is now the latest looked at.
  codes.delete(unit.id)
  if (had) codes.set(unit.id, had)
  if (had && had.code === (unit.code ?? '')) return had
  const have = unit.commits.filter(c => !c.live && had?.files.has(fileKey(c.id, c.files[0]?.path ?? ''))).map(c => c.id)
  const res = await fetch(`/api/unit/${encodeURIComponent(unit.id)}/code${have.length ? `?have=${have.join(',')}` : ''}`)
  if (!res.ok) throw new Error(`${res.status} ${await res.text()}`)
  const got: { code?: string; commits: { id: string; files: FileChange[] }[] } = await res.json()
  const kept = new Set(have)
  const files = new Map([...(had?.files ?? [])].filter(([k]) => kept.has(k.slice(0, k.indexOf('\0')))))
  for (const c of got.commits) for (const f of c.files) files.set(fileKey(c.id, f.path), withLines(f))
  const code = { code: got.code ?? '', files }
  codes.delete(unit.id)
  codes.set(unit.id, code)
  for (const old of [...codes.keys()].slice(0, -KEEP_CODES)) codes.delete(old)
  return code
}

/** The unit with its files' texts, where `code` has them. */
export const withCode = (unit: Unit, code: UnitCode): Unit =>
  ({ ...unit, commits: unit.commits.map(c => ({ ...c, files: c.files.map(f => code.files.get(fileKey(c.id, f.path)) ?? f) })) })

async function loadWorkspace(): Promise<Workspace> {
  if (!MOCK) {
    const res = await fetch('/api/workspace')
    if (!res.ok) throw new Error(`${res.status} ${await res.text()}`)
    return prepare(await res.json())
  }
  const [{ default: demo }, { default: lockscreen }] = await Promise.all([import('./mock/demo.json'), import('./mock/lockscreen.svg')])
  const ws = demo as unknown as Workspace
  // Sample pictures are files next to the data.
  const pictures: Record<string, string> = { 'lockscreen.svg': lockscreen }
  for (const n of unitsOf(ws).flatMap(u => u.tasks)) for (const a of [...(n.output.attachments ?? []), ...(n.output.findings ?? []).flatMap(f => f.attachments ?? [])]) a.url = pictures[a.url] ?? a.url
  ws.alerts = (await import('./mock/metrics')).mockAlerts()
  return prepare(ws)
}

/**
 * The one place data comes from: the workspace now, and again whenever the service says it
 * changed. `onDown` is called while the service cannot be reached. Returns a stop function.
 */
export function watchWorkspace(onData: (ws: Workspace) => void, onDown: () => void): () => void {
  let stopped = false
  // At most one read at a time: changes told meanwhile make one more read after it. A hidden
  // tab reads nothing; it reads once when shown again, if anything changed meanwhile.
  let reading = false, again = false, missed = true
  const get = () => {
    if (stopped) return
    if (document.hidden) { missed = true; return }
    if (reading) { again = true; return }
    reading = true
    missed = again = false
    loadWorkspace()
      .then(ws => { if (!stopped) onData(ws) }, () => { if (!stopped) onDown() })
      .finally(() => { reading = false; if (again) get() })
  }
  const shown = () => { if (!document.hidden && missed) get() }
  document.addEventListener('visibilitychange', shown)
  get()
  if (MOCK) return () => { stopped = true; document.removeEventListener('visibilitychange', shown) }
  const events = new EventSource('/api/events')
  events.addEventListener('changed', get)
  // The stream reconnects by itself; what changed while it was down is fetched then.
  let opened = false
  events.onopen = () => { if (opened) get(); opened = true }
  events.onerror = () => { if (!stopped) onDown() }
  return () => { stopped = true; events.close(); document.removeEventListener('visibilitychange', shown) }
}

/** The repository a unit works in. */
export const repoOf = (ws: Workspace, unit: Unit) => ws.repos.find(r => r.path === unit.repo && r.machine === unit.machine)!

/**
 * Self-check, in development: for every unit and every scope (nothing picked, each node
 * picked), each file's count in the tree equals the change the single-file view shows.
 */
function checkCounts(ws: Workspace) {
  for (const unit of unitsOf(ws).filter(u => !noCode(u)))
    for (const pick of [null, ...unit.tasks.map(n => ({ kind: 'task', id: n.id }) as const)]) {
      const sel = selectionOf(unit, pick)
      for (const r of fileTree(repoOf(ws, unit).modules, unit, sel, true).rows.values()) {
        if (r.folder || !r.status) continue
        const shown = shownDelta(unit, sel, r.id)
        console.assert(!!shown && r.add === shown.add && r.del === shown.del, 'tree and file view disagree', unit.id, pick?.id, r.id)
      }
    }
}

/** The change from one text to another as unified-diff lines, with `context` lines around each change. */
export function changeOf(before: string | null, after: string | null, context = 3) {
  const { hunks } = structuredPatch('a', 'b', before ?? '', after ?? '', undefined, undefined, { context })
  const lines = hunks.flatMap(h => [`@@ -${h.oldStart},${h.oldLines} +${h.newStart},${h.newLines} @@`, ...h.lines])
  return { lines, add: lines.filter(l => l[0] === '+').length, del: lines.filter(l => l[0] === '-').length }
}

export const baseName = (p: string) => p.split('/').pop() ?? p

/** Every unit, across intents (with each intent's own earlier work, `intentLevel`). */
export const unitsOf = (ws: Workspace | null) => ws?.intents.flatMap(i => i.units) ?? []
/** A unit with no branch and no code: research and design, or an intent's own earlier work. */
export const noCode = (u: Unit) => !!u.intentLevel || u.kind === 'research'
/** The first unit the rail shows for a project (or for all): of its active intents, then of its done ones. */
export function firstUnit(ws: Workspace, project: string | null): Unit | undefined {
  const mine = ws.intents.filter(i => !project || i.project === project)
  return [...mine.filter(i => (i.state ?? 'active') === 'active'), ...mine.filter(i => i.state === 'done')].flatMap(i => i.units).find(u => !u.intentLevel)
}
/** The intent a unit belongs to, or the intent itself (its page). */
export const intentOf = (ws: Workspace, unitId: string) => ws.intents.find(i => i.id === unitId || i.units.some(u => u.id === unitId))

/** The data's times (`YYYY-MM-DD HH:mm` or ISO) as milliseconds. */
export const ms = (s: string | undefined) => (s ? new Date(s.replace(' ', 'T')).getTime() : NaN)

/** What is picked inside a unit: one piece of work, or one finding (by its id) of a review. */
export type Pick = { kind: 'task'; id: string; finding?: string } | null

// ---- findings ----

export const SEVERITIES: Severity[] = ['P1', 'P2', 'P3']
export const findingsOf = (n: Task): Finding[] => n.output.findings ?? []
/** Findings still waiting for the user's decision. */
/** Open findings that wait for the user: not those the user handed to the intent's manager. */
const waits = (f: Finding) => f.state === 'open' && f.decider !== 'manager'
export const openCount = (n: Task) => findingsOf(n).filter(waits).length
/** Findings counted per severity, e.g. 'P1 2 · P2 3'. */
export const severityCounts = (found: Finding[]) =>
  SEVERITIES.map(s => [s, found.filter(f => f.severity === s).length] as const)
    .filter(([, k]) => k)
    .map(([s, k]) => `${s} ${k}`)
    .join(' · ')

// ---- labels ----

/** Who found a finding: the reviewer, or the kind of gate. */
export const finderLabel = (t: T, n: Task): string =>
  n.gate ? t(`gate.${n.gate}`) : n.reviewer ? t(`reviewer.${n.reviewer}`) : t('kind.review')

export function reviewLabel(t: T, n: Task): string {
  const by = finderLabel(t, n)
  if (n.state === 'dropped') return t('review.dropped', { by })
  if (n.result === 'running') return t('review.running', { by })
  if (n.result === 'found') return t('review.found', { by, count: findingsOf(n).length })
  return t('review.passed', { by })
}
/** Whom a question about a task goes to: its agent; for a gate, the agent that asked for it. */
export const askeeOf = (unit: Unit, n: Task): string | null =>
  n.kind === 'gate' ? unit.tasks.find(x => x.id === n.inputs.find(i => i.primary)?.from)?.agent ?? null : n.agent

/** Finished work handed in with no check run: shown on the graph, not counted anywhere. */
export const untested = (n: Task) => n.kind === 'implement' && !n.design && !n.research && n.state === 'done' && !n.output.evidence?.length

export function nodeLabel(t: T, n: Task): string {
  if (n.kind === 'review') return reviewLabel(t, n)
  if (n.state === 'unregistered') return t('task.unregistered', { title: n.title })
  return n.taskId ? `#${n.taskId} ${n.title}` : n.title
}


// ---- the graph ----

/**
 * The picked node with everything it came from and everything that came out of it. With a
 * finding picked: the review and the nodes that finding leads to.
 */
export function related(unit: Unit, id: string, finding?: string): Set<string> {
  const byId = new Map(unit.tasks.map(n => [n.id, n]))
  const review = byId.get(id)
  const f = finding && review ? findingsOf(review).find(x => x.id === finding) : undefined
  if (f) return new Set([id, ...f.leadsTo])
  const lit = new Set([id])
  const up = (n: string) =>
    byId.get(n)?.inputs.forEach(i => {
      if (!lit.has(i.from)) { lit.add(i.from); up(i.from) }
    })
  const down = (n: string) =>
    unit.tasks.forEach(k => {
      if (!lit.has(k.id) && k.inputs.some(i => i.from === n)) { lit.add(k.id); down(k.id) }
    })
  up(id)
  down(id)
  return lit
}

// ---- selection: which commits and planned places belong to what is picked ----

export interface Selection {
  node: Task | null
  /** Indexes into unit.commits that the pick covers; empty for "the unit as a whole". */
  commits: number[]
  /** Planned places of the picked node; null when no node is picked. */
  plans: string[] | null
}

export function selectionOf(unit: Unit, pick: Pick): Selection {
  const node = pick ? unit.tasks.find(n => n.id === pick.id) : undefined
  if (!node) return { node: null, commits: [], plans: null }
  // A review or gate stands for the work it followed and the work handed out for its
  // findings; one finding of it, for the work handed out for that finding. A finding that
  // led to nothing yet (still open, say) stands for the review as a whole.
  const finding = findingsOf(node).find(f => f.id === pick?.finding && f.leadsTo.length)
  const group =
    node.kind !== 'implement'
      ? [...new Set(finding ? finding.leadsTo : [node.inputs.find(i => i.primary)?.from, ...findingsOf(node).flatMap(f => f.leadsTo)])]
          .map(id => unit.tasks.find(n => n.id === id))
          .filter((n): n is Task => !!n)
      : [node]
  const ids = new Set(group.flatMap(n => n.output.commits ?? []))
  const commits = unit.commits.flatMap((c, k) => (ids.has(c.id) ? [k] : []))
  return {
    node,
    commits,
    plans: group.filter(n => n.state !== 'done').flatMap(n => n.plans),
  }
}

export const commitStats = (c: Commit) => ({
  files: c.files.length,
  add: c.files.reduce((s, f) => s + f.add, 0),
  del: c.files.reduce((s, f) => s + f.del, 0),
})

// ---- what wants a look ----

export interface Working {
  node: Task
  /** Files sitting uncommitted in the agent's working directory. */
  files: string[]
}
export function working(unit: Unit): Working[] {
  return unit.tasks
    .filter(n => n.kind === 'implement' && n.state === 'running')
    .map(node => ({
      node,
      files: unit.commits.filter(c => c.live && node.output.commits?.includes(c.id)).flatMap(c => c.files.map(f => f.path)),
    }))
}

/**
 * Where a live agent is, in the order the rail lists them. Agents cross units, so this is
 * worked out over the whole workspace:
 * working  running a node, wherever else it holds work
 * queued   holds work that has not started, runs nothing
 * idle     neither; its session is still there
 */
export type AgentState = 'working' | 'queued' | 'idle'

export interface AgentAt {
  agent: string
  /** The session, when the data has one: what the terminal button reaches. */
  session?: string
  /** What the session is about, in the harness's words. */
  topic?: string
  state: AgentState
  /** Waiting at a question or approval in its terminal (the runtime says so). */
  blocked: boolean
  /** The manager of its intent. */
  manager: boolean
  /** The machine it runs on; absent: this one. */
  machine?: string
  /** Its machine cannot be reached: as last seen. */
  unreachable?: boolean
  /** The node it is on; idle: the last one it finished (data order), or null if none. */
  at: { unit: Unit; node: Task } | null
}

/** Agents of one runtime workspace (or, in the sample, of no workspace). */
export interface AgentGroup {
  key: string
  /** The intent's title when the workspace has one, else the workspace's label. */
  label: string
  /** A workspace of another machine that is not linked to one of this machine. */
  machine?: string
  intent: Intent | null
  agents: AgentAt[]
}

/** Where every live agent is, worked out over the whole workspace (agents cross units). */
export function agents(ws: Workspace): AgentAt[] {
  return agentGroups(ws).flatMap(g => g.agents)
}

/**
 * Live agents by runtime workspace: groups with someone working first, then by label;
 * inside, the manager, then working, blocked, holding work, idle.
 */
export function agentGroups(ws: Workspace): AgentGroup[] {
  const tasks = unitsOf(ws).flatMap(unit => unit.tasks.map(node => ({ unit, node })))
  const groups = new Map<string, AgentGroup>()
  for (const s of ws.sessions.filter(s => !s.ended)) {
    const mine = (state: string) => tasks.filter(({ node }) => node.state === state && (s.session && node.agentSession ? node.agentSession === s.session : node.agent === s.agent))
    const running = mine('running')[0], todo = mine('todo')[0], last = mine('done').at(-1) ?? null
    // Working without a node of its own (a manager planning, say): working all the same.
    const [state, at]: [AgentState, AgentAt['at']] = running ? ['working', running] : s.state === 'working' ? ['working', todo ?? last] : todo ? ['queued', todo] : ['idle', last]
    const intent = ws.intents.find(i => i.id === s.intent) ?? null
    const key = s.group?.id ?? intent?.id ?? ''
    if (!groups.has(key)) groups.set(key, { key, label: s.group?.label ?? intent?.title ?? '', machine: s.group?.machine, intent, agents: [] })
    groups.get(key)!.agents.push({ agent: s.agent, session: s.session, topic: s.topic, state, blocked: s.state === 'blocked', manager: !!s.manager, machine: s.machine, unreachable: s.unreachable, at })
  }
  const rank = (a: AgentAt) => (a.blocked ? 0 : a.manager ? 1 : a.state === 'working' ? 2 : a.state === 'queued' ? 3 : 4)
  const list = [...groups.values()]
  for (const g of list) g.agents.sort((a, b) => rank(a) - rank(b))
  const busy = (g: AgentGroup) => (g.agents.some(a => a.state === 'working') ? 0 : 1)
  return list.sort((a, b) => busy(a) - busy(b) || a.label.localeCompare(b.label))
}

/**
 * An agent in the panel's tree: its state in a word, idle (or blocked) since when, and the
 * task it is on or holds, else the last it worked on. A question agent's task is the one
 * asked about, and `asker` says who asked (null: the user).
 */
export interface TreeAgent {
  s: Session
  state: 'working' | 'idle' | 'blocked' | 'gone'
  /** Idle or blocked since, in milliseconds; NaN when nothing says. */
  since: number
  at: { unit: Unit; node: Task } | null
  /** `at` is work it is on or holds, not the last it finished. */
  current: boolean
  /** Idle and holding no work: closing it loses nothing. */
  closable: boolean
  asker?: string | null
  /** Question agents asking about its tasks. */
  kids: TreeAgent[]
}

/** A runtime workspace: its manager, and the workers (with their question agents) under it. */
export interface TreeGroup {
  key: string
  label: string
  /** A workspace of another machine that is not linked to one of this machine. */
  machine?: string
  /** The workspace's current intent. */
  intent: Intent | null
  manager: TreeAgent | null
  workers: TreeAgent[]
}

const QUESTION = /^why-(q\d+)$/

/**
 * Who runs under whom, worked out from tasks (the runtime does not say who started a
 * session): per workspace its manager, then the workers of tasks, each with the question
 * agents (`why-qN`) asking about its work. A gone worker shows only while it holds
 * unfinished work. Sessions that never took a task are `own`: the user's, not workriver's.
 */
export function agentTree(ws: Workspace): { groups: TreeGroup[]; own: TreeAgent[] } {
  const tasks = unitsOf(ws).flatMap(unit => unit.tasks.map(node => ({ unit, node })))
  const intentOfUnit = (u: Unit) => ws.intents.find(i => i.units.includes(u))!
  const mine = (s: Session) => tasks.filter(({ node }) => (s.session && node.agentSession ? node.agentSession === s.session : node.agent === s.agent))
  const agent = (s: Session, held: TreeAgent['at'][]): TreeAgent => {
    const own = held.filter((a): a is NonNullable<TreeAgent['at']> => !!a)
    const live = own.find(a => a.node.state === 'running') ?? own.find(a => a.node.state === 'todo')
    const last = own.filter(a => a.node.state === 'done').sort((a, b) => (ms(a.node.delivered) || 0) - (ms(b.node.delivered) || 0)).at(-1) ?? null
    const state = s.ended ? 'gone' : s.state === 'blocked' ? 'blocked' : s.state === 'working' ? 'working' : 'idle'
    const since = state === 'blocked' ? ms(s.blockedSince) : ms(s.idleSince) || ms(last?.node.delivered)
    return { s, state, since, at: live ?? last, current: !!live, closable: state === 'idle' && !live && own.length > 0, kids: [] }
  }
  const groups = new Map<string, TreeGroup>()
  const group = (key: string, label: string, intent: Intent | null, machine?: string) => {
    if (!groups.has(key)) groups.set(key, { key, label, machine, intent, manager: null, workers: [] })
    return groups.get(key)!
  }
  const groupOf = (s: Session) => {
    const intent = ws.intents.find(i => i.id === s.intent) ?? null
    return group(s.group?.id ?? intent?.id ?? '', s.group?.label ?? intent?.title ?? '', intent, s.group?.machine)
  }
  const own: TreeAgent[] = [], questions: TreeAgent[] = []
  for (const s of ws.sessions) {
    const q = QUESTION.exec(s.agent)?.[1]
    if (q) {
      if (s.ended) continue
      const about = tasks.find(({ node }) => node.threads?.some(t => t.id === q))
      const a = agent(s, [])
      // About a unit as a whole: in its workspace, saying who asked.
      const unitThread = about ? undefined : unitsOf(ws).flatMap(u => u.threads ?? []).find(t => t.id === q)
      if (unitThread) { groupOf(s).workers.push({ ...a, asker: unitThread.turns[0]?.by ?? null }); continue }
      if (!about) { own.push(a); continue }
      questions.push({ ...a, at: about, current: true, asker: about.node.threads!.find(t => t.id === q)!.turns[0]?.by ?? null })
      continue
    }
    if (s.manager && !s.ended) {
      const g = groupOf(s)
      if (!g.manager) { g.manager = agent(s, []); continue }
    }
    const held = mine(s)
    if (s.ended) {
      // Gone: shown only while it holds unfinished work, in the workspace of that work.
      const open = held.find(a => a.node.state === 'running' || a.node.state === 'todo')
      if (!open) continue
      const i = intentOfUnit(open.unit)
      group(i.workspace?.id ?? i.id, i.workspace?.label ?? i.title, i).workers.push(agent(s, held))
      continue
    }
    if (!held.length) { own.push(agent(s, [])); continue }
    groupOf(s).workers.push(agent(s, held))
  }
  // A question agent hangs under the worker whose task was asked about, else in its own workspace.
  const workers = [...groups.values()].flatMap(g => g.workers)
  for (const q of questions) {
    const n = q.at!.node
    const parent = workers.find(w => w.state !== 'gone' && (n.agentSession && w.s.session ? w.s.session === n.agentSession : w.s.agent === n.agent))
    if (parent) parent.kids.push(q)
    else groupOf(q.s).workers.push(q)
  }
  const rank = (a: TreeAgent) => (a.state === 'gone' ? 4 : a.closable ? 3 : { blocked: 0, working: 1, idle: 2 }[a.state])
  const list = [...groups.values()]
  for (const g of list) g.workers.sort((a, b) => rank(a) - rank(b) || a.s.agent.localeCompare(b.s.agent))
  const busy = (g: TreeGroup) => (g.manager?.state === 'working' || g.workers.some(w => w.state === 'working') ? 0 : 1)
  list.sort((a, b) => busy(a) - busy(b) || a.label.localeCompare(b.label))
  return { groups: list, own: own.sort((a, b) => a.s.agent.localeCompare(b.s.agent)) }
}

export interface Attention {
  now: Working[]
  /** Gates the user has not passed yet: agents are stuck behind them. */
  gates: Task[]
  /** Reviews and gates with findings waiting for the user's decision. */
  asked: { review: Task; count: number }[]
  /** Commits that name no node. */
  loose: Task[]
}
export function attention(unit: Unit): Attention {
  return {
    now: working(unit),
    gates: unit.tasks.filter(n => n.kind === 'gate' && n.state === 'waiting'),
    asked: unit.tasks.map(review => ({ review, count: openCount(review) })).filter(a => a.count),
    loose: unit.tasks.filter(n => n.state === 'unregistered'),
  }
}

// ---- the inbox: everything that asks something of the user, across all units ----

export interface InboxRow {
  key: string
  kind: 'agent' | 'stalled' | 'machine' | 'gate' | 'decision' | 'loose' | 'chore' | 'merge'
  /** The intent; null for an agent in a workspace no intent follows, and for a project's chore. */
  intent: Intent | null
  /** A chore of no intent: its project's name, if it has a project. */
  project?: string
  /** agent: the agent waiting in its terminal, and where it runs. */
  agent?: { name: string; session?: string; topic?: string; place: string }
  /** machine: a machine past an alert threshold. */
  alert?: MachineAlert
  /** The unit it belongs to; null for a chore that concerns the intent as a whole. */
  unit: Unit | null
  /** What a click opens; null for a chore. */
  node: Task | null
  pick: Pick
  chore: Chore | null
  /** Since when it has waited, in milliseconds. */
  since: number
}

/** Live agents waiting for a person in their terminal (a question, an approval). */
export const blockedAgents = (ws: Workspace | null) => (ws?.sessions ?? []).filter(s => !s.ended && s.blockedSince)

export interface Inbox {
  /** Gates: an agent cannot go on. */
  blocking: InboxRow[]
  /** Machines past an alert threshold: they slow every agent there; they leave by themselves. */
  machines: InboxRow[]
  /** Reviews and gates with open findings, the highest open severity first. */
  decisions: InboxRow[]
  /** Commits that name no node. */
  gaps: InboxRow[]
  /** Chores, and ready units of an intent with no manager that may be merged already: they block nobody. */
  later: InboxRow[]
}

/** The open findings of a node, the most severe first. */
export const openFindings = (n: Task) =>
  findingsOf(n).filter(waits).sort((a, b) => SEVERITIES.indexOf(a.severity) - SEVERITIES.indexOf(b.severity))

export function inbox(ws: Workspace): Inbox {
  const box: Inbox = { blocking: [], machines: [], decisions: [], gaps: [], later: [] }
  const stalled: InboxRow[] = []
  for (const intent of ws.intents) for (const unit of intent.units) {
    const row = (kind: InboxRow['kind'], unit: Unit, node: Task, since: number, finding?: string): InboxRow =>
      ({ key: `${kind}:${unit.id}:${node.id}`, kind, intent, unit, node, pick: { kind: 'task', id: node.id, finding }, chore: null, since })
    const a = attention(unit)
    for (const n of a.gates) box.blocking.push(row('gate', unit, n, ms(n.since)))
    // A worker that stopped without delivering, and a reminder did not help (or it is gone).
    for (const n of unit.tasks.filter(n => n.stalled?.escalated)) stalled.push(row('stalled', unit, n, ms(n.stalled!.since)))
    for (const { review: n } of a.asked) box.decisions.push(row('decision', unit, n, ms(n.since), openFindings(n)[0].id))
    for (const n of a.loose) box.gaps.push(row('loose', unit, n, Math.min(...unit.commits.filter(c => n.output.commits?.includes(c.id)).map(c => ms(c.date)))))
    // Ready and maybe merged where git cannot see it (a squash), or ready for long: with no
    // manager to tell, the user is the one to check.
    if (!intent.manager && unit.progress === 'ready' && (unit.squash || unit.overdue))
      box.later.push({ key: `merge:${unit.id}`, kind: 'merge', intent, unit, node: null, pick: null, chore: null, since: ms(unit.overdue) || ms(unit.commits.at(-1)?.date) || Date.now() })
  }
  for (const intent of ws.intents)
    for (const c of intent.chores.filter(c => c.state === 'open'))
      box.later.push({ key: `chore:${c.id}`, kind: 'chore', intent, unit: intent.units.find(u => u.id === c.unit) ?? null, node: null, pick: null, chore: c, since: ms(c.since) })
  // Chores of no intent: under their project's name, or under nothing.
  for (const c of (ws.chores ?? []).filter(c => c.state === 'open'))
    box.later.push({ key: `chore:${c.id}`, kind: 'chore', intent: null, project: ws.projects?.find(p => p.id === c.project)?.title, unit: null, node: null, pick: null, chore: c, since: ms(c.since) })
  const oldest = (a: InboxRow, b: InboxRow) => a.since - b.since
  const top = (r: InboxRow) => SEVERITIES.indexOf(openFindings(r.node!)[0].severity)
  // Agents waiting for a person in their terminal come first: nothing moves until someone looks.
  const waiting = blockedAgents(ws).map((s): InboxRow => {
    const intent = ws.intents.find(i => i.id === s.intent) ?? null
    return {
      key: `agent:${s.session ?? s.agent}`, kind: 'agent', intent, unit: null, node: null, pick: null, chore: null, since: ms(s.blockedSince),
      agent: { name: s.agent, session: s.session, topic: s.topic, place: intent?.title ?? s.group?.label ?? '' },
    }
  }).sort(oldest)
  box.blocking.sort(oldest)
  box.blocking.unshift(...waiting, ...stalled.sort(oldest))
  box.machines = (ws.alerts ?? [])
    .map((a): InboxRow => ({ key: `machine:${a.machine}:${a.metric}`, kind: 'machine', alert: a, intent: null, unit: null, node: null, pick: null, chore: null, since: ms(a.since) }))
    .sort(oldest)
  box.decisions.sort((a, b) => top(a) - top(b) || oldest(a, b))
  box.gaps.sort(oldest)
  box.later.sort(oldest)
  return box
}

// ---- the file tree: where the unit changed, plans to change, or is writing right now ----

/** One row of the file tree: a file, or a folder; a chain of single folders is one row. */
export interface TreeRow {
  /** The deepest path the row stands for; ROOT for the root. */
  id: string
  /** A file's name, or folder names joined by '/' for a chain. */
  name: string
  folder: boolean
  children: string[]
  add: number
  del: number
  /** Files the unit changed: how. */
  status?: 'A' | 'M' | 'D'
  /** Work not done yet expects to change it, and nothing has yet. */
  planned: boolean
  /** Not in the repository yet: the unit adds it. */
  ghost: boolean
  /** Agents writing it, or something inside it, right now. */
  writers: string[]
  /** The unit changed, plans to change or is writing it, or something inside it. */
  changed: boolean
  /** Part of what is picked (with nothing picked: of the whole unit). */
  inPick: boolean
}

export const ROOT = '/'

/**
 * Lines picked in the code, by their numbers: in the single-file view, in the file as shown;
 * in the list, in the file as `commit` (an index into the unit's commits) left it.
 */
export interface Lines { path: string; from: number; to: number; commit?: number }

/**
 * Which commit last changed each line of a file, as of commit `upTo`: one index into the
 * unit's commits per line, -1 for a line from before the unit.
 */
export function blame(unit: Unit, path: string, upTo: number): number[] {
  let owner: number[] = []
  let started = false
  unit.commits.slice(0, upTo + 1).forEach((c, k) => {
    const f = c.files.find(x => x.path === path)
    if (!f) return
    if (!started) { owner = (f.before ?? '').split('\n').slice(0, -1).map(() => -1); started = true }
    const next: number[] = []
    let at = 0
    for (const part of diffLines(f.before ?? '', f.after ?? '')) {
      const n = part.count ?? 0
      if (part.removed) at += n
      else if (part.added) for (let i = 0; i < n; i++) next.push(k)
      else { next.push(...owner.slice(at, at + n)); at += n }
    }
    owner = next
  })
  return owner
}

export interface FileTree {
  rows: Map<string, TreeRow>
  /** Folders open by default: those holding the pick's files and (unless `narrow`) files being written. */
  open: string[]
}

export interface FileDelta {
  /** Indexes of the scope's commits that touch the file, oldest first. */
  touched: number[]
  before: string | null
  after: string | null
  status: 'A' | 'M' | 'D'
  add: number
  del: number
}

/**
 * A file's change over some commits taken together: its text before the first of them that
 * touches it against its text after the last. Null when none touches it. The tree's counts
 * and the single-file view both come from here, so they agree.
 */
export function fileDelta(unit: Unit, path: string, scope: number[]): FileDelta | null {
  const touched = scope.filter(k => unit.commits[k].files.some(f => f.path === path))
  if (!touched.length) return null
  const at = (k: number) => unit.commits[k].files.find(f => f.path === path)!
  const first = at(touched[0]), last = at(touched.at(-1)!)
  const before = first.before, after = last.after
  const { add, del } = counted(first, last)
  return { touched, before, after, status: before === null ? 'A' : after === null ? 'D' : 'M', add, del }
}

// Line counts from one file version's `before` to another's `after`, kept per pair: a file's
// versions stay the same objects while its unit's code is unchanged, and diffing whole texts
// again on every pick made picking a task in a big unit take up to a second.
const counts = new WeakMap<FileChange, WeakMap<FileChange, { add: number; del: number }>>()
function counted(first: FileChange, last: FileChange) {
  if (first === last) return first
  let to = counts.get(first)
  if (!to) counts.set(first, (to = new WeakMap()))
  let c = to.get(last)
  if (!c) { const { add, del } = changeOf(first.before, last.after); to.set(last, (c = { add, del })) }
  return c
}

/**
 * The change shown for a file, in the tree and in the single-file view alike: over the
 * picked work's commits when they touch it, otherwise over the whole unit.
 */
export function shownDelta(unit: Unit, sel: Selection, path: string): FileDelta | null {
  return fileDelta(unit, path, sel.commits) ?? fileDelta(unit, path, unit.commits.map((_, k) => k))
}

export function fileTree(root: ModuleNode, unit: Unit, sel: Selection, onlyChanged: boolean, narrow = false): FileTree {
  const repo = new Set<string>(), dirs = new Set<string>()
  const walk = (n: ModuleNode) => { if (n.children) { if (n.path) dirs.add(n.path); n.children.forEach(walk) } else repo.add(n.path) }
  walk(root)
  // What the unit did, file by file, as one change over a scope: the pick's commits for the
  // files it touched, the whole unit's for the others. The single-file view shows the same.
  const did = new Map<string, FileDelta & { inPick: boolean }>()
  for (const path of new Set(unit.commits.flatMap(c => c.files.map(f => f.path))))
    did.set(path, { ...shownDelta(unit, sel, path)!, inPick: sel.commits.some(k => unit.commits[k].files.some(f => f.path === path)) })
  const open = unit.tasks.filter(n => n.state !== 'done')
  const plans = open.flatMap(n => n.plans)
  const writers = new Map<string, string[]>()
  for (const { node, files } of working(unit)) for (const f of files) writers.set(f, [...(writers.get(f) ?? []), node.agent ?? '?'])
  const isDir = (p: string) => dirs.has(p) || !/\.[A-Za-z0-9]+$/.test(p)
  const under = (p: string, list: string[]) => list.some(q => p === q || p.startsWith(q + '/'))
  const picked = (p: string) => !sel.node || !!did.get(p)?.inPick || under(p, sel.plans ?? [])
  // Every path to draw: the repository, plus what the unit adds or plans to add.
  const files = new Set([...repo, ...did.keys(), ...writers.keys(), ...plans.filter(p => !isDir(p))])
  const kids = new Map<string, Set<string>>([[ROOT, new Set()]])
  const link = (p: string) => {
    const up = p.includes('/') ? p.slice(0, p.lastIndexOf('/')) : ROOT
    if (!kids.has(up)) { kids.set(up, new Set()); link(up) }
    kids.get(up)!.add(p)
  }
  for (const f of files) link(f)
  for (const p of plans.filter(isDir)) if (!kids.has(p)) { kids.set(p, new Set()); link(p) }

  const rows = new Map<string, TreeRow>()
  const build = (p: string): TreeRow => {
    const sub = kids.get(p)
    const name = p === ROOT ? '' : p.slice(p.lastIndexOf('/') + 1)
    if (!sub) {
      const d = did.get(p), w = writers.get(p) ?? []
      const planned = !d && plans.includes(p)
      return { id: p, name, folder: false, children: [], add: d?.add ?? 0, del: d?.del ?? 0, status: d?.status, planned, ghost: !repo.has(p) && !d,
        writers: w, changed: !!d || planned || w.length > 0, inPick: picked(p) && (!!d || planned || w.length > 0) }
    }
    const all = [...sub].map(build).filter(r => !onlyChanged || r.changed)
      .sort((a, b) => Number(b.folder) - Number(a.folder) || a.name.localeCompare(b.name))
    const planned = plans.includes(p)
    const row: TreeRow = {
      id: p, name, folder: true, children: all.map(r => r.id),
      add: all.reduce((s, r) => s + r.add, 0), del: all.reduce((s, r) => s + r.del, 0),
      planned, ghost: !dirs.has(p) && p !== ROOT, writers: [...new Set(all.flatMap(r => r.writers))],
      changed: planned || all.some(r => r.changed), inPick: (planned && picked(p)) || all.some(r => r.inPick),
    }
    // A folder holding nothing but one folder is shown as one row with it, as GitHub does.
    if (p !== ROOT && all.length === 1 && all[0].folder) {
      const only = all[0]
      rows.delete(only.id)
      rows.set(only.id, { ...only, name: `${name}/${only.name}`, planned: only.planned || planned })
      return rows.get(only.id)!
    }
    for (const r of all) rows.set(r.id, r)
    return row
  }
  rows.set(ROOT, build(ROOT))
  const focus = [...rows.values()].filter(r => !r.folder && (r.inPick || (!narrow && r.writers.length)) && r.changed).map(r => r.id)
  return { rows, open: [...rows.values()].filter(r => r.folder && r.id !== ROOT && focus.some(f => f.startsWith(r.id + '/'))).map(r => r.id) }
}

/**
 * The tree narrowed to files whose path holds `q` (any case), with the folders above them,
 * all open; the tree as it is for an empty `q`.
 */
export function filterTree(ft: FileTree, q: string): FileTree {
  const needle = q.trim().toLowerCase()
  if (!needle) return ft
  const rows = new Map<string, TreeRow>()
  const keep = (id: string): boolean => {
    const r = ft.rows.get(id)
    if (!r) return false
    if (!r.folder) {
      if (!id.toLowerCase().includes(needle)) return false
      rows.set(id, r)
      return true
    }
    const children = r.children.filter(keep)
    rows.set(id, { ...r, children })
    return children.length > 0
  }
  keep(ROOT)
  return { rows, open: [...rows.values()].filter(r => r.folder && r.id !== ROOT).map(r => r.id) }
}

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
  result: 'done' | 'problem'
  note: string
  attachments: Attachment[]
  findings: NewFinding[]
  at: string
}

/** Everything the user has put in during this sitting; the data shown is the data with these on top. */
export interface Edits {
  /** By `unit:task`. */
  gates: Record<string, GateResult>
  /** By `unit:task:finding`. */
  decisions: Record<string, { decision: Decision; note: string }>
  chores: string[]
  /** Planned intents merged into another (as the service records it: dropped, with this reason). */
  merges: { into: string; from: string[]; title?: string }[]
  /** Planned intents started in a runtime workspace, by intent. */
  starts: Record<string, { id: string; label: string; manager?: string; managerState?: Intent['managerState'] }>
}

export const NO_EDITS: Edits = { gates: {}, decisions: {}, chores: [], merges: [], starts: {} }

const STATE = { accept: 'accepted', revise: 'revised', reject: 'rejected', defer: 'deferred' } as const

/** The workspace as the user's inputs leave it. Nothing in `ws` is changed. */
export function withEdits(ws: Workspace, edits: Edits): Workspace {
  const task = (unit: Unit, n: Task): Task => {
    const g = edits.gates[`${unit.id}:${n.id}`]
    let out = n
    if (g) {
      // Ids per severity, as a reviewer numbers them.
      const seen: Record<string, number> = {}
      const findings: Finding[] = g.result === 'problem'
        ? g.findings.map(f => ({
            id: `${f.severity}-${(seen[f.severity] = (seen[f.severity] ?? 0) + 1)}`, severity: f.severity, problem: f.problem,
            impact: f.impact, evidence: f.evidence, state: 'open', leadsTo: [], attachments: f.attachments,
          }))
        : []
      out = { ...n, state: 'done', since: g.at, output: { ...n.output, note: g.note, attachments: g.attachments, findings } }
    }
    const found = out.output.findings
    if (found?.some(f => edits.decisions[`${unit.id}:${n.id}:${f.id}`]))
      out = { ...out, output: { ...out.output, findings: found.map(f => {
        const d = edits.decisions[`${unit.id}:${n.id}:${f.id}`]
        return d ? { ...f, state: STATE[d.decision], decidedBy: 'user' as const, note: d.note } : f
      }) } }
    return out
  }
  // Only a still planned one: once the service has it, it is dropped there already.
  const mergedInto = (i: Intent) => (i.state === 'planned' ? edits.merges.find(m => m.from.includes(i.id))?.into : undefined)
  // The target's note gains what the merged ones said, as the service writes it (`merged_note`).
  const said = (x: Intent) => [`Merged from ${x.id}: ${x.title}`, x.goal?.trim(), x.note?.trim(), x.external && `${x.external.system}:${x.external.key}${x.external.url ? ` ${x.external.url}` : ''}`].filter(Boolean).join('\n')
  const noteOf = (i: Intent) => {
    const blocks = ws.intents.filter(x => mergedInto(x) === i.id).map(said)
    return blocks.length ? [i.note?.trimEnd(), ...blocks].filter(Boolean).join('\n\n') : i.note
  }
  // And its new title, if one was given.
  const titleOf = (i: Intent) => edits.merges.find(m => m.into === i.id && m.title && ws.intents.some(x => mergedInto(x) === i.id))?.title ?? i.title
  return {
    ...ws,
    chores: ws.chores?.map(c => (edits.chores.includes(c.id) ? { ...c, state: 'done' as const } : c)),
    intents: ws.intents.map(i => ({
      ...i,
      ...(mergedInto(i) && { state: 'dropped' as const, reason: `merged into ${mergedInto(i)}` }),
      ...(i.state === 'planned' && edits.starts[i.id] && { state: 'active' as const, workspace: { id: edits.starts[i.id].id, label: edits.starts[i.id].label }, manager: edits.starts[i.id].manager, managerState: edits.starts[i.id].managerState }),
      note: noteOf(i),
      title: titleOf(i),
      units: i.units.map(u => ({ ...u, tasks: u.tasks.map(n => task(u, n)) })),
      chores: i.chores.map(c => (edits.chores.includes(c.id) ? { ...c, state: 'done' as const } : c)),
    })),
  }
}

// ---- the commits of every unit, newest first ----

export interface FeedRow { intent: Intent; unit: Unit; commit: Commit; task: Task | null }

/** Every commit across intents and units, newest first; work not committed yet is left out. */
export function feed(ws: Workspace): FeedRow[] {
  return ws.intents
    .flatMap(intent => intent.units.flatMap(unit => unit.commits.filter(c => !c.live).map(commit => ({
      intent, unit, commit, task: unit.tasks.find(n => n.output.commits?.includes(commit.id)) ?? null,
    }))))
    .sort((a, b) => ms(b.commit.date) - ms(a.commit.date))
}

/** Agents with uncommitted changes right now, and the files they are writing. */
export const writingNow = (ws: Workspace) =>
  ws.intents.flatMap(intent => intent.units.flatMap(unit => working(unit).filter(w => w.files.length).map(w => ({ intent, unit, ...w }))))

// ---- the commits as a git graph ----

/** A unit's lane in one row: whether it goes on into the row above and the row below. */
export interface GraphSeg {
  lane: number
  unit: Unit
  /** line: on to a newer row of its unit; join: it merges into the base here. */
  up: 'line' | 'join' | null
  /** line: on to an older row; fork: it leaves the base here. */
  down: 'line' | 'fork' | null
  /** Dashed: what is not committed yet. */
  upDash: boolean
  downDash: boolean
}
export interface GraphRow {
  unit: Unit
  /** A commit, with its task (null: none found) … */
  commit?: Commit
  task: Task | null
  /** … or an agent's uncommitted work. */
  writing?: Working
  /** The row's own lane (from 1; 0 is the base). */
  lane: number
  /** Every unit lane drawn in this row, its own included. */
  segs: GraphSeg[]
}
export interface Graph {
  repo: Repo | undefined
  /** The base branch the units leave from (several, joined, when they differ). */
  base: string
  rows: GraphRow[]
  lanes: number
}

/** Which of the lane colours (`--lane-0` … `--lane-5`) a unit's branch takes: from its id, so it stays put. */
export const laneTone = (unit: string) => [...unit].reduce((n, ch) => (n * 31 + ch.charCodeAt(0)) % 997, 7) % 6

/**
 * Every unit's commits as `git log --graph` draws them, newest first, one graph per
 * repository: the base as lane 0 (a plain line, labelled with its name: its own commits are not drawn), one
 * lane per unit from its first commit, where it leaves the base, to its last, where a merged
 * unit joins it again (at `merged` when known). Uncommitted work is on top, dashed. A lane is
 * used again once the unit on it has ended, so the graph is as wide as the most units
 * running side by side.
 */
export function commitGraph(ws: Workspace): Graph[] {
  const live = writingNow(ws)
  const repos = [...new Set(unitsOf(ws).filter(u => u.commits.length).map(u => u.repo))]
  return repos.map(repo => {
    const rows: GraphRow[] = [
      ...live.filter(w => w.unit.repo === repo).map(w => ({ unit: w.unit, writing: { node: w.node, files: w.files }, task: w.node, lane: 0, segs: [] })),
      ...feed(ws).filter(r => r.unit.repo === repo).map(r => ({ unit: r.unit, commit: r.commit, task: r.task, lane: 0, segs: [] })),
    ]
    const time = (r: GraphRow) => (r.writing ? Infinity : ms(r.commit!.date))
    const spans = new Map<Unit, { top: number; bottom: number; join: boolean; dash: [number, number] | null }>()
    rows.forEach((r, i) => {
      const s = spans.get(r.unit)
      if (s) s.bottom = i
      else spans.set(r.unit, { top: i, bottom: i, join: false, dash: null })
    })
    for (const [u, s] of spans) {
      s.join = u.status === 'merged' || u.progress === 'merged'
      // Merged later than its last commit: its lane runs on up to where it was merged.
      const at = ms(u.merged)
      const k = s.join && Number.isFinite(at) ? rows.findIndex(r => time(r) <= at) : -1
      if (k >= 0) s.top = Math.min(s.top, k)
      // Uncommitted work is dashed down to the unit's newest commit (or to where it forks).
      const w = rows.findIndex(r => r.unit === u && r.writing)
      if (w >= 0) { const c = rows.findIndex((r, i) => i > w && r.unit === u && !r.writing); s.dash = [w, c < 0 ? s.bottom + 1 : c] }
    }
    // Lanes, first fit by where each unit starts (from the top): one ended frees its lane.
    const lane = new Map<Unit, number>(), ends: number[] = []
    for (const [u, s] of [...spans].sort((a, b) => a[1].top - b[1].top)) {
      let k = ends.findIndex(e => e < s.top)
      if (k < 0) k = ends.length
      ends[k] = s.bottom
      lane.set(u, k + 1)
    }
    rows.forEach((r, i) => {
      r.lane = lane.get(r.unit)!
      for (const [u, s] of spans) {
        if (i < s.top || i > s.bottom) continue
        const d = s.dash
        r.segs.push({
          lane: lane.get(u)!, unit: u,
          up: i > s.top ? 'line' : s.join ? 'join' : null,
          down: i < s.bottom ? 'line' : 'fork',
          upDash: !!d && i > d[0] && i <= d[1],
          downDash: !!d && i >= d[0] && i < d[1],
        })
      }
    })
    const base = [...new Set(unitsOf(ws).filter(u => u.repo === repo && u.base).map(u => u.base!))].join(', ')
    return { repo: ws.repos.find(x => x.path === repo), base, rows, lanes: ends.length }
  })
}

// ---- quiet: nothing has moved for a while ----

/** No movement for this long is quiet. ponytail: fixed at 3 days; a setting if users ask. */
export const QUIET = 3 * 864e5

const finished = (u: Unit) => u.progress === 'merged' || u.progress === 'done' || u.status === 'merged'
/** Work running with its agent at it. */
const busy = (u: Unit) => u.tasks.some(n => n.state === 'running' && !n.stalled)

/** When a unit last moved (a task started, delivered or dropped, a gate opened, a finding decided, a commit, its merge); NaN: never. */
export function lastMove(u: Unit): number {
  const times = [
    ...u.tasks.flatMap(n => [ms(n.started), ms(n.delivered), ms(n.dropped?.at), n.kind === 'gate' ? ms(n.since) : NaN, ms(n.stalled?.since), ...findingsOf(n).map(f => ms(f.decided))]),
    ...u.commits.filter(c => !c.live).map(c => ms(c.date)),
    ms(u.merged),
  ].filter(Number.isFinite)
  return times.length ? Math.max(...times) : NaN
}

const quiet = (last: number, now: number) => (Number.isFinite(last) && now - last > QUIET ? last : null)

/**
 * Since when a unit has not moved, in milliseconds; null when it is not quiet: merged or
 * done, work running with an agent at it, never moved, or moved within `QUIET`.
 */
export function quietSince(u: Unit, now = Date.now()): number | null {
  return finished(u) || busy(u) ? null : quiet(lastMove(u), now)
}

/**
 * An active intent where nothing moved for `QUIET`, in any of its units or its own tasks
 * (its intent-level unit): since the latest movement. Units that never moved do not count.
 */
export function intentQuietSince(i: Intent, now = Date.now()): number | null {
  if ((i.state ?? 'active') !== 'active' || i.units.some(busy)) return null
  return quiet(Math.max(-Infinity, ...i.units.map(lastMove).filter(Number.isFinite)), now)
}
