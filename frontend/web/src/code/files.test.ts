// The code tree and the single-file view show the branch's net change against its base: a
// path the branch added and later moved or deleted is not there; a moved file is only where
// it is now.
import { describe, expect, it } from 'vitest'
import { blame, changeOf, fileTree, shownDelta, shownFrom } from './files'
import { commitStats } from './select'
import type { Commit, FileChange, ModuleNode } from './types'
import type { Unit } from '@/planning/types'
import { selectionOf } from '@/work/select'

const GONE = '000000', NULL = '0'.repeat(40)
// A file change with git's entries for both sides: a regular file's mode unless given, and its
// text standing for its object id unless ids are given (a submodule pointer has no text).
const change = (path: string, before: string | null, after: string | null, modes = ['100644', '100644'], ids?: [string, string]): FileChange => ({
  path, status: before === null ? 'A' : after === null ? 'D' : 'M', before, after, ...changeOf(before, after),
  beforeMode: before === null ? GONE : modes[0], afterMode: after === null ? GONE : modes[1],
  beforeId: ids?.[0] ?? before ?? NULL, afterId: ids?.[1] ?? after ?? NULL,
})
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

  // The reviewer's cases (u16#2, u16#4): a file is unchanged over a scope only when it has
  // the same existence, mode and object id at both ends; the texts only give the counts.
  const scopes = (u: Unit, path: string) => [null, 't1'].map(pick => [fileRows(u, pick)[path] ?? null, shownDelta(u, selectionOf(u, pick ? { kind: 'task', id: pick } : null), path)?.status ?? null])
  const sh = '#!/bin/sh\necho hello\n'

  it('a mode-only change (100644 to 100755) stays, as M +0 -0, picked or not', () => {
    const u = unit([commit('c1', [change('run.sh', sh, sh, ['100644', '100755'])])])
    expect(scopes(u, 'run.sh')).toEqual([[{ status: 'M', add: 0, del: 0, inPick: true }, 'M'], [{ status: 'M', add: 0, del: 0, inPick: true }, 'M']])
  })

  it('a mode and text change, then the text restored: the mode change stays', () => {
    const u = unit([
      commit('c1', [change('run.sh', sh, 'new\n', ['100644', '100755'])]),
      commit('c2', [change('run.sh', 'new\n', sh, ['100755', '100755'])]),
    ])
    expect(scopes(u, 'run.sh')).toEqual([[{ status: 'M', add: 0, del: 0, inPick: true }, 'M'], [{ status: 'M', add: 0, del: 0, inPick: true }, 'M']])
  })

  it('a file made a symlink with its text changed, then the text restored: the type change stays', () => {
    const u = unit([
      commit('c1', [change('link', sh, 'run.sh', ['100644', '120000'])]),
      commit('c2', [change('link', 'run.sh', sh, ['120000', '120000'])]),
    ])
    expect(scopes(u, 'link')).toEqual([[{ status: 'M', add: 0, del: 0, inPick: true }, 'M'], [{ status: 'M', add: 0, del: 0, inPick: true }, 'M']])
  })

  it('a mode changed and changed back is no change', () => {
    const u = unit([commit('c1', [change('run.sh', sh, sh, ['100644', '100755'])]), commit('c2', [change('run.sh', sh, sh, ['100755', '100644'])])])
    expect(scopes(u, 'run.sh')).toEqual([[null, null], [null, null]])
  })

  it('a mode change stays when the text is changed and changed back around it', () => {
    const u = unit([
      commit('c1', [change('keep.ts', 'k\n', 'k2\n')]),
      commit('c2', [change('keep.ts', 'k2\n', 'k2\n', ['100644', '100755'])]),
      commit('c3', [change('keep.ts', 'k2\n', 'k\n', ['100755', '100755'])]),
    ])
    expect(fileRows(u)).toEqual({ 'keep.ts': { status: 'M', add: 0, del: 0, inPick: true } })
  })

  // A submodule pointer: scan's text is the pointed-to commit's when the repository has it,
  // empty when not; the pointer is the entry's id either way.
  for (const [objects, a, b] of [['present', 'tree a\n', 'tree b\n'], ['missing', '', '']]) {
    const pointer = (from: string, to: string, ids: [string, string]) => change('sub', from, to, ['160000', '160000'], ids)
    const A = '1'.repeat(40), B = '2'.repeat(40)

    it(`a submodule pointer moved stays (objects ${objects})`, () => {
      const u = unit([commit('c1', [pointer(a, b, [A, B])])])
      expect(scopes(u, 'sub').map(([, s]) => s)).toEqual(['M', 'M'])
    })

    it(`a submodule pointer moved and moved back is no change (objects ${objects})`, () => {
      const u = unit([commit('c1', [pointer(a, b, [A, B])]), commit('c2', [pointer(b, a, [B, A])])])
      expect(scopes(u, 'sub')).toEqual([[null, null], [null, null]])
    })
  }

  it('uncommitted work that undoes a commit is no change; git has not hashed it, so its text stands for its id', () => {
    const live = (after: string) => ({ ...commit('live', [{ ...change('keep.ts', 'k2\n', after), afterId: NULL }]), live: true })
    expect(fileRows(unit([commit('c1', [change('keep.ts', 'k\n', 'k2\n')]), live('k\n')]))).toEqual({})
    expect(fileRows(unit([commit('c1', [change('keep.ts', 'k\n', 'k2\n')]), live('k3\n')]))).toEqual({ 'keep.ts': { status: 'M', add: 1, del: 1, inPick: true } })
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

describe('a base merged into the branch is not the unit\'s change', () => {
  // The base had keep.ts "k" and same.ts "s". t1 changes keep.ts to "k1", same.ts to "s2" and
  // adds own.ts; then the base adds main.ts, appends "m" to keep.ts and makes same.ts "s2" too;
  // t2 merges the base in (scan reads that merge against the base's side: the branch's whole
  // change as of it, so not main.ts, nor same.ts, which the base has the same); t3 changes
  // own.ts again. git diff base...head: M keep.ts (+1 -1), A own.ts (+2).
  const merged = unit(
    [
      commit('c1', [change('keep.ts', 'k\n', 'k1\n'), change('same.ts', 's\n', 's2\n'), change('own.ts', null, 'o\n')]),
      { ...commit('m2', [change('keep.ts', 'k\nm\n', 'k1\nm\n'), change('own.ts', null, 'o\n')]), merge: true },
      commit('c3', [change('own.ts', 'o\n', 'o\no2\n')]),
    ],
    [task('t1', ['c1']), task('t2', ['m2']), task('t3', ['c3'])],
  )

  it('with nothing picked: the branch against the base it last took in, as git diff base...head', () => {
    expect(fileRows(merged)).toEqual({
      'keep.ts': { status: 'M', add: 1, del: 1, inPick: true },
      'own.ts': { status: 'A', add: 2, del: 0, inPick: true },
    })
    const d = shownDelta(merged, selectionOf(merged, null), 'keep.ts')!
    expect([d.before, d.after]).toEqual(['k\nm\n', 'k1\nm\n'])
    expect(shownDelta(merged, selectionOf(merged, null), 'same.ts')).toBeNull()
  })

  it('the merge is no task\'s change: picked, the task that merged has none; another its own commits', () => {
    expect(selectionOf(merged, { kind: 'task', id: 't2' }).commits).toEqual([])
    expect(fileRows(merged, 't2')).toEqual({
      'keep.ts': { status: 'M', add: 1, del: 1, inPick: false },
      'own.ts': { status: 'A', add: 2, del: 0, inPick: false },
    })
    expect(fileRows(merged, 't1')).toEqual({
      'keep.ts': { status: 'M', add: 1, del: 1, inPick: true },
      'own.ts': { status: 'A', add: 1, del: 0, inPick: true },
      'same.ts': { status: 'M', add: 1, del: 1, inPick: true },
    })
  })

  it('the work a file\'s change comes from is the commits of the unit\'s own, never the merge', () => {
    expect(shownFrom(merged, selectionOf(merged, null), 'keep.ts')).toEqual([0])
    expect(shownFrom(merged, selectionOf(merged, null), 'own.ts')).toEqual([0, 2])
    expect(shownFrom(merged, selectionOf(merged, { kind: 'task', id: 't3' }), 'own.ts')).toEqual([2])
  })

  it('a line keeps the commit that wrote it through the merge; one the base brought is from before the unit', () => {
    expect(blame(merged, 'keep.ts', 1)).toEqual([0, -1])
    expect(blame(merged, 'own.ts', 2)).toEqual([0, 2])
  })

  it('a line only the merge wrote (a conflict resolved by hand) is the merge\'s', () => {
    const u = unit([
      commit('c1', [change('keep.ts', 'k\n', 'k1\n')]),
      { ...commit('m2', [change('keep.ts', 'k\nm\n', 'k1\nm\nfix\n')]), merge: true },
    ])
    expect(blame(u, 'keep.ts', 1)).toEqual([0, -1, 1])
  })

  it('the merge adds no lines of its own to the commit list\'s counts', () => {
    expect(commitStats(merged.commits[1])).toEqual({ files: 0, add: 0, del: 0 })
    expect(commitStats(merged.commits[0])).toEqual({ files: 3, add: 3, del: 2 })
  })

  it('the single-file view agrees with the tree', () => {
    for (const pick of [null, 't1', 't2', 't3']) {
      const sel = selectionOf(merged, pick ? { kind: 'task', id: pick } : null)
      for (const [path, r] of Object.entries(fileRows(merged, pick))) {
        const d = shownDelta(merged, sel, path)!
        expect([d.status, d.add, d.del]).toEqual([r.status, r.add, r.del])
      }
    }
  })
})
