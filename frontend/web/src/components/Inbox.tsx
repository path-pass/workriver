// The inbox: everything that asks something of the user, across all units, as one list in
// the side panel. One person works one queue; a list keeps order and age where chips would
// only pile up.
import { useMemo } from 'react'
import { useAct } from '@/act'
import { Button } from '@/components/ui/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { baseName, finderLabel, inbox, openFindings, severityCounts, type InboxRow, type Pick } from '@/data'
import { contentLang, useT } from '@/i18n'
import type { Workspace } from '@/types'
import { Icon, Steps, Tip, Who } from './bits'

const GROUPS = ['blocking', 'decisions', 'gaps', 'later'] as const

function Row({ r, onOpen }: { r: InboxRow; onOpen: (unit: string, pick: Pick) => void }) {
  const { t, num, waited, when } = useT()
  const act = useAct()
  const n = r.node
  const follows = n && r.unit?.tasks.find(x => x.id === n.inputs.find(i => i.primary)?.from)?.taskId
  const open = n && r.kind === 'decision' ? openFindings(n) : []
  const suggested = open.filter(f => f.recommendation).length
  const icon = !n ? 'chev'
    : r.kind === 'gate' || r.kind === 'decision' ? (n.gate ? `gate-${n.gate}` : n.reviewer ?? 'ring')
    : r.kind === 'loose' ? 'no' : 'far'
  const title = r.chore ? r.chore.title
    : r.kind === 'decision' ? `${finderLabel(t, n!)}${follows ? ` #${follows}` : ''}`
    : r.kind === 'far' ? `${t('need.far')}${n!.taskId ? ` #${n!.taskId}` : ''}`
    : n!.title
  const body = (
    <>
      <Icon name={icon} />
      <span className="title" lang={r.kind === 'decision' || r.kind === 'far' ? undefined : contentLang(title)}>{title}</span>
      <span className="meta">
        {open.length > 0 && <span className="flex-none font-mono">{severityCounts(open)}</span>}
        {suggested > 0 && (
          <Tip text={t('inbox.suggested', { count: suggested })}>
            <span className="inline-flex flex-none items-center gap-0.5">
              <Icon name="agent" className="size-3.5" /><span aria-hidden>{num(suggested)}</span>
              <span className="sr-only">{t('inbox.suggested', { count: suggested })}</span>
            </span>
          </Tip>
        )}
        {r.kind === 'loose' && <span className="flex-none">{t('need.loose')}</span>}
        {r.far.length > 0 && <span className="truncate font-mono">{r.far.map(baseName).join(' ')}</span>}
        {r.chore && <Who name={r.chore.by} />}
        {/* Where it is: the intent quietly, then the unit. */}
        <span className="unit"><span className="opacity-70"><span lang={contentLang(r.intent.title)}>{r.intent.title}</span>{r.unit && ' / '}</span><span lang={contentLang(r.unit?.title)}>{r.unit?.title}</span></span>
      </span>
      <Tip text={when(r.since)}><span className="age">{waited(r.since)}</span></Tip>
    </>
  )
  if (r.chore) return (
    <Collapsible>
      <CollapsibleTrigger className="irow" data-kind="chore">{body}</CollapsibleTrigger>
      <CollapsibleContent className="pb-2 pl-[34px] text-sm">
        <Steps steps={r.chore.steps} />
        {/* A chore goes to no one: done, it simply leaves the list. */}
        <Button variant="outline" size="xs" className="mt-2 rounded-sm" onClick={() => act.choreDone(r.chore!.id)}>
          <Icon name="check" className="size-3" />{t('chore.done')}
        </Button>
      </CollapsibleContent>
    </Collapsible>
  )
  return <button type="button" className="irow" data-kind={r.kind} onClick={() => onOpen(r.unit!.id, r.pick)}>{body}</button>
}

/** The inbox as a list in groups, an empty group not drawn; one short line when nothing waits. */
export function InboxList({ ws, onOpen }: { ws: Workspace; onOpen: (unit: string, pick: Pick) => void }) {
  const { t, num } = useT()
  const box = useMemo(() => inbox(ws), [ws])
  if (!GROUPS.some(g => box[g].length)) return <p className="px-2.5 text-sm text-ink-2">{t('inbox.empty')}</p>
  return GROUPS.map(g => box[g].length > 0 && (
    <section key={g} className="igroup" aria-labelledby={`inbox-${g}`}>
      <h3 id={`inbox-${g}`} className="mb-1 flex items-baseline gap-1.5 px-2.5 text-xs font-semibold text-ink-2">
        {t(`inbox.${g}`)}<span className="font-normal">{num(box[g].length)}</span>
      </h3>
      <ul>{box[g].map(r => <li key={r.key}><Row r={r} onOpen={onOpen} /></li>)}</ul>
    </section>
  ))
}
