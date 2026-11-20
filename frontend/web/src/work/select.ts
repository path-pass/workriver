// A unit's work as the views read it: labels, findings, what is related to what is picked,
// and which commits and plans the pick covers.
import type { T } from '@/i18n'
import type { Finding, Severity, Task, Pick } from './types'
import type { Unit } from '@/planning/types'

// ---- findings ----

export const SEVERITIES: Severity[] = ['P1', 'P2', 'P3']
export const findingsOf = (n: Task): Finding[] => n.output.findings ?? []
/** Findings still waiting for the user's decision. */
/** Open findings that wait for the user: not those the user handed to the intent's manager. */
const waits = (f: Finding) => f.state === 'open' && f.decider !== 'manager'
export const openCount = (n: Task) => findingsOf(n).filter(waits).length
/** Findings counted per severity, e.g. 'P1 2 · P2 3'. */
export const severityCounts = (found: Finding[]) =>
  SEVERITIES.map(s => [s, found.filter(f => f.severity === s).length] as const)
    .filter(([, k]) => k)
    .map(([s, k]) => `${s} ${k}`)
    .join(' · ')

// ---- labels ----

/** Who found a finding: the reviewer, or the kind of gate. */
export const finderLabel = (t: T, n: Task): string =>
  n.gate ? t(`gate.${n.gate}`) : n.reviewer ? t(`reviewer.${n.reviewer}`) : t('kind.review')

export function reviewLabel(t: T, n: Task): string {
  const by = finderLabel(t, n)
  if (n.state === 'dropped') return t('review.dropped', { by })
  if (n.result === 'running') return t('review.running', { by })
  if (n.result === 'found') return t('review.found', { by, count: findingsOf(n).length })
  return t('review.passed', { by })
}
/** Finished work handed in with no check run: shown on the graph, not counted anywhere. */
export const untested = (n: Task) => n.kind === 'implement' && !n.design && !n.research && n.state === 'done' && !n.output.evidence?.length

export function nodeLabel(t: T, n: Task): string {
  if (n.kind === 'review') return reviewLabel(t, n)
  if (n.state === 'unregistered') return t('task.unregistered', { title: n.title })
  return n.taskId ? `#${n.taskId} ${n.title}` : n.title
}


// ---- the graph ----

/**
 * The picked node with everything it came from and everything that came out of it. With a
 * finding picked: the review and the nodes that finding leads to.
 */
export function related(unit: Unit, id: string, finding?: string): Set<string> {
  const byId = new Map(unit.tasks.map(n => [n.id, n]))
  const review = byId.get(id)
  const f = finding && review ? findingsOf(review).find(x => x.id === finding) : undefined
  if (f) return new Set([id, ...f.leadsTo])
  const lit = new Set([id])
  const up = (n: string) =>
    byId.get(n)?.inputs.forEach(i => {
      if (!lit.has(i.from)) { lit.add(i.from); up(i.from) }
    })
  const down = (n: string) =>
    unit.tasks.forEach(k => {
      if (!lit.has(k.id) && k.inputs.some(i => i.from === n)) { lit.add(k.id); down(k.id) }
    })
  up(id)
  down(id)
  return lit
}

// ---- selection: which commits and planned places belong to what is picked ----

export interface Selection {
  node: Task | null
  /** Indexes into unit.commits that the pick covers; empty for "the unit as a whole". */
  commits: number[]
  /** Planned places of the picked node; null when no node is picked. */
  plans: string[] | null
}

export function selectionOf(unit: Unit, pick: Pick): Selection {
  const node = pick ? unit.tasks.find(n => n.id === pick.id) : undefined
  if (!node) return { node: null, commits: [], plans: null }
  // A review or gate stands for the work it followed and the work handed out for its
  // findings; one finding of it, for the work handed out for that finding. A finding that
  // led to nothing yet (still open, say) stands for the review as a whole.
  const finding = findingsOf(node).find(f => f.id === pick?.finding && f.leadsTo.length)
  const group =
    node.kind !== 'implement'
      ? [...new Set(finding ? finding.leadsTo : [node.inputs.find(i => i.primary)?.from, ...findingsOf(node).flatMap(f => f.leadsTo)])]
          .map(id => unit.tasks.find(n => n.id === id))
          .filter((n): n is Task => !!n)
      : [node]
  const ids = new Set(group.flatMap(n => n.output.commits ?? []))
  const commits = unit.commits.flatMap((c, k) => (ids.has(c.id) ? [k] : []))
  return {
    node,
    commits,
    plans: group.filter(n => n.state !== 'done').flatMap(n => n.plans),
  }
}

// ---- what wants a look ----

export interface Working {
  node: Task
  /** Files sitting uncommitted in the agent's working directory. */
  files: string[]
}
export function working(unit: Unit): Working[] {
  return unit.tasks
    .filter(n => n.kind === 'implement' && n.state === 'running')
    .map(node => ({
      node,
      files: unit.commits.filter(c => c.live && node.output.commits?.includes(c.id)).flatMap(c => c.files.map(f => f.path)),
    }))
}

/** The open findings of a node, the most severe first. */
export const openFindings = (n: Task) =>
  findingsOf(n).filter(waits).sort((a, b) => SEVERITIES.indexOf(a.severity) - SEVERITIES.indexOf(b.severity))
