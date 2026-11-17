// The Machines section of the side panel: CPU and memory of this machine and of every
// followed one over the last ten minutes, read from the service every few seconds while
// the section is open, and below them the newest of what its system says beyond that
// (pressure, swap, disk...). Lines are neutral; a metric in alert takes the violet of
// "waiting for you", the only colour that means the user should look. The memory pressure
// word has a colour of its own: green, amber, red, as the system rates it.
import { type ReactNode, useEffect, useState } from 'react'
import { ms } from '@/data'
import { useT } from '@/i18n'
import { metrics as loadMetrics, metricsHistory } from '@/runtime'
import type { Limit, MachineMetrics, Metric, MetricSample, MetricsReport, Thresholds } from '@/types'
import { MachineChip } from './bits'

/** What the lines show: the last ten minutes, as the service keeps them. */
const WINDOW = 10 * 60 * 1000
const METRICS: Metric[] = ['cpu', 'memory']
const GB = 1024 ** 3

/** A sample's value in percent; null while there is none (the first Linux sample's CPU). */
export const valueOf = (s: MetricSample, m: Metric) => (m === 'cpu' ? s.cpu : s.memTotal ? (s.memUsed * 100) / s.memTotal : null)

/**
 * One metric over the window, 0–100 %, with the threshold as a dashed line. Inline SVG: a
 * polyline and a line do not need a chart library (recharts would add tens of kB gzip).
 */
function Spark({ samples, metric, limit, label }: { samples: MetricSample[]; metric: Metric; limit: Limit; label: string }) {
  const now = Date.now()
  const x = (at: number) => ((at - (now - WINDOW)) / WINDOW) * 120
  const y = (v: number) => 21.5 - (Math.min(Math.max(v, 0), 100) / 100) * 21
  const points = samples.filter(s => s.at >= now - WINDOW).flatMap(s => {
    const v = valueOf(s, metric)
    return v == null ? [] : [[x(s.at), y(v)] as const]
  })
  const line = points.map(([px, py], k) => `${k ? 'L' : 'M'}${px.toFixed(1)} ${py.toFixed(1)}`).join(' ')
  const area = points.length > 1 ? `${line} L${points.at(-1)![0].toFixed(1)} 22 L${points[0][0].toFixed(1)} 22 Z` : ''
  return (
    <svg className="spark" viewBox="0 0 120 22" preserveAspectRatio="none" role="img" aria-label={label}>
      {limit.on && <line className="th" x1="0" x2="120" y1={y(limit.percent)} y2={y(limit.percent)} vectorEffect="non-scaling-stroke" />}
      {area && <path className="area" d={area} />}
      {line && <path className="ln" d={line} vectorEffect="non-scaling-stroke" />}
    </svg>
  )
}

const DAY = 24 * 60 * 60 * 1000

/**
 * CPU and memory over the last 24 hours, one point a minute from the history the service
 * keeps on disk: two lines in one small inline SVG, memory in the water colour so the two
 * tell apart. A gap of more than two minutes (the machine was away) breaks the lines.
 * Read when the card shows and every minute after (one new line a minute).
 */
function DayChart({ machine }: { machine: string }) {
  const { t } = useT()
  const [points, setPoints] = useState<MetricSample[] | null>(null)
  useEffect(() => {
    let stopped = false
    const read = () => metricsHistory(machine, 24).then(p => { if (!stopped) setPoints(p) }, () => {})
    read()
    const timer = setInterval(read, 60_000)
    return () => { stopped = true; clearInterval(timer) }
  }, [machine])
  const now = Date.now()
  const x = (at: number) => ((at - (now - DAY)) / DAY) * 240
  const y = (v: number) => 31.5 - (Math.min(Math.max(v, 0), 100) / 100) * 31
  const path = (metric: Metric) => {
    let d = ''
    let before = -Infinity
    for (const p of points ?? []) {
      const v = valueOf(p, metric)
      if (v == null) { before = -Infinity; continue }
      d += `${p.at - before > 120_000 ? 'M' : 'L'}${x(p.at).toFixed(1)} ${y(v).toFixed(1)} `
      before = p.at
    }
    return d
  }
  return (
    <div className="mline mday">
      <span className="k">{t('metrics.day')}</span>
      <svg className="spark day" viewBox="0 0 240 32" preserveAspectRatio="none" role="img" aria-label={t('metrics.dayChart')}>
        <path className="ln" d={path('cpu')} vectorEffect="non-scaling-stroke" />
        <path className="ln mem" d={path('memory')} vectorEffect="non-scaling-stroke" />
      </svg>
      <span className="v legend">
        {points && !points.length ? t('metrics.noHistory') : <><i className="sw" />{t('metrics.cpu')} <i className="sw mem" />{t('metrics.memory')}</>}
      </span>
    </div>
  )
}

/** The newest sample's readings beyond CPU and memory, those its system has. */
function Facts({ s }: { s: MetricSample }) {
  const { t, lang } = useT()
  const one = { maximumFractionDigits: 1 }
  const unit = (u: string) => new Intl.NumberFormat(lang, { style: 'unit', unit: u, ...one })
  // Bytes in the largest unit under 1024 of it, in steps of 1024 as the memory line counts.
  const bytes = (b: number) => (b >= 1024 * GB ? unit('terabyte').format(b / 1024 / GB) : b >= GB ? unit('gigabyte').format(b / GB) : unit('megabyte').format(b / 1024 ** 2))
  const pct = new Intl.NumberFormat(lang, { maximumFractionDigits: 2 })
  const facts: [string, ReactNode, string?][] = []
  if (s.memTotal) facts.push([t('metrics.available'), bytes(s.memTotal - s.memUsed)])
  if (s.pressure) facts.push([t('metrics.pressure'), <span className="pressure" data-level={s.pressure}>{t(`metrics.pressureLevel.${s.pressure}`)}</span>])
  else if (s.os === 'linux')
    facts.push(s.pressureSome != null && s.pressureFull != null
      ? [t('metrics.pressure'), t('metrics.pressureStall', { some: pct.format(s.pressureSome), full: pct.format(s.pressureFull) }), t('metrics.pressureStallHint')]
      : [t('metrics.pressure'), t('metrics.pressureUnknown')])
  if (s.memCompressed != null) facts.push([t('metrics.compressed'), bytes(s.memCompressed)])
  if (s.oomKills != null) facts.push([t('metrics.oomKills'), t('metrics.oomValue', { total: s.oomKills, watched: s.oomKillsWatched ?? 0 })])
  if (s.swapTotal) facts.push([t('metrics.swap'), t('metrics.swapValue', { used: bytes(s.swapUsed ?? 0), total: bytes(s.swapTotal) })])
  s.disks.forEach((d, k) => facts.push([k ? t('metrics.diskAt', { mount: d.mount }) : t('metrics.disk'), t('metrics.diskFree', { free: bytes(d.free) }), d.mount]))
  if (!facts.length) return null
  return (
    <dl className="mfacts">
      {facts.map(([k, v, title]) => (
        <div key={k} title={title}><dt>{k}</dt><dd>{v}</dd></div>
      ))}
    </dl>
  )
}

function MachineRow({ m, thresholds }: { m: MachineMetrics; thresholds: Thresholds }) {
  const { t, lang, date, waited } = useT()
  const last = m.samples.at(-1)
  const gb = new Intl.NumberFormat(lang, { maximumFractionDigits: 1 })
  const percent = (metric: Metric) => {
    const v = last && valueOf(last, metric)
    return v == null ? t('metrics.noValue') : t('metrics.percent', { value: Math.round(v) })
  }
  const shown = (metric: Metric) =>
    metric === 'memory' && last ? t('metrics.memoryValue', { used: gb.format(last.memUsed / GB), total: gb.format(last.memTotal / GB) }) : percent(metric)
  return (
    <div className="mrow" data-away={!m.reachable || undefined}>
      <div className="mhead">
        {/* This machine by its role; another one by its chip, dashed while it cannot be reached. */}
        {m.here ? <span className="name">{t('metrics.here')}</span> : <MachineChip machine={m.label} away={!m.reachable} since={m.downSince} />}
        {METRICS.filter(x => m.alerts[x]).map(x => (
          <span key={x} className="mpill">{t('metrics.alert', { metric: t(`metrics.${x}`), value: percent(x), time: waited(ms(m.alerts[x])) })}</span>
        ))}
        {!m.reachable && m.downSince && <span className="mnote">{t('machines.downSince', { time: date(m.downSince) })}</span>}
      </div>
      {m.error && <p className="mnote">{t('metrics.probeFailed', { error: m.error })}</p>}
      {METRICS.map(x => (
        <div key={x} className="mline" data-hot={m.alerts[x] ? true : undefined}>
          <span className="k">{t(`metrics.${x}`)}</span>
          <Spark samples={m.samples} metric={x} limit={thresholds[x]} label={t('metrics.chart', { metric: t(`metrics.${x}`) })} />
          <span className="v">{shown(x)}</span>
        </div>
      ))}
      <DayChart machine={m.label} />
      {last && <Facts s={last} />}
    </div>
  )
}

/** Every machine, this one first; read again every step while the section is open. */
export function MachinesList() {
  const { t } = useT()
  const [report, setReport] = useState<MetricsReport | null>(null)
  const [down, setDown] = useState(false)
  useEffect(() => {
    let stopped = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const tick = () =>
      loadMetrics().then(
        r => { if (stopped) return; setReport(r); setDown(false); timer = setTimeout(tick, r.stepSeconds * 1000) },
        () => { if (stopped) return; setDown(true); timer = setTimeout(tick, 5000) },
      )
    tick()
    return () => { stopped = true; clearTimeout(timer) }
  }, [])
  if (!report) return down ? <p className="px-2.5 text-sm text-ink-2">{t('service.unreachable')}</p> : null
  return (
    <>
      <p className="mb-1 px-2.5 text-xs text-ink-2">{t('metrics.lead', { seconds: report.stepSeconds })}</p>
      {report.machines.map(m => <MachineRow key={m.label} m={m} thresholds={report.thresholds} />)}
    </>
  )
}
