// A unit's template steps in a task's detail: the step a task is, and what a gate lists of
// the steps (done by whom, left out with who and why, not done), as the service gave them.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { GateStepsList, StepLine } from './GateSteps'
import type { Task } from './types'
import type { Unit } from '@/planning/types'

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

let root: Root
let host: HTMLDivElement
beforeEach(() => { host = document.createElement('div'); document.body.append(host); root = createRoot(host) })
afterEach(() => { act(() => root.unmount()); host.remove() })

const unit = { id: 'u3', template: { name: 'development', builtin: true } } as unknown as Unit
const task = (over: Partial<Task>) => ({ id: '2', kind: 'review', state: 'todo', taskId: null, title: '', agent: null, writing: false, inputs: [], input: { text: '', causes: [] }, output: {}, plans: [], ...over }) as Task

describe('template steps', () => {
  it('says which step a placed task is, and who fills it while it is not filled', () => {
    act(() => root.render(<StepLine unit={unit} n={task({ step: { id: 'design-review', description: 'The design against the rules.' } })} />))
    expect(host.textContent).toBe('Step design-review· development (built in)The design against the rules.Not filled yet: its manager fills it, or leaves it out with the reason.')
    act(() => root.render(<StepLine unit={unit} n={task({ step: { id: 'design-review', description: 'd', filled: true } })} />))
    expect(host.textContent).not.toContain('Not filled yet')
    act(() => root.render(<StepLine unit={unit} n={task({})} />))
    expect(host.textContent).toBe('')
  })

  it('lists on a gate what was done, left out and not done', () => {
    const steps = {
      template: 'development',
      done: [{ id: 'design', n: 1, by: 'designer' }, { id: 'confirm', n: 3, by: 'user' }],
      leftOut: [{ id: 'security-review', n: 6, by: 'claude 1', reason: 'no input, access or secret' }],
      notDone: [{ id: 'code-review', n: 5 }, { id: 'architecture-review', n: 7 }],
    }
    act(() => root.render(<GateStepsList steps={steps} />))
    expect(host.textContent).toContain('Template development (built in)')
    expect(host.textContent).toContain('Done: design (#1), by designer, confirm (#3), by you')
    expect(host.querySelector('[data-left-out]')?.textContent).toBe('Left out: security-review (#6), by claude 1: no input, access or secret')
    expect(host.querySelector('[data-not-done]')?.textContent).toBe('Not done: code-review (#5), architecture-review (#7)')
    act(() => root.render(<GateStepsList steps={{ template: 'research', users: true, done: [], leftOut: [], notDone: [] }} />))
    expect(host.textContent).toBe('Template research (yours)No step of the template before it.')
  })
})
