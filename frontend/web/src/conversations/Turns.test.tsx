// A manager's question ends with workriver's Other: picked, it needs the note, which is sent
// as the answer's text with the value other.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { en } from '@/i18n/en'
import '@/i18n'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import { MessageView } from './Turns'
import type { Answer, ManagerMessage } from './types'

const choice = { surface: { version: '1.0', createSurface: { components: [{}, { component: 'ChoicePicker', options: [{ value: 'cursor', label: 'By cursor' }, { value: 'page', label: 'By page' }] }] } } }
const m = { id: 'm1', workspace: 'chat', manager: 'boss', text: 'Which paging?', at: '2026-11-22T10:00:00Z', step: 'delivered', replies: [], byManager: true, choice, open: true } as unknown as ManagerMessage

let root: Root
let el: HTMLDivElement
beforeEach(() => { el = document.createElement('div'); document.body.append(el); root = createRoot(el) })
afterEach(() => { act(() => root.unmount()); el.remove() })

describe('a manager’s question answered with Other', () => {
  it('needs the note, then sends other with it', () => {
    const sent: [Answer, string][] = []
    act(() => root.render(<TooltipProvider><MessageView m={m} onAnswer={(a, note) => sent.push([a, note])} asked={() => undefined} /></TooltipProvider>))
    expect([...el.querySelectorAll('[data-slot=radio-group-item]')].map(r => r.getAttribute('value'))).toEqual(['cursor', 'page', 'other'])
    act(() => el.querySelector<HTMLButtonElement>('[data-slot=radio-group-item][value=other]')!.click())
    const note = el.querySelector('input')!
    expect([note.getAttribute('placeholder'), note.getAttribute('aria-required')]).toEqual([en.choice.otherNote, 'true'])
    const answer = () => act(() => [...el.querySelectorAll('button')].find(b => b.textContent === en.choice.answer)!.click())
    answer()
    expect([sent, el.querySelector('[role=alert]')?.textContent]).toEqual([[], en.choice.otherNeedNote])
    // A note of only invisible marks (a pasted zero-width space) shows nothing: no note.
    const type = (text: string) => act(() => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(note, text)
      note.dispatchEvent(new Event('input', { bubbles: true }))
    })
    type('\u200b \ufeff')
    answer()
    expect([sent, el.querySelector('[role=alert]')?.textContent]).toEqual([[], en.choice.otherNeedNote])
    act(() => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(note, 'Keyset paging')
      note.dispatchEvent(new Event('input', { bubbles: true }))
    })
    answer()
    expect(sent).toEqual([[{ message: 'm1', selected: ['other'] }, 'Keyset paging']])
  })
})

it('rates the original broadcast reply, saves an optional note, changes and clears the vote', async () => {
  const post = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(null, { status: 204 }))
  try {
    const reply = { by: 'boss', text: 'Reply', at: m.at, reply: 2 }
    const broadcast: ManagerMessage = { ...m, byManager: false, choice: undefined, to: [{ workspace: 'chat', manager: 'boss', step: 'delivered', replies: [reply] }] }
    act(() => root.render(<TooltipProvider><MessageView m={broadcast} onAnswer={() => undefined} asked={() => undefined} /></TooltipProvider>))
    const button = (name: string) => el.querySelector<HTMLButtonElement>(`button[aria-label="${name}"]`)!
    const click = async (name: string) => act(async () => button(name).click())
    const lastBody = () => JSON.parse(String(post.mock.lastCall?.[1]?.body))
    const up = 'Thumbs up'
    const down = 'Thumbs down'

    expect(button(down)?.getAttribute('aria-pressed')).toBe('false')
    await click(down)
    expect(lastBody()).toEqual({ message: 'm1', reply: 2, vote: 'down' })
    expect(button(down).getAttribute('aria-pressed')).toBe('true')
    const note = el.querySelector('input')!
    act(() => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(note, 'off track')
      note.dispatchEvent(new Event('input', { bubbles: true }))
    })
    await act(async () => el.querySelector<HTMLButtonElement>('button[type=submit]')!.click())
    expect(lastBody()).toEqual({ message: 'm1', reply: 2, vote: 'down', note: 'off track' })
    await click(up)
    expect(lastBody()).toEqual({ message: 'm1', reply: 2, vote: 'up' })
    expect(button(up).getAttribute('aria-pressed')).toBe('true')
    expect(button(down).getAttribute('aria-pressed')).toBe('false')
    await click(up)
    expect(lastBody()).toEqual({ message: 'm1', reply: 2, vote: null })
    expect(button(up).getAttribute('aria-pressed')).toBe('false')
  } finally {
    post.mockRestore()
  }
})
