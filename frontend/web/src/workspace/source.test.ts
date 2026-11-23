// When the page reads the workspace: a hidden tab reads nothing unless the user asked to hear
// or be told of what comes (then at most once every few seconds); shown again, it catches up.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { HIDDEN_READ } from '@/config/defaults'
import type { Workspace } from './types'

const stream = new EventTarget() as EventTarget & { onopen?: () => void; onerror?: () => void; close(): void }
stream.close = () => {}
const snapshot: Workspace = { runtime: 'some-runtime', builtAt: '', repos: [], intents: [], sessions: [], alerts: [], messages: [] }
const read = vi.fn(async () => snapshot)
vi.mock('@/api/http', () => ({ changes: () => stream, read: () => read() }))
const { watchWorkspace } = await import('./source')

describe('watching the workspace', () => {
  let hidden = false
  beforeEach(() => {
    vi.useFakeTimers()
    read.mockReset().mockResolvedValue(snapshot)
    hidden = false
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => hidden })
  })
  afterEach(() => vi.useRealTimers())
  const settle = () => vi.advanceTimersByTimeAsync(0)
  const changed = () => stream.dispatchEvent(new Event('changed'))
  const hide = (h: boolean) => { hidden = h; document.dispatchEvent(new Event('visibilitychange')) }

  it('shows the board while the stream connects, then picks up Git data on changed', async () => {
    const pending = { ...snapshot, gitPending: true }
    read.mockResolvedValueOnce(pending).mockResolvedValueOnce(pending)
    const seen = vi.fn()
    const stop = watchWorkspace(seen, () => {}, () => false)
    try {
      await settle()
      expect(seen).toHaveBeenNthCalledWith(1, pending)
      stream.onopen?.()
      await settle()
      changed()
      await settle()
      expect(seen).toHaveBeenNthCalledWith(3, snapshot)
    } finally {
      stop()
    }
  })

  it('catches up if the stream opens during a pending first read', async () => {
    const pending = { ...snapshot, gitPending: true }
    read.mockResolvedValueOnce(pending)
    const seen = vi.fn()
    const stop = watchWorkspace(seen, () => {}, () => false)
    try {
      stream.onopen?.()
      await settle()
      expect(seen.mock.calls).toEqual([[pending], [snapshot]])
    } finally {
      stop()
    }
  })

  it('still tries the initial read if the stream fails, then reads again on reconnect', async () => {
    const stop = watchWorkspace(() => {}, () => {}, () => false)
    try {
      stream.onerror?.()
      await settle()
      expect(read).toHaveBeenCalledTimes(1)
      stream.onopen?.()
      await settle()
      expect(read).toHaveBeenCalledTimes(2)
    } finally {
      stop()
    }
  })

  it('reads nothing while neither the sound nor notifications is on, and once when shown', async () => {
    const stop = watchWorkspace(() => {}, () => {}, () => false)
    stream.onopen?.()
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
    stream.onopen?.()
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
