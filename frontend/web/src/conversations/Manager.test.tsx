// The Manager section's conversation with every manager: picked in the workspace select, it
// shows a message to every manager once, each copy where it is and each manager's replies
// labelled by its workspace; what is written there goes to every manager, with no chips.
// Needs you counts those replies under the same conversation.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import { inbox } from '@/attention/select'
import type { Session } from '@/agents/types'
import type { Unit } from '@/planning/types'
import type { Workspace } from '@/workspace/types'
import { Manager } from './Manager'
import { EVERY, repliesOf } from './select'
import type { ManagerMessage, Talk } from './types'

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

const session = (agent: string, label: string, machine?: string) => ({ agent, session: `s-${agent}`, manager: true, group: { id: label, label, machine }, machine }) as unknown as Session
const sessions = [session('boss', 'chat'), session('web-mgr', 'web'), session('far', 'api', 'box')]
const reply = (by: string, text: string, at: string) => ({ by, text, at, unread: true })
const broadcast: ManagerMessage = {
  id: 'm2', workspace: EVERY, manager: '', text: 'I shut down at six.', at: '2026-11-21T10:00:00Z', step: 'sending', replies: [],
  to: [
    { workspace: 'chat', manager: 'boss', step: 'delivered', replies: [reply('boss', 'chat: nothing running.', '2026-11-21T10:01:00Z')] },
    { workspace: 'web', manager: 'web-mgr', step: 'held', reason: 'the manager web-mgr is on a blind task', replies: [] },
    { workspace: 'api', machine: 'box', manager: 'far', step: 'delivered', replies: [reply('far', 'api: two worktrees removed.', '2026-11-21T10:02:00Z')] },
  ],
}
const own: ManagerMessage = { id: 'm1', workspace: 'chat', manager: 'boss', text: 'Only to chat.', at: '2026-11-21T09:00:00Z', step: 'delivered', replies: [] }

let root: Root
let host: HTMLDivElement
let told: Parameters<Talk['onTell']>[]
beforeEach(() => { host = document.createElement('div'); document.body.append(host); root = createRoot(host); told = [] })
afterEach(() => { act(() => root.unmount()); host.remove() })

function show(to: string, messages: ManagerMessage[], live = sessions) {
  const talk: Talk = {
    sessions: live, file: null, lines: null, to, onPick: () => {}, projectOf: () => undefined, onRead: () => {}, onFile: () => {}, focusAt: 0, onOpen: () => {}, extra: [],
    intents: [], messages, onTell: (...a) => { told.push(a) }, onDiscuss: async () => {}, onDiscussRead: () => {}, onAnswer: () => {},
  }
  const unit = { id: 'u1', title: 'Search' } as unknown as Unit
  act(() => root.render(<TooltipProvider><Manager unit={unit} n={null} talk={talk} /></TooltipProvider>))
}
const texts = (sel: string) => [...host.querySelectorAll(sel)].map(e => e.textContent)

describe('the conversation with every manager', () => {
  it('shows a message to every manager once, each copy where it is, each reply labelled by its workspace', () => {
    show(EVERY, [own, broadcast])
    expect(host.querySelector('[aria-label="Workspace"]')?.textContent).toBe('All managers')
    expect(texts('.msg[data-side="mine"] .bubble > p:first-child')).toEqual(['I shut down at six.'])
    expect(texts('[aria-label="Where each manager’s copy is"] li')).toEqual([
      'chatIn boss’s terminal',
      'webWaiting: web-mgr is on a blind task; it goes in once that task is delivered',
      'api on boxIn far’s terminal',
    ])
    expect(texts('.msg[data-side="theirs"] .msg-head').map(t => t?.replace(/\d+\w? ago|just now|\d+[smhd]$/, ''))).toEqual([
      expect.stringContaining('bosschat'),
      expect.stringContaining('farapi on box'),
    ])
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
    show('chat', [own, broadcast])
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
