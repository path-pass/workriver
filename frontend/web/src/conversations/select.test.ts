// What the user picked becomes the chips above the Manager box, and the chips the message's
// unit, task and context.
import { describe, expect, it } from 'vitest'
import { chipsOf, leaveOut, messageOf, type Chip } from './select'
import type { Unit } from '@/planning/types'
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
    const noTask = leaveOut(chips, ['task:#3'])
    expect(noTask.map(keyOf)).toEqual(['unit:Search'])
    expect(messageOf(noTask, mine)).toEqual({ unit: 'u1', task: 0, context: [{ kind: 'attachment', label: 'log.txt', id: 'f9' }] })
    const noUnit = leaveOut(chips, ['unit:Search'])
    expect(noUnit).toEqual([])
    expect(messageOf(noUnit, mine)).toEqual({ unit: '', task: 0, context: [{ kind: 'attachment', label: 'log.txt', id: 'f9' }] })
  })
})
