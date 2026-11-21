// The draggable edge, as the Activity graph's bottom edge uses it: with no height set the
// panel sizes itself; a drag or a key sets one, between the least and the most, and it is
// remembered; double-click goes back to sizing itself.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { useResize } from './useResize'

const KEY = 'test.height', VAR = '--test-h'
let seen = 0

function Panel({ auto = 300 }: { auto?: number }) {
  const size = useResize({ key: KEY, variable: VAR, min: 160, max: 800, edge: 'bottom' })
  seen = size.size
  return (
    <div id="panel" ref={el => { if (el) Object.defineProperty(el, 'offsetHeight', { configurable: true, get: () => auto }) }}>
      <div id="edge" {...size.handle} />
    </div>
  )
}

let root: Root, host: HTMLDivElement
const edge = () => host.querySelector<HTMLElement>('#edge')!
const shown = () => document.documentElement.style.getPropertyValue(VAR)
const show = (auto?: number) => act(() => root.render(<Panel auto={auto} />))
const pointer = (type: string, y: number) => act(() => { edge().dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, clientY: y, pointerId: 1 })) })
const key = (k: string) => act(() => { edge().dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: k })) })

beforeEach(() => {
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  // happy-dom has no pointer capture to speak of.
  HTMLElement.prototype.setPointerCapture = () => {}
})
afterEach(() => {
  act(() => root.unmount())
  host.remove()
  localStorage.clear()
  document.documentElement.style.removeProperty(VAR)
})

describe('a height the user drags', () => {
  it('is automatic until set: nothing remembered, no height imposed', () => {
    show()
    expect(seen).toBe(0)
    expect(shown()).toBe('')
    expect(edge().getAttribute('aria-orientation')).toBe('horizontal')
  })

  it('starts a drag from the height the panel has, follows it down, and is remembered', () => {
    show(300)
    pointer('pointerdown', 500)
    pointer('pointermove', 620)
    expect(shown()).toBe('420px')
    pointer('pointerup', 620)
    expect(seen).toBe(420)
    expect(localStorage.getItem(KEY)).toBe('420')
  })

  it('stays between the least and the most', () => {
    show(300)
    pointer('pointerdown', 500)
    pointer('pointermove', 100)
    expect(shown()).toBe('160px')
    pointer('pointermove', 5000)
    pointer('pointerup', 5000)
    expect(seen).toBe(800)
  })

  it('moves with the arrow keys from the automatic height, Home and End to the ends', () => {
    show(300)
    key('ArrowDown')
    expect(seen).toBe(316)
    key('ArrowUp')
    key('ArrowUp')
    expect(seen).toBe(284)
    key('End')
    expect(seen).toBe(800)
    key('Home')
    expect(seen).toBe(160)
  })

  it('comes back after a reload, clamped to the most', () => {
    localStorage.setItem(KEY, '5000')
    show()
    expect(seen).toBe(800)
    expect(shown()).toBe('800px')
  })

  it('goes back to automatic on a double-click, and forgets the height', () => {
    localStorage.setItem(KEY, '400')
    show()
    expect(shown()).toBe('400px')
    act(() => { edge().dispatchEvent(new MouseEvent('dblclick', { bubbles: true })) })
    expect(seen).toBe(0)
    expect(shown()).toBe('')
    expect(localStorage.getItem(KEY)).toBeNull()
  })
})
