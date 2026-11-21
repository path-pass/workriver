// A review's findings: each with its decision (the user's, or the one taken in this sitting
// on its way), and the line that follows a decision or a gate's result to the agents.
import { useState, useEffect, useRef } from 'react'
import { Button } from '@/ui/primitives/button'
import { Input } from '@/ui/primitives/input'
import { findingKey, useAct } from './actions'
import type { Decision, Finding, Task, Pick } from './types'
import { contentLang, useT } from '@/i18n'
import { MANAGER, type DeliveryStep } from '@/api/actions'
import type { Unit } from '@/planning/types'
import type { Sent } from '@/conversations/types'
import { Who } from '@/agents/Mark'
import { CopyButton } from '@/ui/CopyButton'
import { Icon, Mark } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/ui/primitives/collapsible'
import { ScrollArea } from '@/ui/primitives/scroll-area'
import { findingsOf, SEVERITIES } from './select'
import { ms } from '@/planning/select'
import { cn } from '@/lib/utils'
import { Thumbs } from '@/attachments/Thumbs'
import { Content, ExternalChip } from '@/content/Chips'
import { NodeName } from './Nodes'

/**
 * Where a message is: not gone yet (and can be taken back), delivered, picked up. Once the
 * service has it (`sent`), its record is shown, and its text is what gets copied.
 */
export function DeliveryLine({ id, sent, message }: { id: string; sent?: Sent; message?: boolean }) {
  const { t } = useT()
  const act = useAct()
  const local = act.delivery[id]
  const text = sent?.message ?? act.message[id]
  // The service's receipt, once there is one: over this page's attempt, even one it did not
  // take (another tab's result was recorded first).
  if (sent && (!local || local.step === 'recorded' || local.step === 'failed')) {
    const to = sent.to ?? ''
    const steps: [Sent['step'], string][] = sent.step === 'not-delivered'
      ? [['not-delivered', t('deliver.notDelivered', { reason: sent.reasonCode ? t(`deliver.reason.${sent.reasonCode}`) : t('deliver.reason.unknown') })]]
      : sent.step === 'held'
        ? [['held', t('deliver.held', { agent: to })]]
        : [
          ['delivered', sent.toManager ? t('deliver.toManager') : t('deliver.delivered', { agent: to })],
          ['picked', t('deliver.picked', { agent: to })],
        ]
    const at = steps.findIndex(([s]) => s === sent.step)
    return (
      <div className="mt-2 flex flex-wrap items-center gap-x-1.5 gap-y-1 text-xs" aria-live="polite">
        {steps.map(([s, label], k) => {
          const line = (
            <span key={s} className={k <= at ? (k === at ? 'font-semibold text-ink' : 'text-ink-2') : 'text-ink-2 opacity-50'}>
              {k > 0 && <span aria-hidden className="mr-1.5">→</span>}
              {s === 'picked' && k <= at && <Who name={to} />} {label}
            </span>
          )
          // The service's own words, for whoever needs the detail.
          return (s === 'not-delivered' || s === 'held') && sent.reason ? <Tip key={s} text={sent.reason}>{line}</Tip> : line
        })}
        {message && text && <CopyButton text={text} label={t('deliver.copy')} />}
      </div>
    )
  }
  const d = local
  if (!d) return null
  const manager = d.to === MANAGER
  const steps: [DeliveryStep, string][] = d.step === 'recorded' || d.step === 'failed'
    ? [['pending', t('deliver.pending')], [d.step, d.step === 'recorded' ? t('deliver.recorded') : t('deliver.failed')]]
    : [
        ['pending', t('deliver.pending')],
        ['delivered', manager ? t('deliver.toManager') : t('deliver.delivered', { agent: d.to })],
        ...(manager ? [] : [['picked', t('deliver.picked', { agent: d.to })] as [DeliveryStep, string]]),
      ]
  const at = steps.findIndex(([s]) => s === d.step)
  return (
    <div className="mt-2 flex flex-wrap items-center gap-x-1.5 gap-y-1 text-xs" aria-live="polite">
      {steps.map(([s, label], k) => (
        <span key={s} className={k <= at ? (k === at ? 'font-semibold text-ink' : 'text-ink-2') : 'text-ink-2 opacity-50'}>
          {k > 0 && <span aria-hidden className="mr-1.5">→</span>}
          {s === 'picked' && k <= at && <Who name={d.to} />} {label}
        </span>
      ))}
      {d.step === 'pending' && <Button variant="ghost" size="xs" className="rounded-sm" onClick={() => act.undo(id)}>{t('deliver.undo')}</Button>}
      {message && text && <CopyButton text={text} label={t('deliver.copy')} />}
    </div>
  )
}

/** The four decisions on an open finding; the agent's suggestion, if any, stands out. */
export function Decide({ unit, task, f }: { unit: Unit; task: Task; f: Finding }) {
  const { t } = useT()
  const act = useAct()
  const [note, setNote] = useState('')
  const decide = (d: Decision) => act.decide(unit, task, f, d, note.trim())
  return (
    <div className="mt-2 flex flex-col gap-1.5">
      <Input value={note} onChange={e => setNote(e.target.value)} placeholder={t('decide.note')} aria-label={t('decide.note')} className="h-7 text-xs md:text-xs" />
      <div role="group" aria-label={t('decide.label', { id: f.id })} className="flex flex-wrap gap-1.5">
        {(['accept', 'revise', 'reject', 'defer'] as const).map(d => {
          const suggested = f.recommendation?.decision === d
          // Revise is fixing it the user's way: the note says how, so it needs one.
          const needsNote = d === 'revise' && !note.trim()
          return (
            <Button key={d} type="button" size="xs" variant={suggested ? 'default' : 'outline'} className="rounded-sm" disabled={needsNote} aria-describedby={needsNote ? `revise-${f.id}` : undefined} onClick={() => decide(d)}>
              {suggested && <Icon name="agent" className="size-3" />}
              {t(`decide.${d}`)}
              {suggested && <span className="sr-only">{t('decide.suggested')}</span>}
            </Button>
          )
        })}
      </div>
      {!note.trim() && <p id={`revise-${f.id}`} className="text-xs text-ink-2">{t('decide.reviseNeedsNote')}</p>}
    </div>
  )
}

/** The decision taken in this sitting, on its way, with its note. */
export function Decided({ unit, task, f }: { unit: Unit; task: Task; f: Finding }) {
  const { t } = useT()
  return (
    <>
      {f.note && <div className="whitespace-pre-line"><span className="text-ink-2">{t('io.field', { name: t('decide.noteShort') })}</span><span lang={contentLang(f.note)}>{f.note}</span></div>}
      <DeliveryLine id={findingKey(unit, task, f)} sent={f.delivery} message />
    </>
  )
}

// ---- what went into the picked piece of work and what came out of it ----

/** What was added to a finding after it was reported: who, when, what; oldest first. */
export function FindingNotes({ f, id = false }: { f: Finding; id?: boolean }) {
  const { t, when } = useT()
  if (!f.notes?.length) return null
  return (
    <ul className="mt-1.5 flex flex-col gap-1 text-xs">
      {f.notes.map((n, k) => (
        <li key={k} className="flex items-start gap-1.5">
          <Who name={n.by} />
          <span className="min-w-0">
            <span className="text-ink-2">{id && <span className="font-mono font-semibold">{f.id} </span>}{t('finding.noted', { by: n.by })} · {when(ms(n.at))}</span>
            <span className="block whitespace-pre-line wrap-anywhere" lang={contentLang(n.text)}>{n.text}</span>
            <Thumbs list={n.attachments ?? []} />
          </span>
        </li>
      ))}
    </ul>
  )
}

/** One finding of the picked review: a line to scan, its details behind the chevron. */
function FindingRow({ unit, n, f, picked, onSelect, onPick }: { unit: Unit; n: Task; f: Finding; picked: boolean; onSelect: () => void; onPick: (id: string) => void }) {
  const { t } = useT()
  const act = useAct()
  const row = useRef<HTMLDivElement>(null)
  // Arriving from the work a finding caused, the finding may sit below the fold of the list.
  useEffect(() => { if (picked) row.current?.scrollIntoView({ block: 'nearest' }) }, [picked])
  const quiet = f.state !== 'open' && f.state !== 'accepted' && f.state !== 'revised'
  const suggests = f.state === 'open' && f.recommendation ? t('finding.suggests', { decision: t(`finding.decision.${f.recommendation.decision}`) }) : ''
  return (
    <Collapsible ref={row} className="frow group" data-picked={picked || undefined}>
      <div className="flex items-start gap-1">
        <CollapsibleTrigger className="fchev" aria-label={t('finding.details', { id: f.id })}><Icon name="chev" /></CollapsibleTrigger>
        <button type="button" className="flex min-w-0 flex-1 items-baseline gap-2 rounded-sm py-1 text-left" aria-pressed={picked} onClick={onSelect}>
          <span className="flex-none font-mono text-xs font-semibold">{f.id}</span>
          <span className={cn('truncate group-data-[state=open]:whitespace-pre-line', quiet && 'text-ink-2', f.state === 'rejected' && 'line-through')} lang={contentLang(f.problem)}>{f.problem}</span>
        </button>
        {/* What was added to it since: a small count; the additions are in its details. */}
        {(f.notes?.length ?? 0) > 0 && (
          <Tip text={t('finding.notes', { count: f.notes!.length })}>
            <span className="flex h-7 flex-none items-center gap-0.5 text-xs text-ink-2"><Icon name="plus" className="size-3" /><span aria-hidden>{f.notes!.length}</span><span className="sr-only">{t('finding.notes', { count: f.notes!.length })}</span></span>
          </Tip>
        )}
        {f.decidedBy === 'agent' && (
          <span className="flex h-7 items-center text-ink-2"><Mark name="agent" label={t('finding.byAgent')} /></span>
        )}
        {/* Revised: like accepted, the user's way; a pencil says so. */}
        {f.state === 'revised' && <span className="flex h-7 items-center text-ink-2"><Mark name="pen" label={t('finding.revised')} /></span>}
        {(f.state === 'accepted' || f.state === 'revised') && !f.leadsTo.length ? (
          // Decided here and not turned into tasks yet: the manager bot has it.
          <span className="flex flex-none items-center py-1 text-xs text-ink-2">{t(f.state === 'revised' ? 'finding.revised' : 'finding.acceptedHere')}</span>
        ) : f.state === 'accepted' || f.state === 'revised' ? (
          <span role="group" aria-label={t('finding.leadsTo')} className="flex flex-none items-center gap-1 py-0.5">
            {f.leadsTo.map(id => {
              const to = unit.tasks.find(n => n.id === id)
              return to && (
                <Tip key={id} text={<span><NodeName n={to} /></span>}>
                  <button type="button" className="chip" data-state={to.state} onClick={() => onPick(id)}>{to.taskId ? `#${to.taskId}` : <Content text={to.title} />}</button>
                </Tip>
              )
            })}
          </span>
        ) : (
          <span className={cn('flex flex-none items-center gap-1 py-1 text-xs', f.state === 'open' && f.decider !== 'manager' ? 'text-you-ink' : 'text-ink-2')}>
            <ExternalChip ext={f.external} />
            {(f.state === 'deferred' || f.state === 'scheduled') && <Icon name="later" className="size-3.5" />}
            {suggests && <span className="text-ink-2 max-[700px]:hidden">{suggests} ·</span>}
            {/* Handed to the intent's manager by the user: open, but not waiting for the user. */}
            {f.state === 'open' && f.decider === 'manager' ? <><Icon name="manager" className="size-3.5" />{t('finding.managerDecides')}</>
              : f.state === 'open' && f.noManager ? t('finding.noManager')
              : t(`finding.${f.state}`)}
          </span>
        )}
      </div>
      <CollapsibleContent className="pb-1.5 pl-6 text-xs whitespace-pre-line">
        {suggests && <div><span className="text-ink-2">{t('io.field', { name: suggests })}</span><span lang={contentLang(f.recommendation?.reason)}>{f.recommendation?.reason}</span></div>}
        {f.reason && <div><span className="text-ink-2">{t('io.field', { name: t('finding.dropped') })}</span><span lang={contentLang(f.reason)}>{f.reason}</span></div>}
        {f.impact && <div><span className="text-ink-2">{t('io.field', { name: t('io.impact') })}</span><span lang={contentLang(f.impact)}>{f.impact}</span></div>}
        {f.evidence.length > 0 && <div><span className="text-ink-2">{t('io.field', { name: t('io.evidence') })}</span><span className="font-mono">{f.evidence.join(t('list.sep'))}</span></div>}
        <Thumbs list={f.attachments ?? []} />
        <FindingNotes f={f} />
        {f.state === 'open' ? <Decide unit={unit} task={n} f={f} /> : act.delivery[findingKey(unit, n, f)] || f.delivery ? <Decided unit={unit} task={n} f={f} /> : null}
        {/* The manager's decision stands until the user makes another: theirs overrides it. */}
        {f.decidedBy === 'agent' && ['accepted', 'revised', 'rejected', 'deferred'].includes(f.state) && (
          <div className="mt-2">
            <span className="text-ink-2">{t('finding.override')}</span>
            <Decide unit={unit} task={n} f={f} />
          </div>
        )}
      </CollapsibleContent>
    </Collapsible>
  )
}

/** What a review or a gate found: grouped by severity, scrolling inside the panel. */
export function Findings({ unit, n, finding, onPick }: { unit: Unit; n: Task; finding?: string; onPick: (p: Pick) => void }) {
  const { num } = useT()
  const found = findingsOf(n)
  return (
    // Radix lays the content out as a table, which would let a long line widen it instead
    // of being cut: make it a block.
    <ScrollArea className="-my-1 -mr-2 [&>[data-slot=scroll-area-viewport]]:max-h-[50vh] [&>[data-slot=scroll-area-viewport]>div]:block!">
      <div className="py-1 pr-3">
        {SEVERITIES.map(s => {
          const list = found.filter(f => f.severity === s)
          return list.length > 0 && (
            <section key={s} className="fgroup">
              <h4 className="flex items-baseline gap-1.5 text-xs"><b className="font-mono">{s}</b><span className="text-ink-2">{num(list.length)}</span></h4>
              {list.map(f => (
                <FindingRow
                  key={f.id}
                  unit={unit}
                  n={n}
                  f={f}
                  picked={f.id === finding}
                  onSelect={() => onPick({ kind: 'task', id: n.id, finding: f.id === finding ? undefined : f.id })}
                  onPick={id => onPick({ kind: 'task', id })}
                />
              ))}
            </section>
          )
        })}
      </div>
    </ScrollArea>
  )
}
