// A gate's detail, mounted: the files attached to the gate show while it waits, before the
// user answers it, the way a task's attachments show, and they stay once it is submitted.
import { act, useState, type ReactNode } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import type { Unit } from '@/planning/types'
import type { Talk } from '@/conversations/types'
import type { Attachment } from '@/attachments/types'
import { ActContext, type Act } from './actions'
import type { GateResult, Task } from './types'
import { withEdits, NO_EDITS } from '@/workspace/edits'
import type { Workspace } from '@/workspace/types'
import { Detail } from './TaskDetail'
import { AgentContext } from '@/agents/context'
import type { AgentRef, Session } from '@/agents/types'
import { GateDraftsContext, type GateDrafts } from './GateForm'
import { TaskDraftsContext, useTaskDrafts } from './drafts'

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

const plan: Attachment = { id: 'a1', name: 'plan-b.zh-CN.md', url: '/api/attachments/a1', type: 'text/markdown' }
const shot: Attachment = { id: 'a2', name: 'before.png', url: '/api/attachments/a2', type: 'image/png' }
const gate: Task = {
  id: '2', kind: 'gate', gate: 'test', state: 'waiting', taskId: '2', title: 'Try it', agent: null, writing: false,
  inputs: [{ from: '1', primary: true }], input: { text: 'Read the plan, then try it', causes: [], steps: [] },
  output: { commits: [], attachments: [plan, shot] }, plans: [],
}
const unit = (g: Task): Unit => ({ id: 'u1', title: 'Unit', purpose: '', repo: '/r', branch: 'b', status: 'active', tasks: [g], commits: [] } as unknown as Unit)
const talk = { sessions: [], file: null, lines: null, onRead: () => {} } as unknown as Talk
const acts: Act = { submitGate: () => {}, decide: () => {}, undo: () => {}, choreDone: () => {}, merge: async () => {}, delivery: {}, message: {}, retry: {} }

let root: Root
let host: HTMLDivElement
beforeEach(() => { host = document.createElement('div'); document.body.append(host); root = createRoot(host) })
afterEach(() => { act(() => root.unmount()); host.remove() })

function FormProviders({ children }: { children: ReactNode }) {
  const [gates] = useState<GateDrafts>(() => new Map())
  const drafts = useTaskDrafts()
  return (
    <TooltipProvider>
      <ActContext.Provider value={acts}>
        <GateDraftsContext.Provider value={gates}>
          <TaskDraftsContext.Provider value={drafts}>{children}</TaskDraftsContext.Provider>
        </GateDraftsContext.Provider>
      </ActContext.Provider>
    </TooltipProvider>
  )
}

const show = (u: Unit) => act(() => root.render(
  <FormProviders><Detail unit={u} n={u.tasks[0]} onPick={() => {}} talk={talk} /></FormProviders>,
))
const files = () => [...host.querySelectorAll('.ctx')].map(e => e.textContent)
const pictures = () => [...host.querySelectorAll('.thumb img')].map(e => e.getAttribute('src'))
const button = (name: string) => [...host.querySelectorAll('button')].find(b => (b.getAttribute('aria-label') ?? b.textContent) === name)!
const enter = (field: HTMLInputElement, text: string) => {
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!
  act(() => { set.call(field, text); field.dispatchEvent(new Event('input', { bubbles: true })) })
}

describe('a finding decided by the manager', () => {
  it('shows the reason in its details without a delivery record', () => {
    const review: Task = {
      id: '2', kind: 'review', state: 'done', taskId: '2', title: 'Review tokens', agent: null, writing: false,
      inputs: [{ from: '1', primary: true }], input: { text: '', causes: [] }, plans: [],
      output: { findings: [{
        id: 'P2-1', severity: 'P2', problem: 'Expired tokens still work', impact: '', evidence: [],
        state: 'accepted', decidedBy: 'agent', note: 'Expired tokens can be reused', leadsTo: [],
      }] },
    }
    show(unit(review))
    act(() => host.querySelector<HTMLButtonElement>('button[aria-label="Details of P2-1"]')!.click())
    expect(host.textContent).toContain('Expired tokens can be reused')
  })
})

it('keeps the decision note with its finding when its details close or another unit opens', () => {
  const review: Task = {
    ...gate, kind: 'review', state: 'done', title: 'Review the change',
    output: { findings: [{
      id: 'P2-1', severity: 'P2', problem: 'Expired tokens still work', impact: '', evidence: [],
      state: 'open', leadsTo: [],
    }] },
  }
  const first = unit(review)
  const second = { ...first, id: 'u2' }
  const open = () => act(() => button('Details of P2-1').click())
  const note = () => host.querySelector<HTMLInputElement>('input[aria-label="Note for the agent (optional)"]')

  show(first)
  open()
  enter(note()!, 'Check the token expiry too')
  open()
  expect(note()).toBeNull()
  open()
  expect(note()?.value).toBe('Check the token expiry too')

  show(second)
  expect(note()?.value).toBe('')
  enter(note()!, 'Update the error message')
  show(first)
  expect(note()?.value).toBe('Check the token expiry too')
  show(second)
  expect(note()?.value).toBe('Update the error message')
  act(() => button('Accept').click())
  expect(note()?.value).toBe('')

  const replacement = { ...review.output.findings![0], problem: 'Refresh fails after expiry' }
  show(unit({ ...review, output: { findings: [replacement] } }))
  expect(note()?.value).toBe('')
})

it('keeps the drop reason with its task and discards it on Cancel', () => {
  const first = unit(gate)
  const second = unit({ ...gate, id: '3', taskId: '3' })
  const reason = () => host.querySelector<HTMLInputElement>('input[aria-label="Why it is no longer needed"]')

  show(first)
  act(() => button('No longer needed…').click())
  enter(reason()!, 'Superseded by the new check')
  show(second)
  expect(reason()).toBeNull()
  show(first)
  expect(reason()?.value).toBe('Superseded by the new check')
  act(() => button('Cancel').click())
  act(() => button('No longer needed…').click())
  expect(reason()?.value).toBe('')
})

describe('a gate’s attachments', () => {
  it('show while it waits, next to the form', () => {
    show(unit(gate))
    expect(host.querySelector('form')).not.toBeNull()
    expect(files()).toEqual(['plan-b.zh-CN.md'])
    expect(pictures()).toEqual(['/api/attachments/a2'])
  })

  it('stay once the user submits it, with the pictures of the form after them', () => {
    const mine: Attachment = { id: 'b1', name: 'after.png', url: 'blob:after', type: 'image/png' }
    const r: GateResult = { result: 'done', note: 'Works', attachments: [mine], findings: [], at: '2026-01-01T00:00:00Z' }
    const ws = { intents: [{ id: 'i1', title: 'I', units: [unit(gate)], chores: [] }] } as unknown as Workspace
    const shown = withEdits(ws, { ...NO_EDITS, gates: { 'u1:2': r } })
    show(shown.intents[0].units[0])
    expect(host.querySelector('form')).toBeNull()
    expect(files()).toEqual(['plan-b.zh-CN.md'])
    expect(pictures()).toEqual(['/api/attachments/a2', 'blob:after'])
  })

  it('once the service has the result, are its list as it is, each once', () => {
    const mine: Attachment = { id: 'b1', name: 'after.png', url: 'blob:after', type: 'image/png' }
    const stored: Attachment = { id: 'a3', name: 'after.png', url: '/api/attachments/a3', type: 'image/png' }
    const r: GateResult = { result: 'done', note: 'Works', attachments: [mine], findings: [], at: '2026-01-01T00:00:00Z' }
    const done: Task = { ...gate, state: 'done', output: { ...gate.output, note: 'Works', attachments: [plan, shot, stored] } }
    const ws = { intents: [{ id: 'i1', title: 'I', units: [unit(done)], chores: [] }] } as unknown as Workspace
    show(withEdits(ws, { ...NO_EDITS, gates: { 'u1:2': r } }).intents[0].units[0])
    expect(files()).toEqual(['plan-b.zh-CN.md'])
    expect(pictures()).toEqual(['/api/attachments/a2', '/api/attachments/a3'])
  })
})

describe('a task’s trace', () => {
  const work: Task = {
    id: '1', kind: 'implement', state: 'done', taskId: '1', title: 'Do it', agent: 'impl', agentSession: 's-1', writing: false,
    inputs: [{ from: 'start', primary: true }], input: { text: 'Do it', causes: [] }, output: { commits: [] }, plans: [],
  }
  const label = '[aria-label="Its agent: harness and model"]'
  const trace = () => host.querySelector(label)?.textContent

  it('names the harness and the model only: no session, no file, nothing to copy', () => {
    show(unit({ ...work, trace: { harness: 'claude', model: 'claude-opus-5-5' } }))
    expect(trace()).toBe('claude · claude-opus-5-5')
    expect(host.querySelector(`${label} button`)).toBeNull()
  })

  it('says what was not known or not given', () => {
    show(unit({ ...work, trace: { harness: 'codex' } }))
    expect(trace()).toBe('codex · model not given')
    show(unit({ ...work, trace: {} }))
    expect(trace()).toBe('harness not known · model not given')
  })

  it('is not there for work no agent did', () => {
    show(unit(gate))
    expect(trace()).toBeUndefined()
  })
})

describe('a task’s waiting line and terminal button', () => {
  const work: Task = {
    id: '1', kind: 'implement', state: 'running', taskId: '1', title: 'Do it', agent: 'impl', agentSession: 's-1', writing: false,
    inputs: [{ from: 'start', primary: true }], input: { text: 'Do it', causes: [] }, output: { commits: [] }, plans: [],
  }
  // Its workspace is the records' (a seat, or the tasks it has), never the runtime's group.
  const on = (machine?: string, workspace = 'ws1'): Session => ({
    agent: 'impl', session: 's-1', ended: false, state: 'blocked', blockedSince: '2026-01-01T00:00:00Z', machine, workspace,
    group: { id: 'w1', label: 'w' },
  })
  // A research unit: no repository, branch or machine.
  const research = (g: Task): Unit => ({ ...unit(g), kind: 'research', repo: '', branch: null }) as Unit
  const asked: { answer: AgentRef[]; view: AgentRef[] } = { answer: [], view: [] }
  const showIn = (u: Unit, sessions: Session[], workspace?: string) => act(() => root.render(
    <FormProviders>
      <AgentContext.Provider value={{ answer: a => asked.answer.push(a), view: a => asked.view.push(a) }}>
        <Detail unit={u} n={u.tasks[0]} onPick={() => {}} talk={{ ...talk, sessions } as Talk} workspace={workspace} />
      </AgentContext.Provider>
    </FormProviders>,
  ))
  const answer = () => [...host.querySelectorAll('button')].find(b => b.textContent === 'Answer')
  const terminal = () => host.querySelector<HTMLButtonElement>('[aria-label="Open the terminal view of impl"]')
  beforeEach(() => { asked.answer = []; asked.view = [] })

  it('on another machine reach the instance on that machine, not the same session here listed first', () => {
    const remote = { ...unit(work), machine: 'box' } as Unit
    // Both wait; the one here comes first, as the service may list them.
    showIn(remote, [on(undefined), on('box')], 'ws1')
    act(() => answer()!.click())
    act(() => terminal()!.click())
    expect(asked.answer).toEqual([{ name: 'impl', session: 's-1', machine: 'box' }])
    expect(asked.view).toEqual([{ name: 'impl', session: 's-1', machine: 'box' }])
  })

  it('on this machine reach the instance here, not one on another machine listed first', () => {
    showIn(unit(work), [on('box'), on(undefined)], 'ws1')
    act(() => answer()!.click())
    act(() => terminal()!.click())
    expect(asked.answer).toEqual([{ name: 'impl', session: 's-1', machine: undefined }])
    expect(asked.view).toEqual([{ name: 'impl', session: 's-1', machine: undefined }])
  })

  it('by name only, reach the instance in the task’s workspace', () => {
    const named = { ...work, agentSession: null }
    // The same name in another workspace, in the same runtime group: not it.
    const other = { ...on(undefined, 'ws2'), session: 's-2' }
    showIn(unit(named), [other, { ...on(undefined), session: 's-1' }], 'ws1')
    act(() => answer()!.click())
    expect(asked.answer).toEqual([{ name: 'impl', session: 's-1', machine: undefined }])
  })

  it('are not there when no instance is the task’s alone, or none is on its machine', () => {
    const named = { ...work, agentSession: null }
    // Two of that name in the task's workspace on its machine: not one to pick.
    showIn(unit(named), [{ ...on(undefined), session: 's-1' }, { ...on(undefined), session: 's-2' }], 'ws1')
    expect(answer()).toBeUndefined()
    expect(terminal()).toBeNull()
    // The only instance is on another machine.
    showIn(unit(work), [on('box')], 'ws1')
    expect(answer()).toBeUndefined()
    expect(terminal()).toBeNull()
  })

  it('of a task with no repository reach its one instance in the workspace, on any machine', () => {
    showIn(research(work), [on('box')], 'ws1')
    act(() => answer()!.click())
    act(() => terminal()!.click())
    expect(asked.answer).toEqual([{ name: 'impl', session: 's-1', machine: 'box' }])
    expect(asked.view).toEqual([{ name: 'impl', session: 's-1', machine: 'box' }])
  })

  it('of a task with no repository are not there when its session shows on two machines, nor out of its workspace', () => {
    // The same session here and on box, box listed first: neither is picked.
    showIn(research(work), [on('box'), on(undefined)], 'ws1')
    expect(answer()).toBeUndefined()
    expect(terminal()).toBeNull()
    // Its one instance is in another workspace.
    showIn(research(work), [on('box', 'ws2')], 'ws1')
    expect(answer()).toBeUndefined()
    expect(terminal()).toBeNull()
  })
})
