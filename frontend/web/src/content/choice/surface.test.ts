import { describe, expect, it } from 'vitest'
import { labelOf, needsNote, optionsOf, type Choice } from './surface'

// The stored form, as core builds it (`choices/mod.rs`).
const stored: Choice = {
  surface: { version: '1.0', createSurface: {
    surfaceId: 'choice',
    catalogId: 'https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json',
    components: [
      { id: 'root', component: 'Column', children: ['options', 'submit'] },
      { id: 'options', component: 'ChoicePicker', variant: 'mutuallyExclusive', options: [{ label: '核对通过后合并并推送', value: 'merge' }, { label: '先不合并', value: 'hold' }], value: { '@path': '/selected' } },
      { id: 'submit', component: 'Button', variant: 'primary', child: 'submit-label', action: { event: { name: 'answer', context: { selected: { '@path': '/selected' } } } }, '@checks': [{ condition: { '@call': 'required', args: { value: { '@path': '/selected' } } } }] },
      { id: 'submit-label', component: 'Text', text: { '@path': '/ui/submit' } },
    ],
    dataModel: { selected: [] } } },
  needs_note: ['hold'],
}

describe('a stored choice', () => {
  it('reads its options in order, their words, and which need a note', () => {
    expect(optionsOf(stored)).toEqual([{ value: 'merge', label: '核对通过后合并并推送' }, { value: 'hold', label: '先不合并' }])
    expect([labelOf(stored, 'hold'), labelOf(stored, 'nope')]).toEqual(['先不合并', undefined])
    expect([needsNote(stored, 'hold'), needsNote(stored, 'merge'), needsNote({ ...stored, needs_note: undefined }, 'hold')]).toEqual([true, false, false])
  })
  it('draws nothing it does not know: only the subset’s picker, only literal words', () => {
    const odd = (options: unknown): Choice => ({ surface: { version: '1.0', createSurface: { components: [{}, { component: 'ChoicePicker', options }] } } })
    expect(optionsOf({ surface: {} })).toEqual([])
    expect(optionsOf(odd([{ label: { '@path': '/x' }, value: 'a' }]))).toEqual([])
    expect(optionsOf({ surface: { version: '0.9', createSurface: stored.surface.createSurface } })).toEqual([])
  })
})
