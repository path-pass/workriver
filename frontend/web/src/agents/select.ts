// Where every agent is, worked out over the whole workspace (agents cross units).
import type { Task } from '@/work/types'
import type { Intent, Unit } from '@/planning/types'
import type { AgentRef, Session } from './types'
import type { Workspace } from '@/workspace/types'
import { unitsOf, ms } from '@/planning/select'

/** The ref of a session: its session and machine together. */
export const refOf = (s: Session): AgentRef => ({ name: s.agent, session: s.session, machine: s.machine })

/** Whether a session is the one a ref names: its session (or, with none, its name) on its
 *  machine; an absent machine is this one and matches only an absent machine. */
export const isRef = (r: AgentRef, x: Session) => (r.session ? x.session === r.session : x.agent === r.name) && (x.machine ?? '') === (r.machine ?? '')

/** The live session a ref names. */
export const liveSession = (sessions: Session[], r: AgentRef) => sessions.find(x => !x.ended && isRef(r, x))

/**
 * The live session a task's agent runs in: its session (or, with none, its name), in the
 * task's workspace when it is known (a `Space.id`: the records put an agent there, by its
 * seat or its tasks), and, when the task's unit has a repository, on that repository's machine
 * (absent: this one). A unit with none (research, an intent's own work) names no machine, so
 * its agent may be on any. Only when exactly one fits: the same session may show on two
 * machines and a name in several workspaces, and keys must never go to another instance.
 */
export function taskSession(sessions: Session[], n: { agent?: string | null; agentSession?: string | null }, unit: { repo: string; machine?: string }, workspace?: string) {
  if (!n.agent) return undefined
  const fit = sessions.filter(x => !x.ended
    && (n.agentSession && x.session ? x.session === n.agentSession : x.agent === n.agent)
    && (!unit.repo || (x.machine ?? '') === (unit.machine ?? ''))
    && (!workspace || x.workspace === workspace))
  return fit.length === 1 ? fit[0] : undefined
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
    // One waiting on its screen (the runtime missed the dialog) is shown exactly as blocked.
    const state = s.ended ? 'gone' : s.state === 'blocked' || s.onScreen ? 'blocked' : s.state === 'working' ? 'working' : 'idle'
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
