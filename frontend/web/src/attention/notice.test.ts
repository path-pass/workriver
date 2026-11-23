// What comes into Blocking is told outside the page too (if the user turned notifications on):
// one notification per row as it comes, never again while it stays, and one that stays on
// screen until clicked.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import '@/i18n'
import i18n from 'i18next'
import { notify } from '@/api/actions'
import type { Intent, Unit } from '@/planning/types'
import type { Task } from '@/work/types'
import { blockingNews, noticeOf } from './notice'
import type { InboxRow } from './select'

const t = i18n.getFixedT('en')
const waited = () => '12m'
const intent = { id: 'i1', title: 'Hardening' } as Intent
const unit = { id: 'u1', title: 'Login' } as Unit
const node = (id: string, title: string, more: Partial<Task> = {}) => ({ id, taskId: id, title, ...more }) as Task
const base = { intent, unit, pick: null, chore: null, since: 0 }
const agent: InboxRow = { ...base, key: 'agent:s1', kind: 'agent', unit: null, node: null, agent: { name: 'ja-impl', session: 's1', topic: 'Japanese copy', place: 'Hardening' } }
const gate: InboxRow = { ...base, key: 'gate:u1:2', kind: 'gate', node: node('2', 'Try the login'), pick: { kind: 'task', id: '2' } }
const stalled: InboxRow = { ...base, key: 'stalled:u1:3', kind: 'stalled', node: node('3', 'Implement', { agent: 'impl', stalled: { since: '2026-11-23T08:00:00Z', escalated: true } } as Partial<Task>), pick: { kind: 'task', id: '3' } }

describe('what is news in Blocking', () => {
  it('is nothing in the rows there when the page loads', () => {
    const news = blockingNews()
    expect(news([agent, gate])).toEqual([])
  })

  it('is each row that comes, once: an agent waiting, a gate waiting, a stalled worker', () => {
    const news = blockingNews()
    news([])
    expect(news([agent])).toEqual([agent])
    expect(news([agent, gate, stalled])).toEqual([gate, stalled])
    // Staying (or only changing) is not news again.
    expect(news([agent, gate, stalled])).toEqual([])
    expect(news([{ ...agent, since: 5 }, gate, stalled])).toEqual([])
  })

  it('is a row again once it left and came back (the agent asks again)', () => {
    const news = blockingNews()
    news([agent])
    expect(news([])).toEqual([])
    expect(news([agent])).toEqual([agent])
  })
})

describe('a notification of a row of Blocking', () => {
  it('names the agent waiting, with its topic', () => {
    expect(noticeOf(t, waited, agent)).toEqual({ title: 'ja-impl is waiting for you', body: 'Japanese copy' })
  })

  it('names the gate, and where it is', () => {
    expect(noticeOf(t, waited, gate)).toEqual({ title: 'A gate is waiting for you: Try the login', body: 'Hardening / Login' })
  })

  it('says the worker stopped, and where', () => {
    expect(noticeOf(t, waited, stalled)).toEqual({ title: 'u1#3: impl stopped 12m ago without delivering', body: 'Hardening / Login' })
    const gone = { ...stalled, node: { ...stalled.node!, stalled: { ...stalled.node!.stalled!, gone: true } } } as InboxRow
    expect(noticeOf(t, waited, gone).title).toBe('u1#3: impl is gone')
  })
})

describe('a browser notification', () => {
  const made: { title: string; options: NotificationOptions }[] = []
  beforeEach(() => {
    made.length = 0
    vi.stubGlobal('Notification', Object.assign(function (this: object, title: string, options: NotificationOptions) { made.push({ title, options }) }, { permission: 'granted' }))
    localStorage.setItem('workriver.notify', '1')
  })
  afterEach(() => { vi.unstubAllGlobals(); localStorage.clear() })

  it('stays until clicked, one per thing it is about', () => {
    notify('ja-impl is waiting for you', 'Japanese copy', 'agent:s1')
    expect(made).toEqual([{ title: 'ja-impl is waiting for you', options: { body: 'Japanese copy', tag: 'workriver:agent:s1', requireInteraction: true } }])
  })

  it('is not shown while the user has not asked for it', () => {
    localStorage.setItem('workriver.notify', '0')
    notify('ja-impl is waiting for you', undefined, 'agent:s1')
    expect(made).toEqual([])
  })
})
