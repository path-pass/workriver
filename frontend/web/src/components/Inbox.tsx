// The inbox: everything that asks something of the user, across all units, as one list in
// the side panel. One person works one queue; a list keeps order and age where chips would
// only pile up.
import { useMemo } from 'react'
import { useAct } from '@/act'
import { Button } from '@/components/ui/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { baseName, finderLabel, inbox, openFindings, severityCounts, type InboxRow, type Pick } from '@/data'
import { useT } from '@/i18n'
import type { Workspace } from '@/types'
import { focusAgent } from '@/runtime'
import { Icon, Mark, Steps, TerminalButton, Tip, Who } from './bits'
import { alertLimit, alertValue, topLine } from './Machines'

const GROUPS = ['blocking', 'machines', 'decisions', 'gaps', 'later'] as const

function Row({ r, onOpen, onMachines }: { r: InboxRow; onOpen: (unit: string, pick: Pick) => void; onMachines: () => void }) {
  const { t, lang, num, waited, when, date } = useT()
  const act = useAct()
  // A machine past an alert threshold: the row opens the Machines section.
  // Its level is its colour and its first word; the processes using the most memory follow.
  if (r.alert) {
    const a = r.alert
    const metric = t(`metrics.${a.metric}`)
    const top = topLine(t, lang, a)
    return (
      <button type="button" className="irow" data-kind="machine" data-level={a.level} onClick={onMachines}>
        <Icon name="machine" />
        <span className="title">{t('inbox.machineAlert', { machine: a.here ? t('metrics.here') : a.machine, metric, value: alertValue(t, lang, a) })}</span>
        <span className="meta">
          <span className="level">{t(`metrics.level.${a.level}`)}</span>
          <span className="truncate">{alertLimit(t, a)}</span>
        </span>
        <Tip text={when(r.since)}><span className="age">{waited(r.since)}</span></Tip>
        {top && <span className="meta top" title={top}><span className="truncate">{top}</span></span>}
      </button>
    )
  }
  // An agent waiting in its terminal: the row brings that terminal to the front.
  if (r.agent) {
    const a = r.agent
    return (
      <button type="button" className="irow" data-kind="agent" aria-label={t('inbox.blockedAgent', { agent: a.name })} onClick={() => focusAgent(a.session ?? a.name)}>
        <Icon name="terminal" />
        <span className="title flex min-w-0 items-center gap-1.5"><Who name={a.name} topic={a.topic} /><span className="truncate">{a.name}</span></span>
        <span className="meta">
          {a.topic && <span className="truncate">{a.topic}</span>}
          <span className="unit opacity-70">{a.place}</span>
        </span>
        <Tip text={when(r.since)}><span className="age">{waited(r.since)}</span></Tip>
      </button>
    )
  }
  // A worker that stopped without delivering: the row opens the task, the button its terminal.
  if (r.kind === 'stalled') {
    const n = r.node!
    const s = n.stalled!
    const agent = n.agent ?? ''
    const ref = `${r.unit!.id}#${n.taskId ?? n.id}`
    return (
      <div className="flex items-center gap-1">
        <button type="button" className="irow min-w-0" data-kind="stalled" onClick={() => onOpen(r.unit!.id, r.pick)}>
          <Icon name="terminal" />
          <span className="title">{s.gone ? t('inbox.stalledGone', { task: ref, agent }) : t('inbox.stalled', { task: ref, agent, ago: waited(r.since) })}</span>
          <span className="meta">
            {s.reminded && <span className="flex-none">{t('inbox.reminded', { time: date(s.reminded) })}</span>}
            <span className="unit"><span className="opacity-70">{r.intent?.title}{' / '}</span>{r.unit?.title}</span>
          </span>
          <Tip text={when(r.since)}><span className="age">{waited(r.since)}</span></Tip>
        </button>
        {!s.gone && agent && <TerminalButton agent={agent} session={n.agentSession} />}
      </div>
    )
  }
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
      <span className="title">{title}</span>
      <span className="meta">
        {n?.blind && <Mark name="blind" label={t('task.blindHint')} />}
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
        <span className="unit"><span className="opacity-70">{r.intent?.title ?? r.project}{r.unit && ' / '}</span>{r.unit?.title}</span>
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
export function InboxList({ ws, onOpen, onMachines }: { ws: Workspace; onOpen: (unit: string, pick: Pick) => void; onMachines: () => void }) {
  const { t, num } = useT()
  const box = useMemo(() => inbox(ws), [ws])
  if (!GROUPS.some(g => box[g].length)) return <p className="px-2.5 text-sm text-ink-2">{t('inbox.empty')}</p>
  return GROUPS.map(g => box[g].length > 0 && (
    <section key={g} className="igroup" aria-labelledby={`inbox-${g}`}>
      <h3 id={`inbox-${g}`} className="mb-1 flex items-baseline gap-1.5 px-2.5 text-xs font-semibold text-ink-2">
        {t(`inbox.${g}`)}<span className="font-normal">{num(box[g].length)}</span>
      </h3>
      <ul>{box[g].map(r => <li key={r.key}><Row r={r} onOpen={onOpen} onMachines={onMachines} /></li>)}</ul>
    </section>
  ))
}
