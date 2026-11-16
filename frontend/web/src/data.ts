// Loading the workspace (from the service, or the sample with VITE_MOCK=1) and everything
// the views derive from it. Apart from the loading, pure functions only.
import { diffLines, structuredPatch } from 'diff'
import type { T } from './i18n'
import type { Attachment, Chore, Commit, Intent, Finding, Unit, ModuleNode, Severity, Task, Workspace } from './types'

/** Sample data instead of the service, with `VITE_MOCK=1`. */
export const MOCK = import.meta.env.VITE_MOCK === '1'

/** The data carries whole texts; the hunks and line counts are worked out from them. */
function prepare(ws: Workspace): Workspace {
  for (const unit of unitsOf(ws)) for (const c of unit.commits) c.files = c.files.map(f => ({ ...f, ...changeOf(f.before, f.after) }))
  if (import.meta.env.DEV) checkCounts(ws)
  return ws
}

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
  for (const n of unitsOf(ws).flatMap(u => u.tasks)) for (const f of n.output.findings ?? []) for (const a of f.attachments ?? []) a.url = pictures[a.url] ?? a.url
  return prepare(ws)
}

/**
 * The one place data comes from: the workspace now, and again whenever the service says it
 * changed. `onDown` is called while the service cannot be reached. Returns a stop function.
 */
export function watchWorkspace(onData: (ws: Workspace) => void, onDown: () => void): () => void {
  let stopped = false
  let asked = 0
  // Only the latest answer counts; an older one arriving late is dropped.
  const get = () => {
    const n = ++asked
    loadWorkspace().then(ws => { if (!stopped && n === asked) onData(ws) }, () => { if (!stopped && n === asked) onDown() })
  }
  get()
  if (MOCK) return () => { stopped = true }
  const events = new EventSource('/api/events')
  events.addEventListener('changed', get)
  // The stream reconnects by itself; what changed while it was down is fetched then.
  let opened = false
  events.onopen = () => { if (opened) get(); opened = true }
  events.onerror = () => { if (!stopped) onDown() }
  return () => { stopped = true; events.close() }
}

/** The repository a unit works in. */
export const repoOf = (ws: Workspace, unit: Unit) => ws.repos.find(r => r.path === unit.repo && r.machine === unit.machine)!

/**
 * Self-check, in development: for every unit and every scope (nothing picked, each node
 * picked), each file's count in the tree equals the change the single-file view shows.
 */
function checkCounts(ws: Workspace) {
  for (const unit of unitsOf(ws).filter(u => !u.intentLevel))
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

/** Every unit, across intents. */
export const unitsOf = (ws: Workspace | null) => ws?.intents.flatMap(i => i.units) ?? []
/** The intent a unit belongs to. */
export const intentOf = (ws: Workspace, unitId: string) => ws.intents.find(i => i.units.some(u => u.id === unitId))

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

/** The folder a path stands for: a file's folder, or the path itself when it is a folder. */
const folderOf = (p: string) => (/\.[A-Za-z0-9]+$/.test(p) ? p.slice(0, p.lastIndexOf('/')) : p)

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

export interface Attention {
  now: Working[]
  /** Gates the user has not passed yet: agents are stuck behind them. */
  gates: Task[]
  /** Reviews and gates with findings waiting for the user's decision. */
  asked: { review: Task; count: number }[]
  /** Commits that name no node. */
  loose: Task[]
  /**
   * Work cut from the unit whose later rounds reached folders the first round neither
   * changed nor planned; since: when the first of those later rounds got there.
   */
  out: { root: Task; far: string[]; since: number }[]
}
export function attention(unit: Unit): Attention {
  const folders = (n: Task) => [
    ...unit.commits.filter(c => n.output.commits?.includes(c.id)).flatMap(c => c.files.map(f => folderOf(f.path))),
    ...(n.state === 'done' ? [] : n.plans.map(folderOf)),
  ]
  const roots = unit.tasks.filter(n => n.kind === 'implement' && n.inputs.every(i => i.from === 'start'))
  const home = new Set(roots.flatMap(folders))
  const out: Attention['out'] = []
  for (const root of roots) {
    const below = [...related(unit, root.id)].filter(id => id !== root.id && id !== 'start').map(id => unit.tasks.find(n => n.id === id)!)
    const far = new Set(below.flatMap(folders).filter(f => !home.has(f)))
    // A node got there when its first commit landed, or, with none yet, when it began to wait.
    const reached = (n: Task) => {
      const landed = unit.commits.filter(c => n.output.commits?.includes(c.id)).map(c => ms(c.date))
      return landed.length ? Math.min(...landed) : ms(n.since)
    }
    const times = below.filter(n => folders(n).some(f => far.has(f))).map(reached).filter(Number.isFinite)
    if (far.size) out.push({ root, far: [...far], since: Math.min(...times) })
  }
  return {
    now: working(unit),
    gates: unit.tasks.filter(n => n.kind === 'gate' && n.state === 'waiting'),
    asked: unit.tasks.map(review => ({ review, count: openCount(review) })).filter(a => a.count),
    loose: unit.tasks.filter(n => n.state === 'unregistered'),
    out,
  }
}

// ---- the inbox: everything that asks something of the user, across all units ----

export interface InboxRow {
  key: string
  kind: 'agent' | 'gate' | 'decision' | 'loose' | 'far' | 'chore'
  /** The intent; null for an agent in a workspace no intent follows, and for a project's chore. */
  intent: Intent | null
  /** A chore of no intent: its project's name, if it has a project. */
  project?: string
  /** agent: the agent waiting in its terminal, and where it runs. */
  agent?: { name: string; session?: string; topic?: string; place: string }
  /** The unit it belongs to; null for a chore that concerns the intent as a whole. */
  unit: Unit | null
  /** What a click opens; null for a chore. */
  node: Task | null
  pick: Pick
  chore: Chore | null
  /** far: the folders reached beyond the plan. */
  far: string[]
  /** Since when it has waited, in milliseconds. */
  since: number
}

/** Live agents waiting for a person in their terminal (a question, an approval). */
export const blockedAgents = (ws: Workspace | null) => (ws?.sessions ?? []).filter(s => !s.ended && s.blockedSince)

export interface Inbox {
  /** Gates: an agent cannot go on. */
  blocking: InboxRow[]
  /** Reviews and gates with open findings, the highest open severity first. */
  decisions: InboxRow[]
  /** Commits that name no node, branches that grew beyond the plan. */
  gaps: InboxRow[]
  /** Chores: they block nobody. */
  later: InboxRow[]
}

/** The open findings of a node, the most severe first. */
export const openFindings = (n: Task) =>
  findingsOf(n).filter(waits).sort((a, b) => SEVERITIES.indexOf(a.severity) - SEVERITIES.indexOf(b.severity))

export function inbox(ws: Workspace): Inbox {
  const box: Inbox = { blocking: [], decisions: [], gaps: [], later: [] }
  for (const intent of ws.intents) for (const unit of intent.units) {
    const row = (kind: InboxRow['kind'], unit: Unit, node: Task, since: number, finding?: string): InboxRow =>
      ({ key: `${kind}:${unit.id}:${node.id}`, kind, intent, unit, node, pick: { kind: 'task', id: node.id, finding }, chore: null, far: [], since })
    const a = attention(unit)
    for (const n of a.gates) box.blocking.push(row('gate', unit, n, ms(n.since)))
    for (const { review: n } of a.asked) box.decisions.push(row('decision', unit, n, ms(n.since), openFindings(n)[0].id))
    for (const n of a.loose) box.gaps.push(row('loose', unit, n, Math.min(...unit.commits.filter(c => n.output.commits?.includes(c.id)).map(c => ms(c.date)))))
    for (const o of a.out) box.gaps.push({ ...row('far', unit, o.root, o.since), far: o.far })
  }
  for (const intent of ws.intents)
    for (const c of intent.chores.filter(c => c.state === 'open'))
      box.later.push({ key: `chore:${c.id}`, kind: 'chore', intent, unit: intent.units.find(u => u.id === c.unit) ?? null, node: null, pick: null, chore: c, far: [], since: ms(c.since) })
  // Chores of no intent: under their project's name, or under nothing.
  for (const c of (ws.chores ?? []).filter(c => c.state === 'open'))
    box.later.push({ key: `chore:${c.id}`, kind: 'chore', intent: null, project: ws.projects?.find(p => p.id === c.project)?.title, unit: null, node: null, pick: null, chore: c, far: [], since: ms(c.since) })
  const oldest = (a: InboxRow, b: InboxRow) => a.since - b.since
  const top = (r: InboxRow) => SEVERITIES.indexOf(openFindings(r.node!)[0].severity)
  // Agents waiting for a person in their terminal come first: nothing moves until someone looks.
  const waiting = blockedAgents(ws).map((s): InboxRow => {
    const intent = ws.intents.find(i => i.id === s.intent) ?? null
    return {
      key: `agent:${s.session ?? s.agent}`, kind: 'agent', intent, unit: null, node: null, pick: null, chore: null, far: [], since: ms(s.blockedSince),
      agent: { name: s.agent, session: s.session, topic: s.topic, place: intent?.title ?? s.group?.label ?? '' },
    }
  }).sort(oldest)
  box.blocking.sort(oldest)
  box.blocking.unshift(...waiting)
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
  const before = at(touched[0]).before, after = at(touched.at(-1)!).after
  const { add, del } = changeOf(before, after)
  return { touched, before, after, status: before === null ? 'A' : after === null ? 'D' : 'M', add, del }
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
    did.set(path, { ...shownDelta(unit, sel, path)!, inPick: !!fileDelta(unit, path, sel.commits) })
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
}

export const NO_EDITS: Edits = { gates: {}, decisions: {}, chores: [] }

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
  return {
    ...ws,
    chores: ws.chores?.map(c => (edits.chores.includes(c.id) ? { ...c, state: 'done' as const } : c)),
    intents: ws.intents.map(i => ({
      ...i,
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
