// Adding a project from the page: a folder not given whole is refused before anything is
// sent; the service's refusal shows in its words; added, the dialog closes.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const api = vi.hoisted(() => ({ sent: [] as [string, string][], answer: (): Promise<void> => Promise.resolve() }))
vi.mock('@/api/actions', async original => ({
  ...(await original<typeof import('@/api/actions')>()),
  addProject: (path: string, title: string) => { api.sent.push([path, title]); return api.answer() },
}))
const { AddProjectDialog, openAddProject, wholePath } = await import('./AddProject')

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

let root: Root
let host: HTMLDivElement
beforeEach(() => { host = document.createElement('div'); document.body.append(host); root = createRoot(host); api.sent = []; api.answer = () => Promise.resolve() })
afterEach(() => { act(() => root.unmount()); host.remove() })

const open = async () => { act(() => root.render(<AddProjectDialog />)); await act(async () => openAddProject()) }
const fields = () => [...document.querySelectorAll<HTMLInputElement>('[role=dialog] input')]
function type(el: HTMLInputElement, value: string) {
  const set = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), 'value')!.set!
  act(() => { set.call(el, value); el.dispatchEvent(new Event('input', { bubbles: true })) })
}
const add = async () => {
  const b = [...document.querySelectorAll<HTMLButtonElement>('[role=dialog] button')].find(x => x.textContent === 'Add project')!
  await act(async () => { b.click() })
  await act(async () => { await new Promise(r => setTimeout(r)) })
}
const alert = () => document.querySelector('[role=dialog] [role=alert]')?.textContent

describe('adding a project', () => {
  it('takes a path from / or ~/ only', () => {
    expect(['/a/b', '~/code/x', '/'].every(wholePath)).toBe(true)
    expect(['.', 'foo', '../x', '~', '~name', ' /a'].some(wholePath)).toBe(false)
  })

  it('refuses a folder not given whole before sending it', async () => {
    await open()
    type(fields()[0], 'code/web')
    await add()
    expect(api.sent).toEqual([])
    expect(alert()).toBe('Give the folder’s whole path, from / or ~/.')
  })

  it('sends the folder and the title, and closes once added', async () => {
    await open()
    type(fields()[0], ' ~/code/web ')
    type(fields()[1], 'Web')
    await add()
    expect(api.sent).toEqual([['~/code/web', 'Web']])
    expect(document.querySelector('[role=dialog]')).toBeNull()
  })

  it('sends an empty title for the service to name it, and shows its refusal in its words', async () => {
    api.answer = () => Promise.reject(new Error('/a/b is inside project p1 (/a): a project holds no other'))
    await open()
    type(fields()[0], '/a/b')
    await add()
    expect(api.sent).toEqual([['/a/b', '']])
    expect(alert()).toBe('Not added: /a/b is inside project p1 (/a): a project holds no other')
    expect(document.querySelector('[role=dialog]')).not.toBeNull()
  })
})
