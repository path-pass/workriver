// The sound for Needs you: a new row plays, what was there on load does not, several at once
// play once (in all open tabs), with the sound off nothing plays, and the user's own act
// plays nothing.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { chime, claim, QUIET_MS, rowSound, soundingRows, soundMissed, type Sounding } from './sound'
import { inbox } from './select'
import { NO_EDITS, withEdits } from '@/workspace/edits'
import type { Workspace } from '@/workspace/types'

// Rows by key, each coming once.
const rows = (...keys: string[]): Sounding[] => keys.map(key => ({ key, once: `${key}@1` }))
// One tab: whether a list of rows plays there, as it would be decided right before the tones.
const tab = (now: () => number) => {
  const step = rowSound()
  return (rs: Sounding[], wanted: boolean) => { const once = step(rs, wanted); return once.length > 0 && claim(once, now()) }
}

describe('when Needs you plays', () => {
  // What was played is shared by the tabs, in localStorage.
  beforeEach(() => localStorage.clear())
  const clock = () => { let t = 1_000_000; const now = () => t; return { now, pass: (ms: number) => { t += ms } } }

  it('plays for a new row, not for the rows there when the page loaded', () => {
    const { now, pass } = clock()
    const play = tab(now)
    expect(play(rows('gate:u1:2', 'agent:s1'), true)).toBe(false)
    pass(60_000)
    expect(play(rows('gate:u1:2', 'agent:s1'), true)).toBe(false)
    expect(play(rows('gate:u1:2', 'agent:s1', 'message:w1'), true)).toBe(true)
  })

  it('does not play for a row that only changed or left', () => {
    const play = tab(clock().now)
    play(rows('machine:here:swap', 'message:w1'), true)
    // The same keys: an alert rising to critical.
    expect(play(rows('machine:here:swap', 'message:w1'), true)).toBe(false)
    expect(play(rows('message:w1'), true)).toBe(false)
  })

  it('plays once for several rows at once, and once for a burst over a few seconds', () => {
    const { now, pass } = clock()
    const play = tab(now)
    play([], true)
    expect(play(rows('gate:u1:2', 'gate:u1:3', 'agent:s1'), true)).toBe(true)
    pass(1000)
    expect(play(rows('gate:u1:2', 'gate:u1:3', 'agent:s1', 'agent:s2'), true)).toBe(false)
    pass(QUIET_MS)
    expect(play(rows('gate:u1:2', 'gate:u1:3', 'agent:s1', 'agent:s2', 'agent:s3'), true)).toBe(true)
  })

  it('plays nothing while off, and not later for rows that came then', () => {
    const play = tab(clock().now)
    play([], false)
    expect(play(rows('gate:u1:2'), false)).toBe(false)
    expect(play(rows('gate:u1:2'), true)).toBe(false)
    expect(play(rows('gate:u1:2', 'gate:u1:3'), true)).toBe(true)
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
    expect(soundingRows(box).map(r => r.key).sort()).toEqual(['agent:s1', 'gate:u1:2', 'machine:here:swap'])
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
    const play = tab(() => 10_000_000)
    const ws = base()
    play(soundingRows(inbox(ws)), true)
    const finding = { severity: 'P2', problem: 'broken', impact: '', evidence: '' }
    const passed = withEdits(ws, { ...NO_EDITS, gates: { 'u1:2': { result: 'problem', note: '', attachments: [], findings: [finding as never], at: '2026-11-22T00:00:00Z' } } })
    expect(inbox(passed).decisions).toHaveLength(1)
    expect(play(soundingRows(inbox(passed)), true)).toBe(false)
  })

  const said = (replies: { at: string; unread: boolean }[]) =>
    base({ messages: [{ id: 'm1', workspace: 'w1', manager: 'mg', text: 'hi', at: '2026-11-22T00:00:00Z', byManager: false, unread: false, replies: replies.map(r => ({ by: 'mg', text: 'a', ...r })) }] })

  it("plays for a manager's new word while an older one is unread, not for reading them", () => {
    let t = 10_000_000
    const play = tab(() => t)
    play(soundingRows(inbox(said([]))), true)
    expect(play(soundingRows(inbox(said([{ at: '2026-11-22T00:01:00Z', unread: true }]))), true)).toBe(true)
    t += 60_000
    expect(play(soundingRows(inbox(said([{ at: '2026-11-22T00:01:00Z', unread: true }, { at: '2026-11-22T00:05:00Z', unread: true }]))), true)).toBe(true)
    t += 60_000
    expect(play(soundingRows(inbox(said([{ at: '2026-11-22T00:01:00Z', unread: false }, { at: '2026-11-22T00:05:00Z', unread: false }]))), true)).toBe(false)
  })

  it("plays for a manager's second reply recorded in the same second as the first", () => {
    const play = tab(() => 10_000_000)
    const one = { at: '2026-11-22T00:01:00Z', unread: true }
    play(soundingRows(inbox(said([one]))), true)
    const two = inbox(said([one, { ...one }]))
    expect(two.messages[0].talk?.count).toBe(2)
    expect(play(soundingRows(two), true)).toBe(true)
  })

  const talked = (turns: { at: string; answeredAt: string }[]) => {
    const design = {
      id: '3', kind: 'design', state: 'done', title: 'D', agent: 'impl', inputs: [], input: { text: '', causes: [] }, output: {}, plans: [],
      threads: [{ discuss: true, turns: turns.map((x, k) => ({ text: `q${k}`, answer: `a${k}`, answerBy: 'impl', unread: true, ...x })) }],
    }
    return base({ intents: [{ id: 'i1', title: 'H', state: 'active', manager: 'm', chores: [], units: [{ id: 'u1', title: 'One', tasks: [design], commits: [] }] }] })
  }

  it("plays for a discussion's new answer while an older one is unread", () => {
    let t = 10_000_000
    const play = tab(() => t)
    const turn = (n: number) => ({ at: `2026-11-22T00:0${n}:00Z`, answeredAt: `2026-11-22T00:0${n}:30Z` })
    play(soundingRows(inbox(talked([turn(1)]))), true)
    t += 60_000
    expect(inbox(talked([turn(1), turn(2)])).messages).toHaveLength(1)
    expect(play(soundingRows(inbox(talked([turn(1), turn(2)]))), true)).toBe(true)
  })

  it("plays for a discussion's second answer in the same second as the first", () => {
    const play = tab(() => 10_000_000)
    const turn = { at: '2026-11-22T00:01:00Z', answeredAt: '2026-11-22T00:01:30Z' }
    play(soundingRows(inbox(talked([turn]))), true)
    expect(play(soundingRows(inbox(talked([turn, { ...turn }]))), true)).toBe(true)
  })

  it('plays once for the same row in two open tabs', () => {
    let t = 10_000_000
    const one = tab(() => t), two = tab(() => t)
    one(rows('gate:u1:2'), true); two(rows('gate:u1:2'), true)
    expect(one(rows('gate:u1:2', 'agent:s1'), true)).toBe(true)
    t += 300
    expect(two(rows('gate:u1:2', 'agent:s1'), true)).toBe(false)
    t += QUIET_MS
    expect(two(rows('gate:u1:2', 'agent:s1', 'agent:s2'), true)).toBe(true)
  })

  it('plays once for a row that another tab sees more than the quiet time later', () => {
    let t = 10_000_000
    const one = tab(() => t), two = tab(() => t)
    one([], true); two([], true)
    expect(one(rows('gate:u1:2'), true)).toBe(true)
    t += QUIET_MS + 100
    expect(two(rows('gate:u1:2'), true)).toBe(false)
  })

  it('plays for a row that comes again, in any tab', () => {
    let t = 10_000_000
    const one = tab(() => t), two = tab(() => t)
    one([], true); two([], true)
    expect(one([{ key: 'agent:s1', once: 'agent:s1@1' }], true)).toBe(true)
    one([], true); two([], true)
    t += QUIET_MS + 100
    // The same agent asks again: the row comes a second time.
    expect(two([{ key: 'agent:s1', once: 'agent:s1@2' }], true)).toBe(true)
  })
})

describe('the sound', () => {
  beforeEach(() => localStorage.clear())
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

  it('a tab the browser holds back leaves the sound to a tab that can play', async () => {
    // The held tab sees the new gate first; the other a moment later.
    vi.resetModules()
    const heldNotes = fake('suspended', false)
    const held = await import('./sound')
    const first = held.rowPlayer()
    await first([], true)
    expect(await first(rows('gate:u1:2'), true)).toBe(false)
    expect(heldNotes).toEqual([])

    vi.resetModules()
    const notes = fake('running', true)
    const open = await import('./sound')
    const second = open.rowPlayer()
    await second([], true)
    expect(await second(rows('gate:u1:2'), true)).toBe(true)
    expect(notes).toEqual([660, 880])
  })

  it('without Web Audio, says it could not', async () => {
    vi.stubGlobal('AudioContext', undefined)
    expect(await chime()).toBe(false)
    expect(soundMissed()).toBe(true)
  })
})
