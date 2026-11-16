// The rail picks units and nothing else: product name, search, the units grouped under
// their intents with their signals, settings. What asks something of the user and where the agents are live in
// the side panel on the right.
import { Waves } from 'lucide-react'
import { useEffect, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
import { attention, repoOf, unitsOf } from '@/data'
import { contentLang, useT } from '@/i18n'
import type { Intent, Unit, Workspace } from '@/types'
import { Content, Icon, Slot, SLOT, Tip } from './bits'
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
  if (unit.status === 'merged') return last ? <span className="text-xs text-rail-ink-2">{date(last.date, false)}</span> : null
  if (!n.blocking && !n.waiting && !n.grew) return <span className="tag">{t(unit.commits.length ? 'unit.active' : 'unit.untouched')}</span>
  return (
    <>
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
  const n = needs(unitsOf(ws)).blocking
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

export function Rail({ ws, unitId, onUnit, q, onQ, open: railOpen, onToggle }: Props) {
  const { t } = useT()
  const intents = ws?.intents ?? []
  const needle = q.trim().toLowerCase()
  const match = (u: Unit) => !needle || (u.title + ' ' + u.purpose).toLowerCase().includes(needle)
  const groups = intents.map(intent => ({ intent, units: intent.units.filter(match) })).filter(g => g.units.length)
  // The intent of the open unit is open; the others are folded unless the user opened them.
  const home = intents.find(i => i.units.some(u => u.id === unitId))?.id
  // The repository of the open unit; units of one intent may work in several.
  const openUnit = unitsOf(ws).find(u => u.id === unitId)
  const repo = ws && openUnit ? repoOf(ws, openUnit).name : ''
  const [open, setOpen] = useState<string[]>([])
  useEffect(() => { if (home) setOpen(o => (o.includes(home) ? o : [...o, home])) }, [home])
  if (!railOpen)
    return (
      <aside className="rail sticky top-0 flex h-screen w-11 flex-col items-center gap-2 bg-rail pt-4 pb-3 text-rail-ink max-[860px]:static max-[860px]:h-auto max-[860px]:w-auto max-[860px]:flex-row max-[860px]:px-3 max-[860px]:py-2">
        <Waves aria-hidden size={20} strokeWidth={1.6} className="flex-none" />
        <span className="text-lg font-[650] tracking-[-0.01em] min-[861px]:hidden">workriver</span>
        <Fold open={false} onToggle={onToggle} />
        <Blocked ws={ws} />
        <div className="mt-auto max-[860px]:hidden"><Settings compact /></div>
      </aside>
    )
  return (
    <aside className="rail sticky top-0 flex h-screen w-[248px] flex-col gap-3 bg-rail px-3 pt-4 pb-3 text-rail-ink max-[860px]:static max-[860px]:h-auto max-[860px]:w-auto">
      <div className="flex items-center gap-2 pl-1 text-lg font-[650] tracking-[-0.01em]">
        <Waves aria-hidden size={20} strokeWidth={1.6} className="flex-none" />
        workriver
        <span className="min-w-0 flex-1 truncate font-normal text-rail-ink-2">{repo}</span>
        <Fold open onToggle={onToggle} />
      </div>

      <Input
        type="search"
        value={q}
        onChange={e => onQ(e.target.value)}
        placeholder={t('rail.search')}
        aria-label={t('rail.search')}
        className="rounded-md bg-rail-2 text-sm md:text-sm"
      />

      <ScrollArea className="-mx-1 min-h-0 flex-1 max-[860px]:[&>[data-slot=scroll-area-viewport]]:max-h-[40vh]">
      <div className="flex flex-col gap-1 px-1 py-0.5">
        {groups.map(({ intent, units }) => {
          // A search opens every group with a match.
          const isOpen = !!needle || open.includes(intent.id)
          const sum = needs(intent.units)
          return (
            <Collapsible
              key={intent.id}
              open={isOpen}
              onOpenChange={o => setOpen(list => (o ? [...list, intent.id] : list.filter(x => x !== intent.id)))}
            >
              <CollapsibleTrigger className="group/intent flex w-full items-center gap-1.5 rounded-md px-1.5 py-1.5 text-left text-xs font-semibold text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink">
                <Icon name="chev" className="size-3.5 transition-transform group-data-[state=open]/intent:rotate-90" />
                <span className="min-w-0 flex-1 truncate" lang={contentLang(intent.title)}>{intent.title}</span>
                <ManagerState intent={intent} />
                {/* Folded, the group still says what waits inside it. */}
                {!isOpen && <span className="flex flex-none items-center gap-1"><Marks blocking={sum.blocking} waiting={sum.waiting} /></span>}
              </CollapsibleTrigger>
              <CollapsibleContent className="flex flex-col gap-0.5 pl-3">
                {/* The intent's goal, quietly, for whoever opened it. */}
                {intent.goal && <p className="line-clamp-2 px-2.5 pb-1 text-xs text-rail-ink-2" title={intent.goal} lang={contentLang(intent.goal)}>{intent.goal}</p>}
                {units.map(unit => (
                  <button
                    key={unit.id}
                    type="button"
                    aria-pressed={unit.id === unitId}
                    onClick={() => onUnit(unit.id)}
                    className="block w-full rounded-md px-2.5 py-2 text-left transition-colors hover:bg-rail-2 aria-pressed:bg-rail-3"
                  >
                    <span className="block text-sm leading-[1.4] font-[550] wrap-anywhere" lang={contentLang(unit.title)}>{unit.title}</span>
                    <span className="mt-1 flex flex-wrap items-center gap-1 empty:hidden"><Signals unit={unit} /></span>
                  </button>
                ))}
              </CollapsibleContent>
            </Collapsible>
          )
        })}
        {ws && !groups.length && (
          <div className="flex flex-col items-start gap-2 px-2.5 py-2 text-sm text-rail-ink-2">
            {unitsOf(ws).length ? <span><Slot text={t('rail.noMatch', { q: SLOT })}><Content text={q.trim()} /></Slot></span> : t('rail.none')}
            {unitsOf(ws).length > 0 && <Button variant="outline" size="sm" className="rounded-sm bg-transparent text-rail-ink" onClick={() => onQ('')}>{t('rail.clearSearch')}</Button>}
          </div>
        )}
      </div>
      </ScrollArea>

      <div className="flex flex-col border-t border-rail-line pt-2">
        <Settings />
      </div>
    </aside>
  )
}
