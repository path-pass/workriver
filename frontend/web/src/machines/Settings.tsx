// Settings: the few choices that belong to the person, not to the work on screen.
import { useEffect, useState } from 'react'
import { Settings as Gear } from 'lucide-react'
import { Button } from '@/ui/primitives/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/ui/primitives/dialog'
import { Switch } from '@/ui/primitives/switch'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/ui/primitives/tabs'
import { ToggleGroup, ToggleGroupItem } from '@/ui/primitives/toggle-group'
import { MOCK } from '@/workspace/source'
import { machines as loadMachines, metrics as loadMetrics, notifyWanted, record, RUN_GROUP, setMachine, setStallMinutes, setThresholds, stallMinutes, wantNotify } from '@/api/actions'
import type { RuntimeWorkspace } from '@/agents/types'
import type { Levels, Machine, Thresholds } from './types'
import type { Workspace } from '@/workspace/types'
import { Trans } from 'react-i18next'
import { contentLang, LANGS, useT, type Lang } from '@/i18n'
import { readTheme, setTheme, THEMES, type Theme } from '@/theme'
import { Content, Slot, SLOT } from '@/content/Chips'
import { Choice } from '@/ui/Choice'
import { chime, soundMissed, soundWanted, wantSound } from '@/attention/sound'

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
    <div className="flex flex-col gap-2">
      <p className="text-xs text-ink-2">{list.length ? t('machines.lead') : t('machines.none')}</p>
      {list.map(live).map(m => (
        <div key={m.label} className="flex flex-col gap-0.5">
          <div className="flex items-center gap-2">
            <span className="min-w-0 flex-1 truncate text-sm"><span lang={contentLang(m.label)}>{m.label}</span> <span className="font-mono text-xs text-ink-2">{m.ssh}</span></span>
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
              <span className="truncate" title={`${g.machine}:${g.label}`}><span className="font-mono text-xs text-ink-2">{g.machine}:</span><span lang={contentLang(g.label)}>{g.label}</span></span>
              {/* A linked workspace that is not open here now still shows what it is linked to. */}
              <Choice
                label={t('machines.linkTo', { machine: g.machine, workspace: g.label })}
                value={g.linkedTo ?? NONE}
                onChange={v => link(g.machine!, g.label, v === NONE ? '' : v, g.linkedTo)}
                options={[{ value: NONE, label: t('machines.notLinked') }, ...[...new Set([...here.map(h => h.label), ...(g.linkedTo ? [g.linkedTo] : [])])].map(l => ({ value: l, label: l, lang: contentLang(l) }))]}
                className="h-7 w-full"
              />
            </label>
          ))}
        </>
      )}
      {error && <p role="alert" className="text-xs wrap-anywhere text-you-ink">{error}</p>}
    </div>
  )
}

/** Worked out by workriver, in the project picker. */
const AUTO = '-'

/** The workspaces of this machine a project can be set for. */
const ownWorkspaces = (ws: Workspace | null) => (ws?.workspaces ?? []).filter(g => !g.machine && g.label !== RUN_GROUP)

/**
 * Which project each workspace here works on: worked out by workriver (from where its
 * manager runs, else its current intent), or set by the user, which wins. Starting an
 * intent of another project there asks first.
 */
function WorkspaceProjects({ ws }: { ws: Workspace | null }) {
  const { t } = useT()
  const [error, setError] = useState('')
  const list = ownWorkspaces(ws)
  const projects = ws?.projects ?? []
  if (!list.length || !projects.length) return null
  const set = (g: RuntimeWorkspace, v: string) =>
    record('workspace.project_set', { workspace: { id: g.id, label: g.label }, project: v === AUTO ? null : v }).then(() => setError(''), e => setError(t('machines.failed', { error: String(e) })))
  return (
    <div className="flex flex-col gap-2">
      <p className="text-xs text-ink-2">{t('wsProject.lead')}</p>
      {list.map(g => (
        <label key={g.id} className="grid grid-cols-[minmax(0,1fr)_minmax(0,16rem)] items-center gap-2 text-sm">
          <span className="truncate" lang={contentLang(g.label)}>{g.label}</span>
          <Choice
            label={t('wsProject.pick', { workspace: g.label })}
            value={g.projectFrom === 'user' && g.project ? g.project : AUTO}
            onChange={v => set(g, v)}
            options={[
              g.project && g.projectFrom !== 'user'
                ? (() => {
                    const project = g.projectTitle ?? g.project, from = t(`wsProject.from.${g.projectFrom ?? 'intents'}`)
                    return { value: AUTO, label: t('wsProject.auto', { project, from }), content: <Slot text={t('wsProject.auto', { project: SLOT, from })}><Content text={project} /></Slot> }
                  })()
                : { value: AUTO, label: t('wsProject.autoUnknown') },
              ...projects.map(p => ({ value: p.id, label: p.title, lang: contentLang(p.title) })),
            ]}
            className="h-7 w-full"
          />
        </label>
      ))}
      {error && <p role="alert" className="text-xs wrap-anywhere text-you-ink">{error}</p>}
    </div>
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
    <div className="flex flex-col gap-2">
      <p className="text-xs text-ink-2">{t('alerts.lead')}</p>
      {ALERTS.map(([group, metrics]) => (
        <div key={group} role="group" aria-labelledby={`alerts-${group}`} className="flex flex-col gap-2">
          <h4 id={`alerts-${group}`} className="pt-1 text-xs font-semibold text-ink-2">{t(`alerts.group.${group}`)}</h4>
          {metrics.map(row)}
        </div>
      ))}
      {error && <p role="alert" className="text-xs wrap-anywhere text-you-ink">{error}</p>}
    </div>
  )
}

/** After how many idle minutes a worker on a running task is reminded, then escalated. */
function Stall() {
  const { t } = useT()
  const [minutes, setMinutes] = useState<number | null>(null)
  const [error, setError] = useState('')
  useEffect(() => { stallMinutes().then(setMinutes, () => {}) }, [])
  if (minutes == null) return null
  const save = (v: number) => {
    setMinutes(v)
    setStallMinutes(v).then(
      m => { setMinutes(m); setError('') },
      e => { setError(t('settings.stall.failed', { error: String(e) })); stallMinutes().then(setMinutes, () => {}) },
    )
  }
  return (
    <div className="flex flex-col gap-2">
      <p className="flex flex-wrap items-center gap-1.5 text-sm text-ink-2">
        <Trans i18nKey="settings.stall.sentence" components={{ minutes: <Count value={minutes} min={1} max={240} label={t('settings.stall.minutes')} disabled={false} onCommit={save} /> }} />
      </p>
      {error && <p role="alert" className="text-xs wrap-anywhere text-you-ink">{error}</p>}
    </div>
  )
}

/** The sound for a new row in Needs you. Turning it on plays it once, so the user hears it
 *  (and the browser lets the page play it). Mounted when Settings opens, so what it says about
 *  a sound that could not play is fresh. */
function Sound() {
  const { t } = useT()
  const [on, setOn] = useState(soundWanted)
  const [missed, setMissed] = useState(() => on && soundMissed())
  return (
    <>
      <Row label={t('settings.sound')}>
        <Switch
          checked={on}
          aria-label={t('settings.sound')}
          onCheckedChange={next => { wantSound(next); setOn(next); if (next) chime().then(ok => setMissed(!ok)); else setMissed(false) }}
        />
      </Row>
      {missed && <p className="text-xs text-ink-2">{t('settings.soundMissed')}</p>}
    </>
  )
}

/** The person's own choices: language, theme, notifications, the sound. */
function General() {
  const { t, lang, setLang } = useT()
  const [theme, pickTheme] = useState<Theme>(readTheme)
  // Off until the user turns it on; the browser's permission is asked only then.
  const [notifying, setNotifying] = useState(notifyWanted)
  const [denied, setDenied] = useState(false)
  const item = 'h-7 px-3 text-xs font-normal data-[state=on]:font-semibold'
  return (
    <div className="flex flex-col gap-3">
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
      <Sound />
    </div>
  )
}

/** Settings' groups, one tab each, in this order: the person's own first. */
export const GROUPS = ['general', 'workers', 'projects', 'alerts', 'machines'] as const
export type SettingsGroup = (typeof GROUPS)[number]

/** The tab last picked, kept in this browser. */
const TAB = 'workriver.settings.tab'
const readTab = (): SettingsGroup => {
  try {
    const v = localStorage.getItem(TAB)
    return GROUPS.find(g => g === v) ?? 'general'
  } catch { return 'general' }
}
const keepTab = (g: SettingsGroup) => { try { localStorage.setItem(TAB, g) } catch { /* not remembered, still shown */ } }

/** Each Settings on the page, to be opened on a group. */
const openers = new Set<(g: SettingsGroup) => void>()
/** Opens Settings on `group`'s tab: what a link to a setting calls. */
export function openSettings(group: SettingsGroup) { openers.forEach(open => open(group)) }

/** `compact`: only the gear, for the folded rail. */
export function Settings({ compact = false, ws = null }: { compact?: boolean; ws?: Workspace | null }) {
  const { t } = useT()
  const [open, setOpen] = useState(false)
  const [tab, setTab] = useState(readTab)
  const pick = (g: SettingsGroup) => { setTab(g); keepTab(g) }
  useEffect(() => {
    const opener = (g: SettingsGroup) => { setTab(g); keepTab(g); setOpen(true) }
    openers.add(opener)
    return () => { openers.delete(opener) }
  }, [])
  // A group with nothing to set here has no tab; its remembered pick shows the first.
  const shown = GROUPS.filter(g => (g === 'projects' ? ownWorkspaces(ws).length > 0 && (ws?.projects ?? []).length > 0 : g !== 'machines' || !MOCK))
  const label: Record<SettingsGroup, string> = {
    general: t('settings.general'),
    workers: t('settings.stall.title'),
    projects: t('wsProject.title'),
    alerts: t('alerts.title'),
    machines: t('machines.title'),
  }
  return (
    <Dialog open={open} onOpenChange={setOpen}>
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
      {/* One height for every tab: the dialog does not jump when another is picked. */}
      <DialogContent closeLabel={t('settings.close')} className="flex h-[min(85vh,36rem)] flex-col sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t('settings.title')}</DialogTitle>
        </DialogHeader>
        <Tabs value={shown.includes(tab) ? tab : 'general'} onValueChange={v => pick(v as SettingsGroup)} className="min-h-0 flex-1">
          <TabsList variant="line" className="w-full flex-wrap justify-start gap-y-2 border-b border-line group-data-[orientation=horizontal]/tabs:h-auto">
            {shown.map(g => <TabsTrigger key={g} value={g} className="h-[25px] flex-none px-2">{label[g]}</TabsTrigger>)}
          </TabsList>
          {/* Each tab scrolls on its own; its content is mounted only while it shows. On a narrow
              screen the tabs wrap, each keeping its own height (shadcn sizes it to the list). */}
          <TabsContent value="general" className="min-h-0 overflow-y-auto pt-1"><General /></TabsContent>
          <TabsContent value="workers" className="min-h-0 overflow-y-auto pt-1"><Stall /></TabsContent>
          <TabsContent value="projects" className="min-h-0 overflow-y-auto pt-1"><WorkspaceProjects ws={ws} /></TabsContent>
          <TabsContent value="alerts" className="min-h-0 overflow-y-auto pt-1"><MachineAlerts /></TabsContent>
          {!MOCK && <TabsContent value="machines" className="min-h-0 overflow-y-auto pt-1"><Machines ws={ws} /></TabsContent>}
        </Tabs>
      </DialogContent>
    </Dialog>
  )
}
