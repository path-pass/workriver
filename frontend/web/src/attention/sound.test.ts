// The sound for Needs you: a new row plays, what was there on load does not, several at once
// play once (in all open tabs), with the sound off nothing plays, and the user's own act
// plays nothing.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { chime, QUIET_MS, rowSound, soundingRows, soundMissed } from './sound'
import { inbox } from './select'
import { NO_EDITS, withEdits } from '@/workspace/edits'
import type { Workspace } from '@/workspace/types'

describe('when Needs you plays', () => {
  // The time of the last sound is shared by the tabs, in localStorage.
  beforeEach(() => localStorage.clear())
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
    // The same keys: an alert rising to critical.
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

describe('what plays', () => {
  beforeEach(() => localStorage.clear())
  const gate = { id: '2', kind: 'gate', gate: 'test', state: 'waiting', title: 'Try it', since: '2026-11-21T10:00:00Z', inputs: [], input: { text: '', causes: [] }, output: {}, plans: [] }
  const base = (extra: object = {}) => ({
    intents: [{ id: 'i1', title: 'H', state: 'active', manager: 'm', chores: [], units: [{ id: 'u1', title: 'One', tasks: [gate], commits: [] }] }],
    sessions: [], alerts: [], messages: [], ...extra,
  }) as unknown as Workspace

  it("plays nothing for the user's own gate result with a problem", () => {
    const step = rowSound(() => 10_000_000)
    const ws = base()
    step(soundingRows(inbox(ws)), true)
    const finding = { severity: 'P2', problem: 'broken', impact: '', evidence: '' }
    const passed = withEdits(ws, { ...NO_EDITS, gates: { 'u1:2': { result: 'problem', note: '', attachments: [], findings: [finding as never], at: '2026-11-22T00:00:00Z' } } })
    expect(inbox(passed).decisions).toHaveLength(1)
    expect(step(soundingRows(inbox(passed)), true)).toBe(false)
  })

  it("plays for a manager's new word while an older one is unread, not for reading them", () => {
    let t = 10_000_000
    const step = rowSound(() => t)
    const said = (replies: { at: string; unread: boolean }[]) =>
      base({ messages: [{ id: 'm1', workspace: 'w1', manager: 'mg', text: 'hi', at: '2026-11-22T00:00:00Z', byManager: false, unread: false, replies: replies.map(r => ({ by: 'mg', text: 'a', ...r })) }] })
    step(soundingRows(inbox(said([]))), true)
    expect(step(soundingRows(inbox(said([{ at: '2026-11-22T00:01:00Z', unread: true }]))), true)).toBe(true)
    t += 60_000
    expect(step(soundingRows(inbox(said([{ at: '2026-11-22T00:01:00Z', unread: true }, { at: '2026-11-22T00:05:00Z', unread: true }]))), true)).toBe(true)
    t += 60_000
    expect(step(soundingRows(inbox(said([{ at: '2026-11-22T00:01:00Z', unread: false }, { at: '2026-11-22T00:05:00Z', unread: false }]))), true)).toBe(false)
  })

  it("plays for a discussion's new answer while an older one is unread", () => {
    let t = 10_000_000
    const step = rowSound(() => t)
    const turn = (n: number) => ({ text: `q${n}`, at: `2026-11-22T00:0${n}:00Z`, answer: `a${n}`, answerBy: 'impl', answeredAt: `2026-11-22T00:0${n}:30Z`, unread: true })
    const talked = (n: number) => {
      const design = { id: '3', kind: 'design', state: 'done', title: 'D', agent: 'impl', inputs: [], input: { text: '', causes: [] }, output: {}, plans: [], threads: [{ discuss: true, turns: Array.from({ length: n }, (_, k) => turn(k + 1)) }] }
      return base({ intents: [{ id: 'i1', title: 'H', state: 'active', manager: 'm', chores: [], units: [{ id: 'u1', title: 'One', tasks: [design], commits: [] }] }] })
    }
    step(soundingRows(inbox(talked(1))), true)
    t += 60_000
    expect(inbox(talked(2)).messages).toHaveLength(1)
    expect(step(soundingRows(inbox(talked(2))), true)).toBe(true)
  })

  it('plays once for the same row in two open tabs', () => {
    let t = 10_000_000
    const one = rowSound(() => t), two = rowSound(() => t)
    one(['gate:u1:2'], true); two(['gate:u1:2'], true)
    expect(one(['gate:u1:2', 'agent:s1'], true)).toBe(true)
    t += 300
    expect(two(['gate:u1:2', 'agent:s1'], true)).toBe(false)
    t += QUIET_MS
    expect(two(['gate:u1:2', 'agent:s1', 'agent:s2'], true)).toBe(true)
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
