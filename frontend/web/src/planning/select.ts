// Units and intents as the views find them, and whether they have gone quiet.
import type { Intent, Unit } from './types'
import type { Workspace } from '@/workspace/types'
import { QUIET } from '@/config/defaults'
import { findingsOf } from '@/work/select'

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
