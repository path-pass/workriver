// A task's past conversations and how far one has got; what the user picked as the chips of
// a message to the manager, and the message they make; which workspace's conversation the
// Manager section shows, and where it goes on a project switch.
import { baseName } from '@/code/files'
import type { Attachment } from '@/attachments/types'
import type { Lines } from '@/code/types'
import type { Task } from '@/work/types'
import type { Session } from '@/agents/types'
import type { Intent, Unit } from '@/planning/types'
import type { AskContext, ManagerMessage, Talk, Thread } from './types'

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

/** What the Manager section needs to know which workspaces it offers and which it shows. */
export type Shown = Pick<Talk, 'intents' | 'intent' | 'sessions' | 'messages' | 'to' | 'followedAt' | 'project' | 'projectOf'>

/** The workspaces the Manager section offers and the conversation it shows. */
export interface Places {
  /** The intent on screen: its page, or the open unit's. */
  intent?: Intent
  /** Its workspace, while it is active. */
  own?: string
  /** The managers running, each in a workspace. */
  running: Session[]
  /** Every workspace with a manager running or with messages: the one on screen first, then
   *  those of the rail's project, then the rest. */
  labels: string[]
  /** "All managers" is offered: more than one running, or once written to or picked. */
  every: boolean
  /** The conversation shown: the one picked while it is offered (one followed to on a switch
   *  only while the intent then on screen is), else the one on screen, else the first;
   *  `EVERY`: every manager; none: no workspace has a manager. */
  label: string | null
}

/** The workspaces offered and the one shown; `unit`: the open unit, when no intent is named. */
export function placesOf(talk: Shown, unit?: string): Places {
  const intent = talk.intents.find(i => i.id === talk.intent) ?? (unit ? talk.intents.find(i => i.units.some(u => u.id === unit)) : undefined)
  const own = intent?.state === 'active' ? intent.workspace?.label : undefined
  const running = talk.sessions.filter(s => s.manager && !s.ended && s.group)
  const all = [...new Set([...running.map(s => s.group!.label), ...talk.messages.map(m => m.workspace)].filter(l => !!l && l !== own))]
  const ours = (l: string) => !!talk.project && talk.projectOf(l) === talk.project
  const labels = [...(own ? [own] : []), ...all.filter(ours), ...all.filter(l => !ours(l))]
  const held = talk.followedAt === undefined || talk.followedAt === (intent?.id ?? '')
  const chosen = held ? talk.to ?? null : null
  const every = running.length > 1 || chosen === EVERY || talk.messages.some(m => m.workspace === EVERY)
  const label = (chosen !== null && (labels.includes(chosen) || (chosen === EVERY && every)) ? chosen : null) ?? own ?? labels[0] ?? null
  return { intent, own, running, labels, every, label }
}

/**
 * Where the Manager section goes when the rail switches to `next.project` (`next.intent`: the
 * intent then on screen). A workspace of another project gives way to the project's manager:
 * its only one; with several, the one last picked there (`remembered`), else one with replies
 * not read, else the first running. Only a workspace whose manager runs is gone to, besides
 * the opened unit's own. All managers, a workspace already of the project, one of no known
 * project, and a project with no manager stay as they are.
 * Returns the workspace to show; null: the one on screen; undefined: no change.
 */
export function followProject(talk: Shown, next: { project: string; intent?: string }, remembered: string | null): string | null | undefined {
  const shown = placesOf(talk).label
  const from = shown ? talk.projectOf(shown) : undefined
  if (!from || from === next.project) return undefined
  const after = placesOf({ ...talk, ...next })
  const live = (l: string) => after.running.some(s => s.group!.label === l)
  const ours = after.labels.filter(l => talk.projectOf(l) === next.project && (l === after.own || live(l)))
  const unread = (l: string) => talk.messages.some(m => m.workspace === l && ((m.byManager && m.unread) || repliesOf(m).some(r => r.unread)))
  const to = ours.length > 1
    ? (remembered && ours.includes(remembered) ? remembered : ours.find(unread) ?? ours.find(live) ?? ours[0])
    : ours[0]
  return to === undefined ? undefined : to === after.own ? null : to
}
