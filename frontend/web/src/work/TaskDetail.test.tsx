// What the manager or the user added to a task after it was recorded shows under what it
// asked first, in order, each with whose it is; nothing shows for a task with none.
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import { Added } from './TaskDetail'
import type { Task } from './types'

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

let host: HTMLDivElement
beforeEach(() => { host = document.createElement('div'); document.body.append(host) })
afterEach(() => host.remove())

const task = (notes?: Task['input']['notes']): Task => ({
  id: '1', kind: 'implement', state: 'running', taskId: '1', title: 'Build the adapter', agent: 'impl', writing: false,
  inputs: [{ from: 'start', primary: true }], input: { text: 'Build the adapter', causes: [], notes }, output: {}, plans: [],
})

function shown(n: Task) {
  const root = createRoot(host)
  act(() => root.render(<TooltipProvider><Added n={n} /></TooltipProvider>))
  const list = host.querySelector('ul')
  const items = [...host.querySelectorAll('li')].map(li => li.textContent ?? '')
  act(() => root.unmount())
  return { label: list?.getAttribute('aria-label'), items }
}

describe('requirements added to a task', () => {
  it('each under the other, oldest first, with who and when; the user as "you"', () => {
    const { label, items } = shown(task([
      { by: 'boss', user: false, at: '2026-11-20T10:00:00Z', text: 'Also cover the Codex adapter' },
      { by: 'user', user: true, at: '2026-11-20T11:00:00Z', text: 'And keep the old flag' },
    ]))
    expect(label).toBe('Added to what it asks')
    expect(items).toHaveLength(2)
    expect(items[0]).toContain('boss added to what it asks')
    expect(items[0]).toContain('Also cover the Codex adapter')
    expect(items[1]).toContain('You added to what it asks')
    expect(items[1]).toContain('And keep the old flag')
  })

  it('"you" only for what the user recorded, not for an agent that goes by "user"', () => {
    const { items } = shown(task([{ by: 'user', user: false, at: '2026-11-20T10:00:00Z', text: 'Also the docs' }]))
    expect(items[0]).toContain('user added to what it asks')
    expect(items[0]).not.toContain('You added')
  })

  it('nothing for a task with none', () => {
    expect(shown(task()).items).toEqual([])
    expect(shown(task([])).label).toBeUndefined()
  })
})
