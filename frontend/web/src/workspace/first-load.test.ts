import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import indexHtml from '../../index.html?raw'
import type { Workspace } from './types'

const workspace: Workspace = {
  builtAt: '', runtime: '', repos: [], sessions: [], projects: [],
  intents: [], workspaces: [], messages: [],
}

let hidden: boolean
let paint: FrameRequestCallback | undefined
let send: ReturnType<typeof vi.fn>

function show(hiddenNow: boolean) {
  hidden = hiddenNow
  document.dispatchEvent(new Event('visibilitychange'))
}

function runFrame() {
  paint?.(performance.now())
  vi.runAllTimers()
}

async function startLoad() {
  const { recordWorkspaceReady, reportFirstBoardPaint } = await import('./first-load')
  return () => {
    recordWorkspaceReady(workspace, performance.now())
    reportFirstBoardPaint(workspace)
  }
}

beforeEach(() => {
  vi.resetModules()
  hidden = false
  paint = undefined
  send = vi.fn(async () => new Response(null, { status: 204 }))
  vi.stubGlobal('fetch', send)
  vi.spyOn(document, 'hidden', 'get').mockImplementation(() => hidden)
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { paint = callback; return 1 })
  vi.useFakeTimers({ toFake: ['setTimeout'] })
})

afterEach(() => {
  show(true)
  vi.useRealTimers()
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

it('skips a page that starts hidden and is shown before the board loads', async () => {
  hidden = true
  const boardReady = await startLoad()
  show(false)
  boardReady()
  runFrame()
  expect(send).not.toHaveBeenCalled()
})

it('skips a page that the visibility timeline shows was hidden before initialization', async () => {
  vi.spyOn(performance, 'getEntriesByType').mockImplementation(type => type === 'visibility-state' ? [
    { name: 'hidden', startTime: 0 } as PerformanceEntry,
    { name: 'visible', startTime: 5 } as PerformanceEntry,
  ] : [])
  const boardReady = await startLoad()
  boardReady()
  runFrame()
  expect(send).not.toHaveBeenCalled()
})

it('skips a page hidden and shown while the app script downloads without a visibility timeline', async () => {
  const page = new DOMParser().parseFromString(indexHtml, 'text/html')
  const headScript = page.querySelector('head script:not([src])')
  new Function(headScript?.textContent ?? '')()
  vi.spyOn(performance, 'getEntriesByType').mockReturnValue([])
  show(true)
  show(false)
  const boardReady = await startLoad()
  boardReady()
  runFrame()
  expect(send).not.toHaveBeenCalled()
})

it.each(['before the board loads', 'before the frame'])('skips a page hidden and shown %s', async when => {
  const boardReady = await startLoad()
  if (when === 'before the frame') boardReady()
  show(true)
  show(false)
  if (when === 'before the board loads') boardReady()
  runFrame()
  expect(send).not.toHaveBeenCalled()
})
