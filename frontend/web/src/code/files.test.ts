// The code tree and the single-file view show the branch's net change against its base: a
// path the branch added and later moved or deleted is not there; a moved file is only where
// it is now.
import { describe, expect, it } from 'vitest'
import { blame, changeOf, fileTree, shownDelta } from './files'
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

  // A submodule pointer: scan gives it no text (its commit is the entry's id), so no lines.
  const pointer = (ids: [string | null, string | null]): FileChange => ({
    path: 'sub', status: ids[0] === null ? 'A' : ids[1] === null ? 'D' : 'M', before: null, after: null, add: 0, del: 0, lines: [],
    beforeMode: ids[0] === null ? GONE : '160000', afterMode: ids[1] === null ? GONE : '160000', beforeId: ids[0] ?? NULL, afterId: ids[1] ?? NULL,
  })
  const A = '1'.repeat(40), B = '2'.repeat(40)

  it('a submodule pointer moved stays, as the pointer: its two commits, no lines', () => {
    const u = unit([commit('c1', [pointer([A, B])])])
    expect(scopes(u, 'sub')).toEqual([[{ status: 'M', add: 0, del: 0, inPick: true }, 'M'], [{ status: 'M', add: 0, del: 0, inPick: true }, 'M']])
    for (const pick of [null, 't1']) {
      const d = shownDelta(u, selectionOf(u, pick ? { kind: 'task', id: pick } : null), 'sub')!
      expect(d.parts.map(p => p.pointer)).toEqual([{ from: A, to: B }])
    }
  })

  it('a submodule added or removed: the pointer on its one side', () => {
    expect(shownDelta(unit([commit('c1', [pointer([null, B])])]), selectionOf(unit([]), null), 'sub')?.parts[0].pointer).toEqual({ from: null, to: B })
    const gone = unit([commit('c1', [pointer([A, null])])])
    expect(shownDelta(gone, selectionOf(gone, null), 'sub')?.parts[0].pointer).toEqual({ from: A, to: null })
  })

  it('a submodule pointer moved and moved back is no change', () => {
    const u = unit([commit('c1', [pointer([A, B])]), commit('c2', [pointer([B, A])])])
    expect(scopes(u, 'sub')).toEqual([[null, null], [null, null]])
  })

  // A binary file: scan gives it no texts and no counts, as git says "Binary files differ".
  const binary = (path: string, ids: [string | null, string | null]): FileChange => ({
    path, status: ids[0] === null ? 'A' : ids[1] === null ? 'D' : 'M', before: null, after: null, add: 0, del: 0, lines: [],
    beforeBinary: ids[0] !== null, afterBinary: ids[1] !== null,
    beforeMode: ids[0] === null ? GONE : '100644', afterMode: ids[1] === null ? GONE : '100644', beforeId: ids[0] ?? NULL, afterId: ids[1] ?? NULL,
  })

  it('a binary file added, changed or deleted is there, with no lines, picked or not', () => {
    const u = unit([commit('c1', [binary('new.png', [null, A]), binary('keep.ts', ['k', A]), binary('old/base.ts', ['b', null])])])
    for (const [path, status] of [['new.png', 'A'], ['keep.ts', 'M'], ['old/base.ts', 'D']]) {
      expect(scopes(u, path)).toEqual([[{ status, add: 0, del: 0, inPick: true }, status], [{ status, add: 0, del: 0, inPick: true }, status]])
      expect(shownDelta(u, selectionOf(u, null), path)?.parts.map(p => p.binary)).toEqual([true])
    }
  })

  it('a binary file changed and changed back is no change', () => {
    const u = unit([commit('c1', [binary('keep.ts', ['k', A])]), commit('c2', [binary('keep.ts', [A, 'k'])])])
    expect(scopes(u, 'keep.ts')).toEqual([[null, null], [null, null]])
  })

  it('a text file made binary: binary, its old lines not counted as removed', () => {
    const u = unit([commit('c1', [change('keep.ts', 'k\n', 'k2\n')]), commit('c2', [{ ...binary('keep.ts', ['k2\n', A]), before: 'k2\n', beforeBinary: false }])])
    expect(fileRows(u)).toEqual({ 'keep.ts': { status: 'M', add: 0, del: 0, inPick: true } })
    expect(shownDelta(u, selectionOf(u, null), 'keep.ts')?.parts.map(p => p.binary)).toEqual([true])
  })

  it('a binary file changed in parts (other work between): each part changed, by its entries', () => {
    const u = unit([commit('c1', [binary('keep.ts', ['k', A])]), commit('c2', [binary('keep.ts', [A, B])]), commit('c3', [binary('keep.ts', [B, 'k3'])])],
      [task('t1', ['c1', 'c3']), task('t2', ['c2'])])
    const d = shownDelta(u, selectionOf(u, { kind: 'task', id: 't1' }), 'keep.ts')!
    expect(d.parts.map(p => [p.status, p.binary])).toEqual([['M', true], ['M', true]])
  })

  it('uncommitted binary work: its id (hashed by scan) says whether it changed', () => {
    const live = (id: string) => ({ ...commit('live', [binary('keep.ts', [A, id])]), live: true })
    expect(fileRows(unit([commit('c1', [binary('keep.ts', ['k', A])]), live('k')]))).toEqual({})
    expect(fileRows(unit([commit('c1', [binary('keep.ts', ['k', A])]), live(B)]))).toEqual({ 'keep.ts': { status: 'M', add: 0, del: 0, inPick: true } })
  })

  // A binary version between two texts: scan keeps the text of each side that is text.
  const toBinary: FileChange = { ...binary('keep.ts', ['k\n', A]), before: 'k\n', beforeBinary: false }
  const toText = (afterId: string): FileChange => ({ ...binary('keep.ts', [A, afterId]), after: 'k2\n', afterBinary: false })

  it('a binary version between two texts is the text change git shows, picked or not', () => {
    const u = unit([commit('c1', [toBinary]), commit('c2', [toText('k2\n')])])
    for (const pick of [null, 't1']) {
      expect(fileRows(u, pick)).toEqual({ 'keep.ts': { status: 'M', add: 1, del: 1, inPick: true } })
      expect(shownDelta(u, selectionOf(u, pick ? { kind: 'task', id: pick } : null), 'keep.ts')?.parts.map(p => [p.binary, p.before, p.after])).toEqual([[undefined, 'k\n', 'k2\n']])
    }
  })

  it('a binary version between two texts, the last edit uncommitted: the text change too', () => {
    const u = unit([commit('c1', [toBinary]), { ...commit('live', [toText(NULL)]), live: true }])
    for (const pick of [null, 't1']) expect(fileRows(u, pick)).toEqual({ 'keep.ts': { status: 'M', add: 1, del: 1, inPick: true } })
    const net = { ...u, net: { id: 'b..t', files: [toBinary] } } as Unit
    for (const pick of [null, 't1']) expect(fileRows(net, pick)).toEqual({ 'keep.ts': { status: 'M', add: 1, del: 1, inPick: true } })
  })

  it('uncommitted work scan could not read (git lists it, cannot hash it) is a change', () => {
    const unread = { ...binary('keep.ts', [A, NULL]), afterBinary: false }
    expect(fileRows(unit([{ ...commit('live', [unread]), live: true }]))).toEqual({ 'keep.ts': { status: 'M', add: 0, del: 0, inPick: true } })
  })

  it('a SHA-256 repository: 64 zeros are git\'s placeholder too, so the text stands for the id', () => {
    const live = (after: string) => ({ ...commit('live', [{ ...change('keep.ts', 'k2\n', after), afterId: '0'.repeat(64) }]), live: true })
    expect(fileRows(unit([commit('c1', [change('keep.ts', 'k\n', 'k2\n')]), live('k\n')]))).toEqual({})
  })

  it('a file made a submodule: the pointer, no lines, though scan keeps the file\'s text', () => {
    const made: FileChange = { ...change('keep.ts', 'k\n', null), status: 'M', add: 0, del: 0, afterMode: '160000', afterId: B }
    const u = unit([commit('c1', [made])])
    expect(fileRows(u)).toEqual({ 'keep.ts': { status: 'M', add: 0, del: 0, inPick: true } })
    expect(shownDelta(u, selectionOf(u, null), 'keep.ts')?.parts.map(p => [p.pointer, p.binary])).toEqual([[{ from: null, to: B }, undefined]])
    // A binary file made one: the pointer, not "Binary files differ".
    const fromBinary = unit([commit('c1', [{ ...binary('keep.ts', [A, B]), afterBinary: false, afterMode: '160000' }])])
    expect(shownDelta(fromBinary, selectionOf(fromBinary, null), 'keep.ts')?.parts.map(p => [p.pointer, p.binary])).toEqual([[{ from: null, to: B }, undefined]])
  })

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
  // Every commit is against its first parent, as scan reads it; `merge` marks a merge that
  // brought the base in, as the service works it out; `net` is git diff base...head.
  const seven = 'one\ntwo\nthree\nfour\nfive\nsix\nseven\n'
  const withNet = (u: Unit, files: FileChange[]) => ({ ...u, net: { id: 'b..t', files } }) as Unit

  it('the base reaching the branch through another branch is not the unit\'s: the whole is the net change', () => {
    // feature.txt the unit's own; side, made from a newer base, brings side.txt and the
    // base's base-only.txt. git diff main...feature: feature.txt, side.txt.
    const u = withNet(
      unit(
        [commit('c1', [change('feature.txt', null, 'own\n')]), commit('m2', [change('base-only.txt', null, 'base\n'), change('side.txt', null, 'side\n')])],
        [task('t1', ['c1']), task('t2', ['m2'])],
      ),
      [change('feature.txt', null, 'own\n'), change('side.txt', null, 'side\n')],
    )
    expect(fileRows(u)).toEqual({
      'feature.txt': { status: 'A', add: 1, del: 0, inPick: true },
      'side.txt': { status: 'A', add: 1, del: 0, inPick: true },
    })
  })

  // t1 changes line 1, the base line 4 (merged in by t2), t1 then line 7.
  const across = withNet(
    unit(
      [
        commit('c1', [change('shared.txt', seven, seven.replace('one', 'task-first'))]),
        { ...commit('m2', [change('shared.txt', seven.replace('one', 'task-first'), seven.replace('one', 'task-first').replace('four', 'base-only'))]), merge: true },
        commit('c3', [change('shared.txt', seven.replace('one', 'task-first').replace('four', 'base-only'), seven.replace('one', 'task-first').replace('four', 'base-only').replace('seven', 'task-last'))]),
      ],
      [task('t1', ['c1', 'c3']), task('t2', ['m2'])],
    ),
    [change('shared.txt', seven.replace('four', 'base-only'), seven.replace('one', 'task-first').replace('four', 'base-only').replace('seven', 'task-last'))],
  )

  it('with nothing picked: git diff base...head', () => {
    expect(fileRows(across)).toEqual({ 'shared.txt': { status: 'M', add: 2, del: 2, inPick: true } })
  })

  it('a task working on both sides of a base merge: its own changes, part by part, never the base\'s line', () => {
    expect(fileRows(across, 't1')).toEqual({ 'shared.txt': { status: 'M', add: 2, del: 2, inPick: true } })
    const d = shownDelta(across, selectionOf(across, { kind: 'task', id: 't1' }), 'shared.txt')!
    expect(d.parts.map(p => [p.touched, p.add, p.del])).toEqual([[[0], 1, 1], [[2], 1, 1]])
    expect(d.parts.some(p => (p.before ?? '').includes('base-only') !== (p.after ?? '').includes('base-only'))).toBe(false)
  })

  it('the task that merged the base has no change of its own', () => {
    const sel = selectionOf(across, { kind: 'task', id: 't2' })
    expect([sel.commits, sel.onlyMerges]).toEqual([[], true])
    expect(fileRows(across, 't2')).toEqual({ 'shared.txt': { status: 'M', add: 2, del: 2, inPick: false } })
  })

  it('a file a task added and deleted itself, with other work between, is no change of its own', () => {
    const u = unit(
      [commit('c1', [change('x.ts', null, 'x\n')]), commit('c2', [change('x.ts', 'x\n', 'x2\n')]), commit('c3', [change('x.ts', 'x2\n', null)])],
      [task('t1', ['c1', 'c3']), task('t2', ['c2'])],
    )
    expect(shownDelta(u, selectionOf(u, { kind: 'task', id: 't1' }), 'x.ts')).toBeNull()
  })

  it('a task\'s status is from the parts it kept: a file it added and deleted, then the base added, it only changed', () => {
    // t1 adds path.txt and deletes it; the base adds it (t2 merges that in); t1 appends a line.
    const u = unit(
      [
        commit('c1', [change('path.txt', null, 'temporary\n')]),
        commit('c2', [change('path.txt', 'temporary\n', null)]),
        { ...commit('m3', [change('path.txt', null, 'from base\n')]), merge: true },
        commit('c4', [change('path.txt', 'from base\n', 'from base\nmy edit\n')]),
      ],
      [task('t1', ['c1', 'c2', 'c4']), task('t2', ['m3'])],
    )
    const d = shownDelta(u, selectionOf(u, { kind: 'task', id: 't1' }), 'path.txt')!
    expect([d.status, d.add, d.del, d.parts.length]).toEqual(['M', 1, 0, 1])
    expect(fileRows(u, 't1')['path.txt']).toEqual({ status: 'M', add: 1, del: 0, inPick: true })
    // Added by the task and only changed after: still added.
    const added = unit(
      [commit('c1', [change('x.ts', null, 'x\n')]), commit('c2', [change('x.ts', 'x\n', 'x\ny\n')]), commit('c3', [change('x.ts', 'x\ny\n', 'x\ny\nz\n')])],
      [task('t1', ['c1', 'c3']), task('t2', ['c2'])],
    )
    expect(shownDelta(added, selectionOf(added, { kind: 'task', id: 't1' }), 'x.ts')!.status).toBe('A')
  })

  it('the work a file\'s whole change comes from is the unit\'s own commits, never the merge', () => {
    expect(shownDelta(across, selectionOf(across, null), 'shared.txt')!.touched).toEqual([0, 2])
  })

  it('a line a merge of the base brought is the base\'s, even where the merge left the file as the base has it', () => {
    // t1 makes line 2 "task"; the base does the same and puts "incoming" first; t2 appends "new".
    const u = unit(
      [
        commit('c1', [change('shared.txt', 'first\nsecond\n', 'first\ntask\n')]),
        { ...commit('m2', [change('shared.txt', 'first\ntask\n', 'incoming\nfirst\ntask\n')]), merge: true },
        commit('c3', [change('shared.txt', 'incoming\nfirst\ntask\n', 'incoming\nfirst\ntask\nnew\n')]),
      ],
      [task('t1', ['c1']), task('t2', ['m2']), task('t3', ['c3'])],
    )
    expect(blame(u, 'shared.txt', 2)).toEqual([-1, -1, 0, 2])
  })

  it('the merge adds no lines of its own to the commit list\'s counts', () => {
    expect(commitStats(across.commits[1])).toEqual({ files: 0, add: 0, del: 0, counted: true })
    expect(commitStats(across.commits[0])).toEqual({ files: 1, add: 1, del: 1, counted: true })
  })

  it('uncommitted work over the size limit is a change with no lines, said so', () => {
    // Added, its text not read: no text, no id, no counts.
    const big = { ...change('big.txt', null, null), afterMode: '100644', afterId: NULL, add: 0, del: 0, uncounted: true }
    const live = { ...commit('live', [big]), live: true }
    const u = { ...across, commits: [...across.commits, live] } as Unit
    const d = shownDelta(u, selectionOf(u, null), 'big.txt')!
    expect(d.parts[0]).toMatchObject({ uncounted: true, add: 0, del: 0 })
  })

  it('uncommitted work read for its names only has no line counts', () => {
    const named = { ...change('a.txt', null, null), add: 0, del: 0, uncounted: true }
    const live = { ...commit('live', [named, change('b.txt', 'one\n', 'one\ntwo\n')]), live: true }
    expect(commitStats(live)).toMatchObject({ files: 2, counted: false })
  })

  it('uncommitted work goes on top of the net change', () => {
    const tip = seven.replace('one', 'task-first').replace('four', 'base-only').replace('seven', 'task-last')
    const live = { ...commit('live', [{ ...change('shared.txt', tip, tip.replace('task-last', 'seven')), afterId: '0'.repeat(40) }]), live: true }
    const u = { ...across, commits: [...across.commits, live] } as Unit
    expect(fileRows(u)).toEqual({ 'shared.txt': { status: 'M', add: 1, del: 1, inPick: true } })
  })

  it('the single-file view agrees with the tree', () => {
    for (const pick of [null, 't1', 't2']) {
      const sel = selectionOf(across, pick ? { kind: 'task', id: pick } : null)
      for (const [path, r] of Object.entries(fileRows(across, pick))) {
        const d = shownDelta(across, sel, path)!
        expect([d.status, d.add, d.del]).toEqual([r.status, r.add, r.del])
      }
    }
  })
})
