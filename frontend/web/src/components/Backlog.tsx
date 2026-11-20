// The backlog panel: deferred findings of the project shown in the rail (or of every
// project), from reviews and tests and from the backlog itself, grouped by intent and unit
// or by where they came from. Picked ones are done together – one message to the manager
// of the intent they go to – or dropped with a reason. Planned intents are in the rail's
// roadmap, not here.
import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { ms } from '@/planning/select'
import type { Pick } from '@/work/types'
import { contentLang, useT } from '@/i18n'
import { record } from '@/runtime'
import type { Finding } from '@/work/types'
import type { Intent } from '@/planning/types'
import type { Workspace } from '@/workspace/types'
import { ExternalChip, Icon } from './bits'
import { Choice } from './Choice'

interface Item {
  key: string
  /** As the service names it: a task's finding, or a backlog one by its id alone. */
  ref: { unit?: string; task?: number; finding: string }
  f: Finding
  intent?: Intent
  group: string
  /** u1#3 P2-1, or f4. */
  label: string
  /** Where a click goes: the unit and the task that found it. */
  at?: { unit: string; task: string }
  /** Added to the backlog without a task: impact and place may be missing. */
  loose: boolean
  /** The group heading's language when it is made of content (titles) only. */
  groupLang?: string
}

const waiting = (f: Finding) => f.state === 'deferred' || f.state === 'scheduled'
const SEV = { P1: 0, P2: 1, P3: 2 }

function itemsOf(ws: Workspace, project: string | null, t: ReturnType<typeof useT>['t']): Item[] {
  const items: Item[] = []
  const mine = (p?: string) => !project || p === project
  for (const intent of ws.intents.filter(i => mine(i.project)))
    for (const unit of intent.units)
      for (const task of unit.tasks)
        for (const f of (task.output.findings ?? []).filter(waiting))
          items.push({
            key: `${unit.id}#${task.id}:${f.id}`,
            ref: { unit: unit.id, task: Number(task.id), finding: f.id },
            f,
            intent,
            group: t('backlog.unit', { intent: intent.title, unit: unit.intentLevel ? t('rail.intentWork') : unit.title }),
            groupLang: unit.intentLevel ? undefined : contentLang(intent.title + unit.title),
            label: `${unit.id}#${task.id} ${f.id}`,
            at: { unit: unit.id, task: task.id },
            loose: false,
          })
  for (const b of (ws.backlog ?? []).filter(b => mine(b.project) && waiting(b.finding))) {
    const intent = ws.intents.find(i => i.id === b.intent)
    items.push({
      key: b.finding.id,
      ref: { finding: b.finding.id },
      f: b.finding,
      intent,
      group: intent ? intent.title : b.source ? t('backlog.fromSource', { source: b.source }) : t('backlog.added'),
      groupLang: intent ? contentLang(intent.title) : undefined,
      label: b.finding.id,
      loose: true,
    })
  }
  return items.sort((a, b) => SEV[a.f.severity] - SEV[b.f.severity] || ms(a.f.since) - ms(b.f.since))
}

export function BacklogList({ ws, project, onOpen }: { ws: Workspace; project: string | null; onOpen: (unit: string, pick: Pick) => void }) {
  const { t, waited, when } = useT()
  const items = itemsOf(ws, project, t)
  const [picked, setPicked] = useState<string[]>([])
  const [target, setTarget] = useState('')
  const [dropping, setDropping] = useState(false)
  const [reason, setReason] = useState('')
  const [note, setNote] = useState('')
  const chosen = items.filter(i => picked.includes(i.key) && i.f.state === 'deferred')
  // Where they go: the one intent they belong to, else one the user picks among the active ones.
  const theirs = [...new Set(chosen.flatMap(i => (i.intent ? [i.intent.id] : [])))]
  const actives = ws.intents.filter(i => (i.state ?? 'active') === 'active' && (!project || i.project === project))
  const intent = theirs.length === 1 ? ws.intents.find(i => i.id === theirs[0]) : actives.find(i => i.id === target) ?? (actives.length === 1 ? actives[0] : undefined)
  const blocked = theirs.length > 1 ? t('backlog.oneIntent')
    : intent && (intent.state ?? 'active') !== 'active' ? t('backlog.startFirst', { intent: intent.id })
    : !intent && !actives.length ? t('backlog.noIntent') : ''
  const done = (msg: string) => { setPicked([]); setDropping(false); setReason(''); setNote(msg) }
  const fail = (e: unknown) => setNote(t('rail.failed', { error: String(e) }))
  const together = () => {
    if (!intent) return
    record('findings.batched', { intent: intent.id, findings: chosen.map(i => i.ref) })
      .then(() => done(t('backlog.sent', { to: intent.manager ?? intent.id })), fail)
  }
  const drop = () => {
    Promise.all(chosen.map(i => record('finding.dropped', { unit: i.ref.unit ?? '', task: i.ref.task ?? 0, finding: i.ref.finding, reason: reason.trim() })))
      .then(() => done(''), fail)
  }
  const groups = [...new Set(items.map(i => i.group))]
  return (
    <div className="flex flex-col gap-3">
      <p className="px-2.5 text-xs text-ink-2">{project ? t('backlog.lead') : t('backlog.every')}</p>
      {!items.length && <p className="px-2.5 text-sm text-ink-2">{t('backlog.none')}</p>}
      {groups.map(g => (
        <section key={g} aria-label={g}>
          <h3 className="mb-1 px-2.5 text-xs font-semibold text-ink-2" lang={items.find(i => i.group === g)?.groupLang}>{g}</h3>
          {items.filter(i => i.group === g).map(i => (
            <label key={i.key} className="brow">
              <input
                type="checkbox"
                disabled={i.f.state !== 'deferred'}
                checked={picked.includes(i.key)}
                onChange={e => setPicked(p => (e.target.checked ? [...p, i.key] : p.filter(k => k !== i.key)))}
              />
              <span className="min-w-0 flex-1">
                <span className="flex items-baseline gap-1.5">
                  <span className="flex-none font-mono text-xs font-semibold">{i.f.severity}</span>
                  {i.at
                    ? <button type="button" className="min-w-0 text-left hover:underline" onClick={e => { e.preventDefault(); onOpen(i.at!.unit, { kind: 'task', id: i.at!.task, finding: i.f.id }) }} lang={contentLang(i.f.problem)}>{i.f.problem}</button>
                    : <span className="min-w-0" lang={contentLang(i.f.problem)}>{i.f.problem}</span>}
                </span>
                <span className="flex flex-wrap items-center gap-x-1.5 text-xs text-ink-2">
                  <span className="font-mono">{i.label}</span>
                  {i.f.state === 'scheduled' && <span className="flex items-center gap-0.5"><Icon name="later" className="size-3" />{t('backlog.scheduled')}</span>}
                  {i.loose && (!i.f.impact || !i.f.evidence.length) && <span className="italic">{t('backlog.missing')}</span>}
                  <ExternalChip ext={i.f.external} />
                  {i.f.since && <span className="age ml-auto" title={when(ms(i.f.since))}>{waited(ms(i.f.since))}</span>}
                </span>
                {i.f.note && <span className="mt-0.5 block text-xs whitespace-pre-line text-ink-2" lang={contentLang(i.f.note)}>{i.f.note}</span>}
              </span>
            </label>
          ))}
        </section>
      ))}
      {note && <p role="status" className="px-2.5 text-xs text-ink-2">{note}</p>}
      {chosen.length > 0 && (
        <div className="sticky bottom-0 flex flex-col gap-2 border-t border-line bg-surface px-2.5 pt-2 pb-3">
          <span className="text-xs text-ink-2">{t('backlog.selected', { count: chosen.length })}</span>
          {!theirs.length && actives.length > 1 && (
            <label className="flex items-center gap-2 text-xs">
              {t('backlog.for')}
              <Choice
                label={t('backlog.for')}
                value={intent?.id ?? ''}
                placeholder="…"
                onChange={setTarget}
                options={actives.map(a => ({ value: a.id, label: `${a.id} ${a.title}`, lang: contentLang(a.title) }))}
                className="h-7 flex-1 text-xs"
              />
            </label>
          )}
          {blocked && <p className="text-xs text-you-ink">{blocked}</p>}
          {dropping ? (
            <form className="flex flex-col gap-1.5" onSubmit={e => { e.preventDefault(); if (reason.trim()) drop() }}>
              <input className="rounded-sm border border-line bg-surface px-2 py-1 text-sm" autoFocus value={reason} onChange={e => setReason(e.target.value)} placeholder={t('backlog.dropReason')} aria-label={t('backlog.dropReason')} />
              <div className="flex gap-1.5">
                <Button type="submit" size="sm" variant="outline" disabled={!reason.trim()}>{t('backlog.dropConfirm', { count: chosen.length })}</Button>
                <Button type="button" size="sm" variant="ghost" onClick={() => setDropping(false)}>{t('backlog.cancel')}</Button>
              </div>
            </form>
          ) : (
            <div className="flex gap-1.5">
              <Button size="sm" disabled={!intent || !!blocked} onClick={together}>{t('backlog.together')}</Button>
              <Button size="sm" variant="ghost" onClick={() => setDropping(true)}>{t('backlog.drop')}</Button>
            </div>
          )}
        </div>
      )}
    </div>
  )
}
