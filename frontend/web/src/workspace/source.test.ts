// When the page reads the workspace after a change, and when catch-up reads bypass that wait.
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

  it.each([
    { duration: 100, gap: 5000 },
    { duration: 3000, gap: 6000 },
  ])('coalesces changes into one read $gap ms after a $duration ms read ends', async ({ duration, gap }) => {
    read.mockImplementationOnce(() => new Promise(resolve => setTimeout(() => resolve(snapshot), duration)))
    const stop = watchWorkspace(() => {}, () => {}, () => false)
    try {
      changed()
      await vi.advanceTimersByTimeAsync(duration)
      expect(read).toHaveBeenCalledTimes(1)
      changed()
      await vi.advanceTimersByTimeAsync(gap - 1)
      changed()
      expect(read).toHaveBeenCalledTimes(1)
      await vi.advanceTimersByTimeAsync(1)
      expect(read).toHaveBeenCalledTimes(2)
      await vi.advanceTimersByTimeAsync(gap)
      expect(read).toHaveBeenCalledTimes(2)
    } finally {
      stop()
    }
  })

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
      expect(seen).toHaveBeenNthCalledWith(2, pending)
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

  it('catches up on reconnect as soon as the read in flight ends', async () => {
    read.mockImplementationOnce(() => new Promise(resolve => setTimeout(() => resolve(snapshot), 100)))
    const stop = watchWorkspace(() => {}, () => {}, () => false)
    try {
      stream.onopen?.()
      changed()
      stream.onopen?.()
      changed()
      expect(read).toHaveBeenCalledTimes(1)
      await vi.advanceTimersByTimeAsync(100)
      expect(read).toHaveBeenCalledTimes(2)
      await vi.advanceTimersByTimeAsync(5000)
      expect(read).toHaveBeenCalledTimes(2)
    } finally {
      stop()
    }
  })

  it('catches up on reconnect while a change is waiting', async () => {
    const stop = watchWorkspace(() => {}, () => {}, () => false)
    try {
      stream.onopen?.()
      await settle()
      changed()
      expect(read).toHaveBeenCalledTimes(1)
      stream.onopen?.()
      await settle()
      expect(read).toHaveBeenCalledTimes(2)
      await vi.advanceTimersByTimeAsync(5000)
      expect(read).toHaveBeenCalledTimes(2)
    } finally {
      stop()
    }
  })

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

  it('catches up when shown while a change is waiting', async () => {
    const stop = watchWorkspace(() => {}, () => {}, () => false)
    try {
      await settle()
      changed()
      hide(true)
      expect(read).toHaveBeenCalledTimes(1)
      hide(false)
      await settle()
      expect(read).toHaveBeenCalledTimes(2)
      await vi.advanceTimersByTimeAsync(5000)
      expect(read).toHaveBeenCalledTimes(2)
    } finally {
      stop()
    }
  })

  it('paces an ordinary foreground change after a completed background read', async () => {
    hide(true)
    const stop = watchWorkspace(() => {}, () => {}, () => true)
    try {
      await settle()
      hide(false)
      changed()
      await vi.advanceTimersByTimeAsync(4999)
      expect(read).toHaveBeenCalledTimes(1)
      await vi.advanceTimersByTimeAsync(1)
      expect(read).toHaveBeenCalledTimes(2)
    } finally {
      stop()
    }
  })

  it('uses the hidden interval for a pending change when the tab goes into the background', async () => {
    read.mockImplementationOnce(() => new Promise(resolve => setTimeout(() => resolve(snapshot), 3000)))
    const stop = watchWorkspace(() => {}, () => {}, () => true)
    try {
      await vi.advanceTimersByTimeAsync(3000)
      changed()
      hide(true)
      await vi.advanceTimersByTimeAsync(1999)
      expect(read).toHaveBeenCalledTimes(1)
      await vi.advanceTimersByTimeAsync(1)
      expect(read).toHaveBeenCalledTimes(2)
    } finally {
      stop()
    }
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
