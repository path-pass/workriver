// Fields used from the existing /api/state?took=true report.
import type { Vote } from '@/conversations/types'
import { CONTEXT_TOKEN_BANDS } from '@/config/defaults'

interface Usage {
  model?: string
  harness?: string
  harness_version?: string
  compactions?: number
  context_tokens?: number
}

interface Rating {
  vote: Vote
  at: string
  session?: string
  note?: string
  usage?: Usage
}

interface RatedMessage {
  id: string
  rating?: Rating
  replies: { rating?: Rating }[]
}

interface IntentTook {
  intent: string
  created: string
  active_days: number
  units: number
  tasks: number
  tasks_by_kind: Record<string, number>
  review_rounds: Record<string, number>
  dropped_tasks: number
}

interface RecordedFinding {
  severity?: string
  state?: string
  decisions?: { state: string }[]
}

export interface RecordedTask {
  n: number
  kind: string
  state: string
  inputs: { from: string }[]
  model?: string
  done?: string | null
  findings?: RecordedFinding[]
  gate?: string | null
  choice?: unknown
  result?: string | null
  waiting_since?: string
  dropped?: { reason: string }
  placed?: { id: string; filled?: boolean }
}

interface RecordedUnit { id: string; tasks: RecordedTask[] }

export interface Records {
  intents: { id: string; title: string }[]
  intent_took: IntentTook[]
  units: RecordedUnit[]
  messages: RatedMessage[]
}

export interface Votes { up: number; down: number }

const text = (s?: string) => s?.trim() ?? ''
const time = (s?: string | null) => Date.parse(s ?? '')
const newest = (s?: string) => Number.isFinite(time(s)) ? time(s) : -Infinity
const twoDigits = (n: number) => String(n).padStart(2, '0')

// Unrecorded values use the empty key and follow known values in every table.
function ordered<T>(counts: Map<string, T>, compare = (a: string, b: string) => a.localeCompare(b)): [string, T][] {
  return [...counts].sort(([a], [b]) => !a || !b ? Number(!a) - Number(!b) : compare(a, b))
}

export function harnessVersion(usage?: Usage) {
  const version = text(usage?.harness_version)
  return version ? [text(usage?.harness), version].filter(Boolean).join(' ') : ''
}

function contextBand(tokens?: number) {
  if (tokens == null) return ''
  const index = CONTEXT_TOKEN_BANDS.findIndex(limit => tokens < limit)
  const k = (n: number) => n === 0 ? '0' : `${n / 1000}k`
  if (index === -1) return `${k(CONTEXT_TOKEN_BANDS.at(-1)!)}+`
  return `${k(CONTEXT_TOKEN_BANDS[index - 1] ?? 0)}–<${k(CONTEXT_TOKEN_BANDS[index])}`
}

export function ratingStats(messages: RatedMessage[]) {
  const entries = messages.flatMap(m => [m.rating, ...m.replies.map(r => r.rating)]
    .flatMap((rating, reply) => rating ? [{ ...rating, message: m.id, reply }] : []))
  const total = { up: 0, down: 0 }
  const groups = {
    model: new Map<string, Votes>(), version: new Map<string, Votes>(), session: new Map<string, Votes>(),
    day: new Map<string, Votes>(), hour: new Map<string, Votes>(), compactions: new Map<string, Votes>(),
    context: new Map<string, Votes>(),
  }
  for (const { at, session, vote, usage } of entries) {
    const date = new Date(at)
    const dated = Number.isFinite(date.getTime())
    const keys = {
      model: text(usage?.model),
      version: harnessVersion(usage),
      session: text(session),
      day: dated ? `${date.getFullYear()}-${twoDigits(date.getMonth() + 1)}-${twoDigits(date.getDate())}` : '',
      hour: dated ? twoDigits(date.getHours()) : '',
      compactions: usage?.compactions == null ? '' : usage.compactions >= 3 ? '3+' : String(usage.compactions),
      context: contextBand(usage?.context_tokens),
    }
    total[vote]++
    for (const group of Object.keys(keys) as (keyof typeof keys)[]) {
      const key = keys[group]
      const counts = groups[group].get(key) ?? { up: 0, down: 0 }
      counts[vote]++
      groups[group].set(key, counts)
    }
  }
  return {
    total,
    groups: {
      model: ordered(groups.model), version: ordered(groups.version), session: ordered(groups.session),
      day: ordered(groups.day, (a, b) => b.localeCompare(a)), hour: ordered(groups.hour),
      compactions: ordered(groups.compactions), context: ordered(groups.context, (a, b) => parseInt(a) - parseInt(b)),
    },
    downs: entries.filter(r => r.vote === 'down').sort((a, b) => newest(b.at) - newest(a.at)),
  }
}

export function intentStats(records: Records) {
  const titles = new Map(records.intents.map(i => [i.id, i.title]))
  return records.intent_took.map(row => ({ ...row, title: titles.get(row.intent) }))
    .sort((a, b) => newest(b.created) - newest(a.created) || b.intent.localeCompare(a.intent, undefined, { numeric: true }))
}

const filled = (task: RecordedTask) => !task.placed || task.placed.filled === true

function beforeGo({ id, tasks }: RecordedUnit): { unit: string; gate?: number; rounds?: number }[] {
  const possible = tasks.filter(t => t.kind === 'gate' && t.gate === 'decide' && !t.choice && t.state !== 'dropped')
  // The development template's go precedes push, PR and merge approvals.
  const placedGo = possible.find(t => t.placed?.id === 'go')
  const candidates = placedGo ? [placedGo] : possible.filter(filled)
  if (!candidates.length) return []
  const gate = candidates.filter(t => filled(t) && Number.isFinite(time(t.waiting_since)))
    .sort((a, b) => time(b.waiting_since) - time(a.waiting_since) || b.n - a.n)[0]
  if (!gate) return [{ unit: id, gate: undefined, rounds: undefined }]

  const byNumber = new Map(tasks.map(t => [String(t.n), t]))
  const visited = new Set<string>()
  const reviewed = new Set<number>()
  const pending = gate.inputs.map(i => i.from)
  const openedAt = time(gate.waiting_since)
  while (pending.length) {
    const from = pending.pop()!
    if (visited.has(from)) continue
    visited.add(from)
    const task = byNumber.get(from)
    if (!task) continue
    pending.push(...task.inputs.map(i => i.from))
    if (task.kind !== 'review' || task.state !== 'done' || !filled(task) || !(time(task.done) <= openedAt)) continue
    for (const i of task.inputs) {
      const work = byNumber.get(i.from)
      if (work && filled(work) && work.state !== 'dropped' && (work.kind === 'implement' || work.kind === 'design')) reviewed.add(work.n)
    }
  }
  return [{ unit: id, gate: gate.n, rounds: reviewed.size }]
}

const DECISIONS = ['accepted', 'revised', 'rejected', 'deferred', 'open']
const SEVERITIES = ['P1', 'P2', 'P3']

export function reviewStats(units: RecordedUnit[]) {
  const severity = new Map(SEVERITIES.map(key => [key, 0]))
  const decision = new Map(DECISIONS.map(key => [key, 0]))
  const reviewer = new Map<string, number>()
  const count = (counts: Map<string, number>, key: string) => counts.set(key, (counts.get(key) ?? 0) + 1)
  for (const review of units.flatMap(u => u.tasks).filter(t => t.kind === 'review')) {
    for (const finding of review.findings ?? []) {
      const state = finding.decisions?.at(-1)?.state ?? finding.state ?? ''
      const level = finding.severity ?? ''
      count(severity, SEVERITIES.includes(level) ? level : '')
      count(decision, DECISIONS.includes(state) ? state : '')
      count(reviewer, text(review.model))
    }
  }
  return {
    severity: [...severity],
    decision: [...decision],
    reviewer: ordered(reviewer),
    beforeGo: units.flatMap(beforeGo),
  }
}

function askedAgain(unit: RecordedUnit, gate: RecordedTask) {
  // The records have no replacement field. Recognize the forms in the ask-user recipe.
  const reason = gate.dropped?.reason.trim() ?? ''
  if (/^(?:asked (?:again|anew)|re-?asked)\b/i.test(reason)) return true
  const next = /^superseded by (u\d+)?#(\d+)\b/i.exec(reason)
  return !!next && (!next[1] || next[1] === unit.id)
    && unit.tasks.some(t => t.n === Number(next[2]) && t.kind === 'gate' && filled(t))
}

export function gateStats(units: RecordedUnit[]) {
  let opened = 0, answered = 0, dropped = 0, reasked = 0, reaskUnknown = 0
  const waits: number[] = []
  for (const unit of units) {
    for (const gate of unit.tasks.filter(t => t.kind === 'gate' && filled(t))) {
      const start = time(gate.waiting_since)
      if (gate.waiting_since || gate.result || gate.state === 'waiting') opened++
      if (gate.state === 'dropped') {
        dropped++
        if (askedAgain(unit, gate)) reasked++
        else reaskUnknown++
      }
      if (!gate.result) continue
      answered++
      const wait = time(gate.done) - start
      if (Number.isFinite(wait) && wait >= 0) waits.push(wait)
    }
  }
  waits.sort((a, b) => a - b)
  const middle = Math.floor(waits.length / 2)
  const median = waits.length % 2 ? waits[middle] : waits.length ? (waits[middle - 1] + waits[middle]) / 2 : undefined
  return { opened, answered, dropped, askedAgain: reasked, reaskUnknown, median, longest: waits.at(-1), answerUnknown: answered - waits.length }
}
