// How the inbox names waiting agents and stalled workers, and which waiting agents are news.
import type { T } from '@/i18n'
import type { InboxRow, WaitingAgentRow } from './select'

/** The inbox's label for a waiting agent or stalled worker. */
export function blockingLine(t: T, waited: (since: number) => string, r: InboxRow): string {
  if (r.agent) return t('inbox.blockedAgent', { agent: r.agent.name })
  const n = r.node!
  const task = `${r.unit!.id}#${n.taskId ?? n.id}`
  const agent = n.agent ?? ''
  return n.stalled?.gone ? t('inbox.stalledGone', { task, agent }) : t('inbox.stalled', { task, agent, ago: waited(r.since) })
}

/**
 * Finds newly waiting agents by row key. The first list is what was there when the page
 * loaded. An agent that leaves the list and comes back is news again.
 */
export function waitingAgentNews() {
  let seen: Set<string> | null = null
  return (rows: WaitingAgentRow[]): WaitingAgentRow[] => {
    const before = seen
    seen = new Set(rows.map(r => r.key))
    return before ? rows.filter(r => !before.has(r.key)) : []
  }
}
