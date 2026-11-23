// The bar at the top of the Manager conversation: Blocking and open manager questions stay
// in sight while the user talks with a manager.
import { useContext } from 'react'
import { contentLang, useT } from '@/i18n'
import { AgentContext } from '@/agents/context'
import type { Pick } from '@/work/types'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { blockingLine } from './notice'
import type { InboxRow, ManagerQuestionRow } from './select'

export function NeedsBar({ rows, onOpen, onTalk }: {
  rows: (InboxRow | ManagerQuestionRow)[]
  onOpen: (unit: string, pick: Pick) => void
  onTalk: (workspace: string) => void
}) {
  const { t, num, waited, when } = useT()
  const agents = useContext(AgentContext)
  if (!rows.length) return null
  const go = (r: InboxRow | ManagerQuestionRow, button: HTMLButtonElement) => {
    if (r.kind === 'question') {
      const manager = button.closest('[aria-labelledby="side-talk"]')
      onTalk(r.workspace)
      // The conversation may have switched workspaces. Jump after its log is laid out.
      requestAnimationFrame(() => {
        const question = manager?.querySelector<HTMLElement>(`[data-question="${CSS.escape(r.key)}"]`)
        question?.scrollIntoView({ block: 'start' })
        question?.focus({ preventScroll: true })
      })
    } else if (r.agent) agents.answer({ name: r.agent.name, session: r.agent.session, machine: r.agent.machine })
    else onOpen(r.unit!.id, r.pick)
  }
  const icon = (r: InboxRow | ManagerQuestionRow) => (r.kind === 'question' ? 'talk' : r.kind === 'gate' ? (r.node?.gate ? `gate-${r.node.gate}` : 'ring') : 'terminal')
  return (
    <section aria-label={t('inbox.title')} className="nbar mx-4 mb-2 flex-none">
      <h3 className="flex items-baseline gap-1.5 px-2 pt-1.5 pb-1 text-xs font-semibold">
        {t('inbox.title')}<span className="font-normal">{num(rows.length)}</span>
      </h3>
      <ul className="flex max-h-[7.5rem] flex-col gap-0.5 overflow-y-auto px-1 pb-1">
        {rows.map(r => (
          <li key={r.key}>
            <button type="button" className="nrow" data-kind={r.kind} onClick={e => go(r, e.currentTarget)}>
              <Icon name={icon(r)} />
              <span className="title" lang={contentLang(r.kind === 'question' ? r.text : r.kind === 'gate' ? r.node?.title : undefined)}>{r.kind === 'question' ? r.text : blockingLine(t, waited, r)}</span>
              <Tip text={when(r.since)}><span className="age">{waited(r.since)}</span></Tip>
            </button>
          </li>
        ))}
      </ul>
    </section>
  )
}
