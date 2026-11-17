// Settings: the few choices that belong to the person, not to the work on screen.
import { useEffect, useState } from 'react'
import { Settings as Gear } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Switch } from '@/components/ui/switch'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { MOCK } from '@/data'
import { machines as loadMachines, metrics as loadMetrics, notifyWanted, record, setMachine, setThresholds, wantNotify } from '@/runtime'
import type { Levels, Machine, Thresholds, Workspace } from '@/types'
import { Trans } from 'react-i18next'
import { LANGS, useT, type Lang } from '@/i18n'
import { readTheme, setTheme, THEMES, type Theme } from '@/theme'
import { Choice } from './Choice'

/** Not linked, in the link picker (Radix takes no empty value). */
const NONE = '-'

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <span className="text-sm">{label}</span>
      {children}
    </div>
  )
}

/**
 * The other machines: add one (a name and its ssh target), follow it or not, see whether its
 * connection is up and why not; and link a workspace over there to one here.
 */
function Machines({ ws }: { ws: Workspace | null }) {
  const { t, date } = useT()
  const [list, setList] = useState<Machine[]>([])
  const [error, setError] = useState('')
  const [name, setName] = useState('')
  const [ssh, setSsh] = useState('')
  const fail = (e: unknown) => setError(t('machines.failed', { error: String(e) }))
  const act = (p: Promise<Machine[]>) => p.then(l => { setList(l); setError('') }, fail)
  // The list as the service has it; what is live about each comes with the data.
  useEffect(() => { loadMachines().then(setList, () => {}) }, [ws?.machines?.length])
  const live = (m: Machine) => ws?.machines?.find(x => x.label === m.label) ?? m
  const state = (m: Machine) => !m.enabled ? t('machines.off') : m.connecting ? t('machines.connecting') : m.tunnel && m.reachable ? t('machines.connected', { count: m.agents }) : m.tunnel ? t('machines.noHerdr') : m.downSince ? t('machines.downSince', { time: date(m.downSince) }) : t('machines.down')
  const here = (ws?.workspaces ?? []).filter(g => !g.machine)
  const there = (ws?.workspaces ?? []).filter(g => g.machine)
  const link = (machine: string, workspace: string, to: string, was?: string) => {
    const undo = was ? record('workspace.unlinked', { machine, workspace }) : Promise.resolve()
    undo.then(() => (to ? record('workspace.linked', { machine, workspace, local: to }) : undefined)).then(() => setError(''), fail)
  }
  const field = 'w-0 min-w-0 flex-1 rounded-sm border border-line bg-surface px-2 py-1 text-sm'
  return (
    <section aria-labelledby="machines" className="flex flex-col gap-2 border-t border-line pt-3">
      <h3 id="machines" className="text-sm font-semibold">{t('machines.title')}</h3>
      <p className="text-xs text-ink-2">{list.length ? t('machines.lead') : t('machines.none')}</p>
      {list.map(live).map(m => (
        <div key={m.label} className="flex flex-col gap-0.5">
          <div className="flex items-center gap-2">
            <span className="min-w-0 flex-1 truncate text-sm">{m.label} <span className="font-mono text-xs text-ink-2">{m.ssh}</span></span>
            <span className="flex-none text-xs text-ink-2">{state(m)}</span>
            <Switch checked={m.enabled} aria-label={t('machines.follow', { machine: m.label })} onCheckedChange={on => act(setMachine(m.label, on ? 'enable' : 'disable'))} />
            <Button variant="ghost" size="icon-sm" aria-label={t('machines.remove', { machine: m.label })} onClick={() => act(setMachine(m.label, 'remove'))}>×</Button>
          </div>
          {m.enabled && (m.tunnelError || m.error) && <p className="text-xs wrap-anywhere text-ink-2">{m.tunnelError ?? m.error}</p>}
          {m.enabled && !m.tunnel && m.retryIn != null && <p className="text-xs text-ink-2">{t('machines.retry', { count: m.retryIn })}</p>}
        </div>
      ))}
      <form className="flex gap-1.5" onSubmit={e => { e.preventDefault(); act(setMachine(name.trim(), 'add', ssh.trim()).then(l => { setName(''); setSsh(''); return l })) }}>
        <input className={field} value={name} onChange={e => setName(e.target.value)} placeholder={t('machines.name')} aria-label={t('machines.name')} />
        <input className={field} value={ssh} onChange={e => setSsh(e.target.value)} placeholder={t('machines.ssh')} aria-label={t('machines.ssh')} />
        <Button type="submit" size="sm" variant="outline" disabled={!name.trim() || !ssh.trim()}>{t('machines.add')}</Button>
      </form>
      {there.length > 0 && (
        <>
          <h4 className="pt-1 text-xs font-semibold">{t('machines.links')}</h4>
          <p className="text-xs text-ink-2">{t('machines.linkLead')}</p>
          {there.map(g => (
            <label key={g.id} className="grid grid-cols-[minmax(0,1fr)_minmax(0,16rem)] items-center gap-2 text-sm">
              <span className="truncate" title={`${g.machine}:${g.label}`}><span className="font-mono text-xs text-ink-2">{g.machine}:</span>{g.label}</span>
              {/* A linked workspace that is not open here now still shows what it is linked to. */}
              <Choice
                label={t('machines.linkTo', { machine: g.machine, workspace: g.label })}
                value={g.linkedTo ?? NONE}
                onChange={v => link(g.machine!, g.label, v === NONE ? '' : v, g.linkedTo)}
                options={[{ value: NONE, label: t('machines.notLinked') }, ...[...new Set([...here.map(h => h.label), ...(g.linkedTo ? [g.linkedTo] : [])])].map(l => ({ value: l, label: l }))]}
                className="h-7 w-full"
              />
            </label>
          ))}
        </>
      )}
      {error && <p role="alert" className="text-xs wrap-anywhere text-you-ink">{error}</p>}
    </section>
  )
}

/** A whole number from `min` to `max`, given back when the field is left or Enter pressed. */
function Count({ value, min, max, label, disabled, onCommit }: { value: number; min: number; max: number; label: string; disabled: boolean; onCommit: (v: number) => void }) {
  const [text, setText] = useState(String(value))
  useEffect(() => setText(String(value)), [value])
  // Out of range or not a number: back to what it was; the service would refuse it too.
  const commit = () => {
    const v = Number(text)
    if (Number.isInteger(v) && v >= min && v <= max) { if (v !== value) onCommit(v) } else setText(String(value))
  }
  return (
    <input
      type="number" inputMode="numeric" min={min} max={max} step={1} value={text} aria-label={label} disabled={disabled}
      onChange={e => setText(e.target.value)} onBlur={commit} onKeyDown={e => { if (e.key === 'Enter') commit() }}
      className="w-14 rounded-sm border border-line bg-surface px-2 py-1 text-right text-sm text-ink tabular-nums disabled:opacity-50"
    />
  )
}

type Group = 'all' | 'mac' | 'linux'
/** What each alert edits, by the system that has it. */
const ALERTS: [Group, (keyof Thresholds)[]][] = [
  ['all', ['cpu', 'memory', 'disk']],
  ['mac', ['compressor', 'swap', 'pressure', 'systemFree']],
  ['linux', ['available', 'psi', 'oom']],
]
/** A field's bounds: percents, GB of swap and of free disk, minutes. */
const MAX = { percent: 100, gb: 1024, free: 100_000 }

/**
 * When a machine alerts, the same for this machine and every followed one, grouped by the
 * system that has the value: each a switch and a sentence with its fields (warning and
 * critical thresholds, how long, where it applies). A field takes only what keeps warning
 * before critical. Saved as soon as one changes; refused, the service's values come back.
 */
function MachineAlerts() {
  const { t } = useT()
  const [limits, setLimits] = useState<Thresholds | null>(null)
  const [error, setError] = useState('')
  useEffect(() => { loadMetrics().then(r => setLimits(r.thresholds), () => {}) }, [])
  if (!limits) return null
  const save = (next: Thresholds) => {
    setLimits(next)
    setThresholds(next).then(
      r => { setLimits(r.thresholds); setError('') },
      e => { setError(t('alerts.failed', { error: String(e) })); loadMetrics().then(r => setLimits(r.thresholds), () => {}) },
    )
  }
  const row = (m: keyof Thresholds) => {
    const name = t(`metrics.${m}`)
    const value = limits[m]
    const set = (patch: object) => save({ ...limits, [m]: { ...value, ...patch } })
    const field = (key: string, v: number, min: number, max: number, label: string) => (
      <Count value={v} min={min} max={max} label={label} disabled={!value.on} onCommit={x => set({ [key]: x })} />
    )
    // An alert at once has no minutes; its sentence names none.
    const minutes = 'minutes' in value && value.minutes != null ? field('minutes', value.minutes, 1, 10, t('alerts.minutes', { metric: name })) : <></>
    // Warning and critical, each bounded by the other: over them, or under them for a share free.
    const pair = (l: Levels, under: boolean, max: number) => ({
      warn: field('warn', l.warn, under ? l.crit : 0, under ? max : l.crit, t('alerts.warn', { metric: name })),
      crit: field('crit', l.crit, under ? 0 : l.warn, under ? l.warn : max, t('alerts.crit', { metric: name })),
      minutes,
    })
    let sentence: React.ReactNode
    if (m === 'cpu' || m === 'memory') {
      const l = limits[m]
      sentence = <Trans i18nKey="alerts.threshold" components={{ percent: field('percent', l.percent, 1, 100, t('alerts.percent', { metric: name })), minutes }} />
    } else if (m === 'compressor' || m === 'psi') {
      sentence = <Trans i18nKey={limits[m].minutes == null ? 'alerts.overPercentNow' : 'alerts.overPercent'} components={pair(limits[m], false, MAX.percent)} />
    } else if (m === 'swap') {
      sentence = <Trans i18nKey="alerts.overGb" components={pair(limits.swap, false, MAX.gb)} />
    } else if (m === 'systemFree' || m === 'available') {
      sentence = <Trans i18nKey="alerts.underPercent" components={pair(limits[m], true, MAX.percent)} />
    } else if (m === 'pressure' || m === 'oom') {
      sentence = <Trans i18nKey={`alerts.${m}`} components={{ minutes }} />
    } else {
      const d = limits.disk
      sentence = (
        <Trans
          i18nKey="alerts.disk"
          components={{
            warn: field('warn', d.warn, 0, d.crit, t('alerts.warn', { metric: name })),
            crit: field('crit', d.crit, d.warn, MAX.percent, t('alerts.crit', { metric: name })),
            warnFree: field('warnFree', d.warnFree, d.critFree, MAX.free, t('alerts.warnFree', { metric: name })),
            critFree: field('critFree', d.critFree, 0, d.warnFree, t('alerts.critFree', { metric: name })),
            minutes,
          }}
        />
      )
    }
    return (
      // The switch by its name; the sentence wraps as one piece on a narrow screen.
      <div key={m} className="flex flex-wrap items-center gap-x-4 gap-y-2 text-sm">
        <span className="flex w-36 items-center gap-2">
          <Switch checked={value.on} aria-label={t('alerts.on', { metric: name })} onCheckedChange={on => set({ on })} />
          {name}
        </span>
        {/* One sentence with its fields in it, in whatever order the language puts them. */}
        <span className="flex flex-wrap items-center gap-1.5 text-ink-2">{sentence}</span>
      </div>
    )
  }
  return (
    <section aria-labelledby="alerts" className="flex flex-col gap-2 border-t border-line pt-3">
      <h3 id="alerts" className="text-sm font-semibold">{t('alerts.title')}</h3>
      <p className="text-xs text-ink-2">{t('alerts.lead')}</p>
      {ALERTS.map(([group, metrics]) => (
        <div key={group} role="group" aria-labelledby={`alerts-${group}`} className="flex flex-col gap-2">
          <h4 id={`alerts-${group}`} className="pt-1 text-xs font-semibold text-ink-2">{t(`alerts.group.${group}`)}</h4>
          {metrics.map(row)}
        </div>
      ))}
      {error && <p role="alert" className="text-xs wrap-anywhere text-you-ink">{error}</p>}
    </section>
  )
}

/** `compact`: only the gear, for the folded rail. */
export function Settings({ compact = false, ws = null }: { compact?: boolean; ws?: Workspace | null }) {
  const { t, lang, setLang } = useT()
  const [theme, pickTheme] = useState<Theme>(readTheme)
  // Off until the user turns it on; the browser's permission is asked only then.
  const [notifying, setNotifying] = useState(notifyWanted)
  const [denied, setDenied] = useState(false)
  const item = 'h-7 px-3 text-xs font-normal data-[state=on]:font-semibold'
  return (
    <Dialog>
      <DialogTrigger asChild>
        {compact ? (
          <Button variant="ghost" size="icon-sm" aria-label={t('settings.title')} className="rounded-sm text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink">
            <Gear aria-hidden size={16} strokeWidth={1.5} />
          </Button>
        ) : (
          <Button variant="ghost" size="sm" className="justify-start gap-2 rounded-sm px-2 text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink">
            <Gear aria-hidden size={16} strokeWidth={1.5} />
            {t('settings.title')}
          </Button>
        )}
      </DialogTrigger>
      <DialogContent closeLabel={t('settings.close')} className="max-h-[85vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t('settings.title')}</DialogTitle>
        </DialogHeader>
        <div className="flex min-w-0 flex-col gap-3">
          <Row label={t('settings.language')}>
            <ToggleGroup type="single" variant="outline" size="sm" value={lang} onValueChange={v => v && setLang(v as Lang)} aria-label={t('settings.language')}>
              {LANGS.map(l => <ToggleGroupItem key={l.id} value={l.id} lang={l.id} className={item}>{l.label}</ToggleGroupItem>)}
            </ToggleGroup>
          </Row>
          <Row label={t('settings.theme')}>
            <ToggleGroup
              type="single" variant="outline" size="sm" value={theme} aria-label={t('settings.theme')}
              onValueChange={v => { if (v) { setTheme(v as Theme); pickTheme(v as Theme) } }}
            >
              {THEMES.map(id => <ToggleGroupItem key={id} value={id} className={item}>{t(`settings.themes.${id}`)}</ToggleGroupItem>)}
            </ToggleGroup>
          </Row>
          <Row label={t('settings.notify')}>
            <Switch
              checked={notifying}
              aria-label={t('settings.notify')}
              onCheckedChange={on => { wantNotify(on).then(ok => { setNotifying(ok); setDenied(on && !ok) }) }}
            />
          </Row>
          {denied && <p className="text-xs text-ink-2">{t('settings.notifyDenied')}</p>}
          <MachineAlerts />
          {!MOCK && <Machines ws={ws} />}
        </div>
      </DialogContent>
    </Dialog>
  )
}
