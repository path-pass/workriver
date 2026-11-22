// The rail's four levels (project → workspace → intent → unit), who can be seated, and
// which workspaces wait for a manager.
import { describe, expect, it } from 'vitest'
import type { Seat, Session, Space } from '@/agents/types'
import type { Intent, Project, Unit } from './types'
import type { Workspace } from '@/workspace/types'
import { candidates, movedSeat, railOf, replaces, seatList, shortFolder, unmanaged } from './spaces'
import { firstPlace, placesOf } from './Start'

const seat = (name: string, state: Seat['state'] = 'idle', folder = '/web'): Seat => ({ session: `s-${name}`, name, folder, since: 7, state })
const space = (id: string, name: string, project: string, manager?: Seat, closed = false): Space => ({ id, name, project, manager, closed })
const unit = (id: string) => ({ id, title: id, tasks: [] }) as unknown as Unit
const intent = (id: string, state: Intent['state'], project: string, workspace?: string, order?: number) => ({ id, title: id, state, project, workspace, order, units: [unit(`${id}-u`)], chores: [] }) as unknown as Intent
const projects: Project[] = [{ id: '/web', key: 'p1', title: 'Web', repos: [] }, { id: '/notes', key: 'p2', title: 'Notes', repos: [] }]
// Two workspaces of one name in two projects; one closed; one with nothing in it.
const spaces = [space('ws1', 'chat', '/web', seat('boss')), space('ws2', 'big', '/web'), space('ws3', 'chat', '/notes', seat('n')), space('ws4', 'old', '/web', undefined, true), space('ws5', 'spare', '/web')]
const intents = [
  intent('i1', 'active', '/web', 'ws1'), intent('i2', 'active', '/web', 'ws2'), intent('i3', 'active', '/notes', 'ws3'),
  intent('i4', 'planned', '/web', undefined, 1), intent('i5', 'planned', '/web', undefined, 0), intent('i6', 'done', '/web', 'ws4'),
  intent('i7', 'planned', '', undefined, 0), intent('i8', 'active', '/web', 'ws4'), intent('i9', 'planned', '/gone', undefined, 1),
]
const groups = intents.filter(i => i.state !== 'planned').map(i => ({ intent: i, units: i.units }))
const planned = intents.filter(i => i.state === 'planned').sort((a, b) => (a.order ?? 0) - (b.order ?? 0))
const shape = (rail: ReturnType<typeof railOf>) => rail.map(s => ({
  project: s.project?.title ?? null,
  spaces: s.spaces.map(x => [x.space.id, ...x.intents.map(g => g.intent.id)]),
  loose: s.loose.map(g => g.intent.id), planned: s.planned.map(i => i.id), done: s.done.map(g => g.intent.id),
}))

describe('the rail', () => {
  it('is project → workspace → intent: each open workspace with its active intents, then the project’s roadmap and done; "No project" last', () => {
    expect(shape(railOf(projects, spaces, groups, planned, null))).toEqual([
      // A closed workspace is not in the rail: what is still active in it shows under no workspace.
      { project: 'Web', spaces: [['ws1', 'i1'], ['ws2', 'i2'], ['ws5']], loose: ['i8'], planned: ['i5', 'i4'], done: ['i6'] },
      { project: 'Notes', spaces: [['ws3', 'i3']], loose: [], planned: [], done: [] },
      // Planned intents of no project, and of a folder no registered project holds.
      { project: null, spaces: [], loose: [], planned: ['i7', 'i9'], done: [] },
    ])
  })

  it('keeps two workspaces of one name apart: each is its id', () => {
    const rail = railOf(projects, spaces, groups, planned, null)
    const chats = rail.flatMap(s => s.spaces).filter(x => x.space.name === 'chat')
    expect(chats.map(x => [x.space.id, x.intents.map(g => g.intent.id)])).toEqual([['ws1', ['i1']], ['ws3', ['i3']]])
  })

  it('shows one project without "No project"; a search leaves out workspaces with no match; "No project" hides when empty', () => {
    expect(shape(railOf(projects, spaces, groups, planned, '/notes'))).toEqual([{ project: 'Notes', spaces: [['ws3', 'i3']], loose: [], planned: [], done: [] }])
    const found = railOf(projects, spaces, groups.filter(g => g.intent.id === 'i2'), [], null, true)
    expect(shape(found)).toEqual([
      { project: 'Web', spaces: [['ws2', 'i2']], loose: [], planned: [], done: [] },
      { project: 'Notes', spaces: [], loose: [], planned: [], done: [] },
    ])
  })
})

const live = (agent: string, over: Partial<Session> = {}): Session => ({ agent, session: `s-${agent}`, ended: false, state: 'idle', folder: '/web', ...over })
const ws = { intents: [{ ...intents[0], units: [{ id: 'u1', title: 'u1', tasks: [{ id: 't3', taskId: '3', state: 'running', agent: 'worker', agentSession: 's-worker' }] }] }, ...intents.slice(1)], workspaces: spaces, projects, sessions: [live('worker'), live('boss', { manager: true }), live('free'), live('why-q7'), live('gone', { ended: true })] } as unknown as Workspace

describe('who can be seated', () => {
  it('is every agent running now but a question’s own, with what the records know: its seat, its tasks', () => {
    expect(candidates(ws).map(c => [c.key, c.seat?.id, c.tasks.map(t => `${t.unit}#${t.id}`)])).toEqual([
      ['s-boss', 'ws1', []], ['s-free', undefined, []], ['s-worker', undefined, ['u1#3']],
    ])
  })

  it('names the seat the user was shown as the one a change replaces', () => {
    expect(replaces(spaces[0])).toEqual({ session: 's-boss', since: 7 })
    expect(replaces(spaces[1])).toBeNull()
  })

  it('says a seated session shown in another folder is a new terminal: a new appointment', () => {
    const [boss] = candidates(ws)
    expect(movedSeat(spaces[0], boss)).toBe(false)
    expect(movedSeat(spaces[0], { ...boss, s: { ...boss.s, folder: '/elsewhere' } })).toBe(true)
    expect(movedSeat(spaces[0], { ...boss, s: { ...boss.s, machine: 'box' } })).toBe(true)
    expect(movedSeat(spaces[0], candidates(ws)[1])).toBe(false)
  })

  it('lists the project’s agents first: started in its folder or inside it, on this machine; the seated one and the preset kept wherever they are', () => {
    const near = { ...ws, sessions: [
      live('root'), live('sub', { folder: '/web/app' }), live('twin', { folder: '/webby' }), live('far', { folder: '/notes' }),
      live('remote', { machine: 'box' }), live('nowhere', { folder: undefined }), live('boss', { folder: '/notes' }), live('picked', { folder: '/elsewhere' }),
    ] } as unknown as Workspace
    const keys = (l: ReturnType<typeof candidates>) => l.map(c => c.key)
    const { here, rest } = seatList(candidates(near), '/web/', ['s-boss', 's-picked'])
    expect(keys(here)).toEqual(['s-boss', 's-picked', 's-root', 's-sub'])
    expect(keys(rest)).toEqual(['s-far', 's-nowhere', 's-remote', 's-twin'])
  })

  it('shows a folder short: in a registered project as its name and the path under it, else under home as ~, the whole path otherwise', () => {
    const nested: Project[] = [...projects, { id: '/web/packages/ui', title: 'UI', repos: [] }]
    expect(shortFolder('/web', undefined, nested)).toBe('Web')
    expect(shortFolder('/web/app/src', undefined, nested)).toBe('Web/app/src')
    expect(shortFolder('/web/packages/ui/lib', undefined, nested)).toBe('UI/lib')
    expect(shortFolder('/webby', undefined, nested)).toBe('/webby')
    expect(shortFolder('/Users/someone/code/x', undefined, nested)).toBe('~/code/x')
    expect(shortFolder('/home/someone', undefined, nested)).toBe('~')
    // A project's folder is on this machine: the same path elsewhere is not it.
    expect(shortFolder('/web/app', 'box', nested)).toBe('/web/app')
    expect(shortFolder('/home/someone/web', 'box', nested)).toBe('~/web')
    expect(shortFolder('/opt/x', undefined, nested)).toBe('/opt/x')
  })
})

describe('workspaces that wait for a manager', () => {
  it('are the open ones with an active intent and no manager running', () => {
    const gone = spaces.map(w => (w.id === 'ws3' ? { ...w, manager: seat('n', 'gone') } : w))
    expect(unmanaged({ ...ws, workspaces: gone }).map(w => w.id)).toEqual(['ws2', 'ws3'])
  })
})

describe('where an intent starts', () => {
  it('offers the open workspaces of its project; one of no project, any', () => {
    expect(placesOf(ws, '/web').map(w => w.id)).toEqual(['ws1', 'ws2', 'ws5'])
    expect(placesOf(ws, '').map(w => w.id)).toEqual(['ws1', 'ws2', 'ws3', 'ws5'])
  })

  it('preselects the one last started in, else one whose manager runs, else the first', () => {
    const places = placesOf(ws, '/web')
    expect(firstPlace(places, 'ws2')?.id).toBe('ws2')
    expect(firstPlace(places, 'ws9')?.id).toBe('ws1')
    expect(firstPlace(places.slice(1), null)?.id).toBe('ws2')
    expect(firstPlace([], null)).toBeUndefined()
  })
})
