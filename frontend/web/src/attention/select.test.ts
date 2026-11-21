// A task's discussion with new answers of its agent is a row of Needs you beside the
// manager's words; read, it leaves. Nothing waits on it.
import { describe, expect, it } from 'vitest'
import { inbox } from './select'
import { withEdits, NO_EDITS } from '@/workspace/edits'
import type { Workspace } from '@/workspace/types'
import type { Task } from '@/work/types'
import type { Thread } from '@/conversations/types'

const turn = (question: string, answer: string | null, answeredAt?: string, unread?: boolean) =>
  ({ question, context: [], answer, answerBy: answer ? 'agent-R' : undefined, answeredAt, unread, at: '2026-11-21T10:00:00Z' })
const discussion: Thread = {
  id: 'q1', ended: false, live: false, discuss: true,
  turns: [turn('Why IP?', 'Cheaper.', '2026-11-21T10:01:00Z', true), turn('And accounts?', 'Later.\nSee section 2.', '2026-11-21T10:05:00Z', true), turn('More?', null)],
}
const research = { id: '1', kind: 'implement', research: true, state: 'done', title: 'Compare limits', agent: 'agent-R', inputs: [], input: { text: '', causes: [] }, output: { gate: '2' }, plans: [], discuss: 'open', threads: [discussion] } as unknown as Task
const ws = {
  intents: [{ id: 'i1', title: 'Hardening', state: 'active', chores: [], units: [{ id: 'survey', title: 'Survey', tasks: [research], commits: [] }] }],
  sessions: [], messages: [],
} as unknown as Workspace

describe('a discussion in Needs you', () => {
  it('is one row with the messages: how many new answers, the newest, who', () => {
    const rows = inbox(ws).messages
    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({ kind: 'discussion', pick: { kind: 'task', id: '1' }, discussion: { agent: 'agent-R', count: 2, text: 'Later.' } })
    expect(rows[0].since).toBe(Date.parse('2026-11-21T10:05:00Z'))
    // Not something that blocks anyone.
    expect(inbox(ws).blocking).toEqual([])
  })

  it('leaves once read up to its newest answer, and only then', () => {
    const half = withEdits(ws, { ...NO_EDITS, discussRead: { 'survey:1': '2026-11-21T10:01:00Z' } })
    expect(inbox(half).messages[0].discussion?.count).toBe(1)
    const read = withEdits(ws, { ...NO_EDITS, discussRead: { 'survey:1': '2026-11-21T10:05:00Z' } })
    expect(inbox(read).messages).toEqual([])
  })

  it('takes the questions asked in this sitting into the discussion', () => {
    const asked = withEdits(ws, { ...NO_EDITS, discussed: { 'survey:1': [turn('Sum up?', null)] } })
    const th = asked.intents[0].units[0].tasks[0].threads!
    expect(th).toHaveLength(1)
    expect(th[0].turns.map(x => x.question)).toEqual(['Why IP?', 'And accounts?', 'More?', 'Sum up?'])
  })
})
