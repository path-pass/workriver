// A gate's detail, mounted: the files attached to the gate show while it waits, before the
// user answers it, the way a task's attachments show, and they stay once it is submitted.
import { act } from 'react'
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

const show = (u: Unit) => act(() => root.render(
  <TooltipProvider><ActContext.Provider value={acts}><Detail unit={u} n={u.tasks[0]} onPick={() => {}} talk={talk} /></ActContext.Provider></TooltipProvider>,
))
const files = () => [...host.querySelectorAll('.ctx')].map(e => e.textContent)
const pictures = () => [...host.querySelectorAll('.thumb img')].map(e => e.getAttribute('src'))

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
  const trace = () => host.querySelector('[aria-label="Its agent: harness, model, session"]')?.textContent

  it('names the harness, the model and the session, with the path of the session file', () => {
    show(unit({ ...work, trace: { session: 's-1', harness: 'claude', model: 'claude-opus-5-5', file: '/Users/me/.claude/projects/-r/s-1.jsonl' } }))
    expect(trace()).toBe('claude · claude-opus-5-5 · s-1/Users/me/.claude/projects/-r/s-1.jsonl')
    expect(host.querySelector('[aria-label="Its agent: harness, model, session"] button')).not.toBeNull()
  })

  it('says the machine of a far one, what was not given, and why there is no file', () => {
    show(unit({ ...work, trace: { session: 's-1', harness: 'codex', machine: 'box', file: '~/.codex/sessions/*/*/*/rollout-*-s-1.jsonl*' } }))
    expect(trace()).toBe('codex · model not given · s-1~/.codex/sessions/*/*/*/rollout-*-s-1.jsonl*on box')
    show(unit({ ...work, trace: { session: 's-1', unknown: 'harness' } }))
    expect(trace()).toBe('harness not known · model not given · s-1Where this harness keeps its sessions is not known')
  })

  it('is not there for work no agent did', () => {
    show(unit(gate))
    expect(trace()).toBeUndefined()
  })
})
