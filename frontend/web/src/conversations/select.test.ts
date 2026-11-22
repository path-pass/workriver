// What the user picked becomes the chips above the Manager box, and the chips the message's
// unit, task and context. Which workspace's conversation the Manager section shows, and where
// it goes when the rail switches project.
import { describe, expect, it } from 'vitest'
import { chipsOf, entryOf, EVERY, followProject, leaveOut, messageOf, placesOf, type Chip } from './select'
import type { Seat, Space } from '@/agents/types'
import type { Intent, Unit } from '@/planning/types'
import type { ManagerMessage, Talk } from './types'
import type { Task } from '@/work/types'

const unit = { id: 'u1', title: 'Search' } as unknown as Unit
const task = { id: '3', taskId: '3', title: 'Paging' } as unknown as Task
const lines = { path: 'src/api.ts', from: 10, to: 24 }
const extra = [{ kind: 'task' as const, label: '#5', task: 5 }]
const shot = { kind: 'attachment' as const, label: 'shot.png', id: 'a1' }
const picked = { unit, n: task, taskLabel: '#3', finding: 'P2-1', file: 'src/api.ts', lines, extra: [...extra, shot] }
const mine = [{ id: 'f9', name: 'log.txt', url: 'blob:x' }]
const keyOf = (c: Chip) => `${c.kind}:${c.label}`

describe('chips', () => {
  it('are the unit, the task, the finding, the lines over their file, the other tasks and the open attachment', () => {
    const chips = chipsOf(picked, true)
    expect(chips.map(keyOf)).toEqual(['unit:Search', 'task:#3', 'finding:P2-1', 'lines:api.ts L10–24', 'task:#5', 'attachment:shot.png'])
    expect(chipsOf({ ...picked, lines: null }, true).map(keyOf)).toContain('file:api.ts')
  })

  it('are none for another workspace than the unit’s', () => {
    expect(chipsOf(picked, false)).toEqual([])
    expect(chipsOf({ ...picked, unit: null, n: null }, true)).toEqual([])
  })

  it('without a task: the unit and what was picked in its code', () => {
    expect(chipsOf({ ...picked, n: null, finding: undefined, extra: [] }, true).map(keyOf)).toEqual(['unit:Search', 'lines:api.ts L10–24'])
  })
})

describe('the message', () => {
  it('names the unit and task, and carries the rest with the user’s files', () => {
    const body = messageOf(chipsOf(picked, true), mine)
    expect(body.unit).toBe('u1')
    expect(body.task).toBe(3)
    expect(body.context).toEqual([
      { kind: 'finding', label: 'P2-1' },
      { kind: 'lines', label: 'api.ts L10–24', path: 'src/api.ts', from: 10, to: 24 },
      { kind: 'task', label: '#5', task: 5 },
      shot,
      { kind: 'attachment', label: 'log.txt', id: 'f9' },
    ])
  })

  it('leaves out one chip; the task’s × takes what needs the task, the unit’s everything but the user’s files', () => {
    const chips = chipsOf(picked, true)
    const one = leaveOut(chips, ['finding:P2-1'])
    expect(one.map(keyOf)).not.toContain('finding:P2-1')
    expect(one).toHaveLength(chips.length - 1)
    // The finding and the attachment the task shows go with the task; the code and the
    // other tasks that wrote it stay with the unit.
    const noTask = leaveOut(chips, ['task:#3'])
    expect(noTask.map(keyOf)).toEqual(['unit:Search', 'lines:api.ts L10–24', 'task:#5'])
    expect(messageOf(noTask, mine)).toEqual({
      unit: 'u1',
      task: 0,
      context: [
        { kind: 'lines', label: 'api.ts L10–24', path: 'src/api.ts', from: 10, to: 24 },
        { kind: 'task', label: '#5', task: 5 },
        { kind: 'attachment', label: 'log.txt', id: 'f9' },
      ],
    })
    const noUnit = leaveOut(chips, ['unit:Search'])
    expect(noUnit).toEqual([])
    expect(messageOf(noUnit, mine)).toEqual({ unit: '', task: 0, context: [{ kind: 'attachment', label: 'log.txt', id: 'f9' }] })
  })
})

// Two projects: web (managers running in ws1 "api" and ws2 "site"; ws3 "old" open with
// messages and no manager) and notes (one manager, in ws4 "notes"); ws5 "lab" is of project
// lab, its manager running; ws6 "past" (of arch) is seated but not running.
const seat = (name: string, state: Seat['state'] = 'idle'): Seat => ({ session: `s-${name}`, name, folder: '/w', since: 1, state })
const space = (id: string, name: string, project: string, manager?: Seat, closed = false): Space => ({ id, name, project, manager, closed })
const spaces = [space('ws1', 'api', 'web', seat('a')), space('ws2', 'site', 'web', seat('s')), space('ws3', 'old', 'web'), space('ws4', 'notes', 'notes', seat('n')), space('ws5', 'lab', 'lab', seat('l')), space('ws6', 'past', 'arch', seat('p', 'gone'))]
const said = (workspace: string, unread = false): ManagerMessage => ({ workspace, manager: 'm', text: 'hi', at: '2026-11-21T09:00:00Z', step: 'delivered', replies: [{ by: 'm', text: 'ok', at: '2026-11-21T09:01:00Z', unread }] })
const active = (id: string, project: string, workspace: string) => ({ id, project, state: 'active', workspace, units: [{ id: `${id}-u` }] }) as unknown as Intent
const intents = [active('i1', 'web', 'ws2'), active('i2', 'notes', 'ws4'), active('i3', 'web', 'ws1')]
type Shown = Pick<Talk, 'intents' | 'intent' | 'spaces' | 'messages' | 'to' | 'followedAt' | 'project'>
const talk = (over: Partial<Shown> = {}): Shown => ({ intents, spaces, messages: [said('ws3')], ...over })

describe('the Manager section’s workspaces', () => {
  it('are the one on screen, then the rail’s project’s, then the rest; the one shown is the one picked, else the one on screen', () => {
    const p = placesOf(talk({ intent: 'i2', project: 'web' }))
    // Every open workspace, one entry each: the one on screen, the rail's project's, the rest.
    expect(p.ids).toEqual(['ws4', 'ws1', 'ws2', 'ws3', 'ws5', 'ws6'])
    expect(p.id).toBe('ws4')
    expect(placesOf(talk({ intent: 'i2', project: 'web', to: 'ws5' })).id).toBe('ws5')
    // Nothing on screen: the rail's project's first.
    expect(placesOf(talk({ project: 'notes' })).id).toBe('ws4')
  })

  it('show one followed to on a switch only while the intent then on screen is: another’s own takes its place', () => {
    // On notes, "api" has replies not read; the switch to web opens i1 (own "site") and follows to "api".
    const from = { ...talk({ intent: 'i2', project: 'notes' }), messages: [said('ws3'), said('ws1', true)] }
    expect(followProject(from, { project: 'web', intent: 'i1' }, null)).toBe('ws1')
    // i4, another intent of web, runs in "old".
    const followed = { ...from, intents: [...intents, active('i4', 'web', 'ws3')], project: 'web', to: 'ws1', followedAt: 'i1' }
    expect(placesOf({ ...followed, intent: 'i1' }).id).toBe('ws1')
    // Another intent of the project opened: its own workspace.
    expect(placesOf({ ...followed, intent: 'i4' }).id).toBe('ws3')
    // Followed with nothing on screen: holds until something opens.
    expect(placesOf({ ...followed, followedAt: '', intent: undefined }).id).toBe('ws1')
    expect(placesOf({ ...followed, followedAt: '', intent: 'i4' }).id).toBe('ws3')
    // A hand pick stays whatever opens.
    expect(placesOf({ ...followed, followedAt: undefined, intent: 'i4' }).id).toBe('ws1')
  })
})

describe('a project switch', () => {
  it('to a project with one manager shows it', () => {
    expect(followProject(talk({ intent: 'i1', project: 'web' }), { project: 'notes' }, null)).toBe('ws4')
  })

  it('to the project of the unit opened, its manager: the one on screen (nothing picked)', () => {
    expect(followProject(talk({ intent: 'i1', project: 'web', to: 'ws2' }), { project: 'notes', intent: 'i2' }, null)).toBeNull()
  })

  it('to a project with several: the one last picked there, else one with unread replies, else the first running', () => {
    const from = talk({ intent: 'i2', project: 'notes' })
    expect(followProject(from, { project: 'web' }, null)).toBe('ws1')
    expect(followProject({ ...from, messages: [said('ws3'), said('ws2', true)] }, { project: 'web' }, null)).toBe('ws2')
    expect(followProject({ ...from, messages: [said('ws3'), said('ws2', true)] }, { project: 'web' }, 'ws1')).toBe('ws1')
    // A pick remembered that is not there any more counts for nothing.
    expect(followProject(from, { project: 'web' }, 'gone')).toBe('ws1')
    // The first running is in the select's order: the unit opened's own first.
    expect(followProject(from, { project: 'web', intent: 'i1' }, null)).toBeNull()
    expect(followProject(from, { project: 'web', intent: 'i1' }, 'ws1')).toBe('ws1')
  })

  it('a switch back returns to what was picked there', () => {
    // Picked "site" by hand in web (remembered), then to notes, then back.
    const there = followProject(talk({ intent: 'i3', project: 'web', to: 'ws2' }), { project: 'notes' }, null)
    expect(there).toBe('ws4')
    expect(followProject(talk({ project: 'notes', to: there! }), { project: 'web', intent: 'i3' }, 'ws2')).toBe('ws2')
  })

  it('leaves All managers, a workspace already of the project, and a project with no manager', () => {
    expect(followProject(talk({ intent: 'i1', project: 'web', to: EVERY }), { project: 'notes' }, null)).toBeUndefined()
    expect(followProject(talk({ intent: 'i1', to: 'ws1' }), { project: 'web' }, null)).toBeUndefined()
    expect(followProject(talk({ intent: 'i1', project: 'web' }), { project: 'empty' }, null)).toBeUndefined()
  })

  it('goes only to a workspace whose manager runs (the unit opened’s own aside)', () => {
    // "arch" has a workspace whose seated manager does not run.
    const withPast = (over: Partial<Shown>) => talk({ ...over, messages: [said('ws3'), said('ws6')] })
    expect(followProject(withPast({ intent: 'i2', project: 'notes' }), { project: 'arch' }, null)).toBeUndefined()
    // A pick remembered whose manager has ended is passed over for one running.
    expect(followProject(talk({ intent: 'i2', project: 'notes' }), { project: 'web' }, 'ws3')).toBe('ws1')
  })
})

describe('the conversation list', () => {
  it('has one entry per open workspace by id: equal names are never folded, and say their project', () => {
    const twins = [space('ws1', 'chat', 'web', seat('a')), space('ws2', 'chat', 'notes', seat('b')), space('ws3', 'docs', 'web')]
    const projects = [{ id: 'web', title: 'Web' }, { id: 'notes', title: 'Notes' }]
    const p = placesOf({ intents: [], spaces: twins, messages: [], project: 'web' })
    expect(p.ids).toEqual(['ws1', 'ws3', 'ws2'])
    expect(p.ids.map(id => entryOf(twins.find(w => w.id === id)!, twins, projects, false))).toEqual(['chat ws1 · Web · a', 'docs ws3', 'chat ws2 · Notes · b'])
    // With every project shown, each says its project.
    expect(entryOf(twins[2], twins, projects, true)).toBe('docs ws3 · Web')
  })

  it('puts closed workspaces that have messages last, to read only; one with none is not offered', () => {
    const list = [space('ws1', 'chat', 'web', seat('a')), space('ws2', 'done', 'web', undefined, true), space('ws3', 'empty', 'web', undefined, true)]
    const p = placesOf({ intents: [], spaces: list, messages: [said('ws2')], to: 'ws2' })
    expect([p.ids, p.closed, p.id]).toEqual([['ws1'], ['ws2'], 'ws2'])
  })

  it('shows a workspace’s history with its seat empty, and offers every manager only with two running', () => {
    const p = placesOf({ intents, spaces, messages: [said('ws3')], to: 'ws3' })
    expect(p.id).toBe('ws3')
    expect(p.running.map(w => w.id)).toEqual(['ws1', 'ws2', 'ws4', 'ws5'])
    expect(placesOf({ intents: [], spaces: [space('ws1', 'chat', 'web', seat('a'))], messages: [] }).every).toBe(false)
  })
})
