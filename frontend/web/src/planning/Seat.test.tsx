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
  const settings = { harness: 'claude', model: '', harnesses: [{ kind: 'claude', aliases: ['opus'] }] }
  return { ...real, setSpace: (a: unknown) => { sent.acts.push(a); return sent.answer() }, startSettings: () => Promise.resolve(settings) }
})
const { SeatChanged } = await import('@/api/actions')
const { SeatDialog, SeatPicker, openSeat } = await import('./Seat')

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

const seat = (name: string, state: Seat['state'] = 'idle'): Seat => ({ session: `s-${name}`, name, folder: '/web', since: 7, state })
const live = (agent: string, over: Partial<Session> = {}): Session => ({ agent, session: `s-${agent}`, ended: false, state: 'idle', folder: '/web', harness: 'claude', ...over })
const data = (space: Space, sessions: Session[]) => ({ intents: [], projects: [{ id: '/web', key: 'p1', title: 'Web', repos: [] }], workspaces: [space], sessions }) as unknown as Workspace

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
    const rows = [...host.querySelectorAll('[role=radiogroup] label')].map(l => l.textContent)
    expect(rows).toHaveLength(1)
    expect(rows[0]).toContain('boss')
    expect(rows[0]).toContain('claude')
    expect(rows[0]).toContain('Web/app')
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
  // A grid's implicit column is sized by its items' min-content: one long folder (no space
  // to break at) widened the column, and every row, past the dialog's edge. Measured in a
  // browser; here the cause is held: the dialog and the list each have one column that may
  // shrink below its content, and the long folder is cut with the whole of it in a tooltip.
  it('keeps every row inside the dialog: one column that shrinks, the folder cut with the whole in a tooltip', async () => {
    const folder = '/Users/someone/code/a-folder-name-long-enough-to-be-wider-than-the-dialog/and/deeper'
    const space: Space = { id: 'ws2', name: 'chat', project: '/web' }
    const ws = data(space, [live('boss', { folder })])
    await act(() => root.render(<TooltipProvider><SeatDialog ws={ws} /></TooltipProvider>))
    await act(async () => openSeat('ws2'))
    const dialog = document.querySelector('[data-slot=dialog-content]')!
    expect(dialog.classList).toContain('grid-cols-1')
    expect(dialog.querySelector('[role=radiogroup]')!.classList).toContain('grid-cols-1')
    const path = dialog.querySelector(`label span[title="${folder}"]`)!
    expect(path.textContent).toBe('~/code/a-folder-name-long-enough-to-be-wider-than-the-dialog/and/deeper')
    expect(path.classList).toContain('truncate')
  })

  const names = () => [...host.querySelectorAll('[role=radiogroup] label')].filter(l => l.querySelector('[role=radio]')).map(l => l.querySelector('.font-\\[550\\]')!.textContent)
  const allSwitch = () => host.querySelector('[role=switch]') as HTMLButtonElement | null

  it('lists the agents in the project’s folder first; a quiet switch shows all of them', async () => {
    const space: Space = { id: 'ws2', name: 'chat', project: '/web' }
    await show(space, [live('here', { folder: '/web/app' }), live('away', { folder: '/Users/someone/notes' }), live('there', { folder: '/web', machine: 'box' })])
    expect(names()).toEqual(['here'])
    expect(allSwitch()!.getAttribute('aria-checked')).toBe('false')
    expect(allSwitch()!.closest('label')!.textContent).toBe('Show all agents (3)')
    await act(async () => allSwitch()!.click())
    expect(names()).toEqual(['here', 'away', 'there'])
    // The others come under the switch, where it was turned on.
    const order = [...host.querySelectorAll('[role=radiogroup] [role=radio], [role=radiogroup] [role=switch]')].map(e => e.getAttribute('role'))
    expect(order).toEqual(['radio', 'switch', 'radio', 'radio'])
    await pick('s-away')
    await act(async () => allSwitch()!.click())
    // An agent picked among the others is not seated unseen once they are hidden again.
    expect(names()).toEqual(['here'])
    expect(button('Seat as manager').disabled).toBe(true)
  })

  it('keeps the agent seated now and the one preset in the list wherever they run', async () => {
    const space: Space = { id: 'ws1', name: 'chat', project: '/web', manager: seat('boss') }
    await show(space, [live('boss', { folder: '/elsewhere', manager: true }), live('next', { folder: '/notes' }), live('other', { folder: '/opt' })], 's-next')
    expect(names()).toEqual(['boss', 'next'])
    expect(allSwitch()!.closest('label')!.textContent).toBe('Show all agents (3)')
  })

  it('says when no agent runs in the project’s folder, and shows all of them', async () => {
    const space: Space = { id: 'ws2', name: 'chat', project: '/web' }
    await show(space, [live('away', { folder: '/Users/someone/notes' })])
    expect(host.textContent).toContain('No agent runs in this project’s folder; all agents running now are listed.')
    expect(names()).toEqual(['away'])
    // The switch shows open, and stays so: hiding the others would leave nothing to pick.
    expect(allSwitch()!.getAttribute('aria-checked')).toBe('true')
    expect(allSwitch()!.disabled).toBe(true)
  })

  it('shows each folder short, the whole path in its tooltip', async () => {
    const space: Space = { id: 'ws2', name: 'chat', project: '/web' }
    await show(space, [live('root', { folder: '/web' }), live('sub', { folder: '/web/crates/core' })])
    const folders = [...host.querySelectorAll('label span[title]')].map(e => [e.textContent, e.getAttribute('title')])
    expect(folders).toEqual([['Web', '/web'], ['Web/crates/core', '/web/crates/core']])
  })

  it('has a second part that starts a new one, and says a start is pending', async () => {
    const space: Space = { id: 'ws2', name: 'chat', project: '/web', start: { since: '2026-11-23T10:00:00Z', harness: 'claude', name: 'ws2-manager' } }
    await show(space, [live('boss')])
    await act(async () => { await new Promise(r => setTimeout(r)) })
    expect(host.querySelector('section h3')?.textContent).toBe('Start a new one')
    expect(host.querySelector('[role=status]')?.textContent).toBe('starting a manager…')
    expect(button('Start and seat').disabled).toBe(true)
  })
})
