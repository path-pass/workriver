// A link opened while signed out (`/u1/3`) is where the sign-in from `workriver open` lands,
// once, and only within ten minutes; the one-time code leaves the address before anything else.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { rememberWanted, startSession } from './session'

const posted: string[] = []
beforeEach(() => {
  localStorage.clear()
  posted.length = 0
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    if (init?.method === 'POST') posted.push(`${url} ${init.body}`)
    return new Response(JSON.stringify({ user: true }), { status: 200 })
  }))
})
afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers() })

const at = (path: string) => history.replaceState(null, '', path)
const here = () => location.pathname + location.search + location.hash

describe('signing in lands where the user was', () => {
  it('a deep link opened while signed out is where the sign-in lands, once', async () => {
    at('/u1/3?finding=P2-1')
    rememberWanted()
    at('/#k=one-time')
    expect(await startSession()).toBe(true)
    expect(here()).toBe('/u1/3?finding=P2-1')
    expect(posted).toEqual(['/api/session {"code":"one-time"}'])
    at('/#k=again')
    await startSession()
    expect(here()).toBe('/')
  })

  it('not after ten minutes', async () => {
    vi.useFakeTimers({ toFake: ['Date'] })
    at('/u2')
    rememberWanted()
    vi.setSystemTime(Date.now() + 600_001)
    at('/#k=late')
    await startSession()
    expect(here()).toBe('/')
  })

  it('the root is not remembered, and a link opened elsewhere keeps its own address', async () => {
    at('/')
    rememberWanted()
    expect(localStorage.length).toBe(0)
    at('/u4')
    rememberWanted()
    at('/i3#k=code')
    await startSession()
    expect(here()).toBe('/i3')
  })

  it('only a path of this page is followed', async () => {
    localStorage.setItem('workriver.signin-to', JSON.stringify({ path: '//evil.example/x', at: Date.now() }))
    at('/#k=code')
    await startSession()
    expect(here()).toBe('/')
  })
})
