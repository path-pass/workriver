// The Templates page: the library as the service gives it, one template in full, its text
// edited and the service's refusal shown with its line, a removal asked first, and a built-in's
// name given back to the built-in.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import type { Template } from './types'
import type { Workspace } from '@/workspace/types'

const api = vi.hoisted(() => ({
  saved: [] as string[],
  removed: [] as string[],
  save: (): Promise<void> => Promise.resolve(),
  text: (name: string): Promise<{ template: unknown; text: string }> => Promise.resolve({ template: {}, text: `name: ${name}\n# as saved\n` }),
}))
vi.mock('@/api/actions', async original => ({
  ...(await original<typeof import('@/api/actions')>()),
  templateText: (name: string) => api.text(name),
  saveTemplate: (text: string) => { api.saved.push(text); return api.save() },
  removeTemplate: (name: string) => { api.removed.push(name); return Promise.resolve() },
}))
const { TemplatesDialog, openTemplates } = await import('./Templates')

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

const development: Template = {
  name: 'development', unit: 'code', builtin: true, description: 'Work that lands in the repository.',
  steps: [
    { id: 'design', kind: 'design', description: 'Settle the shape of the work.' },
    { id: 'design-review', kind: 'review', reviewer: 'architecture-review', description: 'The design against the rules.' },
    { id: 'go', kind: 'gate', gate: 'decide', after: ['design', 'design-review'], description: "The user's go." },
  ],
}
const quick: Template = { name: 'quick', unit: 'code', revision: 7, at: '2026-11-23T00:00:00Z', description: 'A small fix.', steps: [] }
const mine: Template = { ...development, builtin: undefined, revision: 9, description: 'My development.' }
const ws = (templates: Template[]) => ({ intents: [], repos: [], sessions: [], builtAt: '', runtime: 'Runner', templates }) as unknown as Workspace

let root: Root
let host: HTMLDivElement
beforeEach(() => {
  host = document.createElement('div'); document.body.append(host); root = createRoot(host)
  api.saved = []; api.removed = []; api.save = () => Promise.resolve()
})
afterEach(() => { act(() => root.unmount()); host.remove() })

const settle = () => act(async () => { await new Promise(r => setTimeout(r)) })
const show = async (list: Template[], name?: string) => {
  act(() => root.render(<TooltipProvider><TemplatesDialog ws={ws(list)} /></TooltipProvider>))
  await act(async () => openTemplates(name))
  await settle()
}
// The dialog is portalled into the document's body.
const text = () => document.body.textContent ?? ''
const button = (name: string) => [...document.querySelectorAll('button')].find(b => b.textContent === name) as HTMLButtonElement
const press = async (name: string) => { await act(async () => { button(name).click() }); await settle() }
const area = () => document.querySelector<HTMLTextAreaElement>('textarea')!
function type(el: HTMLTextAreaElement, value: string) {
  const set = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), 'value')!.set!
  act(() => { set.call(el, value); el.dispatchEvent(new Event('input', { bubbles: true })) })
}

describe('Templates', () => {
  it('lists the library and shows one template in full', async () => {
    await show([quick, development], 'development')
    expect(text()).toContain('quick')
    expect(text()).toContain('code · yours')
    expect(text()).toContain('code · built in')
    // Each step: its id, its kind as the graph names it, what it follows, what it is for.
    expect(text()).toContain('designDesignfrom the start')
    expect(text()).toContain('design-reviewArchitecture reviewafter design')
    expect(text()).toContain('goConfirm designafter design, design-review')
    expect(text()).toContain('A change touches only units made after it.')
    // A built-in is edited (saved as the user's), never removed.
    expect(button('Edit')).toBeTruthy()
    expect(button('Remove')).toBeUndefined()
    expect(button('Back to the built-in')).toBeUndefined()
  })

  it('edits the text as saved, and shows the refusal with its line', async () => {
    await show([quick], 'quick')
    await press('Edit')
    expect(area().value).toBe('name: quick\n# as saved\n')
    api.save = () => Promise.reject(new Error('line 3: kind "build" is not one of implement, review'))
    type(area(), 'name: quick\nsteps:\n  - kind: build\n')
    await press('Save')
    expect(document.querySelector('[role=alert]')?.textContent).toBe('Not saved: line 3: kind "build" is not one of implement, review')
    expect(api.saved).toEqual(['name: quick\nsteps:\n  - kind: build\n'])
    api.save = () => Promise.resolve()
    await press('Save')
    expect(document.querySelector('textarea')).toBeNull()
  })

  it('asks before removing one of the user’s, and gives a built-in’s name back to the built-in', async () => {
    await show([quick, mine], 'quick')
    await press('Remove')
    expect(text()).toContain('Remove template quick? Units made with it keep their steps.')
    expect(api.removed).toEqual([])
    const ask = document.querySelector('[role=alertdialog]')!
    await act(async () => { [...ask.querySelectorAll('button')].find(b => b.textContent === 'Remove')!.click() })
    await settle()
    expect(api.removed).toEqual(['quick'])
    await act(async () => { [...document.querySelectorAll('button')].find(b => b.textContent?.startsWith('development'))!.click() })
    await settle()
    await press('Back to the built-in')
    expect(text()).toContain('Remove your development and go back to the built-in one?')
  })

  it('starts a new template from a sample text', async () => {
    await show([development])
    await press('New template')
    expect(area().value).toMatch(/^name: my-flow\nunit: code\n/)
  })
})
