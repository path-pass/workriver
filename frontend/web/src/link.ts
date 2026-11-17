// Each unit, intent and task has its own address: `/u1`, `/u1/3` (task 3 picked, its detail
// open), `/i3` (the intent), `/i3/1` (a task of the intent's own work), and optionally
// `?finding=P2-1`. The fragment is not used: it carries the sign-in code (`#k=`).
import { unitsOf, type Pick } from './data'
import type { Intent, Workspace } from './types'

/** What an address names; null: the page's root. */
export interface Place { id: string; task?: string; finding?: string }

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
 * What a place opens: a unit (an intent opens its first, which is its own work when it has
 * some) with a task picked; a planned intent (its steps to start); or nothing it names.
 */
export function resolve(ws: Workspace, p: Place): { unit: string; pick: Pick } | { planned: Intent } | null {
  const intent = ws.intents.find(i => i.id === p.id)
  const unit = unitsOf(ws).find(u => u.id === p.id) ?? intent?.units[0]
  if (!unit) return intent?.state === 'planned' && !p.task ? { planned: intent } : null
  if (!p.task) return { unit: unit.id, pick: null }
  // A task of an intent is in its own work, whose id is the intent's.
  if (unit.id !== p.id || !unit.tasks.some(n => n.id === p.task)) return null
  return { unit: unit.id, pick: { kind: 'task', id: p.task, finding: p.finding } }
}
