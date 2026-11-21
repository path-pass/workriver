// The tag field, mounted: a batch that would pass the limit is kept in the field and said, not
// cut; an Enter, comma or Backspace that an input method is composing with is the method's.
import { act, useState } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { TagInput } from './Sort'

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

let root: Root
let host: HTMLDivElement
let tags: string[]
beforeEach(() => { host = document.createElement('div'); document.body.append(host); root = createRoot(host) })
afterEach(() => { act(() => root.unmount()); host.remove() })

function Field({ start }: { start: string[] }) {
  const [value, setValue] = useState(start)
  const [text, setText] = useState('')
  tags = value
  return <TagInput value={value} onChange={setValue} text={text} onText={setText} offered={[]} />
}
const show = (start: string[]) => act(() => root.render(<Field start={start} />))
const input = () => host.querySelector('input')!
const alert = () => host.querySelector('[role=alert]')?.textContent ?? ''
/** Types into the field as the user does: React hears the native input event. */
function type(value: string) {
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!
  act(() => { set.call(input(), value); input().dispatchEvent(new Event('input', { bubbles: true })) })
}
/** A key pressed in the field, while an input method composes or not; whether it was taken. */
function press(key: string, isComposing = false) {
  const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, isComposing })
  act(() => { input().dispatchEvent(e) })
  return e.defaultPrevented
}

const seven = ['a', 'b', 'c', 'd', 'e', 'f', 'g']

describe('the tag field', () => {
  it('keeps a pasted batch that would pass eight in the field, and says so, adding none of it', () => {
    show(seven)
    type('ui, cli')
    press('Enter')
    expect(tags).toEqual(seven)
    expect(input().value).toBe('ui, cli')
    expect(input().disabled).toBe(false)
    expect(alert()).toContain('at most 8')
    // Leaving the field does the same; one fewer goes in, and the word goes.
    act(() => { input().focus(); input().blur() })
    expect(tags).toEqual(seven)
    type('ui')
    expect(alert()).toBe('')
    press('Enter')
    expect(tags).toEqual([...seven, 'ui'])
    expect(input().value).toBe('')
  })

  it('takes a batch that fits whole, repeats counted once', () => {
    show(seven.slice(0, 6))
    type('G, a, h')
    press(',')
    expect(tags).toEqual([...seven, 'h'])
  })

  it('leaves an Enter, comma or Backspace to the input method while it composes', () => {
    show(['ui'])
    type('にほんご')
    expect(press('Enter', true)).toBe(false)
    expect(press(',', true)).toBe(false)
    expect(tags).toEqual(['ui'])
    expect(input().value).toBe('にほんご')
    type('')
    expect(press('Backspace', true)).toBe(false)
    expect(tags).toEqual(['ui'])
    // The Enter that follows the confirmed text adds it.
    type('日本語')
    expect(press('Enter')).toBe(true)
    expect(tags).toEqual(['ui', '日本語'])
  })

  it('drops its word once the user removes a chip and the batch fits, and the batch then goes in', () => {
    show(seven)
    type('ui, cli')
    press('Enter')
    expect(alert()).toContain('at most 8')
    act(() => { (host.querySelector('button[aria-label="Remove a"]') as HTMLButtonElement).click() })
    expect(alert()).toBe('')
    expect(input().getAttribute('aria-invalid')).toBeNull()
    press('Enter')
    expect(tags).toEqual([...seven.slice(1), 'ui', 'cli'])
    expect(input().value).toBe('')
    expect(alert()).toBe('')
    expect(input().getAttribute('aria-invalid')).toBeNull()
  })
})
