// Seating a manager: the change names the seat the user was shown; seating over a manager
// that runs is asked once; a seat that changed meanwhile says so and nothing is overwritten.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import type { Seat, Session, Space } from '@/agents/types'
import type { Workspace } from '@/workspace/types'

const sent = vi.hoisted(() => ({ acts: [] as unknown[], answer: (): Promise<void> => Promise.resolve() }))
vi.mock('@/api/actions', async original => {
  const real = await original<typeof import('@/api/actions')>()
  return { ...real, setSpace: (a: unknown) => { sent.acts.push(a); return sent.answer() } }
})
const { SeatChanged } = await import('@/api/actions')
const { SeatPicker } = await import('./Seat')

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

const seat = (name: string, state: Seat['state'] = 'idle'): Seat => ({ session: `s-${name}`, name, folder: '/web', since: 7, state })
const live = (agent: string, over: Partial<Session> = {}): Session => ({ agent, session: `s-${agent}`, ended: false, state: 'idle', folder: '/web', harness: 'claude', ...over })
const data = (space: Space, sessions: Session[]) => ({ intents: [], projects: [], workspaces: [space], sessions }) as unknown as Workspace

let root: Root
let host: HTMLDivElement
beforeEach(() => { host = document.createElement('div'); document.body.append(host); root = createRoot(host); sent.acts = []; sent.answer = () => Promise.resolve() })
afterEach(() => { act(() => root.unmount()); host.remove() })

const show = (space: Space, sessions: Session[], preset?: string) => act(() => root.render(<TooltipProvider><SeatPicker ws={data(space, sessions)} space={space} preset={preset} /></TooltipProvider>))
const button = (name: string) => [...host.querySelectorAll('button')].find(b => b.textContent === name) as HTMLButtonElement
const pick = (session: string) => act(async () => { (host.querySelector(`[data-slot=radio-group-item][value="${session}"]`) as HTMLButtonElement).click() })
const press = (name: string) => act(async () => { button(name).click(); await new Promise(r => setTimeout(r)) })

describe('the manager select', () => {
  it('lists the agents running now, each with its harness, folder, runtime group and what the records know', async () => {
    const space: Space = { id: 'ws2', name: 'chat', project: '/web' }
    await show(space, [live('boss', { group: { id: 'g1', label: 'group-row' }, folder: '/web/app' }), live('gone', { ended: true })])
    expect(host.textContent).toContain('No manager is seated.')
    const rows = [...host.querySelectorAll('label')].map(l => l.textContent)
    expect(rows).toHaveLength(1)
    expect(rows[0]).toContain('boss')
    expect(rows[0]).toContain('claude')
    expect(rows[0]).toContain('/web/app')
    expect(rows[0]).toContain('group-row')
    expect(rows[0]).toContain('no task, no seat')
    expect(button('Seat as manager').disabled).toBe(true)
  })

  it('seats the one picked, naming an empty seat as what it replaces', async () => {
    const space: Space = { id: 'ws2', name: 'chat', project: '/web' }
    await show(space, [live('boss'), live('other')])
    await pick('s-other')
    await press('Seat as manager')
    expect(sent.acts).toEqual([{ action: 'manager', workspace: 'ws2', manager: 's-other', replaces: null }])
  })

  it('asks once before seating over a manager that runs, and names the seat shown as replaced', async () => {
    const space: Space = { id: 'ws1', name: 'chat', project: '/web', manager: seat('boss') }
    await show(space, [live('boss', { manager: true }), live('next')])
    await pick('s-next')
    await press('Seat as manager')
    expect(sent.acts).toEqual([])
    expect(host.querySelector('[role=alert]')?.textContent).toContain('boss runs and is the manager now. Seat next instead?')
    await press('Yes, seat next')
    expect(sent.acts).toEqual([{ action: 'manager', workspace: 'ws1', manager: 's-next', replaces: { session: 's-boss', since: 7 } }])
  })

  it('seats over a manager that does not run without asking', async () => {
    const space: Space = { id: 'ws1', name: 'chat', project: '/web', manager: seat('boss', 'gone') }
    await show(space, [live('next')], 's-next')
    await press('Seat as manager')
    expect(sent.acts).toEqual([{ action: 'manager', workspace: 'ws1', manager: 's-next', replaces: { session: 's-boss', since: 7 } }])
  })

  it('empties the seat, naming the seat shown', async () => {
    const space: Space = { id: 'ws1', name: 'chat', project: '/web', manager: seat('boss') }
    await show(space, [live('boss')])
    await press('Empty the seat')
    expect(sent.acts).toEqual([{ action: 'manager', workspace: 'ws1', manager: null, replaces: { session: 's-boss', since: 7 } }])
  })

  it('says a seat changed meanwhile, and what the service refused in its own words', async () => {
    const space: Space = { id: 'ws2', name: 'chat', project: '/web' }
    await show(space, [live('boss')], 's-boss')
    sent.answer = () => Promise.reject(new SeatChanged('the seat changed'))
    await press('Seat as manager')
    expect(host.querySelector('[role=alert]')?.textContent).toBe('The seat changed meanwhile: it is shown above as it is now. Look again and choose.')
    sent.answer = () => Promise.reject(new Error('boss is shown in several terminals at once'))
    await press('Seat as manager')
    expect(host.querySelector('[role=alert]')?.textContent).toBe('Not seated: boss is shown in several terminals at once')
  })

  it('says the seated session shown in another folder is a new terminal, and that confirming is a new appointment', async () => {
    const space: Space = { id: 'ws1', name: 'chat', project: '/web', manager: seat('boss', 'gone'), proposal: 's-boss' }
    await show(space, [live('boss', { folder: '/elsewhere' })])
    const note = host.querySelector('[role=note]')!.textContent!
    expect(note).toContain('A new terminal reports the session of boss')
    expect(note).toContain('Seated in: /web on This machine (gone)')
    expect(note).toContain('Shows now in: /elsewhere on This machine')
    await press('Seat it: a new appointment')
    expect(sent.acts).toEqual([{ action: 'manager', workspace: 'ws1', manager: 's-boss', replaces: { session: 's-boss', since: 7 } }])
  })
})
