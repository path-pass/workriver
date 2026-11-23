// While the user works in the Manager conversation, what blocks on them stays in sight: a bar
// at its top with Blocking's count and each of its rows, each a click away (where Needs you's
// row goes); gone when nothing waits.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import { AgentContext } from '@/agents/context'
import type { AgentRef } from '@/agents/types'
import { Manager } from '@/conversations/Manager'
import type { Pick } from '@/work/types'
import type { Workspace } from '@/workspace/types'
import { inbox } from './select'
import { NeedsBar } from './NeedsBar'

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

const at = '2026-11-23T08:00:00Z'
const task = (id: string, more: object) => ({ id, taskId: id, inputs: [], input: { text: '', causes: [] }, output: {}, plans: [], since: at, ...more })
// One agent waiting in its terminal, one stalled worker, one gate waiting; and what Needs you
// holds besides (a review with an open finding, a chore) that blocks no one.
const ws = {
  intents: [{
    id: 'i1', title: 'Hardening', state: 'active', workspace: 'ws1',
    chores: [{ id: 'c1', title: 'Renew the key', state: 'open', by: 'impl', since: at, steps: [] }],
    units: [{
      id: 'u1', title: 'Login', commits: [], tasks: [
        task('2', { kind: 'gate', gate: 'test', state: 'waiting', title: 'Try the login' }),
        task('3', { kind: 'implement', state: 'running', title: 'Implement', agent: 'impl', stalled: { since: at, escalated: true } }),
        task('4', { kind: 'review', state: 'done', title: 'Review', reviewer: 'codex', findings: [{ id: 'P2-1', severity: 'P2', problem: 'x', impact: 'y', where: [], state: 'open' }] }),
      ],
    }],
  }],
  sessions: [{ agent: 'ja-impl', session: 's1', machine: 'box', topic: 'Japanese copy', workspace: 'ws1', intent: 'i1', blockedSince: at }],
  messages: [],
} as unknown as Workspace

let root: Root
let host: HTMLDivElement
let opened: [string, Pick][]
let answered: AgentRef[]
beforeEach(() => { host = document.createElement('div'); document.body.append(host); root = createRoot(host); opened = []; answered = [] })
afterEach(() => { act(() => root.unmount()); host.remove() })

function show(w: Workspace) {
  const bar = <NeedsBar rows={inbox(w).blocking} onOpen={(u, p) => opened.push([u, p])} onTalk={() => {}} />
  act(() => root.render(
    <TooltipProvider>
      <AgentContext.Provider value={{ answer: a => answered.push(a), view: () => {} }}>
        <Manager unit={null} n={null} talk={null} needs={bar} />
      </AgentContext.Provider>
    </TooltipProvider>,
  ))
}
const bar = () => host.querySelector<HTMLElement>('[aria-label="Needs you"]')
const items = () => [...bar()!.querySelectorAll<HTMLButtonElement>('li button')]

describe('the Needs you bar in the Manager conversation', () => {
  it('stands at the top of the conversation, under its heading', () => {
    show(ws)
    const section = host.querySelector('section[aria-labelledby="side-talk"]')!
    expect(section.firstElementChild?.id).toBe('side-talk')
    expect(section.children[1]).toBe(bar())
  })

  it('shows what blocks on the user: the count and each row, in Blocking’s order', () => {
    show(ws)
    expect(bar()!.querySelector('h3')?.textContent).toBe('Needs you3')
    expect(items().map(b => [b.dataset.kind, b.querySelector('.title')?.textContent])).toEqual([
      ['agent', 'ja-impl is waiting for you in its terminal'],
      ['stalled', expect.stringMatching(/^u1#3: impl stopped .* ago without delivering$/)],
      ['gate', 'Try the login'],
    ])
    // Decisions and chores wait in Needs you; they block no one, so not here.
    expect(bar()!.textContent).not.toContain('Renew the key')
  })

  it('goes where Needs you’s row goes: the agent’s gate, the task', () => {
    show(ws)
    act(() => items()[0].click())
    expect(answered).toEqual([{ name: 'ja-impl', session: 's1', machine: 'box' }])
    act(() => items()[1].click())
    act(() => items()[2].click())
    expect(opened).toEqual([['u1', { kind: 'task', id: '3', finding: undefined }], ['u1', { kind: 'task', id: '2', finding: undefined }]])
  })

  it('is gone when nothing blocks on the user', () => {
    show({ ...ws, sessions: [], intents: [{ ...ws.intents[0], units: [{ ...ws.intents[0].units[0], tasks: ws.intents[0].units[0].tasks.slice(2) }] }] } as Workspace)
    expect(bar()).toBeNull()
    expect(host.querySelector('h2')?.textContent).toBe('Manager')
  })
})
