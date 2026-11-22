// Where every agent is, worked out over the whole workspace (agents cross units).
import type { Task } from '@/work/types'
import type { Intent, Unit } from '@/planning/types'
import type { Session } from './types'
import type { Workspace } from '@/workspace/types'
import { unitsOf, ms } from '@/planning/select'

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

/** Agents of one workspace (by the records), or of none (`key` empty). */
export interface AgentGroup {
  key: string
  /** The workspace's name; empty for those in no workspace. */
  label: string
  intent: Intent | null
  agents: AgentAt[]
}

/** Where every live agent is, worked out over the whole workspace (agents cross units). */
export function agents(ws: Workspace): AgentAt[] {
  return agentGroups(ws).flatMap(g => g.agents)
}

/**
 * Live agents by workspace: groups with someone working first, then by name;
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
    const space = ws.workspaces?.find(w => w.id === s.workspace)
    const key = space?.id ?? ''
    if (!groups.has(key)) groups.set(key, { key, label: space?.name ?? '', intent, agents: [] })
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

/** A workspace (`key`: its id): its manager, and the workers (with their question agents)
 *  under it. `key` empty: the agents with tasks that are in no one workspace. */
export interface TreeGroup {
  key: string
  /** The workspace's name. */
  label: string
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
  // By the workspace the records say (its seat, else its tasks'), never by where the runtime
  // shows its terminal; with none, under "In no workspace" (`key` empty).
  const group = (id: string | undefined) => {
    const space = ws.workspaces?.find(w => w.id === id)
    const key = space?.id ?? ''
    if (!groups.has(key)) groups.set(key, { key, label: space?.name ?? '', intent: ws.intents.find(i => i.id === space?.current) ?? null, manager: null, workers: [] })
    return groups.get(key)!
  }
  const groupOf = (s: Session) => group(s.workspace)
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
      group(i.workspace).workers.push(agent(s, held))
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
  // Those in no workspace come last.
  list.sort((a, b) => Number(!a.key) - Number(!b.key) || busy(a) - busy(b) || a.label.localeCompare(b.label) || a.key.localeCompare(b.key))
  return { groups: list, own: own.sort((a, b) => a.s.agent.localeCompare(b.s.agent)) }
}
