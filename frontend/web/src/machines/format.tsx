// A machine in words: an alert's value, what it is past and the processes it names, and the
// chip that says which machine something is on.
import { type T } from '@/i18n'
import type { MachineAlert } from './types'
import { contentLang, useT } from '@/i18n'
import { Tip } from '@/ui/Tip'
import { Icon } from '@/ui/Icon'

export const GB = 1024 ** 3

/** An alert's value as words: "97 %", "5.2 GB", "critical", "2 kills", "/: 92 % used, 18 GB free". */
export function alertValue(t: T, lang: string, a: Pick<MachineAlert, 'metric' | 'value' | 'level' | 'disk'>) {
  const one = new Intl.NumberFormat(lang, { maximumFractionDigits: 1 })
  switch (a.metric) {
    case 'swap': return t('metrics.gb', { value: one.format(a.value) })
    case 'pressure': return t(`metrics.pressureLevel.${a.level === 'crit' ? 'critical' : 'warning'}`)
    case 'oom': return t('metrics.kills', { count: a.value })
    case 'psi': return t('metrics.percent', { value: one.format(a.value) })
    case 'disk': if (a.disk) return t('metrics.diskAlert', { mount: a.disk.mount, used: Math.round(a.value), free: t('metrics.gb', { value: one.format(a.disk.free / GB) }) })
  }
  return t('metrics.percent', { value: Math.round(a.value) })
}

/** What an alert is past, as words: "over 4 GB for 1 min", "a process was killed; held 5 min". */
export function alertLimit(t: T, a: MachineAlert) {
  const minutes = a.minutes ?? 0
  const x = a.threshold ?? 0
  switch (a.metric) {
    case 'cpu': case 'memory': return t('inbox.machineLimit', { percent: x, minutes })
    case 'pressure': return t('inbox.limit.pressure', { level: t(`metrics.pressureLevel.${a.level === 'crit' ? 'critical' : 'warning'}`), minutes })
    case 'oom': return t('inbox.limit.oom', { minutes })
    case 'disk': return t('inbox.limit.disk', { used: x, free: a.thresholdFree ?? 0, minutes })
    case 'systemFree': case 'available': return t('inbox.limit.under', { value: t('metrics.percent', { value: x }), minutes })
    case 'swap': return t('inbox.limit.over', { value: t('metrics.gb', { value: x }), minutes })
  }
  const value = t('metrics.percent', { value: x })
  return a.minutes == null ? t('inbox.limit.overNow', { value }) : t('inbox.limit.over', { value, minutes })
}

/** The processes an alert names, as one line: "claude 0.7 GB (reviewer · chat), Chrome 2.3 GB". */
export function topLine(t: T, lang: string, a: MachineAlert) {
  if (!a.top.length) return ''
  const one = new Intl.NumberFormat(lang, { maximumFractionDigits: 1 })
  const list = a.top.map(p => {
    const size = t('metrics.gb', { value: one.format(p.rss / GB) })
    return p.agent ? t('metrics.procAgent', { name: p.name, size, agent: p.place ? `${p.agent} · ${p.place}` : p.agent }) : t('metrics.proc', { name: p.name, size })
  })
  return t('metrics.top', { list: new Intl.ListFormat(lang, { type: 'unit' }).format(list) })
}

/** The machine something is on, when it is not this one: a small chip with its name. */
export function MachineChip({ machine, away = false, since }: { machine?: string; away?: boolean; since?: string }) {
  const { t, date } = useT()
  if (!machine) return null
  const why = since ? t('machines.disconnected', { machine, time: date(since) }) : t('machines.unreachable', { machine })
  return (
    <Tip text={away ? why : t('machines.on', { machine })}>
      <span className="ext-chip" data-away={away || undefined}><Icon name="machine" className="size-3" /><span className="chip-name" lang={contentLang(machine)}>{machine}</span></span>
    </Tip>
  )
}
