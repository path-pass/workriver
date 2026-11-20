// How a gate is named: by what it asks of the user; a decide gate with options is a question.
import { describe, expect, it } from 'vitest'
import { finderLabel } from './select'
import type { Task } from './types'
import type { T } from '@/i18n'

const t = ((key: string) => key) as unknown as T
const gate = (more: Partial<Task['input']> = {}) => ({ kind: 'gate', gate: 'decide', input: { text: '', causes: [], ...more } }) as unknown as Task

describe('a gate’s name', () => {
  it('is its kind, or a question when it carries options', () => {
    expect(finderLabel(t, gate())).toBe('gate.decide')
    expect(finderLabel(t, gate({ choice: { surface: {} } }))).toBe('gate.choice')
  })
})
