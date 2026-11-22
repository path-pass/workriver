// Starting a manager from the page: what is offered and why the rest is not, the default from
// Settings, the ask before starting over a manager that runs, the service's refusals; and the
// workspace's line while the start is pending, waiting for the user in its terminal, or ended.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import { AgentContext } from '@/agents/context'
import type { AgentRef, ManagerStart, Session, Space, StartSettings } from '@/agents/types'
import type { Workspace } from '@/workspace/types'

const api = vi.hoisted(() => ({
  acts: [] as unknown[],
  answer: (): Promise<void> => Promise.resolve(),
  settings: (): Promise<unknown> => Promise.resolve(null),
}))
vi.mock('@/api/actions', async original => ({
  ...(await original<typeof import('@/api/actions')>()),
  setSpace: (a: unknown) => { api.acts.push(a); return api.answer() },
  startSettings: () => api.settings(),
}))
const { SeatChanged } = await import('@/api/actions')
const { StartLine, StartPart, modelOk } = await import('./StartManager')
const { setRuntime } = await import('@/i18n')

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

const both = (over: Partial<StartSettings> = {}): StartSettings => ({
  harness: 'claude', model: 'opus',
  harnesses: [{ kind: 'claude', aliases: ['opus', 'sonnet', 'fable'] }, { kind: 'codex', aliases: [], why: 'not_integrated' }],
  ...over,
})
const space = (over: Partial<Space> = {}): Space => ({ id: 'ws1', name: 'chat', project: '/web', ...over })

let root: Root
let host: HTMLDivElement
beforeEach(() => {
  host = document.createElement('div'); document.body.append(host); root = createRoot(host)
  api.acts = []; api.answer = () => Promise.resolve(); api.settings = () => Promise.resolve(both())
  setRuntime('Runner')
})
afterEach(() => { act(() => root.unmount()); host.remove() })

const settle = () => act(async () => { await new Promise(r => setTimeout(r)) })
const show = async (s: Space) => { act(() => root.render(<TooltipProvider><StartPart space={s} /></TooltipProvider>)); await settle() }
const button = (name: string) => [...host.querySelectorAll('button')].find(b => b.textContent === name) as HTMLButtonElement
const press = async (name: string) => { await act(async () => { button(name).click() }); await settle() }
const model = () => host.querySelector<HTMLInputElement>('input[aria-label=Model]')!
function type(el: HTMLInputElement, value: string) {
  const set = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), 'value')!.set!
  act(() => { set.call(el, value); el.dispatchEvent(new Event('input', { bubbles: true })) })
}
/** Radix opens the select on a pointer going down on its trigger; its options are portalled. */
const options = async () => {
  const trigger = host.querySelector<HTMLButtonElement>('[role=combobox]')!
  await act(async () => { trigger.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerType: 'mouse' })) })
  await settle()
  return [...document.querySelectorAll('[role=option]')].map(o => [o.textContent, o.hasAttribute('data-disabled')])
}

describe('starting a manager', () => {
  it('offers the harnesses this machine can start, the others greyed with why, and the default from Settings', async () => {
    await show(space())
    expect(host.querySelector('[role=combobox]')!.textContent).toBe('Claude Code')
    expect(model().value).toBe('opus')
    expect(['opus', 'sonnet', 'fable'].map(a => button(a)?.getAttribute('aria-pressed'))).toEqual(['true', 'false', 'false'])
    expect(await options()).toEqual([['Claude Code', false], ['Codex: Runner’s hook is not installed or not current', true]])
  })

  it('takes the first harness offered when the default is not, and says when none is', async () => {
    api.settings = () => Promise.resolve(both({ harness: 'codex', model: 'gpt-x', harnesses: [{ kind: 'claude', aliases: [] }, { kind: 'codex', aliases: [], why: 'not_installed' }] }))
    await show(space())
    expect(host.querySelector('[role=combobox]')!.textContent).toBe('Claude Code')
    expect(model().value).toBe('')
    act(() => root.unmount()); root = createRoot(host)
    api.settings = () => Promise.resolve(both({ harnesses: [{ kind: 'claude', aliases: [], why: 'runtime' }, { kind: 'codex', aliases: [], why: 'not_installed' }] }))
    await show(space())
    expect(host.textContent).toContain('No harness can be started here.')
    expect(button('Start and seat').disabled).toBe(true)
  })

  it('starts the harness and model picked, naming the empty seat it replaces', async () => {
    await show(space())
    await press('sonnet')
    await press('Start and seat')
    expect(api.acts).toEqual([{ action: 'start', workspace: 'ws1', harness: 'claude', model: 'sonnet', replaces: null }])
  })

  it('takes an empty model as the user’s default, and refuses one that is not one word', async () => {
    await show(space())
    type(model(), '--dangerously-skip')
    expect(host.querySelector('[role=alert]')?.textContent).toContain('A model is one word')
    expect(button('Start and seat').disabled).toBe(true)
    type(model(), '')
    await press('Start and seat')
    expect(api.acts).toEqual([{ action: 'start', workspace: 'ws1', harness: 'claude', model: '', replaces: null }])
    expect(['', 'opus', 'claude-opus-5-5[1m]', 'gpt-6.2-sol', 'a/b@c:d_e'].every(modelOk)).toBe(true)
    expect(['-m', 'opus sonnet', 'a;b', '$(x)', 'x'.repeat(101), 'é'].some(modelOk)).toBe(false)
  })

  it('asks once before starting over a manager that runs, and names the seat shown', async () => {
    await show(space({ manager: { session: 's-boss', name: 'boss', folder: '/web', since: 7, state: 'idle' } }))
    await press('Start and seat')
    expect(api.acts).toEqual([])
    expect(host.querySelector('[role=alert]')?.textContent).toBe('boss runs and is the manager now. Start a new one to take its seat once it runs? The one replaced is then told it no longer manages this workspace.')
    await press('Yes, start and seat')
    expect(api.acts).toEqual([{ action: 'start', workspace: 'ws1', harness: 'claude', model: 'opus', replaces: { session: 's-boss', since: 7 } }])
  })

  it('says a seat changed meanwhile, and any other refusal in the service’s words', async () => {
    await show(space())
    api.answer = () => Promise.reject(new SeatChanged('the seat changed: look again'))
    await press('Start and seat')
    expect(host.querySelector('[role=alert]')?.textContent).toBe('The seat changed meanwhile: it is shown above as it is now. Look again and choose.')
    api.answer = () => Promise.reject(new Error('a manager is being started for ws1: wait for it, or seat one yourself'))
    await press('Start and seat')
    expect(host.querySelector('[role=alert]')?.textContent).toBe('Not started: a manager is being started for ws1: wait for it, or seat one yourself')
  })

  it('waits while a start is pending here', async () => {
    await show(space({ start: { since: '2026-11-23T10:00:00Z', harness: 'claude', name: 'ws1-manager' } }))
    expect(host.textContent).toContain('A manager is being started here')
    expect(button('Start and seat').disabled).toBe(true)
  })
})

describe('the workspace’s line while it starts', () => {
  const start: ManagerStart = { since: '2026-11-23T10:00:00Z', harness: 'claude', name: 'ws1-manager' }
  const asked: AgentRef[] = []
  const line = (s: Space, sessions: Session[] = []) => act(() => root.render(
    <TooltipProvider>
      <AgentContext.Provider value={{ answer: a => asked.push(a), view: () => {} }}>
        <StartLine ws={{ sessions } as unknown as Workspace} space={s} />
      </AgentContext.Provider>
    </TooltipProvider>,
  ))

  it('says nothing without a start, and that one is starting while it is pending', () => {
    line(space())
    expect(host.textContent).toBe('')
    line(space({ start }))
    expect(host.textContent).toBe('starting a manager…')
  })

  it('opens the gate of its agent waiting in its terminal, found by the name it runs under', async () => {
    asked.length = 0
    const waiting: Session = { agent: 'ws1-manager', session: 'ws1-manager', ended: false, state: 'blocked', blockedSince: '2026-11-23T10:00:05Z' }
    line(space({ start }), [{ ...waiting, agent: 'other' }, { ...waiting, ended: true }, { ...waiting, machine: 'box' }])
    expect(host.textContent).toBe('starting a manager…')
    line(space({ start }), [waiting])
    expect(host.textContent).toBe('starting a manager… waiting for you in its terminal')
    await act(async () => { host.querySelector('button')!.click() })
    expect(asked).toEqual([{ name: 'ws1-manager', session: 'ws1-manager', machine: undefined }])
  })

  it('says why a start ended, with the runtime’s or the rules’ own words', () => {
    const said = (reason: ManagerStart['reason'], words?: string) => { line(space({ start: { ...start, reason, words } })); return host.textContent }
    expect(said('runtime', 'no such harness')).toBe('Manager not started: the runtime refused to start itno such harness')
    expect(said('no_agent')).toBe('Manager not started: nothing started in its terminal (is the harness installed?), or it left before it had a session')
    expect(said('no_session')).toBe('Manager not started: no session to seat within 10 minutes; its terminal is left open: seat it here once it runs')
    expect(said('refused', 'a session known before the start')).toBe('Manager not started: what it brought up could not be seateda session known before the start')
    expect(said('restarted')).toBe('Manager not started: workriver restarted while it was starting')
  })
})
