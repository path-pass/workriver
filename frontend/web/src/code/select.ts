// The commits across units: newest first, and as a git graph.
import type { Commit, Repo } from './types'
import type { Task } from '@/work/types'
import type { Intent, Unit } from '@/planning/types'
import type { Workspace } from '@/workspace/types'
import { working, type Working } from '@/work/select'
import { ms, unitsOf } from '@/planning/select'

/** The repository a unit works in. */
export const repoOf = (ws: Workspace, unit: Unit) => ws.repos.find(r => r.path === unit.repo && r.machine === unit.machine)!

export const commitStats = (c: Commit) => ({
  files: c.files.length,
  add: c.files.reduce((s, f) => s + f.add, 0),
  del: c.files.reduce((s, f) => s + f.del, 0),
})

// ---- the commits of every unit, newest first ----

export interface FeedRow { intent: Intent; unit: Unit; commit: Commit; task: Task | null }

/** Every commit across intents and units, newest first; work not committed yet is left out. */
export function feed(ws: Workspace): FeedRow[] {
  return ws.intents
    .flatMap(intent => intent.units.flatMap(unit => unit.commits.filter(c => !c.live).map(commit => ({
      intent, unit, commit, task: unit.tasks.find(n => n.output.commits?.includes(commit.id)) ?? null,
    }))))
    .sort((a, b) => ms(b.commit.date) - ms(a.commit.date))
}

/** Agents with uncommitted changes right now, and the files they are writing. */
export const writingNow = (ws: Workspace) =>
  ws.intents.flatMap(intent => intent.units.flatMap(unit => working(unit).filter(w => w.files.length).map(w => ({ intent, unit, ...w }))))

// ---- the commits as a git graph ----

/** A unit's lane in one row: whether it goes on into the row above and the row below. */
export interface GraphSeg {
  lane: number
  unit: Unit
  /** line: on to a newer row of its unit; join: it merges into the base here. */
  up: 'line' | 'join' | null
  /** line: on to an older row; fork: it leaves the base here. */
  down: 'line' | 'fork' | null
  /** Dashed: what is not committed yet. */
  upDash: boolean
  downDash: boolean
}
export interface GraphRow {
  unit: Unit
  /** A commit, with its task (null: none found) … */
  commit?: Commit
  task: Task | null
  /** … or an agent's uncommitted work. */
  writing?: Working
  /** The row's own lane (from 1; 0 is the base). */
  lane: number
  /** Every unit lane drawn in this row, its own included. */
  segs: GraphSeg[]
}
export interface Graph {
  repo: Repo | undefined
  /** The base branch the units leave from (several, joined, when they differ). */
  base: string
  rows: GraphRow[]
  lanes: number
}

/** Which of the lane colours (`--lane-0` … `--lane-5`) a unit's branch takes: from its id, so it stays put. */
export const laneTone = (unit: string) => [...unit].reduce((n, ch) => (n * 31 + ch.charCodeAt(0)) % 997, 7) % 6

/**
 * Every unit's commits as `git log --graph` draws them, newest first, one graph per
 * repository: the base as lane 0 (a plain line, labelled with its name: its own commits are not drawn), one
 * lane per unit from its first commit, where it leaves the base, to its last, where a merged
 * unit joins it again (at `merged` when known). Uncommitted work is on top, dashed. A lane is
 * used again once the unit on it has ended, so the graph is as wide as the most units
 * running side by side.
 */
export function commitGraph(ws: Workspace): Graph[] {
  const live = writingNow(ws)
  const repos = [...new Set(unitsOf(ws).filter(u => u.commits.length).map(u => u.repo))]
  return repos.map(repo => {
    const rows: GraphRow[] = [
      ...live.filter(w => w.unit.repo === repo).map(w => ({ unit: w.unit, writing: { node: w.node, files: w.files }, task: w.node, lane: 0, segs: [] })),
      ...feed(ws).filter(r => r.unit.repo === repo).map(r => ({ unit: r.unit, commit: r.commit, task: r.task, lane: 0, segs: [] })),
    ]
    const time = (r: GraphRow) => (r.writing ? Infinity : ms(r.commit!.date))
    const spans = new Map<Unit, { top: number; bottom: number; join: boolean; dash: [number, number] | null }>()
    rows.forEach((r, i) => {
      const s = spans.get(r.unit)
      if (s) s.bottom = i
      else spans.set(r.unit, { top: i, bottom: i, join: false, dash: null })
    })
    for (const [u, s] of spans) {
      s.join = u.status === 'merged' || u.progress === 'merged'
      // Merged later than its last commit: its lane runs on up to where it was merged.
      const at = ms(u.merged)
      const k = s.join && Number.isFinite(at) ? rows.findIndex(r => time(r) <= at) : -1
      if (k >= 0) s.top = Math.min(s.top, k)
      // Uncommitted work is dashed down to the unit's newest commit (or to where it forks).
      const w = rows.findIndex(r => r.unit === u && r.writing)
      if (w >= 0) { const c = rows.findIndex((r, i) => i > w && r.unit === u && !r.writing); s.dash = [w, c < 0 ? s.bottom + 1 : c] }
    }
    // Lanes, first fit by where each unit starts (from the top): one ended frees its lane.
    const lane = new Map<Unit, number>(), ends: number[] = []
    for (const [u, s] of [...spans].sort((a, b) => a[1].top - b[1].top)) {
      let k = ends.findIndex(e => e < s.top)
      if (k < 0) k = ends.length
      ends[k] = s.bottom
      lane.set(u, k + 1)
    }
    rows.forEach((r, i) => {
      r.lane = lane.get(r.unit)!
      for (const [u, s] of spans) {
        if (i < s.top || i > s.bottom) continue
        const d = s.dash
        r.segs.push({
          lane: lane.get(u)!, unit: u,
          up: i > s.top ? 'line' : s.join ? 'join' : null,
          down: i < s.bottom ? 'line' : 'fork',
          upDash: !!d && i > d[0] && i <= d[1],
          downDash: !!d && i >= d[0] && i < d[1],
        })
      }
    })
    const base = [...new Set(unitsOf(ws).filter(u => u.repo === repo && u.base).map(u => u.base!))].join(', ')
    return { repo: ws.repos.find(x => x.path === repo), base, rows, lanes: ends.length }
  })
}
