import { describe, expect, it } from 'vitest'
import { isOther, labelOf, needsNote, OTHER, optionsOf, showsNothing, type Choice } from './surface'

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
  it('takes workriver’s Other, never stored, always with a note; an asker’s option of that value stays theirs', () => {
    expect(OTHER).toBe('other')
    expect(optionsOf(stored).map(o => o.value)).not.toContain(OTHER)
    expect([isOther(stored, OTHER), isOther(stored, 'hold'), needsNote(stored, OTHER)]).toEqual([true, false, true])
    const before: Choice = { surface: { version: '1.0', createSurface: { components: [{}, { component: 'ChoicePicker', options: [{ label: 'A', value: 'a' }, { label: 'Else', value: 'other' }] }] } } }
    expect([isOther(before, OTHER), needsNote(before, OTHER), labelOf(before, OTHER)]).toEqual([false, false, 'Else'])
  })
  it('counts a note that shows nothing once cleaned for a terminal as none, as core does', () => {
    expect(['', ' \n', '\u200b', '\u200b\u200f\u202a\u202e\u2066\u2069\ufeff \t', '\u0000\u001b'].map(showsNothing)).toEqual([true, true, true, true, true])
    expect(['x', '\u200bx', '‹›'].map(showsNothing)).toEqual([false, false, false])
  })
  it('draws nothing it does not know: only the subset’s picker, only literal words', () => {
    const odd = (options: unknown): Choice => ({ surface: { version: '1.0', createSurface: { components: [{}, { component: 'ChoicePicker', options }] } } })
    expect(optionsOf({ surface: {} })).toEqual([])
    expect(optionsOf(odd([{ label: { '@path': '/x' }, value: 'a' }]))).toEqual([])
    expect(optionsOf({ surface: { version: '0.9', createSurface: stored.surface.createSurface } })).toEqual([])
  })
})
