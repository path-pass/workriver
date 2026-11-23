// The Manager section's conversation with every manager: picked in the workspace select, it
// shows a message to every manager once, each copy where it is and each manager's replies
// labelled by its workspace (its name and id); what is written there goes to every manager, with no chips.
// Needs you counts those replies under the same conversation.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import { inbox } from '@/attention/select'
import type { Space } from '@/agents/types'
import { Spaces } from '@/planning/spaces'
import type { Unit } from '@/planning/types'
import type { Workspace } from '@/workspace/types'
import { Manager } from './Manager'
import { EVERY, repliesOf } from './select'
import type { ManagerMessage, Talk } from './types'

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

// Three workspaces, each with its manager running: ws1 "chat", ws2 "web", ws3 "api" (its manager on box).
const space = (id: string, name: string, agent: string, machine?: string): Space => ({ id, name, project: 'p', manager: { session: `s-${agent}`, name: agent, machine, folder: '/w', since: 1, state: 'idle' } })
const sessions = [space('ws1', 'chat', 'boss'), space('ws2', 'web', 'web-mgr'), space('ws3', 'api', 'far', 'box')]
const reply = (by: string, text: string, at: string) => ({ by, text, at, unread: true })
const broadcast: ManagerMessage = {
  id: 'm2', workspace: EVERY, manager: '', text: 'I shut down at six.', at: '2026-11-21T10:00:00Z', step: 'sending', replies: [],
  to: [
    { workspace: 'ws1', manager: 'boss', step: 'delivered', replies: [reply('boss', 'chat: nothing running.', '2026-11-21T10:01:00Z')] },
    { workspace: 'ws2', manager: 'web-mgr', step: 'held', reason: 'the manager web-mgr is on a blind task', replies: [] },
    { workspace: 'ws3', machine: 'box', manager: 'far', step: 'delivered', replies: [reply('far', 'api: two worktrees removed.', '2026-11-21T10:02:00Z')] },
  ],
}
const own: ManagerMessage = { id: 'm1', workspace: 'ws1', manager: 'boss', text: 'Only to chat.', at: '2026-11-21T09:00:00Z', step: 'delivered', replies: [] }

let root: Root
let host: HTMLDivElement
let told: Parameters<Talk['onTell']>[]
beforeEach(() => { host = document.createElement('div'); document.body.append(host); root = createRoot(host); told = [] })
afterEach(() => { act(() => root.unmount()); host.remove() })

function show(to: string, messages: ManagerMessage[], live = sessions) {
  const talk: Talk = {
    sessions: [], spaces: live, projects: [], file: null, lines: null, to, onPick: () => {}, onRead: () => {}, onFile: () => {}, focusAt: 0, onOpen: () => {}, extra: [],
    intents: [], messages, onTell: (...a) => { told.push(a) }, onDiscuss: async () => {}, onDiscussRead: () => {}, onAnswer: () => {},
  }
  const unit = { id: 'u1', title: 'Search' } as unknown as Unit
  act(() => root.render(<TooltipProvider><Spaces.Provider value={live}><Manager unit={unit} n={null} talk={talk} /></Spaces.Provider></TooltipProvider>))
}
const texts = (sel: string) => [...host.querySelectorAll(sel)].map(e => e.textContent)

describe('the conversation with every manager', () => {
  it('shows a message to every manager once, each copy where it is, each reply labelled by its workspace', () => {
    show(EVERY, [own, broadcast])
    expect(host.querySelector('[aria-label="Workspace"]')?.textContent).toBe('All managers')
    expect(texts('.msg[data-side="mine"] .bubble > p:first-child')).toEqual(['I shut down at six.'])
    expect(texts('[aria-label="Where each manager’s copy is"] li')).toEqual([
      'chatws1In boss’s terminal',
      'webws2Waiting: web-mgr is on a blind task; it goes in once that task is delivered',
      'api on boxws3In far’s terminal',
    ])
    expect(texts('.msg[data-side="theirs"] .msg-head').map(t => t?.replace(/\d+\w? ago|just now|\d+[smhd]$/, ''))).toEqual([
      expect.stringContaining('bosschat'),
      expect.stringContaining('farapi on box'),
    ])
    // Each reply names its workspace by id too (two workspaces may share a name).
    expect(texts('.msg[data-side="theirs"] .msg-head .font-mono')).toEqual(['ws1', 'ws3'])
    expect(texts('.msg[data-side="theirs"] .bubble')).toEqual(['chat: nothing running.', 'api: two worktrees removed.'])
  })

  it('sends to every manager, with no chips of what is on screen', () => {
    show(EVERY, [])
    expect(host.textContent).toContain('Write once to every manager running')
    const box = host.querySelector('textarea')!
    const set = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!
    act(() => { set.call(box, 'Clean up your worktrees.'); box.dispatchEvent(new Event('input', { bubbles: true })) })
    act(() => { (host.querySelector('button[aria-label="Send"]') as HTMLButtonElement).click() })
    expect(told).toEqual([[{ workspace: EVERY, all: true }, 'Clean up your worktrees.', '', { unit: '', task: 0, context: [] }, []]])
    expect(host.textContent).toContain('Typed into the terminals of the 3 managers running')
  })

  it('keeps All managers picked while managers close, and the draft when none runs', () => {
    show(EVERY, [], sessions.slice(0, 2))
    const box = host.querySelector('textarea')!
    const set = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!
    act(() => { set.call(box, 'Maintenance in ten minutes.'); box.dispatchEvent(new Event('input', { bubbles: true })) })
    // One closes: still to every manager, not to the one left.
    show(EVERY, [], sessions.slice(0, 1))
    expect(host.querySelector('[aria-label="Workspace"]')?.textContent).toBe('All managers')
    // None runs: nothing can be sent, and the draft stays.
    show(EVERY, [], [])
    expect(host.textContent).toContain('No manager is running: nobody would read it.')
    expect(host.querySelector('textarea')!.value).toBe('Maintenance in ten minutes.')
    expect((host.querySelector('button[aria-label="Send"]') as HTMLButtonElement).disabled).toBe(true)
    show(EVERY, [], sessions.slice(0, 1))
    act(() => { (host.querySelector('button[aria-label="Send"]') as HTMLButtonElement).click() })
    expect(told).toEqual([[{ workspace: EVERY, all: true }, 'Maintenance in ten minutes.', '', { unit: '', task: 0, context: [] }, []]])
  })

  it('keeps a workspace’s own conversation apart', () => {
    show('ws1', [own, broadcast])
    expect(texts('.msg[data-side="mine"] .bubble > p:first-child')).toEqual(['Only to chat.'])
  })
})

describe('Needs you', () => {
  it('counts the replies to a message to every manager under that conversation', () => {
    expect(repliesOf(broadcast).map(r => r.by)).toEqual(['boss', 'far'])
    const ws = { intents: [], sessions: [], messages: [own, broadcast], chores: [], alerts: [], backlog: [] } as unknown as Workspace
    const rows = inbox(ws).messages
    expect(rows.map(r => [r.talk?.workspace, r.talk?.count, r.talk?.manager])).toEqual([[EVERY, 2, 'far']])
  })
})

describe('the manager’s state above the input', () => {
  const line = () => host.querySelector('p[data-manager]')

  it.each([
    ['idle', 'Manager idle'],
    ['done', 'Manager done'],
    ['blocked', 'Manager blocked'],
    ['gone', 'Manager not running'],
  ] as const)('shows the workspace manager’s %s state', (state, words) => {
    show('ws1', [], [{ ...sessions[0], manager: { ...sessions[0].manager!, state } }])
    expect(line()?.textContent).toBe(words)
  })

  it('shows and updates the selected manager’s working time above the input', () => {
    vi.useFakeTimers()
    try {
      const workingSince = Date.now() - 3 * 60_000
      show('ws2', [], [sessions[0], { ...sessions[1], manager: { ...sessions[1].manager!, state: 'working', workingSince } }])
      expect(line()?.textContent).toBe('Manager working… 3m')
      expect(line()!.compareDocumentPosition(host.querySelector('textarea')!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
      act(() => { vi.advanceTimersByTime(60_000) })
      expect(line()?.textContent).toBe('Manager working… 4m')
    } finally {
      vi.useRealTimers()
    }
  })
})

describe('the box', () => {
  const type = (text: string) => {
    const box = host.querySelector('textarea')!
    const set = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!
    act(() => { set.call(box, text); box.dispatchEvent(new Event('input', { bubbles: true })) })
    return box
  }
  /** Presses Enter in the box; whether the browser's own action (a new line) was let through. */
  const enter = (box: HTMLTextAreaElement, keys: KeyboardEventInit = {}) => {
    let free = true
    act(() => { free = box.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true, ...keys })) })
    return free
  }
  const platform = (name: string) => Object.defineProperty(navigator, 'platform', { value: name, configurable: true })
  afterEach(() => platform(''))

  it('sends with Cmd+Enter or Ctrl+Enter', () => {
    show('ws1', [])
    expect(enter(type('First.'), { metaKey: true })).toBe(false)
    expect(enter(type('Second.'), { ctrlKey: true })).toBe(false)
    expect(told.map(a => a[1])).toEqual(['First.', 'Second.'])
    expect(host.querySelector('textarea')!.value).toBe('')
  })

  it('breaks the line with Enter and Shift+Enter, sending nothing', () => {
    show('ws1', [])
    const box = type('Two lines')
    expect(enter(box)).toBe(true)
    expect(enter(box, { shiftKey: true })).toBe(true)
    expect(told).toEqual([])
    expect(box.value).toBe('Two lines')
  })

  it('sends nothing while an input method composes', () => {
    show('ws1', [])
    const box = type('にほんご')
    expect(enter(box, { metaKey: true, isComposing: true })).toBe(true)
    expect(enter(box, { ctrlKey: true, isComposing: true })).toBe(true)
    expect(told).toEqual([])
  })

  it('never sends on the key that confirms an input method in Safari (keyCode 229, not composing)', () => {
    show('ws1', [])
    const box = type('にほんご')
    expect(enter(box, { metaKey: true, keyCode: 229 })).toBe(true)
    expect(enter(box, { ctrlKey: true, keyCode: 229 })).toBe(true)
    expect(told).toEqual([])
  })

  it('says under the box which keys send, as the platform names them', () => {
    platform('MacIntel')
    show('ws1', [])
    expect(host.textContent).toContain('⌘↵ sends')
    act(() => root.unmount()); root = createRoot(host)
    platform('Win32')
    show('ws1', [])
    expect(host.textContent).toContain('Ctrl+Enter sends')
  })
})

describe('a line of workriver’s own', () => {
  const back: ManagerMessage = { id: 'm3', workspace: 'ws1', manager: 'boss', notice: 'manager-back', text: '', at: '2026-11-21T09:30:00Z', step: 'delivered', replies: [] }

  it('is a system line in the conversation, nobody’s bubble', () => {
    show('ws1', [own, back])
    const line = host.querySelector('.chat-note')!
    expect(line.getAttribute('data-notice')).toBe('manager-back')
    expect(line.textContent).toContain('boss is back in its terminal and is the manager again')
    expect(line.closest('.msg')).toBeNull()
    expect(host.querySelectorAll('.msg')).toHaveLength(1)
  })

  it('is never unread: Needs you has no row for it', () => {
    const ws = { intents: [], sessions: [], messages: [back], chores: [], alerts: [], backlog: [] } as unknown as Workspace
    expect(inbox(ws).messages).toEqual([])
  })
})

describe('a workspace whose seat is empty', () => {
  it('still shows its history, and nothing can be sent to it', () => {
    const empty: Space = { id: 'ws1', name: 'chat', project: 'p' }
    show('ws1', [own], [empty, sessions[1]])
    expect(texts('.msg[data-side="mine"] .bubble > p:first-child')).toEqual(['Only to chat.'])
    expect(host.textContent).toContain('No manager is running in this workspace')
    expect((host.querySelector('button[aria-label="Send"]') as HTMLButtonElement).disabled).toBe(true)
  })
})
