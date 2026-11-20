// Every unit, intent and task has an address the page follows and sets.
import { describe, expect, it } from 'vitest'
import { pathOf, placeOf, resolve } from './link'
import type { Workspace } from '@/workspace/types'

const ws = {
  intents: [
    { id: 'i1', units: [{ id: 'i1', intentLevel: true, tasks: [{ id: '1' }] }, { id: 'u1', tasks: [{ id: '1' }, { id: '3' }] }] },
    { id: 'i2', units: [{ id: 'u2', tasks: [] }] },
  ],
} as unknown as Workspace

describe('addresses', () => {
  it('reads what an address names', () => {
    expect(placeOf('/', '')).toBeNull()
    expect(placeOf('', '')).toBeNull()
    expect(placeOf('/u1', '')).toEqual({ id: 'u1', task: undefined, finding: undefined })
    expect(placeOf('/u1/3', '?finding=P2-1')).toEqual({ id: 'u1', task: '3', finding: 'P2-1' })
    expect(placeOf('/i3/1/', '')).toEqual({ id: 'i3', task: '1', finding: undefined })
    expect(placeOf('/a%20b/x%2Fy', '')).toEqual({ id: 'a b', task: 'x/y', finding: undefined })
    // Anything else is still a place, one that names nothing.
    expect(placeOf('/u1/3/extra', '')).toEqual({ id: 'u1/3/extra' })
  })

  it('writes the address of a unit and what is picked in it', () => {
    expect(pathOf('u1', null)).toBe('/u1')
    expect(pathOf('u1', { kind: 'task', id: '3' })).toBe('/u1/3')
    expect(pathOf('u1', { kind: 'task', id: '3', finding: 'P2-1' })).toBe('/u1/3?finding=P2-1')
    expect(pathOf('a b', { kind: 'task', id: 'x/y' })).toBe('/a%20b/x%2Fy')
    for (const p of ['/u1', '/u1/3', '/u1/3?finding=P2-1']) {
      const [path, search = ''] = p.split('?')
      const place = placeOf(path, search ? `?${search}` : '')!
      expect(pathOf(place.id, place.task ? { kind: 'task', id: place.task, finding: place.finding } : null)).toBe(p)
    }
  })

  it('opens a unit, an intent page, a task in either; or nothing it names', () => {
    expect(resolve(ws, { id: 'u1' })).toEqual({ unit: 'u1', pick: null })
    expect(resolve(ws, { id: 'u1', task: '3', finding: 'P2-1' })).toEqual({ unit: 'u1', pick: { kind: 'task', id: '3', finding: 'P2-1' } })
    expect(resolve(ws, { id: 'i1' })).toEqual({ unit: 'i1', pick: null })
    expect(resolve(ws, { id: 'i1', task: '1' })).toEqual({ unit: 'i1', pick: { kind: 'task', id: '1', finding: undefined } })
    expect(resolve(ws, { id: 'i2', task: '1' })).toBeNull()
    expect(resolve(ws, { id: 'i2' })).toEqual({ unit: 'i2', pick: null })
    expect(resolve(ws, { id: 'u1', task: '9' })).toBeNull()
    expect(resolve(ws, { id: 'u9' })).toBeNull()
  })
})
