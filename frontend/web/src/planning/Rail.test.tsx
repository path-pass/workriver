import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it } from 'vitest'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import type { Workspace } from '@/workspace/types'
import { Rail } from './Rail'
import type { Unit, UnitProgress } from './types'

const unit = (id: string, title: string, progress?: UnitProgress): Unit => ({
  id, title, progress, purpose: '', repo: '/example', branch: 'main', status: 'active', tasks: [], commits: [],
})
const ws: Workspace = {
  repos: [], builtAt: '2026-11-26T09:00:00Z', runtime: 'Runner', sessions: [],
  projects: [{ id: '/example', title: 'Example', repos: [] }],
  workspaces: [
    { id: 'ws1', name: 'Example', project: '/example' },
    { id: 'ws2', name: 'Second workspace', project: '/example' },
  ],
  intents: [
    {
      id: 'i1', title: 'Mixed states', workspace: 'ws1', project: '/example', chores: [],
      units: [unit('u1', 'Current work', 'working'), unit('u2', 'Another unit'), unit('u3', 'Waiting work', 'waiting'), unit('u4', 'Closed work', 'done')],
    },
    { id: 'i2', title: 'Only working', workspace: 'ws2', project: '/example', chores: [], units: [unit('u5', 'Other work', 'working')] },
    { id: 'i3', title: 'Planned work', project: '/example', state: 'planned', units: [], chores: [] },
  ],
}

let host: HTMLDivElement
let root: Root
let actEnvironment: boolean | undefined
beforeEach(() => {
  actEnvironment = globalThis.IS_REACT_ACT_ENVIRONMENT
  globalThis.IS_REACT_ACT_ENVIRONMENT = true
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
})
afterEach(() => {
  act(() => root.unmount())
  host.remove()
  localStorage.removeItem('workriver.rail.unitStates')
  globalThis.IS_REACT_ACT_ENVIRONMENT = actEnvironment ?? false
})

const noop = () => {}
const show = (unitId = 'i1', q = '') => act(() => root.render(
  <TooltipProvider>
    <Rail ws={ws} unitId={unitId} onUnit={noop} q={q} onQ={noop} open onToggle={noop}
      project={null} onProject={noop} onStart={noop} onIntent={noop} onDashboard={noop} />
  </TooltipProvider>,
))
const unitRows = () => Array.from(host.querySelectorAll('button[aria-pressed]'))
  .flatMap(b => b.textContent?.match(/^u\d+/)?.[0] ?? [])
const button = (name: string) => Array.from(host.querySelectorAll('button'))
  .find(b => b.getAttribute('aria-label')?.startsWith(name) || b.textContent === name)!
const openFilters = () => act(() => button('Filter').click())
const checkbox = (name: string) => Array.from(document.querySelectorAll('fieldset label'))
  .find(label => label.textContent === name)?.querySelector<HTMLInputElement>('input')
const pick = (name: string) => {
  expect(checkbox(name), name).toBeDefined()
  act(() => checkbox(name)!.click())
}

it('shows all unit states by default', () => {
  show()
  act(() => button('Show or hide the units of Only working').click())
  expect(unitRows()).toEqual(['u1', 'u2', 'u3', 'u4', 'u5'])
})

it('shows either selected state, keeps the open unit reachable and hides empty intent groups', () => {
  show()
  openFilters()
  pick('Waiting')
  pick('Done')
  expect(unitRows()).toEqual(['u3', 'u4'])
  expect(host.textContent).not.toContain('Only working')
  expect(host.textContent).toContain('Planned work')
  show('u1')
  expect(unitRows()).toEqual(['u1', 'u3', 'u4'])
  act(() => button('Clear').click())
  expect(unitRows()).toEqual(['u1', 'u2', 'u3', 'u4'])
  expect(host.textContent).toContain('Only working')
})

it('restores selected states when the rail mounts again', () => {
  show()
  openFilters()
  pick('Done')
  act(() => root.unmount())
  root = createRoot(host)
  show()
  expect(unitRows()).toEqual(['u4'])
  openFilters()
  expect(checkbox('Done')?.checked).toBe(true)
  expect(checkbox('Waiting')?.checked).toBe(false)
})

it('applies the unit state selection when search matches an intent ID', () => {
  show('i1', 'i1')
  openFilters()
  pick('Working')
  expect(unitRows()).toEqual(['u1', 'u2'])
  show('i1', 'i2')
  expect(unitRows()).toEqual(['u5'])
  pick('Working')
  pick('Waiting')
  expect(unitRows()).toEqual([])
  expect(host.textContent).toContain('No units match the current filters.')
})

it('hides workspaces with no matching intents and reports that no units match', () => {
  show()
  openFilters()
  pick('Waiting')
  expect(host.textContent).not.toContain('Second workspace')
  pick('Waiting')
  pick('Merged')
  expect(host.textContent).not.toContain('No open workspace')
  expect(host.textContent).toContain('No units match the current filters.')
  expect(host.textContent).toContain('Planned work')
})
