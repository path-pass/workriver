// Settings, mounted: one group shows at a time, as a tab; the tab picked is kept in this
// browser; a link to a setting opens Settings on its group's tab; a group whose setting has
// not come says it is loading or why it cannot be read. The default of a manager's start:
// saved, and tried once, with what came of it.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Workspace } from '@/workspace/types'

/** The service's reads, each test's own: by default the stall minutes come; the rest never arrive. */
const reads = vi.hoisted(() => ({
  stall: (): Promise<number> => Promise.resolve(10),
  metrics: (): Promise<unknown> => new Promise(() => {}),
  start: (): Promise<unknown> => new Promise(() => {}),
  saved: [] as [string, string][],
  save: (h: string, m: string): Promise<unknown> => Promise.resolve({ harness: h, model: m, harnesses: [] }),
  tried: [] as [string, string][],
  try: (): Promise<unknown> => new Promise(() => {}),
}))
vi.mock('@/api/actions', async original => ({
  ...(await original<typeof import('@/api/actions')>()),
  stallMinutes: () => reads.stall(),
  metrics: () => reads.metrics(),
  machines: () => new Promise(() => {}),
  startSettings: () => reads.start(),
  setStartSettings: (h: string, m: string) => { reads.saved.push([h, m]); return reads.save(h, m) },
  tryStart: (h: string, m: string) => { reads.tried.push([h, m]); return reads.try() },
}))
const { openSettings, Settings } = await import('./Settings')

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

/** An open workspace with its manager and a closed one: every group has a tab. */
const ws = { workspaces: [{ id: 'ws1', name: 'main', project: '/notes', manager: { session: 's1', name: 'boss', folder: '/notes', since: 3, state: 'idle' } }, { id: 'ws2', name: 'old', project: '/notes', closed: true }], projects: [{ id: '/notes', key: 'p1', title: 'Notes' }], sessions: [], intents: [] } as unknown as Workspace

let root: Root
let host: HTMLDivElement
beforeEach(() => {
  reads.stall = () => Promise.resolve(10)
  reads.metrics = () => new Promise(() => {})
  reads.start = () => new Promise(() => {})
  reads.saved = []; reads.tried = []
  reads.try = () => new Promise(() => {})
  localStorage.clear()
  host = document.createElement('div'); document.body.append(host); root = createRoot(host)
})
afterEach(() => { act(() => root.unmount()); host.remove() })

const settle = () => act(async () => { await new Promise(r => setTimeout(r)) })
const mount = async (w: Workspace | null = ws) => { act(() => root.render(<Settings ws={w} />)); await settle() }
const open = async () => { await act(async () => { [...document.querySelectorAll('button')].find(b => b.textContent === 'Settings')!.click() }); await settle() }
const tabs = () => [...document.querySelectorAll<HTMLButtonElement>('[role=dialog] [role=tab]')]
const tab = (name: string) => tabs().find(b => b.textContent === name)!
const selected = () => tabs().find(b => b.getAttribute('aria-selected') === 'true')?.textContent
/** The panels shown (Radix keeps an empty, hidden one per other tab). */
const panels = () => [...document.querySelectorAll<HTMLElement>('[role=dialog] [role=tabpanel]')].filter(p => !p.hidden)
/** Radix picks a tab on the mouse going down, as a person's click does. */
const pick = async (name: string) => { await act(async () => { tab(name).dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 })) }); await settle() }
const text = () => document.querySelector('[role=dialog]')!.textContent!

describe('Settings in tabs', () => {
  it('shows one group at a time, the general one first', async () => {
    await mount()
    await open()
    expect(tabs().map(b => b.textContent)).toEqual(['General', 'Stopped workers', 'Workspaces', 'Starting managers', 'Machine alerts', 'Other machines'])
    expect(selected()).toBe('General')
    expect(panels()).toHaveLength(1)
    expect(text()).toContain('Language')
    expect(text()).not.toContain('Remind an agent idle')

    await pick('Stopped workers')
    expect(selected()).toBe('Stopped workers')
    expect(panels()).toHaveLength(1)
    expect(text()).toContain('Remind an agent idle')
    expect(text()).not.toContain('Language')
  })

  it('remembers the tab picked, and opens on it next time', async () => {
    await mount()
    await open()
    await pick('Machine alerts')
    expect(localStorage.getItem('workriver.settings.tab')).toBe('alerts')

    act(() => root.unmount())
    root = createRoot(host)
    await mount()
    await open()
    expect(selected()).toBe('Machine alerts')
  })

  it('a group with nothing to set has no tab; remembered, it opens on the general one', async () => {
    localStorage.setItem('workriver.settings.tab', 'workspaces')
    await mount({ workspaces: [], projects: [] } as unknown as Workspace)
    await open()
    expect(tabs().map(b => b.textContent)).not.toContain('Workspaces')
    expect(selected()).toBe('General')
  })

  it('a link to a setting opens Settings on its group’s tab', async () => {
    await mount()
    expect(document.querySelector('[role=dialog]')).toBeNull()
    await act(async () => { openSettings('machines') })
    await settle()
    expect(selected()).toBe('Other machines')
    expect(text()).toContain('Add')
  })

  it('a link to a setting leaves the tab picked as it was', async () => {
    await mount()
    await open()
    await pick('Stopped workers')
    await act(async () => { openSettings('machines') })
    await settle()
    expect(selected()).toBe('Other machines')
    expect(localStorage.getItem('workriver.settings.tab')).toBe('workers')

    await act(async () => { document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })) })
    await settle()
    expect(document.querySelector('[role=dialog]')).toBeNull()
    await open()
    expect(selected()).toBe('Stopped workers')
  })

  it('a group whose setting is still coming says it is loading', async () => {
    reads.stall = () => new Promise(() => {})
    await mount()
    await open()
    await pick('Stopped workers')
    expect(panels()[0].textContent).toBe('Loading…')
    await pick('Machine alerts')
    expect(panels()[0].textContent).toBe('Loading…')
    expect(document.querySelector('[role=dialog] [role=alert]')).toBeNull()
  })

  it('a group whose setting cannot be read says why', async () => {
    reads.stall = () => Promise.reject(new Error('the service is gone'))
    reads.metrics = () => Promise.reject(new Error('the session is gone'))
    await mount()
    await open()
    await pick('Stopped workers')
    expect(panels()[0].querySelector('[role=alert]')?.textContent).toBe('Cannot read it: Error: the service is gone')
    await pick('Machine alerts')
    expect(panels()[0].querySelector('[role=alert]')?.textContent).toBe('Cannot read it: Error: the session is gone')
  })

  it('moves between tabs with the arrow keys', async () => {
    await mount()
    await open()
    await act(async () => { tab('General').focus() })
    await act(async () => { tab('General').dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true })) })
    await settle()
    expect(document.activeElement).toBe(tab('Stopped workers'))
    expect(selected()).toBe('Stopped workers')
  })
})

describe('the default of a manager’s start', () => {
  const offers = [{ kind: 'claude', aliases: ['opus', 'sonnet', 'fable'] }, { kind: 'codex', aliases: [], why: 'not_installed' }]
  const button = (name: string) => [...document.querySelectorAll<HTMLButtonElement>('[role=dialog] button')].find(b => b.textContent === name)!
  const press = async (name: string) => { await act(async () => { button(name).click() }); await settle() }

  it('saves the harness and model picked, and says so', async () => {
    reads.start = () => Promise.resolve({ harness: 'claude', model: '', harnesses: offers })
    reads.save = (h, m) => Promise.resolve({ harness: h, model: m, harnesses: offers })
    await mount()
    await open()
    await pick('Starting managers')
    expect(document.querySelector('[role=dialog] [role=combobox]')!.textContent).toBe('Claude Code')
    expect(button('Save').disabled).toBe(true)
    await press('fable')
    await press('Save')
    expect(reads.saved).toEqual([['claude', 'fable']])
    expect(text()).toContain('Saved.')
    expect(button('Save').disabled).toBe(true)
  })

  it('shows a refusal in the service’s words', async () => {
    reads.start = () => Promise.resolve({ harness: 'claude', model: '', harnesses: offers })
    reads.save = () => Promise.reject(new Error('claude is not installed here'))
    await mount()
    await open()
    await pick('Starting managers')
    await press('opus')
    await press('Save')
    expect(document.querySelector('[role=dialog] [role=alert]')?.textContent).toBe('Not saved: claude is not installed here')
  })

  it('tries the harness and model once: busy meanwhile, then the result in words and its screen', async () => {
    reads.start = () => Promise.resolve({ harness: 'claude', model: 'opus', harnesses: offers })
    let done: (v: unknown) => void = () => {}
    reads.try = () => new Promise(r => { done = r })
    await mount()
    await open()
    await pick('Starting managers')
    await press('Try it')
    expect(reads.tried).toEqual([['claude', 'opus']])
    expect(text()).toContain('Trying… this can take a minute or more.')
    expect(button('Try it').disabled).toBe(true)
    await act(async () => { done({ ok: false, words: 'claude started but waits for you in its terminal (blocked)', screen: 'Is this a project you trust?\n❯ 1. Yes' }) })
    await settle()
    expect(text()).not.toContain('Trying…')
    expect(text()).toContain('It did not get ready: claude started but waits for you in its terminal (blocked)')
    expect(document.querySelector('[role=dialog] pre')!.textContent).toBe('Is this a project you trust?\n❯ 1. Yes')
    await act(async () => { button('Try it').click() })
    await act(async () => { done({ ok: true, words: 'claude started and was ready for a prompt', screen: '' }) })
    await settle()
    expect(text()).toContain('It started: claude started and was ready for a prompt')
    expect(document.querySelector('[role=dialog] pre')).toBeNull()
  })
})
