// The rail picks a project and its units: product name, the project switch, search; the
// active intents with their units and signals, the roadmap of planned intents (ordered by
// dragging, added to inline), the done ones folded; settings. What asks something of the
// user and where the agents are live in the side panel on the right.
import { Waves } from 'lucide-react'
import { useEffect, useState, type DragEvent, type KeyboardEvent, type ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
import { attention, blockedAgents, repoOf, unitsOf } from '@/data'
import { useT } from '@/i18n'
import { useResize } from '@/lib/useResize'
import { record } from '@/runtime'
import type { Intent, Unit, Workspace } from '@/types'
import { ExternalChip, Icon, MachineChip, Progress, Tip } from './bits'
import { Settings } from './Settings'

/** What waits for the user in some units: gates an agent is stuck on, and everything else. */
function needs(units: Unit[]) {
  const all = units.map(attention)
  return {
    blocking: all.reduce((s, n) => s + n.gates.length, 0),
    waiting: all.reduce((s, n) => s + n.asked.reduce((t, a) => t + a.count, 0) + n.loose.length, 0),
    grew: all.reduce((s, n) => s + n.out.length, 0),
  }
}

/** The two violet marks, none drawn at zero: blocking filled with a hand, waiting outlined. */
function Marks({ blocking, waiting }: { blocking: number; waiting: number }) {
  const { t, num } = useT()
  return (
    <>
      {blocking > 0 && (
        <Tip text={t('unit.blocking', { count: blocking })}>
          <span className="tag" data-tone="block">
            <Icon name="gate-test" className="size-3" /><span aria-hidden>{num(blocking)}</span>
            <span className="sr-only">{t('unit.blocking', { count: blocking })}</span>
          </span>
        </Tip>
      )}
      {waiting > 0 && (
        <Tip text={t('unit.waiting', { count: waiting })}>
          <span className="tag" data-tone="wait">
            <span aria-hidden>{num(waiting)}</span><span className="sr-only">{t('unit.waiting', { count: waiting })}</span>
          </span>
        </Tip>
      )}
    </>
  )
}

/**
 * What a unit's line says: its two violet marks and branches that grew beyond the plan;
 * merged, its date; with nothing to say, whether it has started.
 */
function Signals({ unit }: { unit: Unit }) {
  const { t, date } = useT()
  const n = needs([unit])
  const last = unit.commits.at(-1)
  const finished = unit.progress === 'merged' || unit.progress === 'done' || unit.status === 'merged'
  if (finished) return <><Progress unit={unit} />{last && <span className="text-xs text-rail-ink-2">{date(last.date, false)}</span>}</>
  // The intent's own work has no branch or commits: nothing to say unless something waits.
  if (unit.intentLevel && !n.blocking && !n.waiting) return null
  if (!n.blocking && !n.waiting && !n.grew) return unit.progress && unit.progress !== 'working' ? <Progress unit={unit} /> : <span className="tag">{t(unit.commits.length ? 'unit.active' : 'unit.untouched')}</span>
  return (
    <>
      {unit.progress && unit.progress !== 'working' && <Progress unit={unit} />}
      <Marks {...n} />
      {n.grew > 0 && <Tip text={t('unit.grewHint')}><span className="tag" data-tone="far">{t('unit.grew', { count: n.grew })}</span></Tip>}
    </>
  )
}

interface Props {
  ws: Workspace | null
  unitId: string | null
  onUnit: (id: string) => void
  q: string
  onQ: (q: string) => void
  /** Folded, the rail is a strip (a header row on a narrow screen). */
  open: boolean
  onToggle: () => void
  /** The project shown (its folder), or null for all. */
  project: string | null
  onProject: (p: string | null) => void
  /** Show the steps to start a planned intent. */
  onStart: (i: Intent) => void
  /** The rail's width settled anew (a drag ended): what fits itself to the room refits. */
  onWidth?: (w: number) => void
}

/** How the intent's manager is: working, idle and so on, not running, or none registered. */
function ManagerState({ intent }: { intent: Intent }) {
  const { t } = useT()
  const state = intent.manager ? intent.managerState ?? 'gone' : 'none'
  const label = t(`manager.state.${state}`)
  return (
    <Tip text={intent.manager ? t('manager.of', { name: intent.manager, state: label }) : label}>
      <span className="flex flex-none items-center gap-1 font-normal" data-manager={state}>
        <Icon name="manager" className="size-3" />
        <span className="text-[11px]">{label}</span>
      </span>
    </Tip>
  )
}

/** Folded, the rail still shows that some agent is stuck on the user, anywhere. */
function Blocked({ ws }: { ws: Workspace | null }) {
  const { t } = useT()
  // Gates anywhere, and agents anywhere waiting in their terminal.
  const n = needs(unitsOf(ws)).blocking + blockedAgents(ws).length
  return n > 0 ? (
    <Tip text={t('unit.blocking', { count: n })}>
      <span role="img" aria-label={t('unit.blocking', { count: n })} className="tag" data-tone="block"><Icon name="gate-test" className="size-3" /></span>
    </Tip>
  ) : null
}

function Fold({ open, onToggle }: Pick<Props, 'open' | 'onToggle'>) {
  const { t } = useT()
  const label = t(open ? 'rail.fold' : 'rail.unfold')
  return (
    <Tip text={t('rail.shortcut', { label })}>
      <Button variant="ghost" size="icon-sm" aria-label={label} aria-expanded={open} className="rounded-sm text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink" onClick={onToggle}>
        <Icon name={open ? 'rail' : 'rail-open'} />
      </Button>
    </Tip>
  )
}

const stateOf = (i: Intent) => i.state ?? 'active'

/** One intent with its units: the header folds it; open, its goal and its units. */
function IntentGroup({ ws, intent, units, isOpen, onOpen, unitId, onUnit }: {
  ws: Workspace | null; intent: Intent; units: Unit[]; isOpen: boolean; onOpen: (o: boolean) => void; unitId: string | null; onUnit: (id: string) => void
}) {
  const { t } = useT()
  const sum = needs(intent.units)
  // Its agents waiting in their terminal count as blocking too.
  sum.blocking += blockedAgents(ws).filter(s => s.intent === intent.id).length
  return (
    <Collapsible open={isOpen} onOpenChange={onOpen}>
      <CollapsibleTrigger className="group/intent flex w-full items-center gap-1.5 rounded-md px-1.5 py-1.5 text-left text-xs font-semibold text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink">
        <Icon name="chev" className="size-3.5 transition-transform group-data-[state=open]/intent:rotate-90" />
        <span className="min-w-0 flex-1 truncate" title={intent.title}>{intent.title}</span>
        {stateOf(intent) === 'active' && <ManagerState intent={intent} />}
        {intent.complete && <Tip text={t('unit.complete')}><span className="tag font-normal" data-progress="ready">{t('unit.finished')}</span></Tip>}
        {/* Folded, the group still says what waits inside it. */}
        {!isOpen && <span className="flex flex-none items-center gap-1"><Marks blocking={sum.blocking} waiting={sum.waiting} /></span>}
      </CollapsibleTrigger>
      <CollapsibleContent className="flex flex-col gap-0.5 pl-3">
        {/* The intent's goal and where it runs, quietly, for whoever opened it. */}
        {(intent.goal || intent.workspace || intent.external) && (
          <p className="flex flex-wrap items-center gap-x-1.5 px-2.5 pb-1 text-xs text-rail-ink-2" title={intent.goal}>
            {intent.goal && <span className="line-clamp-2">{intent.goal}</span>}
            {intent.workspace && stateOf(intent) === 'active' && <span className="font-mono text-[11px]">{intent.workspace.label}</span>}
            <ExternalChip ext={intent.external} />
          </p>
        )}
        {/* Who decides its findings: the user's to say, and only the user's (this page is the user's). */}
        {stateOf(intent) === 'active' && intent.findings && (
          <label className="flex items-center gap-1.5 px-2.5 pb-1 text-xs text-rail-ink-2">
            {t('rail.findings')}
            <select
              className="min-w-0 flex-1 rounded-sm border border-rail-line bg-rail-2 px-1 py-0.5 text-rail-ink"
              value={intent.findings}
              onChange={e => { record('intent.updated', { intent: intent.id, findings: e.target.value }).catch(err => console.warn(err)) }}
            >
              <option value="manager">{t('rail.findingsBy.manager')}</option>
              <option value="user">{t('rail.findingsBy.user')}</option>
            </select>
          </label>
        )}
        {!intent.units.length && <p className="px-2.5 pb-1 text-xs text-rail-ink-2 italic">{t('rail.noUnits')}</p>}
        {units.map(unit => (
          <button
            key={unit.id}
            type="button"
            aria-pressed={unit.id === unitId}
            onClick={() => onUnit(unit.id)}
            className="block w-full rounded-md px-2.5 py-2 text-left transition-colors hover:bg-rail-2 aria-pressed:bg-rail-3"
          >
            <span className="block text-sm leading-[1.4] font-[550] wrap-anywhere">{unit.intentLevel ? t('rail.intentWork') : unit.title}</span>
            {unit.machine && <span className="mt-0.5 flex"><MachineChip machine={unit.machine} /></span>}
            <span className="mt-1 flex flex-wrap items-center gap-1 empty:hidden"><Signals unit={unit} /></span>
          </button>
        ))}
      </CollapsibleContent>
    </Collapsible>
  )
}

/** A quiet section heading in the rail, with an optional action. */
function Heading({ children, extra }: { children: string; extra?: ReactNode }) {
  return (
    <h2 className="flex items-center gap-1.5 px-1.5 pt-2 pb-0.5 text-[11px] font-semibold tracking-wide text-rail-ink-2 uppercase">
      <span className="flex-1">{children}</span>
      {extra}
    </h2>
  )
}

/**
 * The planned intents in order. Dragging a row (or Alt+↑/↓ on its grip) moves it within its
 * project's roadmap; "+" adds one at the end, with a title and a one-sentence goal.
 */
function Roadmap({ list, all, addTo, onStart, onError }: { list: Intent[]; all: Intent[]; addTo: string | null; onStart: (i: Intent) => void; onError: (e: string) => void }) {
  const { t } = useT()
  const [drag, setDrag] = useState<string | null>(null)
  const [over, setOver] = useState<{ id: string; where: 'before' | 'after' } | null>(null)
  const [adding, setAdding] = useState(false)
  const [title, setTitle] = useState('')
  const [goal, setGoal] = useState('')
  const fail = (e: unknown) => onError(t('rail.failed', { error: String(e) }))
  const move = (id: string, to: string, where: 'before' | 'after') => { if (id !== to) record('intent.moved', { intent: id, [where]: to }).catch(fail) }
  const projectOf = (id: string | null) => all.find(i => i.id === id)?.project
  const onOver = (e: DragEvent, i: Intent) => {
    if (!drag || drag === i.id || projectOf(drag) !== i.project) return
    e.preventDefault()
    const r = e.currentTarget.getBoundingClientRect()
    setOver({ id: i.id, where: e.clientY < r.top + r.height / 2 ? 'before' : 'after' })
  }
  const end = () => { setDrag(null); setOver(null) }
  // The keyboard way: Alt+↑/↓ swaps with the neighbour in the same project.
  const onKey = (e: KeyboardEvent, i: Intent) => {
    if (!e.altKey || (e.key !== 'ArrowUp' && e.key !== 'ArrowDown')) return
    e.preventDefault()
    const mine = list.filter(x => x.project === i.project)
    const k = mine.indexOf(i), next = mine[e.key === 'ArrowUp' ? k - 1 : k + 1]
    if (next) move(i.id, next.id, e.key === 'ArrowUp' ? 'before' : 'after')
  }
  const save = () => {
    if (!addTo || !title.trim() || !goal.trim()) return
    record('intent.created', { title: title.trim(), goal: goal.trim(), project: addTo, state: 'planned' })
      .then(() => { setTitle(''); setGoal(''); setAdding(false) }, fail)
  }
  const add = (
    <Tip text={addTo ? t('rail.add') : t('rail.addPick')}>
      <Button variant="ghost" size="icon-sm" aria-label={t('rail.add')} disabled={!addTo} className="size-5 rounded-sm text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink" onClick={() => setAdding(a => !a)}>
        <Icon name="plus" className="size-3.5" />
      </Button>
    </Tip>
  )
  return (
    <section aria-label={t('rail.roadmap')} className="flex flex-col gap-0.5">
      <Heading extra={add}>{t('rail.roadmap')}</Heading>
      {list.map(i => (
        <div
          key={i.id}
          className="plan"
          data-over={over?.id === i.id ? over.where : undefined}
          onDragOver={e => onOver(e, i)}
          onDragLeave={() => setOver(o => (o?.id === i.id ? null : o))}
          onDrop={e => { e.preventDefault(); if (drag && over) move(drag, over.id, over.where); end() }}
        >
          <Tip text={t('rail.drag')}>
            <button
              type="button"
              className="grip"
              aria-label={`${t('rail.drag')}: ${i.title}`}
              draggable
              onDragStart={e => { setDrag(i.id); e.dataTransfer.effectAllowed = 'move'; e.dataTransfer.setData('text/plain', i.id) }}
              onDragEnd={end}
              onKeyDown={e => onKey(e, i)}
            >
              <Icon name="grip" className="size-3.5" />
            </button>
          </Tip>
          <span className="min-w-0 flex-1">
            <span className="block text-sm leading-[1.4] font-[550] wrap-anywhere">{i.title}</span>
            {i.goal && <span className="line-clamp-2 text-xs text-rail-ink-2" title={i.goal}>{i.goal}</span>}
            {i.external && <span className="mt-0.5 flex"><ExternalChip ext={i.external} /></span>}
          </span>
          <Tip text={t('plan.title', { title: i.title })}>
            <Button variant="ghost" size="sm" className="h-6 flex-none gap-1 rounded-sm px-1.5 text-xs text-rail-ink-2 hover:bg-rail-3 hover:text-rail-ink" onClick={() => onStart(i)}>
              <Icon name="play" className="size-3" />{t('rail.start')}
            </Button>
          </Tip>
        </div>
      ))}
      {adding && (
        <form className="flex flex-col gap-1.5 px-1.5 py-1" onSubmit={e => { e.preventDefault(); save() }}>
          <input className="rail-field" autoFocus value={title} onChange={e => setTitle(e.target.value)} placeholder={t('rail.addTitle')} aria-label={t('rail.addTitle')} />
          <input className="rail-field" value={goal} onChange={e => setGoal(e.target.value)} placeholder={t('rail.addGoal')} aria-label={t('rail.addGoal')} />
          <div className="flex gap-1.5">
            <Button type="submit" size="sm" variant="outline" className="h-7 rounded-sm bg-transparent text-rail-ink" disabled={!title.trim() || !goal.trim()}>{t('rail.addSave')}</Button>
            <Button type="button" size="sm" variant="ghost" className="h-7 rounded-sm text-rail-ink-2" onClick={() => setAdding(false)}>{t('rail.addCancel')}</Button>
          </div>
        </form>
      )}
    </section>
  )
}

export function Rail({ ws, unitId, onUnit, q, onQ, open: railOpen, onToggle, project, onProject, onStart, onWidth }: Props) {
  const { t } = useT()
  // The right edge drags the width, between narrow and roomy; double-click: the default.
  const size = useResize({ key: 'workriver.rail.width', variable: '--rail-w', def: 248, min: 200, max: 480, edge: 'right' })
  useEffect(() => onWidth?.(size.width), [onWidth, size.width])
  const projects = ws?.projects ?? []
  const intents = (ws?.intents ?? []).filter(i => !project || i.project === project)
  const needle = q.trim().toLowerCase()
  const match = (u: Unit) => !needle || (u.title + ' ' + u.purpose).toLowerCase().includes(needle)
  // An intent with no units yet still shows (it has a workspace and maybe a manager), but not in a search.
  const groupsOf = (list: Intent[]) => list.map(intent => ({ intent, units: intent.units.filter(match) })).filter(g => g.units.length || (!needle && !g.intent.units.length))
  const active = groupsOf(intents.filter(i => stateOf(i) === 'active'))
  const done = groupsOf(intents.filter(i => stateOf(i) === 'done'))
  const planned = intents
    .filter(i => stateOf(i) === 'planned' && (!needle || (i.title + ' ' + (i.goal ?? '')).toLowerCase().includes(needle)))
    .sort((a, b) => (a.project ?? '').localeCompare(b.project ?? '') || (a.order ?? 0) - (b.order ?? 0))
  // New intents go to the project shown, or to the only one there is.
  const addTo = project ?? (projects.length === 1 ? projects[0].id : null)
  const [error, setError] = useState('')
  // The intent of the open unit is open; the others are folded unless the user opened them.
  const home = intents.find(i => i.units.some(u => u.id === unitId))?.id
  // The repository of the open unit; units of one intent may work in several.
  const openUnit = unitsOf(ws).find(u => u.id === unitId)
  const repo = ws && openUnit && !openUnit.intentLevel ? repoOf(ws, openUnit).name : ''
  const [open, setOpen] = useState<string[]>([])
  useEffect(() => { if (home) setOpen(o => (o.includes(home) ? o : [...o, home])) }, [home])
  useEffect(() => {
    if (!error) return
    const k = setTimeout(() => setError(''), 6000)
    return () => clearTimeout(k)
  }, [error])
  if (!railOpen)
    return (
      <aside className="rail sticky top-0 flex h-screen w-11 flex-col items-center gap-2 bg-rail pt-4 pb-3 text-rail-ink max-[860px]:static max-[860px]:h-auto max-[860px]:w-auto max-[860px]:flex-row max-[860px]:px-3 max-[860px]:py-2">
        <Waves aria-hidden size={20} strokeWidth={1.6} className="flex-none" />
        <span className="text-lg font-[650] tracking-[-0.01em] min-[861px]:hidden">workriver</span>
        <Fold open={false} onToggle={onToggle} />
        <Blocked ws={ws} />
        <div className="mt-auto max-[860px]:hidden"><Settings compact ws={ws} /></div>
      </aside>
    )
  const group = ({ intent, units }: { intent: Intent; units: Unit[] }) => (
    <IntentGroup
      key={intent.id}
      ws={ws}
      intent={intent}
      units={units}
      // A search opens every group with a match.
      isOpen={!!needle || open.includes(intent.id) || !intent.units.length}
      onOpen={o => setOpen(list => (o ? [...list, intent.id] : list.filter(x => x !== intent.id)))}
      unitId={unitId}
      onUnit={onUnit}
    />
  )
  return (
    <aside className="rail sticky top-0 flex h-screen w-[var(--rail-w,248px)] flex-col gap-3 bg-rail px-3 pt-4 pb-3 text-rail-ink max-[860px]:static max-[860px]:h-auto max-[860px]:w-auto">
      {/* Not on a narrow screen, where the rail is a row of its own. */}
      <div {...size.handle} aria-label={t('rail.resize')} className="edge max-[860px]:hidden" data-edge="right" />
      <div className="flex items-center gap-2 pl-1 text-lg font-[650] tracking-[-0.01em]">
        <Waves aria-hidden size={20} strokeWidth={1.6} className="flex-none" />
        workriver
        <span className="min-w-0 flex-1 truncate font-normal text-rail-ink-2">{repo}</span>
        <Fold open onToggle={onToggle} />
      </div>

      {projects.length > 0 && (
        <select className="rail-field" aria-label={t('rail.project')} value={project ?? ''} onChange={e => onProject(e.target.value || null)}>
          <option value="">{t('rail.allProjects')}</option>
          {projects.map(p => <option key={p.id} value={p.id}>{p.title}</option>)}
        </select>
      )}

      <Input
        type="search"
        value={q}
        onChange={e => onQ(e.target.value)}
        placeholder={t('rail.search')}
        aria-label={t('rail.search')}
        className="rounded-md bg-rail-2 text-sm md:text-sm"
      />

      {/* The viewport's inner box is a block, not Radix's table: titles then wrap or cut at the rail's width. */}
      <ScrollArea className="-mx-1 min-h-0 flex-1 max-[860px]:[&>[data-slot=scroll-area-viewport]]:max-h-[40vh] [&>[data-slot=scroll-area-viewport]>div]:block!">
      <div className="flex flex-col gap-1 px-1 py-0.5">
        {active.length > 0 && <section aria-label={t('rail.active')} className="flex flex-col gap-1"><Heading>{t('rail.active')}</Heading>{active.map(group)}</section>}
        {ws && (planned.length > 0 || addTo) && <Roadmap list={planned} all={intents} addTo={addTo} onStart={onStart} onError={setError} />}
        {error && <p role="alert" className="px-2.5 text-xs text-rail-you">{error}</p>}
        {done.length > 0 && (
          <Collapsible>
            <CollapsibleTrigger className="group/done flex w-full items-center gap-1.5 rounded-md px-1.5 pt-2 pb-0.5 text-[11px] font-semibold tracking-wide text-rail-ink-2 uppercase hover:text-rail-ink">
              <Icon name="chev" className="size-3.5 transition-transform group-data-[state=open]/done:rotate-90" />
              {t('rail.done')}<span className="font-normal">{done.length}</span>
            </CollapsibleTrigger>
            <CollapsibleContent className="flex flex-col gap-1">{done.map(group)}</CollapsibleContent>
          </Collapsible>
        )}
        {ws && !active.length && !done.length && !planned.length && (
          <div className="flex flex-col items-start gap-2 px-2.5 py-2 text-sm text-rail-ink-2">
            {needle && unitsOf(ws).length ? t('rail.noMatch', { q: q.trim() }) : t('rail.none')}
            {needle && unitsOf(ws).length > 0 && <Button variant="outline" size="sm" className="rounded-sm bg-transparent text-rail-ink" onClick={() => onQ('')}>{t('rail.clearSearch')}</Button>}
          </div>
        )}
      </div>
      </ScrollArea>

      <div className="flex flex-col border-t border-rail-line pt-2">
        <Settings ws={ws} />
      </div>
    </aside>
  )
}
