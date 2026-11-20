// A gate result the service did not take: the waiting gate's form comes back as it was sent
// (the option, the note, the pictures), under the line saying it failed. A gate the service
// finalized shows the service's receipt, not a failed attempt of this page.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { en } from '@/i18n/en'
import '@/i18n'
import { ActContext, type Act } from './actions'
import { GateForm } from './GateForm'
import { DeliveryLine } from './Findings'
import type { GateResult, Task } from './types'
import type { Unit } from '@/planning/types'
import type { Sent } from '@/conversations/types'

const choice = { surface: { version: '1.0', createSurface: { components: [{}, { component: 'ChoicePicker', options: [{ value: 'short', label: 'Short' }, { value: 'long', label: 'Long' }] }] } }, needs_note: ['long'] }
const gate = { id: '2', title: 'Which?', kind: 'gate', gate: 'decide', state: 'waiting', inputs: [{ from: '1', primary: true }], input: { text: '', causes: [], choice }, output: {} } as unknown as Task
const unit = { id: 'u1', tasks: [{ id: '1', agent: 'worker' }, gate] } as unknown as Unit
const act0 = (more: Partial<Act>): Act => ({ submitGate: () => {}, decide: () => {}, undo: () => {}, choreDone: () => {}, merge: async () => {}, delivery: {}, message: {}, retry: {}, ...more })

let root: Root
let el: HTMLDivElement
beforeEach(() => { el = document.createElement('div'); document.body.append(el); root = createRoot(el) })
afterEach(() => { act(() => root.unmount()); el.remove() })
const show = (a: Act, node: React.ReactNode) => act(() => root.render(<ActContext.Provider value={a}>{node}</ActContext.Provider>))

describe('a gate result the service did not take', () => {
  it('reopens the form with what was sent, under the failed line', () => {
    const sent: GateResult = { result: 'answered', selected: ['long'], note: 'Keep this reason', attachments: [], findings: [], at: '2026-11-21T10:00:00Z' }
    show(act0({ delivery: { 'u1:2': { step: 'failed', to: 'manager' } }, retry: { 'u1:2': sent } }), <GateForm unit={unit} gate={gate} />)
    const checked = el.querySelector('[data-slot=radio-group-item][data-state=checked]')
    expect(checked?.getAttribute('value')).toBe('long')
    expect(el.querySelector('textarea')?.value).toBe('Keep this reason')
    expect(el.textContent).toContain(en.deliver.failed)
  })
})

describe('a finalized gate’s delivery line', () => {
  it('shows the service’s receipt over this page’s failed attempt', () => {
    const receipt: Sent = { step: 'delivered', to: 'boss', toManager: true, message: 'The user selected Short' }
    show(act0({ delivery: { 'u1:2': { step: 'failed', to: 'manager' } } }), <DeliveryLine id="u1:2" sent={receipt} message />)
    expect(el.textContent).toContain(en.deliver.toManager)
    expect(el.textContent).not.toContain(en.deliver.failed)
  })
})
