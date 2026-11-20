// Where a unit's work grew in scope: along each line of work through its task graph, the
// folders changed so far; a task that reaches folders outside all of them is marked.
import type { Task } from '@/work/types'
import type { Unit } from '@/planning/types'

export interface Growth {
  /** The folders it reached that nothing before it on its line did, sorted ('' is the root). */
  folders: string[]
  /** The work before it on its line that widened the scope (its first folders included), in graph order. */
  beyond: Task[]
}

/** A file's folder with its trailing '/'; '' for a file at the root. */
const dirOf = (p: string) => p.slice(0, p.lastIndexOf('/') + 1)

// One entry per unit: what it was worked out from, and the marks.
const cache = new Map<string, { key: string; marks: Map<string, Growth> }>()

/**
 * The marked tasks of a unit, by task id. Only commits count, by the folders of their files:
 * not plans (not changed yet), not uncommitted work (it changes all the time), not merge
 * commits (the base's work, not the task's). Reviews and gates change nothing. A task with
 * nothing changed before it on its line (the first on it) is not marked. Where lines join,
 * "before" is everything before any of them.
 *
 * Worked out again only when the graph's shape or a task's commits change (commits keep their
 * files for good): one topological pass carrying each line's folders as a shared Set, copied
 * only where the scope grows and someone else still holds it, or where lines join. O(V + E +
 * files) plus O(folders) per copy; the "beyond" lists walk back only from marked tasks.
 */
export function growth(unit: Unit): Map<string, Growth> {
  const key = unit.tasks.map(n => `${n.id}:${n.kind}<${n.inputs.map(i => i.from).join(',')}>${n.output.commits?.join(',') ?? ''}`).join(';')
  const hit = cache.get(unit.id)
  if (hit?.key === key) return hit.marks

  const dirs = new Map<string, string[]>()
  for (const c of unit.commits) if (!c.live && !c.merge) dirs.set(c.id, [...new Set(c.files.map(f => dirOf(f.path)))])
  const byId = new Map(unit.tasks.map(n => [n.id, n]))
  const parents = new Map<string, string[]>(), kids = new Map<string, string[]>()
  for (const n of unit.tasks) {
    const ps = [...new Set(n.inputs.map(i => i.from))].filter(p => byId.has(p))
    parents.set(n.id, ps)
    for (const p of ps) (kids.get(p) ?? kids.set(p, []).get(p)!).push(n.id)
  }
  // Kahn's order; a cycle (none should exist) leaves its nodes out.
  const waiting = new Map(unit.tasks.map(n => [n.id, parents.get(n.id)!.length]))
  const order = unit.tasks.filter(n => !waiting.get(n.id)).map(n => n.id)
  for (let k = 0; k < order.length; k++)
    for (const c of kids.get(order[k]) ?? []) {
      const left = waiting.get(c)! - 1
      waiting.set(c, left)
      if (!left) order.push(c)
    }

  // refs: how many tasks are still to read the scope; at zero the last reader may change it.
  type Scope = { set: Set<string>; refs: number }
  const none: Scope = { set: new Set(), refs: Infinity }
  const after = new Map<string, Scope>()
  const marks = new Map<string, Growth>()
  const widened = new Set<string>()
  for (const id of order) {
    const n = byId.get(id)!, ps = parents.get(id)!
    let scope: Scope, own: boolean
    if (!ps.length) [scope, own] = [none, false]
    else if (ps.length === 1) { scope = after.get(ps[0])!; own = --scope.refs === 0 }
    else {
      const ins = ps.map(p => after.get(p)!).sort((a, b) => b.set.size - a.set.size)
      ins.forEach(s => s.refs--)
      scope = { set: new Set(ins[0].set), refs: 0 }
      for (const s of ins.slice(1)) for (const f of s.set) scope.set.add(f)
      own = true
    }
    const touched = n.kind === 'review' || n.kind === 'gate' ? [] : [...new Set((n.output.commits ?? []).flatMap(c => dirs.get(c) ?? []))]
    const fresh = touched.filter(f => !scope.set.has(f))
    if (fresh.length) {
      if (scope.set.size) marks.set(id, { folders: fresh.sort(), beyond: [] })
      widened.add(id)
      if (!own) scope = { set: new Set(scope.set), refs: 0 }
      for (const f of fresh) scope.set.add(f)
    }
    scope.refs += kids.get(id)?.length ?? 0
    after.set(id, scope)
  }
  const rank = new Map(order.map((id, k) => [id, k]))
  for (const [id, g] of marks) {
    const seen = new Set<string>(), todo = [...parents.get(id)!]
    while (todo.length) {
      const p = todo.pop()!
      if (seen.has(p)) continue
      seen.add(p)
      todo.push(...parents.get(p)!)
    }
    g.beyond = [...seen].filter(p => widened.has(p)).sort((a, b) => rank.get(a)! - rank.get(b)!).map(p => byId.get(p)!)
  }
  cache.set(unit.id, { key, marks })
  return marks
}
