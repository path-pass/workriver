// A unit's files: their changes over some commits, which commit last changed each line, and
// the file tree the code layer shows.
import { diffLines, structuredPatch } from 'diff'
import type { Also, FileChange, ModuleNode, FileDelta, FileTree, TreeRow } from './types'
import type { Intent, Unit } from '@/planning/types'
import { type Selection, working } from '@/work/select'

/** The change from one text to another as unified-diff lines, with `context` lines around each change. */
export function changeOf(before: string | null, after: string | null, context = 3) {
  const { hunks } = structuredPatch('a', 'b', before ?? '', after ?? '', undefined, undefined, { context })
  const lines = hunks.flatMap(h => [`@@ -${h.oldStart},${h.oldLines} +${h.newStart},${h.newLines} @@`, ...h.lines])
  return { lines, add: lines.filter(l => l[0] === '+').length, del: lines.filter(l => l[0] === '-').length }
}

export const baseName = (p: string) => p.split('/').pop() ?? p

export const ROOT = '/'

/**
 * Which commit last changed each line of a file, as of commit `upTo`: one index into the
 * unit's commits per line, -1 for a line from before the unit.
 */
export function blame(unit: Unit, path: string, upTo: number): number[] {
  let owner: number[] = []
  let started = false
  unit.commits.slice(0, upTo + 1).forEach((c, k) => {
    const f = c.files.find(x => x.path === path)
    if (!f) return
    if (!started) { owner = (f.before ?? '').split('\n').slice(0, -1).map(() => -1); started = true }
    const next: number[] = []
    let at = 0
    for (const part of diffLines(f.before ?? '', f.after ?? '')) {
      const n = part.count ?? 0
      if (part.removed) at += n
      else if (part.added) for (let i = 0; i < n; i++) next.push(k)
      else { next.push(...owner.slice(at, at + n)); at += n }
    }
    owner = next
  })
  return owner
}

/**
 * A file's change over some commits taken together: its git entry (mode and object id)
 * before the first of them that touches it against its entry after the last; the texts give
 * only the line counts. Null when none touches it, and when the entries are the same: a file
 * added and then moved or deleted, or changed and changed back (its mode, a submodule's
 * pointer too). A mode or type changed alone stays, M +0 -0. Uncommitted work's side has
 * no object id, so its text stands for one. Over the whole unit this is the branch's net
 * change against its base, as `git diff base...head` names it: a moved file is only where it
 * is now. The tree's counts and the single-file view both come from here, so they agree.
 */
export function fileDelta(unit: Unit, path: string, scope: number[]): FileDelta | null {
  const touched = scope.filter(k => unit.commits[k].files.some(f => f.path === path))
  if (!touched.length) return null
  const at = (k: number) => unit.commits[k].files.find(f => f.path === path)!
  const first = at(touched[0]), last = at(touched.at(-1)!)
  // Uncommitted work's side has no object id (git has not hashed it): its text stands for it.
  const unhashed = last.afterId === NULL && last.afterMode !== GONE
  if (first.beforeMode === last.afterMode && (unhashed ? first.before === last.after : first.beforeId === last.afterId)) return null
  const { add, del } = counted(first, last)
  return { touched, before: first.before, after: last.after, status: first.beforeMode === GONE ? 'A' : last.afterMode === GONE ? 'D' : 'M', add, del }
}

/** git's mode and object id for a side that does not exist; the id also of one not hashed. */
const GONE = '000000', NULL = '0'.repeat(40)

// Line counts from one file version's `before` to another's `after`, kept per pair: a file's
// versions stay the same objects while its unit's code is unchanged, and diffing whole texts
// again on every pick made picking a task in a big unit take up to a second.
const counts = new WeakMap<FileChange, WeakMap<FileChange, { add: number; del: number }>>()
function counted(first: FileChange, last: FileChange) {
  if (first === last) return first
  let to = counts.get(first)
  if (!to) counts.set(first, (to = new WeakMap()))
  let c = to.get(last)
  if (!c) { const { add, del } = changeOf(first.before, last.after); to.set(last, (c = { add, del })) }
  return c
}

/**
 * The change shown for a file, in the tree and in the single-file view alike: over the
 * picked work's commits when they touch it, otherwise over the whole unit.
 */
export function shownDelta(unit: Unit, sel: Selection, path: string): FileDelta | null {
  return fileDelta(unit, path, sel.commits) ?? fileDelta(unit, path, unit.commits.map((_, k) => k))
}

export function fileTree(root: ModuleNode, unit: Unit, sel: Selection, onlyChanged: boolean, narrow = false): FileTree {
  const repo = new Set<string>(), dirs = new Set<string>()
  const walk = (n: ModuleNode) => { if (n.children) { if (n.path) dirs.add(n.path); n.children.forEach(walk) } else repo.add(n.path) }
  walk(root)
  // What the unit did, file by file, as one change over a scope: the pick's commits for the
  // files it touched, the whole unit's for the others. The single-file view shows the same.
  const did = new Map<string, FileDelta & { inPick: boolean }>()
  // A path with no change in either scope (gone from the branch) is not drawn.
  for (const path of new Set(unit.commits.flatMap(c => c.files.map(f => f.path)))) {
    const d = shownDelta(unit, sel, path)
    if (d) did.set(path, { ...d, inPick: !!fileDelta(unit, path, sel.commits) })
  }
  const open = unit.tasks.filter(n => n.state !== 'done')
  const plans = open.flatMap(n => n.plans)
  const writers = new Map<string, string[]>()
  for (const { node, files } of working(unit)) for (const f of files) writers.set(f, [...(writers.get(f) ?? []), node.agent ?? '?'])
  const isDir = (p: string) => dirs.has(p) || !/\.[A-Za-z0-9]+$/.test(p)
  const under = (p: string, list: string[]) => list.some(q => p === q || p.startsWith(q + '/'))
  const picked = (p: string) => !sel.node || !!did.get(p)?.inPick || under(p, sel.plans ?? [])
  // Every path to draw: the repository, plus what the unit adds or plans to add.
  const files = new Set([...repo, ...did.keys(), ...writers.keys(), ...plans.filter(p => !isDir(p))])
  const kids = new Map<string, Set<string>>([[ROOT, new Set()]])
  const link = (p: string) => {
    const up = p.includes('/') ? p.slice(0, p.lastIndexOf('/')) : ROOT
    if (!kids.has(up)) { kids.set(up, new Set()); link(up) }
    kids.get(up)!.add(p)
  }
  for (const f of files) link(f)
  for (const p of plans.filter(isDir)) if (!kids.has(p)) { kids.set(p, new Set()); link(p) }

  const rows = new Map<string, TreeRow>()
  const build = (p: string): TreeRow => {
    const sub = kids.get(p)
    const name = p === ROOT ? '' : p.slice(p.lastIndexOf('/') + 1)
    if (!sub) {
      const d = did.get(p), w = writers.get(p) ?? []
      const planned = !d && plans.includes(p)
      return { id: p, name, folder: false, children: [], add: d?.add ?? 0, del: d?.del ?? 0, status: d?.status, planned, ghost: !repo.has(p) && !d,
        writers: w, changed: !!d || planned || w.length > 0, inPick: picked(p) && (!!d || planned || w.length > 0) }
    }
    const all = [...sub].map(build).filter(r => !onlyChanged || r.changed)
      .sort((a, b) => Number(b.folder) - Number(a.folder) || a.name.localeCompare(b.name))
    const planned = plans.includes(p)
    const row: TreeRow = {
      id: p, name, folder: true, children: all.map(r => r.id),
      add: all.reduce((s, r) => s + r.add, 0), del: all.reduce((s, r) => s + r.del, 0),
      planned, ghost: !dirs.has(p) && p !== ROOT, writers: [...new Set(all.flatMap(r => r.writers))],
      changed: planned || all.some(r => r.changed), inPick: (planned && picked(p)) || all.some(r => r.inPick),
    }
    // A folder holding nothing but one folder is shown as one row with it, as GitHub does.
    if (p !== ROOT && all.length === 1 && all[0].folder) {
      const only = all[0]
      rows.delete(only.id)
      rows.set(only.id, { ...only, name: `${name}/${only.name}`, planned: only.planned || planned })
      return rows.get(only.id)!
    }
    for (const r of all) rows.set(r.id, r)
    return row
  }
  rows.set(ROOT, build(ROOT))
  const focus = [...rows.values()].filter(r => !r.folder && (r.inPick || (!narrow && r.writers.length)) && r.changed).map(r => r.id)
  return { rows, open: [...rows.values()].filter(r => r.folder && r.id !== ROOT && focus.some(f => f.startsWith(r.id + '/'))).map(r => r.id) }
}

/**
 * The other open units that change each of the unit's files too (the service compares them:
 * `overlaps`), by path, each with its intent.
 */
export function alsoChanged(intents: Intent[], unit: Unit): Map<string, Also[]> {
  const of = (id: string): Also => {
    const i = intents.find(x => x.units.some(u => u.id === id))
    return { unit: id, intent: i?.id ?? '', title: i?.title ?? '' }
  }
  return new Map((unit.overlaps ?? []).map(o => [o.path, o.units.map(of)]))
}

/**
 * The tree narrowed to files whose path holds `q` (any case), with the folders above them,
 * all open; the tree as it is for an empty `q`.
 */
export function filterTree(ft: FileTree, q: string): FileTree {
  const needle = q.trim().toLowerCase()
  if (!needle) return ft
  const rows = new Map<string, TreeRow>()
  const keep = (id: string): boolean => {
    const r = ft.rows.get(id)
    if (!r) return false
    if (!r.folder) {
      if (!id.toLowerCase().includes(needle)) return false
      rows.set(id, r)
      return true
    }
    const children = r.children.filter(keep)
    rows.set(id, { ...r, children })
    return children.length > 0
  }
  keep(ROOT)
  return { rows, open: [...rows.values()].filter(r => r.folder && r.id !== ROOT).map(r => r.id) }
}
