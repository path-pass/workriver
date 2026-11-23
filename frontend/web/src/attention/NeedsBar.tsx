// Keep agents waiting in their terminals in sight while the user talks with a manager.
import { useContext } from 'react'
import { useT } from '@/i18n'
import { AgentContext } from '@/agents/context'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import type { WaitingAgentRow } from './select'

export function NeedsBar({ rows }: { rows: WaitingAgentRow[] }) {
  const { t, num, waited, when } = useT()
  const agents = useContext(AgentContext)
  if (!rows.length) return null
  return (
    <section aria-label={t('inbox.title')} className="nbar mx-4 mb-2 flex-none">
      <h3 className="flex items-baseline gap-1.5 px-2 pt-1.5 pb-1 text-xs font-semibold">
        {t('inbox.title')}<span className="font-normal">{num(rows.length)}</span>
      </h3>
      <ul className="flex max-h-[7.5rem] flex-col gap-0.5 overflow-y-auto px-1 pb-1">
        {rows.map(({ key, since, agent: { name, session, machine } }) => (
          <li key={key}>
            <button type="button" className="nrow" onClick={() => agents.answer({ name, session, machine })}>
              <Icon name="terminal" />
              <span className="title">{t('inbox.blockedAgent', { agent: name })}</span>
              <Tip text={when(since)}><span className="age">{waited(since)}</span></Tip>
            </button>
          </li>
        ))}
      </ul>
    </section>
  )
}
