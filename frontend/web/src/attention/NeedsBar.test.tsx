// The Manager bar puts the agents waiting in their terminals one click away.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import { AgentContext } from '@/agents/context'
import type { AgentRef } from '@/agents/types'
import { Manager } from '@/conversations/Manager'
import type { WaitingAgentRow } from './select'
import { NeedsBar } from './NeedsBar'

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

const row: WaitingAgentRow = {
  key: 'agent:s1:box', kind: 'agent', intent: null, unit: null, node: null, pick: null, chore: null,
  since: Date.parse('2026-11-23T08:00:00Z'),
  agent: { name: 'ja-impl', session: 's1', machine: 'box', place: 'Hardening' },
}

let root: Root
let host: HTMLDivElement
let answered: AgentRef[]
beforeEach(() => { host = document.createElement('div'); document.body.append(host); root = createRoot(host); answered = [] })
afterEach(() => { act(() => root.unmount()); host.remove() })

function show(rows: WaitingAgentRow[]) {
  act(() => root.render(
    <TooltipProvider>
      <AgentContext.Provider value={{ answer: a => answered.push(a), view: () => {} }}>
        <Manager unit={null} n={null} talk={null} needs={<NeedsBar rows={rows} />} />
      </AgentContext.Provider>
    </TooltipProvider>,
  ))
}
const bar = () => host.querySelector<HTMLElement>('[aria-label="Needs you"]')
const items = () => [...bar()!.querySelectorAll<HTMLButtonElement>('li button')]

describe('the Needs you bar in the Manager conversation', () => {
  it('stands at the top of the conversation, under its heading', () => {
    show([row])
    const section = host.querySelector('section[aria-labelledby="side-talk"]')!
    expect(section.firstElementChild?.id).toBe('side-talk')
    expect(section.children[1]).toBe(bar())
  })

  it('opens the selected agent’s terminal answer gate', () => {
    const here = { ...row, key: 'agent:s1', agent: { ...row.agent, machine: undefined } }
    show([here, row])
    expect(bar()!.querySelector('h3')?.textContent).toBe('Needs you2')
    act(() => items()[1].click())
    expect(answered).toEqual([{ name: 'ja-impl', session: 's1', machine: 'box' }])
  })
})
