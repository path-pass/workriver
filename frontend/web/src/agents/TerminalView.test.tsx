// The terminal view, mounted with a stand-in for xterm.js and for EventSource: it streams only
// while it is shown (folded, the page hidden or closed ends the stream; shown again opens a new
// one), a stream that ends is reopened once, and the terminal is created read-only, with a link
// handler that does nothing and OSC 52 swallowed, so nothing it draws reaches the clipboard or
// opens anything.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import type { Session } from './types'

const xterm = vi.hoisted(() => ({ made: [] as { cols: number; options: Record<string, unknown>; osc: Map<number, (d: string) => boolean>; written: Uint8Array[]; disposed: boolean }[] }))
vi.mock('@xterm/xterm', () => ({
  Terminal: class {
    cols = 80
    rows = 24
    options: Record<string, unknown>
    osc = new Map<number, (d: string) => boolean>()
    written: Uint8Array[] = []
    disposed = false
    parser = { registerOscHandler: (n: number, f: (d: string) => boolean) => { this.osc.set(n, f); return { dispose() {} } } }
    constructor(options: Record<string, unknown>) { this.options = { ...options }; xterm.made.push(this) }
    open() {}
    reset() {}
    resize(c: number, r: number) { this.cols = c; this.rows = r }
    write(b: Uint8Array) { this.written.push(b) }
    dispose() { this.disposed = true }
  },
}))
vi.mock('@xterm/xterm/css/xterm.css', () => ({}))
const { TerminalDock } = await import('./TerminalView')

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

/** EventSource as the page uses it: every one made, open until closed. */
class FakeSource {
  static all: FakeSource[] = []
  url: string
  closed = false
  onerror: (() => void) | null = null
  on = new Map<string, (e: MessageEvent) => void>()
  constructor(url: string) { this.url = url; FakeSource.all.push(this) }
  addEventListener(type: string, f: (e: MessageEvent) => void) { this.on.set(type, f) }
  close() { this.closed = true }
  emit(type: string, data: unknown) { this.on.get(type)?.(new MessageEvent(type, { data: JSON.stringify(data) })) }
}
const open = () => FakeSource.all.filter(s => !s.closed)

const s: Session = { agent: 'ja-impl', session: 's-1', machine: 'mini', ended: false, state: 'working' }
let root: Root, host: HTMLDivElement
let hidden = false
beforeEach(() => {
  FakeSource.all = []
  xterm.made = []
  hidden = false
  vi.stubGlobal('EventSource', FakeSource)
  Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => (hidden ? 'hidden' : 'visible') })
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
})
afterEach(() => { act(() => root.unmount()); host.remove(); vi.unstubAllGlobals(); localStorage.clear() })

const settle = () => act(() => new Promise<void>(r => setTimeout(r, 0)))
const show = async (gone = false) => {
  act(() => root.render(<TooltipProvider><TerminalDock s={s} gone={gone} onClose={() => {}} /></TooltipProvider>))
  await settle()
}
const press = (label: string) => act(() => host.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!.click())
const setHidden = (h: boolean) => act(() => { hidden = h; document.dispatchEvent(new Event('visibilitychange')) })

describe('the terminal view', () => {
  it('streams its agent while shown, and only then', async () => {
    await show()
    expect(open().map(x => x.url)).toEqual(['/api/agent/view?agent=s-1&machine=mini'])
    press('Fold the terminal view')
    expect(open()).toEqual([])
    press('Show the terminal view')
    expect(open()).toHaveLength(1)
    expect(FakeSource.all).toHaveLength(2)
  })

  it('ends the stream when the page is hidden, and opens a new one when it is shown again', async () => {
    await show()
    setHidden(true)
    expect(open()).toEqual([])
    setHidden(false)
    expect(open()).toHaveLength(1)
    expect(FakeSource.all).toHaveLength(2)
  })

  it('ends the stream when closed', async () => {
    await show()
    act(() => root.render(<TooltipProvider><div /></TooltipProvider>))
    expect(open()).toEqual([])
  })

  it('reopens a stream that ended once, then says why it ended', async () => {
    await show()
    act(() => FakeSource.all[0].emit('end', { reason: 'reader too slow' }))
    expect(open()).toHaveLength(1)
    act(() => FakeSource.all[1].onerror!())
    expect(open()).toEqual([])
    expect(FakeSource.all).toHaveLength(2)
    expect(host.textContent).toContain('The view ended: reader too slow')
  })

  it('opens no stream for an agent that is gone, and says so', async () => {
    await show(true)
    expect(FakeSource.all).toEqual([])
    expect(host.textContent).toContain('The agent is gone')
  })

  it('draws read-only: no input, links do nothing, OSC 52 swallowed, nothing reaches the clipboard', async () => {
    const write = vi.fn()
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: write } })
    const opened = vi.spyOn(window, 'open').mockImplementation(() => null)
    const asked = vi.fn(() => true)
    vi.stubGlobal('confirm', asked)
    await show()
    const t = xterm.made[0]
    expect(t.options.disableStdin).toBe(true)
    const link = t.options.linkHandler as { activate: (e: MouseEvent, uri: string) => void; allowNonHttpProtocols: boolean }
    expect(link.allowNonHttpProtocols).toBe(false)
    link.activate(new MouseEvent('click'), 'file:///etc/passwd')
    expect(t.osc.get(52)!('c;aGVsbG8=')).toBe(true)
    // A frame with an OSC 8 link and an OSC 52 write is drawn as bytes, nothing more.
    const bytes = '\u001b]8;;https://evil.example\u0007click\u001b]8;;\u0007\u001b]52;c;aGVsbG8=\u0007'
    act(() => FakeSource.all[0].emit('frame', { full: true, cols: 100, rows: 30, seq: 1, bytes: btoa(bytes) }))
    expect(new TextDecoder().decode(t.written[0])).toBe(bytes)
    expect(t.cols).toBe(100)
    expect(write).not.toHaveBeenCalled()
    expect(opened).not.toHaveBeenCalled()
    expect(asked).not.toHaveBeenCalled()
  })
})
