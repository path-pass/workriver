// An intent's own page (`/i3`): what it is for and what more was said about it, where it runs
// and who manages it, its units with where each stands, and the research and design recorded
// on the intent itself before those became units of their own.
import { useState, type ReactNode } from 'react'
import { Button } from '@/ui/primitives/button'
import { lastMove, quietSince } from './select'
import type { Pick } from '@/work/types'
import { contentLang, useT } from '@/i18n'
import { linkTo, pathOf } from '@/app/link'
import { record } from '@/api/actions'
import type { Intent, Unit } from './types'
import type { Workspace } from '@/workspace/types'
import type { Talk } from '@/conversations/types'
import { Progress, ManagerState, Marks, needs, Quiet } from './Rail'
import { TerminalButton } from '@/agents/Mark'
import { MachineChip } from '@/machines/format'
import { Prose } from '@/content/Prose'
import { Content, ExternalChip, Slot, SLOT } from '@/content/Chips'
import { CopyButton } from '@/ui/CopyButton'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { Choice } from '@/ui/Choice'
import { Activity } from '@/work/Activity'
import { EditIntentDialog } from './EditIntent'
import { HistoryDialog, HistoryLine } from './IntentHistory'
import { draftOf, type Draft } from './edit'
import { SortChip, TagChips } from './Sort'
import { tagsOf } from './classify'

/** A label and its value, one row of the "where it runs" list. */
function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
      <dt className="w-28 flex-none text-ink-2">{label}</dt>
      <dd className="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-1">{children}</dd>
    </div>
  )
}

/** One unit of the intent: id, title, where it stands, what waits for the user, when it last moved. */
function UnitRow({ ws, unit, onUnit }: { ws: Workspace; unit: Unit; onUnit: (id: string) => void }) {
  const { t, waited, when } = useT()
  const n = needs([unit])
  const last = lastMove(unit)
  return (
    <li>
      <button type="button" onClick={() => onUnit(unit.id)} className="flex w-full flex-wrap items-center gap-x-3 gap-y-1 rounded-md px-2.5 py-2 text-left hover:bg-sunken">
        <span className="min-w-0 flex-1 text-sm font-[550] wrap-anywhere">
          <span className="mr-1.5 font-mono text-xs font-normal text-ink-2">{unit.id}</span><span lang={contentLang(unit.title)}>{unit.title}</span>
        </span>
        <span className="flex flex-none flex-wrap items-center gap-1.5">
          <MachineChip machine={unit.machine} away={ws.machines?.some(m => m.label === unit.machine && !m.reachable)} />
          <Progress unit={unit} />
          <Marks {...n} />
          <Quiet since={quietSince(unit)} />
          {Number.isFinite(last)
            ? <Tip text={when(last)}><span className="text-xs text-ink-2 tabular-nums">{t('intent.moved', { time: waited(last) })}</span></Tip>
            : <span className="text-xs text-ink-2">{t('intent.neverMoved')}</span>}
        </span>
      </button>
    </li>
  )
}

export function IntentPage({ ws, intent, earlier, pick, onPick, talk, fitKey, onUnit, onStart, onTalk, aside }: {
  ws: Workspace
  intent: Intent
  /** Its own tasks from before research units, as a unit; null when it has none. */
  earlier: Unit | null
  pick: Pick
  onPick: (p: Pick) => void
  talk: Talk | null
  fitKey: unknown
  onUnit: (id: string) => void
  onStart: (i: Intent) => void
  /** Opens the conversation with its manager in the side panel. */
  onTalk: () => void
  /** The side panel's buttons, on a narrow screen. */
  aside?: ReactNode
}) {
  const { t } = useT()
  const state = intent.state ?? 'active'
  const units = intent.units.filter(u => !u.intentLevel)
  const session = intent.manager ? ws.sessions.find(s => s.manager && !s.ended && s.agent === intent.manager) : undefined
  const worksOn = ws.workspaces?.find(w => w.id === intent.workspace?.id)?.projectTitle
  // The note as written (its indentation is Markdown); only a blank one is none.
  const note = intent.note?.trim() ? intent.note : ''
  // The edit dialog (with what it starts from: its text, or a version to restore) and the history.
  const [editing, setEditing] = useState<{ draft: Draft; restoring?: number } | null>(null)
  const [history, setHistory] = useState(false)
  return (
    <>
      <header className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <h1 className="flex flex-wrap items-baseline gap-x-3 gap-y-1 text-xl font-[650] tracking-[-0.012em] wrap-anywhere">
            <span><span className="mr-2 font-mono text-base font-normal tracking-normal text-ink-2">{intent.id}</span><span lang={contentLang(intent.title)}>{intent.title}</span></span>
            <CopyButton text={linkTo(pathOf(intent.id, null))} aria={t('link.copy')} className="self-center text-ink-2" />
            <Tip text={t('intent.edit.open')}>
              <Button variant="ghost" size="icon-xs" className="flex-none self-center text-ink-2" aria-label={t('intent.edit.open')} onClick={() => setEditing({ draft: draftOf(intent) })}><Icon name="pen" className="size-3.5" /></Button>
            </Tip>
            <span className="tag self-center font-normal tracking-normal">{t(`intent.state.${state}`)}</span>
            <SortChip intent={intent} className="self-center" />
            {intent.complete && <Tip text={t('unit.complete')}><span className="tag self-center font-normal tracking-normal" data-progress="ready">{t('unit.finished')}</span></Tip>}
            <span className="self-center"><ExternalChip ext={intent.external} /></span>
          </h1>
          {!!intent.tags?.length && <p className="mt-1 flex flex-wrap items-center gap-1" aria-label={t('intent.tags')}><TagChips tags={intent.tags} /></p>}
          {intent.goal && <p className="mt-0.5 max-w-[75ch] text-sm text-ink-2" lang={contentLang(intent.goal)}>{intent.goal}</p>}
          <HistoryLine intent={intent} onOpen={() => setHistory(true)} />
          {state === 'dropped' && intent.reason && <p className="mt-0.5 text-sm text-ink-2"><Slot text={t('intent.dropped', { reason: SLOT })}><Content text={intent.reason} /></Slot></p>}
        </div>
        {state === 'planned' && (
          <Tip text={<span><Slot text={t('plan.title', { title: SLOT })}><Content text={intent.title} /></Slot></span>}>
            <Button size="sm" className="flex-none gap-1.5" onClick={() => onStart(intent)}><Icon name="play" className="size-3.5" />{t('rail.start')}</Button>
          </Tip>
        )}
        {aside && <div className="flex flex-none gap-1 min-[861px]:hidden">{aside}</div>}
      </header>

      <section className="block-box flex flex-col gap-4">
        {note && (
          <div>
            <h2 className="mb-1 text-sm font-[650]">{t('intent.note')}</h2>
            <div className="max-w-[75ch] text-sm"><Prose text={note} /></div>
          </div>
        )}
        <div>
          <h2 className="mb-1.5 text-sm font-[650]">{t('intent.where')}</h2>
          {state === 'planned' ? (
            <p className="text-sm text-ink-2">{t('intent.notStarted')}</p>
          ) : (
            <dl className="flex flex-col gap-1.5 text-sm">
              {intent.workspace && (
                <Row label={t('intent.workspace')}>
                  <span className="font-mono text-xs">{intent.workspace.label}</span>
                  {/* The project its manager works on, when workriver knows it. */}
                  {worksOn && <span className="text-xs text-ink-2"><Slot text={t('intent.worksOn', { project: SLOT })}><Content text={worksOn} /></Slot></span>}
                </Row>
              )}
              <Row label={t('intent.manager')}>
                {intent.manager && <span className="font-[550]">{intent.manager}</span>}
                <span className="text-ink-2"><ManagerState intent={intent} /></span>
                {session && <TerminalButton agent={session.agent} session={session.session} machine={session.machine} className="text-ink-2" />}
                {state === 'active' && intent.workspace && intent.manager && (
                  <Button variant="outline" size="sm" className="h-7 gap-1.5" onClick={onTalk}><Icon name="manager" className="size-3.5" />{t('intent.talk')}</Button>
                )}
              </Row>
              {/* Who decides its findings: the user's to say, and only the user's (this page is the user's). */}
              {state === 'active' && intent.findings && (
                <Row label={t('rail.findings')}>
                  <Choice
                    label={t('rail.findings')}
                    value={intent.findings}
                    onChange={v => { record('intent.updated', { intent: intent.id, findings: v }).catch(err => console.warn(err)) }}
                    options={[{ value: 'manager', label: t('rail.findingsBy.manager') }, { value: 'user', label: t('rail.findingsBy.user') }]}
                    className="h-7 w-32 text-xs"
                  />
                </Row>
              )}
            </dl>
          )}
        </div>
      </section>

      <section className="block-box" aria-labelledby="intent-units">
        <h2 id="intent-units" className="mb-2 text-lg font-[650]">{t('intent.units')}</h2>
        {units.length
          ? <ul className="-mx-2.5 flex flex-col">{units.map(u => <UnitRow key={u.id} ws={ws} unit={u} onUnit={onUnit} />)}</ul>
          : <p className="text-sm text-ink-2">{t('rail.noUnits')}</p>}
      </section>

      {editing && <EditIntentDialog key={`${intent.id}:${editing.restoring ?? ''}`} intent={intent} draft={editing.draft} restoring={editing.restoring} offered={tagsOf(ws.intents.filter(i => i.project === intent.project))} onClose={() => setEditing(null)} />}
      {history && <HistoryDialog intent={intent} onClose={() => setHistory(false)} onRestore={(text, n) => { setHistory(false); setEditing({ draft: draftOf(text), restoring: n }) }} />}

      {earlier && talk && <Activity unit={earlier} pick={pick} onPick={onPick} talk={talk} workspace={intent.workspace?.id} fitKey={fitKey} title={t('intent.earlier')} hint={t('intent.earlierHint')} />}
    </>
  )
}
