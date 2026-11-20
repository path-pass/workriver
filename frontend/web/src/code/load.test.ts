// A unit's code is read again once its token changes, and the page then has the new entries:
// a file whose mode changed in the work tree, its text the same, is shown changed.
import { afterEach, expect, it, vi } from 'vitest'
import { loadCode, withCode } from './load'
import { changeOf, shownDelta } from './files'
import type { Commit, FileChange } from './types'
import type { Unit } from '@/planning/types'
import { selectionOf } from '@/work/select'

const NULL = '0'.repeat(40)
// The branch changed run.sh's text; the work tree has the base text back, then is made executable.
const committed: FileChange = { path: 'run.sh', status: 'M', ...changeOf('old\n', 'new\n'), before: 'old\n', after: 'new\n', beforeMode: '100644', beforeId: 'b0', afterMode: '100644', afterId: 'b1' }
const live = (afterMode: string): FileChange => ({ path: 'run.sh', status: 'M', ...changeOf('new\n', 'old\n'), before: 'new\n', after: 'old\n', beforeMode: '100644', beforeId: 'b1', afterMode, afterId: NULL })
const snapshot = (code: string, afterMode: string): Unit => {
  const commits: Commit[] = [
    { id: 'c1', subject: 'c1', date: '2026-11-21T00:00:00Z', author: 'a', live: false, files: [committed] },
    { id: 'uncommitted', subject: '', date: '2026-11-21T00:00:00Z', author: 'a', live: true, files: [live(afterMode)] },
  ]
  return { id: 'mode-case', code, commits, tasks: [{ id: 't1', taskId: '1', kind: 'implement', state: 'done', title: 't1', plans: [], inputs: [], output: { commits: ['c1', 'uncommitted'] } }] } as unknown as Unit
}

afterEach(() => vi.unstubAllGlobals())

it('a mode changed in the work tree, its text the same, reaches the page', async () => {
  const before = snapshot('k1', '100644'), after = snapshot('k2', '100755')
  const fetched: string[] = []
  vi.stubGlobal('fetch', async (url: string) => {
    fetched.push(url)
    const u = fetched.length === 1 ? before : after
    return new Response(JSON.stringify({ code: u.code, commits: u.commits.filter(c => c.live || !url.includes('have=')).map(c => ({ id: c.id, files: c.files })) }))
  })
  const shown = async (u: Unit) => { const c = withCode(u, await loadCode(u)); return shownDelta(c, selectionOf(c, null), 'run.sh') }
  expect(await shown(before)).toBeNull()
  expect(await shown(after)).toMatchObject({ status: 'M', add: 0, del: 0 })
  expect(fetched).toEqual(['/api/unit/mode-case/code', '/api/unit/mode-case/code?have=c1'])
})
