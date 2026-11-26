import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { CloseDialog, ClosedReason } from './Close'

globalThis.IS_REACT_ACT_ENVIRONMENT = true

let root: Root
let host: HTMLDivElement
const fetchClose = vi.fn()
beforeEach(() => {
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  fetchClose.mockReset()
  vi.stubGlobal('fetch', fetchClose)
})
afterEach(() => { act(() => root.unmount()); host.remove(); vi.unstubAllGlobals() })

const dialog = () => document.querySelector('[role=dialog]')
const button = (text: string) => [...dialog()!.querySelectorAll('button')].find(b => b.textContent === text)!
async function open(kind: 'unit' | 'intent', id: string, advice?: string) {
  act(() => root.render(<CloseDialog kind={kind} id={id} available advice={advice} />))
  await act(async () => host.querySelector('button')!.click())
}

const input = () => dialog()!.querySelector('input')!
function type(value: string) {
  const field = input()
  const set = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(field), 'value')!.set!
  act(() => { set.call(field, value); field.dispatchEvent(new Event('input', { bubbles: true })) })
}

describe('closing from the page', () => {
  it('requires a reason and keeps the CLI warning visible after the unit refreshes as closed', async () => {
    await open('unit', 'u1', 'The branch stays as it is.')
    expect(dialog()?.textContent).toContain('The branch stays as it is.')
    expect(button('Close unit').disabled).toBe(true)
    type('  ')
    expect(button('Close unit').disabled).toBe(true)
    expect(fetchClose).not.toHaveBeenCalled()
    let respond!: (value: Response) => void
    fetchClose.mockReturnValue(new Promise<Response>(resolve => { respond = resolve }))
    type('  Experiment ended  ')
    await act(async () => button('Close unit').click())
    expect(fetchClose).toHaveBeenCalledOnce()
    const [path, request] = fetchClose.mock.calls[0]
    expect(path).toBe('/api/close')
    expect(JSON.parse(request.body)).toEqual({ kind: 'unit', id: 'u1', reason: 'Experiment ended' })
    expect(button('Close unit').disabled).toBe(true)
    // The stream can refresh the workspace before the response comes back.
    act(() => root.render(<CloseDialog kind="unit" id="u1" available={false} />))
    await act(async () => respond(Response.json({ warning: 'Steps of template development not done.' })))
    expect(dialog()?.textContent).toContain('Steps of template development not done.')
    expect(dialog()?.querySelector('input')).toBeNull()
    await act(async () => button('Close').click())
    expect(dialog()).toBeNull()
    expect(host.querySelector('button')).toBeNull()
    // Adding another task to the unit reopens it and makes a second close available.
    act(() => root.render(<CloseDialog kind="unit" id="u1" available />))
    await act(async () => host.querySelector('button')!.click())
    expect(input().value).toBe('')
    expect(dialog()?.textContent).not.toContain('Steps of template development not done.')
  })

  it('shows the rule’s refusal in place and keeps the reason for a retry', async () => {
    fetchClose.mockResolvedValueOnce(new Response('intent i1 is done; it cannot become done', { status: 422 }))
    await open('intent', 'i1')
    type('Goal changed')
    await act(async () => button('Close intent').click())
    const [path, request] = fetchClose.mock.calls[0]
    expect(path).toBe('/api/close')
    expect(JSON.parse(request.body)).toEqual({ kind: 'intent', id: 'i1', reason: 'Goal changed' })
    expect(dialog()?.querySelector('[role=alert]')?.textContent).toContain('intent i1 is done; it cannot become done')
    expect(input().value).toBe('Goal changed')
    fetchClose.mockResolvedValueOnce(Response.json({}))
    await act(async () => button('Close intent').click())
    expect(dialog()).toBeNull()
  })

  it('shows a recorded closure reason as text', () => {
    act(() => root.render(<ClosedReason reason="Track <build> separately" />))
    expect(host.textContent).toContain('Track <build> separately')
    expect(host.querySelector('build')).toBeNull()
  })

  it('refuses line breaks a single-line input can retain', async () => {
    await open('unit', 'u1')
    for (const reason of ['first\vsecond', 'first\fsecond', 'first\u2028second']) {
      type(reason)
      expect(button('Close unit').disabled).toBe(true)
    }
    expect(fetchClose).not.toHaveBeenCalled()
  })
})
