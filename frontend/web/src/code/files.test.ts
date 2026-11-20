// The code tree and the single-file view show the branch's net change against its base: a
// path the branch added and later moved or deleted is not there; a moved file is only where
// it is now.
import { describe, expect, it } from 'vitest'
import { changeOf, fileTree, shownDelta } from './files'
import type { Commit, FileChange, ModuleNode } from './types'
import type { Unit } from '@/planning/types'
import { selectionOf } from '@/work/select'

const change = (path: string, before: string | null, after: string | null): FileChange =>
  ({ path, status: before === null ? 'A' : after === null ? 'D' : 'M', before, after, ...changeOf(before, after) })
const commit = (id: string, files: FileChange[]): Commit =>
  ({ id, subject: id, date: '2026-11-21T00:00:00Z', author: 'a', live: false, files })
const task = (id: string, commits: string[]) =>
  ({ id, taskId: id, kind: 'implement', state: 'done', title: id, plans: [], inputs: [], output: { commits } })
const unit = (commits: Commit[], tasks = [task('t1', commits.map(c => c.id))]): Unit =>
  ({ id: 'u1', commits, tasks }) as unknown as Unit

// The base holds keep.ts and old/base.ts.
const root: ModuleNode = {
  name: '', path: '', loc: 0, files: 2, children: [
    { name: 'keep.ts', path: 'keep.ts', loc: 1, files: 1 },
    { name: 'old', path: 'old', loc: 1, files: 1, children: [{ name: 'base.ts', path: 'old/base.ts', loc: 1, files: 1 }] },
  ],
}

const fileRows = (u: Unit, pick: string | null = null) => {
  const sel = selectionOf(u, pick ? { kind: 'task', id: pick } : null)
  const ft = fileTree(root, u, sel, true)
  return Object.fromEntries([...ft.rows.values()].filter(r => !r.folder && r.status).map(r => [r.id, { status: r.status, add: r.add, del: r.del, inPick: r.inPick }]))
}

describe('the code tree shows the net change', () => {
  // t1 adds a/x.ts, t2 moves it to b/x.ts and edits it; t3 adds a/gone.ts, t4 deletes it.
  const moved = unit(
    [
      commit('c1', [change('a/x.ts', null, '1\n2\n')]),
      commit('c2', [change('a/x.ts', '1\n2\n', null), change('b/x.ts', null, '1\n2\n3\n')]),
      commit('c3', [change('a/gone.ts', null, 'g\n')]),
      commit('c4', [change('a/gone.ts', 'g\n', null)]),
    ],
    [task('t1', ['c1']), task('t2', ['c2']), task('t3', ['c3']), task('t4', ['c4'])],
  )

  it('with nothing picked: added then moved or deleted is not there; the moved file is added where it is now', () => {
    // git diff base...head: A b/x.ts (+3); nothing else.
    expect(fileRows(moved)).toEqual({ 'b/x.ts': { status: 'A', add: 3, del: 0, inPick: true } })
    expect(shownDelta(moved, selectionOf(moved, null), 'a/x.ts')).toBeNull()
    expect(shownDelta(moved, selectionOf(moved, null), 'a/gone.ts')).toBeNull()
  })

  it('a file of the base moved: deleted where it was, added where it is', () => {
    const u = unit([commit('c1', [change('old/base.ts', 'b\n', null), change('new/base.ts', null, 'b\n')])])
    expect(fileRows(u)).toEqual({
      'old/base.ts': { status: 'D', add: 0, del: 1, inPick: true },
      'new/base.ts': { status: 'A', add: 1, del: 0, inPick: true },
    })
  })

  it('a file changed and changed back is no change', () => {
    const u = unit([commit('c1', [change('keep.ts', 'k\n', 'k2\n')]), commit('c2', [change('keep.ts', 'k2\n', 'k\n')])])
    expect(fileRows(u)).toEqual({})
    expect(shownDelta(u, selectionOf(u, null), 'keep.ts')).toBeNull()
  })

  it('a mode-only change (100644 to 100755) stays, as M +0 -0, picked or not', () => {
    // The reviewer's input: git reports run.sh modified, its text the same on both sides.
    const u = unit([commit('c1', [change('run.sh', '#!/bin/sh\necho hello\n', '#!/bin/sh\necho hello\n')])])
    for (const pick of [null, 't1']) {
      expect(fileRows(u, pick)).toEqual({ 'run.sh': { status: 'M', add: 0, del: 0, inPick: true } })
      expect(shownDelta(u, selectionOf(u, pick ? { kind: 'task', id: pick } : null), 'run.sh')?.status).toBe('M')
    }
  })

  it('a mode change stays when the text is changed and changed back around it', () => {
    const u = unit([
      commit('c1', [change('keep.ts', 'k\n', 'k2\n')]),
      commit('c2', [change('keep.ts', 'k2\n', 'k2\n')]),
      commit('c3', [change('keep.ts', 'k2\n', 'k\n')]),
    ])
    expect(fileRows(u)).toEqual({ 'keep.ts': { status: 'M', add: 0, del: 0, inPick: true } })
  })

  it('with a task picked: its files show its own change, the others the unit net change, faded, no vanished path', () => {
    // t2 touched a/x.ts (deleted it, as its own change) and b/x.ts; a/gone.ts is gone from the unit.
    expect(fileRows(moved, 't2')).toEqual({
      'a/x.ts': { status: 'D', add: 0, del: 2, inPick: true },
      'b/x.ts': { status: 'A', add: 3, del: 0, inPick: true },
    })
    // t3 added a/gone.ts as its own change; b/x.ts is the unit's, faded; a/x.ts is gone.
    expect(fileRows(moved, 't3')).toEqual({
      'a/gone.ts': { status: 'A', add: 1, del: 0, inPick: true },
      'b/x.ts': { status: 'A', add: 3, del: 0, inPick: false },
    })
  })

  it('a task that adds and deletes a file itself shows nothing for it', () => {
    const u = unit([commit('c1', [change('tmp.ts', null, 't\n')]), commit('c2', [change('tmp.ts', 't\n', null), change('keep.ts', 'k\n', 'k2\n')])])
    expect(fileRows(u, 't1')).toEqual({ 'keep.ts': { status: 'M', add: 1, del: 1, inPick: true } })
  })

  it('the single-file view agrees with the tree', () => {
    for (const pick of [null, 't1', 't2', 't3', 't4']) {
      const sel = selectionOf(moved, pick ? { kind: 'task', id: pick } : null)
      for (const [path, r] of Object.entries(fileRows(moved, pick))) {
        const d = shownDelta(moved, sel, path)!
        expect([d.status, d.add, d.del]).toEqual([r.status, r.add, r.del])
      }
    }
  })
})
