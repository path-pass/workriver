// Signed out, the page shows nothing of the app: one calm screen with the command that signs
// in, and the link that was opened is kept for the sign-in.
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const session = vi.hoisted(() => ({ answer: Promise.resolve<boolean>(false), remembered: 0 }))
vi.mock('@/api/session', async original => ({
  ...(await original<typeof import('@/api/session')>()),
  startSession: () => session.answer,
  rememberWanted: () => { session.remembered += 1 },
}))
const { Session } = await import('./Session')

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

let host: HTMLDivElement
beforeEach(() => { host = document.createElement('div'); document.body.append(host); session.remembered = 0 })
afterEach(() => host.remove())

async function shown(answer: Promise<boolean>) {
  session.answer = answer
  const root = createRoot(host)
  await act(async () => { root.render(<Session><p>the app</p></Session>) })
  await act(async () => { await answer.catch(() => undefined) })
  const text = host.textContent ?? ''
  act(() => root.unmount())
  return text
}

describe('the session gate', () => {
  it('signed out: only how to open it, never the app; the link opened is remembered', async () => {
    const text = await shown(Promise.resolve(false))
    expect(text).not.toContain('the app')
    expect(text).toContain('workriver open')
    expect(text).toContain('it opens only with a link from your own terminal')
    expect(session.remembered).toBe(1)
  })

  it('the service down: says so, and still shows nothing of the app', async () => {
    const text = await shown(Promise.reject(new Error('down')))
    expect(text).not.toContain('the app')
    expect(text).toContain('Cannot reach the workriver service')
    expect(text).not.toContain('workriver open')
  })

  it('signed in: the app', async () => {
    expect(await shown(Promise.resolve(true))).toBe('the app')
  })
})
