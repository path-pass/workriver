import { useEffect, useState, type ReactNode } from 'react'
import { pathOf, replyPath } from '@/app/link'
import { read } from '@/api/http'
import { contentLang, useT } from '@/i18n'
import { Button } from '@/ui/primitives/button'
import { MOCK } from '@/workspace/source'
import { gateStats, harnessVersion, intentStats, ratingStats, reviewStats, type Records, type Votes } from './statistics'

type Row = [key: string, ...cells: ReactNode[]]

function Table({ label, columns, rows }: { label: string; columns: string[]; rows: Row[] }) {
  const { t } = useT()
  if (!rows.length) return <p className="text-sm text-ink-2">{label} · {t('dashboard.empty')}</p>
  return (
    <div className="min-w-0 overflow-x-auto rounded-md border border-line">
      <table className="w-full text-sm tabular-nums">
        <caption className="sr-only">{label}</caption>
        <thead className="bg-sunken text-ink-2">
          <tr>{columns.map(column => <th key={column} scope="col" className="px-3 py-2 text-left font-semibold">{column}</th>)}</tr>
        </thead>
        <tbody>
          {rows.map(([key, name, ...cells]) => (
            <tr key={key} className="border-t border-line align-top">
              <th scope="row" className="px-3 py-2 text-left font-normal wrap-anywhere">{name}</th>
              {cells.map((cell, i) => <td key={i} className="px-3 py-2">{cell}</td>)}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function Ratings({ messages }: { messages: Records['messages'] }) {
  const { t, lang, num, date, when } = useT()
  const { total, groups, downs } = ratingStats(messages)
  const unknown = t('dashboard.unknown')
  const percent = new Intl.NumberFormat(lang, { style: 'percent', maximumFractionDigits: 1 })
  const counts = ({ up, down }: Votes) => [num(up), num(down), up + down ? percent.format(down / (up + down)) : unknown]
  const columns = [t('talk.rating.up'), t('talk.rating.down'), t('dashboard.downShare')]
  const groupLabel = (key: keyof typeof groups, name: string) => {
    if (!name) return unknown
    if (key === 'hour') return `${name}:00`
    if (key === 'day') return date(name + 'T12:00:00', false)
    return name
  }
  const keys = Object.keys(groups) as (keyof typeof groups)[]
  return (
    <section aria-labelledby="dashboard-ratings" className="flex flex-col gap-3">
      <h2 id="dashboard-ratings" className="text-base font-semibold">{t('dashboard.ratings')}</h2>
      <Table label={t('dashboard.ratings')} columns={[t('dashboard.group'), ...columns]} rows={[[ 'total', t('dashboard.total'), ...counts(total) ]]} />
      {total.up + total.down > 0 && (
        <div className="grid items-start gap-3 lg:grid-cols-2 xl:grid-cols-3">
          {keys.map(key => {
            const label = t(`dashboard.ratingGroups.${key}`)
            return <Table key={key} label={label} columns={[label, ...columns]} rows={groups[key].map(([name, count]) => [name, groupLabel(key, name), ...counts(count)])} />
          })}
        </div>
      )}
      <h3 className="mt-1 text-sm font-semibold">{t('dashboard.downs')}</h3>
      <Table label={t('dashboard.downs')} columns={[t('dashboard.reply'), t('dashboard.ratedAt'), t('dashboard.ratingGroups.model'), t('dashboard.ratingGroups.version'), t('dashboard.note')]} rows={downs.map(r => {
        const ref = r.reply ? `${r.message}/${r.reply}` : r.message
        const at = when(Date.parse(r.at))
        return [
          ref,
          <a href={replyPath(r)} target="_blank" rel="noreferrer" className="font-mono underline underline-offset-2">{ref}</a>,
          at ? <time dateTime={r.at} className="whitespace-nowrap">{at}</time> : unknown,
          r.usage?.model?.trim() || unknown, harnessVersion(r.usage) || unknown,
          r.note ? <span lang={contentLang(r.note)} className="whitespace-pre-wrap wrap-anywhere">{r.note}</span> : '—',
        ]
      })} />
    </section>
  )
}

function Intents({ records }: { records: Records }) {
  const { t, lang, num } = useT()
  const decimal = new Intl.NumberFormat(lang, { maximumFractionDigits: 3 })
  const taskKind = (kind: string) => {
    switch (kind) {
      case 'implement': case 'review': case 'gate': case 'design': case 'research': return t(`kind.${kind}`)
      case 'tests': return t('dashboard.tests')
      default: return t('dashboard.unknown')
    }
  }
  const label = t('dashboard.intents')
  const list = new Intl.ListFormat(lang)
  return (
    <section aria-labelledby="dashboard-intents" className="flex flex-col gap-3">
      <h2 id="dashboard-intents" className="text-base font-semibold">{label}</h2>
      <Table label={label} columns={[
        t('dashboard.intent'), t('dashboard.units'), t('dashboard.tasks'), t('dashboard.taskKinds'),
        t('dashboard.roundsPerUnit'), t('dashboard.droppedSteps'), t('dashboard.activeDays'),
      ]} rows={intentStats(records).map(row => [
        row.intent,
        <span><span className="mr-2 font-mono text-ink-2">{row.intent}</span><span lang={contentLang(row.title)}>{row.title || t('dashboard.unknown')}</span></span>,
        num(row.units), num(row.tasks), list.format(Object.entries(row.tasks_by_kind).map(([k, n]) => `${taskKind(k)} ${num(n)}`)) || '—',
        list.format(Object.entries(row.review_rounds).map(([u, n]) => t('dashboard.unitRounds', { unit: u, rounds: num(n) }))) || '—',
        num(row.dropped_tasks), decimal.format(row.active_days),
      ])} />
    </section>
  )
}

function Reviews({ units }: { units: Records['units'] }) {
  const { t, num } = useT()
  const { severity, decision, reviewer, beforeGo } = reviewStats(units)
  const unknown = t('dashboard.unknown')
  const decisionLabel = (key: string) => {
    switch (key) {
      case 'accepted': return t('finding.acceptedHere')
      case 'revised': return t('dashboard.revised')
      case 'rejected': return t('finding.rejected')
      case 'deferred': return t('finding.deferred')
      case 'open': return t('dashboard.open')
      default: return unknown
    }
  }
  const table = (label: string, rows: [string, number][], name = (key: string) => key || unknown) => (
    <Table label={label} columns={[label, t('dashboard.count')]} rows={rows.map(([key, count]) => [key, name(key), num(count)])} />
  )
  const rounds = t('dashboard.beforeGo')
  return (
    <section aria-labelledby="dashboard-reviews" className="flex flex-col gap-3">
      <h2 id="dashboard-reviews" className="text-base font-semibold">{t('dashboard.reviews')}</h2>
      <div className="grid items-start gap-3 lg:grid-cols-3">
        {table(t('dashboard.severity'), severity)}
        {table(t('dashboard.decision'), decision, decisionLabel)}
        {table(t('dashboard.reviewerModel'), reviewer)}
      </div>
      <h3 className="mt-1 text-sm font-semibold">{rounds}</h3>
      <Table label={rounds} columns={[t('dashboard.unit'), t('dashboard.goGate'), t('dashboard.rounds')]} rows={beforeGo.map(({ unit, gate, rounds }) => [
        unit,
        <span className="font-mono">{unit}</span>,
        gate === undefined ? t('dashboard.goPending') : <a className="font-mono underline underline-offset-2" href={pathOf(unit, { kind: 'task', id: String(gate) })} target="_blank" rel="noreferrer">#{gate}</a>,
        rounds === undefined ? unknown : num(rounds),
      ])} />
    </section>
  )
}

function Gates({ units }: { units: Records['units'] }) {
  const { t, lang, num } = useT()
  const g = gateStats(units)
  const duration = (ms?: number) => {
    if (ms === undefined) return t('dashboard.unknown')
    const seconds = ms / 1000
    const [unit, scale] = seconds < 60 ? ['second', 1] : seconds < 3600 ? ['minute', 60] : seconds < 86400 ? ['hour', 3600] : ['day', 86400]
    return new Intl.NumberFormat(lang, { style: 'unit', unit, maximumFractionDigits: 1 }).format(seconds / scale)
  }
  const label = t('dashboard.gates')
  return (
    <section aria-labelledby="dashboard-gates" className="flex flex-col gap-3">
      <h2 id="dashboard-gates" className="text-base font-semibold">{label}</h2>
      <div className="grid items-start gap-3 lg:grid-cols-2">
        <Table label={label} columns={[t('dashboard.gateEvent'), t('dashboard.count')]} rows={[
          ['opened', t('dashboard.opened'), num(g.opened)],
          ['answered', t('dashboard.answered'), num(g.answered)],
          ['dropped', t('dashboard.dropped'), num(g.dropped)],
          ['again', t('dashboard.askedAgain'), num(g.askedAgain)],
          ['again-unknown', t('dashboard.reaskUnknown'), num(g.reaskUnknown)],
        ]} />
        <Table label={t('dashboard.answerTime')} columns={[t('dashboard.answerTime'), t('dashboard.value')]} rows={[
          ['median', t('dashboard.median'), duration(g.median)],
          ['longest', t('dashboard.longest'), duration(g.longest)],
          ['unknown', t('dashboard.answerUnknown'), num(g.answerUnknown)],
        ]} />
      </div>
    </section>
  )
}

export function Dashboard({ onClose }: { onClose: () => void }) {
  const { t } = useT()
  const [records, setRecords] = useState<Records | null>(null)
  const [failed, setFailed] = useState(false)
  useEffect(() => {
    let current = true
    // The normal snapshot only includes vote direction and presentation fields.
    const request = MOCK ? Promise.resolve({ intents: [], intent_took: [], units: [], messages: [] }) : read<Records>('/api/state?took=true')
    request.then(data => { if (current) setRecords(data) }, () => { if (current) setFailed(true) })
    return () => { current = false }
  }, [])

  return (
    <main className="mx-auto flex max-w-[90rem] flex-col gap-8 px-6 pt-5 pb-10 max-[860px]:p-4">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-xl font-[650]">{t('dashboard.title')}</h1>
        <Button type="button" variant="outline" size="sm" autoFocus onClick={onClose}>{t('dashboard.back')}</Button>
      </header>
      {failed ? <p role="alert" className="text-sm text-del">{t('dashboard.failed')}</p> : records ? (
        <>
          <Ratings messages={records.messages} />
          <Intents records={records} />
          <Reviews units={records.units} />
          <Gates units={records.units} />
        </>
      ) : <p role="status" className="text-sm text-ink-2">{t('settings.loading')}</p>}
    </main>
  )
}
