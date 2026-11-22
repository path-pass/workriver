// Seating a workspace's manager: the user's act, and the only one that makes an agent a
// manager (besides the end of a start the user made, below). A list of the agents running
// now (who each is, where it runs, what the records know of it), and a second part that
// starts a new one to be seated; each change names the seat the user was shown, so one
// changed meanwhile is never overwritten unseen.
import { useEffect, useId, useState } from 'react'
import { Button } from '@/ui/primitives/button'
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogTitle } from '@/ui/primitives/dialog'
import { RadioGroup, RadioGroupItem } from '@/ui/primitives/radio-group'
import { Switch } from '@/ui/primitives/switch'
import { contentLang, useT } from '@/i18n'
import { SeatChanged, setSpace } from '@/api/actions'
import type { Space } from '@/agents/types'
import type { Workspace } from '@/workspace/types'
import { Who } from '@/agents/Mark'
import { MachineChip } from '@/machines/format'
import { Content, Slot, slot, SLOT } from '@/content/Chips'
import { Icon } from '@/ui/Icon'
import { candidates, movedSeat, replaces, runs, seatList, shortFolder, type Candidate } from './spaces'
import { StartLine, StartPart } from './StartManager'

/** Each seat dialog on the page, to be opened on a workspace. */
const openers = new Set<(space: string, preset?: string, start?: boolean) => void>()
/** Opens the manager select of workspace `space`, preset to session `preset` when given;
 *  `start`: at its "Start a new one" part. */
export function openSeat(space: string, preset?: string, start = false) { openers.forEach(open => open(space, preset, start)) }

/** What the records know of an agent, in a line: the seat it holds, the tasks it has. */
function Known({ c }: { c: Candidate }) {
  const { t } = useT()
  const parts = [
    c.seat && t('seat.managerOf', { workspace: c.seat.id }),
    c.tasks.length > 0 && t('seat.tasks', { count: c.tasks.length, first: `${c.tasks[0].unit}#${c.tasks[0].id}` }),
  ].filter(Boolean)
  return <span className="text-ink-2">{parts.length ? parts.join(' · ') : t('seat.nothing')}</span>
}

/** One agent of the manager select, as a radio row: who it is, where it runs (its folder
 *  short, whole in the tooltip), what the records know of it. */
function Row({ c, ws, id }: { c: Candidate; ws: Workspace; id: string }) {
  const { t } = useT()
  const folder = c.s.folder && shortFolder(c.s.folder, c.s.machine, ws.projects ?? [])
  return (
    <label htmlFor={`${id}-${c.key}`} className="flex cursor-pointer items-start gap-2 rounded-sm px-1.5 py-1.5 hover:bg-sunken has-data-[state=checked]:bg-sunken">
      <RadioGroupItem id={`${id}-${c.key}`} value={c.key} className="mt-1" />
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="flex min-w-0 items-center gap-1.5">
          <Who name={c.s.agent} topic={c.s.topic} />
          <span className="min-w-0 truncate font-[550]">{c.s.agent}</span>
          {c.s.harness && <span className="flex-none font-mono text-xs text-ink-2">{c.s.harness}</span>}
          <MachineChip machine={c.s.machine} away={c.s.unreachable} />
          {c.s.state && <span className="ml-auto flex-none text-xs text-ink-2">{t(`manager.state.${c.s.state}`)}</span>}
        </span>
        {folder && <span className="truncate font-mono text-xs text-ink-2" title={c.s.folder} lang={contentLang(folder)}>{folder}</span>}
        <span className="flex min-w-0 flex-wrap gap-x-2 text-xs">
          <Known c={c} />
          {/* Where its terminal is in the runtime: a hint only, quietly. */}
          {c.s.group && <span className="text-ink-2 opacity-70"><Slot text={t('seat.group', { group: SLOT })}><span lang={contentLang(c.s.group.label)}>{c.s.group.label}</span></Slot></span>}
        </span>
      </span>
    </label>
  )
}

/**
 * The manager select of one workspace: the agents running now, one picked, and "Seat". Those
 * in the project's folder come first (with the one seated now and the one preset); a switch
 * shows the others, open when none is in the project. It
 * asks once before seating over a manager that runs; "Empty the seat" takes the manager
 * away. `onDone`: seated, emptied or a start recorded. A seat that changed meanwhile says so
 * and shows the seat as it is now, to choose again. Under it, "Start a new one" (`start`:
 * opened there).
 */
export function SeatPicker({ ws, space, preset, start, onDone }: { ws: Workspace; space: Space; preset?: string; start?: boolean; onDone?: () => void }) {
  const { t } = useT()
  const id = useId()
  const list = candidates(ws)
  const seated = space.manager
  const [picked, setPicked] = useState(preset ?? space.proposal ?? '')
  // Kept in the first list wherever they run: the one seated now and the one preset.
  const [keep] = useState(() => [seated?.session, preset ?? space.proposal].filter((k): k is string => !!k))
  const { here, rest } = seatList(list, space.project, keep)
  const [all, setAll] = useState(false)
  const showAll = all || here.length === 0
  const showAllAt = (on: boolean) => {
    setAll(on)
    // One picked among the others is not seated unseen once they are hidden.
    if (!on && rest.some(c => c.key === picked)) setPicked('')
  }
  const [asked, setAsked] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  // The seat shown changed (someone seated meanwhile, or this did): nothing is confirmed yet.
  const shown = `${seated?.session ?? ''}:${seated?.since ?? 0}`
  useEffect(() => setAsked(false), [shown, picked])
  const at = [...here, ...(showAll ? rest : [])].find(c => c.key === picked)
  const moved = movedSeat(space, at)
  // Over a manager that runs: asked once, in a line, and the button says what it does.
  const over = runs(seated) && !!at && at.key !== seated.session
  const act = (manager: string | null) => {
    setBusy(true)
    setSpace({ action: 'manager', workspace: space.id, manager, replaces: replaces(space) }).then(
      () => { setBusy(false); setError(''); onDone?.() },
      e => { setBusy(false); setError(e instanceof SeatChanged ? t('seat.changed') : t('seat.failed', { error: e instanceof Error ? e.message : String(e) })) },
    )
  }
  const seat = () => {
    if (!at) return
    if (over && !asked) return setAsked(true)
    act(at.key)
  }
  return (
    <div className="flex flex-col gap-2.5 text-sm">
      <p className="text-xs wrap-anywhere text-ink-2">
        {seated
          ? <Slot text={t(runs(seated) ? 'seat.now' : 'seat.nowGone', { name: SLOT, folder: seated.folder })}><Content text={seated.name} /></Slot>
          : t('seat.empty')}
      </p>
      <StartLine ws={ws} space={space} />
      {moved && at && (
        <p role="note" className="rounded-sm border border-line bg-sunken px-2 py-1.5 text-xs wrap-anywhere">
          {t('seat.moved', { name: at.s.agent })}
          <span className="mt-1 block font-mono text-ink-2">{t('seat.movedOld', { folder: seated!.folder, machine: seated!.machine ?? t('metrics.here') })}</span>
          <span className="block font-mono">{t('seat.movedNew', { folder: at.s.folder ?? '', machine: at.s.machine ?? t('metrics.here') })}</span>
        </p>
      )}
      {list.length === 0
        ? <p className="text-ink-2">{t('seat.nobody')}</p>
        : (
          <>
            {here.length === 0 && <p className="text-xs text-ink-2">{t('seat.noneHere')}</p>}
            <RadioGroup value={picked} onValueChange={setPicked} aria-label={t('seat.pick')} className="max-h-[40vh] gap-0.5 overflow-y-auto">
              {here.map(c => <Row key={c.key} c={c} ws={ws} id={id} />)}
              {/* The others are listed under the switch, so they show where it was turned on. */}
              {rest.length > 0 && (
                <label className="flex cursor-pointer items-center gap-1.5 self-start px-1.5 py-1 text-xs text-ink-2">
                  <Switch size="sm" checked={showAll} disabled={here.length === 0} onCheckedChange={showAllAt} />
                  {t('seat.showAll', { count: list.length })}
                </label>
              )}
              {showAll && rest.map(c => <Row key={c.key} c={c} ws={ws} id={id} />)}
            </RadioGroup>
          </>
        )}
      {over && asked && at && <p role="alert" className="text-xs"><Slot text={t('seat.over', { old: slot(0), next: slot(1) })}>{[<Content key="o" text={seated.name} />, <Content key="n" text={at.s.agent} />]}</Slot></p>}
      {error && <p role="alert" className="text-xs wrap-anywhere text-del">{error}</p>}
      <div className="flex flex-wrap justify-end gap-2">
        {seated && <Button type="button" variant="ghost" size="sm" disabled={busy} onClick={() => act(null)}>{t('seat.clear')}</Button>}
        <Button type="button" size="sm" disabled={busy || !at || (at.key === seated?.session && runs(seated) && !moved)} onClick={seat}>
          <Icon name="manager" className="size-3.5" />{over && asked ? t('seat.goOver', { name: at!.s.agent }) : moved ? t('seat.goNew') : t('seat.go')}
        </Button>
      </div>
      <StartPart space={space} focus={start} onDone={onDone} />
    </div>
  )
}

/** The one seat dialog of the page: opened on a workspace from its line in the rail, a
 *  Needs you row, Settings or the Start dialog (`openSeat`). */
export function SeatDialog({ ws }: { ws: Workspace | null }) {
  const { t } = useT()
  const [open, setOpen] = useState<{ space: string; preset?: string; start?: boolean } | null>(null)
  useEffect(() => {
    const opener = (space: string, preset?: string, start?: boolean) => setOpen({ space, preset, start })
    openers.add(opener)
    return () => { openers.delete(opener) }
  }, [])
  const space = ws?.workspaces?.find(w => w.id === open?.space)
  const project = ws?.projects?.find(p => p.id === space?.project)?.title
  return (
    <Dialog open={!!open && !!space} onOpenChange={o => { if (!o) setOpen(null) }}>
      {/* With the start part under the list it can be taller than a short window: it scrolls. */}
      <DialogContent closeLabel={t('settings.close')} className="max-h-[calc(100dvh-2rem)] overflow-y-auto">
        {ws && space && (
          <>
            <DialogTitle>
              <Slot text={t('seat.title', { workspace: SLOT })}><Content text={space.name} /></Slot>
              <span className="ml-2 font-mono text-xs font-normal text-ink-2">{space.id}</span>
            </DialogTitle>
            <DialogDescription>{project ? <Slot text={t('seat.lead', { project: SLOT })}><Content text={project} /></Slot> : t('seat.leadNone')}</DialogDescription>
            <SeatPicker key={`${space.id}:${open?.preset ?? ''}:${!!open?.start}`} ws={ws} space={space} preset={open?.preset} start={open?.start} onDone={() => setOpen(null)} />
            <DialogClose asChild><Button type="button" variant="ghost" size="sm" className="self-start">{t('rail.addCancel')}</Button></DialogClose>
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}
