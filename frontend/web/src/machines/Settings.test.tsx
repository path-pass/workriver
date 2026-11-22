// Settings, mounted: one group shows at a time, as a tab; the tab picked is kept in this
// browser; and a link to a setting opens Settings on its group's tab.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Workspace } from '@/workspace/types'

vi.mock('@/api/actions', async original => ({
  ...(await original<typeof import('@/api/actions')>()),
  // The service's values: the stall minutes come; the rest never arrive (nothing to draw).
  stallMinutes: async () => 10,
  metrics: () => new Promise(() => {}),
  machines: () => new Promise(() => {}),
}))
const { openSettings, Settings } = await import('./Settings')

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

/** A workspace here with a project to set: every group has a tab. */
const ws = { workspaces: [{ id: 'w1', label: 'main' }], projects: [{ id: 'p1', title: 'Notes' }] } as unknown as Workspace

let root: Root
let host: HTMLDivElement
beforeEach(() => { localStorage.clear(); host = document.createElement('div'); document.body.append(host); root = createRoot(host) })
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
    expect(tabs().map(b => b.textContent)).toEqual(['General', 'Stopped workers', 'Workspaces’ projects', 'Machine alerts', 'Other machines'])
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
    localStorage.setItem('workriver.settings.tab', 'projects')
    await mount({ workspaces: [], projects: [] } as unknown as Workspace)
    await open()
    expect(tabs().map(b => b.textContent)).not.toContain('Workspaces’ projects')
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
