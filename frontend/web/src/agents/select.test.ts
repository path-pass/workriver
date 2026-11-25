import { expect, it } from 'vitest'
import type { Intent } from '@/planning/types'
import type { Task } from '@/work/types'
import type { Workspace } from '@/workspace/types'
import { agentTree } from './select'

const task = (state: Task['state'], delivered?: string): Task => ({
  id: '1', taskId: '1', title: 'Implement', kind: 'implement', agent: 'worker', agentSession: 'worker-session',
  state, delivered, writing: false, inputs: [], input: { text: '', causes: [] }, output: {}, plans: [],
})

const intent = (id: string, workspace: string, node: Task): Intent => ({
  id, title: id, workspace, chores: [],
  units: [{ id, title: id, purpose: '', repo: '', branch: null, status: 'active', tasks: [node], commits: [] }],
})

it.each([
  { label: 'the last delivery', queued: false, running: false, expected: 'ws1' },
  { label: 'queued work', queued: true, running: false, expected: 'ws2' },
  { label: 'the recorded workspace of running work', queued: true, running: true, explicit: 'ws3', expected: 'ws3' },
  { label: 'its recorded workspace', queued: true, running: true, explicit: 'ws1', expected: 'ws1' },
  { label: 'no workspace for running work across workspaces', queued: false, running: true, alsoRunning: true, expected: '' },
])('groups a live worker by $label', ({ queued, running, explicit, alsoRunning, expected }) => {
  const ws: Workspace = {
    builtAt: '2026-11-21T12:00:00Z', runtime: 'test', repos: [],
    workspaces: [
      { id: 'ws1', name: 'Released', project: '/project' },
      { id: 'ws2', name: 'Queued', project: '/project' },
      { id: 'ws3', name: 'In progress', project: '/project' },
    ],
    sessions: [{ agent: 'worker', session: 'worker-session', ended: false, state: 'idle', workspace: explicit }],
    intents: [
      { ...intent('i1', 'ws1', task('done', '2026-11-21T11:00:00Z')), state: 'done' },
      intent('i2', 'ws2', alsoRunning ? task('running') : queued ? task('todo') : task('done', '2026-11-21T10:00:00Z')),
      ...(running ? [intent('i3', 'ws3', task('running'))] : []),
    ],
  }

  expect(agentTree(ws).groups.map(group => [group.key, group.workers.map(worker => worker.s.session)]))
    .toEqual([[expected, ['worker-session']]])
})
