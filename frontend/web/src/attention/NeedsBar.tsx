// The bar at the top of the Manager conversation: what blocks on the user (Needs you's
// Blocking) stays in sight while they talk with a manager, without opening Needs you. It reads
// the rows Needs you reads; gone when nothing waits.
import { useContext } from 'react'
import { contentLang, useT } from '@/i18n'
import { AgentContext } from '@/agents/context'
import type { Pick } from '@/work/types'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { blockingLine } from './notice'
import type { InboxRow } from './select'

/** Blocking's count and its rows, one line each; a row goes where its Needs you row goes. */
export function NeedsBar({ rows, onOpen }: { rows: InboxRow[]; onOpen: (unit: string, pick: Pick) => void }) {
  const { t, num, waited, when } = useT()
  const agents = useContext(AgentContext)
  if (!rows.length) return null
  const go = (r: InboxRow) => (r.agent ? agents.answer({ name: r.agent.name, session: r.agent.session, machine: r.agent.machine }) : onOpen(r.unit!.id, r.pick))
  const icon = (r: InboxRow) => (r.kind === 'gate' ? (r.node?.gate ? `gate-${r.node.gate}` : 'ring') : 'terminal')
  return (
    <section aria-label={t('inbox.title')} className="nbar mx-4 mb-2 flex-none">
      <h3 className="flex items-baseline gap-1.5 px-2 pt-1.5 pb-1 text-xs font-semibold">
        {t('inbox.title')}<span className="font-normal">{num(rows.length)}</span>
      </h3>
      <ul className="flex max-h-[7.5rem] flex-col gap-0.5 overflow-y-auto px-1 pb-1">
        {rows.map(r => (
          <li key={r.key}>
            <button type="button" className="nrow" data-kind={r.kind} onClick={() => go(r)}>
              <Icon name={icon(r)} />
              <span className="title" lang={r.kind === 'gate' ? contentLang(r.node?.title) : undefined}>{blockingLine(t, waited, r)}</span>
              <Tip text={when(r.since)}><span className="age">{waited(r.since)}</span></Tip>
            </button>
          </li>
        ))}
      </ul>
    </section>
  )
}
