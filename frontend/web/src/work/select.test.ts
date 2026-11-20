// How a gate is named: by what it asks of the user; a decide gate with options is a question.
import { describe, expect, it } from 'vitest'
import { finderLabel, waitsOn } from './select'
import type { Task } from './types'
import type { T } from '@/i18n'
import type { Unit } from '@/planning/types'

const t = ((key: string) => key) as unknown as T
const gate = (more: Partial<Task['input']> = {}) => ({ kind: 'gate', gate: 'decide', input: { text: '', causes: [], ...more } }) as unknown as Task

describe('a gate’s name', () => {
  it('is its kind, or a question when it carries options', () => {
    expect(finderLabel(t, gate())).toBe('gate.decide')
    expect(finderLabel(t, gate({ choice: { surface: {} } }))).toBe('gate.choice')
  })
})

describe('who a gate’s result goes to while it is on its way', () => {
  const unit = (gateKind: string) => ({ id: 'u1', tasks: [{ id: 's1', kind: 'implement', agent: 'worker-1' }, { id: 'g', kind: 'gate', gate: gateKind, inputs: [{ from: 's1', primary: true }] }] }) as unknown as Unit
  it('is the agent of the work it follows, but a decide gate’s always the manager', () => {
    expect(waitsOn(unit('test'), unit('test').tasks[1])).toBe('worker-1')
    expect(waitsOn(unit('decide'), unit('decide').tasks[1])).toBeNull()
  })
})
