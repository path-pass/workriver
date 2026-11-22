// Every choice ends with workriver's Other, in the page's words, after the asker's options;
// one recorded before the rule with an option of that value shows it once, as the asker's.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { en } from '@/i18n/en'
import '@/i18n'
import { ChoiceSurface, Chose } from './Choice'
import type { Choice } from './surface'

const picker = (options: { value: string; label: string }[]): Choice => ({ surface: { version: '1.0', createSurface: { components: [{}, { component: 'ChoicePicker', options }] } } })
const choice = picker([{ value: 'merge', label: 'Merge' }, { value: 'hold', label: 'Hold' }])

let root: Root
let el: HTMLDivElement
beforeEach(() => { el = document.createElement('div'); document.body.append(el); root = createRoot(el) })
afterEach(() => { act(() => root.unmount()); el.remove() })
const show = (node: React.ReactNode) => act(() => root.render(node))
const values = () => [...el.querySelectorAll('[data-slot=radio-group-item]')].map(r => r.getAttribute('value'))

describe('a choice’s options', () => {
  it('end with Other, which asks for a note', () => {
    show(<ChoiceSurface choice={choice} />)
    expect(values()).toEqual(['merge', 'hold', 'other'])
    const last = [...el.querySelectorAll('label')].at(-1)!
    expect(last.textContent).toBe(en.choice.other + en.choice.otherHint)
  })
  it('show an asker’s own option of that value once, as theirs', () => {
    show(<ChoiceSurface choice={picker([{ value: 'a', label: 'A' }, { value: 'other', label: 'Else' }])} />)
    expect(values()).toEqual(['a', 'other'])
    expect(el.textContent).not.toContain(en.choice.other)
  })
  it('read Other back in the page’s words', () => {
    show(<Chose choice={choice} value="other" />)
    expect(el.textContent).toBe(en.choice.chose.replace('{{label}}', en.choice.other))
  })
})
