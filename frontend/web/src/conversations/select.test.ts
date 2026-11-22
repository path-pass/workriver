// What the user picked becomes the chips above the Manager box, and the chips the message's
// unit, task and context. Which workspace's conversation the Manager section shows, and where
// it goes when the rail switches project.
import { describe, expect, it } from 'vitest'
import { chipsOf, EVERY, followProject, leaveOut, messageOf, placesOf, type Chip } from './select'
import type { Session } from '@/agents/types'
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

// Two projects: web (managers in "api" and "site", and "old" with messages only) and notes
// (one manager, in "notes"); "lab" works on no known project.
const manager = (agent: string, label: string) => ({ agent, manager: true, ended: false, group: { id: label, label } }) as unknown as Session
const projects: Record<string, string> = { api: 'web', site: 'web', old: 'web', notes: 'notes' }
const said = (workspace: string, unread = false): ManagerMessage => ({ workspace, manager: 'm', text: 'hi', at: '2026-11-21T09:00:00Z', step: 'delivered', replies: [{ by: 'm', text: 'ok', at: '2026-11-21T09:01:00Z', unread }] })
const active = (id: string, project: string, label: string) => ({ id, project, state: 'active', workspace: { id: label, label }, units: [{ id: `${id}-u` }] }) as unknown as Intent
const intents = [active('i1', 'web', 'site'), active('i2', 'notes', 'notes'), active('i3', 'web', 'api')]
type Shown = Pick<Talk, 'intents' | 'intent' | 'sessions' | 'messages' | 'to' | 'followedAt' | 'project' | 'projectOf'>
const talk = (over: Partial<Shown> = {}): Shown => ({
  intents, sessions: [manager('a', 'api'), manager('s', 'site'), manager('n', 'notes'), manager('l', 'lab')],
  messages: [said('old')], projectOf: l => projects[l], ...over,
})

describe('the Manager section’s workspaces', () => {
  it('are the one on screen, then the rail’s project’s, then the rest; the one shown is the one picked, else the one on screen', () => {
    const p = placesOf(talk({ intent: 'i2', project: 'web' }))
    expect(p.labels).toEqual(['notes', 'api', 'site', 'old', 'lab'])
    expect(p.label).toBe('notes')
    expect(placesOf(talk({ intent: 'i2', project: 'web', to: 'lab' })).label).toBe('lab')
    // Nothing on screen: the rail's project's first.
    expect(placesOf(talk({ project: 'notes' })).label).toBe('notes')
  })

  it('show one followed to on a switch only while the intent then on screen is: another’s own takes its place', () => {
    // On notes, "api" has replies not read; the switch to web opens i1 (own "site") and follows to "api".
    const from = { ...talk({ intent: 'i2', project: 'notes' }), messages: [said('old'), said('api', true)] }
    expect(followProject(from, { project: 'web', intent: 'i1' }, null)).toBe('api')
    // i4, another intent of web, runs in "old".
    const followed = { ...from, intents: [...intents, active('i4', 'web', 'old')], project: 'web', to: 'api', followedAt: 'i1' }
    expect(placesOf({ ...followed, intent: 'i1' }).label).toBe('api')
    // Another intent of the project opened: its own workspace.
    expect(placesOf({ ...followed, intent: 'i4' }).label).toBe('old')
    // Followed with nothing on screen: holds until something opens.
    expect(placesOf({ ...followed, followedAt: '', intent: undefined }).label).toBe('api')
    expect(placesOf({ ...followed, followedAt: '', intent: 'i4' }).label).toBe('old')
    // A hand pick stays whatever opens.
    expect(placesOf({ ...followed, followedAt: undefined, intent: 'i4' }).label).toBe('api')
  })
})

describe('a project switch', () => {
  it('to a project with one manager shows it', () => {
    expect(followProject(talk({ intent: 'i1', project: 'web' }), { project: 'notes' }, null)).toBe('notes')
  })

  it('to the project of the unit opened, its manager: the one on screen (nothing picked)', () => {
    expect(followProject(talk({ intent: 'i1', project: 'web', to: 'site' }), { project: 'notes', intent: 'i2' }, null)).toBeNull()
  })

  it('to a project with several: the one last picked there, else one with unread replies, else the first running', () => {
    const from = talk({ intent: 'i2', project: 'notes' })
    expect(followProject(from, { project: 'web' }, null)).toBe('api')
    expect(followProject({ ...from, messages: [said('old'), said('site', true)] }, { project: 'web' }, null)).toBe('site')
    expect(followProject({ ...from, messages: [said('old'), said('site', true)] }, { project: 'web' }, 'api')).toBe('api')
    // A pick remembered that is not there any more counts for nothing.
    expect(followProject(from, { project: 'web' }, 'gone')).toBe('api')
    // The first running is in the select's order: the unit opened's own first.
    expect(followProject(from, { project: 'web', intent: 'i1' }, null)).toBeNull()
    expect(followProject(from, { project: 'web', intent: 'i1' }, 'api')).toBe('api')
  })

  it('a switch back returns to what was picked there', () => {
    // Picked "site" by hand in web (remembered), then to notes, then back.
    const there = followProject(talk({ intent: 'i3', project: 'web', to: 'site' }), { project: 'notes' }, null)
    expect(there).toBe('notes')
    expect(followProject(talk({ project: 'notes', to: there! }), { project: 'web', intent: 'i3' }, 'site')).toBe('site')
  })

  it('leaves All managers, a workspace already of the project, one of no known project, and a project with no manager', () => {
    expect(followProject(talk({ intent: 'i1', project: 'web', to: EVERY }), { project: 'notes' }, null)).toBeUndefined()
    expect(followProject(talk({ intent: 'i1', to: 'api' }), { project: 'web' }, null)).toBeUndefined()
    expect(followProject(talk({ intent: 'i1', project: 'web', to: 'lab' }), { project: 'notes' }, null)).toBeUndefined()
    expect(followProject(talk({ intent: 'i1', project: 'web' }), { project: 'empty' }, null)).toBeUndefined()
  })

  it('goes only to a workspace whose manager runs (the unit opened’s own aside)', () => {
    // "arch" has a workspace with messages only: its manager has ended.
    const withPast = (over: Partial<Shown>) => talk({ ...over, messages: [said('old'), said('past')], projectOf: l => (l === 'past' ? 'arch' : projects[l]) })
    expect(followProject(withPast({ intent: 'i2', project: 'notes' }), { project: 'arch' }, null)).toBeUndefined()
    // A pick remembered whose manager has ended is passed over for one running.
    expect(followProject(talk({ intent: 'i2', project: 'notes' }), { project: 'web' }, 'old')).toBe('api')
  })
})
