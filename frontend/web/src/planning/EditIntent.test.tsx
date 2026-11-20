// The edit dialog, mounted: it saves what the user changed against the text it opened with,
// whatever the page shows meanwhile, and before saving anyway it shows every field that
// changed since, not only the first the service names.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Intent, IntentText } from './types'

const api = vi.hoisted(() => ({
  sent: [] as Record<string, unknown>[],
  refuse: [] as string[],
  versions: {} as Record<number, IntentText>,
}))
vi.mock('@/api/actions', async original => ({
  ...(await original<typeof import('@/api/actions')>()),
  editIntent: async (_: Intent, body: Record<string, unknown>) => {
    api.sent.push(body)
    const no = api.refuse.shift()
    if (no) throw new Error(`/api/append: 422 ${no}`)
  },
  intentVersion: async (_: Intent, n: number) => ({ text: api.versions[n], before: null }),
}))
const { EditIntentDialog } = await import('./EditIntent')
const { draftOf } = await import('./edit')

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

const v1: Intent = { id: 'i1', title: 'Chat', goal: 'Talk', note: 'First', units: [], chores: [], version: 1 }
let root: Root
let host: HTMLDivElement
beforeEach(() => { host = document.createElement('div'); document.body.append(host); root = createRoot(host); api.sent = []; api.refuse = []; api.versions = {} })
afterEach(() => { act(() => root.unmount()); host.remove() })

const show = (intent: Intent) => act(() => root.render(<EditIntentDialog intent={intent} draft={draftOf(v1)} onClose={() => {}} />))
const fields = () => [...document.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>('[role=dialog] input, [role=dialog] textarea')]
/** Types into a field as the user does: React hears the native input event. */
function type(el: HTMLInputElement | HTMLTextAreaElement, value: string) {
  const set = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), 'value')!.set!
  act(() => { set.call(el, value); el.dispatchEvent(new Event('input', { bubbles: true })) })
}
const button = (name: string) => [...document.querySelectorAll('[role=dialog] button')].find(b => b.textContent === name) as HTMLButtonElement
const click = async (name: string) => { await act(async () => { button(name).click() }); await act(async () => { await new Promise(r => setTimeout(r)) }) }

describe('the edit dialog', () => {
  it('saves only what the user changed, from the version it opened with, when the page shows a newer one meanwhile', async () => {
    await show(v1)
    type(fields()[0], 'Chat v2')
    // A manager changed the note; the page reads the workspace again.
    await show({ ...v1, note: 'The manager’s note', version: 2 })
    await click('Save')
    expect(api.sent).toEqual([{ intent: 'i1', title: 'Chat v2', base: 1 }])
  })

  it('before saving anyway, shows every field it sends that changed since, not only the one named', async () => {
    await show(v1)
    type(fields()[0], 'Chat v2')
    type(fields()[2], 'Mine')
    api.refuse.push("i1's title changed since v1 (in v2; it is at v3, last by the user at x)")
    api.versions[3] = { title: 'Chat 2', goal: 'Talk', note: 'Theirs' }
    await click('Save')
    const said = document.querySelector('[role=dialog] [role=alert]')?.textContent ?? ''
    expect(said).toContain('The title was changed meanwhile (it is at v3 now)')
    expect(said).toContain('The note was changed meanwhile (it is at v3 now)')
    expect(said).not.toContain('The goal was changed')
    await click('Save anyway')
    expect(api.sent[1]).toEqual({ intent: 'i1', title: 'Chat v2', note: 'Mine', base: 3 })
  })
})
