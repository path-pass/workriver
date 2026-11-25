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
  text: string
  rating?: Rating
  replies: { text: string; rating?: Rating }[]
}

interface IntentTook {
  intent: string
  created: string
  active_days: number
  units: number
  tasks: number
  tasks_by_kind: Record<string, number>
  review_rounds: Record<string, number>
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
  model?: string
  done?: string | null
  findings?: RecordedFinding[]
  result?: string | null
  waiting_since?: string
  dropped?: { reason: string }
  placed?: { id: string; filled?: boolean }
}

interface RecordedUnit { id: string; intent: string; tasks: RecordedTask[] }

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
  const entries = messages.flatMap(m => [m, ...m.replies]
    .flatMap(({ rating, text: replyText }, reply) => rating ? [{ ...rating, message: m.id, reply, replyText }] : []))
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
  const droppedSteps = new Map<string, number>()
  for (const unit of records.units) {
    const count = unit.tasks.filter(task => task.placed && task.state === 'dropped').length
    droppedSteps.set(unit.intent, (droppedSteps.get(unit.intent) ?? 0) + count)
  }
  return records.intent_took.map(row => ({
    ...row, title: titles.get(row.intent), droppedSteps: droppedSteps.get(row.intent) ?? 0,
  }))
    .sort((a, b) => newest(b.created) - newest(a.created) || b.intent.localeCompare(a.intent, undefined, { numeric: true }))
}

const filled = (task: RecordedTask) => !task.placed || task.placed.filled === true

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
  let opened = 0, answered = 0, dropped = 0, reasked = 0
  const waits: number[] = []
  for (const unit of units) {
    for (const gate of unit.tasks.filter(t => t.kind === 'gate' && filled(t))) {
      const start = time(gate.waiting_since)
      if (gate.waiting_since || gate.result || gate.state === 'waiting') opened++
      if (gate.state === 'dropped') {
        dropped++
        if (askedAgain(unit, gate)) reasked++
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
  return { opened, answered, dropped, askedAgain: reasked, median, longest: waits.at(-1) }
}
