// The rail picks a project and its units: product name, filters for intents and units, the
// project switch, search; then project → workspace → intent → unit: each project's open
// workspaces (name, id, its manager's state, a manager's start) with their active intents and units, its roadmap of
// planned intents (ordered by dragging, added to inline), its done ones folded; "Add project"; settings. What asks something of the
// user and where the agents are live in the side panel on the right.
import { Waves } from 'lucide-react'
import { useEffect, useRef, useState, type DragEvent, type KeyboardEvent, type ReactNode } from 'react'
import { Button } from '@/ui/primitives/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/ui/primitives/collapsible'
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogTitle } from '@/ui/primitives/dialog'
import { Popover, PopoverContent, PopoverTrigger } from '@/ui/primitives/popover'
import { Input } from '@/ui/primitives/input'
import { ScrollArea } from '@/ui/primitives/scroll-area'
import { attention, blockedAgents } from '@/attention/select'
import { intentQuietSince, noCode, quietSince, unitsOf } from './select'
import { repoOf } from '@/code/select'
import { contentLang, useT } from '@/i18n'
import { linkTo, pathOf } from '@/app/link'
import { recall, remember } from '@/lib/storage'
import { useResize } from '@/lib/useResize'
import { DASHBOARD_CLICKS, RAIL_WIDTH } from '@/config/defaults'
import { record, setSpace } from '@/api/actions'
import { UNIT_PROGRESS, type Intent, type Unit, type UnitProgress } from './types'
import type { Space } from '@/agents/types'
import { activeIn, railOf, runs, type RailProject } from './spaces'
import { openSeat } from './Seat'
import { StartLine } from './StartManager'
import { AddProjectButton } from './AddProject'
import type { Workspace } from '@/workspace/types'
import { MachineChip } from '@/machines/format'
import { Content, ExternalChip, Slot, SLOT } from '@/content/Chips'
import { CopyButton } from '@/ui/CopyButton'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { Choice } from '@/ui/Choice'
import { MergeDialog } from './Merge'
import { KindSelect, RailFilter, SortChip, unitStateLabel } from './Sort'
import { filtersOn, keeps, loadFilter, NO_FILTER, saveFilter, tagsOf, type Filter, type Kind } from './classify'

/** All projects, in the project switch (Radix takes no empty value). */
const ALL = '*'
const UNIT_STATES = 'workriver.rail.unitStates'
import { Settings } from '@/machines/Settings'
import { TemplatesButton } from './Templates'

/** What waits for the user in some units: gates an agent is stuck on, and everything else. */
export function needs(units: Unit[]) {
  const all = units.map(attention)
  return {
    blocking: all.reduce((s, n) => s + n.gates.length, 0),
    waiting: all.reduce((s, n) => s + n.asked.reduce((t, a) => t + a.count, 0) + n.loose.length, 0),
  }
}

/** The two violet marks, none drawn at zero: blocking filled with a hand, waiting outlined. */
export function Marks({ blocking, waiting }: { blocking: number; waiting: number }) {
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

/** Nothing has moved for a while: how long, quietly; the date on hover. */
export function Quiet({ since }: { since: number | null }) {
  const { t, waited, when } = useT()
  if (since === null) return null
  const text = t('rail.quiet', { time: waited(since) })
  return (
    <Tip text={`${text} · ${t('rail.quietSince', { date: when(since) })}`}>
      <span role="img" aria-label={text} className="flex flex-none items-center gap-0.5 text-[11px] font-normal text-rail-ink-2 opacity-80">
        <Icon name="idle" className="size-3" /><span aria-hidden>{text}</span>
      </span>
    </Tip>
  )
}

/**
 * What a unit's line says: its two violet marks;
 * merged, its date; quiet for a while, since when; with nothing to say, whether it has started.
 */
function Signals({ unit, gitPending }: { unit: Unit; gitPending?: boolean }) {
  const { t, date } = useT()
  const n = needs([unit])
  const since = gitPending ? null : quietSince(unit)
  const quiet = <Quiet since={since} />
  const last = unit.commits.at(-1)
  const finished = unit.progress === 'merged' || unit.progress === 'done' || unit.status === 'merged'
  // Merged: when it was recorded so, else its last commit.
  const on = unit.merged ?? last?.date
  if (finished) return <><Progress unit={unit} />{on && <span className="text-xs text-rail-ink-2">{date(on, false)}</span>}</>
  if (!n.blocking && !n.waiting) {
    if (unit.progress && unit.progress !== 'working') return <><Progress unit={unit} />{quiet}</>
    if (gitPending || since !== null) return quiet
    return <span className="tag">{t((noCode(unit) ? unit.status === 'active' : unit.commits.length) ? 'unit.active' : 'unit.untouched')}</span>
  }
  return (
    <>
      {unit.progress && unit.progress !== 'working' && <Progress unit={unit} />}
      <Marks {...n} />
      {quiet}
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
  /** Open an intent's page. */
  onIntent: (id: string) => void
  /** The rail's width settled anew (a drag ended): what fits itself to the room refits. */
  onWidth?: (w: number) => void
  onDashboard: () => void
}

/** How the intent's manager is: working, idle and so on, not running, or none registered (amber: its findings reach no one). */
export function ManagerState({ intent }: { intent: Intent }) {
  const { t } = useT()
  const state = intent.manager ? intent.managerState ?? 'gone' : 'none'
  const label = t(`manager.state.${state}`)
  return (
    <Tip text={intent.manager ? t('manager.of', { name: intent.manager, state: label }) : label}>
      <span className="flex flex-none items-center gap-1 font-normal data-[manager=none]:text-amber" data-manager={state}>
        <Icon name="manager" className="size-3" />
        <span className="text-[11px]">{label}</span>
      </span>
    </Tip>
  )
}

/** The first sentence of a goal, for a tooltip. */
export const firstSentence = (s?: string) => (s ?? '').trim().split(/(?<=[.!?。！？])\s*/)[0] ?? ''

/** What a rail row's tooltip says: the title, the goal's first sentence, the tags. */
function RowTip({ intent }: { intent: Intent }) {
  const { t } = useT()
  const goal = firstSentence(intent.goal)
  return (
    <>
      <span className="block font-semibold" lang={contentLang(intent.title)}>{intent.title}</span>
      {goal && <span className="block" lang={contentLang(goal)}>{goal}</span>}
      {!!intent.tags?.length && <span className="block">{t('rail.tags', { tags: intent.tags.join(', ') })}</span>}
    </>
  )
}

/** An intent's title line in the rail: its id quietly in mono, the title cut to one line; the goal's first sentence and the tags on hover. */
function IntentTitle({ intent }: { intent: Intent }) {
  return (
    <Tip text={<RowTip intent={intent} />}>
      <span className="min-w-0 flex-1 truncate">
        <span className="mr-1.5 font-mono text-[11px] font-normal text-rail-ink-2">{intent.id}</span><span lang={contentLang(intent.title)}>{intent.title}</span>
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
type GroupFold = { isOpen: boolean; onOpen: (open: boolean) => void }

/**
 * One intent with its units, on two lines: its id and title (a click opens its page), then
 * quietly where it runs, its manager, quiet or finished. The chevron folds its units.
 */
function IntentGroup({ ws, intent, units, isOpen, onOpen, unitId, onUnit, onIntent }: {
  ws: Workspace | null; intent: Intent; units: Unit[]; unitId: string | null; onUnit: (id: string) => void; onIntent: (id: string) => void
} & GroupFold) {
  const { t } = useT()
  const sum = needs(intent.units)
  // Its agents waiting in their terminal count as blocking too.
  sum.blocking += blockedAgents(ws).filter(s => s.intent === intent.id).length
  const active = stateOf(intent) === 'active'
  const ran = ws?.workspaces?.find(w => w.id === intent.workspace)
  return (
    <Collapsible open={isOpen} onOpenChange={onOpen}>
      <div className="group/row flex items-start gap-0.5">
        <CollapsibleTrigger aria-label={t('rail.foldGroup', { title: intent.title })} className="group/intent mt-1 flex-none rounded-sm p-0.5 text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink">
          <Icon name="chev" className="size-3.5 transition-transform group-data-[state=open]/intent:rotate-90" />
        </CollapsibleTrigger>
        <button
          type="button"
          aria-pressed={unitId === intent.id}
          onClick={() => onIntent(intent.id)}
          className="flex min-w-0 flex-1 flex-col gap-0.5 rounded-md px-1.5 py-1 text-left text-xs font-semibold text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink aria-pressed:bg-rail-3 aria-pressed:text-rail-ink"
        >
          <span className="flex w-full min-w-0 items-center gap-1.5">
            <IntentTitle intent={intent} />
            {/* Folded, the group still says what waits inside it. */}
            {!isOpen && <span className="flex flex-none items-center gap-1"><Marks blocking={sum.blocking} waiting={sum.waiting} /></span>}
          </span>
          {/* One quiet line: its kind and size, quiet, finished; a done one, the workspace it ran in. (Its manager is on its workspace's line.) */}
          <span className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-[11px] font-normal empty:hidden">
            <SortChip intent={intent} />
            {!active && ran && <Tip text={t('rail.ranIn', { workspace: `${ran.name} ${ran.id}` })}><span className="max-w-full truncate"><span lang={contentLang(ran.name)}>{ran.name}</span> <span className="font-mono">{ran.id}</span></span></Tip>}
            {(!ws?.gitPending || intent.units.every(noCode)) && <Quiet since={intentQuietSince(intent)} />}
            {intent.complete && <Tip text={t('unit.complete')}><span className="tag font-normal" data-progress="ready">{t('unit.finished')}</span></Tip>}
          </span>
        </button>
        <CopyButton text={linkTo(pathOf(intent.id, null))} aria={t('link.copy')} className="mt-0.5 rounded-sm text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink pointer-fine:opacity-0 pointer-fine:group-hover/row:opacity-100 focus-visible:opacity-100" />
      </div>
      <CollapsibleContent className="flex flex-col gap-0.5 pl-3">
        {!units.length && <p className="px-2.5 pb-1 text-xs text-rail-ink-2 italic">{t('rail.noUnits')}</p>}
        {units.map(unit => (
          <button
            key={unit.id}
            type="button"
            aria-pressed={unit.id === unitId}
            onClick={() => onUnit(unit.id)}
            className="block w-full rounded-md px-2.5 py-2 text-left transition-colors hover:bg-rail-2 aria-pressed:bg-rail-3"
          >
            <span className="block text-sm leading-[1.4] font-[550] wrap-anywhere"><span className="mr-1.5 font-mono text-xs font-normal text-rail-ink-2">{unit.id}</span><span lang={contentLang(unit.title)}>{unit.title}</span></span>
            {unit.machine && <span className="mt-0.5 flex"><MachineChip machine={unit.machine} away={ws?.machines?.some(m => m.label === unit.machine && !m.reachable)} since={ws?.machines?.find(m => m.label === unit.machine && !m.tunnel)?.downSince} /></span>}
            <span className="mt-1 flex flex-wrap items-center gap-1 empty:hidden">{unit.kind === 'spike' && <span className="tag font-normal">{t('unit.spike')}</span>}<Signals unit={unit} gitPending={ws?.gitPending && !noCode(unit)} /></span>
          </button>
        ))}
      </CollapsibleContent>
    </Collapsible>
  )
}

/** How a workspace's manager is, on its line: the crown, its name, then working, idle and so
 *  on or not running; "no manager" (amber while the workspace has an active intent: its
 *  findings reach no one). */
export function SeatState({ space, busy }: { space: Space; busy: boolean }) {
  const { t } = useT()
  const m = space.manager
  const state = m ? m.state : 'none'
  const label = t(`manager.state.${state}`)
  return (
    <Tip text={m ? t('manager.of', { name: m.name, state: label }) : label}>
      <span className="flex min-w-0 items-center gap-1 font-normal data-[warn]:text-amber" data-manager={state} data-warn={(busy && !runs(m)) || undefined}>
        <Icon name="manager" className="size-3 flex-none" />
        {m && <span className="min-w-0 truncate" lang={contentLang(m.name)}>{m.name}</span>}
        <span className="flex-none">{label}</span>
      </span>
    </Tip>
  )
}

/** Renaming a workspace: a name, shown only (it may repeat). */
function RenameSpace({ space, onClose }: { space: Space | null; onClose: () => void }) {
  const { t } = useT()
  const [name, setName] = useState('')
  const [error, setError] = useState('')
  useEffect(() => { setName(space?.name ?? ''); setError('') }, [space])
  const save = () => { if (space && name.trim()) setSpace({ action: 'rename', workspace: space.id, name: name.trim() }).then(onClose, e => setError(t('rail.failed', { error: e instanceof Error ? e.message : String(e) }))) }
  return (
    <Dialog open={!!space} onOpenChange={o => { if (!o) onClose() }}>
      <DialogContent closeLabel={t('settings.close')}>
        <DialogTitle>{t('rail.spaceRename')}<span className="ml-2 font-mono text-xs font-normal text-ink-2">{space?.id}</span></DialogTitle>
        <DialogDescription>{t('rail.spaceNameHint')}</DialogDescription>
        <form className="flex flex-col gap-2" onSubmit={e => { e.preventDefault(); save() }}>
          <Input autoFocus value={name} onChange={e => setName(e.target.value)} aria-label={t('rail.spaceName')} className="h-8 text-sm md:text-sm" />
          {error && <p role="alert" className="text-xs wrap-anywhere text-del">{error}</p>}
          <div className="flex justify-end gap-2">
            <DialogClose asChild><Button type="button" variant="ghost" size="sm">{t('rail.addCancel')}</Button></DialogClose>
            <Button type="submit" size="sm" disabled={!name.trim() || name.trim() === space?.name}>{t('rail.spaceSave')}</Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}

/**
 * One workspace of a project: its name with its id quietly in mono, one status line with
 * its manager, a menu (rename, manager, close); under it, while a start of a manager is
 * pending or after it failed, what came of it, and with no manager running "Start manager";
 * then its active intents with their units. The only level of the rail whose group folds
 * its intents.
 */
function SpaceGroup({ ws, space, busy, isOpen, onOpen, onRename, onError, children }: { ws: Workspace | null; space: Space; busy: boolean; onRename: () => void; onError: (e: string) => void; children: ReactNode } & GroupFold) {
  const { t } = useT()
  const pending = !!space.start && !space.start.reason
  const [menu, setMenu] = useState(false)
  const item = 'h-7 justify-start rounded-sm px-2 text-[13px] font-normal text-rail-ink hover:bg-rail-3 hover:text-rail-ink'
  const close = () => { setMenu(false); setSpace({ action: 'close', workspace: space.id }).catch(e => onError(t('rail.failed', { error: e instanceof Error ? e.message : String(e) }))) }
  return (
    <Collapsible open={isOpen} onOpenChange={onOpen} className="rail-space">
      <div className="flex items-start gap-0.5">
        <CollapsibleTrigger aria-label={t('rail.foldSpace', { name: space.name, id: space.id })} className="group/space flex min-w-0 flex-1 items-start gap-0.5 rounded-md py-1 pr-1 text-left text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink">
          <Icon name="chev" className="mt-0.5 size-3.5 flex-none transition-transform group-data-[state=open]/space:rotate-90" />
          <span className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="flex min-w-0 items-baseline gap-1.5 text-xs font-semibold text-rail-ink">
              <span className="min-w-0 truncate" lang={contentLang(space.name)}>{space.name}</span>
              <span className="flex-none font-mono text-[11px] font-normal text-rail-ink-2">{space.id}</span>
            </span>
            <span className="flex min-w-0 text-[11px]"><SeatState space={space} busy={busy} /></span>
          </span>
        </CollapsibleTrigger>
        <Popover open={menu} onOpenChange={setMenu}>
          <PopoverTrigger asChild>
            <Button variant="ghost" size="icon-sm" aria-label={t('rail.spaceMenu', { name: space.name, id: space.id })} className="mt-0.5 size-6 flex-none rounded-sm text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink">
              <Icon name="more" className="size-3.5" />
            </Button>
          </PopoverTrigger>
          <PopoverContent align="end" className="w-44 gap-0.5 rounded-md border border-rail-line bg-rail-2 p-1 text-rail-ink ring-0">
            <Button variant="ghost" size="sm" className={item} onClick={() => { setMenu(false); onRename() }}>{t('rail.spaceRename')}</Button>
            <Button variant="ghost" size="sm" className={item} onClick={() => { setMenu(false); openSeat(space.id) }}>{t('rail.spaceManager')}</Button>
            <Button variant="ghost" size="sm" className={item} onClick={close}>{t('rail.spaceClose')}</Button>
          </PopoverContent>
        </Popover>
      </div>
      {(space.start || !runs(space.manager)) && (
        <div className="flex flex-col items-start gap-1 pb-1 pl-4">
          <StartLine ws={ws} space={space} tone="rail" />
          {!runs(space.manager) && !pending && (
            <Button variant="ghost" size="xs" className="h-6 gap-1 rounded-sm px-1.5 text-[11px] font-normal text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink" onClick={() => openSeat(space.id, undefined, true)}>
              <Icon name="play" className="size-3" />{t('launch.startNow')}
            </Button>
          )}
        </div>
      )}
      <CollapsibleContent className="flex flex-col gap-0.5 pl-2">{children}</CollapsibleContent>
    </Collapsible>
  )
}

/** A project's "+": a new workspace in it, by its name. */
function AddSpace({ project, onError }: { project: string; onError: (e: string) => void }) {
  const { t } = useT()
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const save = () => {
    if (!name.trim()) return
    setSpace({ action: 'add', project, name: name.trim() }).then(() => { setName(''); setOpen(false) }, e => onError(t('rail.failed', { error: e instanceof Error ? e.message : String(e) })))
  }
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <Tip text={t('rail.spaceAdd')}>
        <PopoverTrigger asChild>
          <Button variant="ghost" size="icon-sm" aria-label={t('rail.spaceAdd')} className="size-5 rounded-sm text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink"><Icon name="plus" className="size-3.5" /></Button>
        </PopoverTrigger>
      </Tip>
      <PopoverContent align="end" className="w-60 rounded-md border border-rail-line bg-rail-2 p-2 text-rail-ink ring-0">
        <form className="flex flex-col gap-1.5" onSubmit={e => { e.preventDefault(); save() }}>
          <input className="rail-field" autoFocus value={name} onChange={e => setName(e.target.value)} placeholder={t('rail.spaceName')} aria-label={t('rail.spaceName')} />
          <p className="text-[11px] text-rail-ink-2">{t('rail.spaceNameHint')}</p>
          <Button type="submit" size="sm" variant="outline" className="h-7 self-start rounded-sm bg-transparent text-rail-ink" disabled={!name.trim()}>{t('rail.spaceAdd')}</Button>
        </form>
      </PopoverContent>
    </Popover>
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
 * "Merge…" opens a dialog that folds intents of one project into one (a roadmap split too finely).
 */
function Roadmap({ ws, list, all, addTo, onStart, onIntent, here, onError }: { ws: Workspace; list: Intent[]; all: Intent[]; addTo: string | null; onStart: (i: Intent) => void; onIntent: (id: string) => void; here: string | null; onError: (e: string) => void }) {
  // (`addTo` null: the roadmap of "No project", which takes no new intent and merges nothing.)
  const { t } = useT()
  const [drag, setDrag] = useState<string | null>(null)
  const [over, setOver] = useState<{ id: string; where: 'before' | 'after' } | null>(null)
  const [adding, setAdding] = useState(false)
  const [title, setTitle] = useState('')
  const [goal, setGoal] = useState('')
  const [kind, setKind] = useState<Kind | null>(null)
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
    record('intent.created', { title: title.trim(), goal: goal.trim(), project: addTo, state: 'planned', ...(kind ? { kind } : {}) })
      .then(() => { setTitle(''); setGoal(''); setKind(null); setAdding(false) }, fail)
  }
  const add = addTo && (
    <Tip text={t('rail.add')}>
      <Button variant="ghost" size="icon-sm" aria-label={t('rail.add')} className="size-5 rounded-sm text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink" onClick={() => setAdding(a => !a)}>
        <Icon name="plus" className="size-3.5" />
      </Button>
    </Tip>
  )
  return (
    <section aria-label={t('rail.roadmap')} className="flex flex-col gap-0.5">
      <Heading extra={addTo && <><MergeDialog ws={ws} project={addTo} />{add}</>}>{t('rail.roadmap')}</Heading>
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
          {/* Its id and title (its page on a click); where else it is tracked, under it. */}
          <button type="button" aria-pressed={here === i.id} onClick={() => onIntent(i.id)} className="flex min-w-0 flex-1 flex-col items-start rounded-sm text-left hover:text-rail-ink aria-pressed:text-rail-ink">
            <Tip text={<RowTip intent={i} />}>
              <span className="block text-sm leading-[1.4] font-[550] wrap-anywhere"><span className="mr-1.5 font-mono text-xs font-normal text-rail-ink-2">{i.id}</span><span lang={contentLang(i.title)}>{i.title}</span></span>
            </Tip>
            {(i.kind || i.size || i.external) && <span className="mt-0.5 flex flex-wrap items-center gap-1"><SortChip intent={i} />{i.external && <ExternalChip ext={i.external} />}</span>}
          </button>
          <Tip text={<span><Slot text={t('plan.title', { title: SLOT })}><Content text={i.title} /></Slot></span>}>
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
          <KindSelect tone="rail" value={kind} onChange={setKind} placeholder={t('rail.addKind')} className="h-7" />
          <div className="flex gap-1.5">
            <Button type="submit" size="sm" variant="outline" className="h-7 rounded-sm bg-transparent text-rail-ink" disabled={!title.trim() || !goal.trim()}>{t('rail.addSave')}</Button>
            <Button type="button" size="sm" variant="ghost" className="h-7 rounded-sm text-rail-ink-2" onClick={() => setAdding(false)}>{t('rail.addCancel')}</Button>
          </div>
        </form>
      )}
    </section>
  )
}

function Done({ count, isOpen, onOpen, children }: { count: number; children: ReactNode } & GroupFold) {
  const { t } = useT()
  return (
    <Collapsible open={isOpen} onOpenChange={onOpen}>
      <CollapsibleTrigger className="group/done flex w-full items-center gap-1.5 rounded-md px-1.5 pt-2 pb-0.5 text-[11px] font-semibold tracking-wide text-rail-ink-2 uppercase hover:text-rail-ink">
        <Icon name="chev" className="size-3.5 transition-transform group-data-[state=open]/done:rotate-90" />
        {t('rail.done')}<span className="font-normal">{count}</span>
      </CollapsibleTrigger>
      <CollapsibleContent className="flex flex-col gap-1">{children}</CollapsibleContent>
    </Collapsible>
  )
}

export function Rail({ ws, unitId, onUnit, q, onQ, open: railOpen, onToggle, project, onProject, onStart, onIntent, onWidth, onDashboard }: Props) {
  const { t, lang } = useT()
  const mark = useRef<HTMLButtonElement>(null)
  const streak = useRef({ first: 0, count: 0 })
  useEffect(() => {
    const reset = (e: MouseEvent) => {
      if (mark.current && !e.composedPath().includes(mark.current)) streak.current.count = 0
    }
    document.addEventListener('click', reset, true)
    return () => document.removeEventListener('click', reset, true)
  }, [])
  const clickMark = () => {
    const now = Date.now()
    const s = streak.current
    if (!s.count || now - s.first > DASHBOARD_CLICKS.within) {
      streak.current = { first: now, count: 1 }
    } else if (++s.count === DASHBOARD_CLICKS.count) {
      s.count = 0
      onDashboard()
    }
  }
  const brand = <button ref={mark} type="button" aria-label={t('session.title')} onClick={clickMark} className="flex flex-none rounded-sm focus-visible:ring-2 focus-visible:ring-ring"><Waves aria-hidden size={20} strokeWidth={1.6} /></button>
  // The right edge drags the width, between narrow and roomy; double-click: the default.
  const size = useResize({ key: 'workriver.rail.width', variable: '--rail-w', ...RAIL_WIDTH, edge: 'right' })
  useEffect(() => onWidth?.(size.size), [onWidth, size.size])
  const projects = ws?.projects ?? []
  const inProject = (ws?.intents ?? []).filter(i => !project || i.project === project)
  // The filter by kind, size and tag, remembered per project: it narrows Active, Roadmap and
  // Done alike and never re-sorts the roadmap.
  const [filter, setFilter] = useState<Filter>(() => loadFilter(project))
  useEffect(() => setFilter(loadFilter(project)), [project])
  const filterBy = (f: Filter) => { setFilter(f); saveFilter(project, f) }
  const [unitStates, setUnitStates] = useState(() => {
    const saved = recall(UNIT_STATES)?.split(',')
    return UNIT_PROGRESS.filter(p => saved?.includes(p))
  })
  const needle = q.trim().toLowerCase()
  // Groups start open on each state selection; their folds here leave the unfiltered ones intact.
  const [filteredFolds, setFilteredFolds] = useState<string[]>([])
  const filteredGroup = (id: string): GroupFold => ({
    isOpen: !!needle || !filteredFolds.includes(id),
    onOpen: open => setFilteredFolds(folds => open ? folds.filter(x => x !== id) : [...folds, id]),
  })
  const filterUnits = (states: UnitProgress[]) => {
    setUnitStates(states)
    setFilteredFolds([])
    remember(UNIT_STATES, states.join(','))
  }
  const intentFiltersOn = filtersOn(filter) > 0
  const unitFilterOn = unitStates.length > 0
  const selectedStates = unitFilterOn && new Intl.ListFormat(lang, { style: 'narrow' }).format(
    UNIT_PROGRESS.filter(state => unitStates.includes(state)).map(state => unitStateLabel(t, state)),
  )
  const listed = inProject.filter(i => stateOf(i) !== 'dropped')
  const intents = inProject.filter(i => keeps(filter, i))
  // A search for a tag matches all the units of its intents.
  const tagged = (i: Intent) => !!needle && (i.tags ?? []).some(x => x.includes(needle))
  const match = (u: Unit) => !needle || (u.id + ' ' + u.title + ' ' + u.purpose).toLowerCase().includes(needle)
  // An intent id or tag matches every unit's text. The unit-state filter still applies,
  // keeping the open unit so it remains reachable.
  const groups = intents
    .filter(i => stateOf(i) === 'active' || stateOf(i) === 'done')
    .map(intent => {
      const named = !!needle && (intent.id.toLowerCase() === needle || tagged(intent))
      const units = intent.units.filter(u => (named || match(u))
        && (!unitFilterOn || u.id === unitId || unitStates.includes(unitProgress(u) ?? 'working')))
      return { intent, units, named }
    })
    .filter(g => g.units.length || (!unitFilterOn && (g.named || (!needle && !g.intent.units.length))))
  const planned = intents
    .filter(i => stateOf(i) === 'planned' && (!needle || [i.id, i.title, i.goal, i.note, ...(i.tags ?? [])].join(' ').toLowerCase().includes(needle)))
    .sort((a, b) => (a.order ?? 0) - (b.order ?? 0))
  // Project → workspace → intent → unit; "No project" last, when it holds something.
  const spaces = ws?.workspaces ?? []
  const sections = railOf(projects, spaces, groups, planned, project, !!needle || intentFiltersOn || unitFilterOn)
  const nothing = sections.every(s => !s.spaces.some(x => x.intents.length) && !s.loose.length && !s.planned.length && !s.done.length)
  const noUnitMatch = unitFilterOn && groups.length === 0
  const [error, setError] = useState('')
  // The intent of the open unit is open; the others are folded unless the user opened them.
  const homeIntent = intents.find(i => i.id === unitId || i.units.some(u => u.id === unitId))
  const home = homeIntent?.id
  // The repository of the open unit; units of one intent may work in several.
  const openUnit = unitsOf(ws).find(u => u.id === unitId)
  const repo = ws && openUnit && !noCode(openUnit) ? repoOf(ws, openUnit)?.name ?? '' : ''
  const [open, setOpen] = useState<string[]>([])
  useEffect(() => { if (home) setOpen(o => (o.includes(home) ? o : [...o, home])) }, [home])
  // A workspace's group is open unless the user folded it; the open unit's opens again.
  const [folded, setFolded] = useState<string[]>([])
  const homeSpace = homeIntent?.workspace
  useEffect(() => { if (homeSpace) setFolded(f => f.filter(x => x !== homeSpace)) }, [homeSpace, unitId])
  const homeProject = projects.find(p => p.id === homeIntent?.project)?.id
  const filteredParent = homeIntent && (stateOf(homeIntent) === 'done'
    ? `done:${homeProject ?? ''}`
    : spaces.find(s => s.id === homeSpace && s.project === homeProject && !s.closed)?.id)
  useEffect(() => {
    if (unitFilterOn && home) setFilteredFolds(folds => folds.filter(id => id !== home && id !== filteredParent))
  }, [unitFilterOn, home, filteredParent, unitId])
  // Keep Done folds even if the filter hides an entire section. Outside the filter,
  // a section that leaves the rail loses its expansion as before.
  const [doneOpen, setDoneOpen] = useState<string[]>([])
  useEffect(() => {
    if (unitFilterOn || !doneOpen.length) return
    const visible = doneOpen.filter(id => railOpen && sections.some(sec => (sec.project?.id ?? '') === id && sec.done.length > 0))
    if (visible.length !== doneOpen.length) setDoneOpen(visible)
  }, [doneOpen, railOpen, sections, unitFilterOn])
  const [renaming, setRenaming] = useState<Space | null>(null)
  useEffect(() => {
    if (!error) return
    const k = setTimeout(() => setError(''), 6000)
    return () => clearTimeout(k)
  }, [error])
  if (!railOpen)
    return (
      <aside className="rail sticky top-0 flex h-screen w-11 flex-col items-center gap-2 bg-rail pt-4 pb-3 text-rail-ink max-[860px]:static max-[860px]:h-auto max-[860px]:w-auto max-[860px]:flex-row max-[860px]:px-3 max-[860px]:py-2">
        {brand}
        <span className="text-lg font-[650] tracking-[-0.01em] min-[861px]:hidden">workriver</span>
        <Fold open={false} onToggle={onToggle} />
        <Blocked ws={ws} />
        <div className="mt-auto flex flex-col gap-1 max-[860px]:hidden"><TemplatesButton compact /><Settings compact ws={ws} /></div>
      </aside>
    )
  const group = ({ intent, units }: { intent: Intent; units: Unit[] }) => (
    <IntentGroup
      key={intent.id}
      ws={ws}
      intent={intent}
      units={units}
      {...(unitFilterOn ? filteredGroup(intent.id) : {
        isOpen: !!needle || open.includes(intent.id) || !units.length,
        onOpen: o => setOpen(list => o ? [...list, intent.id] : list.filter(x => x !== intent.id)),
      })}
      unitId={unitId}
      onUnit={onUnit}
      onIntent={onIntent}
    />
  )
  // One project: its heading (its title with all shown, else "Workspaces") with "+" for a new
  // workspace, its open workspaces, its roadmap, what is done. "No project": no workspaces.
  const section = (sec: RailProject) => {
    const p = sec.project
    const projectId = p?.id ?? ''
    const title = p ? (project ? t('rail.workspaces') : p.title) : t('rail.noProject')
    return (
      <section key={projectId} aria-label={p ? p.title : t('rail.noProject')} className="flex flex-col gap-1" data-project={projectId}>
        <h2 className="flex items-center gap-1.5 px-1.5 pt-2 pb-0.5 text-[11px] font-semibold tracking-wide text-rail-ink-2 uppercase">
          <span className="min-w-0 flex-1 truncate" lang={p && !project ? contentLang(p.title) : undefined}>{title}</span>
          {p && <AddSpace project={p.id} onError={setError} />}
        </h2>
        {p && !needle && !spaces.some(w => !w.closed && w.project === p.id) && <p className="px-2.5 text-xs text-rail-ink-2 italic">{t('rail.noSpaces')}</p>}
        {sec.spaces.map(({ space, intents: list }) => (
          <SpaceGroup
            key={space.id} ws={ws} space={space} busy={activeIn(ws?.intents ?? [], space.id).length > 0}
            {...(unitFilterOn ? filteredGroup(space.id) : {
              isOpen: !!needle || !folded.includes(space.id),
              onOpen: o => setFolded(f => o ? f.filter(x => x !== space.id) : [...f, space.id]),
            })}
            onRename={() => setRenaming(space)} onError={setError}
          >
            {list.map(group)}
          </SpaceGroup>
        ))}
        {sec.loose.length > 0 && <section aria-label={t('rail.loose')} className="flex flex-col gap-0.5"><Heading>{t('rail.loose')}</Heading>{sec.loose.map(group)}</section>}
        {ws && (sec.planned.length > 0 || (p && !needle)) && <Roadmap ws={ws} list={sec.planned} all={intents} addTo={p?.id ?? null} onStart={onStart} onIntent={onIntent} here={unitId} onError={setError} />}
        {sec.done.length > 0 && (
          <Done count={sec.done.length} {...(unitFilterOn ? filteredGroup(`done:${projectId}`) : {
            isOpen: doneOpen.includes(projectId),
            onOpen: o => setDoneOpen(list => o ? [...list, projectId] : list.filter(x => x !== projectId)),
          })}>
            {sec.done.map(group)}
          </Done>
        )}
      </section>
    )
  }
  return (
    <aside className="rail sticky top-0 flex h-screen w-[var(--rail-w,248px)] flex-col gap-3 bg-rail px-3 pt-4 pb-3 text-rail-ink max-[860px]:static max-[860px]:h-auto max-[860px]:w-auto">
      {/* Not on a narrow screen, where the rail is a row of its own. */}
      <div {...size.handle} aria-label={t('rail.resize')} className="edge max-[860px]:hidden" data-edge="right" />
      <div className="flex items-center gap-2 pl-1 text-lg font-[650] tracking-[-0.01em]">
        {brand}
        workriver
        <span className="min-w-0 flex-1 truncate font-normal text-rail-ink-2">{repo}</span>
        <RailFilter filter={filter} onChange={filterBy} tags={tagsOf(listed)} unitStates={unitStates} onUnitStates={filterUnits} />
        <Fold open onToggle={onToggle} />
      </div>
      {(intentFiltersOn || unitFilterOn) && (
        <p className="-mt-2 flex items-center gap-2 pl-1 text-xs text-rail-ink-2">
          <span className="flex-1">
            {intentFiltersOn && <span className="block">{t('rail.filtered', { shown: listed.filter(i => keeps(filter, i)).length, all: listed.length })}</span>}
            {selectedStates && <span className="block">{t('rail.showingUnits', { states: selectedStates })}</span>}
          </span>
          <button type="button" className="rounded-sm px-1 font-semibold hover:bg-rail-2 hover:text-rail-ink" onClick={() => {
            if (unitFilterOn) filterUnits([])
            if (intentFiltersOn) filterBy(NO_FILTER)
          }}>{t('rail.filterClear')}</button>
        </p>
      )}

      {projects.length > 0 && (
        <Choice
          tone="rail"
          label={t('rail.project')}
          value={project ?? ALL}
          onChange={v => onProject(v === ALL ? null : v)}
          options={[{ value: ALL, label: t('rail.allProjects') }, ...projects.map(p => ({ value: p.id, label: p.title, lang: contentLang(p.title) }))]}
          className="h-8"
        />
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
        {sections.map(section)}
        {ws && !needle && <AddProjectButton tone="rail" className="mt-1 self-start" />}
        {error && <p role="alert" className="px-2.5 text-xs text-rail-you">{error}</p>}
        {ws && (noUnitMatch || (nothing && (needle || !spaces.some(w => !w.closed)))) && (
          <div className="flex flex-col items-start gap-2 px-2.5 py-2 text-sm text-rail-ink-2">
            {noUnitMatch ? t('rail.filterNoUnits') : needle && unitsOf(ws).length ? <span><Slot text={t('rail.noMatch', { q: SLOT })}><Content text={q.trim()} /></Slot></span> : t('rail.none')}
            {nothing && needle && unitsOf(ws).length > 0 && <Button variant="outline" size="sm" className="rounded-sm bg-transparent text-rail-ink" onClick={() => onQ('')}>{t('rail.clearSearch')}</Button>}
          </div>
        )}
      </div>
      </ScrollArea>

      <div className="flex flex-col border-t border-rail-line pt-2">
        <TemplatesButton />
        <Settings ws={ws} />
      </div>
      <RenameSpace space={renaming} onClose={() => setRenaming(null)} />
    </aside>
  )
}

const unitProgress = (unit: Unit) => unit.progress ?? (unit.status === 'merged' ? 'merged' : null)

/**
 * Where a unit stands, as a small dot and a word: working, waiting, ready to merge (in the
 * done tone, so what can be merged stands out), merged, or done (with why, on hover).
 */
export function Progress({ unit }: { unit: Unit }) {
  const { t } = useT()
  const p = unitProgress(unit)
  if (!p) return null
  // Research and spikes are never merged: ready, they are closed.
  const tag = <span className="tag font-normal tracking-normal" data-progress={p}>{t(p === 'ready' && unit.kind ? 'unit.progress.readyClose' : `unit.progress.${p}`)}</span>
  return unit.doneReason ? <Tip text={unit.doneReason}>{tag}</Tip> : tag
}

/**
 * A ready unit whose every change is already in its base: merged by squash, most likely,
 * which git cannot show. Said next to "Ready to merge", with the command that records it.
 */
export function Squashed({ unit }: { unit: Unit }) {
  const { t } = useT()
  const s = unit.squash!
  const command = t('unit.mergedCommand', { unit: unit.id })
  return (
    <span className="inline-flex items-center gap-1 text-xs font-normal tracking-normal text-ink-2">
      <Tip text={t('unit.squashedWhy', { into: s.into, commit: s.commit.slice(0, 9) })}><span className="tag font-normal tracking-normal">{t('unit.squashed')}</span></Tip>
      <code className="font-mono">{command}</code>
      <CopyButton text={command} />
    </span>
  )
}
