import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import type { Workspace } from '@/workspace/types'
import { watchWorkspace } from '@/workspace/source'
import App from './App'

vi.mock('@/workspace/source', () => ({ MOCK: false, watchWorkspace: vi.fn() }))

const at = '2026-11-21T09:00:00Z'
const firstNote = '[This intent](/i1)'
const ws: Workspace = {
  repos: [], builtAt: at, runtime: 'herdr', sessions: [],
  projects: [{ id: '/project', title: 'Project', repos: [] }],
  intents: [
    { id: 'i1', title: 'First intent', project: '/project', workspace: 'ws1', units: [], chores: [], note: firstNote, version: 2, edited: { at, by: null } },
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
    switch (url) {
      case '/api/attachment/a1': return new Response(firstNote)
      case '/api/intent/i1/history': return Response.json({ versions: [
        { n: 1, at, by: 'user', how: 'created', title: 'First intent' },
        { n: 2, at, by: 'user', how: 'edited', note: firstNote },
      ] })
      case '/api/intent/i1/history?version=2': return Response.json({
        text: { title: 'First intent', goal: '', note: firstNote },
        before: { title: 'First intent', goal: '', note: '' },
      })
      case '/api/run/prepare': return Response.json({
        id: 'run-example', command: 'pwd', dir: '/project', by: 'user', started: false, output: '', exit: null, watching: false,
        review: { state: 'done', verdict: 'ok', text: firstNote },
      })
      default: throw new Error(`Unexpected fetch: ${url}`)
    }
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
const dialog = () => document.querySelector('[data-slot="dialog-content"]')
const noteField = () => dialog()?.querySelector<HTMLTextAreaElement>('textarea[aria-labelledby="edit-note"]')

function type(field: HTMLInputElement | HTMLTextAreaElement, value: string) {
  const set = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(field), 'value')!.set!
  act(() => { set.call(field, value); field.dispatchEvent(new Event('input', { bubbles: true })) })
}

const button = (where: ParentNode, name: string) => [...where.querySelectorAll('button')].find(b => (b.getAttribute('aria-label') ?? b.textContent) === name)!
const mainButton = (name: string) => button(host.querySelector('main')!, name)

async function dialogLink(path: string) {
  await act(async () => { await vi.dynamicImportSettled() })
  return vi.waitFor(() => {
    const link = dialog()?.querySelector<HTMLAnchorElement>(`a[href="${path}"]`)
    expect(link).toBeTruthy()
    return link!
  })
}

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
  expect(dialog()?.querySelector('[data-slot="dialog-title"]')?.textContent).toBe('notes.md')
  const link = await vi.waitFor(() => {
    const link = dialog()?.querySelector<HTMLAnchorElement>('a[href="/i1"]')
    expect(link?.textContent).toBe('This intent')
    return link!
  })

  const entries = history.length
  act(() => link.click())

  expect(location.pathname).toBe('/i1')
  expect(heading()).toContain('First intent')
  expect(attachment.isConnected).toBe(true)
  expect(dialog()?.getAttribute('data-state')).toBeUndefined()
  expect(history.length).toBe(entries)
})

it.each([
  { path: '/i2', title: 'Second intent', note: '', entriesAdded: 1 },
  { path: '/i1', title: 'First intent', note: firstNote, entriesAdded: 0 },
])('closes the edit draft after following a preview link to $path', async ({ path, title, note, entriesAdded }) => {
  render()
  act(() => mainButton('Edit the intent').click())
  type(noteField()!, `Tracked in http://127.0.0.1:8000${path}`)
  act(() => button(dialog()!, 'Preview').click())
  const link = await dialogLink(path)

  const entries = history.length
  act(() => link.click())

  expect(location.pathname).toBe(path)
  expect(heading()).toContain(title)
  expect(dialog()?.getAttribute('data-state')).toBeUndefined()
  expect(history.length).toBe(entries + entriesAdded)
  act(() => mainButton('Edit the intent').click())
  expect(noteField()?.value).toBe(note)
})

it('closes intent history after following a link to the intent in Whole text', async () => {
  render()
  await act(async () => { mainButton('History').click() })
  act(() => button(dialog()!, 'Whole text').click())
  const link = await dialogLink('/i1')

  act(() => link.click())

  expect(location.pathname).toBe('/i1')
  expect(dialog()?.getAttribute('data-state')).toBeUndefined()
})

it.each(['Edit the intent', 'History'])('closes %s when Back shows another intent', async name => {
  history.replaceState(null, '', '/i2')
  history.pushState(null, '', '/i1')
  render()
  await act(async () => { mainButton(name).click() })
  expect(dialog()?.getAttribute('data-state')).toBe('open')

  await act(async () => history.back())

  expect(heading()).toContain('Second intent')
  expect(dialog()?.getAttribute('data-state')).toBeUndefined()
})

it('closes the command review after following a link to the current intent', async () => {
  localStorage.setItem('workriver.side', 'run')
  render()
  const run = host.querySelector('aside section[aria-labelledby="side-run"]')!
  type(run.querySelector<HTMLInputElement>('input[aria-label="A command, run as you"]')!, 'pwd')
  await act(async () => { button(run, 'Run').click() })
  const link = await dialogLink('/i1')

  act(() => link.click())

  expect(location.pathname).toBe('/i1')
  expect(dialog()?.getAttribute('data-state')).toBeUndefined()
})
