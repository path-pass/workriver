// A gate result the service did not take: the waiting gate's form comes back as it was sent
// (the option, the note, the pictures), under the line saying it failed. A gate the service
// finalized shows the service's receipt, not a failed attempt of this page.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { en } from '@/i18n/en'
import '@/i18n'
import { ActContext, type Act } from './actions'
import { GateForm } from './GateForm'
import { DeliveryLine } from './Findings'
import type { GateResult, Task } from './types'
import type { Unit } from '@/planning/types'
import type { Sent } from '@/conversations/types'
import { attach, submitGate } from '@/api/actions'

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

describe('a failed gate’s picture', () => {
  it('stays readable across leaving the gate and coming back, and the draft submits again', async () => {
    const picture = attach(new File(['picture for this check'], 'retry.png', { type: 'image/png' }))
    const sent: GateResult = { result: 'answered', selected: ['long'], note: 'Keep this reason', attachments: [picture], findings: [], at: '2026-11-21T10:00:00Z' }
    const a = act0({ delivery: { 'u1:2': { step: 'failed', to: 'manager' } }, retry: { 'u1:2': sent } })
    const revoked = vi.spyOn(URL, 'revokeObjectURL')
    show(a, <GateForm unit={unit} gate={gate} />)
    show(a, <span>Another task</span>) // away: the form goes
    show(a, <GateForm unit={unit} gate={gate} />) // and back
    expect(revoked).not.toHaveBeenCalledWith(picture.url)
    expect([...el.querySelectorAll('img')].some(i => i.getAttribute('src') === picture.url)).toBe(true)
    // The same draft goes out: the picture is read and uploaded, then the result recorded. (A
    // blob URL reads as in a browser: its picture until it is revoked, then nothing.)
    const calls: string[] = []
    const gone = (url: string) => revoked.mock.calls.some(([u]) => u === url)
    vi.stubGlobal('fetch', (url: string) => {
      if (url.startsWith('blob:')) return gone(url) ? Promise.reject(new TypeError('revoked')) : Promise.resolve(new Response('picture for this check'))
      if (url.startsWith('/api/attachments')) { calls.push('upload'); return Promise.resolve(new Response('{"id":"0000000000000001"}')) }
      if (url.startsWith('/api/gate')) { calls.push('gate'); return Promise.resolve(new Response('{}')) }
      return Promise.reject(new TypeError(url))
    })
    const step = await new Promise(resolve => submitGate(unit, gate, sent, 'manager', d => { if (d.step === 'recorded' || d.step === 'failed') resolve(d.step) }))
    vi.unstubAllGlobals()
    expect([step, calls]).toEqual(['recorded', ['upload', 'gate']])
  }, 10000)
})

describe('a choice gate answered with Other', () => {
  it('needs the note, then sends other with it', () => {
    const sent: GateResult[] = []
    show(act0({ submitGate: (_u, _g, r) => { sent.push(r) } }), <GateForm unit={unit} gate={gate} />)
    const other = el.querySelector<HTMLButtonElement>('[data-slot=radio-group-item][value=other]')!
    act(() => other.click())
    const note = el.querySelector('textarea')!
    expect(note.getAttribute('aria-required')).toBe('true')
    expect(note.getAttribute('placeholder')).toBe(en.choice.otherNote)
    // The form's own Answer, after the options (the result toggle reads Answer too).
    const answer = () => act(() => [...el.querySelectorAll('button')].filter(b => b.textContent === en.choice.answer).at(-1)!.click())
    answer()
    expect(sent).toEqual([])
    expect(el.querySelector('[role=alert]')?.textContent).toBe(en.choice.otherNeedNote)
    act(() => {
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(note, 'Ask the reviewer first')
      note.dispatchEvent(new Event('input', { bubbles: true }))
    })
    answer()
    expect(sent.map(r => [r.result, r.selected, r.note])).toEqual([['answered', ['other'], 'Ask the reviewer first']])
  })
})
