import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import type { Task } from '@/work/types'
import type { Workspace } from '@/workspace/types'
import { watchWorkspace } from '@/workspace/source'
import App from './App'

vi.mock('@/workspace/source', () => ({ MOCK: false, watchWorkspace: vi.fn() }))

const at = '2026-11-21T09:00:00Z'
const firstNote = '[This intent](/i1)'
const ws: Workspace = {
  repos: [], builtAt: at, runtime: 'Runner', sessions: [],
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
  localStorage.removeItem('workriver.rail')
  localStorage.removeItem('workriver.notify')
  history.replaceState(null, '', before)
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
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

it('opens statistics on the tenth consecutive mark click within four seconds', async () => {
  const read = vi.fn(async () => Response.json({
    intents: [], intent_took: [], units: [],
    messages: [{ id: 'm1', text: 'The user question', replies: [
      { text: 'An earlier answer', rating: { vote: 'up', at } },
      { text: 'The rated answer', rating: { vote: 'down', at, note: 'Missed the question' } },
    ] }],
  }))
  vi.stubGlobal('fetch', read)
  let now = 10_000
  vi.spyOn(Date, 'now').mockImplementation(() => now)
  render()
  expect(host.querySelector('a[href*="dashboard"]')).toBeNull()
  const click = (count: number) => {
    const mark = button(host, 'workriver')
    for (let n = 0; n < count; n++) mark.click()
  }
  act(() => click(9))
  expect(heading()).toContain('First intent')
  expect(read).not.toHaveBeenCalled()
  now += 4001
  act(() => click(1))
  expect(heading()).toContain('First intent')
  act(() => host.querySelector('main')!.click())
  act(() => click(9))
  expect(read).not.toHaveBeenCalled()
  now += 3999
  await act(async () => click(1))
  await act(async () => { await vi.dynamicImportSettled() })
  expect(heading()).toBe('Statistics')
  expect(read).toHaveBeenCalledExactlyOnceWith('/api/state?took=true')
  expect(host.querySelector('main')?.textContent).toContain('50%')
  expect(host.querySelector('main')?.textContent).toContain('Missed the question')
  expect(host.querySelector('main')?.textContent).toContain('The rated answer')
  expect(host.querySelector('main')?.textContent).not.toContain('An earlier answer')
  expect(location.pathname).toBe('/i1')
  act(() => mainButton('Back to workriver').click())
  expect(heading()).toContain('First intent')
})

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

it('keeps manager questions answerable in the conversation without adding them to the bar', async () => {
  const choice = { surface: { version: '1.0', createSurface: { components: [{}, { component: 'ChoicePicker', options: [{ value: 'yes', label: 'Yes' }] }] } } }
  const readQuestion = { choice, open: true, unread: false }
  const snapshot: Workspace = {
    ...ws,
    messages: [
      { id: 'm1', workspace: 'ws1', manager: 'boss', at, step: 'delivered', byManager: true, text: 'First question?', replies: [], ...readQuestion, unread: true },
      { id: 'm2', workspace: 'ws1', manager: 'boss', at, step: 'delivered', text: 'My message', replies: [
        { by: 'boss', text: 'Already closed?', at, ...readQuestion, open: false },
        { by: 'boss', text: 'Second question?', at, ...readQuestion },
      ] },
    ],
  }
  vi.mocked(watchWorkspace).mockImplementation(onData => { onData(snapshot); return () => {} })
  const received: unknown[] = []
  vi.stubGlobal('fetch', async (url: string, init: RequestInit) => {
    if (url !== '/api/message') throw new Error(`Unexpected fetch: ${url}`)
    received.push(JSON.parse(String(init.body)))
    return Response.json({})
  })
  localStorage.setItem('workriver.side', 'talk')
  render()
  await act(async () => { mainButton('Manager').click() })
  expect(sheet()?.getAttribute('data-state')).toBe('open')
  expect(host.querySelector('aside section[aria-labelledby="side-talk"]')).not.toBeNull()
  expect(sheet()?.querySelector('section[aria-label="Needs you"]')).toBeNull()
  const answer = async (question: string) => {
    const bubble = [...sheet()!.querySelectorAll('.msg')].find(b => b.textContent?.includes(question))!
    act(() => bubble.querySelector<HTMLButtonElement>('[role=radio][value=yes]')!.click())
    await act(async () => { button(bubble, 'Answer').click() })
  }
  await answer('Second question?')
  await answer('First question?')
  expect(received).toEqual([
    { workspace: 'ws1', text: '', answer: { message: 'm2', reply: 2, selected: ['yes'] } },
    { workspace: 'ws1', text: '', answer: { message: 'm1', selected: ['yes'] } },
  ])
})

const task = { agent: null, since: at, writing: false, inputs: [], input: { text: '', causes: [] }, output: {}, plans: [] }
const gate: Task = { ...task, id: '2', taskId: '2', kind: 'gate', gate: 'test', state: 'waiting', title: 'Try the login' }
const stalled: Task = { ...task, id: '3', taskId: '3', kind: 'implement', state: 'running', title: 'Implement', agent: 'impl', stalled: { since: at, escalated: true } }
const held: Workspace = { ...ws, intents: [
  { ...ws.intents[0], units: [{ id: 'u1', title: 'Login', purpose: '', repo: '', branch: null, kind: 'research', status: 'active', commits: [], tasks: [gate, stalled] }] },
  ws.intents[1],
] }
const stuck = { agent: 'ja-impl', session: 's1', machine: 'box', workspace: 'ws1', intent: 'i1', topic: 'Japanese copy', blockedSince: at, ended: false } as const

function listen(initial: Workspace) {
  let emit: (data: Workspace) => void
  vi.mocked(watchWorkspace).mockImplementation(onData => { emit = onData; onData(initial); return () => {} })
  return (next: Workspace) => act(() => emit(next))
}

it('shows the recorded board while keeping a commit link open for the Git update', async () => {
  const url = '/u1/commit%3Aabc'
  history.replaceState(null, '', url)
  localStorage.setItem('workriver.side', 'commits')
  const unit = { ...held.intents[0].units[0], kind: undefined, repo: '/project', branch: 'fix/login' }
  const pending = {
    ...held,
    gitPending: true,
    repos: [{ path: '/project', name: 'Project', modules: { name: 'Project', path: '', files: 0, loc: 0, children: [] } }],
    intents: [{ ...held.intents[0], units: [unit] }],
  }
  const push = listen(pending)
  const fetch = vi.fn(async () => Response.json({ code: 'after-git', commits: [] }))
  vi.stubGlobal('fetch', fetch)
  await act(async () => { render() })
  expect(heading()).toContain('Login')
  expect(host.textContent).toContain('manual step an agent is waiting on')
  expect(host.textContent).not.toContain('No commits in this project yet.')
  expect(location.pathname).toBe(url)
  expect(fetch).not.toHaveBeenCalled()

  const commit = { id: 'abc', subject: 'Restore login', author: 'impl', date: at, live: false, files: [] }
  const gap: Task = { ...task, id: 'commit:abc', taskId: null, kind: 'implement', state: 'unregistered', title: commit.subject, output: { commits: [commit.id] } }
  await act(async () => push({
    ...pending,
    gitPending: false,
    intents: [{ ...pending.intents[0], units: [{ ...unit, code: 'after-git', tasks: [...unit.tasks, gap], commits: [commit] }] }],
  }))
  expect(host.textContent).toContain('The commit names no task, so its input is unknown.')
  expect(location.pathname).toBe(url)
  expect(fetch).toHaveBeenCalledWith('/api/unit/u1/code')
})

it('shows only agents stuck in their terminals in the Manager bar and hides it when they resume', () => {
  const push = listen({ ...held, sessions: [stuck] })
  localStorage.setItem('workriver.side', 'talk')
  render()
  const bar = () => host.querySelector('aside section[aria-label="Needs you"]')
  expect(bar()?.querySelector('h3')?.textContent).toBe('Needs you1')
  expect([...bar()!.querySelectorAll('li button .title')].map(r => r.textContent)).toEqual(['ja-impl is waiting for you in its terminal'])
  expect(document.title).toBe('(3) workriver')

  push(held)
  expect(bar()).toBeNull()
  expect(document.title).toBe('(2) workriver')
})

it('notifies once per new terminal wait and opens that agent’s terminal answer gate', async () => {
  const notices: { title: string; options: NotificationOptions; onclick?: () => void; close: () => void }[] = []
  vi.stubGlobal('Notification', class {
    static permission = 'granted'
    onclick?: () => void
    close = vi.fn()
    constructor(public title: string, public options: NotificationOptions) { notices.push(this) }
  })
  localStorage.setItem('workriver.notify', '1')
  const waiting = { ...held, sessions: [stuck] }
  const push = listen({ ...ws, sessions: [stuck] })
  render()
  expect(notices).toEqual([])

  push(waiting)
  expect(notices).toEqual([])
  push(held)
  push(waiting)
  push({ ...held, sessions: [{ ...stuck, topic: 'Updated topic' }] })
  expect(notices.map(n => [n.title, n.options])).toEqual([
    ['ja-impl is waiting for you', { body: 'Japanese copy', tag: 'workriver:agent:s1:box', requireInteraction: true }],
  ])
  push(held)
  push(waiting)
  expect(notices).toHaveLength(2)

  const fetch = vi.fn(async () => Response.json({ text: 'Continue?', cols: 40, rows: 1, digest: 'd1', dialog: null }))
  vi.stubGlobal('fetch', fetch)
  await act(async () => { notices[1].onclick!() })
  expect(fetch).toHaveBeenCalledWith('/api/agent/screen?agent=s1&machine=box', { cache: 'no-store' })
  expect(notices[1].close).toHaveBeenCalledOnce()
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
