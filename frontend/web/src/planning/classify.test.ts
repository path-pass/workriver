// An intent's kind, size and tags on the page: tags as kept, the rail's filter, the project's
// tags, and the filter remembered per project.
import { afterEach, describe, expect, it } from 'vitest'
import { filtersOn, keeps, loadFilter, NO_FILTER, normalTags, saveFilter, splitTags, tagsOf } from './classify'
import type { Intent } from './types'

const intent = (more: Partial<Intent>): Intent => ({ id: 'i1', title: 'T', units: [], chores: [], ...more }) as Intent

describe('tags', () => {
  it('are trimmed, lower-case, with no repeats or empties; typed text splits at commas and spaces', () => {
    expect(normalTags([' Security', 'CLI', 'security', '', '認証'])).toEqual(['security', 'cli', '認証'])
    expect(splitTags('ui, Auth  cli,,ui')).toEqual(['ui', 'auth', 'cli'])
  })

  it('of a project, the most used first, then by name', () => {
    expect(tagsOf([intent({ tags: ['ui', 'cli'] }), intent({ tags: ['cli'] }), intent({ tags: ['api'] }), intent({})])).toEqual(['cli', 'api', 'ui'])
  })
})

describe('the rail filter', () => {
  const fix = intent({ kind: 'fix', size: 's', tags: ['security', 'cli'] })
  const plain = intent({})

  it('keeps everything when nothing is on', () => {
    expect([fix, plain].filter(i => keeps(NO_FILTER, i))).toEqual([fix, plain])
    expect(filtersOn(NO_FILTER)).toBe(0)
  })

  it('any one ticked within a group, all groups together; an intent without the field is left out', () => {
    expect(keeps({ kinds: ['feature', 'fix'], sizes: [], tags: [] }, fix)).toBe(true)
    expect(keeps({ kinds: ['fix'], sizes: [], tags: [] }, plain)).toBe(false)
    expect(keeps({ kinds: ['fix'], sizes: ['m'], tags: [] }, fix)).toBe(false)
    expect(keeps({ kinds: [], sizes: ['s'], tags: ['ui', 'cli'] }, fix)).toBe(true)
    expect(keeps({ kinds: [], sizes: [], tags: ['ui'] }, fix)).toBe(false)
    expect(filtersOn({ kinds: ['fix'], sizes: ['s', 'm'], tags: ['ui'] })).toBe(4)
  })
})

describe('the filter is remembered per project', () => {
  afterEach(() => localStorage.clear())

  it('kept for the project it was set in; none kept when it is cleared; what is not a value is dropped', () => {
    saveFilter('/p', { kinds: ['fix'], sizes: [], tags: ['ui'] })
    expect(loadFilter('/p')).toEqual({ kinds: ['fix'], sizes: [], tags: ['ui'] })
    expect(loadFilter('/q')).toEqual(NO_FILTER)
    expect(loadFilter(null)).toEqual(NO_FILTER)
    saveFilter('/p', NO_FILTER)
    expect(localStorage.getItem('workriver.filter./p')).toBeNull()
    localStorage.setItem('workriver.filter.*', JSON.stringify({ kinds: ['bug', 'fix'], sizes: 'm', tags: [3, 'x'] }))
    expect(loadFilter(null)).toEqual({ kinds: ['fix'], sizes: [], tags: ['x'] })
    localStorage.setItem('workriver.filter.*', '{')
    expect(loadFilter(null)).toEqual(NO_FILTER)
  })
})
