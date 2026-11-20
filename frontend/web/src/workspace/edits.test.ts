// What the user put in during this sitting, laid over what the service sent: never over what
// the service recorded instead, and taken back when the service did not take it.
import { describe, expect, it } from 'vitest'
import { answerOptimistically, NO_EDITS, withEdits, type Edits } from './edits'
import type { Workspace } from './types'
import type { GateResult, Task } from '@/work/types'
import type { ManagerMessage } from '@/conversations/types'

const gate = (state: Task['state'], output: Task['output'] = {}) => ({ id: 'g', kind: 'gate', gate: 'decide', state, inputs: [], input: { text: '', causes: [] }, output }) as unknown as Task
const question: ManagerMessage = { id: 'm1', workspace: 'chat', manager: 'boss', text: 'Which?', at: '2026-11-21T10:00:00Z', step: 'delivered', replies: [], byManager: true, choice: { surface: {} }, open: true }
const ws = (g: Task, messages: ManagerMessage[] = []) =>
  ({ messages, chores: [], intents: [{ id: 'i1', state: 'active', title: 'Chat', units: [{ id: 'u1', tasks: [g] }], chores: [] }] }) as unknown as Workspace
const pick = (value: string, note: string): GateResult => ({ result: 'answered', selected: [value], note, attachments: [], findings: [], at: '2026-11-21T10:01:00Z' })
const shown = (w: Workspace) => w.intents[0].units[0].tasks[0]

describe('a gate submitted here', () => {
  it('shows only while the service still has the gate waiting, never over what it recorded', () => {
    const mine: Edits = { ...NO_EDITS, gates: { 'u1:g': pick('b', 'mine') } }
    // Another tab's answer was recorded first; this one was refused.
    const recorded = shown(withEdits(ws(gate('done', { selected: ['a'], note: 'theirs' })), mine))
    expect([recorded.output.selected, recorded.output.note]).toEqual([['a'], 'theirs'])
    // Still on its way (the undo window): shown as given.
    const pending = shown(withEdits(ws(gate('waiting')), mine))
    expect([pending.state, pending.output.selected, pending.output.note]).toEqual(['done', ['b'], 'mine'])
  })
})

describe('an answer to a manager’s question', () => {
  const key = 'm1:0'
  let edits: Edits = NO_EDITS
  const set = (f: (e: Edits) => Edits) => { edits = f(edits) }
  it('shows answered while it goes, and open again when the service refuses it', async () => {
    edits = NO_EDITS
    let fail: (e: Error) => void = () => {}
    const going = answerOptimistically(key, ['a'], () => new Promise<void>((_, reject) => { fail = reject }), set)
    expect(withEdits(ws(gate('waiting'), [question]), edits).messages![0]).toMatchObject({ open: false, answered: { selected: ['a'] } })
    fail(new Error('refused'))
    await expect(going).rejects.toThrow('refused')
    const again = withEdits(ws(gate('waiting'), [question]), edits).messages![0]
    expect([again.open, again.answered]).toEqual([true, undefined])
  })
  it('stays answered once the service took it', async () => {
    edits = NO_EDITS
    await answerOptimistically(key, ['a'], async () => {}, set)
    expect(withEdits(ws(gate('waiting'), [question]), edits).messages![0].answered?.selected).toEqual(['a'])
  })
})
