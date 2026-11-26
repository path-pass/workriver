import { act, StrictMode } from 'react'
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

it('reports the first board paint once, even when the workspace refreshes', async () => {
  const { recordWorkspaceReady } = await import('@/workspace/first-load')
  const pending = { ...ws, gitPending: true }
  const refreshed = { ...ws }
  let update: (snapshot: Workspace) => void = () => {}
  vi.mocked(watchWorkspace).mockImplementation(onData => { update = onData; return () => {} })
  const send = vi.fn(async (_url: string, _init: RequestInit) => new Response(null, { status: 401 }))
  vi.stubGlobal('fetch', (url: string, init: RequestInit) => {
    if (url === '/api/perf/page-load') return send(url, init)
    if (url === '/api/task?ref=still-pending') return new Promise<Response>(() => {})
    return Promise.resolve(new Response())
  })
  const signedOut = vi.fn()
  window.addEventListener('workriver:signed-out', signedOut, { once: true })
  const frames = new Map<number, FrameRequestCallback>()
  let next = 0
  vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => { frames.set(++next, cb); return next })
  vi.stubGlobal('cancelAnimationFrame', (id: number) => frames.delete(id))
  const frame = (at: number) => act(() => {
    const scheduled = [...frames.values()]
    frames.clear()
    for (const cb of scheduled) cb(at)
  })
  const now = vi.spyOn(performance, 'now').mockReturnValue(10)
  document.head.insertAdjacentHTML('beforeend', '<script type="module" src="/assets/index-build.js"></script>')
  const entry = document.head.lastElementChild as HTMLScriptElement
  const workspaceUrl = new URL('/api/workspace', location.href).href
  const assetUrl = entry.src
  vi.spyOn(performance, 'getEntriesByName').mockImplementation((name) => {
    if (name === assetUrl) return [{ responseEnd: 33.2 } as PerformanceResourceTiming]
    if (name === workspaceUrl) return [
      { startTime: 20, requestStart: 21, responseEnd: 29, encodedBodySize: 123 } as PerformanceResourceTiming,
      { startTime: 51, requestStart: 54.1, responseEnd: 75.8, encodedBodySize: 456 } as PerformanceResourceTiming,
    ]
    return []
  })
  const resource = (path: string, initiatorType: string, startTime: number, responseEnd: number) => ({
    name: new URL(path, location.href).href, initiatorType, startTime, responseEnd,
  }) as PerformanceResourceTiming
  vi.spyOn(performance, 'getEntriesByType').mockImplementation(type => type === 'resource' ? [
    resource('/api/task?ref=private', 'fetch', 10, 40),
    resource('/api/task?ref=another', 'fetch', 45, 55),
    resource('/api/unit/private-id/code?have=secret', 'fetch', 110, 125),
    resource('/api/attachments/private-id', 'img', 101, 105),
    resource('/api/task', 'fetch', 120, 125),
    resource('https://other.example/api/task', 'fetch', 10, 40),
    resource('/api/workspace', 'fetch', 51, 75.8),
    resource('/api/events', 'eventsource', 60, 70),
    resource('/api/agent/view?agent=private', 'eventsource', 60, 70),
    resource('/api/task', 'xmlhttprequest', 10, 40),
    resource('/api/unknown/private', 'fetch', 10, 40),
  ] : [])
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
  try {
    act(() => root.render(<StrictMode><TooltipProvider><App /></TooltipProvider></StrictMode>))
    expect(send).not.toHaveBeenCalled()
    void fetch('/api/task?ref=still-pending')
    now.mockReturnValue(90)
    recordWorkspaceReady(pending, 50)
    now.mockReturnValue(100)
    act(() => update(pending))
    expect(heading()).toContain('First intent')
    now.mockReturnValue(110)
    frame(109)
    expect(send).not.toHaveBeenCalled()
    recordWorkspaceReady(refreshed, 100)
    act(() => update(refreshed))
    now.mockReturnValue(120)
    act(() => vi.advanceTimersByTime(0))
    frame(150)
    await Promise.resolve()
    expect(send).toHaveBeenCalledTimes(1)
    const [url, init] = send.mock.calls[0]!
    expect(url).toBe('/api/perf/page-load')
    expect(JSON.parse(init.body as string)).toEqual({
      ms: 120, js_ms: 33, request_ms: 54, response_ms: 76, bytes: 456,
      ready_ms: 90, parse_ms: 14, app_ms: 10, render_ms: 20,
      git_pending: 1,
      api: [
        { path: '/api/attachments/{id}', count: 1, ms: 4, max_ms: 4 },
        { path: '/api/task', count: 2, ms: 40, max_ms: 30 },
        { path: '/api/unit/{id}/code', count: 1, ms: 10, max_ms: 10 },
      ],
    })
    expect(signedOut).not.toHaveBeenCalled()
  } finally {
    vi.useRealTimers()
    window.removeEventListener('workriver:signed-out', signedOut)
    entry.remove()
  }
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

it('keeps the Manager draft through navigation, section switches and the mobile sheet', async () => {
  history.replaceState(null, '', '/u1/3')
  const narrow = vi.spyOn(window, 'matchMedia').mockReturnValue({ matches: false } as MediaQueryList)
  const manager = { name: 'boss', session: 's1', folder: '/project', since: 1, state: 'idle' as const }
  listen({
    ...held,
    messages: [],
    intents: [{ ...held.intents[0], state: 'active' }, ...held.intents.slice(1)],
    workspaces: [{ ...ws.workspaces![0], manager }],
  })
  const received: unknown[] = []
  vi.stubGlobal('fetch', async (url: string, init: RequestInit) => {
    if (url.startsWith('blob:')) return new Response('evidence')
    if (url === '/api/attachments?name=draft.txt') return Response.json({ id: 'draft-file' })
    if (url !== '/api/message') throw new Error(`Unexpected fetch: ${url}`)
    received.push(JSON.parse(String(init.body)))
    return Response.json({})
  })
  const panel = () => host.querySelector('aside:has(button[aria-label="Manager"])')!
  const box = (where: ParentNode) => where.querySelector<HTMLTextAreaElement>('textarea[aria-label="Message to the manager"]')!

  await act(async () => { render() })
  act(() => mainButton('Ask the manager').click())
  type(box(panel()), 'Can you check this?')
  const input = panel().querySelector('input[type="file"]')!
  Object.defineProperty(input, 'files', { value: [new File(['evidence'], 'draft.txt')] })
  act(() => { input.dispatchEvent(new Event('change', { bubbles: true })) })
  act(() => button(panel(), 'Leave #3 out').click())
  act(() => button(panel(), 'Agents').click())
  expect(box(panel())).toBeNull()
  act(() => button(panel(), 'Manager').click())
  expect(box(panel()).value).toBe('Can you check this?')
  expect(panel().textContent).toContain('draft.txt')
  expect(panel().querySelector('[aria-label="Leave #3 out"]')).toBeNull()

  await act(async () => {
    history.pushState(null, '', '/u1/2')
    window.dispatchEvent(new PopStateEvent('popstate'))
  })
  narrow.mockReturnValue({ matches: true } as MediaQueryList)
  await act(async () => { mainButton('Ask the manager').click() })
  expect(box(sheet()!).value).toBe('Can you check this?')
  type(box(sheet()!), 'Can you rerun this check?')
  await act(async () => { button(sheet()!, 'Fold the panel').click() })
  expect(sheet()).toBeNull()
  await act(async () => { mainButton('Ask the manager').click() })
  expect(box(sheet()!).value).toBe('Can you rerun this check?')
  expect(sheet()?.textContent).toContain('draft.txt')
  await act(async () => { button(sheet()!, 'Send').click() })
  expect(received).toEqual([{
    workspace: 'ws1', intent: 'i1', unit: 'u1', task: 2, text: 'Can you rerun this check?',
    context: [{ kind: 'attachment', id: 'draft-file', label: 'draft.txt' }],
  }])
  expect(box(sheet()!).value).toBe('')
  await act(async () => { button(sheet()!, 'Fold the panel').click() })
  expect(box(panel()).value).toBe('')
  expect(panel().textContent).not.toContain('draft.txt')
})

function listen(initial: Workspace) {
  let emit: (data: Workspace) => void
  vi.mocked(watchWorkspace).mockImplementation(onData => { emit = onData; onData(initial); return () => {} })
  return (next: Workspace) => act(() => emit(next))
}

const visit = (path: string) => act(async () => {
  history.pushState(null, '', path)
  window.dispatchEvent(new PopStateEvent('popstate'))
})

it('sends the restored gate note after visiting another unit', async () => {
  const unit = held.intents[0].units[0]
  const snapshot = { ...held, intents: [{ ...held.intents[0], units: [unit, { ...unit, id: 'u2', title: 'Another unit', tasks: [gate] }] }] }
  listen(snapshot)
  const received: unknown[] = []
  vi.stubGlobal('fetch', async (url: string, init: RequestInit) => {
    if (url !== '/api/gate') throw new Error(`Unexpected fetch: ${url}`)
    received.push(JSON.parse(String(init.body)))
    return Response.json({})
  })
  history.replaceState(null, '', '/u1/2')
  await act(async () => { render() })
  const note = () => host.querySelector<HTMLTextAreaElement>('main form textarea')
  type(note()!, 'Login works after refresh.')
  await visit('/u2/2')
  expect(note()?.value).toBe('')
  await visit('/u1/2')
  expect(note()?.value).toBe('Login works after refresh.')

  vi.useFakeTimers()
  try {
    act(() => mainButton('Submit').click())
    expect(note()).toBeNull()
    await act(async () => { await vi.advanceTimersByTimeAsync(3000) })
    expect(received).toEqual([{ unit: 'u1', task: 2, result: 'done', note: 'Login works after refresh.', findings: [], attachments: [], selected: [] }])
  } finally {
    vi.useRealTimers()
  }
})

it('releases a gate draft picture if the gate closes while another page is open', async () => {
  const push = listen(held)
  history.replaceState(null, '', '/u1/2')
  await act(async () => { render() })
  const revoke = vi.spyOn(URL, 'revokeObjectURL')
  const input = host.querySelector('main form input[type=file]')!
  Object.defineProperty(input, 'files', { value: [new File(['evidence'], 'gate.png', { type: 'image/png' })] })
  act(() => { input.dispatchEvent(new Event('change', { bubbles: true })) })
  const picture = host.querySelector<HTMLImageElement>('main form img')!.src

  await visit('/i2')
  expect(revoke).not.toHaveBeenCalledWith(picture)
  const intent = held.intents[0]
  const unit = { ...intent.units[0], tasks: [{ ...gate, state: 'dropped' as const }, stalled] }
  push({ ...held, intents: [{ ...intent, units: [unit] }, held.intents[1]] })
  expect(revoke).toHaveBeenCalledWith(picture)
})

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
