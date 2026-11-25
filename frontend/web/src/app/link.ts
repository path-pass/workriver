// Units, intents, tasks, and manager replies have their own addresses. The fragment is not
// used: it carries the sign-in code (`#k=`).
import { noCode, unitsOf } from '@/planning/select'
import type { Pick } from '@/work/types'
import type { Workspace } from '@/workspace/types'

/** What an address names; null: the page's root. */
export interface Place { id: string; task?: string; finding?: string }

export interface ReplyPlace { message: string; reply: number }

export function replyPlaceOf(path: string): ReplyPlace | null {
  const m = /^\/(m\d+)\/(\d+)\/?$/.exec(path)
  return m ? { message: m[1], reply: Number(m[2]) } : null
}

export const replyPath = ({ message, reply }: ReplyPlace) => `/${encodeURIComponent(message)}/${reply}`

export function placeOf(path: string, search: string): Place | null {
  if (path === '/' || path === '') return null
  const m = /^\/([^/]+)(?:\/([^/]+))?\/?$/.exec(path)
  const finding = new URLSearchParams(search).get('finding') ?? undefined
  // Anything else is still a place, one that names nothing.
  if (!m) return { id: path.slice(1) }
  return { id: decodeURIComponent(m[1]), task: m[2] && decodeURIComponent(m[2]), finding }
}

/** The address of a unit and what is picked in it. */
export function pathOf(unit: string, pick: Pick): string {
  const task = pick ? `/${encodeURIComponent(pick.id)}` : ''
  const finding = pick?.finding ? `?finding=${encodeURIComponent(pick.finding)}` : ''
  return `/${encodeURIComponent(unit)}${task}${finding}`
}

/** A full link to `path` on this page's own address. */
export const linkTo = (path: string) => location.origin + path

/**
 * What a place opens: a unit, or an intent's page (`unit` is then the intent's id), with a
 * task of the unit picked; or nothing it names.
 */
export function resolve(ws: Workspace, p: Place): { unit: string; pick: Pick } | null {
  const intent = ws.intents.find(i => i.id === p.id)
  const unit = intent ? undefined : unitsOf(ws).find(u => u.id === p.id)
  if (!intent && !unit) return null
  if (!p.task) return { unit: p.id, pick: null }
  if (!unit) return null
  const pendingCommit = ws.gitPending && !noCode(unit) && p.task.startsWith('commit:')
  if (!pendingCommit && !unit.tasks.some(n => n.id === p.task)) return null
  return { unit: p.id, pick: { kind: 'task', id: p.task, finding: p.finding } }
}
