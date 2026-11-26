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
      id: 'i1', title: 'Mixed states', workspace: 'ws1', project: '/example', chores: [], kind: 'feature',
      units: [unit('u1', 'Current work', 'working'), unit('u2', 'Another unit'), unit('u3', 'Waiting work', 'waiting'), unit('u4', 'Closed work', 'done')],
    },
    { id: 'i2', title: 'Only working', workspace: 'ws2', project: '/example', chores: [], units: [unit('u5', 'Other work', 'working')] },
    { id: 'i3', title: 'Planned work', project: '/example', state: 'planned', units: [], chores: [] },
  ],
}
const withDone: Workspace = {
  ...ws,
  intents: [...ws.intents, {
    id: 'i4', title: 'Finished intent', workspace: 'ws1', project: '/example', state: 'done', chores: [],
    units: [unit('u6', 'Delivered work', 'done')],
  }],
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
  localStorage.removeItem('workriver.filter.*')
  globalThis.IS_REACT_ACT_ENVIRONMENT = actEnvironment ?? false
})

const noop = () => {}
const show = (unitId = 'i1', q = '', data = ws) => act(() => root.render(
  <TooltipProvider>
    <Rail ws={data} unitId={unitId} onUnit={noop} q={q} onQ={noop} open onToggle={noop}
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

it('reveals a matching unit in another intent inside a folded workspace', () => {
  show()
  const workspace = 'Show or hide the intents of workspace Second workspace'
  act(() => button(workspace).click())
  expect(unitRows()).not.toContain('u5')
  openFilters()
  pick('Working')
  expect(unitRows()).toEqual(['u1', 'u2', 'u5'])
  act(() => button('Clear').click())
  expect(button(workspace).getAttribute('aria-expanded')).toBe('false')
  expect(unitRows()).not.toContain('u5')
})

it('opens the Done section for matching units and restores its default fold on clear', () => {
  show('i1', '', withDone)
  expect(button('Done1').getAttribute('aria-expanded')).toBe('false')
  openFilters()
  pick('Done')
  expect(button('Done1').getAttribute('aria-expanded')).toBe('true')
  expect(unitRows()).toEqual(['u4', 'u6'])
  act(() => button('Clear').click())
  expect(button('Done1').getAttribute('aria-expanded')).toBe('false')
})

it('keeps manual folds until the selected unit states change', () => {
  const workspace = 'Show or hide the intents of workspace Example'
  const intent = 'Show or hide the units of Mixed states'
  const done = 'Done1'
  const toggle = (name: string) => act(() => button(name).click())
  const expanded = (name: string) => button(name).getAttribute('aria-expanded')

  show('i1', '', withDone)
  toggle(intent)
  toggle(workspace)
  expect(unitRows()).toEqual([])
  openFilters()
  pick('Done')
  expect(unitRows()).toEqual(['u4', 'u6'])

  toggle(intent)
  expect(expanded(intent)).toBe('false')
  toggle(workspace)
  toggle(done)
  show('i1', '', { ...withDone, builtAt: '2026-11-26T09:01:00Z' })
  expect(expanded(workspace)).toBe('false')
  expect(expanded(done)).toBe('false')
  toggle(workspace)
  expect(expanded(intent)).toBe('false')
  toggle(intent)
  toggle(done)
  expect(unitRows()).toEqual(['u4', 'u6'])

  toggle(intent)
  toggle(workspace)
  toggle(done)
  pick('Waiting')
  expect(unitRows()).toEqual(['u3', 'u4', 'u6'])

  toggle('Clear')
  expect(expanded(workspace)).toBe('false')
  expect(expanded(done)).toBe('false')
  toggle(workspace)
  expect(expanded(intent)).toBe('false')
})

it('keeps a manual Done expansion through state filtering and resets when search alone hides it', () => {
  show('i1', '', withDone)
  act(() => button('Done1').click())
  openFilters()
  pick('Waiting')
  expect(button('Done1')).toBeUndefined()
  act(() => button('Clear').click())
  expect(button('Done1').getAttribute('aria-expanded')).toBe('true')
  show('i1', 'i1', withDone)
  expect(button('Done1')).toBeUndefined()
  show('i1', '', withDone)
  expect(button('Done1').getAttribute('aria-expanded')).toBe('false')
})

it('restores a manual Done expansion when the whole No project section was filtered away', () => {
  const unassigned = { ...withDone, intents: withDone.intents.map(i => i.state === 'done' ? { ...i, project: undefined } : i) }
  show('i1', '', unassigned)
  act(() => button('Done1').click())
  openFilters()
  pick('Waiting')
  expect(button('Done1')).toBeUndefined()
  act(() => button('Clear').click())
  expect(button('Done1').getAttribute('aria-expanded')).toBe('true')
})

it('names selected unit states even when intent filters are also active', () => {
  show()
  openFilters()
  pick('Waiting')
  pick('Done')
  expect(host.textContent).toContain('Showing units: Waiting, Done')
  pick('Feature')
  expect(host.textContent).toContain('Showing units: Waiting, Done')
  expect(host.textContent).toContain('Filtered: 1 of 3')
})
