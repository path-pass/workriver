// Where a unit's scope grew: along each line of work, a task whose commits reach folders
// nothing before it on its line reached. Commits only, never merges; reviews and gates never.
import { describe, expect, it } from 'vitest'
import { growth } from './scope'
import type { Unit } from '@/planning/types'

type T = { id: string; kind?: string; after?: string[]; commits?: string[] }
type C = { id: string; files: string[]; merge?: boolean; live?: boolean }
let units = 0
const unit = (tasks: T[], commits: C[]): Unit =>
  ({
    id: `u${++units}`,
    tasks: tasks.map(t => ({ id: t.id, kind: t.kind ?? 'implement', inputs: (t.after ?? []).map(from => ({ from })), output: { commits: t.commits } })),
    commits: commits.map(c => ({ id: c.id, merge: c.merge, live: c.live, files: c.files.map(path => ({ path })) })),
  }) as unknown as Unit
const marks = (u: Unit) => Object.fromEntries([...growth(u)].map(([id, g]) => [id, { folders: g.folders, beyond: g.beyond.map(t => t.id) }]))

describe('scope growth', () => {
  it('marks a task reaching new folders on its line, with the work that widened it before', () => {
    const u = unit(
      [{ id: '1', commits: ['a'] }, { id: '2', after: ['1'], commits: ['b'] }, { id: '3', after: ['2'], commits: ['c'] }],
      [{ id: 'a', files: ['src/a.ts', 'README.md'] }, { id: 'b', files: ['src/b.ts'] }, { id: 'c', files: ['docs/x.md', 'src/c.ts'] }],
    )
    expect(marks(u)).toEqual({ 3: { folders: ['docs/'], beyond: ['1'] } })
  })

  it('never marks the first work on a line, a review or a gate', () => {
    const u = unit(
      [{ id: '1', commits: ['a'] }, { id: '2', kind: 'review', after: ['1'], commits: ['b'] }, { id: '3', kind: 'gate', after: ['2'], commits: ['b'] }],
      [{ id: 'a', files: ['src/a.ts'] }, { id: 'b', files: ['other/b.ts'] }],
    )
    expect(marks(u)).toEqual({})
  })

  it('ignores merge commits and uncommitted work', () => {
    const u = unit(
      [{ id: '1', commits: ['a'] }, { id: '2', after: ['1'], commits: ['m', 'w'] }],
      [{ id: 'a', files: ['src/a.ts'] }, { id: 'm', files: ['base/x.ts'], merge: true }, { id: 'w', files: ['wip/y.ts'], live: true }],
    )
    expect(marks(u)).toEqual({})
  })

  it('where lines join, "before" is everything before any of them', () => {
    const u = unit(
      [{ id: '1', commits: ['a'] }, { id: '2', commits: ['b'] }, { id: '3', after: ['1', '2'], commits: ['c'] }, { id: '4', after: ['3'], commits: ['d'] }],
      [{ id: 'a', files: ['src/a.ts'] }, { id: 'b', files: ['lib/b.ts'] }, { id: 'c', files: ['lib/c.ts', 'src/c.ts'] }, { id: 'd', files: ['test/d.ts'] }],
    )
    expect(marks(u)).toEqual({ 4: { folders: ['test/'], beyond: ['1', '2'] } })
  })

  it('a separate line has its own scope', () => {
    const u = unit(
      [{ id: '1', commits: ['a'] }, { id: '2', after: ['1'], commits: ['b'] }, { id: '3', commits: ['c'] }, { id: '4', after: ['3'], commits: ['d'] }],
      [{ id: 'a', files: ['src/a.ts'] }, { id: 'b', files: ['src/b.ts'] }, { id: 'c', files: ['docs/c.md'] }, { id: 'd', files: ['src/d.ts', 'docs/d.md'] }],
    )
    expect(marks(u)).toEqual({ 4: { folders: ['src/'], beyond: ['3'] } })
  })
})
