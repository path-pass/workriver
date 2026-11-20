// A task's past conversations and how far one has got; what the user picked as the chips of
// a message to the manager, and the message they make.
import { baseName } from '@/code/files'
import type { Attachment } from '@/attachments/types'
import type { Lines } from '@/code/types'
import type { Unit } from '@/planning/types'
import type { Task } from '@/work/types'
import type { AskContext, ManagerMessage, Thread } from './types'

/** The conversation with every manager, as the service names it: under no workspace's label. */
export const EVERY = ''

/** A message's replies: its manager's, or (to every manager) each copy's. */
export const repliesOf = (m: ManagerMessage) => [...m.replies, ...(m.to ?? []).flatMap(c => c.replies)]

/** A task's conversations: from the service, or the sample's single one. */
export function threadsOf(n: Task): Thread[] {
  return n.threads ?? (n.thread?.length ? [{ id: '', ended: false, live: false, turns: n.thread }] : [])
}

export const turnsOf = (n: Task) => threadsOf(n).flatMap(t => t.turns)

/** How far a conversation has got, from its last exchange. */
export function stateOf(th: Thread): 'ended' | 'failed' | 'waiting' | 'answered' {
  const last = th.turns.at(-1)
  return th.ended ? 'ended' : last?.failed ? 'failed' : last && last.answer === null ? 'waiting' : 'answered'
}

/** A chip above the Manager box. `own`: the unit's chip (with its id) or the picked task's. */
export type Chip = AskContext & { own?: 'unit' | 'task'; unit?: string }

/** What is picked on screen: the unit, its task (with the label its chip shows), the finding,
 *  the file or lines in the code, and more (other tasks that changed the lines, the open attachment). */
export interface Picked {
  unit: Unit | null
  n: Task | null
  taskLabel: string
  finding?: string
  file: string | null
  lines: Lines | null
  extra: AskContext[]
}

/** The selection as chips: the unit, the task, the finding, the lines (or else the file),
 *  the rest. Only for a message to the unit's own workspace (`here`); none for another. */
export function chipsOf(p: Picked, here: boolean): Chip[] {
  if (!here || !p.unit) return []
  const { lines, file } = p
  return [
    { kind: 'unit', label: p.unit.title, own: 'unit', unit: p.unit.id },
    // Its number: the one shown (#n), else (a review) its id, which is the number with the service.
    ...(p.n ? [{ kind: 'task' as const, label: p.taskLabel, own: 'task' as const, task: Number(p.n.taskId ?? p.n.id) }] : []),
    ...(p.n && p.finding ? [{ kind: 'finding' as const, label: p.finding }] : []),
    // Picked lines say more than their file, so they take the file's place.
    ...(lines
      ? [{ kind: 'lines' as const, label: `${baseName(lines.path)} L${lines.from}${lines.to > lines.from ? `–${lines.to}` : ''}`, path: lines.path, from: lines.from, to: lines.to }]
      : file ? [{ kind: 'file' as const, label: baseName(file), path: file }] : []),
    ...(p.n ? p.extra : p.extra.filter(c => c.kind !== 'task')),
  ]
}

export const chipKey = (c: AskContext) => `${c.kind}:${c.label}`

/** The chips left after those taken out (by key). The task's × takes with it what needs the
 *  task: its finding and the attachment chipped because it shows it; the file or lines and
 *  the other tasks that wrote them stay with the unit. The unit's × takes everything (the
 *  user's own files are not chips: they stay). */
export function leaveOut(chips: Chip[], out: string[]): Chip[] {
  const gone = (own: Chip['own']) => chips.some(c => c.own === own && out.includes(chipKey(c)))
  if (gone('unit')) return []
  const needsTask = (c: Chip) => c.own === 'task' || c.kind === 'finding' || c.kind === 'attachment'
  return chips.filter(c => !out.includes(chipKey(c)) && !(gone('task') && needsTask(c)))
}

/** `/api/message`'s unit, task and context: from the chips, then the user's stored files. */
export function messageOf(chips: Chip[], files: Pick<Attachment, 'id' | 'name'>[]): { unit: string; task: number; context: AskContext[] } {
  return {
    unit: chips.find(c => c.own === 'unit')?.unit ?? '',
    // (A number the sample's ids do not make is none.)
    task: chips.find(c => c.own === 'task')?.task || 0,
    context: [
      ...chips.filter(c => !c.own).map(({ own: _o, unit: _u, ...c }) => c),
      ...files.map(a => ({ kind: 'attachment' as const, label: a.name, id: a.id })),
    ],
  }
}
