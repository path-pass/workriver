// What asks something of the user, across all units: what a unit wants a look at, and the
// rows of Needs you.
import type { Chore, Task, Pick } from '@/work/types'
import type { Intent, Unit } from '@/planning/types'
import type { MachineAlert } from '@/machines/types'
import type { ManagerMessage } from '@/conversations/types'
import type { Workspace } from '@/workspace/types'
import { ms } from '@/planning/select'
import { repliesOf } from '@/conversations/select'
import type { Space } from '@/agents/types'
import { activeIn, candidates, movedSeat, unmanaged } from '@/planning/spaces'
import { type Working, working, openCount, openFindings, SEVERITIES } from '@/work/select'

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
  kind: 'agent' | 'stalled' | 'machine' | 'gate' | 'decision' | 'loose' | 'chore' | 'merge' | 'message' | 'discussion' | 'seat'
  /** The intent; null for an agent in a workspace no intent follows, and for a project's chore. */
  intent: Intent | null
  /** A chore of no intent: its project's name, if it has a project. */
  project?: string
  /** agent: the agent waiting in its terminal, and where it runs. */
  agent?: { name: string; session?: string; machine?: string; topic?: string; place: string }
  /** seat: an open workspace with an active intent and no manager running: whom to offer. */
  seat?: { space: Space; proposal?: string; moved: boolean }
  /** machine: a machine past an alert threshold. */
  alert?: MachineAlert
  /** message: a workspace's conversation (by its id) with unread words of its manager: how many, the newest. */
  talk?: { workspace: string; manager: string; count: number; text: string }
  /** discussion: a task's discussion with answers of its agent the user has not read: how many, the newest. */
  discussion?: { agent: string; count: number; text: string }
  /** The unit it belongs to; null for a chore that concerns the intent as a whole. */
  unit: Unit | null
  /** What a click opens; null for a chore. */
  node: Task | null
  pick: Pick
  chore: Chore | null
  /** Since when it has waited, in milliseconds. */
  since: number
  /** message, discussion: every unread word, by its place (times are whole seconds, so two
   *  words can share one; a place is the word's own, whatever the order of the rest). */
  words?: string[]
}

export type WaitingAgentRow = InboxRow & { kind: 'agent'; agent: NonNullable<InboxRow['agent']> }

/** Live agents waiting for a person in their terminal (a question, an approval). */
export const blockedAgents = (ws: Workspace | null) => (ws?.sessions ?? []).filter(s => !s.ended && s.blockedSince)

export interface Inbox {
  /** Gates: an agent cannot go on. */
  blocking: InboxRow[]
  /** Open workspaces with an active intent and no manager running: what is decided there
   *  reaches no one until the user seats one. They leave once a manager runs. */
  seats: InboxRow[]
  /** Machines past an alert threshold: they slow every agent there; they leave by themselves. */
  machines: InboxRow[]
  /** Reviews and gates with open findings, the highest open severity first. */
  decisions: InboxRow[]
  /** Workspaces whose manager wrote what the user has not read, and tasks whose agent answered
   *  in their discussion what the user has not read: one row each, the newest first. */
  messages: InboxRow[]
  /** Commits that name no node. */
  gaps: InboxRow[]
  /** Chores, and ready units of an intent with no manager that may be merged already: they block nobody. */
  later: InboxRow[]
}

export function inbox(ws: Workspace): Inbox {
  const box: Inbox = { blocking: [], seats: [], machines: [], decisions: [], messages: [], gaps: [], later: [] }
  const stalled: InboxRow[] = []
  const discussions: InboxRow[] = []
  for (const intent of ws.intents) for (const unit of intent.units) {
    const row = (kind: InboxRow['kind'], unit: Unit, node: Task, since: number, finding?: string): InboxRow =>
      ({ key: `${kind}:${unit.id}:${node.id}`, kind, intent, unit, node, pick: { kind: 'task', id: node.id, finding }, chore: null, since })
    const a = attention(unit)
    for (const n of a.gates) box.blocking.push(row('gate', unit, n, ms(n.since)))
    // A worker that stopped without delivering, and a reminder did not help (or it is gone).
    for (const n of unit.tasks.filter(n => n.stalled?.escalated)) stalled.push(row('stalled', unit, n, ms(n.stalled!.since)))
    for (const { review: n } of a.asked) box.decisions.push(row('decision', unit, n, ms(n.since), openFindings(n)[0].id))
    // A discussion's new answers: nothing waits on them, so they sit with the manager's words.
    for (const n of unit.tasks) {
      const said = (n.threads ?? []).flatMap((th, i) => th.discuss ? th.turns.map((x, k) => ({ ...x, word: `${i}.${k}` })) : []).filter(x => x.unread && x.answer !== null && x.answeredAt)
      if (!said.length) continue
      const last = said.reduce((a, b) => (ms(b.answeredAt) >= ms(a.answeredAt) ? b : a))
      discussions.push({ ...row('discussion', unit, n, ms(last.answeredAt)), words: said.map(x => x.word), discussion: { agent: last.answerBy ?? n.agent ?? '', count: said.length, text: last.answer!.trim().split('\n')[0].replace(/`/g, '') } })
    }
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
  const waiting = blockedAgents(ws).map((s): WaitingAgentRow => {
    const intent = ws.intents.find(i => i.id === s.intent) ?? null
    return {
      key: `agent:${s.session ?? s.agent}${s.machine ? `:${s.machine}` : ''}`, kind: 'agent', intent, unit: null, node: null, pick: null, chore: null, since: ms(s.blockedSince),
      agent: { name: s.agent, session: s.session, machine: s.machine, topic: s.topic, place: intent?.title ?? ws.workspaces?.find(w => w.id === s.workspace)?.name ?? s.group?.label ?? '' },
    }
  }).sort(oldest)
  box.blocking.sort(oldest)
  box.blocking.unshift(...waiting, ...stalled.sort(oldest))
  // A workspace whose manager does not run, with work active in it: one row each, the oldest
  // intent's start first (nothing dates the seat's end).
  box.seats = unmanaged(ws).map((space): InboxRow => {
    const intent = activeIn(ws.intents, space.id)[0] ?? null
    const c = candidates(ws).find(x => x.key === space.proposal)
    return { key: `seat:${space.id}`, kind: 'seat', intent, unit: null, node: null, pick: null, chore: null, since: NaN, seat: { space, proposal: space.proposal, moved: movedSeat(space, c) } }
  })
  box.machines = (ws.alerts ?? [])
    .map((a): InboxRow => ({ key: `machine:${a.machine}:${a.metric}`, kind: 'machine', alert: a, intent: null, unit: null, node: null, pick: null, chore: null, since: ms(a.since) }))
    .sort(oldest)
  box.decisions.sort((a, b) => top(a) - top(b) || oldest(a, b))
  // The manager's words not read yet (a message it started, replies), by workspace.
  const unread = new Map<string, { m: ManagerMessage; by: string; text: string; at: string; word: string }[]>()
  for (const m of ws.messages ?? []) {
    const said = [
      ...(m.byManager && m.unread ? [{ m, by: m.manager, text: m.text, at: m.at, word: `${m.id}` }] : []),
      ...repliesOf(m).map((r, k) => ({ m, ...r, word: `${m.id}.${k}` })).filter(r => r.unread),
    ]
    if (said.length) unread.set(m.workspace, [...(unread.get(m.workspace) ?? []), ...said])
  }
  box.messages = [...unread].map(([workspace, said]): InboxRow => {
    const last = said.reduce((a, b) => (ms(b.at) >= ms(a.at) ? b : a))
    return {
      key: `message:${workspace}`, kind: 'message', intent: ws.intents.find(i => i.id === last.m.intent) ?? null, unit: null, node: null, pick: null, chore: null, since: ms(last.at), words: said.map(x => x.word),
      talk: { workspace, manager: last.by, count: said.length, text: last.text.trim().split('\n')[0].replace(/`/g, '') },
    }
  }).concat(discussions).sort((a, b) => b.since - a.since)
  box.gaps.sort(oldest)
  box.later.sort(oldest)
  return box
}
