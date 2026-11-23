// When the page reads the workspace: a hidden tab reads nothing unless the user asked to hear
// or be told of what comes (then at most once every few seconds); shown again, it catches up.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { HIDDEN_READ } from '@/config/defaults'
import type { Seat } from '@/agents/types'
import type { Workspace } from './types'

const stream = new EventTarget() as EventTarget & { onopen?: () => void; onerror?: () => void; close(): void }
stream.close = () => {}
const snapshot: Workspace = { runtime: 'some-runtime', builtAt: '2026-11-21T10:00:00Z', repos: [], intents: [], sessions: [], alerts: [], messages: [] }
const read = vi.fn(async () => snapshot)
vi.mock('@/api/http', () => ({ changes: () => stream, read: () => read() }))
const { watchWorkspace } = await import('./source')

describe('a hidden tab', () => {
  let hidden = false
  beforeEach(() => {
    vi.useFakeTimers()
    read.mockClear()
    hidden = false
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => hidden })
  })
  afterEach(() => vi.useRealTimers())
  const settle = () => vi.advanceTimersByTimeAsync(0)
  const changed = () => stream.dispatchEvent(new Event('changed'))
  const hide = (h: boolean) => { hidden = h; document.dispatchEvent(new Event('visibilitychange')) }

  it('reads nothing while neither the sound nor notifications is on, and once when shown', async () => {
    const stop = watchWorkspace(() => {}, () => {}, () => false)
    await settle()
    expect(read).toHaveBeenCalledTimes(1)
    hide(true)
    changed(); changed()
    await vi.advanceTimersByTimeAsync(HIDDEN_READ * 3)
    expect(read).toHaveBeenCalledTimes(1)
    hide(false)
    await settle()
    expect(read).toHaveBeenCalledTimes(2)
    stop()
  })

  it('reads when told of a change while one is on, at most once every few seconds', async () => {
    const seen = vi.fn()
    const stop = watchWorkspace(seen, () => {}, () => true)
    await settle()
    hide(true)
    await vi.advanceTimersByTimeAsync(HIDDEN_READ)
    changed()
    await settle()
    expect(read).toHaveBeenCalledTimes(2)
    expect(seen).toHaveBeenCalledTimes(2)
    // More changes soon after: one read when the few seconds are over, not one each.
    changed(); changed()
    await vi.advanceTimersByTimeAsync(HIDDEN_READ / 2)
    expect(read).toHaveBeenCalledTimes(2)
    await vi.advanceTimersByTimeAsync(HIDDEN_READ)
    expect(read).toHaveBeenCalledTimes(3)
    // Shown again with nothing missed: no read.
    hide(false)
    await settle()
    expect(read).toHaveBeenCalledTimes(3)
    stop()
  })
})

it('dates each observed working spell and starts over for a new seat', async () => {
  vi.useFakeTimers()
  const delivered = vi.fn<(ws: Workspace) => void>()
  const manager = { session: 's1', name: 'boss', folder: '/w', since: 1 }
  const readNext = async (state: Seat['state'], since = 1) => {
    read.mockResolvedValueOnce({ ...snapshot, workspaces: [{ id: 'ws1', name: 'chat', project: 'p', manager: { ...manager, state, since } }] })
    stream.dispatchEvent(new Event('changed'))
    await vi.advanceTimersByTimeAsync(0)
    return delivered.mock.lastCall?.[0].workspaces?.[0].manager?.workingSince
  }
  const stop = watchWorkspace(delivered, () => {}, () => false)
  try {
    await vi.advanceTimersByTimeAsync(0)
    expect(await readNext('idle')).toBeUndefined()
    const first = Date.now()
    expect(await readNext('working')).toBe(first)
    vi.setSystemTime(first + 60_000)
    expect(await readNext('working')).toBe(first)
    expect(await readNext('blocked')).toBeUndefined()
    const resumed = Date.now()
    expect(await readNext('working')).toBe(resumed)
    vi.setSystemTime(resumed + 60_000)
    expect(await readNext('working', 2)).toBe(Date.now())
  } finally {
    stop()
    vi.useRealTimers()
  }
})
