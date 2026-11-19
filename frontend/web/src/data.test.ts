// Quiet: a unit or intent where nothing moved for three days, worked out in the page.
import { describe, expect, it } from 'vitest'
import { intentQuietSince, lastMove, QUIET, quietSince } from './data'
import type { Intent, Unit } from './types'

const day = 864e5
const now = Date.parse('2026-11-20T12:00:00Z')
const ago = (days: number) => new Date(now - days * day).toISOString()
type T = { state?: string; kind?: string; started?: string; delivered?: string; since?: string; stalled?: { since: string }; decided?: string }
const unit = (tasks: T[], more: Record<string, unknown> = {}): Unit =>
  ({
    id: 'u1',
    commits: [],
    tasks: tasks.map(t => ({ kind: 'implement', state: 'done', ...t, output: { findings: t.decided ? [{ decided: t.decided }] : [] } })),
    ...more,
  }) as unknown as Unit

describe('quiet', () => {
  it('is three days', () => expect(QUIET).toBe(3 * day))

  it('a unit where nothing moved for more than three days is quiet since its last move', () => {
    const u = unit([{ started: ago(9), delivered: ago(4) }])
    expect(quietSince(u, now)).toBe(now - 4 * day)
    expect(quietSince(unit([{ delivered: ago(2.9) }]), now)).toBeNull()
  })

  it('every kind of move counts: a decision, a gate opening, a commit, a merge', () => {
    expect(lastMove(unit([{ delivered: ago(9), decided: ago(1) }]))).toBe(now - day)
    expect(lastMove(unit([{ kind: 'gate', state: 'waiting', since: ago(2) }]))).toBe(now - 2 * day)
    expect(lastMove(unit([], { commits: [{ date: ago(5) }, { date: ago(0.5), live: true }] }))).toBe(now - 5 * day)
    expect(lastMove(unit([], { merged: ago(6) }))).toBe(now - 6 * day)
    expect(lastMove(unit([]))).toBeNaN()
  })

  it('never quiet: merged or done, work running with its agent at it, never moved', () => {
    expect(quietSince(unit([{ delivered: ago(9) }], { progress: 'merged' }), now)).toBeNull()
    expect(quietSince(unit([{ delivered: ago(9) }], { progress: 'done' }), now)).toBeNull()
    expect(quietSince(unit([{ state: 'running', started: ago(9) }]), now)).toBeNull()
    expect(quietSince(unit([{ state: 'todo' }]), now)).toBeNull()
    // A stalled worker does not keep its unit from being quiet.
    expect(quietSince(unit([{ state: 'running', started: ago(9), stalled: { since: ago(5) } }]), now)).toBe(now - 5 * day)
  })

  it('an active intent is quiet since the latest move in any of its units', () => {
    const intent = (units: Unit[], state?: string) => ({ id: 'i1', state, units }) as unknown as Intent
    const old = unit([{ delivered: ago(8) }]), newer = unit([{ delivered: ago(4) }]), never = unit([])
    expect(intentQuietSince(intent([old, newer, never]), now)).toBe(now - 4 * day)
    expect(intentQuietSince(intent([old, unit([{ delivered: ago(1) }])]), now)).toBeNull()
    expect(intentQuietSince(intent([old, unit([{ state: 'running', started: ago(9) }])]), now)).toBeNull()
    expect(intentQuietSince(intent([old], 'planned'), now)).toBeNull()
    expect(intentQuietSince(intent([never]), now)).toBeNull()
  })
})
