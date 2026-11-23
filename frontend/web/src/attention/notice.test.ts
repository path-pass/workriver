// The browser delivers workriver notifications only when the user has enabled them.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { notify } from '@/api/actions'

describe('a browser notification', () => {
  const made: { title: string; options: NotificationOptions }[] = []
  beforeEach(() => {
    made.length = 0
    vi.stubGlobal('Notification', Object.assign(function (this: object, title: string, options: NotificationOptions) { made.push({ title, options }) }, { permission: 'granted' }))
    localStorage.setItem('workriver.notify', '1')
  })
  afterEach(() => { vi.unstubAllGlobals(); localStorage.clear() })

  it('uses the browser’s default persistence for a machine alert', () => {
    notify('This machine: Swap 5.2 GB', 'Warning: over 4 GB', 'alert:box:swap:0:warn')
    expect(made).toEqual([{ title: 'This machine: Swap 5.2 GB', options: { body: 'Warning: over 4 GB', tag: 'workriver:alert:box:swap:0:warn' } }])
  })

  it('is not shown while the user has not asked for it', () => {
    localStorage.setItem('workriver.notify', '0')
    notify('ja-impl is waiting for you', undefined, 'agent:s1')
    expect(made).toEqual([])
  })
})
