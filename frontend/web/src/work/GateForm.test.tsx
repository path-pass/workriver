// A gate result the service did not take: the waiting gate's form comes back as it was sent
// (the option, the note, the pictures), under the line saying it failed. A gate the service
// finalized shows the service's receipt, not a failed attempt of this page.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { en } from '@/i18n/en'
import '@/i18n'
import { ActContext, type Act } from './actions'
import { GateDraftsContext, GateForm, type GateDrafts } from './GateForm'
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
let drafts: GateDrafts
beforeEach(() => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true)
  el = document.createElement('div')
  document.body.append(el)
  root = createRoot(el)
  drafts = new Map()
})
afterEach(() => { act(() => root.unmount()); el.remove(); vi.restoreAllMocks(); vi.unstubAllGlobals() })
const show = (a: Act, node: React.ReactNode) => act(() => root.render(
  <ActContext.Provider value={a}><GateDraftsContext.Provider value={drafts}>{node}</GateDraftsContext.Provider></ActContext.Provider>,
))
const typeText = (field: HTMLInputElement | HTMLTextAreaElement, value: string) => act(() => {
  Object.getOwnPropertyDescriptor(Object.getPrototypeOf(field), 'value')!.set!.call(field, value)
  field.dispatchEvent(new Event('input', { bubbles: true }))
})
const click = (label: string) => act(() => [...el.querySelectorAll('button')].find(b => b.textContent === label)!.click())
const pick = (value: string) => act(() => el.querySelector<HTMLButtonElement>(`[data-slot=radio-group-item][value="${value}"]`)!.click())

describe('an unsent gate draft', () => {
  it('keeps each unit’s note and picked option until the result is sent', () => {
    const otherUnit = { ...unit, id: 'u2' }
    const sent: GateResult[] = []
    const a = act0({ submitGate: (_u, _g, result) => { sent.push(result) } })
    show(a, <GateForm key={unit.id} unit={unit} gate={gate} />)
    pick('long')
    typeText(el.querySelector('textarea')!, 'Run the longer check')

    show(a, <GateForm key={otherUnit.id} unit={otherUnit} gate={gate} />)
    expect(el.querySelector('textarea')?.value).toBe('')
    expect(el.querySelector('[data-slot=radio-group-item][data-state=checked]')).toBeNull()
    typeText(el.querySelector('textarea')!, 'Separate unit')
    show(a, <GateForm key={unit.id} unit={unit} gate={gate} />)
    expect(el.querySelector('textarea')?.value).toBe('Run the longer check')
    expect(el.querySelector('[data-slot=radio-group-item][data-state=checked]')?.getAttribute('value')).toBe('long')
    act(() => [...el.querySelectorAll('button')].filter(b => b.textContent === en.choice.answer).at(-1)!.click())
    expect(sent.map(r => [r.result, r.selected, r.note])).toEqual([['answered', ['long'], 'Run the longer check']])

    show(a, <GateForm key={otherUnit.id} unit={otherUnit} gate={gate} />)
    expect(el.querySelector('textarea')?.value).toBe('Separate unit')
    show(a, <GateForm key={unit.id} unit={unit} gate={gate} />)
    expect(el.querySelector('textarea')?.value).toBe('')
    expect(el.querySelector('[data-slot=radio-group-item][data-state=checked]')).toBeNull()
  })

  it('keeps problem findings and their pictures across navigation', () => {
    const sent: GateResult[] = []
    const a = act0({ submitGate: (_u, _g, result) => { sent.push(result) } })
    const revoked = vi.spyOn(URL, 'revokeObjectURL')
    const picture = (name: string, scope: ParentNode) => {
      const input = scope.querySelector<HTMLInputElement>('input[type=file]')!
      Object.defineProperty(input, 'files', { value: [new File(['picture'], name, { type: 'image/png' })] })
      act(() => input.dispatchEvent(new Event('change', { bubbles: true })))
      return scope.querySelector('img')!.getAttribute('src')
    }

    show(a, <GateForm unit={unit} gate={gate} />)
    click(en.gate.problem)
    click('P1')
    const finding = el.querySelector('fieldset')!
    const fields = finding.querySelectorAll<HTMLInputElement>('input:not([type=file])')
    typeText(fields[0], 'Checkout fails')
    typeText(fields[1], 'Cannot continue')
    typeText(finding.querySelector('textarea')!, 'checkout.ts, test.ts')
    const noteURL = picture('note.png', el)
    const findingURL = picture('finding.png', finding)
    click(en.gate.addFinding)
    const unusedURL = picture('unused.png', el.querySelectorAll('fieldset')[1])

    show(a, <span>Another page</span>)
    show(a, <GateForm unit={unit} gate={gate} />)
    expect([...el.querySelectorAll('img')].map(img => img.getAttribute('src'))).toEqual([noteURL, findingURL, unusedURL])
    expect(revoked).not.toHaveBeenCalled()
    click(en.gate.submit)
    expect(sent).toHaveLength(1)
    expect(sent[0].attachments.map(p => p.url)).toEqual([noteURL])
    expect(sent[0].findings).toMatchObject([{
      severity: 'P1', problem: 'Checkout fails', impact: 'Cannot continue',
      evidence: ['checkout.ts', 'test.ts'], attachments: [{ url: findingURL }],
    }])
    expect(revoked.mock.calls).toEqual([[unusedURL]])
  })

  it('keeps a deliberately cleared note when returning to a gate with a conclusion', () => {
    const withConclusion = { ...gate, draft: 'Conclusion from discussion' }
    const updated = { ...gate, draft: 'Updated conclusion' }
    const a = act0({})
    show(a, <GateForm unit={unit} gate={gate} />)
    show(a, <GateForm unit={unit} gate={withConclusion} />)
    expect(el.querySelector('textarea')?.value).toBe('Conclusion from discussion')
    show(a, <GateForm unit={unit} gate={updated} />)
    expect(el.querySelector('textarea')?.value).toBe('Conclusion from discussion')
    typeText(el.querySelector('textarea')!, '')
    show(a, <span>Another task</span>)
    show(a, <GateForm unit={unit} gate={updated} />)
    expect(el.querySelector('textarea')?.value).toBe('')
  })
})

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
    const submitted: GateResult[] = []
    const a = act0({ delivery: { 'u1:2': { step: 'failed', to: 'manager' } }, retry: { 'u1:2': sent }, submitGate: (_u, _g, result) => { submitted.push(result) } })
    const revoked = vi.spyOn(URL, 'revokeObjectURL')
    show(a, <GateForm unit={unit} gate={gate} />)
    show(a, <span>Another task</span>) // away: the form goes
    show(a, <GateForm unit={unit} gate={gate} />) // and back
    expect(revoked).not.toHaveBeenCalledWith(picture.url)
    expect([...el.querySelectorAll('img')].some(i => i.getAttribute('src') === picture.url)).toBe(true)
    act(() => [...el.querySelectorAll('button')].filter(b => b.textContent === en.choice.answer).at(-1)!.click())
    expect(submitted[0]?.attachments).toEqual([picture])
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
    const step = await new Promise(resolve => submitGate(unit, gate, submitted[0], 'manager', d => { if (d.step === 'recorded' || d.step === 'failed') resolve(d.step) }))
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
    // A note of only invisible marks (a pasted zero-width space) shows nothing: no note.
    act(() => {
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(note, '\u200b\n\u202e')
      note.dispatchEvent(new Event('input', { bubbles: true }))
    })
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
