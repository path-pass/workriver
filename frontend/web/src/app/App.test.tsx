import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import type { Workspace } from '@/workspace/types'
import { watchWorkspace } from '@/workspace/source'
import App from './App'

vi.mock('@/workspace/source', () => ({ MOCK: false, watchWorkspace: vi.fn() }))

const at = '2026-11-21T09:00:00Z'
const ws: Workspace = {
  repos: [], builtAt: at, runtime: 'herdr', sessions: [],
  projects: [{ id: '/project', title: 'Project', repos: [] }],
  intents: [
    { id: 'i1', title: 'First intent', project: '/project', workspace: 'ws1', units: [], chores: [] },
    { id: 'i2', title: 'Second intent', project: '/project', workspace: 'ws1', units: [], chores: [] },
  ],
  workspaces: [{ id: 'ws1', name: 'project', project: '/project' }],
  messages: [{
    id: 'm1', workspace: 'ws1', manager: 'boss', text: 'Please read my notes.', at, step: 'delivered',
    attachments: [{ id: 'a1', name: 'notes.md', url: '/api/attachment/a1', type: 'text/markdown' }],
    replies: [{ by: 'boss', text: '[Second intent](http://127.0.0.1:8000/i2)', at }],
  }],
}

let root: Root
let host: HTMLDivElement
let before: string
let actEnvironment: boolean | undefined

beforeEach(() => {
  actEnvironment = globalThis.IS_REACT_ACT_ENVIRONMENT
  globalThis.IS_REACT_ACT_ENVIRONMENT = true
  before = location.href
  history.replaceState(null, '', '/i1')
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  vi.mocked(watchWorkspace).mockImplementation(onData => { onData(ws); return () => {} })
  vi.stubGlobal('fetch', async (url: string) => {
    if (url !== '/api/attachment/a1') throw new Error(`Unexpected fetch: ${url}`)
    return new Response('[This intent](/i1)')
  })
})

afterEach(() => {
  act(() => root.unmount())
  host.remove()
  localStorage.removeItem('workriver.side')
  history.replaceState(null, '', before)
  vi.unstubAllGlobals()
  globalThis.IS_REACT_ACT_ENVIRONMENT = actEnvironment ?? false
})

const render = () => act(() => root.render(<TooltipProvider><App /></TooltipProvider>))
const heading = () => host.querySelector('main header h1')?.textContent
const sheet = () => document.querySelector('[data-slot="sheet-content"]')
const viewer = () => document.querySelector('[data-slot="dialog-content"]')

it('closes the Manager sheet after following its Markdown link', async () => {
  render()
  expect(heading()).toContain('First intent')
  await act(async () => { host.querySelector<HTMLButtonElement>('main button[aria-label="Manager"]')!.click() })
  await act(async () => { await vi.dynamicImportSettled() })
  expect(sheet()?.getAttribute('data-state')).toBe('open')
  const link = await vi.waitFor(() => {
    const link = sheet()?.querySelector<HTMLAnchorElement>('a[href="/i2"]')
    expect(link?.textContent).toBe('Second intent')
    return link!
  })

  const entries = history.length
  act(() => link.click())

  expect(location.pathname).toBe('/i2')
  expect(heading()).toContain('Second intent')
  expect(sheet()?.getAttribute('data-state')).toBeUndefined()
  expect(history.length).toBe(entries + 1)
  await act(async () => history.back())
  expect(heading()).toContain('First intent')
})

it('closes the attachment viewer when its Markdown link points to the current address', async () => {
  localStorage.setItem('workriver.side', 'talk')
  render()
  const attachment = [...host.querySelectorAll('aside button')].find(button => button.textContent === 'notes.md') as HTMLButtonElement
  await act(async () => { attachment.click() })
  await act(async () => { await vi.dynamicImportSettled() })
  expect(viewer()?.querySelector('[data-slot="dialog-title"]')?.textContent).toBe('notes.md')
  const link = await vi.waitFor(() => {
    const link = viewer()?.querySelector<HTMLAnchorElement>('a[href="/i1"]')
    expect(link?.textContent).toBe('This intent')
    return link!
  })

  const entries = history.length
  act(() => link.click())

  expect(location.pathname).toBe('/i1')
  expect(heading()).toContain('First intent')
  expect(attachment.isConnected).toBe(true)
  expect(viewer()?.getAttribute('data-state')).toBeUndefined()
  expect(history.length).toBe(entries)
})
