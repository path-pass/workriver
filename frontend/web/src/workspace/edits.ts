// What the user put in during this sitting, laid over the workspace the service sent.
import type { Finding, Task, GateResult, Decision } from '@/work/types'
import type { Intent, Unit } from '@/planning/types'
import type { Workspace } from './types'
import type { Asked, Turn } from '@/conversations/types'
import { ms } from '@/planning/select'

/** Everything the user has put in during this sitting; the data shown is the data with these on top. */
export interface Edits {
  /** By `unit:task`. */
  gates: Record<string, GateResult>
  /** By `unit:task:finding`. */
  decisions: Record<string, { decision: Decision; note: string }>
  chores: string[]
  /** Planned intents merged into another (as the service records it: dropped, with this reason). */
  merges: { into: string; from: string[]; title?: string }[]
  /** Planned intents started in a workspace (its id), by intent. */
  starts: Record<string, string>
  /** Up to when the user has read each workspace's conversation with its manager, by its id. */
  read: Record<string, string>
  /** The sample's questions in a task's discussion (the service's come with the workspace), by `unit:task`. */
  discussed: Record<string, Turn[]>
  /** Up to when the user has read each task's discussion, by `unit:task`. */
  discussRead: Record<string, string>
  /** A manager's questions the user answered, by `message:reply` (0: the message): the value picked. */
  answers: Record<string, string[]>
}

export const NO_EDITS: Edits = { gates: {}, decisions: {}, chores: [], merges: [], starts: {}, read: {}, discussed: {}, discussRead: {}, answers: {} }

const STATE = { accept: 'accepted', revise: 'revised', reject: 'rejected', defer: 'deferred' } as const

/** The workspace as the user's inputs leave it. Nothing in `ws` is changed. */
export function withEdits(ws: Workspace, edits: Edits): Workspace {
  const task = (unit: Unit, n: Task): Task => {
    // Only while the service still has it waiting: once it recorded a result (this one, or
    // another tab's first), the record shows, never this sitting's guess.
    const g = n.state === 'waiting' ? edits.gates[`${unit.id}:${n.id}`] : undefined
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
      // The form's pictures go after what the gate has, as the service adds them; once the
      // service has the result, its list holds both already.
      const attachments = n.state === 'waiting' ? [...(n.output.attachments ?? []), ...g.attachments] : n.output.attachments
      out = { ...n, state: 'done', since: g.at, output: { ...n.output, note: g.note, attachments, findings, ...(g.selected && { selected: g.selected }) } }
    }
    // The discussion: the sample's own questions after the service's, and what was read.
    const key = `${unit.id}:${n.id}`
    const mine = edits.discussed[key]
    const read = edits.discussRead[key]
    if (mine?.length || read) {
      const threads = out.threads ?? []
      const had = threads.some(th => th.discuss)
      const all = had ? threads.map(th => (th.discuss ? { ...th, turns: [...th.turns, ...(mine ?? [])] } : th)) : mine?.length ? [...threads, { id: '', ended: false, discuss: true, turns: mine }] : threads
      out = { ...out, threads: all.map(th => (th.discuss && read ? { ...th, turns: th.turns.map(x => (x.unread && x.answeredAt && ms(x.answeredAt) <= ms(read) ? { ...x, unread: false } : x)) } : th)) }
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
  // Answered here: no longer open, the pick shown (the service's record says by which message).
  const answered = (key: string, q: Asked) => (edits.answers[key] && !q.answered ? { answered: { by: '', selected: edits.answers[key] }, open: false } : {})
  // Read: the manager's words up to then are no longer new.
  const fresh = (workspace: string, at: string) => !(ms(at) <= ms(edits.read[workspace]))
  // Started here: active in that workspace, with its manager (one of no project takes the workspace's).
  const started = (space: string) => {
    const w = ws.workspaces?.find(x => x.id === space)
    return { state: 'active' as const, workspace: space, manager: w?.manager?.name, managerState: w?.manager?.state, ...(w && { project: w.project }) }
  }
  return {
    ...ws,
    messages: ws.messages?.map(m => ({
      ...m, ...answered(`${m.id}:0`, m), unread: m.unread && fresh(m.workspace, m.at),
      replies: m.replies.map((r, k) => ({ ...r, ...answered(`${m.id}:${k + 1}`, r), unread: r.unread && fresh(m.workspace, r.at) })),
    })),
    chores: ws.chores?.map(c => (edits.chores.includes(c.id) ? { ...c, state: 'done' as const } : c)),
    intents: ws.intents.map(i => ({
      ...i,
      ...(mergedInto(i) && { state: 'dropped' as const, reason: `merged into ${mergedInto(i)}` }),
      ...(i.state === 'planned' && edits.starts[i.id] && started(edits.starts[i.id])),
      note: noteOf(i),
      title: titleOf(i),
      units: i.units.map(u => ({ ...u, tasks: u.tasks.map(n => task(u, n)) })),
      chores: i.chores.map(c => (edits.chores.includes(c.id) ? { ...c, state: 'done' as const } : c)),
    })),
  }
}

/**
 * The user answers a manager's question (`key`: `message:reply`): shown answered at once, and
 * taken back if the service does not take it, so a question still open can be answered again.
 * `send` records it; its failure is passed on.
 */
export async function answerOptimistically(key: string, selected: string[], send: () => Promise<void>, set: (change: (e: Edits) => Edits) => void) {
  set(e => ({ ...e, answers: { ...e.answers, [key]: selected } }))
  try {
    await send()
  } catch (err) {
    set(e => { const { [key]: _gone, ...rest } = e.answers; return { ...e, answers: rest } })
    throw err
  }
}
