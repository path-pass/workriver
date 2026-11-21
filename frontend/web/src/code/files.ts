// A unit's files: their changes over some commits, which commit last changed each line, and
// the file tree the code layer shows.
import { diffLines, structuredPatch } from 'diff'
import type { FileChange, ModuleNode, FileDelta, FileTree, TreeRow, Kind } from './types'
import type { Unit } from '@/planning/types'
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
 * unit's commits per line, -1 for a line from before the unit. Every commit is read against
 * its first parent, so the texts chain; a line a merge of the base brought in (or wrote while
 * resolving a conflict with it) is the base's, from before the unit, never a task's.
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
      else if (part.added) for (let i = 0; i < n; i++) next.push(c.merge ? -1 : k)
      else { next.push(...owner.slice(at, at + n)); at += n }
    }
    owner = next
  })
  return owner
}

/** git's mode and object id for a side that does not exist; the id also of one not hashed. */
const GONE = '000000', NULL = '0'.repeat(40)
/** A submodule pointer's mode (a gitlink): its object id is a commit, it has no text. */
const GITLINK = '160000'

type Entries = Pick<FileChange, 'beforeMode' | 'beforeId' | 'afterMode' | 'afterId' | 'binary'>

/**
 * What the change from `first`'s before to `last`'s after is when it has no lines: a submodule's
 * pointer (either end one: its commits), else a binary file's (either end binary, as git says
 * "Binary files differ"). Empty for a text change.
 */
export function kindOf(first: Entries, last: Entries): Kind {
  if (first.beforeMode === GITLINK || last.afterMode === GITLINK)
    return { pointer: { from: first.beforeMode === GITLINK ? first.beforeId : null, to: last.afterMode === GITLINK ? last.afterId : null } }
  return first.binary || last.binary ? { binary: true } : {}
}

/**
 * Whether a file is the same at two ends: the same git entry (existence, mode, object id).
 * Uncommitted work's side has no object id (git has not hashed it): its text stands for one.
 */
function same(first: FileChange, last: FileChange) {
  const unhashed = last.afterId === NULL && last.afterMode !== GONE
  return first.beforeMode === last.afterMode && (unhashed ? first.before === last.after : first.beforeId === last.afterId)
}

const statusOf = (first: FileChange, last: FileChange): 'A' | 'M' | 'D' => (first.beforeMode === GONE ? 'A' : last.afterMode === GONE ? 'D' : 'M')

/**
 * A file's change over some commits taken together, from their own changes only: its git
 * entry before the first of them that touches it against its entry after the last, the texts
 * giving only the line counts. Where another commit touched the file between two of them (a
 * merge of the base, other work), their texts do not chain: each stretch is a part of its
 * own, compared alone, and the counts are the parts' sums; how it changed (A, M, D) is
 * from the ends of the parts kept. Null when none touches it, when every part leaves it as
 * it was (added and then moved or deleted, changed and changed back: its mode, a submodule's
 * pointer too), and when it is not there at either end (the work added and deleted it
 * itself). A mode or type changed alone stays, M +0 -0. The tree's
 * counts and the single-file view both come from here, so they agree.
 */
export function fileDelta(unit: Unit, path: string, scope: number[]): FileDelta | null {
  const at = (k: number) => unit.commits[k].files.find(f => f.path === path)
  const touched = scope.filter(k => at(k))
  if (!touched.length) return null
  const stretches: number[][] = []
  for (const k of touched) {
    const prev = stretches.at(-1)?.at(-1)
    if (prev !== undefined && !unit.commits.slice(prev + 1, k).some(c => c.files.some(f => f.path === path))) stretches.at(-1)!.push(k)
    else stretches.push([k])
  }
  const first = at(touched[0])!, last = at(touched.at(-1)!)!
  if (first.beforeMode === GONE && last.afterMode === GONE) return null
  const kept = stretches.flatMap(ks => {
    const a = at(ks[0])!, b = at(ks.at(-1)!)!
    return same(a, b) ? [] : [{ a, b, part: { touched: ks, upTo: ks.at(-1)!, status: statusOf(a, b), before: a.before, after: b.after, ...counted(a, b), ...kindOf(a, b) } }]
  })
  if (!kept.length) return null
  // How it changed, from the parts kept: a part that undid itself says nothing (a file the
  // work added and deleted, then the base added and the work changed, is changed, not added).
  const parts = kept.map(k => k.part), shown = parts.at(-1)!
  const status = statusOf(kept[0].a, kept.at(-1)!.b)
  return { touched, before: shown.before, after: shown.after, upTo: shown.upTo, status, add: sum(parts, 'add'), del: sum(parts, 'del'), parts }
}

const sum = (parts: { add: number; del: number }[], k: 'add' | 'del') => parts.reduce((s, p) => s + p[k], 0)

/**
 * A file's change over the whole unit: the branch's net change against its base, as
 * `git diff base...head` names it (`unit.net`, from where the branch meets its base, which
 * the service works out), with uncommitted work on top. Nothing the base did is in it, however
 * the base came in; a moved file is only where it is now. The work it comes from is the
 * unit's commits that touched the file, not a merge of the base. Without a net change (the
 * sample's other units, a service that could not tell) the unit's commits taken together.
 */
export function wholeDelta(unit: Unit, path: string): FileDelta | null {
  if (!unit.net) return fileDelta(unit, path, unit.commits.map((_, k) => k))
  const live = unit.commits.at(-1)?.live ? unit.commits.at(-1)!.files.find(f => f.path === path) : undefined
  const net = unit.net.files.find(f => f.path === path)
  const first = net ?? live, last = live ?? net
  if (!first || !last || same(first, last)) return null
  const touched = unit.commits.flatMap((c, k) => (!c.merge && c.files.some(f => f.path === path) ? [k] : []))
  const { add, del } = counted(first, last)
  const upTo = unit.commits.length - 1
  return { touched, before: first.before, after: last.after, upTo, status: statusOf(first, last), add, del, parts: [{ touched, upTo, status: statusOf(first, last), before: first.before, after: last.after, add, del, ...kindOf(first, last) }] }
}

// Line counts from one file version's `before` to another's `after`, kept per pair: a file's
// versions stay the same objects while its unit's code is unchanged, and diffing whole texts
// again on every pick made picking a task in a big unit take up to a second. A change with no
// lines (binary, a submodule's pointer) counts none, whatever text the other end has.
const counts = new WeakMap<FileChange, WeakMap<FileChange, { add: number; del: number }>>()
function counted(first: FileChange, last: FileChange) {
  if (first === last) return { add: first.add, del: first.del }
  const kind = kindOf(first, last)
  if (kind.binary || kind.pointer) return { add: 0, del: 0 }
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
  return fileDelta(unit, path, sel.commits) ?? wholeDelta(unit, path)
}

export function fileTree(root: ModuleNode, unit: Unit, sel: Selection, onlyChanged: boolean, narrow = false): FileTree {
  const repo = new Set<string>(), dirs = new Set<string>()
  const walk = (n: ModuleNode) => { if (n.children) { if (n.path) dirs.add(n.path); n.children.forEach(walk) } else repo.add(n.path) }
  walk(root)
  // What the unit did, file by file, as one change over a scope: the pick's commits for the
  // files it touched, the whole unit's for the others. The single-file view shows the same.
  const did = new Map<string, FileDelta & { inPick: boolean }>()
  // A path with no change in either scope (gone from the branch) is not drawn.
  for (const path of new Set([...unit.commits.flatMap(c => c.files.map(f => f.path)), ...(unit.net?.files ?? []).map(f => f.path)])) {
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
