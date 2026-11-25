import { expect, it } from 'vitest'
import { gateStats, intentStats, ratingStats, reviewStats, type Records, type RecordedTask } from './statistics'

const localTime = (hour: number) => new Date(2026, 10, 26, hour).toISOString()

it('groups votes including missing metadata and keeps broadcast reply numbers and newest downs', () => {
  const usage = { model: 'test-model', harness: 'codex', harness_version: '1.0', compactions: 0, context_tokens: 0 }
  const { total, groups, downs } = ratingStats([
    { id: 'm1', rating: { vote: 'up', at: localTime(7), usage, session: 'first-session' }, replies: [] },
    { id: 'm2', replies: [
      { rating: { vote: 'down', at: localTime(17), usage, session: 'first-session', note: 'Missed the question' } },
      { rating: { vote: 'down', at: localTime(7), usage: { compactions: 4, context_tokens: 128_000 } } },
      { rating: { vote: 'up', at: 'invalid' } },
      {},
    ] },
  ])

  expect(total).toEqual({ up: 2, down: 2 })
  expect(Object.fromEntries(groups.model)).toEqual({ 'test-model': { up: 1, down: 1 }, '': { up: 1, down: 1 } })
  expect(Object.fromEntries(groups.version)).toEqual({ 'codex 1.0': { up: 1, down: 1 }, '': { up: 1, down: 1 } })
  expect(groups.session.map(([key]) => key)).toEqual(['first-session', ''])
  expect(Object.fromEntries(groups.day)).toEqual({ '2026-11-26': { up: 1, down: 2 }, '': { up: 1, down: 0 } })
  expect(groups.hour.map(([key]) => key)).toEqual(['07', '17', ''])
  expect(groups.compactions.map(([key]) => key)).toEqual(['0', '3+', ''])
  expect(groups.context.map(([key]) => key)).toEqual(['0–<32k', '128k+', ''])
  expect(downs.map(({ message, reply, note }) => [message, reply, note])).toEqual([
    ['m2', 1, 'Missed the question'], ['m2', 2, undefined],
  ])
})

it('orders intent totals by creation and retains untitled and empty intents', () => {
  const one = { intent: 'i1', created: '2026-11-24T23:00:00Z', active_days: 1.5, units: 2, tasks: 4, tasks_by_kind: { implement: 1, review: 2, gate: 1 }, review_rounds: { u1: 1, u2: 0 }, dropped_tasks: 1 }
  const two = { intent: 'i2', created: '2026-11-26T00:00:00Z', active_days: 0, units: 0, tasks: 0, tasks_by_kind: {}, review_rounds: {}, dropped_tasks: 0 }
  const records: Records = { intents: [{ id: 'i1', title: 'An intent' }], intent_took: [one, two], units: [], messages: [] }
  expect(intentStats(records)).toEqual([{ ...two, title: undefined }, { ...one, title: 'An intent' }])
})

const at = (minutes: number) => new Date(Date.UTC(2026, 10, 26, 0, minutes)).toISOString()
const task = (n: number, kind: string, extra: Partial<RecordedTask> = {}): RecordedTask => ({
  n, kind, state: 'done', inputs: [], ...extra,
})

it('counts review decisions and missing models, and counts completed rounds on the go gate path once', () => {
  const tasks = [
    task(1, 'implement'),
    task(2, 'review', { done: at(2), model: 'model-a', inputs: [{ from: '1' }], findings: [{ severity: 'P1', state: 'accepted' }, { severity: 'P2', state: 'done', decisions: [{ state: 'deferred' }] }] }),
    task(3, 'review', { done: at(2), inputs: [{ from: '1' }], findings: [{ severity: 'P3', state: 'open' }, { severity: 'P2', state: 'revised' }, { state: 'rejected' }, {}] }),
    task(4, 'gate', { gate: 'decide', waiting_since: at(3), placed: { id: 'go', filled: true }, inputs: [{ from: '2' }, { from: '3' }] }),
    task(5, 'design', { inputs: [{ from: '4' }] }),
    task(6, 'review', { done: at(6), inputs: [{ from: '5' }] }),
    task(7, 'gate', { gate: 'decide', choice: {}, waiting_since: at(7), inputs: [{ from: '6' }] }),
    task(8, 'gate', { gate: 'test', findings: [{ severity: 'P1', state: 'open' }] }),
    task(9, 'gate', { gate: 'decide', waiting_since: at(9), inputs: [{ from: '6' }] }),
  ]
  const result = reviewStats([
    { id: 'u1', tasks },
    { id: 'u2', tasks: [task(1, 'gate', { gate: 'decide', waiting_since: at(1) }), task(2, 'gate', { gate: 'decide', state: 'todo', placed: { id: 'go' } })] },
  ])
  expect(Object.fromEntries(result.severity)).toEqual({ P1: 1, P2: 2, P3: 1, '': 2 })
  expect(Object.fromEntries(result.decision)).toEqual({ accepted: 1, revised: 1, rejected: 1, deferred: 1, open: 1, '': 1 })
  expect(Object.fromEntries(result.reviewer)).toEqual({ 'model-a': 2, '': 4 })
  expect(result.beforeGo).toEqual([
    { unit: 'u1', gate: 4, rounds: 1 }, { unit: 'u2', gate: undefined, rounds: undefined },
  ])
})

it('counts opened, answered, dropped and recorded re-asks, and keeps missing answer times', () => {
  const gate = (n: number, extra: Partial<RecordedTask>) => task(n, 'gate', { gate: 'decide', ...extra })
  const result = gateStats([{ id: 'u1', tasks: [
    gate(1, { result: 'done', waiting_since: at(0), done: at(4) }),
    gate(2, { result: 'problem', waiting_since: at(1), done: at(3) }),
    gate(3, { result: 'answered', done: at(1) }),
    gate(4, { state: 'dropped', waiting_since: at(2), dropped: { reason: 'superseded by u1#5: the branch changed' } }),
    gate(5, { state: 'waiting', waiting_since: at(3) }),
    gate(6, { state: 'dropped', waiting_since: at(4), dropped: { reason: 'no longer needed' } }),
    gate(7, { state: 'todo' }),
    gate(8, { state: 'dropped', placed: { id: 'confirm' } }),
  ] }])
  expect(result).toEqual({
    opened: 6, answered: 3, dropped: 2, askedAgain: 1, reaskUnknown: 1,
    median: 3 * 60_000, longest: 4 * 60_000, answerUnknown: 1,
  })
  expect(gateStats([])).toMatchObject({ opened: 0, answered: 0, median: undefined, longest: undefined })
})
