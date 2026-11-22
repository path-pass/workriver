// The sound for Needs you: a new row plays, what was there on load does not, several at once
// play once, and with the sound off nothing plays.
import { afterEach, describe, expect, it, vi } from 'vitest'
import { chime, QUIET_MS, rowSound, soundingRows, soundMissed } from './sound'
import { inbox } from './select'
import type { Workspace } from '@/workspace/types'

describe('when Needs you plays', () => {
  const clock = () => { let t = 1_000_000; const now = () => t; return { now, pass: (ms: number) => { t += ms } } }

  it('plays for a new row, not for the rows there when the page loaded', () => {
    const { now, pass } = clock()
    const step = rowSound(now)
    expect(step(['gate:u1:2', 'agent:s1'], true)).toBe(false)
    pass(60_000)
    expect(step(['gate:u1:2', 'agent:s1'], true)).toBe(false)
    expect(step(['gate:u1:2', 'agent:s1', 'message:w1'], true)).toBe(true)
  })

  it('does not play for a row that only changed or left', () => {
    const step = rowSound(clock().now)
    step(['machine:here:swap', 'message:w1'], true)
    // The same keys: an alert rising to critical, a second unread reply.
    expect(step(['machine:here:swap', 'message:w1'], true)).toBe(false)
    expect(step(['message:w1'], true)).toBe(false)
  })

  it('plays once for several rows at once, and once for a burst over a few seconds', () => {
    const { now, pass } = clock()
    const step = rowSound(now)
    step([], true)
    expect(step(['gate:u1:2', 'gate:u1:3', 'agent:s1'], true)).toBe(true)
    pass(1000)
    expect(step(['gate:u1:2', 'gate:u1:3', 'agent:s1', 'agent:s2'], true)).toBe(false)
    pass(QUIET_MS)
    expect(step(['gate:u1:2', 'gate:u1:3', 'agent:s1', 'agent:s2', 'agent:s3'], true)).toBe(true)
  })

  it('plays nothing while off, and not later for rows that came then', () => {
    const step = rowSound(clock().now)
    step([], false)
    expect(step(['gate:u1:2'], false)).toBe(false)
    expect(step(['gate:u1:2'], true)).toBe(false)
    expect(step(['gate:u1:2', 'gate:u1:3'], true)).toBe(true)
  })

  it('counts what waits on the user, not gaps or chores', () => {
    const gate = { id: '2', kind: 'gate', gate: 'test', state: 'waiting', title: 'Try it', since: '2026-11-21T10:00:00Z', inputs: [], input: { text: '', causes: [] }, output: {}, plans: [] }
    const loose = { id: '3', kind: 'implement', state: 'unregistered', title: '', inputs: [], input: { text: '', causes: [] }, output: { commits: ['c1'] }, plans: [] }
    const ws = {
      intents: [{
        id: 'i1', title: 'Hardening', state: 'active', manager: 'm', chores: [{ id: 'c1', state: 'open', title: 'Log in', since: '2026-11-21T10:00:00Z' }],
        units: [{ id: 'u1', title: 'One', tasks: [gate, loose], commits: [{ id: 'c1', date: '2026-11-21T10:00:00Z' }] }],
      }],
      sessions: [{ agent: 'impl', session: 's1', blockedSince: '2026-11-21T10:00:00Z' }],
      alerts: [{ machine: 'here', metric: 'swap', level: 'warn', since: '2026-11-21T10:00:00Z' }],
      messages: [],
    } as unknown as Workspace
    const box = inbox(ws)
    expect(box.gaps).toHaveLength(1)
    expect(box.later).toHaveLength(1)
    expect(soundingRows(box).sort()).toEqual(['agent:s1', 'gate:u1:2', 'machine:here:swap'])
  })
})

describe('the sound', () => {
  afterEach(() => vi.unstubAllGlobals())

  // A stand-in for the browser's AudioContext: records the notes started.
  const fake = (state: 'running' | 'suspended', resumes: boolean) => {
    const started: number[] = []
    const param = { value: 0, setValueAtTime() {}, linearRampToValueAtTime() {}, exponentialRampToValueAtTime() {} }
    class Ctx {
      state = state
      currentTime = 0
      destination = {}
      resume() { if (resumes) this.state = 'running'; return resumes ? Promise.resolve() : new Promise<void>(() => {}) }
      createGain() { return { gain: { ...param }, connect: (d: unknown) => d } }
      createOscillator() { const o = { type: '', frequency: { value: 0 }, connect: (g: unknown) => g, start: () => started.push(o.frequency.value), stop() {} }; return o }
    }
    vi.stubGlobal('AudioContext', Ctx)
    return started
  }

  it('plays two notes, or says it could not when the browser holds it back', async () => {
    vi.resetModules()
    const held = fake('suspended', false)
    const blocked = await import('./sound')
    expect(await blocked.chime()).toBe(false)
    expect(blocked.soundMissed()).toBe(true)
    expect(held).toEqual([])

    vi.resetModules()
    const notes = fake('suspended', true)
    const allowed = await import('./sound')
    expect(await allowed.chime()).toBe(true)
    expect(allowed.soundMissed()).toBe(false)
    expect(notes).toEqual([660, 880])
  })

  it('without Web Audio, says it could not', async () => {
    vi.stubGlobal('AudioContext', undefined)
    expect(await chime()).toBe(false)
    expect(soundMissed()).toBe(true)
  })
})
