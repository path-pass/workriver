// The Machines section of the side panel: CPU and memory of this machine and of every
// followed one over the last ten minutes, read from the service every few seconds while
// the section is open. Lines are neutral; a metric in alert takes the violet of "waiting
// for you", the only colour that means the user should look.
import { useEffect, useState } from 'react'
import { ms } from '@/data'
import { useT } from '@/i18n'
import { metrics as loadMetrics } from '@/runtime'
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
